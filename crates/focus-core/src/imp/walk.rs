use crate::NodeKind;
use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const EXCLUDED: [&str; 4] = [".git", ".obsidian", ".trash", "node_modules"];

pub(crate) struct Ignore {
    globs: GlobSet,
}

impl Ignore {
    pub fn new(patterns: &[String]) -> Result<Ignore, globset::Error> {
        let mut b = GlobSetBuilder::new();
        for p in patterns {
            b.add(Glob::new(p.trim_end_matches('/'))?);
        }
        Ok(Ignore { globs: b.build()? })
    }

    pub fn excluded(&self, relative: &Path) -> bool {
        for c in relative.components() {
            let name = c.as_os_str().to_string_lossy();
            if name.starts_with('.') || EXCLUDED.contains(&name.as_ref()) {
                return true;
            }
        }
        if self.globs.is_empty() {
            return false;
        }
        if self.globs.is_match(relative) {
            return true;
        }
        let mut prefix = PathBuf::new();
        for c in relative.components() {
            prefix.push(c);
            if self.globs.is_match(c.as_os_str()) || self.globs.is_match(&prefix) {
                return true;
            }
        }
        false
    }
}

#[derive(Clone)]
pub(crate) struct FileRec {
    pub rel: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
    pub outside: bool,
}

pub(crate) fn resolves_outside(abs: &Path, root: &Path) -> bool {
    std::fs::symlink_metadata(abs).is_ok_and(|m| m.file_type().is_symlink())
        && dunce::canonicalize(abs).is_ok_and(|real| !real.starts_with(root))
}

#[derive(Default)]
pub(crate) struct Walk {
    pub files: Vec<FileRec>,
    pub folders: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
}

impl Walk {
    fn merge(mut self, other: Walk) -> Walk {
        self.files.extend(other.files);
        self.folders.extend(other.folders);
        self.skipped.extend(other.skipped);
        self
    }
}

pub(crate) fn kind_of(path: &Path) -> NodeKind {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "md" => NodeKind::Note,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "avif" | "ico" | "pdf"
        | "mp3" | "wav" | "ogg" | "m4a" | "flac" | "mp4" | "webm" | "mov" | "mkv" | "canvas"
        | "base" | "excalidraw" => NodeKind::Attachment,
        _ => NodeKind::Other,
    }
}

pub(crate) fn is_canvas(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("canvas"))
}

pub(crate) fn is_media(path: &Path) -> bool {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "svg"
            | "bmp"
            | "avif"
            | "ico"
            | "pdf"
            | "mp3"
            | "wav"
            | "ogg"
            | "m4a"
            | "flac"
            | "mp4"
            | "webm"
            | "mov"
            | "mkv"
    )
}

pub(crate) fn walk(
    root: &Path,
    start: &Path,
    ignore: &Ignore,
    follow: bool,
) -> std::io::Result<Walk> {
    let dir = root.join(start);
    let entries = std::fs::read_dir(&dir)?;
    let mut here = Walk::default();
    let mut subdirs = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let rel = start.join(entry.file_name());
        if ignore.excluded(&rel) {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        let outside = ft.is_symlink()
            && dunce::canonicalize(entry.path()).is_ok_and(|real| !real.starts_with(root));
        if outside && !follow {
            here.skipped.push(rel);
            continue;
        }
        let meta = if ft.is_symlink() {
            match std::fs::metadata(entry.path()) {
                Ok(m) if m.is_file() => m,
                _ => continue,
            }
        } else if ft.is_dir() {
            subdirs.push(rel);
            continue;
        } else if ft.is_file() {
            match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            }
        } else {
            continue;
        };
        here.files.push(FileRec {
            rel,
            size: meta.len(),
            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            outside,
        });
    }
    here.folders.extend(subdirs.iter().cloned());
    let below = subdirs
        .into_par_iter()
        .map(|d| walk(root, &d, ignore, follow).unwrap_or_default())
        .reduce(Walk::default, Walk::merge);
    Ok(here.merge(below))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusions() {
        let ig = Ignore::new(&["Private".into(), "*.tmp".into(), "archive/old/**".into()]).unwrap();
        for p in [
            ".git/x",
            "a/.obsidian/y",
            ".trash/n.md",
            "node_modules/a.md",
            ".hidden.md",
            "Private/n.md",
            "a/Private/n.md",
            "a/b.tmp",
            "archive/old/x.md",
        ] {
            assert!(ig.excluded(Path::new(p)), "{p}");
        }
        for p in ["a/b.md", "Privateer.md", "archive/new/x.md"] {
            assert!(!ig.excluded(Path::new(p)), "{p}");
        }
    }
}
