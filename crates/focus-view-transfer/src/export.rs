use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context as _;
use focus_core::{NoteId, Vault};
use focus_i18n::t;
use focus_md::export::{
    CodeColors, HtmlOptions, Labels, html_to_pdf, to_html, to_standard_markdown, zip,
};

use crate::resolver::{OutputPaths, VaultResolver, is_inside};

const CALLOUT_KINDS: [&str; 27] = [
    "note",
    "abstract",
    "summary",
    "tldr",
    "info",
    "todo",
    "tip",
    "hint",
    "important",
    "success",
    "check",
    "done",
    "question",
    "help",
    "faq",
    "warning",
    "caution",
    "attention",
    "failure",
    "fail",
    "missing",
    "danger",
    "error",
    "bug",
    "example",
    "quote",
    "cite",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Note(NoteId),
    Folder(PathBuf),
    Vault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Html,
    Markdown,
    Pdf,
    Zip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub dark: bool,
    pub embed_images: bool,
    pub embed_fonts: bool,
    pub render_mermaid: bool,
    pub soft_code_colors: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            dark: false,
            embed_images: true,
            embed_fonts: true,
            render_mermaid: true,
            soft_code_colors: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    pub scope: Scope,
    pub format: Format,
    pub options: Options,
    pub destination: PathBuf,
    pub language: String,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub notes: usize,
    pub files: usize,
    pub bytes: u64,
    pub folder: PathBuf,
    pub main_file: Option<PathBuf>,
    pub skipped: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Notes { done: usize, total: usize },
    Packing,
}

#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Cancel::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("no Chromium based browser found")]
    PdfUnavailable,
    #[error("cancelled")]
    Cancelled,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub fn pdf_available() -> bool {
    focus_md::export::pdf_available()
}

pub fn labels() -> Labels {
    Labels {
        copy: t("export-html-copy"),
        copied: t("export-html-copied"),
        diagram: t("export-html-diagram"),
        detected: t("export-html-detected"),
        properties: t("export-html-properties"),
        footnotes: t("export-html-footnotes"),
        back_to_text: t("export-html-back-to-text"),
        callout_titles: CALLOUT_KINDS
            .iter()
            .map(|kind| ((*kind).to_string(), t(&format!("reading-callout-{kind}"))))
            .collect(),
    }
}

pub fn page_language(code: &str) -> String {
    code.split('-').next().unwrap_or("en").to_string()
}

pub const OUTSIDE_VAULT: &str = "outside the vault";
const OUTSIDE: &str = OUTSIDE_VAULT;

struct Selection {
    notes: Vec<(NoteId, PathBuf, String)>,
    outside: Vec<(String, String)>,
    root_name: String,
}

fn select(vault: &Vault, scope: &Scope) -> Selection {
    let mut summaries: Vec<_> = vault.summaries();
    summaries.sort_by(|a, b| a.path.cmp(&b.path));
    let keep = |path: &Path| match scope {
        Scope::Note(_) | Scope::Vault => true,
        Scope::Folder(folder) => path.starts_with(folder),
    };
    let (mut notes, mut outside) = (Vec::new(), Vec::new());
    for s in summaries.into_iter().filter(|s| match scope {
        Scope::Note(id) => s.id == *id,
        _ => keep(&s.path),
    }) {
        if is_inside(vault, &vault.absolute(&s.path)) {
            notes.push((s.id, s.path.to_path_buf(), s.title.to_string()));
        } else {
            outside.push((s.path.display().to_string(), OUTSIDE.to_string()));
        }
    }
    let root_name = match scope {
        Scope::Folder(folder) => folder
            .file_name()
            .map_or_else(|| vault.name(), |n| n.to_string_lossy().into_owned()),
        _ => vault.name(),
    };
    Selection {
        notes,
        outside,
        root_name,
    }
}

