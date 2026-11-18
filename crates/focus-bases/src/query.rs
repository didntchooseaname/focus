use std::path::Path;
use std::sync::Arc;

use focus_core::{NoteId, Vault};

use crate::base::{Base, Direction, Filter, View};
use crate::date::Date;
use crate::error::{BasesError, Part, QueryError};
use crate::eval::{Cell, EvalError, Formulas, RowFormulas, Scope, mean, median, stddev, unique};
use crate::expr::{Expr, ExprError, Feature, FileField, Node, compile, compile_summary};
use crate::source::{FileSource, VaultFiles};
use crate::table::load;
use crate::typed::Typed;

const MAX_FORMULA_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub id: String,
    pub label: String,
    pub named: bool,
    pub link: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub path: Arc<str>,
    pub note: Option<NoteId>,
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub key: Typed,
    pub rows: Vec<usize>,
    pub summaries: Vec<Cell>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub property: String,
    pub name: String,
    pub built_in: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewResult {
    pub view: Option<String>,
    pub kind: String,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub matched: usize,
    pub group_by: Option<String>,
    pub groups: Vec<Group>,
    pub summaries: Vec<Summary>,
    pub now: Date,
}

pub(crate) struct Settings<'a> {
    pub this: Option<&'a str>,
    pub now: Date,
    pub refuse_this: bool,
}

enum Compiled {
    Statement(String, Expr),
    And(Vec<Compiled>),
    Or(Vec<Compiled>),
    Not(Vec<Compiled>),
}

