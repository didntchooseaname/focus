#[path = "support/vaultgen.rs"]
mod vaultgen;

use focus_core::{OpenOptions, Vault};
use focus_search::{CancelToken, FuzzyScope, SearchIndex, SearchOptions, parse_query};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

macro_rules! say {
    ($($t:tt)*) => {
        if writeln!(std::io::stdout().lock(), $($t)*).is_err() {
            std::process::exit(0);
        }
    };
}
use std::time::Instant;

fn target_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target")
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

fn summary(mut v: Vec<f64>) -> String {
    v.sort_by(f64::total_cmp);
    format!(
        "median {:.2} ms, p95 {:.2} ms, max {:.2} ms ({} runs)",
        percentile(&v, 0.5),
        percentile(&v, 0.95),
        v.last().copied().unwrap_or(0.0),
        v.len()
    )
}

fn status_kb(field: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with(field))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0)
}

fn queries(vault: &Vault) -> Vec<String> {
    let mut words: Vec<String> = vault
        .notes()
        .iter()
        .flat_map(|m| {
            m.title
                .split(|c: char| !c.is_alphanumeric())
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        })
        .filter(|w| w.chars().count() >= 4 && !w.chars().all(|c| c.is_ascii_digit()))
        .collect();
    words.sort();
    words.dedup();
    let mut rng = vaultgen::Rng::new(42);
    (0..200).map(|_| rng.pick(&words).clone()).collect()
}

fn measure(root: &Path, label: &str) -> anyhow::Result<()> {
    let cache = target_dir().join("bench-cache").join(label);
    let _ = std::fs::remove_dir_all(&cache);
    let options = || OpenOptions {
        cache_dir: cache.clone(),
        ignore: Vec::new(),
        watch: false,
    };

    let t = Instant::now();
    let vault = Vault::open(root, options())?;
    let cold = ms(t);
    let stats = vault.stats();
    say!(
        "[{label}] {} notes, {} folders, {} attachments, {:.1} MB",
        stats.notes,
        stats.folders,
        stats.attachments,
        stats.bytes as f64 / 1e6
    );
    say!(
        "[{label}] vault open without cache: {cold:.1} ms ({} notes parsed)",
        stats.reparsed
    );
    drop(vault);
    let t = Instant::now();
    let vault = Vault::open(root, options())?;
    let warm = ms(t);
    say!(
        "[{label}] vault open with cache: {warm:.1} ms ({} notes parsed)",
        vault.stats().reparsed
    );

    let t = Instant::now();
    let index = SearchIndex::open(vault.clone(), &cache, &|_| {})?;
    let build = ms(t);
    let t = Instant::now();
    index.persist()?;
    let persist = ms(t);
    let s = index.stats();
    say!(
        "[{label}] index build (written and mapped included): {build:.1} ms; persist {persist:.1} ms; {} trigrams; {:.1} MB on disk; {:.1} MB held in memory",
        s.trigrams,
        s.bytes_on_disk as f64 / 1e6,
        s.bytes_in_memory as f64 / 1e6
    );
    drop(index);
    let t = Instant::now();
    let index = SearchIndex::open(vault.clone(), &cache, &|_| {})?;
    say!(
        "[{label}] index load from cache: {:.1} ms ({} notes reindexed)",
        ms(t),
        index.stats().reindexed
    );

    let qs = queries(&vault);
    let options = SearchOptions::default();
    let mut times = Vec::new();
    let mut files = 0usize;
    let mut truncated = 0usize;
    let mut partial = 0usize;
    for q in &qs {
        let r = index.search(&parse_query(q), &options, &CancelToken::new());
        times.push(r.elapsed_ms as f64);
        files += r.total_files;
        truncated += r.truncated as usize;
        partial += r.totals_are_minimums as usize;
    }
    say!(
        "[{label}] full text search, 200 terms of 4+ characters from titles: {}; {:.0} notes found on average, {truncated} lists cut at {}, {partial} with lower-bound totals",
        summary(times),
        files as f64 / qs.len() as f64,
        options.max_files
    );

    let typed = [
        "kerberoast",
        "privesc linux",
        "active directory",
        "golden ticket",
        "élévation",
        "nmap scan",
        "reverse shell",
        "web sqli",
        "token",
        "index",
    ];
    let mut keystrokes = Vec::new();
    for word in typed {
        let mut prefix = String::new();
        for c in word.chars() {
            prefix.push(c);
            let t = Instant::now();
            let hits = index.fuzzy(&prefix, FuzzyScope::Notes, &[], 50);
            keystrokes.push(ms(t));
            std::hint::black_box(hits);
        }
    }
    say!(
        "[{label}] palette fuzzy search, per keystroke: {}",
        summary(keystrokes)
    );
    say!(
        "[{label}] resident memory after the builds above (steady state: memprobe): {:.1} MB (peak {:.1} MB)",
        status_kb("VmRSS:") as f64 / 1024.0,
        status_kb("VmHWM:") as f64 / 1024.0
    );
    Ok(())
}

fn parse_bench() {
    let mut text = String::new();
    let mut i = 0;
    while text.len() < 100_000 {
        text.push_str(&vaultgen::note_text(i, 1000));
        i += 1;
    }
    let mut times = Vec::new();
    for _ in 0..50 {
        let t = Instant::now();
        std::hint::black_box(focus_core::parse_meta(&text));
        times.push(ms(t));
    }
    say!(
        "parse_meta of a {:.0} KB note: {}",
        text.len() as f64 / 1000.0,
        summary(times)
    );
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--measure") {
        let [_, path, label] = args.as_slice() else {
            anyhow::bail!("--measure <vault> <label>")
        };
        return measure(Path::new(path), label);
    }
    let count = match args.iter().position(|a| a == "--count") {
        Some(i) => args
            .get(i + 1)
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| anyhow::anyhow!("--count expects a number"))?,
        None => 100_000,
    };
    let skip_private = args.iter().any(|a| a == "--skip-private");
    say!(
        "focus bench, {} rayon threads",
        rayon::current_num_threads()
    );
    parse_bench();

    let exe = std::env::current_exe()?;
    let private = std::env::var_os("FOCUS_REAL_VAULT").map(PathBuf::from);
    if let Some(private) = private.filter(|p| !skip_private && p.join(".obsidian").exists()) {
        let status = Command::new(&exe)
            .arg("--measure")
            .arg(&private)
            .arg("private")
            .status()?;
        anyhow::ensure!(status.success(), "private vault measurement failed");
    }
    let generated = target_dir().join(format!("bench-vault-{count}"));
    let t = Instant::now();
    if vaultgen::generate(&generated, count)? {
        say!(
            "generated vault: {count} notes in {:.1} s",
            t.elapsed().as_secs_f64()
        );
    }
    let status = Command::new(&exe)
        .arg("--measure")
        .arg(&generated)
        .arg(format!("generated-{count}"))
        .status()?;
    anyhow::ensure!(status.success(), "generated vault measurement failed");
    Ok(())
}
