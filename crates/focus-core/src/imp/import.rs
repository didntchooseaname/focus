use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{Result, Vault, VaultError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Copy,
    Move,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    SymbolicLink,
    Special,
    Failed(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub created: Vec<PathBuf>,
    pub renamed: Vec<(PathBuf, PathBuf)>,
    pub skipped: Vec<(PathBuf, SkipReason)>,
    pub files: usize,
    pub bytes: u64,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportProgress {
    pub files: usize,
    pub bytes: u64,
}

fn reserved(rel: &Path) -> bool {
    matches!(
        rel.components().next().and_then(|c| c.as_os_str().to_str()),
        Some(".obsidian" | ".trash")
    )
}

pub(crate) fn free_name(folder: &Path, name: &str) -> String {
    if std::fs::symlink_metadata(folder.join(name)).is_err() {
        return name.to_string();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (2..)
        .map(|n| format!("{stem} {n}{ext}"))
        .find(|candidate| std::fs::symlink_metadata(folder.join(candidate)).is_err())
        .unwrap_or_else(|| name.to_string())
}

struct Run<'a> {
    cancel: &'a AtomicBool,
    progress: &'a dyn Fn(ImportProgress),
    report: ImportReport,
}

impl Run<'_> {
    fn stopped(&mut self) -> bool {
        if self.cancel.load(Ordering::Relaxed) {
            self.report.cancelled = true;
        }
        self.report.cancelled
    }

    fn skip(&mut self, path: &Path, reason: SkipReason) {
        self.report.skipped.push((path.to_path_buf(), reason));
    }

    fn copy(&mut self, source: &Path, dest: &Path, given: &Path) -> bool {
        if self.stopped() {
            return false;
        }
        let Ok(meta) = std::fs::symlink_metadata(source) else {
            self.skip(source, SkipReason::Failed("not found".into()));
            return false;
        };
        let file_type = meta.file_type();
        if file_type.is_symlink() {
            let inside = source != given
                && dunce::canonicalize(source)
                    .ok()
                    .zip(dunce::canonicalize(given).ok())
                    .is_some_and(|(t, g)| t.starts_with(&g) && t.is_file());
            if !inside && source != given {
                self.skip(source, SkipReason::SymbolicLink);
                return false;
            }
            return match std::fs::metadata(source) {
                Ok(target) if target.is_file() => self.copy_file(source, dest, target.len()),
                Ok(target) if target.is_dir() && source == given => {
                    self.copy_dir(source, dest, given)
                }
                _ => {
                    self.skip(source, SkipReason::SymbolicLink);
                    false
                }
            };
        }
        if file_type.is_dir() {
            return self.copy_dir(source, dest, given);
        }
        if file_type.is_file() {
            return self.copy_file(source, dest, meta.len());
        }
        self.skip(source, SkipReason::Special);
        false
    }

    fn copy_dir(&mut self, source: &Path, dest: &Path, given: &Path) -> bool {
        if let Err(error) = std::fs::create_dir_all(dest) {
            self.skip(source, SkipReason::Failed(error.to_string()));
            return false;
        }
        let entries = match std::fs::read_dir(source) {
            Ok(entries) => entries,
            Err(error) => {
                self.skip(source, SkipReason::Failed(error.to_string()));
                return false;
            }
        };
        let mut names: Vec<_> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        names.sort();
        let mut whole = true;
        for name in names {
            whole &= self.copy(&source.join(&name), &dest.join(&name), given);
        }
        whole
    }

    fn copy_file(&mut self, source: &Path, dest: &Path, len: u64) -> bool {
        if let Err(error) = std::fs::copy(source, dest) {
            self.skip(source, SkipReason::Failed(error.to_string()));
            return false;
        }
        self.report.files += 1;
        self.report.bytes += len;
        (self.progress)(ImportProgress {
            files: self.report.files,
            bytes: self.report.bytes,
        });
        true
    }
}

pub(crate) fn import(
    vault: &Vault,
    sources: &[PathBuf],
    into: &Path,
    mode: ImportMode,
    cancel: &AtomicBool,
    progress: &dyn Fn(ImportProgress),
) -> Result<ImportReport> {
    let root = vault.root().to_path_buf();
    let into = if into.as_os_str().is_empty() {
        PathBuf::new()
    } else {
        super::edit::clean_rel(&root, into)?
    };
    if reserved(&into) {
        return Err(VaultError::Other(format!(
            "nothing is imported into {}",
            into.display()
        )));
    }
    let folder = root.join(&into);
    if !folder.is_dir() {
        return Err(VaultError::Io {
            path: folder,
            source: std::io::Error::from(std::io::ErrorKind::NotADirectory),
        });
    }
    let canonical_root = dunce::canonicalize(&root).map_err(|e| VaultError::Io {
        path: root.clone(),
        source: e,
    })?;
    let mut run = Run {
        cancel,
        progress,
        report: ImportReport::default(),
    };
    let mut touched = HashSet::new();
    for source in sources {
        if run.stopped() {
            break;
        }
        let Some(name) = source.file_name().and_then(|n| n.to_str()) else {
            run.skip(source, SkipReason::Failed("no file name".into()));
            continue;
        };
        let inside = dunce::canonicalize(source)
            .ok()
            .and_then(|c| c.strip_prefix(&canonical_root).ok().map(Path::to_path_buf));
        if let Some(within) = &inside {
            if source.is_dir() && folder.starts_with(root.join(within)) {
                run.skip(
                    source,
                    SkipReason::Failed("a folder cannot go into itself".into()),
                );
                continue;
            }
            if mode == ImportMode::Move && within.parent() == Some(into.as_path()) {
                continue;
            }
        }
        let given = free_name(&folder, name);
        let rel = into.join(&given);
        if given != name {
            run.report.renamed.push((source.clone(), rel.clone()));
        }
        match (mode, inside) {
            (ImportMode::Move, Some(within)) => match vault.rename(&within, &rel, true) {
                Ok(_) => run.report.created.push(rel),
                Err(error) => run.skip(source, SkipReason::Failed(error.to_string())),
            },
            (mode, _) => {
                let whole = run.copy(source, &root.join(&rel), source);
                if std::fs::symlink_metadata(root.join(&rel)).is_ok() {
                    touched.insert(rel.clone());
                    run.report.created.push(rel);
                }
                if mode == ImportMode::Move && whole && !run.report.cancelled {
                    let removed = if source.is_dir() && !source.is_symlink() {
                        std::fs::remove_dir_all(source)
                    } else {
                        std::fs::remove_file(source)
                    };
                    if let Err(error) = removed {
                        run.skip(source, SkipReason::Failed(error.to_string()));
                    }
                }
            }
        }
    }
    if !touched.is_empty() {
        vault.inner.apply_batch(touched, false);
    }
    Ok(run.report)
}
