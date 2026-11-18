use std::cell::RefCell;
use std::sync::Arc;

use crate::date::{Date, Duration};
use crate::expr::{Arith, Compare, Expr, FileField, Global, Local, Method, Node};
use crate::source::FileSource;
use crate::typed::{Link, Pattern, Typed, Url, is_url, split_link, tag_within};

const MAX_REPEAT_BYTES: usize = 1 << 20;
const MAX_EVAL_DEPTH: usize = 256;
const MAX_VALUE_DEPTH: usize = 64;

const TYPE_NAMES: [&str; 16] = [
    "null", "boolean", "number", "string", "date", "duration", "list", "object", "link", "url",
    "file", "tag", "regexp", "html", "icon", "image",
];

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EvalError {
    #[error("{operator} does not apply to {left} and {right}")]
    Operator {
        operator: &'static str,
        left: &'static str,
        right: &'static str,
    },
    #[error("- does not apply to {0}")]
    Negate(&'static str),
    #[error("{on} has no function {method}")]
    NoMethod {
        method: &'static str,
        on: &'static str,
    },
    #[error("{on} has no field {field}")]
    NoField { field: String, on: &'static str },
    #[error("{on} cannot be indexed by {key}")]
    NoIndex { on: &'static str, key: &'static str },
    #[error("{function}: argument {position} is {found}, expected {expected}")]
    Argument {
        function: &'static str,
        position: usize,
        expected: &'static str,
        found: &'static str,
    },
    #[error("not a {what}: {text}")]
    Parse { what: &'static str, text: String },
    #[error("date format token {0} depends on the locale and is not supported")]
    Format(String),
    #[error("the date is out of range")]
    DateRange,
    #[error("{function}: the result is too large")]
    TooLarge { function: &'static str },
    #[error("no formula named {0}")]
    UnknownFormula(String),
    #[error("unknown type {0}")]
    UnknownType(String),
    #[error("the expression or its value is nested too deeply")]
    TooDeep,
}

pub type Cell = Result<Typed, EvalError>;

pub(crate) struct Formulas {
    pub names: Vec<String>,
    pub exprs: Vec<Expr>,
}

pub(crate) struct RowFormulas<'a> {
    pub set: &'a Formulas,
    pub done: RefCell<Vec<Option<Cell>>>,
}

impl<'a> RowFormulas<'a> {
    pub(crate) fn new(set: &'a Formulas) -> RowFormulas<'a> {
        RowFormulas {
            set,
            done: RefCell::new(vec![None; set.names.len()]),
        }
    }
}

pub struct Scope<'a> {
    pub(crate) files: &'a dyn FileSource,
    pub(crate) file: Option<Arc<str>>,
    pub(crate) this: Option<Arc<str>>,
    pub(crate) now: Date,
    pub(crate) formulas: Option<&'a RowFormulas<'a>>,
    pub(crate) values: Option<&'a Typed>,
}

impl<'a> Scope<'a> {
    pub fn new(files: &'a dyn FileSource, now: Date) -> Scope<'a> {
        Scope {
            files,
            file: None,
            this: None,
            now,
            formulas: None,
            values: None,
        }
    }

    pub fn file(mut self, path: &str) -> Scope<'a> {
        self.file = Some(Arc::from(path));
        self
    }

    pub fn this(mut self, path: &str) -> Scope<'a> {
        self.this = Some(Arc::from(path));
        self
    }

    pub fn values(mut self, column: &'a Typed) -> Scope<'a> {
        self.values = Some(column);
        self
    }
}

struct Frame {
    value: Typed,
    index: usize,
    acc: Option<Typed>,
}

impl Expr {
    pub fn eval(&self, scope: &Scope) -> Cell {
        eval(&self.node, scope, &mut Vec::new())
    }
}

fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn folder_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

fn stem_and_extension(path: &str) -> (&str, &str) {
    let name = name_of(path);
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, extension),
        _ => (name, ""),
    }
}

fn link_to(path: &Arc<str>) -> Link {
    Link {
        target: path.strip_suffix(".md").unwrap_or(path).to_string(),
        anchor: None,
        display: None,
        file: Some(path.clone()),
    }
}

pub(crate) fn file_field(scope: &Scope, path: &Arc<str>, field: FileField) -> Typed {
    let files = scope.files;
    match field {
        FileField::Name => Typed::Text(name_of(path).to_string()),
        FileField::Basename => Typed::Text(stem_and_extension(path).0.to_string()),
        FileField::Ext => Typed::Text(stem_and_extension(path).1.to_string()),
        FileField::Folder => Typed::Text(folder_of(path).to_string()),
        FileField::Path => Typed::Text(path.to_string()),
        FileField::Mtime => files.modified(path).map_or(Typed::Null, Typed::Date),
        FileField::Ctime => files.created(path).map_or(Typed::Null, Typed::Date),
        FileField::Size => files
            .size(path)
            .map_or(Typed::Null, |bytes| Typed::Number(bytes as f64)),
        FileField::Tags => Typed::List(files.tags(path).into_iter().map(Typed::Tag).collect()),
        FileField::Links => Typed::List(files.links(path).into_iter().map(Typed::Link).collect()),
        FileField::Embeds => Typed::List(files.embeds(path).into_iter().map(Typed::Link).collect()),
        FileField::Backlinks => Typed::List(
            files
                .backlinks(path)
                .iter()
                .map(|source| Typed::Link(link_to(source)))
                .collect(),
        ),
        FileField::Properties => Typed::Object(files.properties(path)),
        FileField::File => Typed::File(path.clone()),
    }
}

fn folder_contains(folder: &str, wanted: &str) -> bool {
    let wanted = wanted.trim_matches('/');
    wanted.is_empty()
        || folder == wanted
        || folder
            .strip_prefix(wanted)
            .is_some_and(|rest| rest.starts_with('/'))
}

enum Target {
    File(Arc<str>),
    Missing(String),
}

fn link_from(scope: &Scope, text: &str, display: Option<Box<Typed>>) -> Link {
    let (target, anchor, alias) = split_link(text);
    Link {
        target: target.to_string(),
        anchor: anchor.map(str::to_string),
        display: display.or_else(|| alias.map(|a| Box::new(Typed::Text(a.to_string())))),
        file: scope.files.resolve(scope.file.as_deref(), target),
    }
}

fn target_of(scope: &Scope, value: &Typed) -> Option<Target> {
    match value {
        Typed::File(path) => Some(Target::File(path.clone())),
        Typed::Link(link) => Some(match &link.file {
            Some(path) => Target::File(path.clone()),
            None => Target::Missing(link.target.clone()),
        }),
        Typed::Text(text) => {
            let link = link_from(scope, text, None);
            Some(match link.file {
                Some(path) => Target::File(path),
                None => Target::Missing(link.target),
            })
        }
        _ => None,
    }
}

fn links_to(scope: &Scope, from: &str, target: &Target) -> bool {
    scope.files.links(from).iter().any(|link| match target {
        Target::File(path) => link.file.as_ref() == Some(path),
        Target::Missing(text) => {
            link.file.is_none() && link.target.to_lowercase() == text.to_lowercase()
        }
    })
}

fn random() -> f64 {
    use std::hash::{BuildHasher, Hasher};
    thread_local! {
        static STATE: std::cell::Cell<u64> = std::cell::Cell::new(
            std::collections::hash_map::RandomState::new().build_hasher().finish() | 1,
        );
    }
    STATE.with(|state| {
        let mut x = state.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        state.set(x);
        (x >> 11) as f64 / (1u64 << 53) as f64
    })
}

fn compare(op: Compare, a: &Typed, b: &Typed) -> bool {
    match op {
        Compare::Eq => a.equals(b),
        Compare::Ne => !a.equals(b),
        _ => a.compare(b).is_some_and(|order| match op {
            Compare::Gt => order.is_gt(),
            Compare::Lt => order.is_lt(),
            Compare::Ge => order.is_ge(),
            _ => order.is_le(),
        }),
    }
}

fn arith(op: Arith, left: Typed, right: Typed, now: Date) -> Cell {
    use Typed as T;
    if left == T::Null || right == T::Null {
        return Ok(T::Null);
    }
    let shift = |date: &Date, by: Duration, sign: f64| {
        date.shifted(by, sign)
            .map(T::Date)
            .ok_or(EvalError::DateRange)
    };
    let sign = if op == Arith::Sub { -1.0 } else { 1.0 };
    match (op, &left, &right) {
        (_, T::Number(a), T::Number(b)) => Ok(T::Number(match op {
            Arith::Add => a + b,
            Arith::Sub => a - b,
            Arith::Mul => a * b,
            Arith::Div => a / b,
            Arith::Rem => a % b,
        })),
        (Arith::Add | Arith::Sub, T::Date(date), T::Duration(by)) => shift(date, *by, sign),
        (Arith::Add, T::Duration(by), T::Date(date)) => shift(date, *by, 1.0),
        (Arith::Add | Arith::Sub, T::Date(date), T::Text(text)) => {
            let by = Duration::parse(text).ok_or_else(|| EvalError::Parse {
                what: "duration",
                text: text.clone(),
            })?;
            shift(date, by, sign)
        }
        (Arith::Sub, T::Date(a), T::Date(b)) => Ok(T::Duration(a.since(*b))),
        (Arith::Add | Arith::Sub, T::Duration(a), T::Duration(b)) => {
            Ok(T::Duration(a.plus(*b, sign)))
        }
        (Arith::Mul, T::Duration(a), T::Number(n)) => Ok(T::Duration(a.scaled(*n))),
        (Arith::Div, T::Duration(a), T::Number(n)) => Ok(T::Duration(a.scaled(1.0 / n))),
        (Arith::Add, T::List(a), T::List(b)) => Ok(T::List(a.iter().chain(b).cloned().collect())),
        (Arith::Add, T::Text(a), b) => Ok(T::Text(format!("{a}{}", b.text(now)))),
        (Arith::Add, a, T::Text(b)) => Ok(T::Text(format!("{}{b}", a.text(now)))),
        _ => Err(EvalError::Operator {
            operator: op.symbol(),
            left: left.type_name(),
            right: right.type_name(),
        }),
    }
}

fn member(scope: &Scope, value: Typed, name: &str) -> Cell {
    let missing = |on: &Typed| EvalError::NoField {
        field: name.to_string(),
        on: on.type_name(),
    };
    match &value {
        Typed::Null => Ok(Typed::Null),
        Typed::Object(entries) => Ok(entries
            .iter()
            .find(|(key, _)| key == name)
            .map_or(Typed::Null, |(_, found)| found.clone())),
        Typed::List(items) if name == "length" => Ok(Typed::Number(items.len() as f64)),
        Typed::Text(_) | Typed::Url(_) | Typed::Tag(_) if name == "length" => Ok(Typed::Number(
            value.as_str().map_or(0, |s| s.chars().count()) as f64,
        )),
        Typed::Date(date) => date
            .field(name)
            .map(Typed::Number)
            .ok_or_else(|| missing(&value)),
        Typed::File(path) => FileField::parse(name)
            .map(|field| file_field(scope, path, field))
            .ok_or_else(|| missing(&value)),
        _ => Err(missing(&value)),
    }
}

fn index(scope: &Scope, value: Typed, key: Typed) -> Cell {
    let whole = |n: f64| (n >= 0.0 && n.fract() == 0.0).then_some(n as usize);
    match (&value, &key) {
        (Typed::Null, _) | (_, Typed::Null) => Ok(Typed::Null),
        (Typed::List(items), Typed::Number(n)) => Ok(whole(*n)
            .and_then(|i| items.get(i))
            .cloned()
            .unwrap_or(Typed::Null)),
        (Typed::Text(text), Typed::Number(n)) => Ok(whole(*n)
            .and_then(|i| text.chars().nth(i))
            .map_or(Typed::Null, |c| Typed::Text(c.to_string()))),
        (Typed::Object(_) | Typed::File(_), Typed::Text(name)) => member(scope, value, name),
        _ => Err(EvalError::NoIndex {
            on: value.type_name(),
            key: key.type_name(),
        }),
    }
}

fn deeper_than(value: &Typed, limit: usize) -> bool {
    match value {
        Typed::List(items) => limit == 0 || items.iter().any(|i| deeper_than(i, limit - 1)),
        Typed::Object(entries) => {
            limit == 0 || entries.iter().any(|(_, v)| deeper_than(v, limit - 1))
        }
        _ => false,
    }
}

fn eval(node: &Node, scope: &Scope, frames: &mut Vec<Frame>) -> Cell {
    thread_local! {
        static DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }
    let depth = DEPTH.get() + 1;
    if depth > MAX_EVAL_DEPTH {
        return Err(EvalError::TooDeep);
    }
    DEPTH.set(depth);
    let cell = eval_node(node, scope, frames);
    DEPTH.set(depth - 1);
    cell
}

fn eval_node(node: &Node, scope: &Scope, frames: &mut Vec<Frame>) -> Cell {
    match node {
        Node::Literal(value) => Ok(value.clone()),
        Node::List(items) => Ok(Typed::List(
            items
                .iter()
                .map(|item| eval(item, scope, frames))
                .collect::<Result<_, _>>()?,
        )),
        Node::Object(entries) => Ok(Typed::Object(
            entries
                .iter()
                .map(|(key, value)| Ok((key.clone(), eval(value, scope, frames)?)))
                .collect::<Result<_, EvalError>>()?,
        )),
        Node::Property(name) => Ok(scope
            .file
            .as_ref()
            .and_then(|path| scope.files.property(path, name))
            .unwrap_or(Typed::Null)),
        Node::Note => Ok(scope.file.as_ref().map_or(Typed::Null, |path| {
            Typed::Object(scope.files.properties(path))
        })),
        Node::File => Ok(scope.file.clone().map_or(Typed::Null, Typed::File)),
        Node::FileField(field) => Ok(scope
            .file
            .as_ref()
            .map_or(Typed::Null, |path| file_field(scope, path, *field))),
        Node::This => Ok(scope.this.clone().map_or(Typed::Null, Typed::File)),
        Node::ThisProperty(name) => Ok(scope
            .this
            .as_ref()
            .and_then(|path| scope.files.property(path, name))
            .unwrap_or(Typed::Null)),
        Node::Formula(name) => {
            let unknown = || EvalError::UnknownFormula(name.clone());
            let row = scope.formulas.ok_or_else(unknown)?;
            let at = row
                .set
                .names
                .iter()
                .position(|n| n == name)
                .ok_or_else(unknown)?;
            if let Some(done) = &row.done.borrow()[at] {
                return done.clone();
            }
            let cell = eval(&row.set.exprs[at].node, scope, &mut Vec::new());
            row.done.borrow_mut()[at] = Some(cell.clone());
            cell
        }
        Node::Local(local) => Ok(match local {
            Local::Values => scope.values.cloned().unwrap_or(Typed::Null),
            Local::Value => frames.last().map_or(Typed::Null, |f| f.value.clone()),
            Local::Index => frames
                .last()
                .map_or(Typed::Null, |f| Typed::Number(f.index as f64)),
            Local::Acc => frames
                .iter()
                .rev()
                .find_map(|f| f.acc.clone())
                .unwrap_or(Typed::Null),
        }),
        Node::Not(inner) => Ok(Typed::Bool(!eval(inner, scope, frames)?.is_truthy())),
        Node::Negate(inner) => match eval(inner, scope, frames)? {
            Typed::Null => Ok(Typed::Null),
            Typed::Number(n) => Ok(Typed::Number(-n)),
            Typed::Duration(d) => Ok(Typed::Duration(d.scaled(-1.0))),
            other => Err(EvalError::Negate(other.type_name())),
        },
        Node::And(a, b) => Ok(Typed::Bool(
            eval(a, scope, frames)?.is_truthy() && eval(b, scope, frames)?.is_truthy(),
        )),
        Node::Or(a, b) => Ok(Typed::Bool(
            eval(a, scope, frames)?.is_truthy() || eval(b, scope, frames)?.is_truthy(),
        )),
        Node::Compare(op, a, b) => Ok(Typed::Bool(compare(
            *op,
            &eval(a, scope, frames)?,
            &eval(b, scope, frames)?,
        ))),
        Node::Arith(op, a, b) => arith(
            *op,
            eval(a, scope, frames)?,
            eval(b, scope, frames)?,
            scope.now,
        ),
        Node::Member(inner, name) => member(scope, eval(inner, scope, frames)?, name),
        Node::Index(inner, key) => index(
            scope,
            eval(inner, scope, frames)?,
            eval(key, scope, frames)?,
        ),
        Node::Call(Global::If, args) => {
            if eval(&args[0], scope, frames)?.is_truthy() {
                eval(&args[1], scope, frames)
            } else {
                args.get(2)
                    .map_or(Ok(Typed::Null), |otherwise| eval(otherwise, scope, frames))
            }
        }
        Node::Call(global, args) => {
            let args = args
                .iter()
                .map(|arg| eval(arg, scope, frames))
                .collect::<Result<Vec<_>, _>>()?;
            call(scope, *global, args)
        }
        Node::Method(receiver, method, args) => {
            let receiver = eval(receiver, scope, frames)?;
            match method {
                Method::Filter | Method::Map | Method::Reduce => {
                    each(scope, frames, receiver, *method, args)
                }
                _ => {
                    let args = args
                        .iter()
                        .map(|arg| eval(arg, scope, frames))
                        .collect::<Result<Vec<_>, _>>()?;
                    apply(scope, *method, receiver, args)
                }
            }
        }
    }
}

fn each(
    scope: &Scope,
    frames: &mut Vec<Frame>,
    receiver: Typed,
    method: Method,
    args: &[Node],
) -> Cell {
    let items = match receiver {
        Typed::Null => return Ok(Typed::Null),
        Typed::List(items) => items,
        other => {
            return Err(EvalError::NoMethod {
                method: method.name(),
                on: other.type_name(),
            });
        }
    };
    let mut acc = match method {
        Method::Reduce => Some(eval(&args[1], scope, frames)?),
        _ => None,
    };
    let mut out = Vec::new();
    for (index, value) in items.into_iter().enumerate() {
        frames.push(Frame {
            value: value.clone(),
            index,
            acc: acc.clone(),
        });
        let result = eval(&args[0], scope, frames);
        frames.pop();
        let result = result?;
        match method {
            Method::Filter if result.is_truthy() => out.push(value),
            Method::Filter => {}
            Method::Map => out.push(result),
            _ if deeper_than(&result, MAX_VALUE_DEPTH) => return Err(EvalError::TooDeep),
            _ => acc = Some(result),
        }
    }
    Ok(acc.unwrap_or(Typed::List(out)))
}

fn wrong(
    function: &'static str,
    position: usize,
    expected: &'static str,
    found: &Typed,
) -> EvalError {
    EvalError::Argument {
        function,
        position,
        expected,
        found: found.type_name(),
    }
}

fn text_arg(function: &'static str, position: usize, value: &Typed) -> Result<String, EvalError> {
    value
        .as_str()
        .map(|s| s.into_owned())
        .ok_or_else(|| wrong(function, position, "string", value))
}

fn number_arg(function: &'static str, position: usize, value: &Typed) -> Result<f64, EvalError> {
    match value {
        Typed::Number(n) => Ok(*n),
        other => Err(wrong(function, position, "number", other)),
    }
}

fn call(scope: &Scope, global: Global, args: Vec<Typed>) -> Cell {
    let name = global.name();
    let first = args.first().cloned().unwrap_or(Typed::Null);
    match global {
        Global::Now => {
            return Ok(Typed::Date(Date {
                time: true,
                relative: false,
                ..scope.now
            }));
        }
        Global::Today => {
            return scope
                .now
                .date_only()
                .map(Typed::Date)
                .ok_or(EvalError::DateRange);
        }
        Global::Random => return Ok(Typed::Number(random())),
        Global::List => {
            return Ok(match first {
                Typed::List(_) => first,
                other => Typed::List(vec![other]),
            });
        }
        Global::Max | Global::Min => {
            let mut best: Option<f64> = None;
            for (at, arg) in args.iter().enumerate() {
                if *arg == Typed::Null {
                    return Ok(Typed::Null);
                }
                let n = number_arg(name, at + 1, arg)?;
                best = Some(match best {
                    None => n,
                    Some(b) if global == Global::Max => b.max(n),
                    Some(b) => b.min(n),
                });
            }
            return Ok(best.map_or(Typed::Null, Typed::Number));
        }
        _ => {}
    }
    if first == Typed::Null {
        return Ok(Typed::Null);
    }
    match global {
        Global::EscapeHtml => {
            let text = text_arg(name, 1, &first)?;
            let mut out = String::with_capacity(text.len());
            for c in text.chars() {
                match c {
                    '&' => out.push_str("&amp;"),
                    '<' => out.push_str("&lt;"),
                    '>' => out.push_str("&gt;"),
                    '"' => out.push_str("&quot;"),
                    '\'' => out.push_str("&#39;"),
                    other => out.push(other),
                }
            }
            Ok(Typed::Text(out))
        }
        Global::Date => match &first {
            Typed::Date(_) => Ok(first),
            Typed::Text(text) => {
                Date::parse(text)
                    .map(Typed::Date)
                    .ok_or_else(|| EvalError::Parse {
                        what: "date",
                        text: text.clone(),
                    })
            }
            other => Err(wrong(name, 1, "string", other)),
        },
        Global::Duration => match &first {
            Typed::Duration(_) => Ok(first),
            Typed::Text(text) => {
                Duration::parse(text)
                    .map(Typed::Duration)
                    .ok_or_else(|| EvalError::Parse {
                        what: "duration",
                        text: text.clone(),
                    })
            }
            other => Err(wrong(name, 1, "string", other)),
        },
        Global::File => match &first {
            Typed::Url(_) => Ok(Typed::Null),
            Typed::File(_) | Typed::Link(_) | Typed::Text(_) => {
                Ok(match target_of(scope, &first) {
                    Some(Target::File(path)) => Typed::File(path),
                    _ => Typed::Null,
                })
            }
            other => Err(wrong(name, 1, "string, file or link", other)),
        },
        Global::Html => Ok(Typed::Html(text_arg(name, 1, &first)?)),
        Global::Icon => Ok(Typed::Icon(text_arg(name, 1, &first)?)),
        Global::Image => match &first {
            Typed::File(path) => Ok(Typed::Image(path.to_string())),
            Typed::Link(link) => Ok(Typed::Image(
                link.file
                    .as_ref()
                    .map_or_else(|| link.target.clone(), |path| path.to_string()),
            )),
            Typed::Image(_) => Ok(first),
            other => Ok(Typed::Image(text_arg(name, 1, other)?)),
        },
        Global::Link => {
            let display = match args.get(1) {
                None | Some(Typed::Null) => None,
                Some(display @ (Typed::Text(_) | Typed::Icon(_))) => {
                    Some(Box::new(display.clone()))
                }
                Some(other) => Some(Box::new(Typed::Text(other.text(scope.now)))),
            };
            match first {
                Typed::File(path) => Ok(Typed::Link(Link {
                    display,
                    ..link_to(&path)
                })),
                Typed::Link(link) => Ok(Typed::Link(Link {
                    display: display.or(link.display.clone()),
                    ..link
                })),
                Typed::Url(url) => Ok(Typed::Url(Url {
                    display: display.or(url.display.clone()),
                    ..url
                })),
                Typed::Text(text) if is_url(&text) => Ok(Typed::Url(Url {
                    address: text,
                    display,
                })),
                Typed::Text(text) => Ok(Typed::Link(link_from(scope, &text, display))),
                other => Err(wrong(name, 1, "string or file", &other)),
            }
        }
        Global::Number => match &first {
            Typed::Number(_) => Ok(first),
            Typed::Bool(b) => Ok(Typed::Number(f64::from(u8::from(*b)))),
            Typed::Date(date) => Ok(Typed::Number(date.millis as f64)),
            Typed::Text(text) => crate::typed::parse_number(text)
                .map(Typed::Number)
                .ok_or_else(|| EvalError::Parse {
                    what: "number",
                    text: text.clone(),
                }),
            other => Err(wrong(name, 1, "number, string, boolean or date", other)),
        },
        Global::If
        | Global::Now
        | Global::Today
        | Global::Random
        | Global::List
        | Global::Max
        | Global::Min => Ok(Typed::Null),
    }
}

fn is_question(method: Method) -> bool {
    matches!(
        method,
        Method::Contains
            | Method::ContainsAll
            | Method::ContainsAny
            | Method::StartsWith
            | Method::EndsWith
            | Method::HasLink
            | Method::HasProperty
            | Method::HasTag
            | Method::InFolder
            | Method::LinksTo
            | Method::Matches
    )
}

fn slice_bounds(len: usize, start: f64, end: Option<f64>) -> (usize, usize) {
    let place = |at: f64| {
        let at = at.trunc();
        if at < 0.0 {
            (len as f64 + at).max(0.0) as usize
        } else {
            at.min(len as f64) as usize
        }
    };
    let start = place(start);
    let end = end.map_or(len, place);
    (start, end.max(start))
}

fn round_half_up(n: f64) -> f64 {
    (n + 0.5).floor()
}

fn to_fixed(n: f64, places: usize) -> String {
    const GUARD: usize = 25;
    let long = format!("{:.*}", places + GUARD, n.abs());
    let tail = &long[long.len() - GUARD..];
    if tail.starts_with('5') && tail[1..].bytes().all(|b| b == b'0') {
        let step = 10f64.powi(-(places as i32) - 2);
        return format!("{:.*}", places, n + n.signum() * step);
    }
    format!("{n:.places$}")
}

fn expand(replacement: &str, found: &regex::Captures, out: &mut String) {
    let chars: Vec<char> = replacement.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' || i + 1 == chars.len() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        match chars[i + 1] {
            '$' => {
                out.push('$');
                i += 2;
            }
            '&' => {
                out.push_str(&found[0]);
                i += 2;
            }
            '<' => match chars[i + 2..].iter().position(|c| *c == '>') {
                Some(len) => {
                    let name: String = chars[i + 2..i + 2 + len].iter().collect();
                    if let Some(group) = found.name(&name) {
                        out.push_str(group.as_str());
                    }
                    i += len + 3;
                }
                None => {
                    out.push('$');
                    i += 1;
                }
            },
            d if d.is_ascii_digit() => {
                let one = d as usize - '0' as usize;
                let two = chars
                    .get(i + 2)
                    .filter(|c| c.is_ascii_digit())
                    .map(|c| one * 10 + (*c as usize - '0' as usize));
                let (group, used) = match two {
                    Some(n) if n >= 1 && n < found.len() => (n, 3),
                    _ => (one, 2),
                };
                if group >= 1 && group < found.len() {
                    out.push_str(found.get(group).map_or("", |m| m.as_str()));
                } else {
                    out.extend(&chars[i..i + used]);
                }
                i += used;
            }
            _ => {
                out.push('$');
                i += 1;
            }
        }
    }
}

