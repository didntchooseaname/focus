use std::ops::Range;

use focus_view_reading::{LiveKind, LiveOpen};
use gpui::{
    AppContext, Context, Entity, EntityInputHandler, IntoElement, Render, Subscription, Window,
};
use gpui_component::input::{InputEvent, InputState, Position};
use gpui_component::{Rope, RopeExt as _};

use crate::editing::{EditorBounds, EditorFont, editor_surface};
use crate::editor::{EditorEvent, NoteEditor, ViewMode};
use crate::shell::Shell;

const MAX_ROWS: usize = 400;

pub struct LiveBlock {
    pub input: Entity<InputState>,
    pub lines: Range<u32>,
    appended: Option<usize>,
    _changes: Subscription,
}

pub struct BlockEditor {
    input: Entity<InputState>,
    font: EditorFont,
    bounds: EditorBounds,
}

impl Render for BlockEditor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        editor_surface(&self.input, self.font, &self.bounds, false)
    }
}

fn line_span(text: &Rope, lines: &Range<u32>) -> Range<usize> {
    let from = text.line_start_offset(lines.start as usize);
    let to = text.line_start_offset(lines.end as usize).max(from);
    let tail = text.slice(to.saturating_sub(2).max(from)..to).to_string();
    let ending = if tail.ends_with("\r\n") {
        2
    } else {
        usize::from(tail.ends_with('\n'))
    };
    from..to - ending
}

fn has_crlf(text: &Rope) -> bool {
    let mut after_cr = false;
    text.chunks().any(|chunk| {
        let found = (after_cr && chunk.starts_with('\n')) || chunk.contains("\r\n");
        after_cr = chunk.ends_with('\r');
        found
    })
}

pub(crate) fn with_endings(value: &str, crlf: bool) -> String {
    if crlf {
        value.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        value.to_string()
    }
}

fn source_is_code(kind: LiveKind, first_line: &str) -> bool {
    let line = first_line.trim_start();
    let fence = line
        .strip_prefix("```")
        .or_else(|| line.strip_prefix("~~~"));
    kind == LiveKind::Properties
        || fence.is_some_and(|info| info.trim_start_matches(['`', '~']).trim() != "tabs")
        || line.starts_with("$$")
        || line.starts_with('|')
}

fn without_blank_end(text: &str, lines: Range<u32>) -> Range<u32> {
    let all: Vec<&str> = text.split_inclusive('\n').collect();
    let mut end = lines.end;
    while end > lines.start + 1
        && all
            .get(end as usize - 1)
            .is_some_and(|line| line.trim().is_empty())
    {
        end -= 1;
    }
    lines.start..end
}

fn end_paragraph(text: &str) -> (String, u32) {
    let added = if text.is_empty() || text.ends_with("\n\n") || text.ends_with("\r\n\r\n") {
        "\n"
    } else if text.ends_with('\n') {
        "\n\n"
    } else {
        "\n\n\n"
    };
    let added = with_endings(added, text.contains("\r\n"));
    let lines = format!("{text}{added}").split_inclusive('\n').count() as u32;
    (added, lines - 1)
}

impl NoteEditor {
    pub fn typing_input(&self) -> Entity<InputState> {
        self.live
            .as_ref()
            .map_or_else(|| self.input.clone(), |block| block.input.clone())
    }

    pub fn set_view(&mut self, view: ViewMode, window: &mut Window, cx: &mut Context<Self>) {
        if view != ViewMode::Live {
            self.close_block(true, window, cx);
            self.reading
                .update(cx, |reading, cx| reading.remove_caret(cx));
        }
        self.view = view;
        if view != ViewMode::Reading {
            self.editing_view = view;
        }
        self.reading.update(cx, |reading, cx| {
            reading.set_live(view == ViewMode::Live, cx)
        });
        cx.notify();
    }

    pub fn status_cursor(&self, cx: &gpui::App) -> Option<Position> {
        (self.view == ViewMode::Source || self.live.is_some()).then(|| self.cursor(cx))
    }

    pub fn on_live_open(&mut self, open: &LiveOpen, window: &mut Window, cx: &mut Context<Self>) {
        if self.view == ViewMode::Live && !self.parked {
            self.open_block(open.kind, open.lines.clone(), open.line, false, window, cx);
            if let Some(block) = &self.live {
                let row = open.line.saturating_sub(block.lines.start);
                block.input.update(cx, |input, cx| {
                    input.set_cursor_position(
                        Position {
                            line: row,
                            character: open.column,
                        },
                        window,
                        cx,
                    )
                });
            }
        }
    }

