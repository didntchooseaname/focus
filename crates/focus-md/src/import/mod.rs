mod html_to_md;
mod notion;

use std::fs;
use std::io::{BufReader, Cursor, Read, Seek, Write as _};
use std::path::{Component, Path, PathBuf};

use anyhow::Context;

#[cfg(not(test))]
const MAX_ENTRIES: usize = 20_000;
#[cfg(test)]
const MAX_ENTRIES: usize = 20;
#[cfg(not(test))]
const MAX_FILE_BYTES: u64 = 256 << 20;
#[cfg(test)]
const MAX_FILE_BYTES: u64 = 1 << 20;
#[cfg(not(test))]
pub(crate) const MAX_TOTAL_BYTES: u64 = 2 << 30;
#[cfg(test)]
pub(crate) const MAX_TOTAL_BYTES: u64 = 3 << 20;
const MAX_NESTING: usize = 2;
const MAX_DEPTH: usize = 64;

#[derive(Debug, thiserror::Error)]
#[error("path leaves the destination")]
pub(crate) struct LeavesDestination;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Budget {
    entries: usize,
    bytes: u64,
}

impl Budget {
    fn entry(&mut self) -> anyhow::Result<()> {
        self.entries += 1;
        anyhow::ensure!(
            self.entries <= MAX_ENTRIES,
            "the import holds more than {MAX_ENTRIES} files"
        );
        Ok(())
    }

