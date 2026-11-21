use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Focusable, Pixels, Subscription, Window};
use gpui_component::RopeExt as _;
use gpui_component::input::Position;

use crate::editor::{NoteEditor, ViewMode};

impl NoteEditor {
    pub fn set_edit_in_place(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit_in_place == on {
            return;
        }
        self.close_block(true, window, cx);
        self.edit_in_place = on;
        let field = on.then(|| self.input.clone());
        self.reading
            .update(cx, |reading, cx| reading.set_in_place(field, cx));
        cx.notify();
    }

    pub fn edit_in_place(&self) -> bool {
        self.edit_in_place
    }

    pub fn caret_in_place(&self, cx: &gpui::App) -> bool {
        self.edit_in_place
            && self.view == ViewMode::Live
            && self.live.is_none()
            && self.reading.read(cx).caret_shown()
    }

    pub fn caret_in_view(&self, cx: &gpui::App) -> bool {
        self.caret_in_place(cx)
            || (self.view == ViewMode::Reading && self.reading.read(cx).caret_shown())
    }

    pub fn set_block_handles(&mut self, on: bool, cx: &mut Context<Self>) {
        self.block_handles = on;
        self.reading
            .update(cx, |reading, cx| reading.set_block_handles(on, cx));
    }

    pub(crate) fn in_place_changed(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().to_string();
        self.parsing = self.input.read(cx).text().clone();
        let meta = self.sheet_meta();
        let resolver = self.resolver();
        self.reading.update(cx, |reading, cx| {
            reading.text_changed(cx);
            reading.set_markdown(text, Some(meta), resolver, cx);
        });
    }

    pub(crate) fn keep_field_unfocused(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.on_focus_lost(window, |this: &mut NoteEditor, window, cx| {
            if !this.input.read(cx).focus_handle(cx).is_focused(window) {
                return;
            }
            let read_caret = this.view == ViewMode::Reading && this.reading.read(cx).caret_shown();
            if (this.edit_in_place && this.view == ViewMode::Live && this.live.is_none())
                || read_caret
            {
                window.focus(&this.reading.read(cx).focus_handle(cx));
            }
        })
    }

    pub fn replace_range(
        &mut self,
        range: Range<usize>,
        text: &str,
        caret: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.caret_in_view(cx) {
            let at = range.start + caret;
            self.reading.update(cx, |reading, cx| {
                reading.replace_in_place(range, text, at..at, window, cx);
                reading.show_caret(window, cx);
            });
            return;
        }
        crate::editing::replace(&self.typing_input(), range, text, caret, window, cx);
    }

    pub fn source_if_unseen(&mut self, cx: &mut Context<Self>) {
        self.reading
            .update(cx, |reading, cx| reading.source_if_unseen(cx));
    }

    pub fn open_caret_source(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.view != ViewMode::Live {
            return false;
        }
        let Some((kind, lines)) = self.reading.read(cx).caret_block() else {
            return false;
        };
        let caret = self.input.read(cx).cursor_position();
        self.open_block(kind, lines, caret.line, true, window, cx);
        if let Some(block) = &self.live {
            let row = caret.line.saturating_sub(block.lines.start);
            block.input.update(cx, |input, cx| {
                input.set_cursor_position(
                    Position {
                        line: row,
                        character: caret.character,
                    },
                    window,
                    cx,
                )
            });
        }
        true
    }

    pub fn anchor_bounds(
        &self,
        byte: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.caret_in_view(cx) {
            return self.reading.read(cx).bounds_for_byte(byte);
        }
        let element = self.bounds.0.get();
        self.typing_input().update(cx, |input, cx| {
            let text = input.text();
            let range = text.offset_to_offset_utf16(byte)..text.offset_to_offset_utf16(byte + 1);
            let content = input.bounds_for_range(range, element, window, cx)?;
            Some(Bounds {
                origin: content.origin + input.scroll_offset(),
                size: content.size,
            })
        })
    }
}
