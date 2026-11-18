use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use focus_md::highlight::{Detected, Hints, canonical, detect, detect_in_note};
use focus_md::{Block, BlockKind, ParseOptions, parse};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                collect(&path, out)?;
            }
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    Ok(())
}

fn blocks(list: &[Block], out: &mut Vec<(String, String)>) {
    for block in list {
        match &block.kind {
            BlockKind::CodeBlock { lang, code, .. } => out.push((lang.clone(), code.clone())),
            BlockKind::List { items, .. } => items.iter().for_each(|i| blocks(&i.blocks, out)),
            BlockKind::Quote(b)
            | BlockKind::Details { blocks: b, .. }
            | BlockKind::Callout { blocks: b, .. } => blocks(b, out),
            _ => {}
        }
    }
}

struct Sample {
    note: String,
    truth: Option<&'static str>,
    declared: String,
    code: String,
    others: Vec<String>,
}

const SHELL_FAMILY: [&str; 4] = ["bash", "powershell", "cmd", "console"];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn pick<T: Clone>(items: &[T], n: usize, seed: u64) -> Vec<T> {
    let mut rng = Rng(seed | 1);
    let mut indices: Vec<usize> = (0..items.len()).collect();
    let mut chosen = Vec::new();
    while chosen.len() < n && !indices.is_empty() {
        let at = (rng.next() % indices.len() as u64) as usize;
        chosen.push(items[indices.swap_remove(at)].clone());
    }
    chosen
}

