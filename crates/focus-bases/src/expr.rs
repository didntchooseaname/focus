use crate::typed::{Pattern, Typed};

const MAX_DEPTH: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileField {
    Name,
    Basename,
    Ext,
    Folder,
    Path,
    Mtime,
    Ctime,
    Size,
    Tags,
    Links,
    Embeds,
    Backlinks,
    Properties,
    File,
}

impl FileField {
    pub fn parse(name: &str) -> Option<FileField> {
        Some(match name {
            "name" => FileField::Name,
            "basename" => FileField::Basename,
            "ext" => FileField::Ext,
            "folder" => FileField::Folder,
            "path" => FileField::Path,
            "mtime" => FileField::Mtime,
            "ctime" => FileField::Ctime,
            "size" => FileField::Size,
            "tags" => FileField::Tags,
            "links" => FileField::Links,
            "embeds" => FileField::Embeds,
            "backlinks" => FileField::Backlinks,
            "properties" => FileField::Properties,
            "file" => FileField::File,
            _ => return None,
        })
    }

    pub fn id(self) -> &'static str {
        match self {
            FileField::Name => "file.name",
            FileField::Basename => "file.basename",
            FileField::Ext => "file.ext",
            FileField::Folder => "file.folder",
            FileField::Path => "file.path",
            FileField::Mtime => "file.mtime",
            FileField::Ctime => "file.ctime",
            FileField::Size => "file.size",
            FileField::Tags => "file.tags",
            FileField::Links => "file.links",
            FileField::Embeds => "file.embeds",
            FileField::Backlinks => "file.backlinks",
            FileField::Properties => "file.properties",
            FileField::File => "file.file",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Function,
    Method,
    Property,
    Formula,
    This,
    Index,
    SortDirection,
}

impl Feature {
    pub fn name(self) -> &'static str {
        match self {
            Feature::Function => "function",
            Feature::Method => "method",
            Feature::Property => "property",
            Feature::Formula => "formula",
            Feature::This => "this property",
            Feature::Index => "index access",
            Feature::SortDirection => "sort direction",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprError {
    Syntax(String),
    Unsupported(Feature, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Compare {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Arith {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl Arith {
    pub(crate) fn symbol(self) -> &'static str {
        match self {
            Arith::Add => "+",
            Arith::Sub => "-",
            Arith::Mul => "*",
            Arith::Div => "/",
            Arith::Rem => "%",
        }
    }
}

macro_rules! names {
    ($(#[$meta:meta])* $kind:ident { $($variant:ident = $name:literal, $min:literal, $max:expr;)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum $kind { $($variant),* }

        impl $kind {
            fn parse(name: &str) -> Option<($kind, usize, usize)> {
                match name {
                    $($name => Some(($kind::$variant, $min, $max)),)*
                    _ => None,
                }
            }

            pub(crate) fn name(self) -> &'static str {
                match self { $($kind::$variant => $name),* }
            }
        }
    };
}

names! {
    Global {
        EscapeHtml = "escapeHTML", 1, 1;
        Date = "date", 1, 1;
        Duration = "duration", 1, 1;
        File = "file", 1, 1;
        Html = "html", 1, 1;
        If = "if", 2, 3;
        Image = "image", 1, 1;
        Icon = "icon", 1, 1;
        Link = "link", 1, 2;
        List = "list", 1, 1;
        Max = "max", 1, usize::MAX;
        Min = "min", 1, usize::MAX;
        Now = "now", 0, 0;
        Number = "number", 1, 1;
        Today = "today", 0, 0;
        Random = "random", 0, 0;
    }
}

names! {
    Method {
        IsTruthy = "isTruthy", 0, 0;
        IsType = "isType", 1, 1;
        ToString = "toString", 0, 0;
        IsEmpty = "isEmpty", 0, 0;
        Date = "date", 0, 0;
        Format = "format", 1, 1;
        Time = "time", 0, 0;
        Relative = "relative", 0, 0;
        Contains = "contains", 1, 1;
        ContainsAll = "containsAll", 1, usize::MAX;
        ContainsAny = "containsAny", 1, usize::MAX;
        EndsWith = "endsWith", 1, 1;
        StartsWith = "startsWith", 1, 1;
        Lower = "lower", 0, 0;
        Replace = "replace", 2, 2;
        Repeat = "repeat", 1, 1;
        Reverse = "reverse", 0, 0;
        Slice = "slice", 1, 2;
        Split = "split", 1, 2;
        Title = "title", 0, 0;
        Trim = "trim", 0, 0;
        Abs = "abs", 0, 0;
        Ceil = "ceil", 0, 0;
        Floor = "floor", 0, 0;
        Round = "round", 0, 1;
        ToFixed = "toFixed", 1, 1;
        Filter = "filter", 1, 1;
        Flat = "flat", 0, 0;
        Join = "join", 1, 1;
        Map = "map", 1, 1;
        Reduce = "reduce", 2, 2;
        Sort = "sort", 0, 0;
        Unique = "unique", 0, 0;
        Mean = "mean", 0, 0;
        Median = "median", 0, 0;
        Stddev = "stddev", 0, 0;
        AsFile = "asFile", 0, 0;
        LinksTo = "linksTo", 1, 1;
        AsLink = "asLink", 0, 1;
        HasLink = "hasLink", 1, 1;
        HasProperty = "hasProperty", 1, 1;
        HasTag = "hasTag", 1, usize::MAX;
        InFolder = "inFolder", 1, 1;
        Keys = "keys", 0, 0;
        Values = "values", 0, 0;
        Matches = "matches", 1, 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Local {
    Value,
    Index,
    Acc,
    Values,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Node {
    Literal(Typed),
    List(Vec<Node>),
    Object(Vec<(String, Node)>),
    Property(String),
    Note,
    File,
    FileField(FileField),
    This,
    ThisProperty(String),
    Formula(String),
    Local(Local),
    Not(Box<Node>),
    Negate(Box<Node>),
    And(Box<Node>, Box<Node>),
    Or(Box<Node>, Box<Node>),
    Compare(Compare, Box<Node>, Box<Node>),
    Arith(Arith, Box<Node>, Box<Node>),
    Member(Box<Node>, String),
    Index(Box<Node>, Box<Node>),
    Call(Global, Vec<Node>),
    Method(Box<Node>, Method, Vec<Node>),
}

impl Node {
    pub(crate) fn visit(&self, see: &mut dyn FnMut(&Node)) {
        see(self);
        match self {
            Node::List(items) | Node::Call(_, items) => items.iter().for_each(|n| n.visit(see)),
            Node::Object(entries) => entries.iter().for_each(|(_, n)| n.visit(see)),
            Node::Not(inner) | Node::Negate(inner) | Node::Member(inner, _) => inner.visit(see),
            Node::And(a, b)
            | Node::Or(a, b)
            | Node::Compare(_, a, b)
            | Node::Arith(_, a, b)
            | Node::Index(a, b) => {
                a.visit(see);
                b.visit(see);
            }
            Node::Method(receiver, _, args) => {
                receiver.visit(see);
                args.iter().for_each(|n| n.visit(see));
            }
            Node::Literal(_)
            | Node::Property(_)
            | Node::Note
            | Node::File
            | Node::FileField(_)
            | Node::This
            | Node::ThisProperty(_)
            | Node::Formula(_)
            | Node::Local(_) => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub(crate) node: Node,
}

impl Expr {
    pub(crate) fn formulas(&self) -> Vec<String> {
        let mut names = Vec::new();
        self.node.visit(&mut |node| {
            if let Node::Formula(name) = node
                && !names.contains(name)
            {
                names.push(name.clone());
            }
        });
        names
    }

    pub(crate) fn this_use(&self) -> Option<String> {
        let mut found = None;
        self.node.visit(&mut |node| {
            if found.is_none() {
                match node {
                    Node::This => found = Some("this".to_string()),
                    Node::ThisProperty(name) => found = Some(format!("this.{name}")),
                    _ => {}
                }
            }
        });
        found
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Text(String),
    Number(f64),
    Ident(String),
    Regex(String, String),
    Op(&'static str),
    Dot,
    Comma,
    Colon,
    Open,
    Close,
    OpenBracket,
    CloseBracket,
    OpenBrace,
    CloseBrace,
}

const OPERATORS: [&str; 15] = [
    "==", "!=", ">=", "<=", "&&", "||", ">", "<", "!", "+", "-", "*", "/", "%", "=",
];

fn syntax(message: impl Into<String>) -> ExprError {
    ExprError::Syntax(message.into())
}

fn lex(source: &str) -> Result<Vec<Token>, ExprError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let value_expected = matches!(
            tokens.last(),
            None | Some(
                Token::Op(_) | Token::Open | Token::Comma | Token::OpenBracket | Token::Colon
            )
        );
        if c.is_whitespace() {
            i += 1;
        } else if c == '"' || c == '\'' {
            let mut text = String::new();
            i += 1;
            loop {
                match chars.get(i) {
                    None => return Err(syntax("unterminated text")),
                    Some('\\') => match chars.get(i + 1) {
                        Some(&escaped) => {
                            text.push(match escaped {
                                'n' => '\n',
                                't' => '\t',
                                other => other,
                            });
                            i += 2;
                        }
                        None => return Err(syntax("unterminated text")),
                    },
                    Some(&q) if q == c => {
                        i += 1;
                        break;
                    }
                    Some(&other) => {
                        text.push(other);
                        i += 1;
                    }
                }
            }
            tokens.push(Token::Text(text));
        } else if c == '/' && value_expected {
            let mut pattern = String::new();
            let mut in_class = false;
            i += 1;
            loop {
                match chars.get(i) {
                    None | Some('\n') => return Err(syntax("unterminated regular expression")),
                    Some('\\') => {
                        let Some(&escaped) = chars.get(i + 1) else {
                            return Err(syntax("unterminated regular expression"));
                        };
                        if escaped != '/' {
                            pattern.push('\\');
                        }
                        pattern.push(escaped);
                        i += 2;
                    }
                    Some('/') if !in_class => {
                        i += 1;
                        break;
                    }
                    Some(&other) => {
                        in_class = match other {
                            '[' => true,
                            ']' => false,
                            _ => in_class,
                        };
                        pattern.push(other);
                        i += 1;
                    }
                }
            }
            let start = i;
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                i += 1;
            }
            tokens.push(Token::Regex(pattern, chars[start..i].iter().collect()));
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_digit()
                    || (chars[i] == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)))
            {
                i += 1;
            }
            if matches!(chars.get(i), Some('e' | 'E')) {
                let sign = usize::from(matches!(chars.get(i + 1), Some('+' | '-')));
                if chars.get(i + 1 + sign).is_some_and(char::is_ascii_digit) {
                    i += 1 + sign;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let literal: String = chars[start..i].iter().collect();
            let number = literal
                .parse()
                .map_err(|_| syntax(format!("bad number {literal}")))?;
            tokens.push(Token::Number(number));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(Token::Ident(chars[start..i].iter().collect()));
        } else {
            i += 1;
            match c {
                '.' => tokens.push(Token::Dot),
                ',' => tokens.push(Token::Comma),
                ':' => tokens.push(Token::Colon),
                '(' => tokens.push(Token::Open),
                ')' => tokens.push(Token::Close),
                '[' => tokens.push(Token::OpenBracket),
                ']' => tokens.push(Token::CloseBracket),
                '{' => tokens.push(Token::OpenBrace),
                '}' => tokens.push(Token::CloseBrace),
                _ => {
                    let two: String = chars[i - 1..(i + 1).min(chars.len())].iter().collect();
                    let op = OPERATORS.iter().find(|op| two.starts_with(**op)).copied();
                    match op {
                        Some(op) => {
                            i += op.len() - 1;
                            tokens.push(Token::Op(op));
                        }
                        None => return Err(syntax(format!("unexpected character {c}"))),
                    }
                }
            }
        }
    }
    Ok(tokens)
}

fn pattern(source: String, flags: String) -> Result<Pattern, ExprError> {
    let mut builder = regex::RegexBuilder::new(&source);
    for flag in flags.chars() {
        match flag {
            'i' => builder.case_insensitive(true),
            'm' => builder.multi_line(true),
            's' => builder.dot_matches_new_line(true),
            'g' | 'u' => &mut builder,
            other => {
                return Err(syntax(format!(
                    "regular expression /{source}/{flags}: flag {other} is not supported"
                )));
            }
        };
    }
    let regex = builder
        .build()
        .map_err(|error| syntax(format!("regular expression /{source}/{flags}: {error}")))?;
    Ok(Pattern {
        source,
        flags,
        regex,
    })
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
    lambdas: Vec<bool>,
    summary: bool,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn eat(&mut self, token: &Token) -> bool {
        if self.peek() == Some(token) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn eat_op(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Token::Op(found)) if *found == op) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn deeper(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(syntax("the expression is nested too deeply"));
        }
        Ok(())
    }

    fn nested<T>(
        &mut self,
        read: impl FnOnce(&mut Parser) -> Result<T, ExprError>,
    ) -> Result<T, ExprError> {
        self.deeper()?;
        let result = read(self);
        self.depth -= 1;
        result
    }

    fn chain<Op>(
        &mut self,
        operand: fn(&mut Parser) -> Result<Node, ExprError>,
        link: fn(Option<&Token>) -> Result<Option<Op>, ExprError>,
        join: fn(Op, Box<Node>, Box<Node>) -> Node,
    ) -> Result<Node, ExprError> {
        let base = self.depth;
        let mut left = operand(self)?;
        while let Some(op) = link(self.peek())? {
            self.at += 1;
            self.deeper()?;
            left = join(op, Box::new(left), Box::new(operand(self)?));
        }
        self.depth = base;
        Ok(left)
    }

    fn or(&mut self) -> Result<Node, ExprError> {
        self.chain(
            Parser::and,
            |token| Ok(matches!(token, Some(Token::Op("||"))).then_some(())),
            |(), a, b| Node::Or(a, b),
        )
    }

    fn and(&mut self) -> Result<Node, ExprError> {
        self.chain(
            Parser::equality,
            |token| Ok(matches!(token, Some(Token::Op("&&"))).then_some(())),
            |(), a, b| Node::And(a, b),
        )
    }

    fn equality(&mut self) -> Result<Node, ExprError> {
        self.chain(
            Parser::relation,
            |token| match token {
                Some(Token::Op("==")) => Ok(Some(Compare::Eq)),
                Some(Token::Op("!=")) => Ok(Some(Compare::Ne)),
                Some(Token::Op("=")) => Err(syntax("= is not a comparison, write ==")),
                _ => Ok(None),
            },
            Node::Compare,
        )
    }

    fn relation(&mut self) -> Result<Node, ExprError> {
        self.chain(
            Parser::sum,
            |token| {
                Ok(match token {
                    Some(Token::Op(">")) => Some(Compare::Gt),
                    Some(Token::Op("<")) => Some(Compare::Lt),
                    Some(Token::Op(">=")) => Some(Compare::Ge),
                    Some(Token::Op("<=")) => Some(Compare::Le),
                    _ => None,
                })
            },
            Node::Compare,
        )
    }

    fn sum(&mut self) -> Result<Node, ExprError> {
        self.chain(
            Parser::product,
            |token| {
                Ok(match token {
                    Some(Token::Op("+")) => Some(Arith::Add),
                    Some(Token::Op("-")) => Some(Arith::Sub),
                    _ => None,
                })
            },
            Node::Arith,
        )
    }

    fn product(&mut self) -> Result<Node, ExprError> {
        self.chain(
            Parser::unary,
            |token| {
                Ok(match token {
                    Some(Token::Op("*")) => Some(Arith::Mul),
                    Some(Token::Op("/")) => Some(Arith::Div),
                    Some(Token::Op("%")) => Some(Arith::Rem),
                    _ => None,
                })
            },
            Node::Arith,
        )
    }

    fn unary(&mut self) -> Result<Node, ExprError> {
        if self.eat_op("!") {
            return self.nested(|p| Ok(Node::Not(Box::new(p.unary()?))));
        }
        if self.eat_op("-") {
            return self.nested(|p| {
                Ok(match p.unary()? {
                    Node::Literal(Typed::Number(n)) => Node::Literal(Typed::Number(-n)),
                    other => Node::Negate(Box::new(other)),
                })
            });
        }
        self.postfix()
    }

    fn arguments(&mut self, lambda: Option<bool>) -> Result<Vec<Node>, ExprError> {
        let mut args = Vec::new();
        if self.eat(&Token::Close) {
            return Ok(args);
        }
        loop {
            let bound = lambda.filter(|_| args.is_empty());
            if let Some(acc) = bound {
                self.lambdas.push(acc);
            }
            let arg = self.nested(Parser::or);
            if bound.is_some() {
                self.lambdas.pop();
            }
            args.push(arg?);
            if self.eat(&Token::Close) {
                return Ok(args);
            }
            if !self.eat(&Token::Comma) {
                return Err(syntax("expected , or )"));
            }
        }
    }

    fn ident(&mut self) -> Result<String, ExprError> {
        match self.next() {
            Some(Token::Ident(name)) => Ok(name),
            _ => Err(syntax("expected a name after .")),
        }
    }

    fn bracket(&mut self) -> Result<Node, ExprError> {
        let inner = self.nested(Parser::or)?;
        if !self.eat(&Token::CloseBracket) {
            return Err(syntax("missing ]"));
        }
        Ok(inner)
    }

    fn row_only(&self, name: &str) -> Result<(), ExprError> {
        if self.summary {
            return Err(ExprError::Unsupported(Feature::Property, name.to_string()));
        }
        Ok(())
    }

    fn primary(&mut self) -> Result<Node, ExprError> {
        match self.next() {
            Some(Token::Text(text)) => Ok(Node::Literal(Typed::Text(text))),
            Some(Token::Number(n)) => Ok(Node::Literal(Typed::Number(n))),
            Some(Token::Regex(source, flags)) => {
                Ok(Node::Literal(Typed::Regex(pattern(source, flags)?)))
            }
            Some(Token::Open) => {
                let inner = self.nested(Parser::or)?;
                if !self.eat(&Token::Close) {
                    return Err(syntax("missing )"));
                }
                Ok(inner)
            }
            Some(Token::OpenBracket) => {
                let mut items = Vec::new();
                if self.eat(&Token::CloseBracket) {
                    return Ok(Node::List(items));
                }
                loop {
                    items.push(self.nested(Parser::or)?);
                    if self.eat(&Token::CloseBracket) {
                        return Ok(Node::List(items));
                    }
                    if !self.eat(&Token::Comma) {
                        return Err(syntax("expected , or ]"));
                    }
                }
            }
            Some(Token::OpenBrace) => {
                let mut entries = Vec::new();
                if self.eat(&Token::CloseBrace) {
                    return Ok(Node::Object(entries));
                }
                loop {
                    let key = match self.next() {
                        Some(Token::Text(key) | Token::Ident(key)) => key,
                        _ => return Err(syntax("expected a key")),
                    };
                    if !self.eat(&Token::Colon) {
                        return Err(syntax("expected : after a key"));
                    }
                    entries.push((key, self.nested(Parser::or)?));
                    if self.eat(&Token::CloseBrace) {
                        return Ok(Node::Object(entries));
                    }
                    if !self.eat(&Token::Comma) {
                        return Err(syntax("expected , or }"));
                    }
                }
            }
            Some(Token::Ident(name)) => self.name(name),
            Some(_) => Err(syntax("unexpected token")),
            None => Err(syntax("unexpected end")),
        }
    }

    fn name(&mut self, name: String) -> Result<Node, ExprError> {
        match name.as_str() {
            "true" => return Ok(Node::Literal(Typed::Bool(true))),
            "false" => return Ok(Node::Literal(Typed::Bool(false))),
            "null" => return Ok(Node::Literal(Typed::Null)),
            "value" | "index" if !self.lambdas.is_empty() => {
                return Ok(Node::Local(if name == "value" {
                    Local::Value
                } else {
                    Local::Index
                }));
            }
            "acc" if self.lambdas.contains(&true) => return Ok(Node::Local(Local::Acc)),
            "values" if self.summary => return Ok(Node::Local(Local::Values)),
            _ => {}
        }
        if self.peek() == Some(&Token::Open) {
            self.at += 1;
            let Some((global, least, most)) = Global::parse(&name) else {
                return Err(ExprError::Unsupported(Feature::Function, name));
            };
            let args = self.arguments(None)?;
            if args.len() < least || args.len() > most {
                return Err(syntax(arity(&format!("{name}()"), least, most)));
            }
            return Ok(Node::Call(global, args));
        }
        match name.as_str() {
            "file" => {
                self.row_only("file")?;
                if self.peek() != Some(&Token::Dot) {
                    return Ok(Node::File);
                }
                if matches!(self.tokens.get(self.at + 2), Some(Token::Open)) {
                    return Ok(Node::File);
                }
                self.at += 1;
                let member = self.ident()?;
                FileField::parse(&member)
                    .map(Node::FileField)
                    .ok_or_else(|| {
                        ExprError::Unsupported(Feature::Property, format!("file.{member}"))
                    })
            }
            "note" => {
                self.row_only("note")?;
                if self.eat(&Token::Dot) {
                    return Ok(Node::Property(self.ident()?));
                }
                if self.eat(&Token::OpenBracket) {
                    return Ok(match self.bracket()? {
                        Node::Literal(Typed::Text(key)) => Node::Property(key),
                        key => Node::Index(Box::new(Node::Note), Box::new(key)),
                    });
                }
                Ok(Node::Note)
            }
            "formula" => {
                self.row_only("formula")?;
                if self.eat(&Token::Dot) {
                    return Ok(Node::Formula(self.ident()?));
                }
                if self.eat(&Token::OpenBracket) {
                    return match self.bracket()? {
                        Node::Literal(Typed::Text(key)) => Ok(Node::Formula(key)),
                        _ => Err(ExprError::Unsupported(
                            Feature::Index,
                            "formula[...]".to_string(),
                        )),
                    };
                }
                Err(ExprError::Unsupported(
                    Feature::Formula,
                    "formula".to_string(),
                ))
            }
            "this" => {
                if self.peek() == Some(&Token::Dot)
                    && !matches!(self.tokens.get(self.at + 2), Some(Token::Open))
                {
                    self.at += 1;
                    let member = self.ident()?;
                    return Ok(if member == "file" {
                        Node::This
                    } else {
                        Node::ThisProperty(member)
                    });
                }
                if self.eat(&Token::OpenBracket) {
                    return Ok(match self.bracket()? {
                        Node::Literal(Typed::Text(key)) => Node::ThisProperty(key),
                        key => Node::Index(Box::new(Node::This), Box::new(key)),
                    });
                }
                Ok(Node::This)
            }
            _ => {
                self.row_only(&name)?;
                Ok(Node::Property(name))
            }
        }
    }

    fn postfix(&mut self) -> Result<Node, ExprError> {
        let base = self.depth;
        let mut node = self.primary()?;
        loop {
            if !matches!(self.peek(), Some(Token::Dot | Token::OpenBracket)) {
                self.depth = base;
                return Ok(node);
            }
            self.deeper()?;
            if self.eat(&Token::Dot) {
                let name = self.ident()?;
                if !self.eat(&Token::Open) {
                    node = Node::Member(Box::new(node), name);
                    continue;
                }
                let Some((method, least, most)) = Method::parse(&name) else {
                    return Err(if node == Node::File {
                        ExprError::Unsupported(Feature::Function, format!("file.{name}"))
                    } else {
                        ExprError::Unsupported(Feature::Method, name)
                    });
                };
                let lambda = match method {
                    Method::Filter | Method::Map => Some(false),
                    Method::Reduce => Some(true),
                    _ => None,
                };
                let args = self.arguments(lambda)?;
                if args.len() < least || args.len() > most {
                    return Err(syntax(arity(&format!(".{name}()"), least, most)));
                }
                node = Node::Method(Box::new(node), method, args);
            } else {
                self.at += 1;
                node = Node::Index(Box::new(node), Box::new(self.bracket()?));
            }
        }
    }
}

fn arity(name: &str, least: usize, most: usize) -> String {
    if least == most {
        format!("{name} takes {least} argument(s)")
    } else if most == usize::MAX {
        format!("{name} takes at least {least} argument(s)")
    } else {
        format!("{name} takes {least} to {most} arguments")
    }
}

fn read(source: &str, summary: bool) -> Result<Expr, ExprError> {
    let mut parser = Parser {
        tokens: lex(source)?,
        at: 0,
        depth: 0,
        lambdas: Vec::new(),
        summary,
    };
    if parser.tokens.is_empty() {
        return Err(syntax("empty expression"));
    }
    let node = parser.or()?;
    match parser.peek() {
        None => Ok(Expr { node }),
        Some(_) => Err(syntax("unexpected token after the expression")),
    }
}

pub fn compile(source: &str) -> Result<Expr, ExprError> {
    read(source, false)
}

pub fn compile_summary(source: &str) -> Result<Expr, ExprError> {
    read(source, true)
}
