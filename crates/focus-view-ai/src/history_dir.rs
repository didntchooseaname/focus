use std::path::{Path, PathBuf};

#[cfg(not(windows))]
fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

pub fn history_dir_for(vault_root: &Path) -> PathBuf {
    #[cfg(windows)]
    let base = focus_core::settings::data_dir();
    #[cfg(not(windows))]
    let base = env_dir("FOCUS_DATA_DIR")
        .or_else(|| env_dir("XDG_DATA_HOME").map(|d| d.join("focus")))
        .or_else(|| env_dir("HOME").map(|h| h.join(".local/share/focus")))
        .unwrap_or_else(|| PathBuf::from(".focus-data"));
    let cache = focus_core::settings::cache_dir_for(vault_root);
    base.join(cache.file_name().unwrap_or_default())
        .join("conversations")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_vault_has_its_own_folder_under_the_data_dir() {
        let a = history_dir_for(Path::new("/tmp/vault-a"));
        let b = history_dir_for(Path::new("/tmp/vault-b"));
        assert_ne!(a, b);
        assert!(a.ends_with("conversations"));
        assert!(
            !a.starts_with(focus_core::settings::config_dir()),
            "not in the settings folder: {}",
            a.display()
        );
        assert_eq!(a, history_dir_for(Path::new("/tmp/vault-a")));
    }
}