fn run(sample: &Sample, with_context: bool) -> Option<Detected> {
    if with_context {
        let others: Vec<&str> = sample.others.iter().map(String::as_str).collect();
        detect_in_note(
            &sample.code,
            &Hints {
                languages: &others,
                path: Some(&sample.note),
            },
        )
    } else {
        detect(&sample.code)
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage : detect_eval <coffre> [options]"))?
        .clone();
    let flag = |name: &str| args.iter().position(|a| a == name);
    let number = |name: &str| {
        flag(name)
            .and_then(|i| args.get(i + 1))
            .and_then(|n| n.parse::<usize>().ok())
    };
    let seed = flag("--seed")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse().ok())
        .unwrap_or(7u64);

    let mut files = Vec::new();
    collect(Path::new(&root), &mut files)?;
    let mut samples: Vec<Sample> = Vec::new();
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut found = Vec::new();
        blocks(&parse(&text, &ParseOptions::default()).blocks, &mut found);
        let note = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        let labelled: Vec<&String> = found
            .iter()
            .map(|(l, _)| l)
            .filter(|l| !l.is_empty())
            .collect();
        for (i, (declared, code)) in found.iter().enumerate() {
            let truth = if declared.is_empty() {
                None
            } else {
                canonical(declared).filter(|t| *t != "text")
            };
            if !declared.is_empty() && truth.is_none() {
                continue;
            }
            let others = found
                .iter()
                .enumerate()
                .filter(|(j, (l, _))| *j != i && !l.is_empty())
                .map(|(_, (l, _))| l.clone())
                .collect();
            let _ = &labelled;
            samples.push(Sample {
                note: note.clone(),
                truth,
                declared: declared.clone(),
                code: code.clone(),
                others,
            });
        }
    }
    let labelled: Vec<&Sample> = samples.iter().filter(|s| s.truth.is_some()).collect();
    let unlabelled: Vec<&Sample> = samples.iter().filter(|s| s.truth.is_none()).collect();
    println!(
        "labelled blocks: {}, unlabelled: {}",
        labelled.len(),
        unlabelled.len()
    );

    for with_context in [false, true] {
        let mut matrix: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        let started = std::time::Instant::now();
        for s in &labelled {
            let predicted = run(s, with_context).map_or("-", |d| d.lang);
            *matrix
                .entry((s.truth.unwrap_or("?"), predicted))
                .or_default() += 1;
        }
        let elapsed = started.elapsed();
        let total = labelled.len() as f64;
        let share = |keep: &dyn Fn(&str, &str) -> bool| -> f64 {
            100.0
                * matrix
                    .iter()
                    .filter(|&(&(t, p), _)| keep(t, p))
                    .map(|(_, n)| *n)
                    .sum::<usize>() as f64
                / total
        };
        let family = |l: &str| SHELL_FAMILY.contains(&l);
        println!(
            "\n== {} context ({:.0} ms)",
            if with_context { "with" } else { "without" },
            elapsed.as_secs_f64() * 1e3
        );
        println!("correct: {:.1} %", share(&|t, p| t == p));
        println!(
            "wrong outside the shell family: {:.1} %",
            share(&|t, p| t != p && p != "-" && !(family(t) && family(p)))
        );
        println!(
            "confusions inside the shell family: {:.1} %",
            share(&|t, p| t != p && family(t) && family(p))
        );
        println!("no detection: {:.1} %", share(&|_, p| p == "-"));
        let count = |lang: &str| {
            matrix
                .iter()
                .filter(|&(&(t, _), _)| t == lang)
                .map(|(_, n)| *n)
                .sum::<usize>()
        };
        let bash = count("bash");
        let bash_none = matrix.get(&("bash", "-")).copied().unwrap_or(0);
        println!(
            "bash: {:.1} % without detection ({bash_none} of {bash})",
            100.0 * bash_none as f64 / bash.max(1) as f64
        );
        if with_context {
            let mut per: BTreeMap<&str, (usize, usize, usize, usize)> = BTreeMap::new();
            for (&(t, p), &n) in &matrix {
                let e = per.entry(t).or_default();
                e.0 += n;
                if t == p {
                    e.1 += n;
                } else if p == "-" {
                    e.3 += n;
                } else if family(t) && family(p) {
                    e.2 += n;
                }
            }
            let mut ranked: Vec<_> = per.iter().collect();
            ranked.sort_by_key(|(_, e)| std::cmp::Reverse(e.0));
            println!(
                "\n{:<12}{:>7}{:>9}{:>12}{:>9}  (wrong outside the family = remainder)",
                "language", "blocks", "correct", "internal", "none"
            );
            for (lang, (n, ok, internal, none)) in ranked.iter().take(14) {
                let outside = n - ok - internal - none;
                println!(
                    "{:<12}{:>7}{:>8.1}%{:>11.1}%{:>8.1}%  wrong {:.1} %",
                    lang,
                    n,
                    100.0 * *ok as f64 / *n as f64,
                    100.0 * *internal as f64 / *n as f64,
                    100.0 * *none as f64 / *n as f64,
                    100.0 * outside as f64 / *n as f64
                );
            }
            let mut confusions: Vec<_> = matrix
                .iter()
                .filter(|&(&(t, p), &n)| t != p && n >= 8)
                .collect();
            confusions.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
            println!("\nconfusions (truth to detected), at least 8:");
            for (&(t, p), n) in confusions {
                println!("  {t} vers {p} : {n}");
            }
        }
    }

    if let Some(n) = number("--false") {
        let wrong: Vec<&&Sample> = labelled
            .iter()
            .filter(|s| run(s, true).is_some_and(|d| Some(d.lang) != s.truth))
            .collect();
        println!("\n#### {n} wrong detections drawn from {}", wrong.len());
        for (i, s) in pick(&wrong, n, seed).iter().enumerate() {
            let d = run(s, true).map_or("-", |d| d.lang);
            println!(
                "\n--- {} | {} vers {d} | {}\n{}",
                i + 1,
                s.truth.unwrap_or("?"),
                s.note.rsplit('/').next().unwrap_or(""),
                s.code
                    .lines()
                    .take(6)
                    .map(|l| l.chars().take(110).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
    if let Some(n) = number("--unlabelled") {
        let mut distribution: BTreeMap<&str, usize> = BTreeMap::new();
        let mut detected = Vec::new();
        for s in &unlabelled {
            let d = run(s, true);
            *distribution.entry(d.map_or("-", |d| d.lang)).or_default() += 1;
            detected.push((s, d));
        }
        let mut ranked: Vec<_> = distribution.into_iter().collect();
        ranked.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        println!("\n#### unlabelled blocks: {}", unlabelled.len());
        for (lang, n) in &ranked {
            println!(
                "  {lang:<12}{n:>6}  {:.1} %",
                100.0 * *n as f64 / unlabelled.len() as f64
            );
        }
        for (i, (s, d)) in pick(&detected, n, seed).iter().enumerate() {
            println!(
                "\n--- {} | note {} | detected {}\n{}",
                i + 1,
                s.note,
                d.map_or("-", |d| d.lang),
                s.code.lines().take(7).collect::<Vec<_>>().join("\n")
            );
        }
    }
    if let Some(i) = flag("--pair") {
        let spec = args.get(i + 1).cloned().unwrap_or_default();
        let n = args.get(i + 2).and_then(|n| n.parse().ok()).unwrap_or(5);
        let (truth, predicted) = spec.split_once(':').unwrap_or(("", ""));
        let matching: Vec<&&Sample> = labelled
            .iter()
            .filter(|s| s.truth == Some(truth) && run(s, true).map_or("-", |d| d.lang) == predicted)
            .collect();
        for s in pick(&matching, n, seed) {
            println!(
                "\n--- {} | {} detected {predicted}\n{}",
                s.note,
                s.declared,
                s.code.lines().take(8).collect::<Vec<_>>().join("\n")
            );
        }
    }
    Ok(())
}
