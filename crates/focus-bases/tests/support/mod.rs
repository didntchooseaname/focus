#![allow(dead_code)]

use std::sync::Arc;

use focus_bases::{Cell, Date, FileSource, Link, Scope, Typed, compile};

#[derive(Default, Clone)]
pub struct File {
    pub path: &'static str,
    pub props: Vec<(&'static str, Typed)>,
    pub tags: Vec<&'static str>,
    pub links: Vec<&'static str>,
    pub embeds: Vec<&'static str>,
    pub size: u64,
    pub modified: Option<Date>,
}

#[derive(Default)]
pub struct Memory {
    pub files: Vec<File>,
}

impl Memory {
    fn find(&self, path: &str) -> Option<&File> {
        self.files.iter().find(|file| file.path == path)
    }

    fn link(&self, from: &str, target: &str) -> Link {
        Link {
            target: target.to_string(),
            anchor: None,
            display: None,
            file: self.resolve(Some(from), target),
        }
    }
}

fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

impl FileSource for Memory {
    fn files(&self) -> Vec<Arc<str>> {
        self.files.iter().map(|file| Arc::from(file.path)).collect()
    }
    fn note(&self, path: &str) -> Option<u32> {
        self.files
            .iter()
            .position(|file| file.path == path && path.ends_with(".md"))
            .map(|at| at as u32)
    }
    fn property(&self, path: &str, name: &str) -> Option<Typed> {
        self.find(path)?
            .props
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.clone())
    }
    fn properties(&self, path: &str) -> Vec<(String, Typed)> {
        self.find(path).map_or_else(Vec::new, |file| {
            file.props
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone()))
                .collect()
        })
    }
    fn size(&self, path: &str) -> Option<u64> {
        self.find(path).map(|file| file.size)
    }
    fn modified(&self, path: &str) -> Option<Date> {
        self.find(path)?.modified
    }
    fn created(&self, path: &str) -> Option<Date> {
        self.find(path)?.modified
    }
    fn tags(&self, path: &str) -> Vec<String> {
        self.find(path).map_or_else(Vec::new, |file| {
            file.tags.iter().map(|tag| tag.to_string()).collect()
        })
    }
    fn links(&self, path: &str) -> Vec<Link> {
        self.find(path).map_or_else(Vec::new, |file| {
            file.links
                .iter()
                .chain(&file.embeds)
                .map(|target| self.link(path, target))
                .collect()
        })
    }
    fn embeds(&self, path: &str) -> Vec<Link> {
        self.find(path).map_or_else(Vec::new, |file| {
            file.embeds
                .iter()
                .map(|target| self.link(path, target))
                .collect()
        })
    }
    fn backlinks(&self, path: &str) -> Vec<Arc<str>> {
        self.files
            .iter()
            .filter(|file| {
                self.links(file.path)
                    .iter()
                    .any(|link| link.file.as_deref() == Some(path))
            })
            .map(|file| Arc::from(file.path))
            .collect()
    }
    fn resolve(&self, _from: Option<&str>, target: &str) -> Option<Arc<str>> {
        self.files
            .iter()
            .find(|file| {
                file.path == target
                    || file.path.strip_suffix(".md") == Some(target)
                    || stem(file.path) == target
            })
            .map(|file| Arc::from(file.path))
    }
}

pub fn text(value: &str) -> Typed {
    Typed::Text(value.to_string())
}

pub fn number(value: f64) -> Typed {
    Typed::Number(value)
}

pub fn list(items: impl IntoIterator<Item = Typed>) -> Typed {
    Typed::List(items.into_iter().collect())
}

pub fn texts(items: &[&str]) -> Typed {
    list(items.iter().map(|item| text(item)))
}

pub fn numbers(items: &[f64]) -> Typed {
    list(items.iter().map(|item| number(*item)))
}

pub fn date(written: &str) -> Date {
    Date::parse(written).unwrap_or_else(|| panic!("{written} is not a date"))
}

pub fn now() -> Date {
    date("2025-06-15 12:00:00")
}

pub fn vault() -> Memory {
    Memory {
        files: vec![
            File {
                path: "Labs/Network/Host.md",
                props: vec![
                    ("status", text("draft")),
                    ("score", number(7.5)),
                    ("price", number(4.0)),
                    ("age", number(2.0)),
                    ("aliases", texts(&["dc"])),
                    ("blank", text("")),
                    ("done", Typed::Bool(true)),
                    ("isModified", Typed::Bool(true)),
                    ("due", Typed::Date(date("2025-06-20"))),
                    ("tags", list([Typed::Tag("attack/ad".into())])),
                    ("a b", number(1.0)),
                ],
                tags: vec!["attack/ad", "draft"],
                links: vec!["Index", "Missing"],
                embeds: vec!["diagram.png"],
                size: 2048,
                modified: Some(date("2025-06-14 09:30:00")),
            },
            File {
                path: "Index.md",
                props: vec![("status", text("ready")), ("hours", number(3.0))],
                links: vec!["Labs/Network/Host"],
                ..File::default()
            },
            File {
                path: "Notes/Other.md",
                links: vec!["Index"],
                ..File::default()
            },
            File {
                path: "diagram.png",
                size: 10,
                ..File::default()
            },
        ],
    }
}

pub fn eval_in(memory: &Memory, source: &str) -> Cell {
    let scope = Scope::new(memory, now())
        .file("Labs/Network/Host.md")
        .this("Index.md");
    compile(source)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"))
        .eval(&scope)
}

pub fn eval(source: &str) -> Cell {
    eval_in(&vault(), source)
}

pub fn value(source: &str) -> Typed {
    eval(source).unwrap_or_else(|error| panic!("{source}: {error}"))
}

pub fn holds(source: &str) -> bool {
    value(source).is_truthy()
}

pub fn shown(source: &str) -> String {
    value(source).text(now())
}

pub fn summary_scope<'a>(memory: &'a Memory, column: &'a Typed) -> Scope<'a> {
    Scope::new(memory, now()).values(column)
}
