use super::*;
use std::os::unix::fs::PermissionsExt as _;
use std::sync::atomic::AtomicU64;

fn test_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/focus-history-tests")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("test directory under target/");
    dir
}

struct Fixture {
    history: History,
    now: Arc<AtomicU64>,
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str, config: HistoryConfig) -> Fixture {
        let dir = test_dir(name).join("store");
        let now = Arc::new(AtomicU64::new(100 * DAY_MS));
        let clock = {
            let now = now.clone();
            Arc::new(move || UNIX_EPOCH + Duration::from_millis(now.load(Ordering::SeqCst)))
        };
        Fixture {
            history: History::open_with_clock(&dir, config, clock).unwrap(),
            now,
            dir,
        }
    }

    fn advance(&self, by: Duration) {
        self.now.fetch_add(by.as_millis() as u64, Ordering::SeqCst);
    }

    fn keep(&self, rel: &str, kind: Kind, content: &str) -> Option<VersionId> {
        self.history
            .keep(Path::new(rel), kind, || Ok(content.as_bytes().to_vec()))
            .unwrap()
    }

    fn texts(&self, rel: &str) -> Vec<String> {
        self.history
            .list(Path::new(rel))
            .unwrap()
            .iter()
            .map(|v| self.history.read(Path::new(rel), &v.id).unwrap())
            .collect()
    }
}

const MINUTE: Duration = Duration::from_secs(60);

#[test]
fn an_edit_is_kept_only_after_five_minutes() {
    let f = Fixture::new("interval", HistoryConfig::default());
    assert!(f.keep("a.md", Kind::Edit, "one").is_some());
    f.advance(4 * MINUTE + Duration::from_secs(59));
    assert!(f.keep("a.md", Kind::Edit, "two").is_none());
    f.advance(Duration::from_secs(1));
    assert!(
        f.keep("a.md", Kind::Edit, "two").is_none(),
        "exactly 5 minutes is not more"
    );
    f.advance(Duration::from_millis(1));
    assert!(f.keep("a.md", Kind::Edit, "two").is_some());
    assert_eq!(f.texts("a.md"), ["two", "one"]);
}

#[test]
fn versions_kept_in_the_same_millisecond_stay_in_order() {
    let f = Fixture::new("same-millisecond", HistoryConfig::default());
    let kept = ["a longer first text", "second", "3"];
    for content in kept {
        assert!(f.keep("a.md", Kind::Bulk, content).is_some());
    }
    assert_eq!(f.texts("a.md"), ["3", "second", "a longer first text"]);
    assert!(
        f.keep("a.md", Kind::Edit, "typed right after").is_none(),
        "an edit still waits after versions stamped ahead of the clock"
    );
    f.advance(EDIT_INTERVAL + Duration::from_millis(3));
    assert!(f.keep("a.md", Kind::Edit, "typed later").is_some());
}

#[test]
fn the_closure_runs_only_when_a_version_is_stored() {
    let f = Fixture::new("lazy", HistoryConfig::default());
    f.keep("a.md", Kind::Edit, "one");
    let called = std::cell::Cell::new(false);
    let kept = f
        .history
        .keep(Path::new("a.md"), Kind::Edit, || {
            called.set(true);
            Ok(Vec::new())
        })
        .unwrap();
    assert!(kept.is_none() && !called.get());
}

#[test]
fn bulk_and_restore_ignore_the_interval() {
    let f = Fixture::new("bulk", HistoryConfig::default());
    assert!(f.keep("a.md", Kind::Edit, "one").is_some());
    f.advance(Duration::from_secs(1));
    assert!(f.keep("a.md", Kind::Bulk, "two").is_some());
    f.advance(Duration::from_secs(1));
    assert!(f.keep("a.md", Kind::Restore, "three").is_some());
    let kinds: Vec<Kind> = f
        .history
        .list(Path::new("a.md"))
        .unwrap()
        .iter()
        .map(|v| v.kind)
        .collect();
    assert_eq!(kinds, [Kind::Restore, Kind::Bulk, Kind::Edit]);
}

#[test]
fn both_sides_of_an_external_change_are_kept_whatever_the_interval() {
    let f = Fixture::new("external", HistoryConfig::default());
    assert!(f.keep("a.md", Kind::Edit, "saved").is_some());
    f.advance(Duration::from_secs(1));
    assert!(f.keep("a.md", Kind::External, "from disk").is_some());
    f.advance(Duration::from_millis(1));
    assert!(f.keep("a.md", Kind::Unsaved, "typed").is_some());
    let kinds: Vec<Kind> = f
        .history
        .list(Path::new("a.md"))
        .unwrap()
        .iter()
        .map(|v| v.kind)
        .collect();
    assert_eq!(kinds, [Kind::Unsaved, Kind::External, Kind::Edit]);
    assert_eq!(f.texts("a.md"), ["typed", "from disk", "saved"]);
}

