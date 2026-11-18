use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};

use focus_history::{History, HistoryConfig, Kind};

use super::super::{Shared, same_theirs, takes_theirs};
use super::{Found, Target, Theirs, compare};

const BASE: &str = "# Plan\n\nLine 1.\nLine 2.\nLine 3.\n";
const OURS: &str = "Draft.# Plan\n\nLine 1.\nLine 2.\nLine 3.\n";
const THEIRS: &str = "# Plan\n\nLine 1.\nLine 2.\nLine 3.\nAdded on the phone.\n";
const MERGED: &str = "Draft.# Plan\n\nLine 1.\nLine 2.\nLine 3.\nAdded on the phone.\n";

fn history(name: &str) -> Arc<History> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/test-tmp/conflict")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    Arc::new(History::open(&dir, HistoryConfig::default()).expect("a history under target/"))
}

fn target(history: &Arc<History>, note: &str) -> Target {
    Some((history.clone(), PathBuf::from(note)))
}

fn compared(target: &Target) -> Theirs {
    match compare(THEIRS.into(), None, BASE, OURS.into(), target) {
        Found::Differs { theirs, kept } => {
            kept.expect("both texts kept");
            theirs
        }
        Found::Same { .. } => panic!("the texts differ"),
    }
}

fn versions(history: &History, note: &str) -> Vec<(Kind, String)> {
    let rel = Path::new(note);
    history
        .list(rel)
        .expect("versions listed")
        .iter()
        .map(|v| (v.kind, history.read(rel, &v.id).expect("version read")))
        .collect()
}

fn assert_kept_once(history: &History, note: &str) {
    let mut kept = versions(history, note);
    kept.sort_by_key(|(kind, _)| *kind == Kind::Unsaved);
    assert_eq!(
        kept,
        vec![
            (Kind::External, THEIRS.to_string()),
            (Kind::Unsaved, OURS.to_string())
        ],
        "{note}: the text on disk and the unsaved text, each once"
    );
}

fn shared(conflict: bool, theirs: Option<Arc<Theirs>>) -> Shared {
    Shared {
        text: OURS.into(),
        loaded: BASE.into(),
        disk_modified: None,
        dirty: true,
        conflict,
        theirs,
    }
}

#[test]
fn a_leader_that_closed_after_keeping_both_texts_is_not_kept_again() {
    let history = history("after");
    let note = target(&history, "Sync/Plan.md");
    let closed = compared(&note);
    let leading = compared(&note);
    assert_kept_once(&history, "Sync/Plan.md");
    assert_eq!(
        leading.merged.as_deref(),
        Some(MERGED),
        "the banner offers Merge"
    );
    assert_eq!(leading.merged, closed.merged);
}

#[test]
fn a_leader_that_closed_before_its_comparison_ran_leaves_both_texts_to_keep() {
    let history = history("before");
    let leading = compared(&target(&history, "Sync/Plan.md"));
    assert_kept_once(&history, "Sync/Plan.md");
    assert_eq!(
        leading.merged.as_deref(),
        Some(MERGED),
        "the banner offers Merge"
    );
}

#[test]
fn comparisons_running_at_once_keep_each_text_once() {
    const TABS: usize = 8;
    let history = history("together");
    for n in 0..200 {
        let note = format!("Sync/Plan {n}.md");
        let start = Barrier::new(TABS);
        std::thread::scope(|scope| {
            for _ in 0..TABS {
                scope.spawn(|| {
                    let target = target(&history, &note);
                    start.wait();
                    compared(&target)
                });
            }
        });
        assert_kept_once(&history, &note);
    }
}

#[test]
fn a_follower_holds_no_comparison_while_the_leader_compares() {
    let comparing = shared(true, None);
    assert!(takes_theirs(true, &comparing));
    let earlier = Arc::new(compared(&target(&history("earlier"), "Sync/Plan.md")));
    assert!(
        !same_theirs(&Some(earlier), &comparing.theirs),
        "a follower that still held an older comparison notices the change"
    );
    assert!(!takes_theirs(false, &comparing));
}

