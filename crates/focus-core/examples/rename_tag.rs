use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Context as _;
use focus_core::{HistoryConfig, OpenOptions, Vault};

fn copy(from: &Path, to: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let source = PathBuf::from(args.next().context("usage: <vault> <from> <to>")?);
    let from = args.next().context("usage: <vault> <from> <to>")?;
    let to = args.next().context("usage: <vault> <from> <to>")?;
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/rename-tag");
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("vault");
    copy(&source, &root)?;
    let vault = Vault::open(
        &root,
        OpenOptions {
            cache_dir: base.join("cache"),
            ignore: Vec::new(),
            watch: false,
        },
    )?;
    vault.set_history(&base.join("history"), HistoryConfig::default())?;
    println!("notes: {}", vault.stats().notes);
    let started = Instant::now();
    let preview = vault.preview_rename_tag(&from, &to)?;
    println!(
        "preview: {} notes, {} occurrences, {:.0} ms",
        preview.notes.len(),
        preview.occurrences,
        started.elapsed().as_secs_f64() * 1000.
    );
    let started = Instant::now();
    let done = vault.rename_tag(&from, &to)?;
    println!(
        "rename: {} notes, {} occurrences, {} failed, {:.0} ms",
        done.notes.len(),
        done.occurrences,
        done.failed.len(),
        started.elapsed().as_secs_f64() * 1000.
    );
    Ok(())
}