#[test]
fn a_reload_waits_for_the_interval_as_an_edit_does() {
    let f = Fixture::new("reload", HistoryConfig::default());
    assert!(f.keep("a.md", Kind::Reload, "shown").is_some());
    f.advance(4 * MINUTE);
    assert!(f.keep("a.md", Kind::Reload, "shown later").is_none());
    assert!(f.keep("a.md", Kind::Edit, "edited").is_none());
    f.advance(2 * MINUTE);
    assert!(f.keep("a.md", Kind::Reload, "shown later").is_some());
    assert_eq!(
        f.history.list(Path::new("a.md")).unwrap()[0].kind,
        Kind::Reload
    );
}

#[test]
fn content_equal_to_the_last_version_is_not_kept_again() {
    let f = Fixture::new("same", HistoryConfig::default());
    f.keep("a.md", Kind::Edit, "one");
    f.advance(10 * MINUTE);
    assert!(f.keep("a.md", Kind::Edit, "one").is_none());
    assert!(f.keep("a.md", Kind::Bulk, "one").is_none());
    assert_eq!(f.history.list(Path::new("a.md")).unwrap().len(), 1);
}

#[test]
fn identical_contents_share_one_file() {
    let f = Fixture::new("dedup", HistoryConfig::default());
    f.keep("a.md", Kind::Edit, "shared text");
    f.keep("b.md", Kind::Edit, "shared text");
    f.advance(10 * MINUTE);
    f.keep("a.md", Kind::Edit, "other");
    f.advance(10 * MINUTE);
    f.keep("a.md", Kind::Edit, "shared text");
    assert_eq!(f.history.list(Path::new("a.md")).unwrap().len(), 3);
    assert_eq!(
        f.history.size().unwrap(),
        ("shared text".len() + "other".len()) as u64
    );
}

#[test]
fn a_disabled_history_keeps_nothing_and_still_reads() {
    let f = Fixture::new("disabled", HistoryConfig::default());
    f.keep("a.md", Kind::Bulk, "old");
    f.history.set_config(HistoryConfig {
        enabled: false,
        ..HistoryConfig::default()
    });
    assert!(f.keep("a.md", Kind::Bulk, "newer").is_none());
    assert_eq!(f.texts("a.md"), ["old"]);
}

#[test]
fn a_missing_note_has_nothing_to_keep() {
    let f = Fixture::new("missing", HistoryConfig::default());
    let kept = f
        .history
        .keep(Path::new("a.md"), Kind::Bulk, || {
            Err(std::io::ErrorKind::NotFound.into())
        })
        .unwrap();
    assert!(kept.is_none());
    let denied = f.history.keep(Path::new("a.md"), Kind::Bulk, || {
        Err(std::io::ErrorKind::PermissionDenied.into())
    });
    assert!(matches!(denied, Err(Error::Io { .. })));
}

#[test]
fn versions_list_newest_first_with_size_and_date() {
    let f = Fixture::new("list", HistoryConfig::default());
    f.keep("a.md", Kind::Edit, "é");
    f.advance(10 * MINUTE);
    f.keep("a.md", Kind::Edit, "second");
    let versions = f.history.list(Path::new("a.md")).unwrap();
    assert_eq!(versions.iter().map(|v| v.bytes).collect::<Vec<_>>(), [6, 2]);
    assert_eq!(
        versions[0].at,
        UNIX_EPOCH + Duration::from_millis(100 * DAY_MS + 600_000)
    );
    assert!(f.history.list(Path::new("none.md")).unwrap().is_empty());
}

#[test]
fn invalid_utf8_is_stored_as_found_and_read_lossy() {
    let f = Fixture::new("bytes", HistoryConfig::default());
    let id = f
        .history
        .keep(Path::new("a.md"), Kind::Bulk, || Ok(b"a\xffb".to_vec()))
        .unwrap()
        .unwrap();
    assert_eq!(
        f.history.read(Path::new("a.md"), &id).unwrap(),
        "a\u{fffd}b"
    );
    assert_eq!(f.history.list(Path::new("a.md")).unwrap()[0].bytes, 3);
}