fn unique(path: &Path) -> PathBuf {
    let free = |p: &Path| p.symlink_metadata().is_err();
    if free(path) {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let dir = path.parent().unwrap_or(Path::new("."));
    (2..)
        .map(|n| dir.join(format!("{stem} {n}{ext}")))
        .find(|p| free(p))
        .unwrap_or_else(|| path.to_path_buf())
}

fn size_of(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn image_references(source: &str) -> BTreeSet<String> {
    use std::sync::LazyLock;
    static MARKDOWN: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"!\[[^\]]*\]\(([^)\s]+)").expect("image pattern"));
    static WIKI: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"!\[\[([^\]|#]+)").expect("embed pattern"));
    let is_image = |s: &str| {
        matches!(
            Path::new(s)
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .as_deref(),
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "avif")
        )
    };
    MARKDOWN
        .captures_iter(source)
        .chain(WIKI.captures_iter(source))
        .map(|c| c[1].to_string())
        .filter(|s| !s.contains("://") && !s.starts_with("data:") && is_image(s))
        .collect()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn copy_images(source: &str, resolver: &VaultResolver, page_dir: &Path, report: &mut Report) {
    use focus_md::export::Resolver as _;
    for src in image_references(source) {
        let decoded = percent_decode(&src);
        let Some(from) = resolver.image(&decoded).or_else(|| resolver.image(&src)) else {
            if is_outside_reference(resolver, &decoded) {
                report.skipped.push((decoded, OUTSIDE.to_string()));
            }
            continue;
        };
        let relative = Path::new(&decoded);
        if !relative.components().all(|c| {
            matches!(
                c,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        }) {
            continue;
        }
        let to = page_dir.join(relative);
        if let Some(parent) = to.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            continue;
        }
        let copied = std::fs::File::open(&from).and_then(|mut input| {
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&to)?;
            std::io::copy(&mut input, &mut output)
        });
        if let Ok(bytes) = copied {
            report.files += 1;
            report.bytes += bytes;
        }
    }
}

fn is_outside_reference(resolver: &VaultResolver, src: &str) -> bool {
    let Some(meta) = resolver.vault.note(resolver.from) else {
        return false;
    };
    let beside = resolver
        .vault
        .absolute(meta.path.parent().unwrap_or(Path::new("")))
        .join(src);
    beside.is_file() && !crate::resolver::is_inside(resolver.vault, &beside)
}

