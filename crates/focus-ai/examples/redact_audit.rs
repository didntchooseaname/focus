use focus_ai::redact::{Options, redact};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn collect_markdown(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_markdown(&path, found);
        } else if path.extension().is_some_and(|x| x == "md") {
            found.push(path);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(prefix) = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .cloned()
    else {
        eprintln!("usage: redact_audit <folder>... --out <prefix>");
        std::process::exit(2);
    };
    let mut files = Vec::new();
    for dir in args.iter().take_while(|a| *a != "--out") {
        collect_markdown(Path::new(dir), &mut files);
    }
    files.sort();
    let options = Options {
        secrets: true,
        ip_addresses: std::env::var("IPS").is_ok(),
        hostnames: std::env::var("HOSTS").is_ok(),
    };
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut samples: Vec<String> = Vec::new();
    let mut intact: Vec<String> = Vec::new();
    let mut dump = String::new();
    let mut total_bytes = 0usize;
    let started = std::time::Instant::now();
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        total_bytes += text.len();
        let redacted = redact(&text, &options, &[]);
        for (number, line) in redacted.text.lines().enumerate() {
            let _ = writeln!(dump, "{}:{}:{}", file.display(), number + 1, line);
        }
        for replacement in &redacted.replacements {
            *counts.entry(replacement.kind).or_default() += 1;
            if let Some(line) = redacted
                .text
                .lines()
                .find(|l| l.contains(&replacement.token))
            {
                samples.push(format!(
                    "{}\t{}\t{}\t{}",
                    file.display(),
                    replacement.kind,
                    replacement.original.chars().take(80).collect::<String>(),
                    line.chars().take(200).collect::<String>()
                ));
            }
        }
        for line in redacted.text.lines() {
            let lower = line.to_lowercase();
            if (lower.contains("password") || lower.contains("hash") || lower.contains("ntlm"))
                && !line.contains('‹')
            {
                intact.push(format!(
                    "{}\t{}",
                    file.display(),
                    line.chars().take(220).collect::<String>()
                ));
            }
        }
    }
    let mut report = String::new();
    let _ = writeln!(
        report,
        "files {} bytes {} duration {:?}",
        files.len(),
        total_bytes,
        started.elapsed()
    );
    for (kind, count) in &counts {
        let _ = writeln!(report, "{kind}\t{count}");
    }
    let _ = writeln!(report, "total\t{}", counts.values().sum::<usize>());
    let _ = writeln!(report, "candidate_intact_lines\t{}", intact.len());
    let outputs = [
        ("report", report),
        ("dump", dump),
        ("samples", samples.join("\n")),
        ("intact", intact.join("\n")),
    ];
    for (extension, content) in outputs {
        if let Err(e) = std::fs::write(format!("{prefix}.{extension}"), content) {
            eprintln!("cannot write {prefix}.{extension}: {e}");
            std::process::exit(1);
        }
    }
}
