use super::*;
use focus_core::{HistoryConfig, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn text(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("{l}\n")).collect()
}

fn numbered(count: usize) -> Vec<String> {
    (1..=count).map(|n| format!("line {n}")).collect()
}

fn line(mark: Mark, number: usize, text: &str) -> Row {
    Row::Line {
        mark,
        number,
        text: text.into(),
    }
}

#[test]
fn unchanged_runs_far_from_a_change_fold() {
    let old = numbered(20).join("\n") + "\n";
    let new = old.replace("line 10\n", "line ten\n");
    let rows = rows(&diff(&old, &new), 3);
    assert_eq!(rows.len(), 1 + 3 + 2 + 3 + 1);
    assert_eq!(rows[0], Row::Fold(6));
    assert_eq!(rows[1], line(Mark::Same, 7, "line 7"));
    assert_eq!(rows[4], line(Mark::Removed, 10, "line 10"));
    assert_eq!(rows[5], line(Mark::Added, 10, "line ten"));
    assert_eq!(rows[6], line(Mark::Same, 11, "line 11"));
    assert_eq!(rows[9], Row::Fold(7));
}

#[test]
fn numbers_follow_the_side_each_line_belongs_to() {
    let old = text(&["a", "b", "c", "d"]);
    let new = text(&["a", "x", "y", "c", "d"]);
    let rows = rows(&diff(&old, &new), 1);
    assert_eq!(
        rows,
        [
            line(Mark::Same, 1, "a"),
            line(Mark::Removed, 2, "b"),
            line(Mark::Added, 2, "x"),
            line(Mark::Added, 3, "y"),
            line(Mark::Same, 4, "c"),
            Row::Fold(1),
        ]
    );
}

#[test]
fn identical_texts_fold_into_one_row() {
    let same = numbered(30).join("\n") + "\n";
    assert_eq!(rows(&diff(&same, &same), 3), [Row::Fold(30)]);
}

#[test]
fn changes_within_the_context_of_each_other_leave_no_fold() {
    let old = numbered(12).join("\n") + "\n";
    let new = old
        .replace("line 4\n", "four\n")
        .replace("line 9\n", "nine\n");
    let rows = rows(&diff(&old, &new), 3);
    assert!(rows.iter().all(|r| !matches!(r, Row::Fold(_))));
}

struct Fixture {
    vault: Arc<Vault>,
    note: NoteId,
    history: Arc<History>,
    path: PathBuf,
}

fn fixture(name: &str, config: HistoryConfig) -> Fixture {
    fixture_with(name, config, &[])
}

fn fixture_with(name: &str, config: HistoryConfig, files: &[(&str, &str)]) -> Fixture {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(format!("view-history-{name}"));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("vault");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Note.md"), "# Note\n\nfirst\n").unwrap();
    for (path, content) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    let vault = Vault::open(
        &root,
        OpenOptions {
            cache_dir: base.join("cache"),
            ignore: Vec::new(),
            watch: false,
        },
    )
    .unwrap();
    vault.set_history(&base.join("history"), config).unwrap();
    let note = vault.note_by_path(Path::new("Note.md")).unwrap();
    let history = vault.history().unwrap();
    Fixture {
        vault,
        note,
        history,
        path: root.join("Note.md"),
    }
}

impl Fixture {
    fn write(&self, content: &str, kind: HistoryKind) -> SystemTime {
        self.vault
            .write_with(self.note, content, None, kind)
            .unwrap()
    }
}

#[test]
fn a_vault_without_history_reports_it() {
    let f = fixture("detached", HistoryConfig::default());
    let base = f.path.parent().unwrap().parent().unwrap().to_path_buf();
    let detached = Vault::open(
        &base.join("vault"),
        OpenOptions {
            cache_dir: base.join("cache-detached"),
            ignore: Vec::new(),
            watch: false,
        },
    )
    .unwrap();
    assert!(matches!(
        snapshot(&detached, f.note),
        Err(Error::Unavailable)
    ));
}

#[test]
fn the_snapshot_lists_versions_newest_first_with_the_current_text() {
    let f = fixture("snapshot", HistoryConfig::default());
    f.write("second\n", HistoryKind::Bulk);
    f.write("third\n", HistoryKind::Bulk);
    let snapshot = snapshot(&f.vault, f.note).unwrap();
    assert_eq!(snapshot.current, "third\n");
    assert!(snapshot.keeping);
    let texts: Vec<String> = snapshot
        .versions
        .iter()
        .map(|v| f.history.read(Path::new("Note.md"), &v.id).unwrap())
        .collect();
    assert_eq!(texts, ["second\n", "# Note\n\nfirst\n"]);
    assert_eq!(
        snapshot.modified,
        std::fs::metadata(&f.path).unwrap().modified().unwrap()
    );
}

#[test]
fn the_snapshot_says_when_the_history_is_off() {
    let f = fixture(
        "off",
        HistoryConfig {
            enabled: false,
            ..HistoryConfig::default()
        },
    );
    assert!(!snapshot(&f.vault, f.note).unwrap().keeping);
}