fn replace(text: &str, pattern: &Pattern, replacement: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    for found in pattern.regex.captures_iter(text) {
        let Some(whole) = found.get(0) else { continue };
        out.push_str(&text[last..whole.start()]);
        expand(replacement, &found, &mut out);
        last = whole.end();
        if !pattern.flags.contains('g') {
            break;
        }
    }
    out.push_str(&text[last..]);
    out
}

fn numbers(items: &[Typed]) -> Vec<f64> {
    items
        .iter()
        .filter_map(|item| match item {
            Typed::Number(n) => Some(*n),
            _ => None,
        })
        .collect()
}

pub(crate) fn mean(items: &[Typed]) -> Typed {
    let numbers = numbers(items);
    if numbers.is_empty() {
        return Typed::Null;
    }
    Typed::Number(numbers.iter().sum::<f64>() / numbers.len() as f64)
}

pub(crate) fn median(items: &[Typed]) -> Typed {
    let mut numbers = numbers(items);
    if numbers.is_empty() {
        return Typed::Null;
    }
    numbers.sort_by(f64::total_cmp);
    let middle = numbers.len() / 2;
    Typed::Number(if numbers.len() % 2 == 1 {
        numbers[middle]
    } else {
        (numbers[middle - 1] + numbers[middle]) / 2.0
    })
}

