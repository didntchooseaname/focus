use focus_core::{OpenOptions, Vault};
use focus_search::{CancelToken, MentionOptions, SearchIndex};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let source = PathBuf::from(args.next().ok_or_else(|| anyhow::anyhow!("vault path"))?);
    let name = args.next().unwrap_or_else(|| "dump".into());
    let work = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/mentions-bench");
    let root = work.join("vault");
    if !root.exists() {
        copy_tree(&source, &root)?;
    }
    let file = format!("{name}.md");
    std::fs::write(root.join(&file), "A note with a common one word name.\n")?;
    let vault = Vault::open(
        &root,
        OpenOptions {
            cache_dir: work.join("cache"),
            ignore: Vec::new(),
            watch: false,
        },
    )?;
    let index = SearchIndex::open(vault.clone(), &work.join("cache"), &|_| {})?;
    let note = vault
        .note_by_path(Path::new(&file))
        .ok_or_else(|| anyhow::anyhow!("{file} not indexed"))?;
    let mut times = Vec::new();
    let mut last = None;
    for _ in 0..10 {
        let started = Instant::now();
        let found = index.unlinked_mentions(note, &MentionOptions::default(), &CancelToken::new());
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        last = Some(found);
    }
    times.sort_by(f64::total_cmp);
    let found = last.ok_or_else(|| anyhow::anyhow!("no run"))?;
    println!(
        "notes {}, name {name:?}: default options, median {:.1} ms, max {:.1} ms; {} notes, {} mentions found, partial {}",
        vault.summaries().len(),
        times[times.len() / 2],
        times[times.len() - 1],
        found.total_notes,
        found.total_mentions,
        found.partial
    );
    let started = Instant::now();
    let all = index.unlinked_mentions(
        note,
        &MentionOptions {
            budget_ms: u32::MAX,
            ..Default::default()
        },
        &CancelToken::new(),
    );
    println!(
        "without a time budget: {:.1} ms; {} notes, {} mentions",
        started.elapsed().as_secs_f64() * 1000.0,
        all.total_notes,
        all.total_mentions
    );
    Ok(())
}
