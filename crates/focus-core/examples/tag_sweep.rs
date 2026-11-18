use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn notes(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            notes(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            out.push(path);
        }
    }
    Ok(())
}

fn has_tag(source: &str, tag: &str) -> bool {
    focus_core::parse_meta(source)
        .tags
        .iter()
        .any(|t| t.eq_ignore_ascii_case(tag))
}

fn line_of(source: &str, tag: &str) -> usize {
    let ends: Vec<usize> = source
        .match_indices('\n')
        .map(|(i, _)| i + 1)
        .chain([source.len()])
        .collect();
    let at = ends.partition_point(|end| !has_tag(&source[..*end], tag));
    at + 1
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("usage: tag_sweep <vault> [tag...]"))?,
    );
    let (all, wanted): (Vec<String>, Vec<String>) = args.partition(|a| a == "--all");
    let mut files = Vec::new();
    notes(&root, &mut files)?;
    files.sort();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut found: BTreeMap<String, Vec<(PathBuf, usize)>> = BTreeMap::new();
    for file in &files {
        let source = String::from_utf8_lossy(&std::fs::read(file)?).into_owned();
        for tag in focus_core::parse_meta(&source).tags {
            *counts.entry(tag.to_lowercase()).or_default() += 1;
            if wanted.iter().any(|w| w.eq_ignore_ascii_case(&tag)) {
                let relative = file.strip_prefix(&root).unwrap_or(file).to_path_buf();
                found
                    .entry(tag.to_lowercase())
                    .or_default()
                    .push((relative, line_of(&source, &tag)));
            }
        }
    }
    println!("{} notes, {} distinct tags", files.len(), counts.len());
    if !all.is_empty() {
        for (tag, count) in &counts {
            println!("{count:6} #{tag}");
        }
    }
    for (tag, places) in &found {
        for (path, line) in places {
            println!("#{tag}  {}:{line}", path.display());
        }
    }
    Ok(())
}
