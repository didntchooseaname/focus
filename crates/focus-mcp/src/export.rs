use std::path::{Path, PathBuf};
use std::sync::Arc;

use focus_core::{LinkTarget, NoteId, Vault};
use focus_md::export::{HtmlOptions, Resolver};
use serde_json::Value;

use crate::rules::WriteOptions;
use crate::tools::{Refusal, VaultTools, dry_run_text, text_argument};

pub const DEFAULT_EXPORT_DIR: &str = "Export";

struct VaultResolver {
    vault: Arc<Vault>,
    paths: Arc<crate::paths::VaultPaths>,
    tools: VaultTools,
    note: NoteId,
    note_dir: PathBuf,
    markdown: bool,
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = text.get(i + 1..i + 3)
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            out.push(value);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn encode(path: &str) -> String {
    path.chars()
        .map(|c| {
            if matches!(c, ' ' | '%' | '#' | '?' | '(' | ')') {
                format!("%{:02X}", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn relative_from(from_dir: &Path, to: &Path) -> String {
    let from: Vec<_> = from_dir.components().collect();
    let target: Vec<_> = to.components().collect();
    let common = from.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> =
        std::iter::repeat_n("..".to_string(), from.len() - common).collect();
    parts.extend(
        target[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/")
}

impl Resolver for VaultResolver {
    fn image(&self, src: &str) -> Option<PathBuf> {
        let decoded = decode(src);
        let relative = match self.vault.resolve_link(self.note, &decoded) {
            LinkTarget::Attachment(path) => path,
            _ => normalized(&self.note_dir.join(&decoded))?,
        };
        let absolute = self.paths.check_inside(&relative).ok()?;
        absolute.is_file().then_some(absolute)
    }

    fn link(&self, target: &str, anchor: Option<&str>) -> Option<String> {
        if !self.markdown {
            return None;
        }
        match self.vault.resolve_link(self.note, &decode(target)) {
            LinkTarget::Note { id, .. } => {
                let path = self.vault.note(id)?.path.clone();
                let mut link = encode(&relative_from(&self.note_dir, &path));
                if let Some(anchor) = anchor {
                    link.push('#');
                    link.push_str(&encode(anchor));
                }
                Some(link)
            }
            _ => None,
        }
    }

    fn embed(&self, target: &str) -> Option<String> {
        match self.vault.resolve_link(self.note, &decode(target)) {
            LinkTarget::Note { id, .. } if self.tools.shown(id) => self
                .vault
                .read(id)
                .ok()
                .map(|text| self.tools.displayed(id, &text)),
            _ => None,
        }
    }
}

fn normalized(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => out.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

fn extension(format: &str) -> Option<&'static str> {
    match format {
        "html" => Some("html"),
        "markdown" => Some("md"),
        "pdf" => Some("pdf"),
        _ => None,
    }
}

impl VaultTools {
    fn export_destination(
        &self,
        requested: Option<&str>,
        stem: &str,
        extension: &str,
    ) -> Result<PathBuf, String> {
        let root = dunce::canonicalize(self.vault.root())
            .unwrap_or_else(|_| self.vault.root().to_path_buf());
        let export_dir = if self.export_dir.is_absolute() {
            self.export_dir.clone()
        } else {
            root.join(&self.export_dir)
        };
        let destination = match requested.map(str::trim).filter(|d| !d.is_empty()) {
            None => export_dir.join(format!("{stem}.{extension}")),
            Some(path) if Path::new(path).is_absolute() => PathBuf::from(path),
            Some(path) => export_dir.join(self.paths.clean(path, false)?),
        };
        let export_canonical = crate::paths::canonical_with_missing_tail(&export_dir);
        if !self.export_dir.is_absolute() && !export_canonical.starts_with(&root) {
            return Err(format!(
                "the export folder {} leads outside the vault",
                self.export_dir.display()
            ));
        }
        let parent = destination.parent().ok_or("invalid destination")?;
        if !crate::paths::canonical_with_missing_tail(parent).starts_with(&export_canonical) {
            return Err(format!(
                "destination outside the export folder ({}): {}",
                export_canonical.display(),
                destination.display()
            ));
        }
        if destination.exists() {
            return Err(format!(
                "already exists: {} (an export never overwrites a file)",
                destination.display()
            ));
        }
        Ok(destination)
    }

    pub(crate) fn export_note(
        &self,
        arguments: &Value,
        options: &WriteOptions,
        cancel: &focus_ai::Cancel,
    ) -> Result<String, Refusal> {
        let (id, relative) = self.locate(text_argument(arguments, "path")?)?;
        let format = text_argument(arguments, "format")?;
        let ext = extension(format)
            .ok_or_else(|| format!("unknown format: {format} (html, markdown or pdf)"))?;
        let stem = relative
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "note".into());
        let destination = self.export_destination(
            arguments.get("destination").and_then(Value::as_str),
            &stem,
            ext,
        )?;
        let source = self.displayed(id, &self.vault.read(id).map_err(crate::tools::describe)?);
        if options.dry_run {
            return Ok(dry_run_text(
                &format!(
                    "export_note would export {} as {format}",
                    relative.display()
                ),
                &format!("Destination: {}\n", destination.display()),
            ));
        }
        self.authorize(
            "export_note",
            &destination,
            None,
            &format!("export {} as {format}", relative.display()),
            cancel,
        )?;

        let note_dir = relative.parent().map(Path::to_path_buf).unwrap_or_default();
        let resolver = VaultResolver {
            vault: self.vault.clone(),
            paths: self.paths.clone(),
            tools: self.clone(),
            note: id,
            note_dir,
            markdown: format == "markdown",
        };
        let title = self
            .vault
            .note(id)
            .map(|m| m.title.to_string())
            .unwrap_or(stem);
        let options = HtmlOptions {
            title,
            dark: false,
            embed_images: true,
            render_mermaid: true,
            embed_fonts: false,
            ..HtmlOptions::default()
        };
        if let Some(dir) = destination.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| Refusal::Error(format!("{}: {e}", dir.display())))?;
        }
        match format {
            "html" => {
                let html = focus_md::export::to_html(&source, &options, &resolver);
                std::fs::write(&destination, html)
                    .map_err(|e| Refusal::Error(format!("{}: {e}", destination.display())))?;
            }
            "markdown" => {
                let text = focus_md::export::to_standard_markdown(&source, &resolver);
                std::fs::write(&destination, text)
                    .map_err(|e| Refusal::Error(format!("{}: {e}", destination.display())))?;
            }
            _ => {
                let html = focus_md::export::to_html(&source, &options, &resolver);
                let temporary = destination.with_extension("html.focus-tmp");
                std::fs::write(&temporary, html)
                    .map_err(|e| Refusal::Error(format!("{}: {e}", temporary.display())))?;
                let made = focus_md::export::html_to_pdf(&temporary, &destination);
                let _ = std::fs::remove_file(&temporary);
                match made {
                    Ok(true) => {}
                    Ok(false) => return Err(Refusal::Error("no Chromium browser (chromium, google-chrome-stable, brave) is installed: export as html".into())),
                    Err(e) => return Err(Refusal::Error(format!("PDF export: {e}"))),
                }
            }
        }
        let size = std::fs::metadata(&destination)
            .map(|m| m.len())
            .unwrap_or(0);
        Ok(format!(
            "Exported: {} ({size} bytes)",
            destination.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_helpers() {
        assert_eq!(
            relative_from(Path::new("Red Team"), Path::new("Red Team/Defense.md")),
            "Defense.md"
        );
        assert_eq!(
            relative_from(Path::new("Red Team/sous"), Path::new("Notes/Pont.md")),
            "../../Notes/Pont.md"
        );
        assert_eq!(
            relative_from(Path::new(""), Path::new("Accueil.md")),
            "Accueil.md"
        );
        assert_eq!(
            encode("Red Team/Note (1).md"),
            "Red%20Team/Note%20%281%29.md"
        );
        assert_eq!(decode("Red%20Team/a%C3%A9.md"), "Red Team/aé.md");
        assert_eq!(decode("100%"), "100%");
    }
}
