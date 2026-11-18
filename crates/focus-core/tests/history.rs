use focus_core::{HistoryConfig, HistoryKind, OpenOptions, Vault, VaultError, settings};
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct Fixture {
    vault: Arc<Vault>,
    root: PathBuf,
    store: PathBuf,
}

fn fixture(name: &str, files: &[(&str, &str)]) -> Fixture {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/focus-core-tests")
        .join(format!("history-{name}"));
    make_writable(&base);
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("vault");
    for (rel, text) in files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();
    let vault = Vault::open(
        &root,
        OpenOptions {
            cache_dir: base.join("cache"),
            ignore: Vec::new(),
            watch: false,
        },
    )
    .unwrap();
    let store = base.join("history");
    Fixture { vault, root, store }
}

fn make_writable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(items) = std::fs::read_dir(dir) {
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        for item in items.flatten() {
            if item.path().is_dir() {
                make_writable(&item.path());
            }
        }
    }
}

impl Fixture {
    fn attach(&self) {
        self.vault
            .set_history(&self.store, HistoryConfig::default())
            .unwrap();
    }

    fn id(&self, rel: &str) -> u32 {
        self.vault.note_by_path(Path::new(rel)).expect("note")
    }

    fn texts(&self, rel: &str) -> Vec<String> {
        let history = self.vault.history().expect("history attached");
        history
            .list(Path::new(rel))
            .unwrap()
            .iter()
            .map(|v| history.read(Path::new(rel), &v.id).unwrap())
            .collect()
    }
}

#[test]
fn nothing_is_kept_until_a_history_is_attached() {
    let f = fixture("detached", &[("a.md", "one")]);
    f.vault.write(f.id("a.md"), "two", None).unwrap();
    assert!(f.vault.history().is_none());
    assert!(!f.store.exists());
}

#[test]
fn the_first_write_keeps_the_content_it_replaces_and_autosaves_within_five_minutes_do_not() {
    let f = fixture("edit", &[("a.md", "original")]);
    f.attach();
    let id = f.id("a.md");
    f.vault.write(id, "first edit", None).unwrap();
    f.vault.write(id, "second edit", None).unwrap();
    assert_eq!(f.texts("a.md"), ["original"]);
    assert_eq!(
        std::fs::read_to_string(f.root.join("a.md")).unwrap(),
        "second edit"
    );
}

#[test]
fn a_bulk_write_always_keeps_one() {
    let f = fixture("bulk", &[("a.md", "original")]);
    f.attach();
    let id = f.id("a.md");
    f.vault.write(id, "first edit", None).unwrap();
    f.vault
        .write_with(id, "replaced everywhere", None, HistoryKind::Bulk)
        .unwrap();
    assert_eq!(f.texts("a.md"), ["first edit", "original"]);
}

#[test]
fn a_restore_keeps_the_current_text_first_and_can_be_undone() {
    let f = fixture("restore", &[("a.md", "original")]);
    f.attach();
    let id = f.id("a.md");
    f.vault.write(id, "edited", None).unwrap();
    let history = f.vault.history().unwrap();
    let version = history.list(Path::new("a.md")).unwrap().remove(0);
    let old = history.read(Path::new("a.md"), &version.id).unwrap();
    f.vault
        .write_with(id, &old, None, HistoryKind::Restore)
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(f.root.join("a.md")).unwrap(),
        "original"
    );
    assert_eq!(f.texts("a.md"), ["edited", "original"]);
}

#[test]
fn a_toggled_task_goes_through_the_history() {
    let f = fixture("task", &[("a.md", "- [ ] item\n")]);
    f.attach();
    f.vault.toggle_task(f.id("a.md"), 0).unwrap();
    assert_eq!(f.texts("a.md"), ["- [ ] item\n"]);
}

#[test]
fn a_renamed_note_and_a_renamed_folder_take_their_versions() {
    let f = fixture(
        "rename",
        &[
            ("a.md", "note a"),
            ("dir/b.md", "note b"),
            ("dir/deep/c.md", "note c"),
        ],
    );
    f.attach();
    for rel in ["a.md", "dir/b.md", "dir/deep/c.md"] {
        f.vault.write(f.id(rel), "changed", None).unwrap();
    }
    f.vault
        .rename(Path::new("a.md"), Path::new("x/a2.md"), true)
        .unwrap();
    f.vault
        .rename(Path::new("dir"), Path::new("moved"), true)
        .unwrap();
    assert_eq!(f.texts("x/a2.md"), ["note a"]);
    assert_eq!(f.texts("moved/b.md"), ["note b"]);
    assert_eq!(f.texts("moved/deep/c.md"), ["note c"]);
    assert!(f.texts("a.md").is_empty() && f.texts("dir/b.md").is_empty());
}

