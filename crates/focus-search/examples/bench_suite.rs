#[path = "support/vaultgen.rs"]
mod vaultgen;

use focus_core::{OpenOptions, Vault};
use focus_search::{CancelToken, FuzzyScope, SearchIndex, SearchOptions, parse_query};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Measure {
    id: String,
    area: &'static str,
    unit: &'static str,
    values: Vec<f64>,
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn status_kb(field: &str) -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find(|l| l.starts_with(field))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn dir_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| {
            e.metadata()
                .map(|m| {
                    if m.is_dir() {
                        dir_bytes(&e.path())
                    } else {
                        m.len()
                    }
                })
                .unwrap_or(0)
        })
        .sum()
}

fn options(cache: &Path) -> OpenOptions {
    OpenOptions {
        cache_dir: cache.to_path_buf(),
        ignore: Vec::new(),
        watch: false,
    }
}

fn json_escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            c if (c as u32) < 0x20 => vec![' '],
            c => vec![c],
        })
        .collect()
}

fn render(measures: &[Measure], notes: usize, command: &str) -> String {
    let mut out = String::from("[\n");
    for (i, m) in measures.iter().enumerate() {
        let mut v = m.values.clone();
        v.sort_by(f64::total_cmp);
        out.push_str(&format!(
            "  {{\"id\": \"{}\", \"area\": \"{}\", \"vault_notes\": {notes}, \"unit\": \"{}\", \"runs\": {}, \"min\": {:.4}, \"median\": {:.4}, \"p95\": {:.4}, \"p99\": {:.4}, \"max\": {:.4}, \"command\": \"{}\"}}{}\n",
            m.id, m.area, m.unit, v.len(), v.first().copied().unwrap_or(0.0), percentile(&v, 0.5), percentile(&v, 0.95), percentile(&v, 0.99), v.last().copied().unwrap_or(0.0), json_escape(command),
            if i + 1 < measures.len() { "," } else { "" }
        ));
    }
    out.push(']');
    out
}

