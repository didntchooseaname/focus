use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use focus_md::{Block, BlockKind, ParseOptions, parse};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            collect(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    Ok(())
}

fn tally(blocks: &[Block], counts: &mut BTreeMap<&'static str, usize>) {
    for block in blocks {
        let name = match &block.kind {
            BlockKind::Heading { .. } => "Heading",
            BlockKind::Paragraph(_) => "Paragraph",
            BlockKind::List { items, .. } => {
                for item in items {
                    tally(&item.blocks, counts);
                }
                "List"
            }
            BlockKind::CodeBlock { .. } => "CodeBlock",
            BlockKind::Mermaid { .. } => "Mermaid",
            BlockKind::Math { .. } => "Math",
            BlockKind::Quote(b) => {
                tally(b, counts);
                "Quote"
            }
            BlockKind::Callout { blocks, .. } => {
                tally(blocks, counts);
                "Callout"
            }
            BlockKind::Table { .. } => "Table",
            BlockKind::Embed { .. } => "Embed",
            BlockKind::Image(_) => "Image",
            BlockKind::Rule => "Rule",
            BlockKind::Html(_) => "Html",
            BlockKind::Details { blocks, .. } => {
                tally(blocks, counts);
                "Details"
            }
            BlockKind::Tabs { tabs, .. } => {
                for tab in tabs {
                    tally(&tab.blocks, counts);
                }
                "Tabs"
            }
            BlockKind::Directive(_) => "Directive",
        };
        *counts.entry(name).or_default() += 1;
    }
}

fn main() -> anyhow::Result<()> {
    let root = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: vault_sweep <vault>"))?;
    let mut files = Vec::new();
    collect(Path::new(&root), &mut files)?;
    let options = ParseOptions::default();
    let mut counts = BTreeMap::new();
    let mut total = Duration::ZERO;
    let mut slowest = (Duration::ZERO, PathBuf::new(), 0usize);
    let mut bytes = 0usize;
    let mut unreadable = 0;
    let mut largest = (0usize, String::new());
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            unreadable += 1;
            continue;
        };
        bytes += text.len();
        let start = Instant::now();
        let doc = parse(&text, &options);
        let elapsed = start.elapsed();
        total += elapsed;
        if elapsed > slowest.0 {
            slowest = (elapsed, path.clone(), text.len());
        }
        if text.len() > largest.0 && text.len() < 400_000 {
            largest = (text.len(), text.clone());
        }
        tally(&doc.blocks, &mut counts);
    }
    println!(
        "notes: {} ({} unreadable), {:.1} MB",
        files.len(),
        unreadable,
        bytes as f64 / 1e6
    );
    println!("total parse time: {:.1} ms", total.as_secs_f64() * 1e3);
    println!(
        "slowest note: {} ({} bytes, {:.2} ms)",
        slowest.1.display(),
        slowest.2,
        slowest.0.as_secs_f64() * 1e3
    );
    println!("blocks: {counts:?}");

    let rich = std::env::args()
        .nth(2)
        .map(std::fs::read_to_string)
        .transpose()?;
    for (label, unit) in [
        ("largest note", &largest.1),
        ("rich note", &rich.unwrap_or_default()),
    ] {
        if unit.is_empty() {
            continue;
        }
        let mut big = String::new();
        while big.len() < 100_000 {
            big.push_str(unit);
            big.push('\n');
        }
        let mut best = Duration::MAX;
        for _ in 0..30 {
            let start = Instant::now();
            let doc = parse(&big, &options);
            std::hint::black_box(&doc);
            best = best.min(start.elapsed());
        }
        println!(
            "parse of {} bytes ({label}): best of 30 = {:.2} ms",
            big.len(),
            best.as_secs_f64() * 1e3
        );
    }
    Ok(())
}