impl Compiled {
    fn holds(&self, scope: &Scope) -> Result<bool, (String, EvalError)> {
        let any = |items: &[Compiled]| {
            for item in items {
                if item.holds(scope)? {
                    return Ok(true);
                }
            }
            Ok(false)
        };
        match self {
            Compiled::Statement(source, expr) => expr
                .eval(scope)
                .map(|value| value.is_truthy())
                .map_err(|error| (source.clone(), error)),
            Compiled::And(items) => {
                for item in items {
                    if !item.holds(scope)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Compiled::Or(items) => any(items),
            Compiled::Not(items) => any(items).map(|found| !found),
        }
    }

    fn statements<'a>(&'a self, out: &mut Vec<(&'a str, &'a Expr)>) {
        match self {
            Compiled::Statement(source, expr) => out.push((source, expr)),
            Compiled::And(items) | Compiled::Or(items) | Compiled::Not(items) => {
                items.iter().for_each(|item| item.statements(out))
            }
        }
    }
}

fn expression(source: &str, summary: bool) -> Result<Expr, BasesError> {
    let compiled = if summary {
        compile_summary(source)
    } else {
        compile(source)
    };
    compiled.map_err(|error| match error {
        ExprError::Unsupported(feature, subject) => BasesError::Unsupported {
            expression: source.to_string(),
            feature,
            subject,
        },
        ExprError::Syntax(message) => BasesError::Syntax {
            expression: source.to_string(),
            message,
        },
    })
}

fn compile_filter(filter: &Filter) -> Result<Compiled, BasesError> {
    let all = |items: &[Filter]| items.iter().map(compile_filter).collect::<Result<_, _>>();
    Ok(match filter {
        Filter::Statement(source) => {
            Compiled::Statement(source.clone(), expression(source, false)?)
        }
        Filter::And(items) => Compiled::And(all(items)?),
        Filter::Or(items) => Compiled::Or(all(items)?),
        Filter::Not(items) => Compiled::Not(all(items)?),
    })
}

pub(crate) fn normal_id(property: &str) -> String {
    if property.starts_with("file.")
        || property.starts_with("note.")
        || property.starts_with("formula.")
    {
        property.to_string()
    } else {
        format!("note.{property}")
    }
}

fn property(written: &str) -> Result<(String, Expr), BasesError> {
    let id = normal_id(written);
    let (id, node) = if let Some(name) = id.strip_prefix("note.") {
        (id.clone(), Node::Property(name.to_string()))
    } else if let Some(name) = id.strip_prefix("formula.") {
        (id.clone(), Node::Formula(name.to_string()))
    } else {
        let name = id.strip_prefix("file.").unwrap_or(&id);
        match FileField::parse(name) {
            Some(field) => (field.id().to_string(), Node::FileField(field)),
            None => {
                return Err(BasesError::Unsupported {
                    expression: written.to_string(),
                    feature: Feature::Property,
                    subject: id,
                });
            }
        }
    };
    Ok((id, Expr { node }))
}

fn link_formulas(
    base: &Base,
    roots: &[(&str, &Expr)],
    refuse_this: bool,
) -> Result<Formulas, QueryError> {
    fn visit(
        base: &Base,
        name: &str,
        used_in: &str,
        refuse_this: bool,
        path: &mut Vec<String>,
        set: &mut Formulas,
    ) -> Result<(), QueryError> {
        if set.names.iter().any(|n| n == name) {
            return Ok(());
        }
        if let Some(at) = path.iter().position(|n| n == name) {
            let mut cycle = path[at..].to_vec();
            cycle.push(name.to_string());
            return Err(QueryError::FormulaCycle(cycle));
        }
        let Some((_, source)) = base.formulas.iter().find(|(n, _)| n == name) else {
            return Err(QueryError::UnknownFormula {
                expression: used_in.to_string(),
                name: name.to_string(),
            });
        };
        if path.len() >= MAX_FORMULA_DEPTH {
            return Err(BasesError::Invalid(format!(
                "formulas read each other more than {MAX_FORMULA_DEPTH} deep"
            ))
            .into());
        }
        let expr = expression(source, false)?;
        refuse(source, &expr, refuse_this)?;
        path.push(name.to_string());
        for inner in expr.formulas() {
            visit(base, &inner, source, refuse_this, path, set)?;
        }
        path.pop();
        set.names.push(name.to_string());
        set.exprs.push(expr);
        Ok(())
    }
    let mut set = Formulas {
        names: Vec::new(),
        exprs: Vec::new(),
    };
    for (source, expr) in roots {
        for name in expr.formulas() {
            visit(base, &name, source, refuse_this, &mut Vec::new(), &mut set)?;
        }
    }
    Ok(set)
}

fn refuse(source: &str, expr: &Expr, refuse_this: bool) -> Result<(), BasesError> {
    match expr.this_use().filter(|_| refuse_this) {
        Some(subject) => Err(BasesError::Unsupported {
            expression: source.to_string(),
            feature: Feature::This,
            subject,
        }),
        None => Ok(()),
    }
}

#[derive(Clone, Copy)]
enum BuiltIn {
    Average,
    Min,
    Max,
    Sum,
    Range,
    Median,
    Stddev,
    Earliest,
    Latest,
    Checked,
    Unchecked,
    Empty,
    Filled,
    Unique,
}

impl BuiltIn {
    fn parse(name: &str) -> Option<BuiltIn> {
        Some(match name {
            "Average" => BuiltIn::Average,
            "Min" => BuiltIn::Min,
            "Max" => BuiltIn::Max,
            "Sum" => BuiltIn::Sum,
            "Range" => BuiltIn::Range,
            "Median" => BuiltIn::Median,
            "Stddev" => BuiltIn::Stddev,
            "Earliest" => BuiltIn::Earliest,
            "Latest" => BuiltIn::Latest,
            "Checked" => BuiltIn::Checked,
            "Unchecked" => BuiltIn::Unchecked,
            "Empty" => BuiltIn::Empty,
            "Filled" => BuiltIn::Filled,
            "Unique" => BuiltIn::Unique,
            _ => return None,
        })
    }

