use focus_core::{OpenOptions, Vault};
use std::io::Write as _;

macro_rules! say {
    ($($t:tt)*) => {
        if writeln!(std::io::stdout().lock(), $($t)*).is_err() {
            std::process::exit(0);
        }
    };
}
use focus_search::{CancelToken, FuzzyScope, SearchIndex, SearchOptions, SortOrder, parse_query};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn cache_dir(root: &Path) -> PathBuf {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut h: u64 = 0xcbf29ce484222325;
    for b in canonical.as_os_str().as_encoded_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/focus-cache")
        .join(format!("{h:016x}"))
}

fn mark(text: &str, ranges: &[std::ops::Range<usize>]) -> String {
    let mut out = String::new();
    let mut at = 0;
    for r in ranges {
        if r.start < at || r.end > text.len() {
            continue;
        }
        out.push_str(&text[at..r.start]);
        out.push('[');
        out.push_str(&text[r.clone()]);
        out.push(']');
        at = r.end;
    }
    out.push_str(&text[at..]);
    out
}

fn mark_chars(text: &str, positions: &[u32]) -> String {
    text.chars()
        .enumerate()
        .map(|(i, c)| {
            if positions.contains(&(i as u32)) {
                format!("[{c}]")
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut positional = Vec::new();
    let mut options = SearchOptions::default();
    let mut fuzzy = false;
    let mut explain = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--case" => options.case_sensitive = true,
            "--word" => options.whole_word = true,
            "--regex" => options.regex = true,
            "--fuzzy" => fuzzy = true,
            "--explain" => explain = true,
            "--budget" => {
                i += 1;
                options.budget_ms = args
                    .get(i)
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| anyhow::anyhow!("--budget expects milliseconds"))?;
            }
            "--max" => {
                i += 1;
                options.max_files = args
                    .get(i)
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| anyhow::anyhow!("--max expects a number"))?;
            }
            "--sort" => {
                i += 1;
                options.sort = match args.get(i).map(String::as_str) {
                    Some("path") => SortOrder::Path,
                    Some("newest") => SortOrder::ModifiedNewest,
                    Some("oldest") => SortOrder::ModifiedOldest,
                    Some("relevance") => SortOrder::Relevance,
                    other => anyhow::bail!("unknown --sort: {other:?}"),
                };
            }
            a => positional.push(a.to_string()),
        }
        i += 1;
    }
    let [vault_path, query] = positional.as_slice() else {
        anyhow::bail!(
            "usage: focusq <vault> \"<query>\" [--case] [--word] [--regex] [--max N] [--budget MS] [--sort relevance|path|newest|oldest] [--explain] [--fuzzy]"
        );
    };
    let root = PathBuf::from(vault_path);
    let cache = cache_dir(&root);

    let t = Instant::now();
    let vault = Vault::open(
        &root,
        OpenOptions {
            cache_dir: cache.clone(),
            ignore: Vec::new(),
            watch: false,
        },
    )?;
    let open_ms = t.elapsed().as_secs_f64() * 1000.0;
    let vs = vault.stats();
    let t = Instant::now();
    let index = SearchIndex::open(vault.clone(), &cache, &|_| {})?;
    let index_ms = t.elapsed().as_secs_f64() * 1000.0;
    let is = index.stats();
    say!(
        "vault: {} notes, opened in {open_ms:.1} ms ({} parsed); index: {} trigrams, {index_ms:.1} ms ({} notes reindexed)",
        vs.notes,
        vs.reparsed,
        is.trigrams,
        is.reindexed
    );

    if fuzzy {
        let t = Instant::now();
        let hits = index.fuzzy(query, FuzzyScope::Notes, &[], 20);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        say!("palette {query:?}: {} results in {ms:.2} ms", hits.len());
        for h in hits {
            let path = h
                .note
                .and_then(|n| vault.note(n))
                .map(|m| m.path.to_string_lossy().into_owned())
                .unwrap_or_default();
            say!(
                "{:>5}  {}  ({path})",
                h.score,
                mark_chars(&h.text, &h.positions)
            );
        }
    } else {
        let parsed = parse_query(query);
        for d in &parsed.diagnostics {
            say!("diagnostic: {:?} at {:?}", d.kind, d.range);
        }
        let results = index.search(&parsed, &options, &CancelToken::new());
        let again = index.search(&parsed, &options, &CancelToken::new());
        say!(
            "{query:?}: {} notes, {} matches{}{}; {:.2} ms (second run {:.2} ms)",
            results.total_files,
            results.total_matches,
            if results.totals_are_minimums {
                " (lower bounds: time budget exceeded)"
            } else {
                ""
            },
            if results.truncated {
                format!(", {} shown", results.hits.len())
            } else {
                String::new()
            },
            results.elapsed_ms,
            again.elapsed_ms
        );
        for h in &results.hits {
            let Some(meta) = vault.note(h.note) else {
                continue;
            };
            say!(
                "\n{:>6.1}  {}  ({})",
                h.score,
                meta.path.display(),
                mark(&meta.title, &h.title_ranges)
            );
            if explain {
                let parts: Vec<String> = h
                    .explain
                    .iter()
                    .filter(|(_, v)| *v != 0.0)
                    .map(|(k, v)| format!("{k} {v:.2}"))
                    .collect();
                say!("        score: {}", parts.join(", "));
            }
            for m in &h.matches {
                say!("        {:>5}: {}", m.line + 1, mark(&m.text, &m.ranges));
            }
            if h.match_count as usize > h.matches.len() {
                say!("        ({} matches in total)", h.match_count);
            }
        }
    }
    index.persist()?;
    Ok(())
}