    fn bytes(&mut self, count: u64) -> anyhow::Result<()> {
        self.bytes += count;
        anyhow::ensure!(
            self.bytes <= MAX_TOTAL_BYTES,
            "the import holds more than {} MiB",
            MAX_TOTAL_BYTES >> 20
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub notes: Vec<PathBuf>,
    pub attachments: Vec<PathBuf>,
    pub skipped: Vec<(PathBuf, String)>,
    dry: bool,
    root: Option<PathBuf>,
    budget: Budget,
    taken: std::collections::HashSet<PathBuf>,
}

impl Report {
    fn unique(&mut self, path: &Path) -> PathBuf {
        let free = |p: &Path, taken: &std::collections::HashSet<PathBuf>| {
            p.symlink_metadata().is_err() && !taken.contains(p)
        };
        let chosen = if free(path, &self.taken) {
            path.to_path_buf()
        } else {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let extension = path
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            let dir = path.parent().unwrap_or(Path::new("."));
            (2..)
                .map(|n| dir.join(format!("{stem} {n}{extension}")))
                .find(|p| free(p, &self.taken))
                .expect("an unbounded range always yields")
        };
        self.taken.insert(chosen.clone());
        chosen
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Folder,
    Zip,
    Notion,
    Files,
}

pub fn detect(path: &Path) -> Option<Source> {
    if path.is_dir() {
        return Some(Source::Folder);
    }
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "md" | "markdown" | "txt" | "html" | "htm" => Some(Source::Files),
        "zip" => {
            let archive = zip::ZipArchive::new(fs::File::open(path).ok()?).ok()?;
            let notion = archive.file_names().filter_map(Result::ok).any(|name| {
                let lower = name.to_ascii_lowercase();
                notion::is_notion_name(&name) || lower.ends_with(".csv") || lower.ends_with(".zip")
            });
            Some(if notion { Source::Notion } else { Source::Zip })
        }
        _ => None,
    }
}

pub fn run(source: Source, inputs: &[PathBuf], destination_dir: &Path) -> anyhow::Result<Report> {
    import(source, inputs, destination_dir, false)
}

pub fn plan(source: Source, inputs: &[PathBuf], destination_dir: &Path) -> anyhow::Result<Report> {
    import(source, inputs, destination_dir, true)
}

fn import(
    source: Source,
    inputs: &[PathBuf],
    destination_dir: &Path,
    dry: bool,
) -> anyhow::Result<Report> {
    let mut report = Report {
        dry,
        ..Report::default()
    };
    if !dry {
        fs::create_dir_all(destination_dir)
            .with_context(|| format!("creating {}", destination_dir.display()))?;
        report.root = Some(
            dunce::canonicalize(destination_dir)
                .with_context(|| format!("reading {}", destination_dir.display()))?,
        );
    }
    for input in inputs {
        match source {
            Source::Folder => copy_folder(input, destination_dir, &mut report)?,
            Source::Zip => extract_zip(input, destination_dir, &mut report)?,
            Source::Notion => notion::import(input, destination_dir, &mut report)?,
            Source::Files => import_file(input, destination_dir, &mut report)?,
        }
    }
    Ok(report)
}

pub fn html_to_markdown(html: &str) -> String {
    html_to_md::convert(html)
}

pub(crate) fn is_note(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

pub(crate) fn write_new(path: &Path, bytes: &[u8], report: &mut Report) -> anyhow::Result<PathBuf> {
    let target = report.unique(path);
    if report.dry {
        return Ok(target);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
        let inside = report.root.as_ref().is_some_and(|root| {
            dunce::canonicalize(parent).is_ok_and(|real| real.starts_with(root))
        });
        if !inside {
            return Err(LeavesDestination.into());
        }
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .with_context(|| format!("writing {}", target.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("writing {}", target.display()))?;
    Ok(target)
}

fn copy_folder(input: &Path, destination: &Path, report: &mut Report) -> anyhow::Result<()> {
    let name = input
        .file_name()
        .map_or_else(|| "import".into(), |n| n.to_string_lossy().into_owned());
    let source =
        dunce::canonicalize(input).with_context(|| format!("reading {}", input.display()))?;
    anyhow::ensure!(
        !nearest_real_path(destination).starts_with(&source),
        "the destination is inside the folder to import"
    );
    let root = report.unique(&destination.join(name));
    if !report.dry {
        fs::create_dir_all(&root)?;
    }
    copy_tree(input, &root, 0, report)
}

fn nearest_real_path(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut here = path;
    loop {
        if let Ok(real) = dunce::canonicalize(here) {
            return rest.iter().rev().fold(real, |acc, part| acc.join(part));
        }
        match (here.file_name(), here.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_os_string());
                here = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

fn copy_tree(from: &Path, to: &Path, depth: usize, report: &mut Report) -> anyhow::Result<()> {
    if depth > MAX_DEPTH {
        report
            .skipped
            .push((from.to_path_buf(), "folders nested too deep".into()));
        return Ok(());
    }
    let entries = match fs::read_dir(from) {
        Ok(entries) => entries,
        Err(error) => {
            report.skipped.push((from.to_path_buf(), error.to_string()));
            return Ok(());
        }
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            report.skipped.push((path, "hidden entry".into()));
            continue;
        }
        let target = to.join(&name);
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            report.skipped.push((path, "symbolic link".into()));
        } else if kind.is_dir() {
            if report.dry || fs::create_dir_all(&target).is_ok() {
                copy_tree(&path, &target, depth + 1, report)?;
            }
        } else if !kind.is_file() {
            report.skipped.push((path, "not a regular file".into()));
        } else {
            report.budget.entry()?;
            let size = entry.metadata().map_or(0, |m| m.len());
            if size > MAX_FILE_BYTES {
                report.skipped.push((path, "file too large".into()));
                continue;
            }
            report.budget.bytes(size)?;
            let written = if report.dry {
                write_new(&target, &[], report)
            } else {
                fs::read(&path)
                    .map_err(anyhow::Error::from)
                    .and_then(|bytes| write_new(&target, &bytes, report))
            };
            match written {
                Ok(written) => classify(written, report),
                Err(error) => report.skipped.push((path, error.to_string())),
            }
        }
    }
    Ok(())
}

fn classify(path: PathBuf, report: &mut Report) {
    if is_note(&path) {
        report.notes.push(path)
    } else {
        report.attachments.push(path)
    }
}

pub(crate) fn safe_entry_path(name: &str) -> Option<PathBuf> {
    if name.contains('\0') {
        return None;
    }
    let name = name.replace('\\', "/");
    let mut clean = PathBuf::new();
    for (index, component) in Path::new(&name).components().enumerate() {
        match component {
            Component::Normal(part) => {
                let text = part.to_string_lossy();
                let drive = index == 0
                    && text.len() == 2
                    && text.ends_with(':')
                    && text.as_bytes()[0].is_ascii_alphabetic();
                if drive {
                    return None;
                }
                clean.push(part);
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!clean.as_os_str().is_empty()).then_some(clean)
}

pub(crate) fn walk_zip<R: Read + Seek>(
    reader: R,
    report: &mut Report,
    mut each: impl FnMut(String, Vec<u8>, &mut Report) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut archive = zip::ZipArchive::new(reader).context("reading zip archive")?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name()?.into_owned();
        report.budget.entry()?;
        if let Some(mode) = file.unix_mode()
            && !matches!(mode & 0o170_000, 0 | 0o100_000)
        {
            let reason = if mode & 0o170_000 == 0o120_000 {
                "symbolic link"
            } else {
                "not a regular file"
            };
            report.skipped.push((PathBuf::from(&name), reason.into()));
            continue;
        }
        if file.size() > MAX_FILE_BYTES {
            report
                .skipped
                .push((PathBuf::from(&name), "file too large".into()));
            continue;
        }
        let mut data = Vec::with_capacity(file.size().min(1 << 20) as usize);
        (&mut file)
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut data)?;
        anyhow::ensure!(
            data.len() as u64 <= MAX_FILE_BYTES,
            "{name} is larger than its header says"
        );
        report.budget.bytes(data.len() as u64)?;
        each(name, data, report)?;
    }
    Ok(())
}

pub(crate) fn read_zip_entries(
    bytes: Vec<u8>,
    report: &mut Report,
) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    let mut entries = Vec::new();
    walk_zip(Cursor::new(bytes), report, |name, data, _| {
        entries.push((name, data));
        Ok(())
    })?;
    Ok(entries)
}

pub(crate) const fn max_nesting() -> usize {
    MAX_NESTING
}

fn extract_zip(input: &Path, destination: &Path, report: &mut Report) -> anyhow::Result<()> {
    let file = fs::File::open(input).with_context(|| format!("reading {}", input.display()))?;
    let stem = input
        .file_stem()
        .map_or_else(|| "import".into(), |s| s.to_string_lossy().into_owned());
    let root = report.unique(&destination.join(stem));
    walk_zip(BufReader::new(file), report, |name, data, report| {
        let Some(relative) = safe_entry_path(&name) else {
            report
                .skipped
                .push((PathBuf::from(&name), "path leaves the destination".into()));
            return Ok(());
        };
        if relative.components().any(|c| matches!(c, Component::Normal(p) if p.to_string_lossy().starts_with('.') || p == "__MACOSX")) {
            report.skipped.push((relative, "hidden or system entry".into()));
            return Ok(());
        }
        match write_new(&root.join(&relative), &data, report) {
            Ok(written) => classify(written, report),
            Err(error) if error.is::<LeavesDestination>() => {
                report.skipped.push((relative, error.to_string()))
            }
            Err(error) => return Err(error),
        }
        Ok(())
    })
}

fn import_file(input: &Path, destination: &Path, report: &mut Report) -> anyhow::Result<()> {
    let extension = input
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let stem = input
        .file_stem()
        .map_or_else(|| "note".into(), |s| s.to_string_lossy().into_owned());
    match fs::metadata(input) {
        Ok(meta) if !meta.is_file() => {
            report
                .skipped
                .push((input.to_path_buf(), "not a regular file".into()));
            return Ok(());
        }
        Ok(meta) if meta.len() > MAX_FILE_BYTES => {
            report
                .skipped
                .push((input.to_path_buf(), "file too large".into()));
            return Ok(());
        }
        _ => {}
    }
    let content = match extension.as_str() {
        "md" | "markdown" | "txt" => fs::read(input),
        "html" | "htm" => {
            fs::read_to_string(input).map(|html| html_to_markdown(&html).into_bytes())
        }
        _ => {
            report
                .skipped
                .push((input.to_path_buf(), "unsupported file type".into()));
            return Ok(());
        }
    };
    match content {
        Ok(bytes) => {
            let written = write_new(&destination.join(format!("{stem}.md")), &bytes, report)?;
            report.notes.push(written);
        }
        Err(error) => report
            .skipped
            .push((input.to_path_buf(), error.to_string())),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp/import-limits")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn zip_of(path: &Path, entries: &[(&str, Vec<u8>)]) {
        let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn too_many_entries_stop_the_import() {
        let dir = scratch("entries");
        let entries: Vec<(String, Vec<u8>)> = (0..MAX_ENTRIES + 1)
            .map(|i| (format!("n{i}.md"), b"x".to_vec()))
            .collect();
        let borrowed: Vec<(&str, Vec<u8>)> = entries
            .iter()
            .map(|(n, d)| (n.as_str(), d.clone()))
            .collect();
        zip_of(&dir.join("in.zip"), &borrowed);
        let error = run(Source::Zip, &[dir.join("in.zip")], &dir.join("dest")).unwrap_err();
        assert!(error.to_string().contains("more than"), "{error}");
    }

    #[test]
    fn a_decompression_bomb_is_refused_without_filling_the_disk_or_memory() {
        let dir = scratch("bomb");
        zip_of(
            &dir.join("in.zip"),
            &[("big.md", vec![0u8; (MAX_FILE_BYTES + 1) as usize])],
        );
        assert!(
            fs::metadata(dir.join("in.zip")).unwrap().len() < 100_000,
            "the archive itself is small"
        );
        let report = run(Source::Zip, &[dir.join("in.zip")], &dir.join("dest")).unwrap();
        assert!(report.notes.is_empty());
        assert_eq!(report.skipped[0].1, "file too large");
        let files: Vec<_> = walk(&dir.join("dest"));
        assert!(files.is_empty(), "{files:?}");
    }

    #[test]
    fn many_files_below_the_file_limit_still_hit_the_total_limit() {
        let dir = scratch("total");
        let each = (MAX_FILE_BYTES - 1) as usize;
        let entries: Vec<(String, Vec<u8>)> = (0..4)
            .map(|i| (format!("n{i}.md"), vec![b'a'; each]))
            .collect();
        let borrowed: Vec<(&str, Vec<u8>)> = entries
            .iter()
            .map(|(n, d)| (n.as_str(), d.clone()))
            .collect();
        zip_of(&dir.join("in.zip"), &borrowed);
        let error = run(Source::Zip, &[dir.join("in.zip")], &dir.join("dest")).unwrap_err();
        assert!(error.to_string().contains("MiB"), "{error}");
    }

    #[test]
    fn archives_inside_archives_stop_after_the_nesting_limit() {
        let dir = scratch("nesting");
        let mut inner = b"# deep".to_vec();
        let mut bytes = Vec::new();
        for level in 0..5 {
            let path = dir.join(format!("l{level}.zip"));
            zip_of(
                &path,
                &[(
                    if level == 0 {
                        "Page abc123abc123abc123abc123abc123ab.md"
                    } else {
                        "next.zip"
                    },
                    inner.clone(),
                )],
            );
            bytes = fs::read(&path).unwrap();
            inner = bytes.clone();
        }
        fs::write(dir.join("Export-1.zip"), &bytes).unwrap();
        let report = run(
            Source::Notion,
            &[dir.join("Export-1.zip")],
            &dir.join("dest"),
        )
        .unwrap();
        assert!(report.notes.is_empty());
        assert!(
            report
                .skipped
                .iter()
                .any(|(_, why)| why == "archive nested too deep"),
            "{:?}",
            report.skipped
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_folder_that_is_a_link_to_the_outside_is_not_written_through() {
        let dir = scratch("link");
        let outside = dir.join("outside");
        fs::create_dir_all(&outside).unwrap();
        let destination = dir.join("dest");
        fs::create_dir_all(&destination).unwrap();
        std::os::unix::fs::symlink(&outside, destination.join("trap")).unwrap();
        let mut report = Report {
            root: Some(destination.canonicalize().unwrap()),
            ..Report::default()
        };
        let error = write_new(&destination.join("trap/x.md"), b"x", &mut report).unwrap_err();
        assert!(error.is::<LeavesDestination>());
        assert!(!outside.join("x.md").exists());
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path))
            } else {
                out.push(path)
            }
        }
        out
    }
}