    fn reduce(self, values: &[Typed]) -> Cell {
        let numbers: Vec<f64> = values
            .iter()
            .filter_map(|value| match value {
                Typed::Number(n) => Some(*n),
                _ => None,
            })
            .collect();
        let dates: Vec<Date> = values
            .iter()
            .filter_map(|value| match value {
                Typed::Date(date) => Some(*date),
                _ => None,
            })
            .collect();
        let count = |keep: &dyn Fn(&Typed) -> bool| {
            Typed::Number(values.iter().filter(|value| keep(value)).count() as f64)
        };
        let number = |found: Option<f64>| found.map_or(Typed::Null, Typed::Number);
        let low = numbers.iter().copied().reduce(f64::min);
        let high = numbers.iter().copied().reduce(f64::max);
        let earliest = dates.iter().copied().min_by_key(|date| date.millis);
        let latest = dates.iter().copied().max_by_key(|date| date.millis);
        Ok(match self {
            BuiltIn::Average => mean(values),
            BuiltIn::Median => median(values),
            BuiltIn::Stddev => stddev(values),
            BuiltIn::Min => number(low),
            BuiltIn::Max => number(high),
            BuiltIn::Sum if numbers.is_empty() => Typed::Null,
            BuiltIn::Sum => Typed::Number(numbers.iter().sum()),
            BuiltIn::Range => match (low.zip(high), earliest.zip(latest)) {
                (Some(_), Some(_)) => {
                    return Err(EvalError::Operator {
                        operator: "Range",
                        left: "number",
                        right: "date",
                    });
                }
                (Some((low, high)), None) => Typed::Number(high - low),
                (None, Some((earliest, latest))) => Typed::Duration(latest.since(earliest)),
                (None, None) => Typed::Null,
            },
            BuiltIn::Earliest => earliest.map_or(Typed::Null, Typed::Date),
            BuiltIn::Latest => latest.map_or(Typed::Null, Typed::Date),
            BuiltIn::Checked => count(&|value| *value == Typed::Bool(true)),
            BuiltIn::Unchecked => count(&|value| *value == Typed::Bool(false)),
            BuiltIn::Empty => count(&|value| value.is_empty()),
            BuiltIn::Filled => count(&|value| !value.is_empty()),
            BuiltIn::Unique => {
                let filled: Vec<Typed> = values
                    .iter()
                    .filter(|value| !value.is_empty())
                    .cloned()
                    .collect();
                Typed::Number(unique(&filled).len() as f64)
            }
        })
    }
}

enum Reducer {
    BuiltIn(BuiltIn),
    Formula(Expr),
}

struct Matched<'a> {
    path: Arc<str>,
    name: String,
    formulas: RowFormulas<'a>,
    keys: Vec<Typed>,
}

impl Base {
    pub(crate) fn view_index(&self, view: Option<&str>) -> Result<usize, BasesError> {
        match view {
            None => Ok(0),
            Some(name) => self
                .views
                .iter()
                .position(|v| v.name.as_deref() == Some(name))
                .ok_or_else(|| BasesError::UnknownView(name.to_string())),
        }
    }

    pub fn evaluate_at(
        &self,
        vault: &Vault,
        index: usize,
        this: Option<&Path>,
    ) -> Result<ViewResult, QueryError> {
        let files = VaultFiles::new(vault, true);
        let this = this.map(crate::source::slashed);
        self.evaluate_in(&files, index, this.as_deref(), Date::now())
    }

    pub fn evaluate_in(
        &self,
        files: &dyn FileSource,
        index: usize,
        this: Option<&str>,
        now: Date,
    ) -> Result<ViewResult, QueryError> {
        let view = self.views.get(index).ok_or(BasesError::NoView)?;
        run(
            self,
            view,
            files,
            &Settings {
                this,
                now,
                refuse_this: false,
            },
        )
    }
}

pub fn evaluate(
    vault: &Vault,
    base: &Path,
    view: Option<&str>,
    this: Option<&Path>,
) -> Result<ViewResult, QueryError> {
    let loaded = load(vault, base)?;
    let index = loaded.view_index(view)?;
    loaded.evaluate_at(vault, index, this)
}