#[test]
fn a_comparison_handed_over_is_seen_by_reference() {
    let theirs = Arc::new(compared(&target(&history("handed"), "Sync/Plan.md")));
    let copy = Arc::new(Theirs::clone(&theirs));
    assert!(same_theirs(&Some(theirs.clone()), &Some(theirs.clone())));
    assert!(
        !same_theirs(&Some(theirs.clone()), &Some(copy)),
        "a new comparison with the same texts is handed over too"
    );
    assert!(takes_theirs(false, &shared(false, None)));
    assert!(takes_theirs(false, &shared(true, Some(theirs))));
}

mod handover {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use focus_history::{HistoryConfig, Kind};
    use gpui::{
        AppContext as _, Entity, EntityInputHandler as _, TestAppContext, VisualTestContext,
    };

    use super::{BASE, MERGED, OURS, THEIRS};
    use crate::editing::{EditorBounds, EditorFont};
    use crate::editor::{EditorOptions, NoteEditor, ViewMode};
    use crate::library::Library;

    const NOTE: &str = "Sync/Plan.md";
    const AUTOSAVE: Duration = Duration::from_secs(5);

    struct Vault {
        library: Arc<Library>,
        root: PathBuf,
    }

    fn vault(name: &str) -> Vault {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/test-tmp/conflict-handover")
            .join(name);
        let _ = std::fs::remove_dir_all(&root);
        let notes = root.join("vault");
        std::fs::create_dir_all(notes.join("Sync")).expect("a vault under target/");
        std::fs::write(notes.join(NOTE), BASE).expect("the note written");
        let library = Library::open_in(
            &notes,
            root.join("cache"),
            Vec::new(),
            focus_core::ExternalLinks::Skip,
            None,
        )
        .expect("the vault opened");
        library
            .vault()
            .set_history(&root.join("history"), HistoryConfig::default())
            .expect("the history attached");
        Vault {
            library: Arc::new(library),
            root,
        }
    }

    fn editor(vault: &Vault, cx: &mut VisualTestContext) -> Entity<NoteEditor> {
        let id = vault
            .library
            .note_by_path(Path::new(NOTE))
            .expect("the note listed");
        let meta = vault.library.note(id).expect("its metadata");
        let library = vault.library.clone();
        cx.update(|window, cx| {
            cx.new(|cx| {
                let options = EditorOptions {
                    view: ViewMode::Source,
                    line_numbers: false,
                    typesetting: focus_view_reading::view::Typesetting::compact(640.),
                    autosave: AUTOSAVE,
                    wrap: true,
                    tab_width: 4,
                    font: EditorFont::from_choice(0),
                    bounds: EditorBounds::default(),
                    edit_in_place: false,
                };
                NoteEditor::new(meta, library, options, window, cx)
            })
        })
    }

    fn sync(from: &Entity<NoteEditor>, to: &Entity<NoteEditor>, cx: &mut VisualTestContext) {
        cx.update(|window, cx| {
            let Some(shared) = from.read(cx).shared(cx) else {
                return;
            };
            if to.read(cx).behind(&shared, cx) {
                to.update(cx, |to, cx| to.follow(&shared, window, cx));
            }
        });
    }

    fn versions(vault: &Vault) -> Vec<(Kind, String)> {
        let history = vault.library.vault().history().expect("a history");
        let rel = Path::new(NOTE);
        let mut kept: Vec<(Kind, String)> = history
            .list(rel)
            .expect("versions listed")
            .iter()
            .map(|v| (v.kind, history.read(rel, &v.id).expect("version read")))
            .collect();
        kept.sort_by_key(|(kind, _)| *kind == Kind::Unsaved);
        kept
    }

