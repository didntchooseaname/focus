use focus_view_reading::blockmove;
use gpui::{Context, EntityInputHandler as _, Focusable as _, Window};
use gpui_component::RopeExt as _;

use crate::editor::{NoteEditor, ViewMode};
use crate::shell::Shell;

impl NoteEditor {
    pub fn move_block(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.caret_in_place(cx) {
            return self
                .reading
                .update(cx, |reading, cx| reading.move_caret_block(down, window, cx));
        }
        if self.view != ViewMode::Source {
            return false;
        }
        let (text, caret) = {
            let input = self.input.read(cx);
            (input.text().to_string(), input.cursor())
        };
        let line = text[..caret].matches('\n').count() as u32;
        let outline = blockmove::outline(&text);
        let Some(moved) = blockmove::step(&text, &outline, line, down) else {
            return false;
        };
        let caret = (moved.start + caret.saturating_sub(moved.was)).min(moved.start + moved.len);
        self.input.update(cx, |input, cx| {
            let rope = input.text();
            let range = rope.offset_to_offset_utf16(moved.range.start)
                ..rope.offset_to_offset_utf16(moved.range.end);
            input.replace_text_in_range(Some(range), &moved.text, window, cx);
            let position = input.text().offset_to_position(caret);
            input.set_cursor_position(position, window, cx);
        });
        true
    }
}

impl Shell {
    pub fn move_block(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor().cloned() else {
            return;
        };
        let (view, input) = {
            let e = editor.read(cx);
            (e.view, e.input.clone())
        };
        if view == ViewMode::Source && !input.read(cx).focus_handle(cx).is_focused(window) {
            return;
        }
        editor.update(cx, |editor, cx| editor.move_block(down, window, cx));
    }
}

#[cfg(test)]
mod tests;
