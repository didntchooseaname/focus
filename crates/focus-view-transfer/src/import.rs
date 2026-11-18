use std::path::{Component, Path, PathBuf};

use anyhow::Context as _;
use focus_md::import::detect;
pub use focus_md::import::{Report, Source};

const SAMPLES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    Hidden,
    Symlink,
    UnsafePath,
    Unsupported,
    Special,
    TooLarge,
    TooDeep,
    Other(String),
}

pub fn skip_reason(text: &str) -> SkipReason {
    match text {
        "hidden entry" | "hidden or system entry" => SkipReason::Hidden,
        "symbolic link" => SkipReason::Symlink,
        "path leaves the destination" => SkipReason::UnsafePath,
        "unsupported file type" => SkipReason::Unsupported,
        "not a regular file" => SkipReason::Special,
        "file too large" => SkipReason::TooLarge,
        "folders nested too deep" | "archive nested too deep" => SkipReason::TooDeep,
        other => SkipReason::Other(other.to_string()),
    }
}

#[derive(Debug, Clone)]
pub struct Preview {
    pub source: Source,
    pub notes: usize,
    pub attachments: usize,
    pub skipped: Vec<(String, SkipReason)>,
    pub samples: Vec<String>,
}

pub fn detect_source(path: &Path) -> Option<Source> {
    detect(path)
}

fn unsafe_path(name: &str) -> bool {
    Path::new(name)
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
}

fn confine(vault_root: &Path, destination: &Path) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        !unsafe_path(&destination.to_string_lossy()),
        "the destination leaves the vault"
    );
    let base = vault_root.join(destination);
    let mut existing = base.as_path();
    while !existing.exists() {
        existing = existing.parent().unwrap_or(vault_root);
    }
    let real =
        dunce::canonicalize(existing).with_context(|| format!("reading {}", existing.display()))?;
    let root = dunce::canonicalize(vault_root)
        .with_context(|| format!("reading {}", vault_root.display()))?;
    anyhow::ensure!(real.starts_with(&root), "the destination leaves the vault");
    Ok(base)
}

pub fn preview(vault_root: &Path, path: &Path, destination: &Path) -> anyhow::Result<Preview> {
    let source = detect(path)
        .with_context(|| format!("{} is not something Focus can import", path.display()))?;
    let base = confine(vault_root, destination)?;
    let planned = focus_md::import::plan(source, &[path.to_path_buf()], &base)?;
    let relative = |p: &PathBuf| {
        p.strip_prefix(&base)
            .map(|r| r.display().to_string())
            .unwrap_or_else(|_| p.display().to_string())
    };
    Ok(Preview {
        source,
        notes: planned.notes.len(),
        attachments: planned.attachments.len(),
        skipped: planned
            .skipped
            .iter()
            .map(|(p, why)| (p.display().to_string(), skip_reason(why)))
            .collect(),
        samples: planned.notes.iter().take(SAMPLES).map(relative).collect(),
    })
}

#[derive(Debug, Clone, Default)]
pub struct Imported {
    pub notes: Vec<PathBuf>,
    pub attachments: Vec<PathBuf>,
    pub skipped: Vec<(PathBuf, SkipReason)>,
}

