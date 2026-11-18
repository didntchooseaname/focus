use std::cell::OnceCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use focus_core::{LinkKind, LinkTarget, NodeKind, NoteId, NoteMeta, PropValue, Vault};

use crate::date::Date;
use crate::typed::{Link, Typed, Url, is_url, is_wikilink, split_link, tag_name};

pub trait FileSource {
    fn files(&self) -> Vec<Arc<str>>;
    fn note(&self, path: &str) -> Option<NoteId>;
    fn property(&self, path: &str, name: &str) -> Option<Typed>;
    fn properties(&self, path: &str) -> Vec<(String, Typed)>;
    fn size(&self, path: &str) -> Option<u64>;
    fn modified(&self, path: &str) -> Option<Date>;
    fn created(&self, path: &str) -> Option<Date>;
    fn tags(&self, path: &str) -> Vec<String>;
    fn links(&self, path: &str) -> Vec<Link>;
    fn embeds(&self, path: &str) -> Vec<Link>;
    fn backlinks(&self, path: &str) -> Vec<Arc<str>>;
    fn resolve(&self, from: Option<&str>, target: &str) -> Option<Arc<str>>;
}

pub(crate) fn slashed(path: &Path) -> Arc<str> {
    let text = path.to_string_lossy();
    if std::path::MAIN_SEPARATOR == '/' {
        Arc::from(&*text)
    } else {
        Arc::from(text.replace(std::path::MAIN_SEPARATOR, "/"))
    }
}

pub(crate) struct VaultFiles<'a> {
    vault: &'a Vault,
    notes: HashMap<Arc<str>, Arc<NoteMeta>>,
    paths: HashMap<NoteId, Arc<str>>,
    others: Vec<Arc<str>>,
    attachment_backlinks: OnceCell<HashMap<Arc<str>, Vec<Arc<str>>>>,
}

impl<'a> VaultFiles<'a> {
    pub(crate) fn new(vault: &'a Vault, every_file: bool) -> VaultFiles<'a> {
        let mut notes = HashMap::new();
        let mut paths = HashMap::new();
        for meta in vault.notes() {
            let path = slashed(&meta.path);
            paths.insert(meta.id, path.clone());
            notes.insert(path, meta);
        }
        let others = if every_file {
            vault
                .tree()
                .nodes
                .iter()
                .filter(|node| node.kind != NodeKind::Folder && node.note.is_none())
                .map(|node| slashed(&node.path))
                .filter(|path| !notes.contains_key(path))
                .collect()
        } else {
            Vec::new()
        };
        VaultFiles {
            vault,
            notes,
            paths,
            others,
            attachment_backlinks: OnceCell::new(),
        }
    }

    fn target_path(&self, target: &LinkTarget) -> Option<Arc<str>> {
        match target {
            LinkTarget::Note { id, .. } => self.paths.get(id).cloned(),
            LinkTarget::Attachment(path) => Some(slashed(path)),
            LinkTarget::External(_) | LinkTarget::Unresolved => None,
        }
    }

    fn links_where(&self, path: &str, keep: impl Fn(LinkKind) -> bool) -> Vec<Link> {
        let Some(meta) = self.notes.get(path) else {
            return Vec::new();
        };
        self.vault
            .links_of(meta.id)
            .into_iter()
            .filter(|(link, target)| {
                keep(link.kind)
                    && match target {
                        LinkTarget::External(_) => false,
                        LinkTarget::Unresolved => link.kind != LinkKind::Include,
                        _ => true,
                    }
            })
            .map(|(link, target)| Link {
                target: link.target,
                anchor: link.anchor,
                display: None,
                file: self.target_path(&target),
            })
            .collect()
    }

    fn metadata(&self, path: &str) -> Option<std::fs::Metadata> {
        std::fs::metadata(self.vault.absolute(Path::new(path))).ok()
    }

    fn typed(&self, from: &str, name: &str, value: &PropValue) -> Typed {
        match value {
            PropValue::Null => Typed::Null,
            PropValue::Bool(b) => Typed::Bool(*b),
            PropValue::Number(n) => Typed::Number(*n),
            PropValue::Text(text) => {
                if name.eq_ignore_ascii_case("tags") {
                    Typed::Tag(tag_name(text).to_string())
                } else if is_wikilink(text.trim()) {
                    let (target, anchor, display) = split_link(text);
                    Typed::Link(Link {
                        target: target.to_string(),
                        anchor: anchor.map(str::to_string),
                        display: display.map(|d| Box::new(Typed::Text(d.to_string()))),
                        file: self.resolve(Some(from), target),
                    })
                } else if let Some(date) = Date::parse_property(text) {
                    Typed::Date(date)
                } else if is_url(text) && !text.contains(char::is_whitespace) {
                    Typed::Url(Url {
                        address: text.clone(),
                        display: None,
                    })
                } else {
                    Typed::Text(text.clone())
                }
            }
            PropValue::List(items) => Typed::List(
                items
                    .iter()
                    .map(|item| self.typed(from, name, item))
                    .collect(),
            ),
            PropValue::Map(entries) => Typed::Object(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), self.typed(from, key, value)))
                    .collect(),
            ),
        }
    }
}

