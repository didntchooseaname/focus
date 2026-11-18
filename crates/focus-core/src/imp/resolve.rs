use super::config::fnv1a;
use super::fsx::{normalize, parent_dir};
use super::parse::is_external;
use super::{FileEntry, NoteEntry, Resolved};
use crate::NoteId;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Via {
    SelfRef,
    Relative,
    Root,
    Name,
    Alias,
    None,
}

struct HashIndex<T> {
    entries: Vec<(u64, T)>,
}

impl<T: Copy + Send> HashIndex<T> {
    fn new(mut entries: Vec<(u64, T)>) -> HashIndex<T> {
        entries.par_sort_unstable_by_key(|(h, _)| *h);
        HashIndex { entries }
    }

    fn get(&self, key: &str) -> impl Iterator<Item = T> + '_ {
        let h = fnv1a(key.as_bytes());
        let start = self.entries.partition_point(|(k, _)| *k < h);
        self.entries[start..]
            .iter()
            .take_while(move |(k, _)| *k == h)
            .map(|(_, v)| *v)
    }
}

impl<T> Default for HashIndex<T> {
    fn default() -> Self {
        HashIndex {
            entries: Vec::new(),
        }
    }
}

#[derive(Default)]
pub(crate) struct NameIndex {
    note_path: HashIndex<NoteId>,
    note_stem: HashIndex<NoteId>,
    alias: HashIndex<NoteId>,
    files: Vec<Arc<Path>>,
    file_path: HashIndex<u32>,
    file_name: HashIndex<u32>,
}

fn lower_slash(p: &Path) -> String {
    super::fsx::slash(p).to_lowercase()
}

fn lower_stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn lower_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn entry(notes: &[Option<NoteEntry>], id: NoteId) -> Option<&NoteEntry> {
    notes.get(id as usize)?.as_ref()
}

impl NameIndex {
    pub fn build(notes: &[Option<NoteEntry>], files: &HashMap<Arc<Path>, FileEntry>) -> NameIndex {
        let h = |s: String| fnv1a(s.as_bytes());
        let live: Vec<&NoteEntry> = notes.iter().flatten().collect();
        let note_path = live
            .par_iter()
            .map(|e| (h(lower_slash(&e.path)), e.id()))
            .collect();
        let note_stem = live
            .par_iter()
            .map(|e| (h(lower_stem(&e.path)), e.id()))
            .collect();
        let alias = live
            .iter()
            .flat_map(|e| e.note.aliases.iter().map(|a| (h(a.to_lowercase()), e.id())))
            .collect();
        let files: Vec<Arc<Path>> = files.keys().cloned().collect();
        let file_path = files
            .par_iter()
            .enumerate()
            .map(|(i, p)| (h(lower_slash(p)), i as u32))
            .collect();
        let file_name = files
            .par_iter()
            .enumerate()
            .map(|(i, p)| (h(lower_name(p)), i as u32))
            .collect();
        NameIndex {
            note_path: HashIndex::new(note_path),
            note_stem: HashIndex::new(note_stem),
            alias: HashIndex::new(alias),
            files,
            file_path: HashIndex::new(file_path),
            file_name: HashIndex::new(file_name),
        }
    }

    pub fn notes_named(&self, notes: &[Option<NoteEntry>], stem: &str) -> Vec<NoteId> {
        let key = stem.to_lowercase();
        self.note_stem
            .get(&key)
            .filter(|&id| entry(notes, id).is_some_and(|e| lower_stem(&e.path) == key))
            .collect()
    }

    pub fn files_named(&self, name: &str) -> Vec<Arc<Path>> {
        let key = name.to_lowercase();
        self.file_name
            .get(&key)
            .map(|i| self.files[i as usize].clone())
            .filter(|p| lower_name(p) == key)
            .collect()
    }

    fn note_at(&self, notes: &[Option<NoteEntry>], key: &str) -> Option<NoteId> {
        self.note_path
            .get(key)
            .find(|&id| entry(notes, id).is_some_and(|e| lower_slash(&e.path) == key))
    }

