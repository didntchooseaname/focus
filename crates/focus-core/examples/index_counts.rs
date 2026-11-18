use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use focus_core::{OpenOptions, Vault};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("usage: index_counts <vault> <cache dir>"))?,
    );
    let cache = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("usage: index_counts <vault> <cache dir>"))?,
    );
    let vault = Vault::open(
        &root,
        OpenOptions {
            cache_dir: cache,
            ignore: Vec::new(),
            watch: false,
        },
    )?;
    let stats = vault.stats();
    println!("open {:.1} ms, {} parsed", stats.open_ms, stats.reparsed);
    let notes = vault.notes();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut tags = HashSet::new();
    let (mut headings, mut tasks, mut blocks) = (0, 0, 0);
    for note in &notes {
        for link in &note.links {
            *kinds.entry(format!("{:?}", link.kind)).or_default() += 1;
        }
        tags.extend(note.tags.iter().map(|t| t.to_lowercase()));
        headings += note.headings.len();
        tasks += (note.tasks_open + note.tasks_done) as usize;
        blocks += note.blocks.len();
    }
    let links: usize = kinds.values().sum();
    println!("{} notes", notes.len());
    println!(
        "links {links} ({})",
        kinds
            .iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let unresolved = vault.all_unresolved();
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    for (_, link) in &unresolved {
        *by_kind.entry(format!("{:?}", link.kind)).or_default() += 1;
    }
    println!(
        "unresolved {} ({})",
        unresolved.len(),
        by_kind
            .iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("distinct tags {}", tags.len());
    println!("headings {headings}, tasks {tasks}, block ids {blocks}");
    Ok(())
}
