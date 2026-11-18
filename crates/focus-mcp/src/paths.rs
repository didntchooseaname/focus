use std::path::{Component, Path, PathBuf};

pub struct VaultPaths {
    root: PathBuf,
}

impl VaultPaths {
    pub fn new(root: &Path) -> VaultPaths {
        VaultPaths {
            root: dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()),
        }
    }

    pub fn clean(&self, input: &str, allow_empty: bool) -> Result<PathBuf, String> {
        if input.contains('\0') {
            return Err("invalid path: null character".into());
        }
        let path = Path::new(input.trim());
        if path.is_absolute() || input.trim().starts_with('~') {
            return Err(format!(
                "path outside the vault: {input}. Give a path relative to the vault root."
            ));
        }
        let mut clean = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => clean.push(part),
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(format!("path outside the vault: {input} (`..` is refused)"));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(format!("path outside the vault: {input}"));
                }
            }
        }
        if clean.as_os_str().is_empty() && !allow_empty {
            return Err("empty path".into());
        }
        #[cfg(windows)]
        let clean = PathBuf::from(
            clean
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
        );
        Ok(clean)
    }

    pub fn check_inside(&self, relative: &Path) -> Result<PathBuf, String> {
        let absolute = self.root.join(relative);
        let mut probe = absolute.as_path();
        loop {
            match std::fs::symlink_metadata(probe) {
                Ok(_) => break,
                Err(_) => match probe.parent() {
                    Some(parent) if parent.starts_with(&self.root) => probe = parent,
                    _ => return Err(format!("path outside the vault: {}", relative.display())),
                },
            }
        }
        let canonical = dunce::canonicalize(probe)
            .map_err(|e| format!("unreadable path: {} ({e})", relative.display()))?;
        if canonical.starts_with(&self.root) {
            Ok(absolute)
        } else {
            Err(format!(
                "path outside the vault: {} (symbolic link to the outside)",
                relative.display()
            ))
        }
    }

    pub fn check_writable(&self, relative: &Path) -> Result<PathBuf, String> {
        for component in relative.components() {
            if let Component::Normal(part) = component {
                let name = part.to_string_lossy();
                if name.starts_with('.') || name == "node_modules" {
                    return Err(format!(
                        "write refused in a reserved folder: {}",
                        relative.display()
                    ));
                }
            }
        }
        self.check_inside(relative)
    }
}

pub(crate) fn canonical_with_missing_tail(path: &Path) -> PathBuf {
    let mut probe = path;
    while !probe.exists() {
        match probe.parent() {
            Some(parent) => probe = parent,
            None => return path.to_path_buf(),
        }
    }
    let canonical = dunce::canonicalize(probe).unwrap_or_else(|_| probe.to_path_buf());
    canonical.join(path.strip_prefix(probe).unwrap_or(Path::new("")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(format!("mcp-paths-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lexical_rules() {
        let paths = VaultPaths::new(&scratch("lexical"));
        assert_eq!(
            paths.clean("a/./b.md", false).unwrap(),
            PathBuf::from("a/b.md")
        );
        assert_eq!(paths.clean("", true).unwrap(), PathBuf::new());
        assert!(paths.clean("", false).is_err());
        for bad in [
            "../x.md",
            "a/../../x.md",
            "a/..",
            "/etc/passwd",
            "~/x.md",
            "a\0b",
        ] {
            assert!(paths.clean(bad, true).is_err(), "{bad}");
        }
        assert!(
            paths.clean("a/../b.md", false).is_err(),
            "`..` is refused even when the result stays in the vault"
        );
    }

    #[test]
    #[cfg(unix)]
    fn symlinks_cannot_leave_the_vault() {
        let base = scratch("symlinks");
        let vault = base.join("vault");
        let outside = base.join("outside");
        std::fs::create_dir_all(vault.join("dossier")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.md"), "secret").unwrap();
        std::fs::write(vault.join("dossier/ok.md"), "ok").unwrap();
        std::os::unix::fs::symlink(outside.join("secret.md"), vault.join("link.md")).unwrap();
        std::os::unix::fs::symlink(&outside, vault.join("link-folder")).unwrap();
        std::os::unix::fs::symlink(vault.join("dossier/ok.md"), vault.join("internal-link.md"))
            .unwrap();
        let paths = VaultPaths::new(&vault);
        assert!(paths.check_inside(Path::new("dossier/ok.md")).is_ok());
        assert!(paths.check_inside(Path::new("internal-link.md")).is_ok());
        assert!(
            paths
                .check_inside(Path::new("nouveau/dossier/x.md"))
                .is_ok()
        );
        for bad in [
            "link.md",
            "link-folder/secret.md",
            "link-folder/nouveau.md",
            "link-folder",
        ] {
            let error = paths.check_inside(Path::new(bad)).unwrap_err();
            assert!(error.contains("outside the vault"), "{bad}: {error}");
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_folder_that_does_not_exist_yet_has_a_canonical_form() {
        let base = scratch("missing-tail");
        std::fs::create_dir_all(base.join("real")).unwrap();
        std::os::unix::fs::symlink(base.join("real"), base.join("alias")).unwrap();
        let canonical = std::fs::canonicalize(&base).unwrap();
        assert_eq!(
            canonical_with_missing_tail(&base.join("alias/Engagements")),
            canonical.join("real/Engagements")
        );
    }

    #[test]
    fn writes_refuse_reserved_folders() {
        let paths = VaultPaths::new(&scratch("reserved"));
        for bad in [
            ".git/config",
            ".obsidian/app.json",
            "a/.trash/x.md",
            "node_modules/x.md",
            ".hidden.md",
        ] {
            assert!(paths.check_writable(Path::new(bad)).is_err(), "{bad}");
        }
        assert!(paths.check_writable(Path::new("Red Team/Note.md")).is_ok());
    }
}