    fn aliased(&self, notes: &[Option<NoteEntry>], key: &str) -> Vec<NoteId> {
        self.alias
            .get(key)
            .filter(|&id| {
                entry(notes, id)
                    .is_some_and(|e| e.note.aliases.iter().any(|a| a.to_lowercase() == key))
            })
            .collect()
    }

    fn lookup(&self, notes: &[Option<NoteEntry>], p: &Path) -> Option<Resolved> {
        let s = lower_slash(p);
        if s.is_empty() {
            return None;
        }
        if let Some(id) = self.note_at(notes, &s) {
            return Some(Resolved::Note(id));
        }
        if let Some(id) = self.note_at(notes, &format!("{s}.md")) {
            return Some(Resolved::Note(id));
        }
        self.file_path
            .get(&s)
            .map(|i| &self.files[i as usize])
            .find(|f| lower_slash(f) == s)
            .map(|f| Resolved::File(f.clone()))
    }

    pub fn resolve(
        &self,
        notes: &[Option<NoteEntry>],
        from_id: Option<NoteId>,
        from: &Path,
        target: &str,
    ) -> (Resolved, Via) {
        let t = target.trim();
        if t.is_empty() {
            return match from_id {
                Some(id) => (Resolved::Note(id), Via::SelfRef),
                None => (Resolved::Unresolved, Via::None),
            };
        }
        if is_external(t) {
            return (Resolved::External, Via::None);
        }
        let rooted = t.starts_with('/');
        let t = t.trim_start_matches('/');
        let from_dir = parent_dir(from);
        if !rooted
            && let Some(p) = normalize(&from_dir.join(t))
            && let Some(r) = self.lookup(notes, &p)
        {
            return (r, Via::Relative);
        }
        if let Some(p) = normalize(Path::new(t))
            && let Some(r) = self.lookup(notes, &p)
        {
            return (r, Via::Root);
        }
        let lower = t.to_lowercase();
        let lower = lower.trim_start_matches("./");
        let name = lower.rsplit('/').next().unwrap_or(lower);
        let has_dir = lower.contains('/');
        let note_key = name.strip_suffix(".md").unwrap_or(name);
        let lower_stemmed = lower.strip_suffix(".md").unwrap_or(lower);
        let note_path = |id: NoteId| {
            notes
                .get(id as usize)
                .and_then(|e| e.as_ref())
                .map(|e| e.path.clone())
        };
        let ids = self.notes_named(notes, note_key);
        if !ids.is_empty() {
            let cands = ids
                .iter()
                .filter_map(|&id| note_path(id).map(|p| (p, id)))
                .filter(|(p, _)| {
                    if !has_dir {
                        return true;
                    }
                    let s = lower_slash(p);
                    let s = s.strip_suffix(".md").unwrap_or(&s);
                    s == lower_stemmed || s.ends_with(&format!("/{lower_stemmed}"))
                });
            if let Some(id) = closest(from_dir, cands) {
                return (Resolved::Note(id), Via::Name);
            }
        }
        let paths = self.files_named(name);
        if !paths.is_empty() {
            let cands = paths
                .iter()
                .map(|p| (p.clone(), p.clone()))
                .filter(|(p, _)| {
                    if !has_dir {
                        return true;
                    }
                    let s = lower_slash(p);
                    s == lower || s.ends_with(&format!("/{lower}"))
                });
            if let Some(p) = closest(from_dir, cands) {
                return (Resolved::File(p), Via::Name);
            }
        }
        if !has_dir
            && let Some(id) = closest(
                from_dir,
                self.aliased(notes, lower)
                    .into_iter()
                    .filter_map(|id| note_path(id).map(|p| (p, id))),
            )
        {
            return (Resolved::Note(id), Via::Alias);
        }
        (Resolved::Unresolved, Via::None)
    }
}

fn closest<T>(from_dir: &Path, cands: impl Iterator<Item = (Arc<Path>, T)>) -> Option<T> {
    let from: Vec<_> = from_dir.components().collect();
    cands
        .map(|(p, v)| {
            let dir = parent_dir(&p);
            let common = dir
                .components()
                .zip(&from)
                .take_while(|(a, b)| a == *b)
                .count();
            (
                (std::cmp::Reverse(common), p.components().count(), p.clone()),
                v,
            )
        })
        .min_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, v)| v)
}
