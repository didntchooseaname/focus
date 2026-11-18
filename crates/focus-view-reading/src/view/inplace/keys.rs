use gpui::{Context, Div, InteractiveElement, KeyBinding, Stateful, Window, actions};

use super::Key;
use super::caret::{Move, Vertical};
use crate::view::{CONTEXT, ReadingView};

actions!(
    focus_reading_in_place,
    [
        CaretLeft,
        CaretRight,
        CaretUp,
        CaretDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        CaretLineStart,
        CaretLineEnd,
        SelectLineStart,
        SelectLineEnd,
        CaretWordLeft,
        CaretWordRight,
        SelectWordLeft,
        SelectWordRight,
        CaretNoteStart,
        CaretNoteEnd,
        SelectNoteStart,
        SelectNoteEnd,
        DeleteBack,
        DeleteForward,
        DeleteWordBack,
        DeleteWordForward,
        Enter,
        ShiftEnter,
        Tab,
        ShiftTab,
        Escape,
        Cut
    ]
);

pub(crate) fn bindings() -> Vec<KeyBinding> {
    let c = Some(CONTEXT);
    vec![
        KeyBinding::new("left", CaretLeft, c),
        KeyBinding::new("right", CaretRight, c),
        KeyBinding::new("up", CaretUp, c),
        KeyBinding::new("down", CaretDown, c),
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new("home", CaretLineStart, c),
        KeyBinding::new("end", CaretLineEnd, c),
        KeyBinding::new("shift-home", SelectLineStart, c),
        KeyBinding::new("shift-end", SelectLineEnd, c),
        KeyBinding::new("ctrl-left", CaretWordLeft, c),
        KeyBinding::new("ctrl-right", CaretWordRight, c),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, c),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, c),
        KeyBinding::new("ctrl-home", CaretNoteStart, c),
        KeyBinding::new("ctrl-end", CaretNoteEnd, c),
        KeyBinding::new("ctrl-shift-home", SelectNoteStart, c),
        KeyBinding::new("ctrl-shift-end", SelectNoteEnd, c),
        KeyBinding::new("backspace", DeleteBack, c),
        KeyBinding::new("delete", DeleteForward, c),
        KeyBinding::new("ctrl-backspace", DeleteWordBack, c),
        KeyBinding::new("ctrl-delete", DeleteWordForward, c),
        KeyBinding::new("enter", Enter, c),
        KeyBinding::new("shift-enter", ShiftEnter, c),
        KeyBinding::new("tab", Tab, c),
        KeyBinding::new("shift-tab", ShiftTab, c),
        KeyBinding::new("escape", Escape, c),
        KeyBinding::new("ctrl-x", Cut, c),
    ]
}

impl ReadingView {
    fn key_move(&mut self, mv: Move, select: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.caret_shown() || self.prop_menu.is_some() {
            return cx.propagate();
        }
        let Some((text, anchor, head)) = self.field_state(window, cx) else {
            return;
        };
        if let Some(target) = self.moved(&text, anchor, head, mv, select) {
            self.select(if select { anchor } else { target }, target, cx);
        }
        if let Some(editing) = &mut self.editing {
            editing.goal_x = None;
        }
    }

    fn key_vertical(
        &mut self,
        down: bool,
        select: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.caret_shown() || self.prop_menu.is_some() {
            return cx.propagate();
        }
        self.vertical(
            Vertical {
                down,
                select,
                tries: 0,
            },
            window,
            cx,
        );
    }

    fn key_press(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) {
        if !self.caret_shown() || self.prop_menu.is_some() {
            return cx.propagate();
        }
        self.press(key, window, cx);
    }

    fn delete_word(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.caret_shown() || self.prop_menu.is_some() {
            return cx.propagate();
        }
        let Some((text, anchor, head)) = self.field_state(window, cx) else {
            return;
        };
        if anchor != head {
            self.press(Key::Backspace, window, cx);
            return;
        }
        let mv = if forward {
            Move::WordRight
        } else {
            Move::WordLeft
        };
        let Some(target) = self.moved(&text, anchor, head, mv, false) else {
            return;
        };
        let same_text = self.stop_at(&text, head).map(|s| (s.key_item, s.ord))
            == self.stop_at(&text, target).map(|s| (s.key_item, s.ord));
        if same_text {
            let range = head.min(target)..head.max(target);
            self.apply(
                super::Edit {
                    caret: range.start,
                    range,
                    text: String::new(),
                },
                window,
                cx,
            );
        } else {
            self.press(
                if forward { Key::Delete } else { Key::Backspace },
                window,
                cx,
            );
        }
    }

    pub(in crate::view) fn copy_source(
        &mut self,
        cut: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((text, anchor, head)) = self.field_state(window, cx) else {
            return;
        };
        let range = anchor.min(head)..anchor.max(head);
        if range.is_empty() {
            return;
        }
        focus_kit::clipboard::copy_text(cx, text.slice(range.clone()).to_string());
        if cut {
            self.apply(
                super::Edit {
                    caret: range.start,
                    range,
                    text: String::new(),
                },
                window,
                cx,
            );
        }
    }

