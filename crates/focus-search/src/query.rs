use crate::{Diagnostic, DiagnosticKind, Field, Pattern, Query, QueryNode};
use std::ops::Range;

#[derive(Debug, Clone)]
enum Tok {
    LParen,
    RParen,
    Or,
    Not,
    Term(Field, Pattern),
}

fn field_named(name: &str) -> Option<Field> {
    Some(match name.to_ascii_lowercase().as_str() {
        "content" => Field::Content,
        "title" => Field::Title,
        "heading" | "section" => Field::Heading,
        "tag" => Field::Tag,
        "path" => Field::Path,
        "file" => Field::File,
        "task" => Field::Task { done: None },
        "task-todo" => Field::Task { done: Some(false) },
        "task-done" => Field::Task { done: Some(true) },
        "prop" => Field::Prop,
        _ => return None,
    })
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn phrase(&mut self) -> String {
        let start = self.pos;
        self.pos += 1;
        let rest = &self.src[self.pos..];
        let mut out = String::new();
        let mut chars = rest.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '\\' => {
                    if let Some((_, n)) = chars.next() {
                        out.push(n);
                    }
                }
                '"' => {
                    self.pos += i + 1;
                    return out;
                }
                _ => out.push(c),
            }
        }
        self.pos = self.src.len();
        self.diagnostics.push(Diagnostic {
            kind: DiagnosticKind::UnclosedQuote,
            range: start..self.src.len(),
        });
        out
    }

    fn regex(&mut self) -> Option<String> {
        let rest = &self.src[self.pos + 1..];
        let mut escaped = false;
        for (i, c) in rest.char_indices() {
            match c {
                '\\' if !escaped => escaped = true,
                '/' if !escaped => {
                    let after = rest[i + 1..].chars().next();
                    if i == 0 || after.is_some_and(|c| !c.is_whitespace() && c != ')') {
                        return None;
                    }
                    let body = rest[..i].to_string();
                    let start = self.pos;
                    self.pos += 1 + i + 1;
                    if let Err(e) = regex::Regex::new(&body) {
                        self.diagnostics.push(Diagnostic {
                            kind: DiagnosticKind::InvalidRegex {
                                message: first_line(&e.to_string()),
                            },
                            range: start..self.pos,
                        });
                    }
                    return Some(body);
                }
                _ => escaped = false,
            }
        }
        None
    }

    fn word(&mut self) -> &'a str {
        let start = self.pos;
        let rest = &self.src[start..];
        let end = rest
            .find(|c: char| c.is_whitespace() || c == ')' || c == '(')
            .unwrap_or(rest.len());
        self.pos += end;
        &self.src[start..start + end]
    }

    fn value(&mut self) -> Pattern {
        match self.peek() {
            Some('"') => Pattern::Phrase(self.phrase()),
            Some('/') => match self.regex() {
                Some(r) => Pattern::Regex(r),
                None => Pattern::Text(self.word().to_string()),
            },
            _ => Pattern::Text(self.word().to_string()),
        }
    }

    fn tokens(&mut self) -> Vec<(Tok, Range<usize>)> {
        let mut out = Vec::new();
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.pos += self.peek().map_or(1, char::len_utf8);
            }
            let Some(c) = self.peek() else { break };
            let start = self.pos;
            let before = out.len();
            self.token(c, &mut out);
            for t in &mut out[before..] {
                t.1 = start..self.pos;
            }
        }
        out
    }

    fn token(&mut self, c: char, out: &mut Vec<(Tok, Range<usize>)>) {
        let mut push = |t: Tok| out.push((t, 0..0));
        {
            match c {
                '(' => {
                    self.pos += 1;
                    push(Tok::LParen);
                }
                ')' => {
                    self.pos += 1;
                    push(Tok::RParen);
                }
                '-' if self.src[self.pos + 1..].starts_with(|n: char| !n.is_whitespace()) => {
                    self.pos += 1;
                    push(Tok::Not);
                }
                '"' => {
                    let p = self.phrase();
                    push(Tok::Term(Field::Any, Pattern::Phrase(p)));
                }
                '/' => match self.regex() {
                    Some(r) => push(Tok::Term(Field::Any, Pattern::Regex(r))),
                    None => {
                        let w = self.word();
                        push(Tok::Term(Field::Any, Pattern::Text(w.to_string())));
                    }
                },
                _ => {
                    let start = self.pos;
                    let rest = &self.src[start..];
                    let colon = rest.find(':').filter(|&i| {
                        !rest[..i].contains(|c: char| c.is_whitespace() || c == '(' || c == ')')
                    });
                    if let Some(i) = colon
                        && let Some(field) = field_named(&rest[..i])
                    {
                        self.pos += i + 1;
                        if self.peek().is_none_or(|c| c.is_whitespace() || c == ')') {
                            self.diagnostics.push(Diagnostic {
                                kind: DiagnosticKind::MissingValue {
                                    operator: rest[..i].to_string(),
                                },
                                range: start..self.pos,
                            });
                            return;
                        }
                        let value = self.value();
                        let pattern = match (field, value) {
                            (Field::Prop, Pattern::Text(t)) => match t.split_once('=') {
                                Some((k, v)) => Pattern::KeyValue(k.to_string(), v.to_string()),
                                None => Pattern::Text(t),
                            },
                            (Field::Tag, Pattern::Text(t)) => {
                                Pattern::Text(t.trim_start_matches('#').to_string())
                            }
                            (_, v) => v,
                        };
                        push(Tok::Term(field, pattern));
                        return;
                    }
                    if let Some(i) = colon
                        && looks_like_operator(&rest[..i], &rest[i + 1..])
                    {
                        self.diagnostics.push(Diagnostic {
                            kind: DiagnosticKind::UnknownOperator {
                                name: rest[..i].to_string(),
                            },
                            range: start..start + i + 1,
                        });
                    }
                    let w = self.word();
                    if w == "OR" {
                        push(Tok::Or);
                    } else if !w.is_empty() {
                        push(Tok::Term(Field::Any, Pattern::Text(w.to_string())));
                    } else {
                        self.pos += c.len_utf8();
                    }
                }
            }
        }
    }
}