pub(crate) fn stddev(items: &[Typed]) -> Typed {
    let numbers = numbers(items);
    if numbers.is_empty() {
        return Typed::Null;
    }
    let mean = numbers.iter().sum::<f64>() / numbers.len() as f64;
    let variance = numbers.iter().map(|n| (n - mean).powi(2)).sum::<f64>() / numbers.len() as f64;
    Typed::Number(variance.sqrt())
}

pub(crate) fn unique(items: &[Typed]) -> Vec<Typed> {
    let mut out: Vec<Typed> = Vec::new();
    for item in items {
        if !out.iter().any(|kept| kept.equals(item)) {
            out.push(item.clone());
        }
    }
    out
}

fn apply(scope: &Scope, method: Method, receiver: Typed, args: Vec<Typed>) -> Cell {
    use Method as M;
    use Typed as T;
    let name = method.name();
    match method {
        M::IsTruthy => return Ok(T::Bool(receiver.is_truthy())),
        M::IsEmpty => return Ok(T::Bool(receiver.is_empty())),
        M::IsType => {
            let wanted = text_arg(name, 1, &args[0])?;
            if !TYPE_NAMES.contains(&wanted.as_str()) {
                return Err(EvalError::UnknownType(wanted));
            }
            return Ok(T::Bool(receiver.type_name() == wanted));
        }
        _ => {}
    }
    let question = is_question(method);
    if receiver == T::Null {
        return Ok(if question { T::Bool(false) } else { T::Null });
    }
    if !question && args.contains(&T::Null) {
        return Ok(T::Null);
    }
    if method == M::ToString {
        return Ok(T::Text(receiver.text(scope.now)));
    }
    let no_method = || {
        if question {
            Ok(T::Bool(false))
        } else {
            Err(EvalError::NoMethod {
                method: name,
                on: receiver.type_name(),
            })
        }
    };
    if let Some(text) = receiver.as_str() {
        let text: &str = &text;
        let has = |wanted: &Typed| wanted.as_str().is_some_and(|w| text.contains(&*w));
        return match method {
            M::Contains => Ok(T::Bool(has(&args[0]))),
            M::ContainsAll => Ok(T::Bool(args.iter().all(has))),
            M::ContainsAny => Ok(T::Bool(args.iter().any(has))),
            M::StartsWith => Ok(T::Bool(
                args[0].as_str().is_some_and(|w| text.starts_with(&*w)),
            )),
            M::EndsWith => Ok(T::Bool(
                args[0].as_str().is_some_and(|w| text.ends_with(&*w)),
            )),
            M::Lower => Ok(T::Text(text.to_lowercase())),
            M::Trim => Ok(T::Text(text.trim().to_string())),
            M::Reverse => Ok(T::Text(text.chars().rev().collect())),
            M::Title => {
                let mut out = String::with_capacity(text.len());
                let mut start = true;
                for c in text.chars() {
                    if start {
                        out.extend(c.to_uppercase());
                    } else {
                        out.push(c);
                    }
                    start = c.is_whitespace();
                }
                Ok(T::Text(out))
            }
            M::Repeat => {
                let count = number_arg(name, 1, &args[0])?;
                if count < 0.0 || count.fract() != 0.0 {
                    return Err(wrong(name, 1, "a whole number, zero or more", &args[0]));
                }
                if count * text.len() as f64 > MAX_REPEAT_BYTES as f64 {
                    return Err(EvalError::TooLarge { function: name });
                }
                Ok(T::Text(text.repeat(count as usize)))
            }
            M::Slice => {
                let chars: Vec<char> = text.chars().collect();
                let end = args
                    .get(1)
                    .map(|end| number_arg(name, 2, end))
                    .transpose()?;
                let (start, end) = slice_bounds(chars.len(), number_arg(name, 1, &args[0])?, end);
                Ok(T::Text(chars[start..end].iter().collect()))
            }
            M::Split => {
                let mut parts: Vec<Typed> = match &args[0] {
                    T::Regex(pattern) => pattern
                        .regex
                        .split(text)
                        .map(|part| T::Text(part.to_string()))
                        .collect(),
                    separator => {
                        let separator = text_arg(name, 1, separator)?;
                        if separator.is_empty() {
                            text.chars().map(|c| T::Text(c.to_string())).collect()
                        } else {
                            text.split(&separator)
                                .map(|part| T::Text(part.to_string()))
                                .collect()
                        }
                    }
                };
                if let Some(limit) = args.get(1) {
                    let limit = number_arg(name, 2, limit)?;
                    parts.truncate(limit.max(0.0) as usize);
                }
                Ok(T::List(parts))
            }
            M::Replace => {
                let replacement = text_arg(name, 2, &args[1])?;
                match &args[0] {
                    T::Regex(pattern) => Ok(T::Text(replace(text, pattern, &replacement))),
                    plain => {
                        let plain = text_arg(name, 1, plain)?;
                        if plain.is_empty() {
                            return Ok(T::Text(text.to_string()));
                        }
                        Ok(T::Text(text.replace(&plain, &replacement)))
                    }
                }
            }
            _ => no_method(),
        };
    }
    match (&receiver, method) {
        (T::List(items), M::Contains) => Ok(T::Bool(items.iter().any(|i| i.holds(&args[0])))),
        (T::List(items), M::ContainsAll) => Ok(T::Bool(
            args.iter().all(|w| items.iter().any(|i| i.holds(w))),
        )),
        (T::List(items), M::ContainsAny) => Ok(T::Bool(
            args.iter().any(|w| items.iter().any(|i| i.holds(w))),
        )),
        (T::List(items), M::Flat) => Ok(T::List(
            items
                .iter()
                .flat_map(|item| match item {
                    T::List(inner) => inner.clone(),
                    other => vec![other.clone()],
                })
                .collect(),
        )),
        (T::List(items), M::Join) => {
            let separator = text_arg(name, 1, &args[0])?;
            Ok(T::Text(
                items
                    .iter()
                    .map(|item| item.text(scope.now))
                    .collect::<Vec<_>>()
                    .join(&separator),
            ))
        }
        (T::List(items), M::Reverse) => Ok(T::List(items.iter().rev().cloned().collect())),
        (T::List(items), M::Slice) => {
            let end = args
                .get(1)
                .map(|end| number_arg(name, 2, end))
                .transpose()?;
            let (start, end) = slice_bounds(items.len(), number_arg(name, 1, &args[0])?, end);
            Ok(T::List(items[start..end].to_vec()))
        }
        (T::List(items), M::Sort) => {
            let mut sorted = items.clone();
            sorted.sort_by(Typed::sort_order);
            Ok(T::List(sorted))
        }
        (T::List(items), M::Unique) => Ok(T::List(unique(items))),
        (T::List(items), M::Mean) => Ok(mean(items)),
        (T::List(items), M::Median) => Ok(median(items)),
        (T::List(items), M::Stddev) => Ok(stddev(items)),
        (T::Number(n), M::Abs) => Ok(T::Number(n.abs())),
        (T::Number(n), M::Ceil) => Ok(T::Number(n.ceil())),
        (T::Number(n), M::Floor) => Ok(T::Number(n.floor())),
        (T::Number(n), M::Round) => match args.first() {
            None => Ok(T::Number(round_half_up(*n))),
            Some(digits) => {
                let digits = number_arg(name, 1, digits)?;
                if digits.fract() != 0.0 || digits.abs() > 100.0 {
                    return Err(wrong(name, 1, "a whole number", &args[0]));
                }
                let scale = 10f64.powi(digits as i32);
                Ok(T::Number(round_half_up(n * scale) / scale))
            }
        },
        (T::Number(n), M::ToFixed) => {
            let places = number_arg(name, 1, &args[0])?;
            if !(0.0..=100.0).contains(&places) || places.fract() != 0.0 {
                return Err(wrong(name, 1, "a whole number from 0 to 100", &args[0]));
            }
            if !n.is_finite() {
                return Ok(T::Text(crate::typed::number_text(*n)));
            }
            Ok(T::Text(to_fixed(*n, places as usize)))
        }
        (T::Date(date), M::Date) => date.date_only().map(T::Date).ok_or(EvalError::DateRange),
        (T::Date(date), M::Format) => Ok(T::Text(date.format(&text_arg(name, 1, &args[0])?)?)),
        (T::Date(date), M::Time) => Ok(T::Text(date.time_text())),
        (T::Date(date), M::Relative) => Ok(T::Date(Date {
            relative: true,
            ..*date
        })),
        (T::Link(link), M::AsFile) => Ok(link.file.clone().map_or(T::Null, T::File)),
        (T::Link(link), M::LinksTo) => {
            Ok(T::Bool(match (&link.file, target_of(scope, &args[0])) {
                (Some(from), Some(target)) => links_to(scope, from, &target),
                _ => false,
            }))
        }
        (T::File(path), M::LinksTo | M::HasLink) => Ok(T::Bool(
            target_of(scope, &args[0]).is_some_and(|target| links_to(scope, path, &target)),
        )),
        (T::File(path), M::AsLink) => {
            let display = match args.first() {
                None => None,
                Some(display @ (T::Text(_) | T::Icon(_))) => Some(Box::new(display.clone())),
                Some(other) => return Err(wrong(name, 1, "string", other)),
            };
            Ok(T::Link(Link {
                display,
                ..link_to(path)
            }))
        }
        (T::File(path), M::HasProperty) => {
            Ok(T::Bool(args[0].as_str().is_some_and(|wanted| {
                scope.files.property(path, &wanted).is_some()
            })))
        }
        (T::File(path), M::HasTag) => {
            let tags = scope.files.tags(path);
            Ok(T::Bool(args.iter().any(|wanted| {
                wanted
                    .as_str()
                    .is_some_and(|w| tags.iter().any(|tag| tag_within(tag, &w)))
            })))
        }
        (T::File(path), M::InFolder) => {
            Ok(T::Bool(args[0].as_str().is_some_and(|wanted| {
                folder_contains(folder_of(path), &wanted)
            })))
        }
        (T::Object(entries), M::Keys) => Ok(T::List(
            entries
                .iter()
                .map(|(key, _)| T::Text(key.clone()))
                .collect(),
        )),
        (T::Object(entries), M::Values) => Ok(T::List(
            entries.iter().map(|(_, value)| value.clone()).collect(),
        )),
        (T::Regex(pattern), M::Matches) => Ok(T::Bool(
            args[0]
                .as_str()
                .is_some_and(|text| pattern.regex.is_match(&text)),
        )),
        _ => no_method(),
    }
}
