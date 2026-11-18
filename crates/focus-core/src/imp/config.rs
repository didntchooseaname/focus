use crate::settings::Settings;
use crate::{ObsidianConfig, PropertyType};
use anyhow::Context;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) fn home_dir() -> Option<PathBuf> {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn home() -> PathBuf {
    home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(not(windows))]
fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(v) if Path::new(&v).is_absolute() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

pub(crate) fn config_dir() -> PathBuf {
    env_dir("FOCUS_CONFIG_DIR").unwrap_or_else(|| platform_dir(Kind::Config))
}

enum Kind {
    Config,
    Cache,
    Data,
}

#[cfg(not(windows))]
fn platform_dir(kind: Kind) -> PathBuf {
    match kind {
        Kind::Config => xdg("XDG_CONFIG_HOME", ".config").join("focus"),
        Kind::Cache => xdg("XDG_CACHE_HOME", ".cache").join("focus"),
        Kind::Data => xdg("XDG_DATA_HOME", ".local/share").join("focus"),
    }
}

#[cfg(windows)]
fn platform_dir(kind: Kind) -> PathBuf {
    let base =
        |var: &str, under: &str| env_dir(var).unwrap_or_else(|| home().join("AppData").join(under));
    match kind {
        Kind::Config => base("APPDATA", "Roaming").join("focus"),
        Kind::Cache => base("LOCALAPPDATA", "Local").join("focus").join("cache"),
        Kind::Data => base("LOCALAPPDATA", "Local").join("focus").join("data"),
    }
}

pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub(crate) fn cache_dir_for(vault_root: &Path) -> PathBuf {
    let canonical = dunce::canonicalize(vault_root).unwrap_or_else(|_| vault_root.to_path_buf());
    let hash = fnv1a(canonical.as_os_str().as_encoded_bytes());
    let base = env_dir("FOCUS_CACHE_DIR").unwrap_or_else(|| platform_dir(Kind::Cache));
    base.join(format!("{hash:016x}"))
}

pub(crate) fn data_dir() -> PathBuf {
    env_dir("FOCUS_DATA_DIR").unwrap_or_else(|| platform_dir(Kind::Data))
}

pub(crate) fn history_dir_for(vault_root: &Path) -> PathBuf {
    let id = cache_dir_for(vault_root)
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    data_dir().join("history").join(id)
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub(crate) fn load_settings() -> anyhow::Result<Settings> {
    let path = settings_path();
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("unreadable settings: {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

pub(crate) fn save_settings(settings: &Settings) -> anyhow::Result<()> {
    let path = settings_path();
    let json = serde_json::to_vec_pretty(settings)?;
    super::fsx::write_atomic(&path, &json)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub(crate) const TYPES_FILE: &str = "types.json";

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(crate) fn read_obsidian(root: &Path) -> ObsidianConfig {
    let dir = root.join(".obsidian");
    let mut cfg = ObsidianConfig {
        attachment_folder: "/".into(),
        use_markdown_links: false,
        new_link_format: "shortest".into(),
        ..Default::default()
    };
    if let Some(app) = read_json(&dir.join("app.json")) {
        if let Some(s) = app.get("attachmentFolderPath").and_then(|v| v.as_str()) {
            cfg.attachment_folder = s.to_string();
        }
        if let Some(b) = app.get("useMarkdownLinks").and_then(|v| v.as_bool()) {
            cfg.use_markdown_links = b;
        }
        if let Some(s) = app.get("newLinkFormat").and_then(|v| v.as_str()) {
            cfg.new_link_format = s.to_string();
        }
    }
    if let Some(t) = read_json(&dir.join("templates.json")) {
        cfg.templates_folder = t
            .get("folder")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);
    }
    if let Some(d) = read_json(&dir.join("daily-notes.json")) {
        cfg.daily_notes_folder = d.get("folder").and_then(|v| v.as_str()).map(str::to_string);
        cfg.daily_notes_format = d
            .get("format")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);
    }
    if let Some(b) = read_json(&dir.join("bookmarks.json")) {
        fn collect(items: &serde_json::Value, out: &mut Vec<PathBuf>) {
            for item in items.as_array().into_iter().flatten() {
                match item.get("type").and_then(|v| v.as_str()) {
                    Some("file") => {
                        if let Some(p) = item.get("path").and_then(|v| v.as_str()) {
                            out.push(PathBuf::from(p));
                        }
                    }
                    Some("group") => {
                        collect(item.get("items").unwrap_or(&serde_json::Value::Null), out)
                    }
                    _ => {}
                }
            }
        }
        collect(
            b.get("items").unwrap_or(&serde_json::Value::Null),
            &mut cfg.bookmarks,
        );
    }
    cfg.property_types = read_property_types(root);
    cfg
}

pub(crate) fn read_property_types(root: &Path) -> BTreeMap<String, PropertyType> {
    read_json(&root.join(".obsidian").join(TYPES_FILE))
        .map(|json| super::props::read_types(&json))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obsidian_defaults_and_values() {
        let dir = super::super::test_dir("obsidian-config");
        let cfg = read_obsidian(&dir);
        assert_eq!(cfg.attachment_folder, "/");
        assert!(!cfg.use_markdown_links);
        std::fs::create_dir_all(dir.join(".obsidian")).unwrap();
        std::fs::write(dir.join(".obsidian/app.json"), r#"{"attachmentFolderPath":"./assets","useMarkdownLinks":true,"newLinkFormat":"relative"}"#).unwrap();
        std::fs::write(
            dir.join(".obsidian/bookmarks.json"),
            r#"{"items":[{"type":"file","path":"a.md"},{"type":"group","items":[{"type":"file","path":"b/c.md"}]},{"type":"search","query":"x"}]}"#,
        )
        .unwrap();
        let cfg = read_obsidian(&dir);
        assert_eq!(cfg.attachment_folder, "./assets");
        assert!(cfg.use_markdown_links);
        assert_eq!(cfg.new_link_format, "relative");
        assert_eq!(
            cfg.bookmarks,
            [PathBuf::from("a.md"), PathBuf::from("b/c.md")]
        );
    }

    #[test]
    fn cache_dir_is_stable_per_path() {
        let a = cache_dir_for(Path::new("/nonexistent/vault-a"));
        let b = cache_dir_for(Path::new("/nonexistent/vault-b"));
        assert_ne!(a, b);
        assert_eq!(a, cache_dir_for(Path::new("/nonexistent/vault-a")));
        assert!(a.ends_with(format!("{:016x}", fnv1a(b"/nonexistent/vault-a"))));
    }
}
