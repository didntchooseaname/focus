use std::path::{Path, PathBuf};
use std::time::Instant;

use focus_core::{HistoryConfig, HistoryKind, OpenOptions, Vault};

fn main() -> anyhow::Result<()> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/history-write");
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("vault");
    std::fs::create_dir_all(&root)?;
    let text = |n: usize| -> String {
        (0..10_000)
            .map(|i| format!("line {i} of the large note, with some text to fill it out {n}\n"))
            .collect()
    };
    std::fs::write(root.join("big.md"), text(0))?;
    let vault = Vault::open(
        &root,
        OpenOptions {
            cache_dir: base.join("cache"),
            ignore: Vec::new(),
            watch: false,
        },
    )?;
    let id = vault
        .note_by_path(Path::new("big.md"))
        .ok_or_else(|| anyhow::anyhow!("note missing"))?;
    let bodies: Vec<String> = (1..=200).map(text).collect();
    let run = |label: &str, kind: HistoryKind, from: usize| -> anyhow::Result<()> {
        let started = Instant::now();
        for body in &bodies[from..from + 100] {
            vault.write_with(id, body, None, kind)?;
        }
        println!(
            "{label}: {:.2} ms per write",
            started.elapsed().as_secs_f64() * 10.0
        );
        Ok(())
    };
    run("no history", HistoryKind::Edit, 0)?;
    vault.set_history(&base.join("history"), HistoryConfig::default())?;
    vault.write(id, &bodies[0], None)?;
    run("history, version not due", HistoryKind::Edit, 0)?;
    run("history, version stored (Bulk)", HistoryKind::Bulk, 100)?;
    Ok(())
}
