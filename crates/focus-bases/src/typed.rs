use std::borrow::Cow;
use std::cmp::Ordering;
use std::sync::Arc;

use crate::date::{Date, Duration};

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub target: String,
    pub anchor: Option<String>,
    pub display: Option<Box<Typed>>,
    pub file: Option<Arc<str>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Url {
    pub address: String,
    pub display: Option<Box<Typed>>,
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub source: String,
    pub flags: String,
    pub(crate) regex: regex::Regex,
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Pattern) -> bool {
        self.source == other.source && self.flags == other.flags
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Typed {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Date(Date),
    Duration(Duration),
    List(Vec<Typed>),
    Object(Vec<(String, Typed)>),
    Link(Link),
    Url(Url),
    File(Arc<str>),
    Tag(String),
    Regex(Pattern),
    Html(String),
    Icon(String),
    Image(String),
}

pub(crate) fn split_link(text: &str) -> (&str, Option<&str>, Option<&str>) {
    let text = text.trim();
    let (inner, display) = match text
        .strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]"))
    {
        Some(inner) => match inner.split_once('|') {
            Some((target, display)) => (target, Some(display)),
            None => (inner, None),
        },
        None => (text, None),
    };
    match inner.split_once('#') {
        Some((target, anchor)) => (target.trim(), Some(anchor), display),
        None => (inner.trim(), None, display),
    }
}

pub(crate) fn is_wikilink(text: &str) -> bool {
    text.strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]"))
        .is_some_and(|inner| !inner.is_empty() && !inner.contains("]]") && !inner.contains("[["))
}

pub(crate) fn is_url(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once("://") else {
        return false;
    };
    scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        && !rest.is_empty()
}

pub(crate) fn tag_name(text: &str) -> &str {
    text.trim().trim_start_matches('#')
}

pub(crate) fn tag_within(tag: &str, wanted: &str) -> bool {
    let tag = tag_name(tag).to_lowercase();
    let wanted = tag_name(wanted).to_lowercase();
    tag == wanted
        || tag
            .strip_prefix(&wanted)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub(crate) fn parse_number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty()
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'))
    {
        return None;
    }
    text.parse().ok()
}

pub(crate) fn number_text(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_string()
    } else if n.is_infinite() {
        if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
    } else if n == 0.0 {
        "0".to_string()
    } else {
        n.to_string()
    }
}