#[test]
fn equivalent_spellings_of_a_path_name_one_note() {
    let f = Fixture::new("spellings", HistoryConfig::default());
    f.keep("dir/a.md", Kind::Bulk, "one");
    assert_eq!(f.history.list(Path::new("./dir//a.md")).unwrap().len(), 1);
}

#[test]
fn a_renamed_note_takes_its_versions() {
    let f = Fixture::new("rename", HistoryConfig::default());
    f.keep("a.md", Kind::Bulk, "one");
    f.history
        .move_note(Path::new("a.md"), Path::new("sub/b.md"))
        .unwrap();
    assert!(f.history.list(Path::new("a.md")).unwrap().is_empty());
    assert_eq!(f.texts("sub/b.md"), ["one"]);
    f.advance(10 * MINUTE);
    f.keep("sub/b.md", Kind::Edit, "two");
    assert_eq!(f.texts("sub/b.md"), ["two", "one"]);
}

#[test]
fn a_renamed_folder_takes_every_note_below_it_deleted_ones_included() {
    let f = Fixture::new("folder", HistoryConfig::default());
    f.keep("dir/a.md", Kind::Bulk, "a");
    f.keep("dir/deep/b.md", Kind::Bulk, "b");
    f.keep("direct.md", Kind::Bulk, "outside");
    f.keep("dir2/c.md", Kind::Bulk, "prefix of the name only");
    f.history
        .move_folder(Path::new("dir"), Path::new("renamed/dir"))
        .unwrap();
    assert_eq!(f.texts("renamed/dir/a.md"), ["a"]);
    assert_eq!(f.texts("renamed/dir/deep/b.md"), ["b"]);
    assert_eq!(f.texts("direct.md"), ["outside"]);
    assert_eq!(f.texts("dir2/c.md"), ["prefix of the name only"]);
    assert!(f.history.list(Path::new("dir/a.md")).unwrap().is_empty());
}

#[test]
fn versions_moved_onto_a_name_that_already_has_some_are_merged() {
    let f = Fixture::new("merge", HistoryConfig::default());
    f.keep("old.md", Kind::Bulk, "from old");
    f.advance(MINUTE);
    f.keep("new.md", Kind::Bulk, "from new");
    f.history
        .move_note(Path::new("old.md"), Path::new("new.md"))
        .unwrap();
    assert_eq!(f.texts("new.md"), ["from new", "from old"]);
}

#[test]
fn a_note_moved_to_the_trash_keeps_its_versions_until_they_expire() {
    let f = Fixture::new(
        "trash",
        HistoryConfig {
            days: 14,
            ..HistoryConfig::default()
        },
    );
    f.keep("a.md", Kind::Bulk, "one");
    f.history
        .move_note(Path::new("a.md"), Path::new(".trash/a.md"))
        .unwrap();
    assert_eq!(f.texts(".trash/a.md"), ["one"]);
    f.advance(Duration::from_secs(13 * 86_400));
    f.history.prune().unwrap();
    assert_eq!(f.texts(".trash/a.md"), ["one"]);
    f.advance(Duration::from_secs(2 * 86_400));
    f.history.prune().unwrap();
    assert!(f.history.list(Path::new(".trash/a.md")).unwrap().is_empty());
}

#[test]
fn prune_drops_versions_older_than_the_days_and_their_unshared_contents() {
    let f = Fixture::new(
        "days",
        HistoryConfig {
            days: 7,
            ..HistoryConfig::default()
        },
    );
    f.keep("a.md", Kind::Bulk, "ancient");
    f.keep("b.md", Kind::Bulk, "shared");
    f.advance(Duration::from_secs(5 * 86_400));
    f.keep("c.md", Kind::Bulk, "shared");
    f.keep("a.md", Kind::Bulk, "recent");
    f.advance(Duration::from_secs(3 * 86_400));
    let report = f.history.prune().unwrap();
    assert_eq!(report.versions, 2);
    assert_eq!(report.bytes, "ancient".len() as u64);
    assert_eq!(f.texts("a.md"), ["recent"]);
    assert!(f.texts("b.md").is_empty());
    assert_eq!(f.texts("c.md"), ["shared"]);
    assert!(
        !f.dir
            .join("notes")
            .join(note_key(Path::new("b.md")))
            .exists()
    );
}