#[test]
fn a_comparison_counts_lines_against_the_current_text() {
    let f = fixture("compare", HistoryConfig::default());
    f.write("# Note\n\nsecond\nextra\n", HistoryKind::Bulk);
    let snapshot = snapshot(&f.vault, f.note).unwrap();
    let id = &snapshot.versions[0].id;
    let comparison = compare(&f.vault, f.note, id, &snapshot.current).unwrap();
    assert_eq!(comparison.text, "# Note\n\nfirst\n");
    assert_eq!((comparison.added, comparison.removed), (2, 1));
    assert!(!comparison.same());
    assert!(
        comparison
            .rows
            .contains(&line(Mark::Removed, 3, "first\n".trim_end()))
    );
    let same = compare(&f.vault, f.note, id, &comparison.text).unwrap();
    assert!(same.same());
}

#[test]
fn restoring_writes_the_version_and_keeps_the_current_text_first() {
    let f = fixture("restore", HistoryConfig::default());
    f.write("# Note\n\nsecond\n", HistoryKind::Bulk);
    let snapshot = snapshot(&f.vault, f.note).unwrap();
    let wanted = snapshot.versions[0].id.clone();
    restore(&f.vault, f.note, &wanted, snapshot.modified).unwrap();
    assert_eq!(
        std::fs::read_to_string(&f.path).unwrap(),
        "# Note\n\nfirst\n"
    );
    let after = self::snapshot(&f.vault, f.note).unwrap();
    assert_eq!(after.versions.len(), 2);
    assert_eq!(after.versions[0].kind, focus_history::Kind::Restore);
    assert_eq!(
        f.history
            .read(Path::new("Note.md"), &after.versions[0].id)
            .unwrap(),
        "# Note\n\nsecond\n",
        "the text the restore replaced is the newest version"
    );
}

#[test]
fn a_note_changed_since_the_snapshot_is_not_restored() {
    let f = fixture("conflict", HistoryConfig::default());
    f.write("# Note\n\nsecond\n", HistoryKind::Bulk);
    let snapshot = snapshot(&f.vault, f.note).unwrap();
    let wanted = snapshot.versions[0].id.clone();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&f.path, "written elsewhere\n").unwrap();
    let refused = restore(&f.vault, f.note, &wanted, snapshot.modified);
    assert!(matches!(
        refused,
        Err(Error::Vault(VaultError::Conflict(_)))
    ));
    assert_eq!(
        std::fs::read_to_string(&f.path).unwrap(),
        "written elsewhere\n"
    );
}

#[test]
fn conflict_copies_of_the_note_are_found_in_its_folder_only() {
    let f = fixture_with(
        "copies",
        HistoryConfig::default(),
        &[
            (
                "Note.sync-conflict-20241128-143005-MFZWI3D.md",
                "# Note\n\nfrom the phone\n",
            ),
            (
                "Note (Conflicted copy MyMacBook2 202411281430).md",
                "# Note\n\nfrom the mac\n",
            ),
            ("Other.sync-conflict-20241128-143005-MFZWI3D.md", "other\n"),
            (
                "Sub/Note.sync-conflict-20241128-143005-MFZWI3D.md",
                "elsewhere\n",
            ),
            ("Note copy.md", "not a conflict copy\n"),
        ],
    );
    let copies = conflict_copies(&f.vault, f.note);
    let mut found: Vec<(String, CopyOrigin, Option<String>)> = copies
        .iter()
        .map(|c| (c.path.display().to_string(), c.origin, c.device.clone()))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        found,
        [
            (
                "Note (Conflicted copy MyMacBook2 202411281430).md".to_string(),
                CopyOrigin::ObsidianSync,
                Some("MyMacBook2".to_string())
            ),
            (
                "Note.sync-conflict-20241128-143005-MFZWI3D.md".to_string(),
                CopyOrigin::Syncthing,
                Some("MFZWI3D".to_string())
            ),
        ]
    );
    let snapshot = snapshot(&f.vault, f.note).unwrap();
    assert_eq!(snapshot.copies.len(), 2);
}

#[test]
fn restoring_a_conflict_copy_keeps_the_current_text_and_the_copy() {
    let copy_path = "Note.sync-conflict-20241128-143005-MFZWI3D.md";
    let f = fixture_with(
        "copy-restore",
        HistoryConfig::default(),
        &[(copy_path, "# Note\n\nfrom the phone\n")],
    );
    let snapshot = snapshot(&f.vault, f.note).unwrap();
    let copy = snapshot.copies[0].note;
    let comparison = compare_copy(&f.vault, copy, &snapshot.current).unwrap();
    assert_eq!(comparison.source, Source::Copy(copy));
    assert_eq!((comparison.added, comparison.removed), (1, 1));
    restore_copy(&f.vault, f.note, copy, snapshot.modified).unwrap();
    assert_eq!(
        std::fs::read_to_string(&f.path).unwrap(),
        "# Note\n\nfrom the phone\n"
    );
    let after = self::snapshot(&f.vault, f.note).unwrap();
    assert_eq!(after.versions[0].kind, focus_history::Kind::Restore);
    assert_eq!(
        f.history
            .read(Path::new("Note.md"), &after.versions[0].id)
            .unwrap(),
        "# Note\n\nfirst\n"
    );
    assert!(f.path.with_file_name(copy_path).exists());
}