fn text_order(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

impl Link {
    pub fn label(&self) -> &str {
        match self.display.as_deref() {
            Some(Typed::Text(text)) => text,
            _ => &self.target,
        }
    }

    pub fn wikilink(&self) -> String {
        let mut out = format!("[[{}", self.target);
        if let Some(anchor) = &self.anchor {
            out.push('#');
            out.push_str(anchor);
        }
        if let Some(Typed::Text(display)) = self.display.as_deref() {
            out.push('|');
            out.push_str(display);
        }
        out.push_str("]]");
        out
    }

    fn same(&self, other: &Link) -> bool {
        match (&self.file, &other.file) {
            (Some(a), Some(b)) => a == b,
            (None, None) => self.target == other.target,
            _ => false,
        }
    }

    fn names(&self, text: &str) -> bool {
        let (target, _, _) = split_link(text);
        if target == self.target {
            return true;
        }
        self.file.as_deref().is_some_and(|path| {
            let name = path.rsplit('/').next().unwrap_or(path);
            [path, name]
                .into_iter()
                .any(|form| form == target || form.strip_suffix(".md") == Some(target))
        })
    }
}

impl Typed {
    pub fn type_name(&self) -> &'static str {
        match self {
            Typed::Null => "null",
            Typed::Bool(_) => "boolean",
            Typed::Number(_) => "number",
            Typed::Text(_) => "string",
            Typed::Date(_) => "date",
            Typed::Duration(_) => "duration",
            Typed::List(_) => "list",
            Typed::Object(_) => "object",
            Typed::Link(_) => "link",
            Typed::Url(_) => "url",
            Typed::File(_) => "file",
            Typed::Tag(_) => "tag",
            Typed::Regex(_) => "regexp",
            Typed::Html(_) => "html",
            Typed::Icon(_) => "icon",
            Typed::Image(_) => "image",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Typed::Null => false,
            Typed::Bool(b) => *b,
            Typed::Number(n) => *n != 0.0 && !n.is_nan(),
            Typed::Text(s) | Typed::Tag(s) | Typed::Html(s) | Typed::Icon(s) | Typed::Image(s) => {
                !s.is_empty()
            }
            Typed::Url(url) => !url.address.is_empty(),
            Typed::Duration(d) => d.total_millis() != 0.0,
            Typed::List(items) => !items.is_empty(),
            Typed::Object(entries) => !entries.is_empty(),
            Typed::Date(_) | Typed::Link(_) | Typed::File(_) | Typed::Regex(_) => true,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Typed::Null => true,
            Typed::Number(n) => n.is_nan(),
            Typed::Text(s) | Typed::Tag(s) | Typed::Html(s) | Typed::Icon(s) | Typed::Image(s) => {
                s.is_empty()
            }
            Typed::Url(url) => url.address.is_empty(),
            Typed::List(items) => items.is_empty(),
            Typed::Object(entries) => entries.is_empty(),
            _ => false,
        }
    }

    pub(crate) fn as_str(&self) -> Option<Cow<'_, str>> {
        match self {
            Typed::Text(s) => Some(Cow::Borrowed(s)),
            Typed::Url(url) => Some(Cow::Borrowed(&url.address)),
            Typed::Tag(tag) => Some(Cow::Owned(format!("#{tag}"))),
            _ => None,
        }
    }

    pub fn text(&self, now: Date) -> String {
        match self {
            Typed::Null => String::new(),
            Typed::Bool(b) => b.to_string(),
            Typed::Number(n) => number_text(*n),
            Typed::Text(s) | Typed::Html(s) | Typed::Icon(s) | Typed::Image(s) => s.clone(),
            Typed::Date(date) => date.text(now),
            Typed::Duration(duration) => duration.text(),
            Typed::List(items) => items
                .iter()
                .map(|item| item.text(now))
                .collect::<Vec<_>>()
                .join(", "),
            Typed::Object(entries) => entries
                .iter()
                .map(|(key, value)| format!("{key}: {}", value.text(now)))
                .collect::<Vec<_>>()
                .join(", "),
            Typed::Link(link) => link.label().to_string(),
            Typed::Url(url) => url.address.clone(),
            Typed::File(path) => path.to_string(),
            Typed::Tag(tag) => format!("#{tag}"),
            Typed::Regex(pattern) => format!("/{}/{}", pattern.source, pattern.flags),
        }
    }

    pub fn equals(&self, other: &Typed) -> bool {
        use Typed::*;
        match (self, other) {
            (Null, Null) => true,
            (Bool(a), Bool(b)) => a == b,
            (Number(a), Number(b)) => a == b,
            (Text(a), Text(b)) => a == b,
            (Number(n), Text(s)) | (Text(s), Number(n)) => parse_number(s) == Some(*n),
            (Date(a), Date(b)) => a.same(*b),
            (Date(d), Text(s)) | (Text(s), Date(d)) => {
                crate::date::Date::parse(s).is_some_and(|other| d.same(other))
            }
            (Duration(a), Duration(b)) => a.total_millis() == b.total_millis(),
            (List(a), List(b)) => a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.equals(y)),
            (Object(a), Object(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(key, value)| {
                        b.iter()
                            .any(|(other, found)| other == key && value.equals(found))
                    })
            }
            (Link(a), Link(b)) => a.same(b),
            (Link(link), File(path)) | (File(path), Link(link)) => link.file.as_ref() == Some(path),
            (Link(link), Text(s)) | (Text(s), Link(link)) => link.names(s),
            (File(a), File(b)) => a == b,
            (Tag(a), Tag(b)) => a.to_lowercase() == b.to_lowercase(),
            (Tag(tag), Text(s)) | (Text(s), Tag(tag)) => {
                tag.to_lowercase() == tag_name(s).to_lowercase()
            }
            (Url(a), Url(b)) => a.address == b.address,
            (Url(url), Text(s)) | (Text(s), Url(url)) => &url.address == s,
            (Regex(a), Regex(b)) => a == b,
            (Html(a), Html(b)) | (Icon(a), Icon(b)) | (Image(a), Image(b)) => a == b,
            _ => false,
        }
    }

    pub(crate) fn holds(&self, wanted: &Typed) -> bool {
        match (self, wanted) {
            (Typed::Tag(tag), Typed::Text(s) | Typed::Tag(s)) => tag_within(tag, s),
            _ => self.equals(wanted),
        }
    }

    pub fn compare(&self, other: &Typed) -> Option<Ordering> {
        use Typed::*;
        match (self, other) {
            (Bool(a), Bool(b)) => Some(a.cmp(b)),
            (Number(a), Number(b)) => a.partial_cmp(b),
            (Text(a), Text(b)) => Some(text_order(a, b)),
            (Number(n), Text(s)) => n.partial_cmp(&parse_number(s)?),
            (Text(s), Number(n)) => parse_number(s)?.partial_cmp(n),
            (Date(a), Date(b)) => Some(a.order(*b)),
            (Date(d), Text(s)) => Some(d.order(crate::date::Date::parse(s)?)),
            (Text(s), Date(d)) => Some(crate::date::Date::parse(s)?.order(*d)),
            (Duration(a), Duration(b)) => a.total_millis().partial_cmp(&b.total_millis()),
            (File(a), File(b)) => Some(text_order(a, b)),
            (Tag(_) | Url(_) | Link(_), Tag(_) | Url(_) | Link(_) | Text(_))
            | (Text(_), Tag(_) | Url(_) | Link(_)) => {
                Some(text_order(&self.sort_text(), &other.sort_text()))
            }
            _ => None,
        }
    }

    fn rank(&self) -> u8 {
        match self {
            Typed::Null => 0,
            Typed::Bool(_) => 1,
            Typed::Number(_) => 2,
            Typed::Text(_)
            | Typed::Tag(_)
            | Typed::Url(_)
            | Typed::Link(_)
            | Typed::Html(_)
            | Typed::Icon(_)
            | Typed::Image(_) => 3,
            Typed::Date(_) => 4,
            Typed::Duration(_) => 5,
            Typed::List(_) => 6,
            Typed::Object(_) => 7,
            Typed::File(_) => 8,
            Typed::Regex(_) => 9,
        }
    }

    fn sort_text(&self) -> Cow<'_, str> {
        match self {
            Typed::Text(s) | Typed::Tag(s) | Typed::Html(s) | Typed::Icon(s) | Typed::Image(s) => {
                Cow::Borrowed(s)
            }
            Typed::Link(link) => Cow::Borrowed(link.label()),
            Typed::Url(url) => match url.display.as_deref() {
                Some(Typed::Text(text)) => Cow::Borrowed(text),
                _ => Cow::Borrowed(&url.address),
            },
            other => Cow::Owned(other.text(Date {
                millis: 0,
                time: true,
                relative: false,
            })),
        }
    }

    pub fn sort_order(&self, other: &Typed) -> Ordering {
        self.rank()
            .cmp(&other.rank())
            .then_with(|| match (self, other) {
                (Typed::Bool(a), Typed::Bool(b)) => a.cmp(b),
                (Typed::Number(a), Typed::Number(b)) => a.total_cmp(b),
                (Typed::Date(a), Typed::Date(b)) => a.millis.cmp(&b.millis),
                (Typed::Duration(a), Typed::Duration(b)) => {
                    a.total_millis().total_cmp(&b.total_millis())
                }
                (Typed::List(a), Typed::List(b)) => a
                    .iter()
                    .zip(b)
                    .map(|(x, y)| x.sort_order(y))
                    .find(|order| order.is_ne())
                    .unwrap_or_else(|| a.len().cmp(&b.len())),
                _ => text_order(&self.sort_text(), &other.sort_text()),
            })
    }
}
