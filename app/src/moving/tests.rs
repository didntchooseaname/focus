use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use gpui::{AppContext as _, Entity, EntityInputHandler as _, TestAppContext, VisualTestContext};
use gpui_component::RopeExt as _;

use crate::editing::{EditorBounds, EditorFont};
use crate::editor::{EditorOptions, NoteEditor, ViewMode};
use crate::format::{self, Block, Inline};
use crate::library::Library;

const NOTE: &str = "Moves.md";
const TEXT: &str = "Intro line.\n\nFirst paragraph with plain words.\n\nSecond paragraph.\n\n- one\n- two\n  - two a\n- three\n";

fn library(name: &str, text: &str) -> Arc<Library> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/test-tmp/editing-actions")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    let notes = root.join("vault");
    std::fs::create_dir_all(&notes).expect("a vault under target/");
    std::fs::write(notes.join(NOTE), text).expect("the note written");
    let library = Library::open_in(
        &notes,
        root.join("cache"),
        Vec::new(),
        focus_core::ExternalLinks::Skip,
        None,
    )
    .expect("the vault opened");
    Arc::new(library)
}

fn editor(
    library: &Arc<Library>,
    view: ViewMode,
    cx: &mut VisualTestContext,
) -> Entity<NoteEditor> {
    let id = library.note_by_path(Path::new(NOTE)).expect("listed");
    let meta = library.note(id).expect("its metadata");
    let library = library.clone();
    let editor = cx.update(|window, cx| {
        cx.new(|cx| {
            let options = EditorOptions {
                view,
                line_numbers: false,
                typesetting: focus_view_reading::view::Typesetting::compact(640.),
                autosave: Duration::from_secs(60),
                wrap: true,
                tab_width: 4,
                font: EditorFont::from_choice(0),
                bounds: EditorBounds::default(),
                edit_in_place: true,
            };
            NoteEditor::new(meta, library, options, window, cx)
        })
    });
    cx.update(|window, cx| editor.update(cx, |editor, cx| editor.load(window, cx).detach()));
    cx.run_until_parked();
    editor
}

fn select(editor: &Entity<NoteEditor>, needle: &str, end: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        let (input, reading, view) = {
            let e = editor.read(cx);
            (e.input.clone(), e.reading.clone(), e.view)
        };
        let text = input.read(cx).text().to_string();
        let start = text.find(needle).expect("in the note");
        let stop = text[start..].find(end).expect("after it") + start + end.len();
        if view == ViewMode::Live {
            let line = text[..start].matches('\n').count() as u32;
            reading.update(cx, |reading, cx| {
                reading.place_caret_at(line, 0, window, cx)
            });
        }
        input.update(cx, |input, cx| input.set_selection(start, stop, cx));
    });
}

fn caret_on(editor: &Entity<NoteEditor>, needle: &str, cx: &mut VisualTestContext) {
    select(editor, needle, "", cx);
    cx.update(|_, cx| {
        let input = editor.read(cx).input.clone();
        let at = input.read(cx).cursor();
        input.update(cx, |input, cx| input.set_selection(at, at, cx));
    });
}

fn text(editor: &Entity<NoteEditor>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| editor.read(cx).input.read(cx).text().to_string())
}

fn selected(editor: &Entity<NoteEditor>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| editor.read(cx).input.read(cx).selection_text())
}

fn undo(editor: &Entity<NoteEditor>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        let input = editor.read(cx).input.clone();
        input.update(cx, |input, cx| input.undo_change(window, cx));
    });
    cx.run_until_parked();
}

fn press_format(editor: &Entity<NoteEditor>, kind: Inline, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        if editor.read(cx).view == ViewMode::Live {
            assert!(format::apply_in_place(editor, Some(kind), window, cx));
        } else {
            let input = editor.read(cx).input.clone();
            format::apply_field(&input, Some(kind), window, cx);
        }
    });
    cx.run_until_parked();
}

fn each_view(test: impl Fn(ViewMode, &mut VisualTestContext), cx: &mut TestAppContext) {
    for view in [ViewMode::Source, ViewMode::Live] {
        let window = cx.add_empty_window();
        test(view, window);
    }
}

#[gpui::test]
fn each_format_is_applied_and_removed_on_a_selection(cx: &mut TestAppContext) {
    each_view(
        |view, cx| {
            let library = library(&format!("formats-{view:?}"), TEXT);
            let editor = editor(&library, view, cx);
            for (kind, marked) in [
                (Inline::Bold, "**plain**"),
                (Inline::Italic, "*plain*"),
                (Inline::Strike, "~~plain~~"),
                (Inline::Highlight, "==plain=="),
                (Inline::Code, "`plain`"),
            ] {
                select(&editor, "plain", "plain", cx);
                press_format(&editor, kind, cx);
                assert_eq!(
                    text(&editor, cx),
                    TEXT.replace("plain", marked),
                    "{view:?} {kind:?} applied"
                );
                assert_eq!(selected(&editor, cx), "plain", "the text stays selected");
                let active = cx.update(|window, cx| {
                    let input = editor.read(cx).input.clone();
                    let text = input.read(cx).text().to_string();
                    let range = input.update(cx, |input, cx| {
                        let s = input
                            .selected_text_range(true, window, cx)
                            .expect("a range");
                        let t = input.text();
                        t.offset_utf16_to_offset(s.range.start)
                            ..t.offset_utf16_to_offset(s.range.end)
                    });
                    format::active(&text, &range, kind)
                });
                assert!(active, "{view:?} {kind:?}: the button shows it active");
                press_format(&editor, kind, cx);
                assert_eq!(text(&editor, cx), TEXT, "{view:?} {kind:?} removed");
            }
            select(&editor, "plain", "plain", cx);
            press_format(&editor, Inline::Bold, cx);
            std::thread::sleep(Duration::from_millis(1100));
            press_format(&editor, Inline::Bold, cx);
            undo(&editor, cx);
            assert_eq!(
                text(&editor, cx),
                TEXT.replace("plain", "**plain**"),
                "{view:?}: one undo step takes the removal back"
            );
            undo(&editor, cx);
            assert_eq!(text(&editor, cx), TEXT, "{view:?}: and one the application");
        },
        cx,
    );
}

