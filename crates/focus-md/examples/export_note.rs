use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use focus_md::export::{HtmlOptions, Labels, Resolver, html_to_pdf, to_html};
use focus_md::mermaid;

struct VaultResolver {
    root: PathBuf,
    note_dir: PathBuf,
    files: HashMap<String, PathBuf>,
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                walk(&path, out);
            }
        } else {
            out.push(path);
        }
    }
}

impl VaultResolver {
    fn new(root: &Path, note: &Path) -> Self {
        let mut all = Vec::new();
        walk(root, &mut all);
        let mut files = HashMap::new();
        for path in all {
            if let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_lowercase()) {
                files.entry(name).or_insert(path);
            }
        }
        VaultResolver {
            root: root.to_path_buf(),
            note_dir: note.parent().unwrap_or(root).to_path_buf(),
            files,
        }
    }

    fn note(&self, target: &str) -> Option<&PathBuf> {
        let name = target.rsplit('/').next().unwrap_or(target).to_lowercase();
        self.files
            .get(&format!("{name}.md"))
            .or_else(|| self.files.get(&name))
    }
}

impl Resolver for VaultResolver {
    fn image(&self, src: &str) -> Option<PathBuf> {
        let direct = [self.note_dir.join(src), self.root.join(src)];
        direct.into_iter().find(|p| p.is_file()).or_else(|| {
            self.files
                .get(&src.rsplit('/').next().unwrap_or(src).to_lowercase())
                .cloned()
        })
    }

    fn link(&self, target: &str, anchor: Option<&str>) -> Option<String> {
        let path = self.note(target)?;
        let stem = path.file_stem()?.to_string_lossy().replace(' ', "%20");
        Some(match anchor {
            Some(a) => format!("{stem}.html#{}", a.to_lowercase().replace(' ', "-")),
            None => format!("{stem}.html"),
        })
    }

    fn embed(&self, target: &str) -> Option<String> {
        std::fs::read_to_string(self.note(target)?).ok()
    }
}

fn chromium() -> Option<PathBuf> {
    ["chromium", "google-chrome-stable", "brave"]
        .iter()
        .find_map(|n| {
            std::env::split_paths(&std::env::var_os("PATH")?)
                .map(|d| d.join(n))
                .find(|p| p.is_file())
        })
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [vault, note, out_dir, rest @ ..] = args.as_slice() else {
        anyhow::bail!("usage: export_note <vault> <note.md> <output dir> [language]");
    };
    let lang = rest.first().cloned().unwrap_or_else(|| "en".into());
    let vault = Path::new(vault).canonicalize()?;
    let note_path = vault.join(note);
    let out_dir = Path::new(out_dir);
    std::fs::create_dir_all(out_dir)?;
    let fonts_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/");
    let fonts = [
        "Inter-Regular.ttf",
        "Inter-Medium.ttf",
        "Inter-SemiBold.ttf",
        "Inter-Bold.ttf",
        "NotoSansArabic-Regular.ttf",
        "NotoSansArabic-Medium.ttf",
        "NotoSansArabic-SemiBold.ttf",
    ]
    .iter()
    .map(|n| std::fs::read(format!("{fonts_dir}{n}")))
    .collect::<Result<Vec<_>, _>>()?;
    mermaid::init_fonts(&fonts.iter().map(Vec::as_slice).collect::<Vec<_>>());
    let source = std::fs::read_to_string(&note_path)?;
    let resolver = VaultResolver::new(&vault, &note_path);
    let stem = note_path
        .file_stem()
        .map(|s| s.to_string_lossy().replace(' ', "_"))
        .unwrap_or_default();
    let browser = chromium();
    for (dark, name) in [(true, "dark"), (false, "light")] {
        let options = HtmlOptions {
            title: note_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            dark,
            lang: lang.clone(),
            labels: Labels::default(),
            ..HtmlOptions::default()
        };
        let started = std::time::Instant::now();
        let html = to_html(&source, &options, &resolver);
        let html_file = out_dir.join(format!("{stem}_{name}.html"));
        std::fs::write(&html_file, &html)?;
        println!(
            "{}: {} KiB, {:.0} ms",
            html_file.display(),
            html.len() / 1024,
            started.elapsed().as_secs_f64() * 1e3
        );
        if let Some(browser) = &browser {
            let png = out_dir.join(format!("{stem}_{name}.png"));
            let profile = out_dir.join(".profile");
            let status = Command::new(browser)
                .args([
                    "--headless=new",
                    "--disable-gpu",
                    "--no-sandbox",
                    "--hide-scrollbars",
                    "--window-size=1200,4600",
                    "--force-device-scale-factor=1",
                ])
                .arg(format!("--user-data-dir={}", profile.display()))
                .arg(format!("--screenshot={}", png.display()))
                .arg(format!("file://{}", html_file.canonicalize()?.display()))
                .output()?;
            println!(
                "  capture: {} ({})",
                png.display(),
                if status.status.success() {
                    "ok"
                } else {
                    "failed"
                }
            );
        }
    }
    let pdf = out_dir.join(format!("{stem}.pdf"));
    let html_for_pdf = out_dir.join(format!("{stem}_light.html"));
    match html_to_pdf(&html_for_pdf, &pdf)? {
        true => println!(
            "pdf: {} ({} bytes)",
            pdf.display(),
            std::fs::metadata(&pdf)?.len()
        ),
        false => println!("pdf: no Chromium browser found"),
    }
    let _ = std::fs::remove_dir_all(out_dir.join(".profile"));
    Ok(())
}
