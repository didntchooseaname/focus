use std::path::{Path, PathBuf};

use focus_core::NoteId;

use crate::tools::VaultTools;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Exposure {
    Open,
    Masked,
    Secret,
    Outside,
}

pub(crate) const SECRET_NOTE: &str = "engagement secrets are not served as notes: use ops_credentials_list, or start focus-mcp with --reveal-secrets";
pub(crate) const HIDDEN_NOTE: &str = "(hidden note)";

pub(crate) fn engagements_relative(vault_root: &Path, ops_dir: &Path) -> Option<PathBuf> {
    let root = dunce::canonicalize(vault_root).ok()?;
    let absolute = if ops_dir.is_absolute() {
        ops_dir.to_path_buf()
    } else {
        root.join(ops_dir)
    };
    crate::paths::canonical_with_missing_tail(&absolute)
        .strip_prefix(&root)
        .ok()
        .map(Path::to_path_buf)
}

impl VaultTools {
    pub(crate) fn exposure(&self, relative: &Path) -> Exposure {
        if self.paths.check_inside(relative).is_err() {
            return Exposure::Outside;
        }
        let Some(engagements) = self.secret_guard.as_deref() else {
            return Exposure::Open;
        };
        match relative.strip_prefix(engagements) {
            Ok(inner)
                if inner
                    .components()
                    .any(|c| c.as_os_str().eq_ignore_ascii_case("Loot")) =>
            {
                Exposure::Secret
            }
            Ok(_) => Exposure::Masked,
            Err(_) => Exposure::Open,
        }
    }

    pub(crate) fn exposure_of(&self, id: NoteId) -> Exposure {
        match self.vault.source_path(id) {
            Some(path) => self.exposure(&path),
            None => Exposure::Outside,
        }
    }

    pub(crate) fn shown(&self, id: NoteId) -> bool {
        matches!(self.exposure_of(id), Exposure::Open | Exposure::Masked)
    }

    pub(crate) fn displayed(&self, id: NoteId, text: &str) -> String {
        match self.vault.source_path(id) {
            Some(path) if self.exposure(&path) == Exposure::Masked => {
                let secrets = self.engagement_secrets(&path);
                mask_with(text, &secrets)
            }
            _ => text.to_string(),
        }
    }

    pub(crate) fn line_filter(&self, relative: &Path) -> Box<dyn Fn(&str) -> String> {
        if self.exposure(relative) == Exposure::Masked {
            let secrets = self.engagement_secrets(relative);
            Box::new(move |line| mask_with(line, &secrets))
        } else {
            Box::new(str::to_string)
        }
    }

    fn engagement_secrets(&self, relative: &Path) -> Vec<String> {
        let Some(engagements) = self.secret_guard.as_deref() else {
            return Vec::new();
        };
        let Some(name) = relative
            .strip_prefix(engagements)
            .ok()
            .and_then(|inner| inner.components().next())
        else {
            return Vec::new();
        };
        let dir = dunce::canonicalize(self.vault.root())
            .unwrap_or_else(|_| self.vault.root().to_path_buf())
            .join(engagements)
            .join(name);
        focus_ops::report::secrets_of(&dir)
    }
}

pub(crate) fn mask_with(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        out = out.replace(secret.as_str(), "[masked]");
    }
    let lines: Vec<String> = mask_secrets(&out)
        .split('\n')
        .map(|line| {
            let named = line.split_once([':', '=']).filter(|(name, value)| {
                !value.trim().is_empty()
                    && crate::ops::sensitive_name(name.trim().trim_start_matches(['-', ' ']))
                    && name
                        .trim()
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_- ".contains(c))
            });
            match named {
                Some((name, _)) => format!("{name}: [masked]"),
                None => line.to_string(),
            }
        })
        .collect();
    lines.join("\n")
}

pub(crate) fn mask_secrets(text: &str) -> String {
    let options = focus_ai::redact::Options {
        secrets: true,
        ip_addresses: false,
        hostnames: false,
    };
    let redacted = focus_ai::redact::redact(text, &options, &[]);
    let mut out = redacted.text;
    for replacement in &redacted.replacements {
        out = out.replace(&replacement.token, "[masked]");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engagements_folder_is_guarded_before_it_exists() {
        let root = crate::testvault::scratch("guard-before");
        assert_eq!(
            engagements_relative(&root, &root.join("Engagements")),
            Some(PathBuf::from("Engagements"))
        );
        assert_eq!(
            engagements_relative(&root, Path::new("Pentests/2026")),
            Some(PathBuf::from("Pentests/2026"))
        );
        assert_eq!(
            engagements_relative(&root, Path::new("/somewhere/else")),
            None
        );
    }

    #[test]
    fn secrets_are_replaced_by_a_fixed_mark_that_cannot_be_reversed() {
        let masked = mask_secrets("found password=Hunter2-Corp-Secret-9 on the share");
        assert!(
            !masked.contains("Hunter2") && masked.contains("[masked]"),
            "{masked}"
        );
        assert_eq!(mask_secrets("nothing to hide here"), "nothing to hide here");
    }
}
