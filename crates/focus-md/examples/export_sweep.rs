use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use focus_md::export::{HtmlOptions, Resolver, to_html, to_standard_markdown};

struct NoLinks;

impl Resolver for NoLinks {
    fn image(&self, _: &str) -> Option<PathBuf> {
        None
    }
    fn link(&self, _: &str, _: Option<&str>) -> Option<String> {
        None
    }
    fn embed(&self, _: &str) -> Option<String> {
        None
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                collect(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

fn main() -> anyhow::Result<()> {
    let root = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: export_sweep <vault>"))?;
    let mut files = Vec::new();
    collect(Path::new(&root), &mut files);
    let options = HtmlOptions {
        embed_fonts: false,
        embed_images: false,
        render_mermaid: true,
        ..HtmlOptions::default()
    };
    let (mut html_time, mut md_time) = (Duration::ZERO, Duration::ZERO);
    let mut slowest = (Duration::ZERO, PathBuf::new(), 0usize);
    let mut bytes = 0;
    let mut notes = 0;
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        notes += 1;
        let started = Instant::now();
        let html = to_html(&text, &options, &NoLinks);
        let elapsed = started.elapsed();
        html_time += elapsed;
        bytes += html.len();
        if elapsed > slowest.0 {
            slowest = (elapsed, path.clone(), text.len());
        }
        let started = Instant::now();
        let _ = to_standard_markdown(&text, &NoLinks);
        md_time += started.elapsed();
    }
    println!(
        "notes: {notes}, html: {:.1} MB in {:.0} ms, standard markdown: {:.0} ms",
        bytes as f64 / 1e6,
        html_time.as_secs_f64() * 1e3,
        md_time.as_secs_f64() * 1e3
    );
    println!(
        "slowest html: {} ({} bytes, {:.0} ms)",
        slowest.1.display(),
        slowest.2,
        slowest.0.as_secs_f64() * 1e3
    );
    Ok(())
}