#[test]
fn prune_drops_the_oldest_versions_until_the_size_fits() {
    let f = Fixture::new(
        "size",
        HistoryConfig {
            max_bytes: 25,
            ..HistoryConfig::default()
        },
    );
    for (i, name) in ["a.md", "b.md", "c.md", "d.md"].iter().enumerate() {
        f.keep(name, Kind::Bulk, &format!("{i}{}", "x".repeat(9)));
        f.advance(MINUTE);
    }
    let report = f.history.prune().unwrap();
    assert_eq!(report.versions, 2);
    assert_eq!(f.history.size().unwrap(), 20);
    assert!(f.texts("a.md").is_empty() && f.texts("b.md").is_empty());
    assert_eq!(f.texts("d.md").len(), 1);
}

#[test]
fn a_content_shared_by_a_surviving_version_is_not_dropped_with_the_oldest() {
    let f = Fixture::new(
        "size-shared",
        HistoryConfig {
            max_bytes: 10,
            ..HistoryConfig::default()
        },
    );
    f.keep("a.md", Kind::Bulk, "0123456789");
    f.advance(MINUTE);
    f.keep("b.md", Kind::Bulk, "0123456789");
    f.history.prune().unwrap();
    assert!(f.texts("a.md").len() == 1 && f.texts("b.md").len() == 1);
}

#[test]
fn prune_removes_orphan_contents_and_abandoned_temporaries_only() {
    let f = Fixture::new("orphans", HistoryConfig::default());
    f.keep("a.md", Kind::Bulk, "kept");
    let orphan = f.history.blob_path(&"ab".repeat(32));
    fs::create_dir_all(orphan.parent().unwrap()).unwrap();
    fs::write(&orphan, "orphan").unwrap();
    let shard = orphan.parent().unwrap().to_path_buf();
    fs::write(shard.join(".tmp-1-1"), "recent").unwrap();
    f.history.prune().unwrap();
    assert!(!orphan.exists());
    assert!(
        shard.join(".tmp-1-1").exists(),
        "a recent temporary may be in use"
    );
    f.advance(Duration::from_secs(2 * 3600));
    let file = fs::File::options()
        .write(true)
        .open(shard.join(".tmp-1-1"))
        .unwrap();
    file.set_modified(UNIX_EPOCH + Duration::from_millis(100 * DAY_MS))
        .unwrap();
    f.history.prune().unwrap();
    assert!(!shard.join(".tmp-1-1").exists());
    assert_eq!(f.texts("a.md"), ["kept"]);
}

fn modes(dir: &Path, found: &mut Vec<(PathBuf, u32, bool)>) {
    for item in fs::read_dir(dir).unwrap() {
        let item = item.unwrap();
        let meta = item.metadata().unwrap();
        found.push((
            item.path(),
            meta.permissions().mode() & 0o777,
            meta.is_dir(),
        ));
        if meta.is_dir() {
            modes(&item.path(), found);
        }
    }
}

#[test]
fn folders_are_private_and_files_are_readable_by_the_owner_only() {
    let _guard = UmaskGuard::set(0o000);
    let f = Fixture::new("modes", HistoryConfig::default());
    f.keep("a/b.md", Kind::Bulk, "secret");
    f.history
        .move_folder(Path::new("a"), Path::new("c"))
        .unwrap();
    f.history.prune().unwrap();
    f.keep("c/b.md", Kind::Bulk, "secret two");
    let mut found = vec![(
        f.dir.clone(),
        fs::metadata(&f.dir).unwrap().permissions().mode() & 0o777,
        true,
    )];
    modes(&f.dir, &mut found);
    assert!(found.len() > 8);
    for (path, mode, is_dir) in found {
        assert_eq!(
            mode,
            if is_dir { 0o700 } else { 0o600 },
            "{}",
            path.display()
        );
    }
}

struct UmaskGuard(libc_umask::Mode);
mod libc_umask {
    pub type Mode = u32;
    unsafe extern "C" {
        fn umask(mask: u32) -> u32;
    }
    pub fn set(mask: Mode) -> Mode {
        // SAFETY: umask only changes the process file mode creation mask.
        unsafe { umask(mask) }
    }
}
impl UmaskGuard {
    fn set(mask: u32) -> UmaskGuard {
        UmaskGuard(libc_umask::set(mask))
    }
}
impl Drop for UmaskGuard {
    fn drop(&mut self) {
        libc_umask::set(self.0);
    }
}

