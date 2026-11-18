use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use focus_core::{NoteId, Vault};

pub struct Group<T> {
    pub note: NoteId,
    pub title: String,
    pub canvas: bool,
    pub items: Vec<T>,
}

pub struct Grouped<T> {
    groups: BTreeMap<Arc<Path>, Group<T>>,
    paths: HashMap<NoteId, Arc<Path>>,
}

impl<T> Default for Grouped<T> {
    fn default() -> Self {
        Grouped {
            groups: BTreeMap::new(),
            paths: HashMap::new(),
        }
    }
}

impl<T> Grouped<T> {
    pub(crate) fn from_items(
        vault: &Vault,
        items: impl IntoIterator<Item = (NoteId, T)>,
    ) -> Grouped<T> {
        let mut by_note: HashMap<NoteId, Vec<T>> = HashMap::new();
        for (note, item) in items {
            by_note.entry(note).or_default().push(item);
        }
        let mut grouped = Grouped::default();
        for (note, items) in by_note {
            grouped.put(vault, note, items);
        }
        grouped
    }

    pub fn notes(&self) -> usize {
        self.groups.len()
    }

    pub fn items(&self) -> usize {
        self.groups.values().map(|g| g.items.len()).sum()
    }

    pub(crate) fn put(&mut self, vault: &Vault, note: NoteId, items: Vec<T>) -> Vec<Arc<Path>> {
        let mut changed: Vec<Arc<Path>> = self.remove(note).into_iter().collect();
        if items.is_empty() {
            return changed;
        }
        let Some(path) = vault.source_path(note) else {
            return changed;
        };
        let (title, canvas) = match vault.note(note) {
            Some(meta) => (meta.title.to_string(), false),
            None => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (name, true)
            }
        };
        self.paths.insert(note, path.clone());
        self.groups.insert(
            path.clone(),
            Group {
                note,
                title,
                canvas,
                items,
            },
        );
        if !changed.contains(&path) {
            changed.push(path);
        }
        changed
    }

    pub(crate) fn remove(&mut self, note: NoteId) -> Option<Arc<Path>> {
        let path = self.paths.remove(&note)?;
        self.groups.remove(&path);
        Some(path)
    }

    fn kept(
        path: &Path,
        group: &Group<T>,
        excluded: &[PathBuf],
        keep: &impl Fn(&T) -> bool,
    ) -> Vec<u32> {
        if excluded.iter().any(|folder| path.starts_with(folder)) {
            return Vec::new();
        }
        group
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| keep(item))
            .map(|(i, _)| i as u32)
            .collect()
    }

    pub fn rows(&self, excluded: &[PathBuf], keep: impl Fn(&T) -> bool) -> Rows {
        let mut rows = Rows::default();
        for (path, group) in &self.groups {
            let kept = Self::kept(path, group, excluded, &keep);
            if !kept.is_empty() {
                rows.groups.push((path.clone(), kept));
            }
        }
        rows.count();
        rows
    }

    pub fn refresh(
        &self,
        rows: &mut Rows,
        paths: &[Arc<Path>],
        excluded: &[PathBuf],
        keep: impl Fn(&T) -> bool,
    ) {
        for path in paths {
            let kept = self
                .groups
                .get(path)
                .map(|group| Self::kept(path, group, excluded, &keep))
                .unwrap_or_default();
            match (
                rows.groups.binary_search_by(|(p, _)| p.cmp(path)),
                kept.is_empty(),
            ) {
                (Ok(at), false) => rows.groups[at].1 = kept,
                (Ok(at), true) => {
                    rows.groups.remove(at);
                }
                (Err(at), false) => rows.groups.insert(at, (path.clone(), kept)),
                (Err(_), true) => {}
            }
        }
        rows.count();
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Row {
    Note(usize),
    Item(usize, usize),
}

#[derive(Default)]
pub struct Rows {
    groups: Vec<(Arc<Path>, Vec<u32>)>,
    starts: Vec<usize>,
}

impl Rows {
    fn count(&mut self) {
        self.starts.clear();
        let mut row = 0;
        for (_, kept) in &self.groups {
            self.starts.push(row);
            row += 1 + kept.len();
        }
        self.starts.push(row);
    }

    pub fn len(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn groups(&self) -> usize {
        self.groups.len()
    }

    pub(crate) fn get(&self, row: usize) -> Row {
        let g = self.starts.partition_point(|start| *start <= row) - 1;
        match row - self.starts[g] {
            0 => Row::Note(g),
            n => Row::Item(g, self.groups[g].1[n - 1] as usize),
        }
    }

    pub(crate) fn path(&self, group: usize) -> &Arc<Path> {
        &self.groups[group].0
    }

    pub(crate) fn items(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.groups
            .iter()
            .enumerate()
            .flat_map(|(g, (_, kept))| kept.iter().map(move |i| (g, *i as usize)))
    }
}

impl<T> std::ops::Index<&Arc<Path>> for Grouped<T> {
    type Output = Group<T>;
    fn index(&self, path: &Arc<Path>) -> &Group<T> {
        &self.groups[path]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grouped(groups: &[(&str, &[u32])]) -> Grouped<u32> {
        let mut grouped = Grouped::default();
        for (note, (path, items)) in groups.iter().enumerate() {
            let path: Arc<Path> = Arc::from(Path::new(path));
            grouped.paths.insert(note as NoteId, path.clone());
            grouped.groups.insert(
                path,
                Group {
                    note: note as NoteId,
                    title: String::new(),
                    canvas: false,
                    items: items.to_vec(),
                },
            );
        }
        grouped
    }

    fn shown(rows: &Rows) -> Vec<String> {
        (0..rows.len())
            .map(|r| match rows.get(r) {
                Row::Note(g) => rows.path(g).display().to_string(),
                Row::Item(g, i) => format!("{}:{i}", rows.path(g).display()),
            })
            .collect()
    }

    #[test]
    fn refreshing_one_group_gives_the_rows_of_a_full_build() {
        let even = |n: &u32| n.is_multiple_of(2);
        let excluded = [PathBuf::from("Templates")];
        let mut index = grouped(&[
            ("a.md", &[1, 2, 4]),
            ("Templates/t.md", &[2]),
            ("c.md", &[3]),
        ]);
        let mut rows = index.rows(&excluded, even);
        assert_eq!(shown(&rows), ["a.md", "a.md:1", "a.md:2"]);
        for (path, items) in [("c.md", vec![3, 6]), ("a.md", vec![1])] {
            if let Some(group) = index.groups.get_mut(Path::new(path)) {
                group.items = items;
            }
        }
        let changed: Vec<Arc<Path>> =
            vec![Arc::from(Path::new("c.md")), Arc::from(Path::new("a.md"))];
        index.refresh(&mut rows, &changed, &excluded, even);
        assert_eq!(shown(&rows), shown(&index.rows(&excluded, even)));
        assert_eq!(shown(&rows), ["c.md", "c.md:1"]);
        assert_eq!(rows.items().collect::<Vec<_>>(), [(0, 1)]);
    }
}
