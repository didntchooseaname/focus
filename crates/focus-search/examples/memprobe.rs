use focus_core::{OpenOptions, Vault};
use focus_search::{CancelToken, FuzzyScope, SearchIndex, SearchOptions, parse_query};
use std::io::Write as _;
use std::path::PathBuf;

macro_rules! say {
    ($($t:tt)*) => {
        if writeln!(std::io::stdout().lock(), $($t)*).is_err() {
            std::process::exit(0);
        }
    };
}

fn rss_mb() -> f64 {
    // SAFETY: `malloc_trim` returns free glibc arena memory to the system and does not
    // touch memory in use.
    unsafe {
        libc::malloc_trim(0);
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
        / 1024.0
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("usage: memprobe <vault>"))?,
    );
    let label = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/memprobe-cache")
        .join(&label);
    rayon::broadcast(|_| ());
    say!(
        "[{label}] empty process, rayon threads started: {:.1} MB",
        rss_mb()
    );
    if !cache.join("meta.bin").exists() || !cache.join("search.bin").exists() {
        let vault = Vault::open(
            &root,
            OpenOptions {
                cache_dir: cache.clone(),
                ignore: Vec::new(),
                watch: false,
            },
        )?;
        let index = SearchIndex::open(vault.clone(), &cache, &|_| {})?;
        index.persist()?;
    }
    rayon::broadcast(|_| ());
    let base = rss_mb();
    say!("[{label}] baseline: {base:.1} MB");
    let vault = Vault::open(
        &root,
        OpenOptions {
            cache_dir: cache.clone(),
            ignore: Vec::new(),
            watch: false,
        },
    )?;
    let after_vault = rss_mb();
    say!(
        "[{label}] vault open (cache): +{:.1} MB",
        after_vault - base
    );
    let index = SearchIndex::open(vault.clone(), &cache, &|_| {})?;
    let after_index = rss_mb();
    say!(
        "[{label}] index open (cache, catalog included): +{:.1} MB",
        after_index - after_vault
    );
    let _ = index.fuzzy("ker", FuzzyScope::Notes, &[], 50);
    let r = index.search(
        &parse_query("kerberos"),
        &SearchOptions::default(),
        &CancelToken::new(),
    );
    std::hint::black_box(r);
    let after_use = rss_mb();
    say!(
        "[{label}] after one search and one palette keystroke: +{:.1} MB",
        after_use - after_index
    );
    say!(
        "[{label}] total: {after_use:.1} MB; index: {:?}",
        index.stats()
    );
    Ok(())
}