#[test]
fn rewriting_links_after_a_move_keeps_a_bulk_version_of_each_note_touched() {
    let f = fixture(
        "links",
        &[
            ("a.md", "see [[b]] and [[b]]\n"),
            ("b.md", "target"),
            ("c.md", "unrelated"),
        ],
    );
    f.attach();
    let report = f
        .vault
        .rename(Path::new("b.md"), Path::new("b2.md"), true)
        .unwrap();
    assert_eq!(report.updated_notes.len(), 1);
    assert_eq!(f.texts("a.md"), ["see [[b]] and [[b]]\n"]);
    assert!(f.texts("c.md").is_empty());
    assert!(
        std::fs::read_to_string(f.root.join("a.md"))
            .unwrap()
            .contains("[[b2]]")
    );
}

#[test]
fn a_note_both_moved_and_rewritten_has_its_bulk_version_under_the_new_path() {
    let f = fixture(
        "moved-and-rewritten",
        &[
            ("dir/a.md", "links [[b]]\n"),
            ("dir/b.md", "target"),
            ("keep.md", "[[a]]"),
        ],
    );
    f.attach();
    f.vault
        .rename(Path::new("dir"), Path::new("moved"), true)
        .unwrap();
    f.vault
        .rename(Path::new("moved/b.md"), Path::new("moved/b2.md"), true)
        .unwrap();
    assert_eq!(f.texts("moved/a.md"), ["links [[b]]\n"]);
    assert!(f.texts("dir/a.md").is_empty());
}

#[test]
fn a_note_sent_to_the_vault_trash_keeps_its_versions_there() {
    let f = fixture("trash", &[("a.md", "doomed")]);
    f.attach();
    f.vault.write(f.id("a.md"), "edited", None).unwrap();
    f.vault.trash(Path::new("a.md")).unwrap();
    assert_eq!(f.texts(".trash/a.md"), ["doomed"]);
}

#[test]
fn a_failing_history_stops_a_bulk_write_but_not_an_autosave() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture("failing", &[("a.md", "original"), ("b.md", "other")]);
    f.attach();
    let notes = f.store.join("notes");
    std::fs::set_permissions(&notes, std::fs::Permissions::from_mode(0o500)).unwrap();
    if std::fs::File::create(notes.join("probe")).is_ok() {
        let _ = std::fs::remove_file(notes.join("probe"));
        return;
    }
    let bulk = f
        .vault
        .write_with(f.id("a.md"), "new", None, HistoryKind::Bulk);
    assert!(matches!(bulk, Err(VaultError::Io { .. })), "{bulk:?}");
    assert_eq!(
        std::fs::read_to_string(f.root.join("a.md")).unwrap(),
        "original"
    );
    assert!(f.vault.take_history_error().is_none());

    f.vault.write(f.id("b.md"), "autosaved", None).unwrap();
    assert_eq!(
        std::fs::read_to_string(f.root.join("b.md")).unwrap(),
        "autosaved"
    );
    assert!(matches!(
        f.vault.take_history_error(),
        Some(VaultError::Io { .. })
    ));
    assert!(f.vault.take_history_error().is_none(), "returned once");
    std::fs::set_permissions(&notes, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn history_settings_default_to_the_specification_and_old_files_still_load() {
    let defaults = settings::Settings::default().history;
    assert_eq!(
        defaults,
        HistoryConfig {
            enabled: true,
            days: 14,
            max_bytes: 200 * 1024 * 1024
        }
    );
    let old: settings::Settings = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
    assert_eq!(old.history, defaults);
    let partial: settings::Settings = serde_json::from_str(r#"{"history":{"days":3}}"#).unwrap();
    assert_eq!((partial.history.enabled, partial.history.days), (true, 3));
}

#[test]
fn each_vault_has_its_own_history_folder_under_the_data_folder() {
    let a = settings::history_dir_for(Path::new("/tmp/vault-a"));
    let b = settings::history_dir_for(Path::new("/tmp/vault-b"));
    assert_ne!(a, b);
    assert_eq!(
        a.parent(),
        Some(settings::data_dir().join("history").as_path())
    );
    assert_eq!(
        a.file_name(),
        settings::cache_dir_for(Path::new("/tmp/vault-a")).file_name()
    );
}
