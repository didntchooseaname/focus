use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::images::Images;
use crate::text;

static URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://[^\s<>()\[\]]+").expect("constant pattern"));

#[derive(Debug, Default, Clone)]
pub(crate) struct Sections {
    pub(crate) description: String,
    pub(crate) evidence: String,
    pub(crate) impact: String,
    pub(crate) remediation: String,
    pub(crate) references: String,
    pub(crate) others: Vec<(String, String)>,
}

fn is_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

pub(crate) fn body_of(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let rest = match text.strip_prefix("---\n") {
        Some(after) => match after
            .find("\n---\n")
            .map(|at| (at, 5))
            .or_else(|| after.ends_with("\n---").then(|| (after.len() - 4, 4)))
        {
            Some((at, len)) => after[(at + len).min(after.len())..].to_string(),
            None => text.clone(),
        },
        None => text.clone(),
    };
    let mut out = String::new();
    let mut fenced = false;
    let mut skipped = false;
    for line in rest.split_inclusive('\n') {
        if is_fence(line) {
            fenced = !fenced;
        }
        if !fenced && !skipped && line.starts_with("# ") {
            skipped = true;
            continue;
        }
        out.push_str(line);
    }
    out.trim().to_string()
}

pub(crate) fn split_sections(body: &str) -> Sections {
    let mut sections = Sections::default();
    let mut current: Option<String> = None;
    let mut buffer = String::new();
    let mut fenced = false;
    let flush = |heading: Option<String>, text: &mut String, sections: &mut Sections| {
        let text = std::mem::take(text).trim().to_string();
        match heading
            .as_deref()
            .map(|h| h.trim().to_ascii_lowercase())
            .as_deref()
        {
            None => sections.description = text,
            Some("description") => append(&mut sections.description, &text),
            Some("evidence") => append(&mut sections.evidence, &text),
            Some("impact") => append(&mut sections.impact, &text),
            Some("remediation") => append(&mut sections.remediation, &text),
            Some("references") => append(&mut sections.references, &text),
            Some(_) if text.is_empty() => {}
            Some(_) => sections
                .others
                .push((heading.unwrap_or_default().trim().to_string(), text)),
        }
    };
    for line in body.split_inclusive('\n') {
        if is_fence(line) {
            fenced = !fenced;
        }
        if !fenced && let Some(heading) = line.strip_prefix("## ") {
            flush(current.take(), &mut buffer, &mut sections);
            current = Some(heading.trim().to_string());
            continue;
        }
        buffer.push_str(line);
    }
    flush(current, &mut buffer, &mut sections);
    sections
}

fn append(target: &mut String, text: &str) {
    if text.is_empty() {
        return;
    }
    if !target.is_empty() {
        target.push_str("\n\n");
    }
    target.push_str(text);
}

pub(crate) fn references(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.trim().trim_start_matches(['-', '*', '+']).trim())
        .filter(|line| !line.is_empty())
        .map(|line| match URL.find(line) {
            Some(found) => found.as_str().trim_end_matches(['.', ',', ';']).to_string(),
            None => line.to_string(),
        })
        .collect()
}

pub(crate) fn converted(
    section: &str,
    images: &mut Images,
    note_dir: &Path,
    report: &mut text::Report,
) -> String {
    let (text, found) = text::convert(section, images, note_dir);
    report.merge(found);
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_loses_its_frontmatter_and_title() {
        let note =
            "---\nid: F-001\nseverity: high\n---\n# Title here\n\nText\n\n## Description\n\nBody\n";
        assert_eq!(body_of(note), "Text\n\n## Description\n\nBody");
        assert_eq!(body_of("# Only\r\n\r\nline\r\n"), "line");
        assert_eq!(body_of("---\nkey: v\n---"), "");
        assert_eq!(
            body_of("no frontmatter\n---\nnot a key block"),
            "no frontmatter\n---\nnot a key block"
        );
    }

    #[test]
    fn sections_are_split_on_level_two_headings_outside_code() {
        let body = "Intro line\n\n## Description\n\nOne\n\n```\n## not a heading\n```\n\n## Evidence\n\nShot\n\n## Retest notes\n\nLater\n\n## Impact\n\n## Description\n\nTwo";
        let sections = split_sections(body);
        assert_eq!(
            sections.description, "Intro line\n\nOne\n\n```\n## not a heading\n```\n\nTwo",
            "{sections:?}"
        );
        assert_eq!(sections.evidence, "Shot");
        assert_eq!(sections.impact, "");
        assert_eq!(
            sections.others,
            [("Retest notes".to_string(), "Later".to_string())]
        );
    }

    #[test]
    fn references_are_addresses_when_the_item_has_one() {
        let text = "- https://example.test/a.\n* [advisory](https://example.test/b)\n- Vendor bulletin 12\n\n";
        assert_eq!(
            references(text),
            [
                "https://example.test/a",
                "https://example.test/b",
                "Vendor bulletin 12"
            ]
        );
    }
}
