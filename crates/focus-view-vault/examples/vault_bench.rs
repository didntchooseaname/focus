use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use focus_core::{NodeKind, OpenOptions, Vault};
use focus_view_vault::{BrokenIndex, TagIndex, TaskFilter, TaskIndex, visible_rows};

fn median<R>(runs: usize, mut f: impl FnMut() -> R) -> (f64, R) {
    let mut times = Vec::with_capacity(runs);
    let mut last = None;
    for _ in 0..runs {
        let t = Instant::now();
        let r = f();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        last = Some(r);
    }
    times.sort_by(|a, b| a.total_cmp(b));
    (times[runs / 2], last.expect("runs is at least one"))
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let vault_path = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("usage: vault_bench <vault> [cache]"))?,
    );
    let cache = args.next().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/vault-bench-cache")
    });
    let t = Instant::now();
    let vault = Arc::new(Vault::open(
        &vault_path,
        OpenOptions {
            cache_dir: cache,
            ignore: Vec::new(),
            watch: false,
        },
    )?);
    let snapshot = vault.tree();
    let notes = snapshot
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Note)
        .count();
    println!(
        "vault {} : {} nodes, {} notes, opened in {:.0} ms",
        vault_path.display(),
        snapshot.nodes.len(),
        notes,
        t.elapsed().as_secs_f64() * 1000.0
    );

    println!("off the interface thread (full loads, median of 5):");
    let (ms, tags) = median(5, || TagIndex::load(&vault));
    println!("  tags          {ms:8.1} ms  {} tags", tags.len());
    let (ms, tasks) = median(5, || TaskIndex::load(&vault));
    println!(
        "  tasks         {ms:8.1} ms  {} tasks in {} notes",
        tasks.items(),
        tasks.notes()
    );
    let (ms, broken) = median(5, || BrokenIndex::load(&vault));
    println!(
        "  broken links  {ms:8.1} ms  {} links in {} notes",
        broken.items(),
        broken.notes()
    );

    println!("on the interface thread (median of 21):");
    let (ms, _) = median(21, || vault.tree());
    println!("  tree snapshot      {ms:8.3} ms");
    let none = HashSet::new();
    let (ms, rows) = median(21, || visible_rows(&snapshot, &none, "", false));
    println!("  rows, folded       {ms:8.3} ms  {} rows", rows.len());
    let all: HashSet<Arc<Path>> = snapshot
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Folder)
        .map(|n| n.path.clone())
        .collect();
    let (ms, rows) = median(21, || visible_rows(&snapshot, &all, "", false));
    println!("  rows, all open     {ms:8.3} ms  {} rows", rows.len());
    for filter in ["a", "kerb", "zzzz"] {
        let (ms, rows) = median(21, || visible_rows(&snapshot, &none, filter, false));
        println!(
            "  filter \"{filter}\"{:pad$} {ms:8.3} ms  {} rows",
            "",
            rows.len(),
            pad = 10 - filter.len()
        );
    }

    let (ms, rows) = median(21, || tasks.rows(&[], |task| TaskFilter::Open.keeps(task)));
    println!(
        "  task rows, full    {ms:8.3} ms  {} rows (load, filter change)",
        rows.len()
    );
    let some_task = vault
        .tasks(false)
        .first()
        .and_then(|t| vault.note(t.note))
        .map(|m| m.path.clone());
    let mut rows = tasks.rows(&[], |task| TaskFilter::Open.keeps(task));
    let (ms, _) = median(21, || {
        tasks.refresh(&mut rows, some_task.as_slice(), &[], |task| {
            TaskFilter::Open.keeps(task)
        })
    });
    println!("  task rows, 1 note  {ms:8.3} ms  (after each event)");
    let (ms, rows) = median(21, || broken.rows(&[], |_| true));
    println!(
        "  broken rows, full  {ms:8.3} ms  {} rows (load)",
        rows.len()
    );

    let mut sample: Vec<_> = vault.summaries().into_iter().map(|s| s.id).collect();
    sample.sort_by_key(|id| std::cmp::Reverse(vault.note(*id).map(|m| m.links.len()).unwrap_or(0)));
    sample.truncate(20);
    let mut tags = tags;
    let (ms, _) = median(21, || {
        for id in &sample {
            tags.set_note(*id, vault.note(*id).map(|n| n.tags.clone()));
        }
    });
    println!("  tags, one note     {:8.3} ms", ms / sample.len() as f64);
    let (ms, _) = median(21, || {
        sample
            .iter()
            .map(|id| vault.tasks_of(*id).len())
            .sum::<usize>()
    });
    println!("  tasks, one note    {:8.3} ms", ms / sample.len() as f64);
    let (ms, _) = median(21, || {
        sample
            .iter()
            .map(|id| vault.unresolved(*id).len())
            .sum::<usize>()
    });
    println!("  broken, one note   {:8.3} ms", ms / sample.len() as f64);
    let (ms, _) = median(21, || {
        sample
            .iter()
            .map(|id| {
                vault
                    .linked_from(*id)
                    .iter()
                    .map(|n| vault.unresolved(*n).len())
                    .sum::<usize>()
            })
            .sum::<usize>()
    });
    println!(
        "  broken, note added {:8.3} ms  (the note and the notes linking to it)",
        ms / sample.len() as f64
    );
    Ok(())
}