pub fn run(
    vault: &Vault,
    request: &Request,
    cancel: &Cancel,
    progress: &mut dyn FnMut(Progress),
) -> Result<Report, ExportError> {
    let selection = select(vault, &request.scope);
    let total = selection.notes.len();
    if total == 0 {
        return Err(anyhow::anyhow!("nothing to export").into());
    }
    if request.format == Format::Pdf && !pdf_available() {
        return Err(ExportError::PdfUnavailable);
    }
    std::fs::create_dir_all(&request.destination)
        .with_context(|| format!("creating {}", request.destination.display()))?;
    let single = matches!(request.scope, Scope::Note(_));
    let base = if single {
        request.destination.clone()
    } else {
        unique(&request.destination.join(&selection.root_name))
    };
    let extension = match request.format {
        Format::Html => "html",
        Format::Markdown => "md",
        Format::Pdf => "pdf",
        Format::Zip => "zip",
    };

    if request.format == Format::Zip {
        progress(Progress::Packing);
        let scope_root = match &request.scope {
            Scope::Folder(folder) => folder.clone(),
            _ => PathBuf::new(),
        };
        let mut files: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut seen = BTreeSet::new();
        let mut skipped = selection.outside.clone();
        for (id, path, _) in &selection.notes {
            for relative in std::iter::once(path.clone()).chain(vault.attachments_of(*id)) {
                if !is_inside(vault, &vault.absolute(&relative)) {
                    skipped.push((relative.display().to_string(), OUTSIDE.to_string()));
                    continue;
                }
                if seen.insert(relative.clone()) {
                    let inside = relative.strip_prefix(&scope_root).unwrap_or(&relative);
                    files.push((
                        vault.absolute(&relative),
                        Path::new(&selection.root_name).join(inside),
                    ));
                }
            }
        }
        let target = unique(&request.destination.join(format!(
            "{}.zip",
            if single {
                selection.notes[0].2.clone()
            } else {
                selection.root_name.clone()
            }
        )));
        if cancel.is_cancelled() {
            return Err(ExportError::Cancelled);
        }
        let bytes = zip(&files, &target)?;
        return Ok(Report {
            notes: total,
            files: 1,
            bytes,
            folder: request.destination.clone(),
            main_file: Some(target),
            skipped,
        });
    }

    let stem_of = |path: &Path| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let outputs: OutputPaths = selection
        .notes
        .iter()
        .map(|(id, path, _)| {
            let relative = if single {
                PathBuf::from(format!("{}.{extension}", stem_of(path)))
            } else {
                let inside = match &request.scope {
                    Scope::Folder(folder) => path.strip_prefix(folder).unwrap_or(path),
                    _ => path,
                };
                inside.with_extension(extension)
            };
            (*id, relative)
        })
        .collect();
    let mut report = Report {
        notes: 0,
        folder: base.clone(),
        skipped: selection.outside.clone(),
        ..Report::default()
    };
    let labels = labels();
    for (done, (id, _, title)) in selection.notes.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(ExportError::Cancelled);
        }
        progress(Progress::Notes { done, total });
        let source = match vault.read(*id) {
            Ok(source) => source,
            Err(error) => {
                report.skipped.push((title.clone(), error.to_string()));
                continue;
            }
        };
        let resolver = VaultResolver {
            vault,
            from: *id,
            outputs: &outputs,
        };
        let relative = &outputs[id];
        let target = if single {
            unique(&base.join(relative))
        } else {
            base.join(relative)
        };
        let page_dir = target.parent().unwrap_or(&base).to_path_buf();
        std::fs::create_dir_all(&page_dir)
            .with_context(|| format!("creating {}", page_dir.display()))?;
        let html_options = |embed_images: bool| HtmlOptions {
            title: title.clone(),
            dark: request.options.dark,
            embed_images,
            render_mermaid: request.options.render_mermaid,
            embed_fonts: request.options.embed_fonts,
            code_colors: if request.options.soft_code_colors {
                CodeColors::Soft
            } else {
                CodeColors::Mono
            },
            lang: page_language(&request.language),
            labels: labels.clone(),
        };
        match request.format {
            Format::Html => {
                let html = to_html(
                    &source,
                    &html_options(request.options.embed_images),
                    &resolver,
                );
                std::fs::write(&target, html)
                    .with_context(|| format!("writing {}", target.display()))?;
                if !request.options.embed_images {
                    copy_images(&source, &resolver, &page_dir, &mut report);
                }
            }
            Format::Markdown => {
                std::fs::write(&target, to_standard_markdown(&source, &resolver))
                    .with_context(|| format!("writing {}", target.display()))?;
                copy_images(&source, &resolver, &page_dir, &mut report);
            }
            Format::Pdf => {
                let page =
                    private_page(&page_dir, &to_html(&source, &html_options(true), &resolver))?;
                let made = html_to_pdf(&page, &target);
                let _ = std::fs::remove_file(&page);
                if !made? {
                    return Err(ExportError::PdfUnavailable);
                }
            }
            Format::Zip => unreachable!("handled above"),
        }
        report.notes += 1;
        report.files += 1;
        report.bytes += size_of(&target);
        if single {
            report.main_file = Some(target);
        }
    }
    progress(Progress::Notes { done: total, total });
    Ok(report)
}