    fn key_cut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.caret_shown() || self.prop_menu.is_some() {
            return cx.propagate();
        }
        self.copy_source(true, window, cx);
    }

    fn key_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cancel_handle(cx) {
            return;
        }
        if self.prop_menu.is_some() {
            return self.close_prop_menu(window, cx);
        }
        if !self.caret_shown() {
            return cx.propagate();
        }
        self.remove_caret(cx);
    }

    pub(in crate::view) fn select_note(&mut self, cx: &mut Context<Self>) {
        let len = self
            .in_place_frame
            .as_ref()
            .map_or(0, |frame| frame.text.len());
        self.select(0, len, cx);
    }
}

pub(in crate::view) fn on_actions(
    root: Stateful<Div>,
    cx: &mut Context<ReadingView>,
) -> Stateful<Div> {
    root.on_action(
        cx.listener(|this, _: &CaretLeft, window, cx| this.key_move(Move::Left, false, window, cx)),
    )
    .on_action(
        cx.listener(|this, _: &CaretRight, window, cx| {
            this.key_move(Move::Right, false, window, cx)
        }),
    )
    .on_action(
        cx.listener(|this, _: &SelectLeft, window, cx| this.key_move(Move::Left, true, window, cx)),
    )
    .on_action(
        cx.listener(|this, _: &SelectRight, window, cx| {
            this.key_move(Move::Right, true, window, cx)
        }),
    )
    .on_action(
        cx.listener(|this, _: &CaretUp, window, cx| this.key_vertical(false, false, window, cx)),
    )
    .on_action(
        cx.listener(|this, _: &CaretDown, window, cx| this.key_vertical(true, false, window, cx)),
    )
    .on_action(
        cx.listener(|this, _: &SelectUp, window, cx| this.key_vertical(false, true, window, cx)),
    )
    .on_action(
        cx.listener(|this, _: &SelectDown, window, cx| this.key_vertical(true, true, window, cx)),
    )
    .on_action(cx.listener(|this, _: &CaretLineStart, window, cx| {
        this.key_move(Move::LineStart, false, window, cx)
    }))
    .on_action(cx.listener(|this, _: &CaretLineEnd, window, cx| {
        this.key_move(Move::LineEnd, false, window, cx)
    }))
    .on_action(cx.listener(|this, _: &SelectLineStart, window, cx| {
        this.key_move(Move::LineStart, true, window, cx)
    }))
    .on_action(cx.listener(|this, _: &SelectLineEnd, window, cx| {
        this.key_move(Move::LineEnd, true, window, cx)
    }))
    .on_action(cx.listener(|this, _: &CaretWordLeft, window, cx| {
        this.key_move(Move::WordLeft, false, window, cx)
    }))
    .on_action(cx.listener(|this, _: &CaretWordRight, window, cx| {
        this.key_move(Move::WordRight, false, window, cx)
    }))
    .on_action(cx.listener(|this, _: &SelectWordLeft, window, cx| {
        this.key_move(Move::WordLeft, true, window, cx)
    }))
    .on_action(cx.listener(|this, _: &SelectWordRight, window, cx| {
        this.key_move(Move::WordRight, true, window, cx)
    }))
    .on_action(cx.listener(|this, _: &CaretNoteStart, window, cx| {
        this.key_move(Move::NoteStart, false, window, cx)
    }))
    .on_action(cx.listener(|this, _: &CaretNoteEnd, window, cx| {
        this.key_move(Move::NoteEnd, false, window, cx)
    }))
    .on_action(cx.listener(|this, _: &SelectNoteStart, window, cx| {
        this.key_move(Move::NoteStart, true, window, cx)
    }))
    .on_action(cx.listener(|this, _: &SelectNoteEnd, window, cx| {
        this.key_move(Move::NoteEnd, true, window, cx)
    }))
    .on_action(
        cx.listener(|this, _: &DeleteBack, window, cx| this.key_press(Key::Backspace, window, cx)),
    )
    .on_action(
        cx.listener(|this, _: &DeleteForward, window, cx| this.key_press(Key::Delete, window, cx)),
    )
    .on_action(
        cx.listener(|this, _: &DeleteWordBack, window, cx| this.delete_word(false, window, cx)),
    )
    .on_action(
        cx.listener(|this, _: &DeleteWordForward, window, cx| this.delete_word(true, window, cx)),
    )
    .on_action(cx.listener(|this, _: &Enter, window, cx| this.key_press(Key::Enter, window, cx)))
    .on_action(
        cx.listener(|this, _: &ShiftEnter, window, cx| this.key_press(Key::ShiftEnter, window, cx)),
    )
    .on_action(cx.listener(|this, _: &Tab, window, cx| this.key_press(Key::Tab, window, cx)))
    .on_action(
        cx.listener(|this, _: &ShiftTab, window, cx| this.key_press(Key::ShiftTab, window, cx)),
    )
    .on_action(cx.listener(|this, _: &Cut, window, cx| this.key_cut(window, cx)))
    .on_action(cx.listener(|this, _: &Escape, window, cx| this.key_escape(window, cx)))
}
