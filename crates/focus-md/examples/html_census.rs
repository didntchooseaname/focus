use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use focus_md::{Block, BlockKind, Inline, InlineKind, ParseOptions, parse};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                collect(&path, out)?;
            }
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    Ok(())
}

fn inline_htmls(inlines: &[Inline], file: &Path, out: &mut Vec<(String, PathBuf)>) {
    for inline in inlines {
        match &inline.kind {
            InlineKind::Html(h) => out.push((format!("{INLINE_MARK}{h}"), file.to_path_buf())),
            InlineKind::Strong(c)
            | InlineKind::Emphasis(c)
            | InlineKind::Strike(c)
            | InlineKind::Highlight(c) => inline_htmls(c, file, out),
            InlineKind::Link(l) => inline_htmls(&l.children, file, out),
            _ => {}
        }
    }
}

const INLINE_MARK: &str = "\u{1}";

fn htmls(blocks: &[Block], file: &Path, out: &mut Vec<(String, PathBuf)>) {
    for block in blocks {
        match &block.kind {
            BlockKind::Paragraph(i) | BlockKind::Heading { inlines: i, .. } => {
                inline_htmls(i, file, out)
            }
            BlockKind::Table { head, rows, .. } => {
                head.iter().for_each(|c| inline_htmls(c, file, out));
                rows.iter()
                    .flatten()
                    .for_each(|c| inline_htmls(c, file, out));
            }
            BlockKind::Html(h) => out.push((h.clone(), file.to_path_buf())),
            BlockKind::Quote(b)
            | BlockKind::Callout { blocks: b, .. }
            | BlockKind::Details { blocks: b, .. } => htmls(b, file, out),
            BlockKind::List { items, .. } => items.iter().for_each(|i| htmls(&i.blocks, file, out)),
            _ => {}
        }
    }
}

fn shape(html: &str) -> String {
    if let Some(inline) = html.strip_prefix(INLINE_MARK) {
        return format!("en ligne {}", shape(inline));
    }
    let t = html.trim_start();
    if t.starts_with("<!--") {
        let inner = t.trim_start_matches("<!--").trim_start();
        let first: String = inner
            .split_whitespace()
            .next()
            .unwrap_or("")
            .chars()
            .take(24)
            .collect();
        return format!("commentaire {first}");
    }
    let rest = t.strip_prefix('<').unwrap_or(t);
    let (closing, rest) = rest.strip_prefix('/').map_or((false, rest), |r| (true, r));
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == ':')
        .collect();
    if name.is_empty() {
        return format!(
            "chevrons sans balise : {}",
            t.chars().take(12).collect::<String>()
        );
    }
    let lower = name.to_lowercase();
    let known = [
        "img",
        "br",
        "details",
        "summary",
        "div",
        "p",
        "a",
        "span",
        "table",
        "tr",
        "td",
        "th",
        "script",
        "style",
        "iframe",
        "svg",
        "h1",
        "h2",
        "h3",
        "h4",
        "ul",
        "ol",
        "li",
        "pre",
        "code",
        "center",
        "hr",
        "b",
        "i",
        "u",
        "strong",
        "em",
        "font",
        "input",
        "form",
        "video",
        "audio",
        "link",
        "meta",
        "html",
        "body",
        "head",
        "button",
        "marquee",
        "object",
        "embed",
        "details",
        "kbd",
        "sup",
        "sub",
        "blockquote",
        "title",
        "textarea",
        "select",
        "label",
        "section",
        "nav",
        "main",
        "figure",
        "figcaption",
        "picture",
        "source",
        "noscript",
        "template",
        "base",
        "style",
    ];
    if known.contains(&lower.as_str()) {
        format!("{}<{lower}>", if closing { "/" } else { "" })
    } else {
        format!("balise inconnue <{name}> (marqueur, charge utile ou composant)")
    }
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: html_census <vault> [shape [n]]"))?;
    let wanted = args.next();
    let show: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(8);
    let mut files = Vec::new();
    collect(Path::new(&root), &mut files)?;
    let mut all = Vec::new();
    for path in &files {
        if let Ok(text) = std::fs::read_to_string(path) {
            htmls(
                &parse(&text, &ParseOptions::default()).blocks,
                path,
                &mut all,
            );
        }
    }
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (html, _) in &all {
        *counts.entry(shape(html)).or_default() += 1;
    }
    let mut ranked: Vec<_> = counts.into_iter().collect();
    ranked.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!("Html blocks: {}", all.len());
    for (form, n) in ranked.iter().take(if wanted.as_deref() == Some("tout") {
        400
    } else {
        25
    }) {
        println!("{n:>6}  {form}");
    }
    if let Some(form) = wanted {
        for (html, file) in all.iter().filter(|(h, _)| shape(h) == form).take(show) {
            let short = file.strip_prefix(&root).unwrap_or(file);
            println!(
                "\n--- {} ---\n{}",
                short.display(),
                html.chars().take(300).collect::<String>()
            );
        }
    }
    Ok(())
}