fn looks_like_operator(name: &str, value: &str) -> bool {
    name.len() >= 2
        && name.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
        && !value.is_empty()
        && !value.starts_with("//")
        && !value.starts_with(char::is_whitespace)
}

struct Parser {
    toks: Vec<(Tok, Range<usize>)>,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    fn or(&mut self) -> QueryNode {
        let mut alts = vec![self.and()];
        while matches!(self.toks.get(self.pos), Some((Tok::Or, _))) {
            self.pos += 1;
            alts.push(self.and());
        }
        let alts: Vec<_> = alts
            .into_iter()
            .filter(|n| !matches!(n, QueryNode::Empty))
            .collect();
        match alts.len() {
            0 => QueryNode::Empty,
            1 => alts.into_iter().next().unwrap_or(QueryNode::Empty),
            _ => QueryNode::Or(alts),
        }
    }

    fn and(&mut self) -> QueryNode {
        let mut items = Vec::new();
        while let Some((t, _)) = self.toks.get(self.pos) {
            if matches!(t, Tok::Or | Tok::RParen) {
                break;
            }
            let n = self.unary();
            if !matches!(n, QueryNode::Empty) {
                items.push(n);
            }
        }
        match items.len() {
            0 => QueryNode::Empty,
            1 => items.pop().unwrap_or(QueryNode::Empty),
            _ => QueryNode::And(items),
        }
    }

    fn unary(&mut self) -> QueryNode {
        match self.toks.get(self.pos).cloned() {
            Some((Tok::Not, _)) => {
                self.pos += 1;
                match self.unary() {
                    QueryNode::Empty => QueryNode::Empty,
                    n => QueryNode::Not(Box::new(n)),
                }
            }
            Some((Tok::LParen, range)) => {
                self.pos += 1;
                let n = self.or();
                if matches!(self.toks.get(self.pos), Some((Tok::RParen, _))) {
                    self.pos += 1;
                } else {
                    self.diagnostics.push(Diagnostic {
                        kind: DiagnosticKind::UnclosedParenthesis,
                        range,
                    });
                }
                n
            }
            Some((Tok::Term(field, pattern), _)) => {
                self.pos += 1;
                let empty = match &pattern {
                    Pattern::Text(t) | Pattern::Phrase(t) | Pattern::Regex(t) => t.is_empty(),
                    Pattern::KeyValue(k, _) => k.is_empty(),
                };
                if empty {
                    QueryNode::Empty
                } else {
                    QueryNode::Term { field, pattern }
                }
            }
            Some(_) => {
                self.pos += 1;
                QueryNode::Empty
            }
            None => QueryNode::Empty,
        }
    }
}

fn first_line(s: &str) -> String {
    s.lines()
        .rfind(|l| l.trim_start().starts_with("error:"))
        .map(|l| l.trim().trim_start_matches("error:").trim().to_string())
        .unwrap_or_else(|| s.lines().next().unwrap_or("").to_string())
}

