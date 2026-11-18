use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use focus_core::{LinkTarget, NoteId, Vault};
use focus_md::export::Resolver;

pub type OutputPaths = HashMap<NoteId, PathBuf>;

pub struct VaultResolver<'a> {
    pub vault: &'a Vault,
    pub from: NoteId,
    pub outputs: &'a OutputPaths,
}

fn relative_url(from_dir: &Path, to: &Path) -> String {
    let from: Vec<_> = from_dir.components().collect();
    let target: Vec<_> = to.components().collect();
    let common = from.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = from[common..].iter().map(|_| "..".to_string()).collect();
    parts.extend(
        target[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/").replace(' ', "%20")
}

pub(crate) fn is_inside(vault: &Vault, path: &Path) -> bool {
    match (dunce::canonicalize(vault.root()), dunce::canonicalize(path)) {
        (Ok(root), Ok(real)) => real.starts_with(root),
        _ => false,
    }
}

const MAX_INCLUDE_BYTES: u64 = 2 << 20;

fn split_include(path: &str) -> (&str, Option<(usize, usize)>) {
    let Some((file, range)) = path.trim().split_once(':') else {
        return (path.trim(), None);
    };
    let (start, end) = range.split_once(':').unwrap_or((range, ""));
    let number = |text: &str| text.trim().parse::<usize>().ok();
    if (start.trim().is_empty() || number(start).is_some())
        && (end.trim().is_empty() || number(end).is_some())
    {
        (
            file.trim(),
            Some((
                number(start).unwrap_or(1).max(1),
                number(end).unwrap_or(usize::MAX),
            )),
        )
    } else {
        (path.trim(), None)
    }
}

fn stays_below(src: &str) -> bool {
    Path::new(src)
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

impl Resolver for VaultResolver<'_> {
    fn image(&self, src: &str) -> Option<PathBuf> {
        let found = match self.vault.resolve_link(self.from, src) {
            LinkTarget::Attachment(path) => Some(self.vault.absolute(&path)),
            _ => {
                let meta = self.vault.note(self.from)?;
                let beside = self
                    .vault
                    .absolute(meta.path.parent().unwrap_or(Path::new("")))
                    .join(src);
                (stays_below(src) && beside.is_file()).then_some(beside)
            }
        };
        found.filter(|path| is_inside(self.vault, path))
    }

    fn link(&self, target: &str, anchor: Option<&str>) -> Option<String> {
        let LinkTarget::Note {
            id,
            anchor: resolved,
        } = self.vault.resolve_link(self.from, target)
        else {
            return None;
        };
        let to = self.outputs.get(&id)?;
        let from = self.outputs.get(&self.from)?;
        let mut url = relative_url(from.parent().unwrap_or(Path::new("")), to);
        if let Some(anchor) = anchor.map(str::to_string).or(resolved) {
            url.push('#');
            url.push_str(&anchor.to_lowercase().replace(' ', "-"));
        }
        Some(url)
    }

    fn include(&self, path: &str) -> Option<String> {
        let (file, lines) = split_include(path);
        if !file.to_ascii_lowercase().ends_with(".md") {
            return None;
        }
        let meta = self.vault.note(self.from)?;
        let wanted = self
            .vault
            .absolute(meta.path.parent().unwrap_or(Path::new("")))
            .join(file);
        let real = dunce::canonicalize(&wanted).ok()?;
        let root = dunce::canonicalize(self.vault.root()).ok()?;
        let inside = real.strip_prefix(&root).ok()?;
        if !real.is_file()
            || inside
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
            || std::fs::metadata(&real).ok()?.len() > MAX_INCLUDE_BYTES
        {
            return None;
        }
        let text = std::fs::read_to_string(&real).ok()?;
        Some(match lines {
            Some((start, end)) => text
                .lines()
                .skip(start - 1)
                .take(end.saturating_sub(start - 1))
                .collect::<Vec<_>>()
                .join("\n"),
            None => text,
        })
    }

    fn embed(&self, target: &str) -> Option<String> {
        match self.vault.resolve_link(self.from, target) {
            LinkTarget::Note { id, .. }
                if is_inside(self.vault, &self.vault.absolute(&self.vault.note(id)?.path)) =>
            {
                self.vault.read(id).ok()
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::relative_url;
    use std::path::Path;

    #[test]
    fn an_include_names_a_file_and_maybe_lines() {
        use super::split_include;
        assert_eq!(split_include("a/b.md"), ("a/b.md", None));
        assert_eq!(split_include(" b.md:2:10 "), ("b.md", Some((2, 10))));
        assert_eq!(split_include("b.md:3:"), ("b.md", Some((3, usize::MAX))));
        assert_eq!(split_include("b.md::4"), ("b.md", Some((1, 4))));
        assert_eq!(split_include("b.md:anchor"), ("b.md:anchor", None));
    }

    #[test]
    fn urls_climb_out_of_the_source_folder() {
        assert_eq!(
            relative_url(Path::new("a/b"), Path::new("a/c/Note two.html")),
            "../c/Note%20two.html"
        );
        assert_eq!(relative_url(Path::new(""), Path::new("x.html")), "x.html");
        assert_eq!(
            relative_url(Path::new("a"), Path::new("a/x.html")),
            "x.html"
        );
    }
}