#[gpui::test]
fn a_block_type_changes_the_blocks_the_selection_touches(cx: &mut TestAppContext) {
    each_view(
        |view, cx| {
            let library = library(&format!("types-{view:?}"), TEXT);
            let editor = editor(&library, view, cx);
            select(&editor, "plain words", "Second", cx);
            cx.update(|window, cx| format::set_block_type(&editor, Block::Heading(2), window, cx));
            cx.run_until_parked();
            let changed = TEXT
                .replace("First paragraph", "## First paragraph")
                .replace("Second paragraph", "## Second paragraph");
            assert_eq!(text(&editor, cx), changed, "{view:?}: both blocks");
            let shown = cx.update(|_, cx| {
                let text = editor.read(cx).input.read(cx).text().to_string();
                let at = text.find("First").expect("kept");
                format::block_at(&text, at)
            });
            assert_eq!(shown, Block::Heading(2), "the bar names the new type");
            undo(&editor, cx);
            assert_eq!(text(&editor, cx), TEXT, "{view:?}: one undo step");
        },
        cx,
    );
}

#[gpui::test]
fn a_block_moves_with_its_bytes_and_one_undo_step(cx: &mut TestAppContext) {
    each_view(
        |view, cx| {
            let library = library(&format!("moves-{view:?}"), TEXT);
            let editor = editor(&library, view, cx);
            let step = |down: bool, cx: &mut VisualTestContext| {
                let moved = cx.update(|window, cx| {
                    editor.update(cx, |editor, cx| editor.move_block(down, window, cx))
                });
                cx.run_until_parked();
                moved
            };
            caret_on(&editor, "Second paragraph", cx);
            assert!(step(false, cx), "{view:?}: up");
            assert_eq!(
                text(&editor, cx),
                "Intro line.\n\nSecond paragraph.\n\nFirst paragraph with plain words.\n\n- one\n- two\n  - two a\n- three\n"
            );
            undo(&editor, cx);
            assert_eq!(text(&editor, cx), TEXT, "{view:?}: one undo step");
            caret_on(&editor, "Second paragraph", cx);
            assert!(
                step(false, cx) && step(false, cx),
                "{view:?}: to the first place"
            );
            assert!(text(&editor, cx).starts_with("Second paragraph.\n\nIntro line."));
            assert!(!step(false, cx), "{view:?}: nothing above the first place");
            let second = text(&editor, cx);
            caret_on(&editor, "Intro", cx);
            for _ in 0..2 {
                assert!(step(true, cx), "{view:?}: down");
            }
            assert!(
                text(&editor, cx).ends_with("- three\n\nIntro line.\n"),
                "{view:?}: to the last place"
            );
            assert!(!step(true, cx), "{view:?}: nothing under the last place");
            let mut before: Vec<&str> = second.lines().collect();
            let after = text(&editor, cx);
            let mut now: Vec<&str> = after.lines().collect();
            before.sort_unstable();
            now.sort_unstable();
            assert_eq!(before, now);
            caret_on(&editor, "two\n", cx);
            assert!(step(true, cx), "{view:?}: the item down");
            assert!(
                text(&editor, cx).contains("- one\n- three\n- two\n  - two a\n"),
                "{view:?}: with its sub-item"
            );
            assert!(!step(true, cx), "{view:?}: the last item of its list");
        },
        cx,
    );
}

#[gpui::test]
fn a_block_is_refused_where_it_would_join_its_neighbour(cx: &mut TestAppContext) {
    each_view(
        |view, cx| {
            let note = "Intro.\n\n## Head\nBody.\n";
            let library = library(&format!("refused-{view:?}"), note);
            let editor = editor(&library, view, cx);
            caret_on(&editor, "Intro", cx);
            let moved = cx.update(|window, cx| {
                editor.update(cx, |editor, cx| editor.move_block(true, window, cx))
            });
            cx.run_until_parked();
            assert!(!moved, "{view:?}: under the heading, over its paragraph");
            assert_eq!(text(&editor, cx), note, "{view:?}: the note is untouched");
        },
        cx,
    );
}

#[gpui::test]
fn a_folded_section_moves_whole_in_live(cx: &mut TestAppContext) {
    let note = "## A\n\nA body.\n\n## B\n\nB body.\n";
    let library = library("folded", note);
    let cx = cx.add_empty_window();
    let editor = editor(&library, ViewMode::Live, cx);
    cx.update(|_, cx| {
        let reading = editor.read(cx).reading.clone();
        reading.update(cx, |reading, cx| reading.toggle_section(0, cx));
    });
    caret_on(&editor, "## A", cx);
    let moved =
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.move_block(true, window, cx)));
    cx.run_until_parked();
    assert!(moved);
    assert_eq!(
        text(&editor, cx),
        "## B\n\n## A\n\nA body.\n\nB body.\n",
        "the heading and the section it hides pass the next block shown"
    );
}
