use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use focus_history::{History, Kind};

use crate::{OpsError, Result};

struct Attached {
    history: Arc<History>,
    root: PathBuf,
}

static ATTACHED: RwLock<Option<Attached>> = RwLock::new(None);

pub fn attach_history(history: Arc<History>, vault_root: &Path) {
    let root = crate::fsutil::absolute(vault_root);
    *ATTACHED.write().unwrap_or_else(|e| e.into_inner()) = Some(Attached { history, root });
}

pub fn detach_history() {
    *ATTACHED.write().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn keep_version(path: &Path) -> Result<()> {
    let attached = ATTACHED.read().unwrap_or_else(|e| e.into_inner());
    let Some(attached) = attached.as_ref() else {
        return Ok(());
    };
    let absolute = crate::fsutil::absolute(path);
    let Ok(relative) = absolute.strip_prefix(&attached.root) else {
        return Ok(());
    };
    match attached
        .history
        .keep(relative, Kind::Edit, || std::fs::read(&absolute))
    {
        Ok(_) => Ok(()),
        Err(focus_history::Error::Io { path, source }) => Err(OpsError::Io { path, source }),
        Err(other) => Err(OpsError::Invalid(other.to_string())),
    }
}

#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngagementKind, EngagementStatus, NewEngagement, fsutil};
    use focus_history::HistoryConfig;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, UNIX_EPOCH};

    fn history(name: &str) -> (Arc<History>, Arc<AtomicU64>) {
        let store = fsutil::test_dir(name);
        let now = Arc::new(AtomicU64::new(1_000_000_000));
        let clock = {
            let now = now.clone();
            Arc::new(move || UNIX_EPOCH + Duration::from_millis(now.load(Ordering::SeqCst)))
        };
        let history = History::open_with_clock(&store, HistoryConfig::default(), clock).unwrap();
        (Arc::new(history), now)
    }

    fn texts(history: &History, rel: &str) -> Vec<String> {
        history
            .list(Path::new(rel))
            .unwrap()
            .iter()
            .map(|v| history.read(Path::new(rel), &v.id).unwrap())
            .collect()
    }

    #[test]
    fn a_replaced_note_keeps_its_content_and_a_new_one_keeps_nothing() {
        let _guard = test_lock();
        let vault = fsutil::test_dir("versions-vault");
        let (history, now) = history("versions-store");
        attach_history(history.clone(), &vault);
        let path = vault.join("Engagement/Notes.md");

        fsutil::write_atomic(&path, "one").unwrap();
        assert!(texts(&history, "Engagement/Notes.md").is_empty());
        fsutil::write_atomic(&path, "two").unwrap();
        assert_eq!(texts(&history, "Engagement/Notes.md"), ["one"]);
        fsutil::write_atomic(&path, "three").unwrap();
        assert_eq!(
            texts(&history, "Engagement/Notes.md"),
            ["one"],
            "within the interval"
        );
        now.fetch_add(6 * 60 * 1000, Ordering::SeqCst);
        fsutil::write_private(&path, "four").unwrap();
        assert_eq!(texts(&history, "Engagement/Notes.md"), ["three", "one"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "four");
        detach_history();
    }

    #[test]
    fn a_file_outside_the_vault_has_no_history() {
        let _guard = test_lock();
        let vault = fsutil::test_dir("versions-outside-vault");
        let elsewhere = fsutil::test_dir("versions-outside-files");
        let (history, _) = history("versions-outside-store");
        attach_history(history.clone(), &vault);
        let path = elsewhere.join("Notes.md");
        fsutil::write_atomic(&path, "one").unwrap();
        fsutil::write_atomic(&path, "two").unwrap();
        assert_eq!(history.size().unwrap(), 0);
        detach_history();
    }

    #[test]
    fn saving_an_engagement_keeps_the_readme_it_replaces() {
        let _guard = test_lock();
        let vault = fsutil::test_dir("versions-engagement");
        let (history, _) = history("versions-engagement-store");
        attach_history(history.clone(), &vault);
        let created = crate::create(
            &vault.join("Engagements"),
            &NewEngagement {
                name: "Mission".into(),
                kind: EngagementKind::Lab,
                client: None,
                scope: Vec::new(),
                exam: None,
                checklist: None,
            },
        )
        .unwrap();
        let readme = created.dir.strip_prefix(fsutil::absolute(&vault)).unwrap();
        let readme = readme.join("README.md");
        let before = std::fs::read_to_string(created.dir.join("README.md")).unwrap();
        assert!(
            history.list(&readme).unwrap().is_empty(),
            "creation keeps nothing"
        );
        let mut engagement = crate::load(&created.dir).unwrap();
        engagement.status = EngagementStatus::InProgress;
        crate::save(&engagement).unwrap();
        assert_eq!(
            texts(&history, readme.to_str().unwrap()),
            [before],
            "the README as it was before the save"
        );
        detach_history();
    }

    #[test]
    #[cfg(unix)]
    fn a_failing_history_refuses_the_write() {
        let _guard = test_lock();
        let vault = fsutil::test_dir("versions-failing-vault");
        let (history, now) = history("versions-failing-store");
        attach_history(history.clone(), &vault);
        let path = vault.join("Notes.md");
        fsutil::write_atomic(&path, "one").unwrap();
        fsutil::write_atomic(&path, "two").unwrap();
        now.fetch_add(6 * 60 * 1000, Ordering::SeqCst);
        let blobs = history.dir().join("blobs");
        std::fs::set_permissions(&blobs, std::fs::Permissions::from_mode(0o500)).unwrap();
        let refused = fsutil::write_atomic(&path, "three");
        std::fs::set_permissions(&blobs, std::fs::Permissions::from_mode(0o700)).unwrap();
        detach_history();
        if std::fs::File::create(blobs.join("probe")).is_ok() {
            return;
        }
        assert!(matches!(refused, Err(OpsError::Io { .. })));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
    }
}
