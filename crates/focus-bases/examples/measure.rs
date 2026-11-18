use std::path::Path;
use std::time::{Duration, Instant};

use focus_core::{OpenOptions, Vault};

const RUNS: usize = 9;

fn timed<T>(mut run: impl FnMut() -> T) -> (T, Duration, Duration) {
    let mut times = Vec::with_capacity(RUNS);
    let mut last = None;
    for _ in 0..RUNS {
        let started = Instant::now();
        last = Some(run());
        times.push(started.elapsed());
    }
    times.sort();
    (
        last.expect("RUNS is not zero"),
        times[RUNS / 2],
        times[RUNS - 1],
    )
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(root), Some(base), Some(cache)) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: measure <vault> <base file> <cache dir>");
        std::process::exit(2);
    };
    let started = Instant::now();
    let vault = Vault::open(
        Path::new(&root),
        OpenOptions {
            cache_dir: cache.into(),
            ignore: Vec::new(),
            watch: false,
        },
    )
    .expect("open the vault");
    println!(
        "open: {} notes in {:?}",
        vault.stats().notes,
        started.elapsed()
    );
    let source = std::fs::read_to_string(&base).expect("read the base file");
    let loaded = focus_bases::Base::parse(&source).expect("parse the base");
    for (index, view) in loaded.views.iter().enumerate() {
        let name = view.name.clone().unwrap_or_default();
        let (result, median, max) = timed(|| loaded.evaluate_at(&vault, index, None));
        match result {
            Ok(result) => println!(
                "view {name:?}: evaluate: {} rows of {} matched, {} groups, median {median:?}, max {max:?} over {RUNS} runs",
                result.rows.len(),
                result.matched,
                result.groups.len(),
            ),
            Err(error) => println!("view {name:?}: evaluate: {error}"),
        }
        let (table, median, max) = timed(|| loaded.table_at(&vault, index, None));
        match table {
            Ok(table) => println!(
                "view {name:?}: table: {} rows of {} matched, {} groups, median {median:?}, max {max:?} over {RUNS} runs",
                table.groups.iter().map(|g| g.rows.len()).sum::<usize>(),
                table.matched,
                table.groups.len(),
            ),
            Err(error) => println!("view {name:?}: table: {error}"),
        }
    }
}
