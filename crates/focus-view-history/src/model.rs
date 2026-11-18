use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use focus_core::{HistoryKind, NoteId, Vault, VaultError};
use focus_history::{CopyOrigin, DiffLine, History, Version, VersionId, conflict_copy, diff};

pub const CONTEXT: usize = 3;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the history of this vault is not attached")]
    Unavailable,
    #[error("the note is no longer in the vault")]
    UnknownNote,
    #[error(transparent)]
    History(#[from] focus_history::Error),
    #[error(transparent)]
    Vault(#[from] VaultError),
    #[error("{path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Same,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Line {
        mark: Mark,
        number: usize,
        text: String,
    },
    Fold(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConflictCopy {
    pub note: NoteId,
    pub path: Arc<Path>,
    pub origin: CopyOrigin,
    pub device: Option<String>,
    pub modified: SystemTime,
    pub bytes: u64,
}

pub fn conflict_copies(vault: &Vault, note: NoteId) -> Vec<ConflictCopy> {
    let Some(meta) = vault.note(note) else {
        return Vec::new();
    };
    let (folder, name) = (meta.path.parent(), meta.path.file_name());
    let tree = vault.tree();
    let mut copies: Vec<ConflictCopy> = tree
        .nodes
        .iter()
        .filter(|node| node.path.parent() == folder)
        .filter_map(|node| {
            let id = node.note?;
            let parsed = conflict_copy(&node.path.file_name()?.to_string_lossy())?;
            if Some(std::ffi::OsStr::new(&parsed.original)) != name {
                return None;
            }
            let copy = vault.note(id)?;
            Some(ConflictCopy {
                note: id,
                path: copy.path.clone(),
                origin: parsed.origin,
                device: parsed.device,
                modified: copy.modified,
                bytes: copy.size,
            })
        })
        .collect();
    copies.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.path.cmp(&b.path))
    });
    copies
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Version(VersionId),
    Copy(NoteId),
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub versions: Vec<Version>,
    pub copies: Vec<ConflictCopy>,
    pub current: String,
    pub modified: SystemTime,
    pub keeping: bool,
}

#[derive(Debug, Clone)]
pub struct Comparison {
    pub source: Source,
    pub text: String,
    pub rows: Vec<Row>,
    pub added: usize,
    pub removed: usize,
}

impl Comparison {
    pub fn same(&self) -> bool {
        self.added == 0 && self.removed == 0
    }
}

fn history_of(vault: &Vault) -> Result<std::sync::Arc<History>> {
    vault.history().ok_or(Error::Unavailable)
}

pub fn snapshot(vault: &Vault, note: NoteId) -> Result<Snapshot> {
    let history = history_of(vault)?;
    let meta = vault.note(note).ok_or(Error::UnknownNote)?;
    let absolute = vault.absolute(&meta.path);
    let modified = std::fs::metadata(&absolute)
        .and_then(|m| m.modified())
        .map_err(|source| Error::Io {
            path: absolute.clone(),
            source,
        })?;
    let current = vault.read(note)?;
    Ok(Snapshot {
        versions: history.list(&meta.path)?,
        copies: conflict_copies(vault, note),
        current,
        modified,
        keeping: history.config().enabled,
    })
}

pub fn compare(vault: &Vault, note: NoteId, id: &VersionId, current: &str) -> Result<Comparison> {
    let history = history_of(vault)?;
    let meta = vault.note(note).ok_or(Error::UnknownNote)?;
    let text = history.read(&meta.path, id)?;
    Ok(comparison(Source::Version(id.clone()), text, current))
}

pub fn compare_copy(vault: &Vault, copy: NoteId, current: &str) -> Result<Comparison> {
    if vault.note(copy).is_none() {
        return Err(Error::UnknownNote);
    }
    let text = vault.read(copy)?;
    Ok(comparison(Source::Copy(copy), text, current))
}

fn comparison(source: Source, text: String, current: &str) -> Comparison {
    let lines = diff(&text, current);
    let added = lines
        .iter()
        .filter(|l| matches!(l, DiffLine::Added(_)))
        .count();
    let removed = lines
        .iter()
        .filter(|l| matches!(l, DiffLine::Removed(_)))
        .count();
    Comparison {
        source,
        rows: rows(&lines, CONTEXT),
        text,
        added,
        removed,
    }
}

pub fn restore(
    vault: &Vault,
    note: NoteId,
    id: &VersionId,
    expected: SystemTime,
) -> Result<SystemTime> {
    let history = history_of(vault)?;
    let meta = vault.note(note).ok_or(Error::UnknownNote)?;
    let text = history.read(&meta.path, id)?;
    Ok(vault.write_with(note, &text, Some(expected), HistoryKind::Restore)?)
}

pub fn restore_copy(
    vault: &Vault,
    note: NoteId,
    copy: NoteId,
    expected: SystemTime,
) -> Result<SystemTime> {
    if vault.note(note).is_none() || vault.note(copy).is_none() {
        return Err(Error::UnknownNote);
    }
    let text = vault.read(copy)?;
    Ok(vault.write_with(note, &text, Some(expected), HistoryKind::Restore)?)
}

pub fn rows(lines: &[DiffLine], context: usize) -> Vec<Row> {
    let mut keep = vec![false; lines.len()];
    for (index, line) in lines.iter().enumerate() {
        if !matches!(line, DiffLine::Same(_)) {
            let from = index.saturating_sub(context);
            let to = (index + context + 1).min(lines.len());
            keep[from..to].fill(true);
        }
    }
    let (mut old, mut new) = (0, 0);
    let mut out = Vec::new();
    let mut folded = 0;
    for (index, line) in lines.iter().enumerate() {
        let (mark, text) = match line {
            DiffLine::Same(text) => {
                old += 1;
                new += 1;
                (Mark::Same, text)
            }
            DiffLine::Removed(text) => {
                old += 1;
                (Mark::Removed, text)
            }
            DiffLine::Added(text) => {
                new += 1;
                (Mark::Added, text)
            }
        };
        if !keep[index] {
            folded += 1;
            continue;
        }
        if folded > 0 {
            out.push(Row::Fold(folded));
            folded = 0;
        }
        out.push(Row::Line {
            mark,
            number: if mark == Mark::Removed { old } else { new },
            text: text.clone(),
        });
    }
    if folded > 0 {
        out.push(Row::Fold(folded));
    }
    out
}

#[cfg(test)]
mod tests;
