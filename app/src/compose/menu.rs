use focus_kit::{ActiveTokens, MenuRow, menu, menu_separator};
use gpui::{
    Action, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Pixels, Point, Render, Styled, Window, div,
};

use crate::actions::{Confirm, SelectNext, SelectPrev};

pub enum MenuEvent {
    Close,
    Edit(Box<dyn Action>),
    Extract,
    AskAi,
}

#[derive(Clone, Copy)]
enum Command {
    Cut,
    Copy,
    Paste,
    SelectAll,
    Extract,
    AskAi,
}

impl Command {
    fn row(self) -> (&'static str, &'static str, Option<&'static str>) {
        match self {
            Command::Cut => ("editor-menu-cut", "ui-editor-cut", Some("Ctrl X")),
            Command::Copy => ("editor-menu-copy", "ui-action-copy", Some("Ctrl C")),
            Command::Paste => ("editor-menu-paste", "ui-editor-paste", Some("Ctrl V")),
            Command::SelectAll => (
                "editor-menu-select-all",
                "ui-editor-select-all",
                Some("Ctrl A"),
            ),
            Command::Extract => ("editor-menu-extract", "ui-cmd-extract-selection", None),
            Command::AskAi => ("editor-menu-ask-ai", "ui-cmd-ask-ai", None),
        }
    }

    fn event(self) -> MenuEvent {
        match self {
            Command::Cut => MenuEvent::Edit(Box::new(gpui_component::input::Cut)),
            Command::Copy => MenuEvent::Edit(Box::new(gpui_component::input::Copy)),
            Command::Paste => MenuEvent::Edit(Box::new(gpui_component::input::Paste)),
            Command::SelectAll => MenuEvent::Edit(Box::new(gpui_component::input::SelectAll)),
            Command::Extract => MenuEvent::Extract,
            Command::AskAi => MenuEvent::AskAi,
        }
    }
}

pub struct EditorMenu {
    at: Point<Pixels>,
    groups: Vec<Vec<(Command, bool)>>,
    highlighted: Option<usize>,
    focus: FocusHandle,
}

impl EventEmitter<MenuEvent> for EditorMenu {}

impl Focusable for EditorMenu {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EditorMenu {
    pub fn new(
        at: Point<Pixels>,
        field: bool,
        selected: bool,
        ai: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut groups = Vec::new();
        if field {
            let paste = cx.read_from_clipboard().is_some();
            groups.push(vec![
                (Command::Cut, selected),
                (Command::Copy, selected),
                (Command::Paste, paste),
                (Command::SelectAll, true),
            ]);
        }
        let mut actions = vec![(Command::Extract, selected)];
        if ai {
            actions.push((Command::AskAi, selected));
        }
        groups.push(actions);
        EditorMenu {
            at,
            groups,
            highlighted: None,
            focus: cx.focus_handle(),
        }
    }

    fn rows(&self) -> Vec<(Command, bool)> {
        self.groups.iter().flatten().copied().collect()
    }

    fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        let rows = self.rows();
        let count = rows.len() as isize;
        let mut at = self
            .highlighted
            .map_or(if step > 0 { -1 } else { count }, |h| h as isize);
        for _ in 0..count {
            at = (at + step).rem_euclid(count);
            if rows[at as usize].1 {
                self.highlighted = Some(at as usize);
                cx.notify();
                return;
            }
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        if let Some((command, true)) = self.highlighted.and_then(|h| self.rows().get(h).copied()) {
            cx.emit(command.event());
        }
    }
}

impl Render for EditorMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let mut items = Vec::new();
        let mut rank = 0;
        for (g, group) in self.groups.iter().enumerate() {
            if g > 0 {
                items.push(menu_separator(&t).into_any_element());
            }
            for &(command, enabled) in group {
                let (id, key, keys) = command.row();
                let row = MenuRow::new(id, focus_i18n::t(key))
                    .disabled(!enabled)
                    .highlighted(self.highlighted == Some(rank))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        if enabled {
                            cx.emit(command.event());
                        }
                    }));
                items.push(match keys {
                    Some(keys) => row.shortcut(keys).into_any_element(),
                    None => row.into_any_element(),
                });
                rank += 1;
            }
        }
        div()
            .id("editor-menu-layer")
            .key_context(crate::actions::LIST_CONTEXT)
            .absolute()
            .inset_0()
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.step(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.step(1, cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.run(cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.emit(MenuEvent::Close)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|_, _, _, cx| cx.emit(MenuEvent::Close)),
            )
            .child(gpui::deferred(
                gpui::anchored().position(self.at).snap_to_window().child(
                    menu(&t)
                        .id("editor-menu")
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .children(items),
                ),
            ))
    }
}