impl FileSource for VaultFiles<'_> {
    fn files(&self) -> Vec<Arc<str>> {
        self.notes
            .keys()
            .chain(self.others.iter())
            .cloned()
            .collect()
    }

    fn note(&self, path: &str) -> Option<NoteId> {
        self.notes.get(path).map(|meta| meta.id)
    }

    fn property(&self, path: &str, name: &str) -> Option<Typed> {
        let props = &self.notes.get(path)?.props;
        props
            .iter()
            .find(|(key, _)| key == name)
            .or_else(|| {
                let lower = name.to_lowercase();
                props.iter().find(|(key, _)| key.to_lowercase() == lower)
            })
            .map(|(key, value)| self.typed(path, key, value))
    }

    fn properties(&self, path: &str) -> Vec<(String, Typed)> {
        self.notes.get(path).map_or_else(Vec::new, |meta| {
            meta.props
                .iter()
                .map(|(key, value)| (key.clone(), self.typed(path, key, value)))
                .collect()
        })
    }

    fn size(&self, path: &str) -> Option<u64> {
        match self.notes.get(path) {
            Some(meta) => Some(meta.size),
            None => self.metadata(path).map(|m| m.len()),
        }
    }

    fn modified(&self, path: &str) -> Option<Date> {
        let time = match self.notes.get(path) {
            Some(meta) => meta.modified,
            None => self.metadata(path)?.modified().ok()?,
        };
        Date::from_system(time)
    }

    fn created(&self, path: &str) -> Option<Date> {
        Date::from_system(self.metadata(path)?.created().ok()?)
    }

    fn tags(&self, path: &str) -> Vec<String> {
        self.notes
            .get(path)
            .map_or_else(Vec::new, |meta| meta.tags.clone())
    }

    fn links(&self, path: &str) -> Vec<Link> {
        self.links_where(path, |_| true)
    }

    fn embeds(&self, path: &str) -> Vec<Link> {
        self.links_where(path, |kind| {
            matches!(kind, LinkKind::Embed | LinkKind::Include)
        })
    }

    fn backlinks(&self, path: &str) -> Vec<Arc<str>> {
        if let Some(meta) = self.notes.get(path) {
            return self
                .vault
                .linked_from(meta.id)
                .iter()
                .filter_map(|source| self.paths.get(source).cloned())
                .collect();
        }
        self.attachment_backlinks
            .get_or_init(|| {
                let mut sources: HashMap<Arc<str>, Vec<Arc<str>>> = HashMap::new();
                for (source, meta) in &self.notes {
                    for (_, target) in self.vault.links_of(meta.id) {
                        if let LinkTarget::Attachment(file) = target {
                            let list = sources.entry(slashed(&file)).or_default();
                            if !list.contains(source) {
                                list.push(source.clone());
                            }
                        }
                    }
                }
                sources
            })
            .get(path)
            .cloned()
            .unwrap_or_default()
    }

    fn resolve(&self, from: Option<&str>, target: &str) -> Option<Arc<str>> {
        let from = from.and_then(|path| self.notes.get(path));
        let found = match from {
            Some(meta) => self.vault.resolve_link(meta.id, target),
            None => self.vault.resolve_link_from_root(target),
        };
        self.target_path(&found)
    }
}