fn words_of(vault: &Vault) -> (Vec<String>, Vec<Vec<String>>) {
    let titles: Vec<Vec<String>> = vault
        .notes()
        .iter()
        .map(|m| {
            m.title
                .split(|c: char| !c.is_alphanumeric())
                .map(str::to_lowercase)
                .filter(|w| w.chars().count() >= 4 && !w.chars().all(|c| c.is_ascii_digit()))
                .collect()
        })
        .collect();
    let mut words: Vec<String> = titles.iter().flatten().cloned().collect();
    words.sort();
    words.dedup();
    (words, titles)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, notes, mode, out, command] = args.as_slice() else {
        anyhow::bail!("usage: bench_suite <vault> <notes> <quick|full> <out.json> <command>")
    };
    let root = PathBuf::from(root);
    let notes: usize = notes.parse()?;
    let quick = mode == "quick";
    let (build_runs, load_runs, update_runs, rename_runs, samples) = if quick {
        (3, 3, 10, 3, 200)
    } else {
        (10, 10, 30, 10, 1000)
    };
    vaultgen::generate(&root, notes)?;
    let cache = root
        .join("../bench-cache")
        .join(root.file_name().unwrap_or_default());
    let mut m: Vec<Measure> = Vec::new();
    let mut add = |id: &str, area: &'static str, unit: &'static str, values: Vec<f64>| {
        m.push(Measure {
            id: id.to_string(),
            area,
            unit,
            values,
        })
    };

    let (mut open_cold, mut build, mut rate_notes, mut rate_mib) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut peak_mib, mut cache_mib, mut corpus_mib) = (0.0, 0.0, 0.0);
    for run in 0..build_runs {
        let _ = std::fs::remove_dir_all(&cache);
        let t = Instant::now();
        let vault = Vault::open(&root, options(&cache))?;
        let opened = ms(t);
        let t = Instant::now();
        let index = SearchIndex::open(vault.clone(), &cache, &|_| {})?;
        let built = ms(t);
        if run == 0 {
            peak_mib = status_kb("VmHWM:") / 1024.0;
            corpus_mib = vault.stats().bytes as f64 / 1_048_576.0;
        }
        index.persist()?;
        drop(index);
        drop(vault);
        if run == 0 {
            cache_mib = dir_bytes(&cache) as f64 / 1_048_576.0;
        }
        open_cold.push(opened);
        build.push(built);
        rate_notes.push(notes as f64 / ((opened + built) / 1000.0));
        rate_mib.push(corpus_mib / ((opened + built) / 1000.0));
    }
    add("vault.open.no_cache", "index", "ms", open_cold);
    add("index.build", "index", "ms", build);
    add("index.build.notes_per_s", "index", "notes/s", rate_notes);
    add("index.build.mib_per_s", "index", "MiB/s", rate_mib);
    add("index.build.peak_rss", "index", "MiB", vec![peak_mib]);
    add("index.cache_size", "index", "MiB", vec![cache_mib]);
    add("vault.corpus_size", "index", "MiB", vec![corpus_mib]);

    let (mut open_warm, mut load) = (Vec::new(), Vec::new());
    for _ in 0..load_runs {
        let t = Instant::now();
        let vault = Vault::open(&root, options(&cache))?;
        open_warm.push(ms(t));
        let t = Instant::now();
        let index = SearchIndex::open(vault.clone(), &cache, &|_| {})?;
        load.push(ms(t));
        drop(index);
        drop(vault);
    }
    add("vault.open.with_cache", "index", "ms", open_warm);
    add("index.load", "index", "ms", load);

    let vault = Vault::open(&root, options(&cache))?;
    let index = SearchIndex::open(vault.clone(), &cache, &|_| {})?;
    let events = vault.subscribe();
    let all = vault.notes();
    let mut rng = vaultgen::Rng::new(7);

    let mut update = Vec::new();
    for n in 0..update_runs {
        let meta = rng.pick(&all).clone();
        let token = format!("zzupdate{n}token");
        let text = vault.read(meta.id)?;
        let t = Instant::now();
        vault.write(meta.id, &format!("{text}\n{token}\n"), None)?;
        let event = events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| anyhow::anyhow!("no vault event after a write"))?;
        index.apply(&event);
        let found = index.search(
            &parse_query(&token),
            &SearchOptions::default(),
            &CancelToken::new(),
        );
        anyhow::ensure!(
            found.total_files == 1,
            "the new term was not found after the update"
        );
        update.push(ms(t));
        while events.try_recv().is_ok() {}
    }
    add("index.update_after_write", "index", "ms", update);

    let target = all
        .iter()
        .take(2000)
        .max_by_key(|n| vault.linked_from(n.id).len())
        .cloned()
        .expect("a note");
    let from = target.path.to_path_buf();
    let to = from.with_file_name("Renamed bench note.md");
    let mut rename = Vec::new();
    let mut links = 0usize;
    for _ in 0..rename_runs {
        let t = Instant::now();
        let report = vault.rename(&from, &to, true)?;
        rename.push(ms(t));
        links = report.updated_links;
        vault.rename(&to, &from, true)?;
        while events.try_recv().is_ok() {}
    }
    add("note.rename_with_links", "index", "ms", rename);
    add(
        "note.rename_with_links.links_rewritten",
        "index",
        "links",
        vec![links as f64],
    );
    drop(events);

    let (words, titles) = words_of(&vault);
    let sample_words = |rng: &mut vaultgen::Rng, n: usize| {
        (0..n).map(|_| rng.pick(&words).clone()).collect::<Vec<_>>()
    };
    let phrase_titles: Vec<&Vec<String>> = titles.iter().filter(|t| t.len() >= 2).collect();
    let mut classes: Vec<(&str, Vec<String>)> = vec![
        ("one_term", sample_words(&mut rng, samples)),
        (
            "two_terms",
            (0..samples)
                .map(|_| format!("{} {}", rng.pick(&words), rng.pick(&words)))
                .collect(),
        ),
        (
            "exact_phrase",
            (0..samples)
                .map(|_| {
                    let t = rng.pick(&phrase_titles);
                    format!("\"{} {}\"", t[0], t[1])
                })
                .collect(),
        ),
        (
            "tag_filter",
            (0..samples)
                .map(|_| format!("tag:#{}", rng.pick(vaultgen::TAGS)))
                .collect(),
        ),
        (
            "path_filter",
            (0..samples)
                .map(|_| format!("path:{} {}", rng.pick(vaultgen::DIRS), rng.pick(&words)))
                .collect(),
        ),
        (
            "no_result",
            (0..samples).map(|i| format!("qzxv{i}wkjh")).collect(),
        ),
    ];
    let options = SearchOptions::default();
    for (class, queries) in classes.drain(..) {
        let mut times = Vec::with_capacity(queries.len());
        let mut found = 0usize;
        for q in &queries {
            let t = Instant::now();
            let r = index.search(&parse_query(q), &options, &CancelToken::new());
            times.push(ms(t));
            found += r.hits.len();
            std::hint::black_box(&r);
        }
        add(&format!("search.{class}"), "search", "ms", times);
        add(
            &format!("search.{class}.hits_returned_mean"),
            "search",
            "notes",
            vec![found as f64 / queries.len() as f64],
        );
    }

    let (mut fuzzy, mut typed, mut typo) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..(samples / 8).max(25) {
        let word = rng.pick(&words).clone();
        let mut prefix = String::new();
        for c in word.chars() {
            prefix.push(c);
            let t = Instant::now();
            std::hint::black_box(index.fuzzy(&prefix, FuzzyScope::Notes, &[], 50));
            fuzzy.push(ms(t));
            if prefix.chars().count() >= 3 {
                let t = Instant::now();
                std::hint::black_box(index.search(
                    &parse_query(&prefix),
                    &options,
                    &CancelToken::new(),
                ));
                typed.push(ms(t));
            }
        }
        let mut swapped: Vec<char> = word.chars().collect();
        if swapped.len() >= 4 {
            swapped.swap(1, 2);
            let t = Instant::now();
            std::hint::black_box(index.fuzzy(
                &swapped.iter().collect::<String>(),
                FuzzyScope::Notes,
                &[],
                50,
            ));
            typo.push(ms(t));
        }
    }
    add("search.typing.palette_per_keystroke", "search", "ms", fuzzy);
    add(
        "search.typing.fulltext_per_keystroke",
        "search",
        "ms",
        typed,
    );
    add(
        "search.typo.palette_two_letters_swapped",
        "search",
        "ms",
        typo,
    );

    // SAFETY: returns free glibc arena memory to the system; memory in use is untouched.
    unsafe {
        libc::malloc_trim(0);
    }
    add(
        "index.steady_rss",
        "index",
        "MiB",
        vec![status_kb("VmRSS:") / 1024.0],
    );
    let _: Arc<Vault> = vault;
    std::fs::write(out, render(&m, notes, command))?;
    Ok(())
}