    fn rewrite(vault: &Vault, tabs: &[&Entity<NoteEditor>], cx: &mut VisualTestContext) {
        let id = vault
            .library
            .note_by_path(Path::new(NOTE))
            .expect("the note listed");
        let events = vault.library.vault().subscribe();
        let note = vault.root.join("vault").join(NOTE);
        let beside = note.with_file_name(".Plan.md.tmp");
        std::fs::write(&beside, THEIRS).expect("written beside the note");
        std::fs::rename(&beside, &note).expect("renamed over the note");
        loop {
            let event = events
                .recv_timeout(Duration::from_secs(10))
                .expect("the vault reports the change");
            if matches!(event, focus_core::VaultEvent::NoteChanged(changed) if changed == id) {
                vault.library.apply(&event);
                break;
            }
        }
        cx.update(|window, cx| {
            for tab in tabs {
                tab.update(cx, |tab, cx| {
                    if let Some(task) = tab.external_change(window, cx) {
                        task.detach();
                    }
                });
            }
        });
    }

    fn conflict_pending(
        vault: &Vault,
        cx: &mut VisualTestContext,
    ) -> (Entity<NoteEditor>, Entity<NoteEditor>) {
        let leader = editor(vault, cx);
        cx.update(|window, cx| leader.update(cx, |leader, cx| leader.load(window, cx).detach()));
        cx.run_until_parked();
        let follower = editor(vault, cx);
        follower.update(cx, |follower, _| follower.follower = true);
        sync(&leader, &follower, cx);
        cx.update(|window, cx| {
            let input = leader.read(cx).input.clone();
            input.update(cx, |input, cx| {
                input.replace_text_in_range(Some(0..0), "Draft.", window, cx)
            });
        });
        sync(&leader, &follower, cx);
        cx.run_until_parked();
        rewrite(vault, &[&leader, &follower], cx);
        sync(&follower, &leader, cx);
        cx.update(|_, cx| {
            let (leader, follower) = (leader.read(cx), follower.read(cx));
            assert!(leader.conflict_task.is_some(), "the leader compares");
            assert!(follower.conflict && follower.theirs.is_none());
            assert_eq!(follower.source(cx), OURS);
        });
        (leader, follower)
    }

    fn lead_and_merge(follower: Entity<NoteEditor>, vault: &Vault, cx: &mut VisualTestContext) {
        follower.update(cx, |follower, cx| follower.lead(cx));
        cx.run_until_parked();
        assert!(
            follower.read_with(cx, |follower, _| follower.can_merge()),
            "the banner of the tab that leads offers Merge"
        );
        let once = vec![
            (Kind::External, THEIRS.to_string()),
            (Kind::Unsaved, OURS.to_string()),
        ];
        assert_eq!(versions(vault), once, "each text kept once");
        cx.update(|window, cx| follower.update(cx, |follower, cx| follower.merge(window, cx)));
        cx.run_until_parked();
        cx.executor().advance_clock(AUTOSAVE);
        cx.run_until_parked();
        let saved = std::fs::read_to_string(vault.root.join("vault").join(NOTE)).expect("read");
        assert_eq!(saved, MERGED, "the merged text saved by the tab that leads");
        assert_eq!(versions(vault), once, "nothing kept by the merge");
    }

    #[gpui::test]
    fn the_leader_closes_before_its_comparison_ran(cx: &mut TestAppContext) {
        let vault = vault("before");
        let cx = cx.add_empty_window();
        let (leader, follower) = conflict_pending(&vault, cx);
        drop(leader);
        lead_and_merge(follower, &vault, cx);
    }

    #[gpui::test]
    fn the_leader_closes_after_its_comparison_ran(cx: &mut TestAppContext) {
        let vault = vault("after");
        let dispatcher = cx.dispatcher.clone();
        let cx = cx.add_empty_window();
        let (leader, follower) = conflict_pending(&vault, cx);
        while dispatcher.tick(true) {}
        assert!(
            leader.read_with(cx, |leader, _| leader.conflict_task.is_some()),
            "the outcome has not come back"
        );
        assert_eq!(versions(&vault).len(), 2, "the closed tab kept both texts");
        std::thread::sleep(Duration::from_millis(5));
        drop(leader);
        lead_and_merge(follower, &vault, cx);
    }
}