#[test]
fn an_existing_loose_store_is_made_private_when_opened() {
    let dir = test_dir("loosen").join("store");
    fs::create_dir_all(dir.join("blobs")).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(dir.join("blobs"), fs::Permissions::from_mode(0o755)).unwrap();
    History::open(&dir, HistoryConfig::default()).unwrap();
    assert_eq!(
        fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(dir.join("blobs"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
fn damage_to_entries_loses_no_other_content() {
    let f = Fixture::new("damage", HistoryConfig::default());
    for note in ["a.md", "b.md", "c.md"] {
        f.keep(note, Kind::Bulk, &format!("content of {note}"));
    }
    let a = f.history.note_dir(Path::new("a.md"));
    fs::write(a.join("garbage"), "x").unwrap();
    fs::write(a.join("0000-zz-1-abc"), "x").unwrap();
    assert_eq!(f.texts("a.md").len(), 1, "foreign names are ignored");

    let b_blob = f
        .history
        .blob_path(&hex(&Sha256::digest(b"content of b.md")));
    fs::remove_file(&b_blob).unwrap();
    assert!(
        f.history.list(Path::new("b.md")).unwrap().is_empty(),
        "missing content is hidden"
    );

    let c = f.history.list(Path::new("c.md")).unwrap();
    let c_blob = f
        .history
        .blob_path(&hex(&Sha256::digest(b"content of c.md")));
    fs::write(&c_blob, "content of c.mX").unwrap();
    assert!(matches!(
        f.history.read(Path::new("c.md"), &c[0].id),
        Err(Error::Damaged(_))
    ));
    fs::write(&c_blob, "short").unwrap();
    assert!(
        f.history.list(Path::new("c.md")).unwrap().is_empty(),
        "a short content is hidden"
    );

    let report = f.history.prune().unwrap();
    assert_eq!(report.versions, 2);
    assert_eq!(f.texts("a.md"), ["content of a.md"]);
}

#[test]
fn a_lost_path_file_is_rewritten_by_the_next_version() {
    let f = Fixture::new("lost-path", HistoryConfig::default());
    f.keep("dir/a.md", Kind::Bulk, "one");
    f.advance(MINUTE);
    fs::remove_file(f.history.note_dir(Path::new("dir/a.md")).join("path")).unwrap();
    f.history
        .move_folder(Path::new("dir"), Path::new("moved"))
        .unwrap();
    assert_eq!(
        f.texts("dir/a.md"),
        ["one"],
        "not found by the folder move, still listed"
    );
    f.keep("dir/a.md", Kind::Bulk, "two");
    f.history
        .move_folder(Path::new("dir"), Path::new("moved"))
        .unwrap();
    assert_eq!(f.texts("moved/a.md"), ["two", "one"]);
}

#[test]
fn deleting_the_whole_store_between_calls_is_survivable() {
    let f = Fixture::new("wiped", HistoryConfig::default());
    f.keep("a.md", Kind::Bulk, "one");
    fs::remove_dir_all(f.dir.join("notes")).unwrap();
    fs::remove_dir_all(f.dir.join("blobs")).unwrap();
    assert!(f.history.list(Path::new("a.md")).unwrap().is_empty());
    assert!(matches!(f.history.prune(), Err(Error::Io { .. })));
}

#[test]
fn threads_writing_and_pruning_together_lose_no_version() {
    let dir = test_dir("threads").join("store");
    let config = HistoryConfig::default();
    let writers: Vec<_> = (0..4)
        .map(|w| {
            let dir = dir.clone();
            std::thread::spawn(move || {
                let history = History::open(&dir, config).unwrap();
                for i in 0..40 {
                    let note = format!("n{}.md", i % 5);
                    let content = format!("writer {w} version {i}");
                    history
                        .keep(Path::new(&note), Kind::Bulk, || Ok(content.into_bytes()))
                        .unwrap();
                }
            })
        })
        .collect();
    let pruner = {
        let dir = dir.clone();
        std::thread::spawn(move || {
            let history = History::open(&dir, config).unwrap();
            for _ in 0..30 {
                history.prune().unwrap();
            }
        })
    };
    for t in writers {
        t.join().unwrap();
    }
    pruner.join().unwrap();
    let history = History::open(&dir, config).unwrap();
    let mut total = 0;
    for n in 0..5 {
        let rel = PathBuf::from(format!("n{n}.md"));
        for v in history.list(&rel).unwrap() {
            history.read(&rel, &v.id).unwrap();
            total += 1;
        }
    }
    assert_eq!(total, 4 * 40);
}

const CHILD_ENV: &str = "FOCUS_HISTORY_TEST_CHILD";

#[test]
fn child_writer() {
    let Some(spec) = std::env::var_os(CHILD_ENV) else {
        return;
    };
    let spec = spec.to_string_lossy().into_owned();
    let (dir, id) = spec.rsplit_once('|').unwrap();
    let history = History::open(Path::new(dir), HistoryConfig::default()).unwrap();
    for i in 0..30 {
        let content = format!("process {id} version {i}");
        history
            .keep(Path::new(&format!("n{}.md", i % 3)), Kind::Bulk, || {
                Ok(content.into_bytes())
            })
            .unwrap();
        if i % 10 == 0 {
            history.prune().unwrap();
        }
    }
}

#[test]
fn processes_writing_together_lose_no_version() {
    let dir = test_dir("processes").join("store");
    let exe = std::env::current_exe().unwrap();
    let children: Vec<_> = (0..3)
        .map(|id| {
            std::process::Command::new(&exe)
                .args(["--exact", "tests::child_writer", "--test-threads=1"])
                .env(CHILD_ENV, format!("{}|{id}", dir.display()))
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    let history = History::open(&dir, HistoryConfig::default()).unwrap();
    let mut total = 0;
    for n in 0..3 {
        let rel = PathBuf::from(format!("n{n}.md"));
        for v in history.list(&rel).unwrap() {
            history.read(&rel, &v.id).unwrap();
            total += 1;
        }
    }
    assert_eq!(total, 3 * 30);
}

#[test]
fn clear_drops_every_version_and_content_and_leaves_a_usable_store() {
    let config = HistoryConfig {
        days: 9,
        max_bytes: 5_000_000,
        ..HistoryConfig::default()
    };
    let f = Fixture::new("clear", config);
    for note in ["a.md", "b/c.md"] {
        f.keep(note, Kind::Bulk, "one");
        f.advance(MINUTE);
        f.keep(note, Kind::Bulk, "two");
    }
    f.keep("a.md", Kind::Bulk, "three");
    let orphan = f.history.blob_path(&"cd".repeat(32));
    fs::create_dir_all(orphan.parent().unwrap()).unwrap();
    fs::write(&orphan, "orphan").unwrap();
    let report = f.history.clear().unwrap();
    assert_eq!(report.versions, 5);
    assert_eq!(
        report.bytes,
        ("one".len() + "two".len() + "three".len() + 6) as u64
    );
    assert_eq!(f.history.size().unwrap(), 0);
    assert!(f.history.list(Path::new("a.md")).unwrap().is_empty());
    assert!(f.history.list(Path::new("b/c.md")).unwrap().is_empty());
    assert_eq!(fs::read_dir(f.dir.join("notes")).unwrap().count(), 0);
    assert_eq!(f.history.config(), config);
    assert_eq!(f.history.clear().unwrap(), PruneReport::default());
    assert!(f.keep("a.md", Kind::Edit, "four").is_some());
    assert_eq!(f.texts("a.md"), ["four"]);
}

#[test]
fn clearing_while_two_writers_keep_versions_leaves_a_consistent_store() {
    let dir = test_dir("clear-writers").join("store");
    let config = HistoryConfig::default();
    let writers: Vec<_> = (0..2)
        .map(|w| {
            let dir = dir.clone();
            std::thread::spawn(move || {
                let history = History::open(&dir, config).unwrap();
                for i in 0..60 {
                    let content = format!("writer {w} version {i}");
                    history
                        .keep(Path::new(&format!("n{}.md", i % 4)), Kind::Bulk, || {
                            Ok(content.into_bytes())
                        })
                        .unwrap();
                }
            })
        })
        .collect();
    let clearer = {
        let dir = dir.clone();
        std::thread::spawn(move || {
            let history = History::open(&dir, config).unwrap();
            for _ in 0..20 {
                history.clear().unwrap();
            }
        })
    };
    for t in writers {
        t.join().unwrap();
    }
    clearer.join().unwrap();
    let history = History::open(&dir, config).unwrap();
    for n in 0..4 {
        let rel = PathBuf::from(format!("n{n}.md"));
        for v in history.list(&rel).unwrap() {
            history.read(&rel, &v.id).unwrap();
        }
    }
    history.clear().unwrap();
    assert_eq!(history.size().unwrap(), 0);
    assert_eq!(fs::read_dir(dir.join("notes")).unwrap().count(), 0);
    assert!(
        history
            .keep(Path::new("n0.md"), Kind::Bulk, || Ok(b"after".to_vec()))
            .unwrap()
            .is_some()
    );
}