pub(crate) fn parse(input: &str) -> Query {
    let mut lexer = Lexer {
        src: input,
        pos: 0,
        diagnostics: Vec::new(),
    };
    let toks = lexer.tokens();
    let mut parser = Parser {
        toks,
        pos: 0,
        diagnostics: lexer.diagnostics,
    };
    let mut root = parser.or();
    while parser.pos < parser.toks.len() {
        if let Some((Tok::RParen, range)) = parser.toks.get(parser.pos) {
            parser.diagnostics.push(Diagnostic {
                kind: DiagnosticKind::UnmatchedParenthesis,
                range: range.clone(),
            });
        }
        parser.pos += 1;
        let more = parser.or();
        root = match (root, more) {
            (QueryNode::Empty, m) => m,
            (r, QueryNode::Empty) => r,
            (QueryNode::And(mut v), m) => {
                v.push(m);
                QueryNode::And(v)
            }
            (r, m) => QueryNode::And(vec![r, m]),
        };
    }
    let mut diagnostics = parser.diagnostics;
    diagnostics.sort_by_key(|d| d.range.start);
    Query {
        source: input.to_string(),
        root,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(n: &QueryNode) -> String {
        match n {
            QueryNode::And(v) => {
                format!("(and {})", v.iter().map(show).collect::<Vec<_>>().join(" "))
            }
            QueryNode::Or(v) => {
                format!("(or {})", v.iter().map(show).collect::<Vec<_>>().join(" "))
            }
            QueryNode::Not(n) => format!("(not {})", show(n)),
            QueryNode::Term { field, pattern } => {
                let f = match field {
                    Field::Any => String::new(),
                    f => format!("{f:?}:"),
                };
                match pattern {
                    Pattern::Text(t) => format!("{f}{t}"),
                    Pattern::Phrase(p) => format!("{f}\"{p}\""),
                    Pattern::Regex(r) => format!("{f}/{r}/"),
                    Pattern::KeyValue(k, v) => format!("{f}{k}={v}"),
                }
            }
            QueryNode::Empty => "∅".into(),
        }
    }

    fn p(s: &str) -> String {
        show(&parse(s).root)
    }

    #[test]
    fn grammar() {
        assert_eq!(p("kerberos"), "kerberos");
        assert_eq!(p("a b"), "(and a b)");
        assert_eq!(p("a b OR c"), "(or (and a b) c)");
        assert_eq!(p("a (b OR c) -d"), "(and a (or b c) (not d))");
        assert_eq!(
            p("\"golden ticket\" -\"silver\""),
            "(and \"golden ticket\" (not \"silver\"))"
        );
        assert_eq!(p("/ker+beros?/ x"), "(and /ker+beros?/ x)");
        assert_eq!(p("/etc/passwd"), "/etc/passwd");
        assert_eq!(
            p("tag:#red/ad path:Red_Team file:index title:Kerb heading:Outils"),
            "(and Tag:red/ad Path:Red_Team File:index Title:Kerb Heading:Outils)"
        );
        assert_eq!(
            p("task-todo:nmap task-done:x task:y"),
            "(and Task { done: Some(false) }:nmap Task { done: Some(true) }:x Task { done: None }:y)"
        );
        assert_eq!(
            p("prop:status=ready prop:type"),
            "(and Prop:status=ready Prop:type)"
        );
        assert_eq!(p("path:\"Red Team\""), "Path:\"Red Team\"");
        assert_eq!(p("http://x.org a-b"), "(and http://x.org a-b)");
        assert_eq!(p(""), "∅");
        assert_eq!(p("  - OR )"), "-");
    }

    #[test]
    fn diagnostics() {
        let kinds = |s: &str| {
            parse(s)
                .diagnostics
                .into_iter()
                .map(|d| (d.kind, d.range))
                .collect::<Vec<_>>()
        };
        let q = parse("x \"open");
        assert_eq!(
            q.diagnostics,
            [Diagnostic {
                kind: DiagnosticKind::UnclosedQuote,
                range: 2..7
            }]
        );
        assert_eq!(show(&q.root), "(and x \"open\")");
        let q = parse("/a(b/ x");
        assert_eq!(q.diagnostics.len(), 1);
        assert_eq!(q.diagnostics[0].range, 0..5);
        assert!(
            matches!(&q.diagnostics[0].kind, DiagnosticKind::InvalidRegex { message } if !message.is_empty())
        );
        assert_eq!(
            kinds("tagg:red x"),
            [(
                DiagnosticKind::UnknownOperator {
                    name: "tagg".into()
                },
                0..5
            )]
        );
        assert_eq!(
            kinds("x tag: y"),
            [(
                DiagnosticKind::MissingValue {
                    operator: "tag".into()
                },
                2..6
            )]
        );
        assert_eq!(kinds("(a b"), [(DiagnosticKind::UnclosedParenthesis, 0..1)]);
        assert_eq!(
            kinds("a) b"),
            [(DiagnosticKind::UnmatchedParenthesis, 1..2)]
        );
        assert!(kinds("https://x.org 12:30 C:\\x").is_empty());
    }
}