    pub fn open_block(
        &mut self,
        kind: LiveKind,
        lines: Range<u32>,
        line: u32,
        reveal: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_block(true, window, cx);
        self.reading
            .update(cx, |reading, cx| reading.remove_caret(cx));
        let mut lines = lines;
        let mut appended = None;
        if kind == LiveKind::End {
            let text = self.input.read(cx).value().to_string();
            let (added, line) = end_paragraph(&text);
            appended = Some(text.len());
            self.input.update(cx, |input, cx| {
                let end = input.text().offset_to_offset_utf16(text.len());
                input.replace_text_in_range(Some(end..end), &added, window, cx);
            });
            lines = line..line + 1;
            let text = self.input.read(cx).value().to_string();
            let meta = self
                .library
                .note(self.id)
                .unwrap_or_else(|| self.meta.clone());
            let vault = self.library.vault().clone();
            self.reading.update(cx, |reading, cx| {
                reading.set_markdown_sync(text, Some(meta), vault.as_ref(), cx)
            });
        }
        let text = self.input.read(cx).value().to_string();
        let lines = if kind == LiveKind::Block {
            without_blank_end(&text, lines)
        } else {
            lines
        };
        let span = line_span(self.input.read(cx).text(), &lines);
        let source = text[span].replace("\r\n", "\n");
        let source_first_line = source.lines().next().unwrap_or_default().to_string();
        let caret = line
            .saturating_sub(lines.start)
            .min(lines.end.saturating_sub(lines.start + 1));
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(1, MAX_ROWS)
                .soft_wrap(true)
                .searchable(true)
                .alignment(gpui_component::input::TextAlignment::ByParagraph);
            state.set_value(source, window, cx);
            state.set_cursor_position(
                Position {
                    line: caret,
                    character: 0,
                },
                window,
                cx,
            );
            state
        });
        let changes = cx.subscribe_in(
            &input,
            window,
            |this: &mut NoteEditor, _, event: &InputEvent, window, cx| {
                if let InputEvent::Change = event {
                    this.block_changed(window, cx);
                    cx.emit(EditorEvent::Edited);
                }
            },
        );
        let font = if source_is_code(kind, &source_first_line) {
            EditorFont::from_choice(1)
        } else {
            self.block_font
        };
        let view = cx.new(|_| BlockEditor {
            input: input.clone(),
            font,
            bounds: self.bounds.clone(),
        });
        self.reading.update(cx, |reading, cx| {
            reading.open_block(lines.clone(), view.into(), cx);
            if reveal {
                reading.reveal_open_block(cx);
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        self.live = Some(LiveBlock {
            input,
            lines,
            appended,
            _changes: changes,
        });
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    fn block_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(block) = &self.live else { return };
        let lines = block.lines.clone();
        let crlf = has_crlf(self.input.read(cx).text());
        let value = with_endings(&block.input.read(cx).value(), crlf);
        self.input.update(cx, |input, cx| {
            let span = line_span(input.text(), &lines);
            if input.text().slice(span.clone()) == value.as_str() {
                return;
            }
            let range = input.text().offset_to_offset_utf16(span.start)
                ..input.text().offset_to_offset_utf16(span.end);
            input.replace_text_in_range(Some(range), &value, window, cx);
        });
        if let Some(block) = &mut self.live {
            block.lines = lines.start..lines.start + value.split('\n').count() as u32;
        }
    }

    pub(crate) fn live_reparse(&mut self, cx: &mut Context<Self>) {
        let Some(block) = &self.live else { return };
        let lines = block.lines.clone();
        let text = self.input.read(cx).text().to_string();
        self.parsing = self.input.read(cx).text().clone();
        let meta = self.sheet_meta();
        let resolver = self.resolver();
        self.reading.update(cx, |reading, cx| {
            reading.set_markdown(text, Some(meta), resolver, cx);
            reading.set_open_lines(lines, cx);
        });
    }

    pub fn close_block(&mut self, tidy: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(block) = self.live.take() else {
            return;
        };
        if tidy
            && let Some(length) = block.appended
            && block.input.read(cx).value().is_empty()
        {
            self.input.update(cx, |input, cx| {
                let text = input.value().to_string();
                if text.len() > length && text[length..].chars().all(|c| c == '\n' || c == '\r') {
                    let range = input.text().offset_to_offset_utf16(length)
                        ..input.text().offset_to_offset_utf16(text.len());
                    input.replace_text_in_range(Some(range), "", window, cx);
                }
            });
        }
        self.reading
            .update(cx, |reading, cx| reading.close_block(cx));
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    pub fn move_to_neighbor(
        &mut self,
        up: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(block) = &self.live else {
            return false;
        };
        let caret = block.input.read(cx).cursor_position();
        let last = block.lines.end.saturating_sub(block.lines.start + 1);
        if (up && caret.line != 0) || (!up && caret.line != last) {
            return false;
        }
        let (before, after) = self.reading.read(cx).live_neighbors(&block.lines);
        let Some((kind, lines)) = (if up { before } else { after }) else {
            return false;
        };
        let line = if up {
            lines.end.saturating_sub(1)
        } else {
            lines.start
        };
        self.open_block(kind, lines.clone(), line, true, window, cx);
        if let Some(block) = &self.live {
            let row = line - block.lines.start;
            block.input.update(cx, |input, cx| {
                let length = input
                    .text()
                    .slice_line(row as usize)
                    .to_string()
                    .trim_end_matches(['\n', '\r'])
                    .chars()
                    .count() as u32;
                input.set_cursor_position(
                    Position {
                        line: row,
                        character: caret.character.min(length),
                    },
                    window,
                    cx,
                );
            });
        }
        true
    }

    pub fn live_undo(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        if (self.edit_in_place && self.live.is_none()) || self.view == ViewMode::Reading {
            self.input.update(cx, |input, cx| {
                if redo {
                    input.redo_change(window, cx)
                } else {
                    input.undo_change(window, cx)
                }
            });
            self.reading
                .update(cx, |reading, cx| reading.show_caret(window, cx));
            return;
        }
        self.close_block(false, window, cx);
        self.input.update(cx, |input, cx| {
            if redo {
                input.redo_change(window, cx)
            } else {
                input.undo_change(window, cx)
            }
        });
        let text = self.input.read(cx).value().to_string();
        let meta = self
            .library
            .note(self.id)
            .unwrap_or_else(|| self.meta.clone());
        let vault = self.library.vault().clone();
        let line = self.input.read(cx).cursor_position().line;
        let found = self.reading.update(cx, |reading, cx| {
            reading.set_markdown_sync(text, Some(meta), vault.as_ref(), cx);
            reading.live_block_at(line)
        });
        if let Some((kind, lines)) = found {
            self.open_block(kind, lines, line, true, window, cx);
        }
    }
}

impl Shell {
    fn live_block_focused(&self, window: &Window, cx: &gpui::App) -> Option<Entity<NoteEditor>> {
        let editor = self.active_editor()?.clone();
        let focused = editor.read(cx).live.as_ref().is_some_and(|block| {
            gpui::Focusable::focus_handle(block.input.read(cx), cx).is_focused(window)
        });
        focused.then_some(editor)
    }

    pub fn live_keys(
        &mut self,
        event: &gpui::KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let keystroke = &event.keystroke;
        let m = &keystroke.modifiers;
        let plain = !m.control && !m.alt && !m.platform && !m.shift;
        let in_block = self.live_block_focused(window, cx);
        let undo = m.control
            && !m.alt
            && !m.platform
            && (keystroke.key == "z" || (keystroke.key == "y" && !m.shift));
        if undo
            && let Some(editor) = self.active_editor().cloned()
            && (editor.read(cx).view == ViewMode::Live || editor.read(cx).caret_in_view(cx))
            && matches!(self.overlay, crate::shell::Overlay::None)
            && (in_block.is_some()
                || window.focused(cx).is_none_or(|focused| {
                    focused == self.focus
                        || focused
                            == gpui::Focusable::focus_handle(editor.read(cx).reading.read(cx), cx)
                }))
        {
            let redo = keystroke.key == "y" || m.shift;
            editor.update(cx, |editor, cx| editor.live_undo(redo, window, cx));
            return true;
        }
        let Some(editor) = in_block else { return false };
        match keystroke.key.as_str() {
            "escape" => {
                editor.update(cx, |editor, cx| editor.close_block(true, window, cx));
                self.focus_content(window, cx);
                true
            }
            "up" | "down" if plain => editor.update(cx, |editor, cx| {
                editor.move_to_neighbor(keystroke.key == "up", window, cx)
            }),
            _ => false,
        }
    }

    pub fn close_unfocused_block(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor().cloned() else {
            return;
        };
        self.remove_unfocused_caret(&editor, window, cx);
        let Some(input) = editor
            .read(cx)
            .live
            .as_ref()
            .map(|block| block.input.clone())
        else {
            return;
        };
        let Some(focused) = window.focused(cx) else {
            return;
        };
        let in_find = self.find.as_ref().is_some_and(|bar| {
            gpui::Focusable::focus_handle(bar.query.read(cx), cx) == focused
                || gpui::Focusable::focus_handle(bar.replacement.read(cx), cx) == focused
        });
        if gpui::Focusable::focus_handle(input.read(cx), cx) == focused
            || in_find
            || self.slash.is_some()
            || !matches!(self.overlay, crate::shell::Overlay::None)
        {
            return;
        }
        editor.update(cx, |editor, cx| editor.close_block(true, window, cx));
    }
}

impl Shell {
    fn remove_unfocused_caret(
        &mut self,
        editor: &Entity<NoteEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let reading = editor.read(cx).reading.clone();
        if !reading.read(cx).caret_shown() || !matches!(self.overlay, crate::shell::Overlay::None) {
            return;
        }
        let Some(focused) = window.focused(cx) else {
            return;
        };
        let in_find = self.find.as_ref().is_some_and(|bar| {
            gpui::Focusable::focus_handle(bar.query.read(cx), cx) == focused
                || gpui::Focusable::focus_handle(bar.replacement.read(cx), cx) == focused
        });
        if focused == gpui::Focusable::focus_handle(reading.read(cx), cx)
            || in_find
            || self.slash.is_some()
        {
            return;
        }
        reading.update(cx, |reading, cx| reading.remove_caret(cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_spans_its_lines_without_the_last_line_break() {
        let text = "# Title\n\nFirst line\nsecond line\n\n- item\n";
        let rope = Rope::from(text);
        assert_eq!(&text[line_span(&rope, &(2..4))], "First line\nsecond line");
        assert_eq!(&text[line_span(&rope, &(0..1))], "# Title");
        assert_eq!(&text[line_span(&rope, &(5..6))], "- item");
        assert_eq!(line_span(&rope, &(1..1)), 8..8, "no line: an empty span");
        let last = "a\nb";
        assert_eq!(
            &last[line_span(&Rope::from(last), &(1..2))],
            "b",
            "a last line without a line break"
        );
        assert_eq!(
            line_span(&Rope::from(last), &(2..3)),
            3..3,
            "past the end: an empty span at the end"
        );
    }

    #[test]
    fn code_like_blocks_open_in_the_code_font() {
        for code in [
            "```bash",
            "~~~",
            "```mermaid",
            "$$",
            "| Port | Service |",
            "  ```python",
        ] {
            assert!(source_is_code(LiveKind::Block, code), "{code}");
        }
        assert!(source_is_code(LiveKind::Properties, "---"));
        for prose in [
            "```tabs",
            "Paragraphe",
            "- item",
            "> [!note]",
            "# Title",
            "---",
        ] {
            assert!(!source_is_code(LiveKind::Block, prose), "{prose}");
        }
    }

    #[test]
    fn a_block_leaves_its_trailing_blank_lines_out() {
        let text = "- a\n- b\n\n\nnext\n";
        assert_eq!(without_blank_end(text, 0..4), 0..2);
        assert_eq!(
            without_blank_end("\n", 0..1),
            0..1,
            "an empty paragraph keeps its line"
        );
    }

    #[test]
    fn a_paragraph_under_the_last_block_follows_a_blank_line_and_ends_the_note() {
        let typed = |text: &str| {
            let (added, line) = end_paragraph(text);
            let note = format!("{text}{added}");
            let span = line_span(&Rope::from(note.as_str()), &(line..line + 1));
            format!("{}Fin.{}", &note[..span.start], &note[span.end..])
        };
        assert_eq!(typed(""), "Fin.\n");
        assert_eq!(typed("a"), "a\n\nFin.\n");
        assert_eq!(typed("a\n"), "a\n\nFin.\n");
        assert_eq!(typed("a\n\n"), "a\n\nFin.\n");
        assert_eq!(
            typed("a\r\n"),
            "a\r\n\r\nFin.\r\n",
            "a note in CRLF stays in CRLF"
        );
    }

    #[test]
    fn a_crlf_note_keeps_its_endings() {
        assert_eq!(
            with_endings("un\ndeux\r\ntrois", true),
            "un\r\ndeux\r\ntrois"
        );
        assert_eq!(with_endings("un\ndeux", false), "un\ndeux");
        let note = "# T\r\n\r\nligne un\r\nligne deux\r\n";
        let rope = Rope::from(note);
        assert_eq!(
            &note[line_span(&rope, &(2..4))],
            "ligne un\r\nligne deux",
            "a block leaves out its last CRLF"
        );
        assert!(has_crlf(&rope) && !has_crlf(&Rope::from("a\nb\r")));
        let shown = note[line_span(&rope, &(2..4))].replace("\r\n", "\n");
        let typed = shown.replacen("un", "unZZ", 1);
        assert_eq!(
            with_endings(&typed, true),
            "ligne unZZ\r\nligne deux",
            "the text goes before the CR"
        );
    }
}