pub(crate) fn run(
    base: &Base,
    view: &View,
    files: &dyn FileSource,
    settings: &Settings,
) -> Result<ViewResult, QueryError> {
    let filters = [base.filter.as_ref(), view.filter.as_ref()]
        .into_iter()
        .flatten()
        .map(compile_filter)
        .collect::<Result<Vec<_>, _>>()?;
    let default_order = ["file.name".to_string()];
    let order = if view.order.is_empty() {
        &default_order[..]
    } else {
        &view.order[..]
    };
    let mut columns = Vec::new();
    let mut column_exprs = Vec::new();
    for written in order {
        let (id, expr) = property(written)?;
        let named = base
            .display_names
            .iter()
            .find(|(key, _)| normal_id(key) == id)
            .map(|(_, name)| name.clone());
        let default = id
            .split_once('.')
            .map_or(id.as_str(), |(_, name)| name)
            .to_string();
        columns.push(Column {
            link: matches!(
                expr.node,
                Node::FileField(FileField::Name | FileField::Basename)
            ),
            named: named.is_some(),
            label: named.unwrap_or(default),
            id,
        });
        column_exprs.push(expr);
    }
    let sorts = view
        .sort
        .iter()
        .map(|key| property(&key.property).map(|(id, expr)| (id, expr, key.direction)))
        .collect::<Result<Vec<_>, _>>()?;
    let group = view
        .group_by
        .as_ref()
        .map(|key| property(&key.property).map(|(id, expr)| (id, expr, key.direction)))
        .transpose()?;
    let mut summaries = Vec::new();
    for (written, name) in &view.summaries {
        let (id, expr) = property(written)?;
        let reducer = match base.summaries.iter().find(|(n, _)| n == name) {
            Some((_, source)) => {
                let formula = expression(source, true)?;
                refuse(source, &formula, settings.refuse_this)?;
                Reducer::Formula(formula)
            }
            None => Reducer::BuiltIn(BuiltIn::parse(name).ok_or_else(|| {
                QueryError::UnknownSummary {
                    property: id.clone(),
                    name: name.clone(),
                }
            })?),
        };
        summaries.push((
            Summary {
                property: id,
                name: name.clone(),
                built_in: matches!(reducer, Reducer::BuiltIn(_)),
            },
            expr,
            reducer,
        ));
    }

    let mut roots: Vec<(&str, &Expr)> = Vec::new();
    filters.iter().for_each(|f| f.statements(&mut roots));
    for (source, expr) in &roots {
        refuse(source, expr, settings.refuse_this)?;
    }
    roots.extend(columns.iter().map(|c| c.id.as_str()).zip(&column_exprs));
    roots.extend(sorts.iter().map(|(id, expr, _)| (id.as_str(), expr)));
    roots.extend(group.iter().map(|(id, expr, _)| (id.as_str(), expr)));
    roots.extend(
        summaries
            .iter()
            .map(|(summary, expr, _)| (summary.property.as_str(), expr)),
    );
    let formulas = link_formulas(base, &roots, settings.refuse_this)?;

    let this: Option<Arc<str>> = settings.this.map(Arc::from);
    let scope_of = |path: &Arc<str>| Scope {
        files,
        file: Some(path.clone()),
        this: this.clone(),
        now: settings.now,
        formulas: None,
        values: None,
    };
    let failed =
        |part: Part, expression: &str, path: &Arc<str>, error: EvalError| QueryError::Row {
            part,
            expression: expression.to_string(),
            path: path.to_string(),
            error,
        };

    let mut matched: Vec<Matched> = Vec::new();
    for path in files.files() {
        let row = RowFormulas::new(&formulas);
        let mut keys = Vec::with_capacity(sorts.len());
        {
            let scope = Scope {
                formulas: Some(&row),
                ..scope_of(&path)
            };
            let mut keep = true;
            for filter in &filters {
                match filter.holds(&scope) {
                    Ok(true) => {}
                    Ok(false) => {
                        keep = false;
                        break;
                    }
                    Err((source, error)) => {
                        return Err(failed(Part::Filter, &source, &path, error));
                    }
                }
            }
            if !keep {
                continue;
            }
            for (id, expr, _) in &sorts {
                keys.push(
                    expr.eval(&scope)
                        .map_err(|error| failed(Part::Sort, id, &path, error))?,
                );
            }
        }
        matched.push(Matched {
            name: path.rsplit('/').next().unwrap_or(&path).to_lowercase(),
            path,
            formulas: row,
            keys,
        });
    }
    matched.sort_by(|a, b| {
        for (at, (_, _, direction)) in sorts.iter().enumerate() {
            let order = a.keys[at].sort_order(&b.keys[at]);
            let order = match direction {
                Direction::Asc => order,
                Direction::Desc => order.reverse(),
            };
            if order.is_ne() {
                return order;
            }
        }
        a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path))
    });
    let total = matched.len();
    matched.truncate(view.limit.unwrap_or(usize::MAX));

    let mut rows = Vec::with_capacity(matched.len());
    let mut group_keys = Vec::with_capacity(matched.len());
    let mut summary_inputs: Vec<Vec<Cell>> = vec![Vec::new(); summaries.len()];
    for row in &matched {
        let scope = Scope {
            formulas: Some(&row.formulas),
            ..scope_of(&row.path)
        };
        rows.push(Row {
            note: files.note(&row.path),
            cells: column_exprs.iter().map(|expr| expr.eval(&scope)).collect(),
            path: row.path.clone(),
        });
        if let Some((id, expr, _)) = &group {
            let key = expr
                .eval(&scope)
                .map_err(|error| failed(Part::Group, id, &row.path, error))?;
            group_keys.push(if key.is_empty() { Typed::Null } else { key });
        }
        for (inputs, (_, expr, _)) in summary_inputs.iter_mut().zip(&summaries) {
            inputs.push(expr.eval(&scope));
        }
    }

    let mut groups: Vec<(Typed, Vec<usize>)> = Vec::new();
    match &group {
        None => groups.push((Typed::Null, (0..rows.len()).collect())),
        Some((_, _, direction)) => {
            let mut by_key: Vec<usize> = (0..rows.len()).collect();
            by_key.sort_by(|a, b| group_keys[*a].sort_order(&group_keys[*b]));
            for at in by_key {
                match groups.last_mut() {
                    Some((key, members)) if key.sort_order(&group_keys[at]).is_eq() => {
                        members.push(at)
                    }
                    _ => groups.push((group_keys[at].clone(), vec![at])),
                }
            }
            if *direction == Direction::Desc {
                groups.reverse();
            }
        }
    }
    if let (Some(_), Some(listed)) = (&group, &view.group_order) {
        let mut shown: Vec<(Typed, Vec<usize>)> = Vec::new();
        for wanted in listed {
            if shown.iter().any(|(key, _)| key.equals(wanted)) {
                continue;
            }
            match groups.iter().position(|(key, _)| key.equals(wanted)) {
                Some(at) => shown.push(groups.swap_remove(at)),
                None => shown.push((wanted.clone(), Vec::new())),
            }
        }
        groups = shown;
    }

    let groups = groups
        .into_iter()
        .map(|(key, members)| Group {
            summaries: summaries
                .iter()
                .zip(&summary_inputs)
                .map(|((_, _, reducer), inputs)| {
                    let values = members
                        .iter()
                        .map(|at| inputs[*at].clone())
                        .collect::<Result<Vec<_>, _>>()?;
                    match reducer {
                        Reducer::BuiltIn(built_in) => built_in.reduce(&values),
                        Reducer::Formula(formula) => formula.eval(&Scope {
                            files,
                            file: None,
                            this: this.clone(),
                            now: settings.now,
                            formulas: None,
                            values: Some(&Typed::List(values)),
                        }),
                    }
                })
                .collect(),
            key,
            rows: members,
        })
        .collect();

    Ok(ViewResult {
        view: view.name.clone(),
        kind: view.kind.clone(),
        columns,
        rows,
        matched: total,
        group_by: group.map(|(id, _, _)| id),
        groups,
        summaries: summaries
            .into_iter()
            .map(|(summary, _, _)| summary)
            .collect(),
        now: settings.now,
    })
}
