use focus_history::{History, HistoryConfig, Kind, diff};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/focus-history-bench");
    let _ = std::fs::remove_dir_all(&dir);
    let history = History::open(&dir.join("store"), HistoryConfig::default()).unwrap();

    let big: String = (0..10_000)
        .map(|i| format!("line {i} of the large note, with some text to fill it out\n"))
        .collect();
    let started = Instant::now();
    for i in 0..20 {
        let content = format!("{big}{i}");
        history
            .keep(Path::new(&format!("big{i}.md")), Kind::Bulk, || {
                Ok(content.into_bytes())
            })
            .unwrap();
    }
    println!(
        "keep, 10,000 lines ({} KB), version stored: {:.2} ms each",
        big.len() / 1024,
        ms(started) / 20.0
    );

    let started = Instant::now();
    for _ in 0..10_000 {
        let kept = history
            .keep(Path::new("big0.md"), Kind::Edit, || unreachable!())
            .unwrap();
        assert!(kept.is_none());
    }
    println!(
        "keep, Edit within the interval (nothing stored): {:.1} us each",
        ms(started) * 1000.0 / 10_000.0
    );

    let started = Instant::now();
    for note in 0..2_000 {
        for version in 0..10 {
            let content = format!("note {note} version {version} {}", "x".repeat(2_000));
            let rel = format!("dir{}/n{note}.md", note % 20);
            history
                .keep(Path::new(&rel), Kind::Bulk, || Ok(content.into_bytes()))
                .unwrap();
        }
    }
    println!(
        "keep, 20,000 small versions of 2,000 notes: {:.2} s ({:.0} us each)",
        ms(started) / 1000.0,
        ms(started) * 1000.0 / 20_000.0
    );

    let started = Instant::now();
    let report = history.prune().unwrap();
    println!(
        "prune, 20,020 versions, nothing to drop: {:.0} ms ({} dropped)",
        ms(started),
        report.versions
    );
    println!("store size: {} MB", history.size().unwrap() / 1_000_000);

    let started = Instant::now();
    history
        .move_folder(Path::new("dir3"), Path::new("moved/dir3"))
        .unwrap();
    println!("move_folder, 100 notes of 2,000: {:.0} ms", ms(started));
    let started = Instant::now();
    history
        .move_note(Path::new("dir4/n4.md"), Path::new("n4.md"))
        .unwrap();
    println!("move_note: {:.2} ms", ms(started));

    let few: String = big
        .lines()
        .enumerate()
        .map(|(i, l)| {
            if i % 100 == 0 {
                format!("edited {i}\n")
            } else {
                format!("{l}\n")
            }
        })
        .collect();
    let started = Instant::now();
    let lines = diff(&big, &few);
    println!(
        "diff, 10,000 lines, 100 changed: {:.1} ms ({} lines)",
        ms(started),
        lines.len()
    );
    let other: String = (0..10_000).map(|i| format!("other {i}\n")).collect();
    let started = Instant::now();
    let lines = diff(&big, &other);
    println!(
        "diff, 10,000 lines, all different: {:.1} ms ({} lines)",
        ms(started),
        lines.len()
    );
}
