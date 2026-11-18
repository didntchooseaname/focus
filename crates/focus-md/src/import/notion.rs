use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::Context;

use super::{Report, classify, max_nesting, read_zip_entries, safe_entry_path, write_new};

pub(super) fn is_notion_name(name: &str) -> bool {
    name.split('/')
        .any(|segment| clean_segment(segment) != segment)
}

fn clean_segment(segment: &str) -> String {
    let (stem, extension) = match segment.rfind('.') {
        Some(dot) if dot > 0 && segment.len() - dot <= 6 => (&segment[..dot], &segment[dot..]),
        _ => (segment, ""),
    };
    let stem = stem
        .strip_suffix("_all")
        .map(|s| (s, "_all"))
        .unwrap_or((stem, ""));
    let base = stem.0;
    let cleaned = match base.len().checked_sub(33) {
        Some(at)
            if base.as_bytes()[at] == b' '
                && base[at + 1..].bytes().all(|b| b.is_ascii_hexdigit()) =>
        {
            &base[..at]
        }
        _ => base,
    };
    format!("{cleaned}{}{extension}", stem.1)
}

fn clean_path(path: &Path) -> PathBuf {
    path.components()
        .map(|c| match c {
            Component::Normal(part) => clean_segment(&part.to_string_lossy()),
            other => other.as_os_str().to_string_lossy().into_owned(),
        })
        .collect()
}

pub(super) fn import(input: &Path, destination: &Path, report: &mut Report) -> anyhow::Result<()> {
    let size = fs::metadata(input)
        .with_context(|| format!("reading {}", input.display()))?
        .len();
    anyhow::ensure!(
        size <= super::MAX_TOTAL_BYTES,
        "{} is larger than {} MiB",
        input.display(),
        super::MAX_TOTAL_BYTES >> 20
    );
    let bytes = fs::read(input).with_context(|| format!("reading {}", input.display()))?;
    let mut files: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    collect(bytes, 0, &mut files, report)?;
    let name = input
        .file_stem()
        .map_or_else(|| "notion".into(), |s| clean_segment(&s.to_string_lossy()));
    let root = report.unique(&destination.join(name));

    let originals: HashSet<PathBuf> = files.iter().map(|(p, _)| p.clone()).collect();
    let mut mapping: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut used: HashSet<String> = HashSet::new();
    for (path, _) in &files {
        let lower = path.to_string_lossy().to_lowercase();
        let (target, skip) = if lower.ends_with(".csv") {
            let sibling = PathBuf::from(path.to_string_lossy().replace("_all.csv", ".csv"));
            (
                clean_path(path).with_extension("md"),
                lower.ends_with("_all.csv") && originals.contains(&sibling),
            )
        } else {
            (clean_path(path), false)
        };
        if skip {
            report
                .skipped
                .push((path.clone(), "duplicate of the database export".into()));
            continue;
        }
        let mut candidate = target.clone();
        let mut n = 2;
        while !used.insert(candidate.to_string_lossy().to_lowercase()) {
            let stem = target
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let extension = target
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            candidate = target.with_file_name(format!("{stem} {n}{extension}"));
            n += 1;
        }
        mapping.insert(path.clone(), candidate);
    }

    for (path, data) in &files {
        let Some(target) = mapping.get(path) else {
            continue;
        };
        let lower = path.to_string_lossy().to_lowercase();
        let content = if lower.ends_with(".csv") {
            csv_to_markdown(&String::from_utf8_lossy(data)).into_bytes()
        } else if lower.ends_with(".md") {
            rewrite_links(&String::from_utf8_lossy(data), path, target, &mapping).into_bytes()
        } else {
            data.clone()
        };
        match write_new(&root.join(target), &content, report) {
            Ok(written) => classify(written, report),
            Err(error) if error.is::<super::LeavesDestination>() => {
                report.skipped.push((path.clone(), error.to_string()))
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn collect(
    bytes: Vec<u8>,
    depth: usize,
    out: &mut Vec<(PathBuf, Vec<u8>)>,
    report: &mut Report,
) -> anyhow::Result<()> {
    for (name, data) in read_zip_entries(bytes, report)? {
        let Some(path) = safe_entry_path(&name) else {
            report
                .skipped
                .push((PathBuf::from(&name), "path leaves the destination".into()));
            continue;
        };
        if path.components().any(|c| matches!(c, Component::Normal(p) if p == "__MACOSX" || p.to_string_lossy().starts_with('.'))) {
            continue;
        }
        if name.to_ascii_lowercase().ends_with(".zip") {
            if depth + 1 >= max_nesting() {
                report
                    .skipped
                    .push((path, "archive nested too deep".into()));
            } else {
                collect(data, depth + 1, out, report)?;
            }
        } else {
            out.push((path, data));
        }
    }
    Ok(())
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(v) = text
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn normalize(path: PathBuf) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
            _ => {}
        }
    }
    out
}

fn relative(from_dir: &Path, to: &Path) -> String {
    let from: Vec<_> = from_dir.components().collect();
    let target: Vec<_> = to.components().collect();
    let common = from.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".to_string(); from.len() - common];
    parts.extend(
        target[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/")
}

fn rewrite_links(
    markdown: &str,
    original: &Path,
    new: &Path,
    mapping: &HashMap<PathBuf, PathBuf>,
) -> String {
    let original_dir = original.parent().unwrap_or(Path::new(""));
    let new_dir = new.parent().unwrap_or(Path::new(""));
    let mut out = String::with_capacity(markdown.len());
    let mut rest = markdown;
    while let Some(at) = rest.find("](") {
        out.push_str(&rest[..at + 2]);
        rest = &rest[at + 2..];
        let Some(end) = rest.find(')') else { break };
        let destination = &rest[..end];
        let decoded = percent_decode(destination);
        let (path_part, anchor) = decoded
            .split_once('#')
            .map_or((decoded.as_str(), ""), |(p, a)| (p, a));
        let is_external = path_part.contains("://") || path_part.starts_with("mailto:");
        let resolved = (!is_external && !path_part.is_empty())
            .then(|| normalize(original_dir.join(path_part)))
            .and_then(|p| mapping.get(&p));
        match resolved {
            Some(target) => {
                out.push_str(&relative(new_dir, target).replace(' ', "%20"));
                if !anchor.is_empty() {
                    out.push('#');
                    out.push_str(anchor);
                }
            }
            None => out.push_str(destination),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (other, _) => field.push(other),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn csv_to_markdown(text: &str) -> String {
    let rows = parse_csv(text);
    let Some(columns) = rows.iter().map(Vec::len).max().filter(|c| *c > 0) else {
        return String::new();
    };
    let line = |row: &Vec<String>| {
        let mut cells: Vec<String> = row
            .iter()
            .map(|c| c.replace('|', "\\|").replace('\n', "<br>"))
            .collect();
        cells.resize(columns, String::new());
        format!("| {} |", cells.join(" | "))
    };
    let mut out = format!("{}\n|{}", line(&rows[0]), " --- |".repeat(columns));
    for row in &rows[1..] {
        out.push('\n');
        out.push_str(&line(row));
    }
    out.push('\n');
    out
}