fn private_page(dir: &Path, html: &str) -> anyhow::Result<PathBuf> {
    use std::io::Write as _;
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let path = dir.join(format!(
        ".focus-export-{}-{nanos}-{}.html",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(&path)
        .and_then(|mut file| file.write_all(html.as_bytes()))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

pub fn copy_html(vault: &Vault, note: NoteId) -> anyhow::Result<String> {
    let source = vault.read(note)?;
    let outputs: OutputPaths = HashMap::new();
    Ok(focus_md::export::to_html_fragment(
        &source,
        &VaultResolver {
            vault,
            from: note,
            outputs: &outputs,
        },
    ))
}

pub fn copy_markdown(vault: &Vault, note: NoteId) -> anyhow::Result<String> {
    let source = vault.read(note)?;
    let outputs: OutputPaths = HashMap::new();
    Ok(to_standard_markdown(
        &source,
        &VaultResolver {
            vault,
            from: note,
            outputs: &outputs,
        },
    ))
}

#[cfg(test)]
pub(crate) fn scratch_vault(name: &str, notes: &[(&str, &[u8])]) -> (Arc<Vault>, PathBuf) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-vaults");
    let root = base.join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch vault folder");
    for (path, bytes) in notes {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("note folder")).expect("note folder");
        std::fs::write(file, bytes).expect("scratch note");
    }
    let options = focus_core::OpenOptions {
        cache_dir: base.join(format!("{name}-cache")),
        ignore: Vec::new(),
        watch: false,
    };
    (Vault::open(&root, options).expect("scratch vault"), root)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

    fn vault(name: &str) -> (Arc<Vault>, PathBuf) {
        scratch_vault(
            name,
            &[
                (
                    "Index.md",
                    b"# Index\n\nSee [[Deep note]] and ![pic](img/pic.png).\n",
                ),
                (
                    "sub/Deep note.md",
                    b"# Deep note\n\nBack to [[Index]]. ![[pic.png]]\n",
                ),
                ("img/pic.png", PNG),
            ],
        )
    }

    fn id_of(vault: &Vault, title: &str) -> NoteId {
        vault
            .summaries()
            .iter()
            .find(|s| &*s.title == title)
            .map(|s| s.id)
            .expect("note")
    }

    fn request(scope: Scope, format: Format, destination: PathBuf) -> Request {
        Request {
            scope,
            format,
            options: Options {
                embed_fonts: false,
                render_mermaid: false,
                ..Options::default()
            },
            destination,
            language: "fr".into(),
        }
    }

    fn silent(_: Progress) {}

    const SECRET: &[u8] = b"\x89PNG\r\n\x1a\nTOP-SECRET-PIXELS";

    fn linked_vault(name: &str, notes: &[(&str, &[u8])]) -> (Arc<Vault>, PathBuf, PathBuf) {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-vaults");
        let root = base.join(name);
        let outside = base.join(format!("{name}-outside"));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(
            outside.join("secret.md"),
            "# Secret note\n\nTOP-SECRET-TEXT\n",
        )
        .unwrap();
        std::fs::write(outside.join("secret.png"), SECRET).unwrap();
        std::fs::write(outside.join("secret.txt"), "TOP-SECRET-FILE").unwrap();
        for (path, bytes) in notes {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, bytes).unwrap();
        }
        std::os::unix::fs::symlink(outside.join("secret.md"), root.join("Linked note.md")).unwrap();
        std::fs::create_dir_all(root.join("img")).unwrap();
        std::os::unix::fs::symlink(outside.join("secret.png"), root.join("img/linked.png"))
            .unwrap();
        let options = focus_core::OpenOptions {
            cache_dir: base.join(format!("{name}-cache")),
            ignore: Vec::new(),
            watch: false,
        };
        (Vault::open(&root, options).expect("vault"), root, outside)
    }

    fn files_of(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(files_of(&path))
            } else {
                out.push(path)
            }
        }
        out
    }

    fn contains_secret(dir: &Path) -> Vec<PathBuf> {
        files_of(dir)
            .into_iter()
            .filter(|f| {
                let bytes = std::fs::read(f).unwrap_or_default();
                let text = String::from_utf8_lossy(&bytes).into_owned();
                text.contains("TOP-SECRET")
                    || text.contains("base64,")
                    || f.extension().is_some_and(|e| e == "zip")
                        && zip_names(f)
                            .iter()
                            .any(|n| n.contains("Linked") || n.contains("linked"))
            })
            .collect()
    }

    fn zip_names(path: &Path) -> Vec<String> {
        let archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        archive
            .file_names()
            .filter_map(Result::ok)
            .map(|n| n.into_owned())
            .collect()
    }

    #[test]
    fn an_image_path_that_leaves_the_vault_is_not_embedded_or_copied() {
        let (vault, root, _) = linked_vault(
            "export-outside-path",
            &[("a/Note.md", b"# Note\n\n![up](../../export-outside-path-outside/secret.png)\n\n![abs](/etc/hostname)\n\n![txt](../../export-outside-path-outside/secret.txt)\n\n![link](../img/linked.png)\n")],
        );
        let abs_probe = std::fs::read("/etc/hostname").unwrap_or_default();
        let id = id_of(&vault, "Note");
        for (format, embed) in [
            (Format::Html, true),
            (Format::Html, false),
            (Format::Markdown, false),
        ] {
            let out = root.join(format!("../export-outside-path-out-{embed}-{format:?}"));
            let _ = std::fs::remove_dir_all(&out);
            let mut req = request(Scope::Note(id), format, out.clone());
            req.options.embed_images = embed;
            let report = run(&vault, &req, &Cancel::new(), &mut silent).unwrap();
            assert!(
                contains_secret(&out).is_empty(),
                "{format:?} embed {embed}: {:?}",
                contains_secret(&out)
            );
            if !abs_probe.is_empty() {
                let probe = String::from_utf8_lossy(&abs_probe).trim().to_string();
                assert!(
                    files_of(&out)
                        .iter()
                        .all(|f| !String::from_utf8_lossy(&std::fs::read(f).unwrap())
                            .contains(&format!("\n{probe}\n"))),
                    "/etc/hostname was copied"
                );
            }
            assert!(report.files >= 1);
        }
    }

    #[test]
    fn notes_and_images_reached_through_links_out_of_the_vault_are_left_out_and_counted() {
        let (vault, root, _) = linked_vault("export-links", &[("Index.md", b"# Index\n\n![[linked.png]] ![x](img/linked.png)\n\n![[Linked note]]\n\n[[Linked note]]\n")]);
        let listed = vault
            .summaries()
            .iter()
            .any(|s| s.path.ends_with("Linked note.md"));
        for format in [Format::Html, Format::Markdown, Format::Zip] {
            let out = root.join(format!("../export-links-out-{format:?}"));
            let _ = std::fs::remove_dir_all(&out);
            let report = run(
                &vault,
                &request(Scope::Vault, format, out.clone()),
                &Cancel::new(),
                &mut silent,
            )
            .unwrap();
            assert!(
                contains_secret(&out).is_empty(),
                "{format:?}: {:?}",
                contains_secret(&out)
            );
            if listed {
                assert!(
                    report
                        .skipped
                        .iter()
                        .any(|(name, why)| name.contains("Linked note") && why == OUTSIDE),
                    "{format:?}: {:?}",
                    report.skipped
                );
            }
        }
        let zipped = root.join("../export-links-out-Zip");
        let archive = files_of(&zipped)
            .into_iter()
            .find(|f| f.extension().is_some_and(|e| e == "zip"))
            .unwrap();
        assert!(
            zip_names(&archive)
                .iter()
                .all(|n| !n.to_lowercase().contains("linked")),
            "{:?}",
            zip_names(&archive)
        );
    }

    #[test]
    fn an_include_is_expanded_from_the_vault_only() {
        let (vault, root, _) = linked_vault(
            "export-include",
            &[
                ("shared/banner.md", b"# Banner\n\nline two\n\nline three\n\n{{#include banner.md}}\n"),
                ("shared/notes.txt", b"TEXT-FILE"),
                (".obsidian/hidden.md", b"HIDDEN-NOTE"),
                (
                    "a/Page.md",
                    b"# Page\n\n{{#include ../shared/banner.md}}\n\n{{#include ../shared/banner.md:3:3}}\n\n{{#include ../Linked note.md}}\n\n{{#include ../../export-include-outside/secret.md}}\n\n{{#include ../shared/notes.txt}}\n\n{{#include ../.obsidian/hidden.md}}\n\n{{#include ../shared/missing.md}}\n",
                ),
            ],
        );
        let id = id_of(&vault, "Page");
        let out = root.join("../export-include-out");
        let _ = std::fs::remove_dir_all(&out);
        let mut req = request(Scope::Note(id), Format::Html, out.clone());
        req.options.render_mermaid = false;
        run(&vault, &req, &Cancel::new(), &mut silent).unwrap();
        let html = std::fs::read_to_string(out.join("Page.html")).unwrap();
        assert_eq!(
            html.matches("<div class=\"include\">").count(),
            2,
            "the whole banner and the one line of the range"
        );
        assert!(html.contains("line two") && html.contains("line three"));
        assert_eq!(
            html.matches("line two").count(),
            2,
            "once whole, once as the range 3:3 (lines count from 1)"
        );
        assert_eq!(
            html.matches("line three").count(),
            1,
            "the range stops at its last line"
        );
        for forbidden in ["TOP-SECRET", "TEXT-FILE", "HIDDEN-NOTE"] {
            assert!(!html.contains(forbidden), "{forbidden} was included");
        }
        for muted in [
            "include ../Linked note.md",
            "include ../../export-include-outside/secret.md",
            "include ../shared/notes.txt",
            "include ../.obsidian/hidden.md",
            "include ../shared/missing.md",
            "include banner.md",
        ] {
            assert!(
                html.contains(&format!("<p class=\"directive muted\">{muted}</p>")),
                "{muted} should stay a muted line"
            );
        }
    }

    #[test]
    fn the_page_the_browser_prints_is_private_and_never_follows_a_planted_link() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-vaults/private-page");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, "keep").unwrap();
        std::os::unix::fs::symlink(
            &victim,
            dir.join(format!(".focus-export-{}.html", std::process::id())),
        )
        .unwrap();
        let page = private_page(&dir, "<p>x</p>").unwrap();
        assert_eq!(
            std::fs::metadata(&page).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        assert_ne!(
            private_page(&dir, "<p>y</p>").unwrap(),
            page,
            "two pages never share a name"
        );
    }

    #[test]
    fn a_note_becomes_a_standalone_page_with_embedded_image_and_page_language() {
        let (vault, root) = vault("export-note");
        let out = root.join("../export-note-out");
        let _ = std::fs::remove_dir_all(&out);
        let report = run(
            &vault,
            &request(
                Scope::Note(id_of(&vault, "Index")),
                Format::Html,
                out.clone(),
            ),
            &Cancel::new(),
            &mut silent,
        )
        .unwrap();
        let html = std::fs::read_to_string(out.join("Index.html")).unwrap();
        assert!(
            html.contains("<html lang=\"fr\""),
            "{}",
            &html[..200.min(html.len())]
        );
        assert!(html.contains("data:image/png;base64,"));
        assert_eq!(report.main_file, Some(out.join("Index.html")));
        assert_eq!(report.notes, 1);
        let again = run(
            &vault,
            &request(
                Scope::Note(id_of(&vault, "Index")),
                Format::Html,
                out.clone(),
            ),
            &Cancel::new(),
            &mut silent,
        )
        .unwrap();
        assert_eq!(
            again.main_file,
            Some(out.join("Index 2.html")),
            "an existing file is never overwritten"
        );
    }

    #[test]
    fn a_folder_keeps_its_layout_with_relative_links() {
        let (vault, root) = vault("export-folder");
        let out = root.join("../export-folder-out");
        let _ = std::fs::remove_dir_all(&out);
        let mut options = request(Scope::Vault, Format::Html, out.clone());
        options.options.embed_images = false;
        let report = run(&vault, &options, &Cancel::new(), &mut silent).unwrap();
        let folder = out.join(vault.name());
        assert_eq!(report.folder, folder);
        let index = std::fs::read_to_string(folder.join("Index.html")).unwrap();
        let deep = std::fs::read_to_string(folder.join("sub/Deep note.html")).unwrap();
        assert!(index.contains("href=\"sub/Deep%20note.html\""), "{index}");
        assert!(deep.contains("href=\"../Index.html\""));
        assert!(
            folder.join("img/pic.png").is_file() && folder.join("sub/pic.png").is_file(),
            "images keep the path the note wrote, under the page"
        );
    }

    #[test]
    fn markdown_export_copies_the_images_and_zip_packs_notes_with_attachments() {
        let (vault, root) = vault("export-md");
        let out = root.join("../export-md-out");
        let _ = std::fs::remove_dir_all(&out);
        let report = run(
            &vault,
            &request(
                Scope::Note(id_of(&vault, "Index")),
                Format::Markdown,
                out.clone(),
            ),
            &Cancel::new(),
            &mut silent,
        )
        .unwrap();
        assert!(
            std::fs::read_to_string(out.join("Index.md"))
                .unwrap()
                .contains("Deep%20note.md")
                || report.notes == 1
        );
        let zipped = run(
            &vault,
            &request(Scope::Vault, Format::Zip, out.clone()),
            &Cancel::new(),
            &mut silent,
        )
        .unwrap();
        let archive =
            zip::ZipArchive::new(std::fs::File::open(zipped.main_file.unwrap()).unwrap()).unwrap();
        let names: Vec<String> = archive
            .file_names()
            .filter_map(Result::ok)
            .map(|n| n.to_string())
            .collect();
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(names.iter().any(|n| n.ends_with("img/pic.png")));
    }

    #[test]
    fn cancelling_stops_before_the_next_note_and_progress_counts_notes() {
        let (vault, root) = vault("export-cancel");
        let out = root.join("../export-cancel-out");
        let _ = std::fs::remove_dir_all(&out);
        let cancel = Cancel::new();
        let mut seen = Vec::new();
        let outcome = run(
            &vault,
            &request(Scope::Vault, Format::Html, out),
            &cancel,
            &mut |p| {
                seen.push(p);
                cancel.cancel();
            },
        );
        assert!(matches!(outcome, Err(ExportError::Cancelled)));
        assert_eq!(seen, [Progress::Notes { done: 0, total: 2 }]);
    }

    #[test]
    fn clipboard_text_is_a_fragment_and_standard_markdown() {
        let (vault, _) = vault("export-clip");
        let id = id_of(&vault, "Index");
        let html = copy_html(&vault, id).unwrap();
        assert!(!html.contains("<html") && html.contains("<h1"));
        assert!(!copy_markdown(&vault, id).unwrap().contains("[[Deep note]]"));
    }

    #[test]
    fn image_references_cover_both_syntaxes_and_skip_urls() {
        let refs = image_references(
            "![a](x/a.png) ![[b.JPG|200]] ![w](https://x.test/c.png) ![d](data:image/png;base64,AA) [l](not.png)",
        );
        assert_eq!(refs.into_iter().collect::<Vec<_>>(), ["b.JPG", "x/a.png"]);
        assert_eq!(percent_decode("a%20b%2Fc"), "a b/c");
    }
}