pub fn run(
    vault_root: &Path,
    source: Source,
    inputs: &[PathBuf],
    destination: &Path,
) -> anyhow::Result<Imported> {
    let base = confine(vault_root, destination)?;
    let report = focus_md::import::run(source, inputs, &base)?;
    let relative = |p: &PathBuf| {
        p.strip_prefix(vault_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| p.clone())
    };
    Ok(Imported {
        notes: report.notes.iter().map(relative).collect(),
        attachments: report.attachments.iter().map(relative).collect(),
        skipped: report
            .skipped
            .into_iter()
            .map(|(p, why)| (p, skip_reason(&why)))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-import")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn folder_preview_counts_notes_attachments_and_skips() {
        let dir = scratch("folder");
        fs::create_dir_all(dir.join("src/.git")).unwrap();
        fs::write(dir.join("src/a.md"), "# a").unwrap();
        fs::write(dir.join("src/b.md"), "# b").unwrap();
        fs::write(dir.join("src/pic.png"), [0u8; 4]).unwrap();
        fs::write(dir.join("src/.git/config"), "x").unwrap();
        fs::write(dir.join("src/.hidden.md"), "x").unwrap();
        let vault = dir.join("vault");
        fs::create_dir_all(vault.join("Imports/src")).unwrap();
        fs::write(vault.join("Imports/src/a.md"), "taken").unwrap();
        let preview = preview(&vault, &dir.join("src"), Path::new("Imports")).unwrap();
        assert_eq!(
            (preview.source, preview.notes, preview.attachments),
            (Source::Folder, 2, 1)
        );
        assert_eq!(preview.skipped.len(), 2);
        assert!(
            preview
                .skipped
                .iter()
                .all(|(_, why)| *why == SkipReason::Hidden)
        );
        assert_eq!(
            preview.samples,
            ["src 2/a.md", "src 2/b.md"],
            "the folder name is taken: the real names carry the number"
        );
    }

    #[test]
    fn files_preview_flags_unsupported_types() {
        let dir = scratch("files");
        fs::write(dir.join("x.exe"), [0u8]).unwrap();
        assert!(
            preview(&dir, &dir.join("x.exe"), Path::new("In")).is_err(),
            "not importable at all"
        );
        fs::write(dir.join("t.txt"), "hello").unwrap();
        let preview = preview(&dir, &dir.join("t.txt"), Path::new("In")).unwrap();
        assert_eq!((preview.source, preview.notes), (Source::Files, 1));
    }

    #[test]
    fn run_imports_inside_the_vault_and_refuses_to_leave_it() {
        let dir = scratch("run");
        fs::create_dir_all(dir.join("vault")).unwrap();
        fs::write(dir.join("one.md"), "# one").unwrap();
        let imported = run(
            &dir.join("vault"),
            Source::Files,
            &[dir.join("one.md")],
            Path::new("Imports"),
        )
        .unwrap();
        assert_eq!(imported.notes, [PathBuf::from("Imports/one.md")]);
        let again = run(
            &dir.join("vault"),
            Source::Files,
            &[dir.join("one.md")],
            Path::new("Imports"),
        )
        .unwrap();
        assert_eq!(
            again.notes,
            [PathBuf::from("Imports/one 2.md")],
            "nothing is overwritten"
        );
        assert!(
            run(
                &dir.join("vault"),
                Source::Files,
                &[dir.join("one.md")],
                Path::new("../out")
            )
            .is_err()
        );
    }

    #[test]
    fn a_destination_that_is_a_link_out_of_the_vault_is_refused() {
        let dir = scratch("confine");
        let vault = dir.join("vault");
        let outside = dir.join("outside");
        fs::create_dir_all(&vault).unwrap();
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, vault.join("Imports")).unwrap();
        fs::write(dir.join("one.md"), "# one").unwrap();
        for destination in ["Imports", "Imports/deeper/still"] {
            assert!(
                run(
                    &vault,
                    Source::Files,
                    &[dir.join("one.md")],
                    Path::new(destination)
                )
                .is_err(),
                "{destination}"
            );
            assert!(
                preview(&vault, &dir.join("one.md"), Path::new(destination)).is_err(),
                "{destination}"
            );
        }
        assert!(
            fs::read_dir(&outside).unwrap().next().is_none(),
            "nothing was written outside the vault"
        );
        fs::create_dir_all(vault.join("Real")).unwrap();
        std::os::unix::fs::symlink(vault.join("Real"), vault.join("Alias")).unwrap();
        assert!(
            run(
                &vault,
                Source::Files,
                &[dir.join("one.md")],
                Path::new("Alias")
            )
            .is_ok()
        );
    }

    #[test]
    fn report_reasons_are_recognised() {
        assert_eq!(skip_reason("not a regular file"), SkipReason::Special);
        assert_eq!(skip_reason("file too large"), SkipReason::TooLarge);
        assert_eq!(skip_reason("archive nested too deep"), SkipReason::TooDeep);
        assert_eq!(skip_reason("symbolic link"), SkipReason::Symlink);
        assert_eq!(skip_reason("hidden or system entry"), SkipReason::Hidden);
        assert_eq!(
            skip_reason("disk full"),
            SkipReason::Other("disk full".into())
        );
    }
}
