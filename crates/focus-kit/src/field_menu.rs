use std::rc::Rc;

use gpui::{
    Action, AnyView, App, AppContext as _, Context, Entity, FocusHandle, Global, IntoElement,
    ParentElement, Pixels, Point, Render, SharedString, Window, anchored, deferred,
};
use gpui_component::input::{Copy, Cut, InputState, Paste, RightClickMenu, SelectAll};

use crate::menu::{KeyboardMenu, MenuCursor, MenuEntry, MenuItem, keyboard_menu};

#[derive(Clone)]
pub struct FieldMenuText {
    pub cut: (SharedString, SharedString),
    pub copy: (SharedString, SharedString),
    pub paste: (SharedString, SharedString),
    pub select_all: (SharedString, SharedString),
}

impl Global for FieldMenuText {}

struct FieldMenuHost(Option<Entity<FieldMenu>>);

impl Global for FieldMenuHost {}

pub fn set_text(text: FieldMenuText, cx: &mut App) {
    cx.set_global(text);
}

pub fn layer(cx: &App) -> Option<AnyView> {
    cx.try_global::<FieldMenuHost>()?
        .0
        .clone()
        .map(|menu| menu.into())
}

pub fn holds_focus(window: &Window, cx: &App) -> bool {
    cx.try_global::<FieldMenuHost>()
        .and_then(|host| host.0.as_ref())
        .is_some_and(|menu| menu.read(cx).focus.contains_focused(window, cx))
}

pub(crate) fn install(cx: &mut App) {
    cx.set_global(RightClickMenu(Rc::new(open)));
    cx.set_global(FieldMenuHost(None));
}

fn open(
    input: Entity<InputState>,
    at: Point<Pixels>,
    editable: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(text) = cx.try_global::<FieldMenuText>().cloned() else {
        return;
    };
    let selected = !input.read(cx).selection_text().is_empty();
    let paste = cx
        .read_from_clipboard()
        .is_some_and(|item| item.text().is_some_and(|text| !text.is_empty()));
    let menu = cx.new(|cx| FieldMenu {
        input,
        at,
        text,
        cut: editable && selected,
        copy: selected,
        paste: editable && paste,
        cursor: MenuCursor::default(),
        focus: cx.focus_handle(),
    });
    let focus = menu.read(cx).focus.clone();
    input_of(&menu, cx).update(cx, |input, _| input.hold_for_menu(true));
    cx.set_global(FieldMenuHost(Some(menu)));
    window.focus(&focus);
    window.refresh();
}

fn input_of(menu: &Entity<FieldMenu>, cx: &App) -> Entity<InputState> {
    menu.read(cx).input.clone()
}

pub struct FieldMenu {
    input: Entity<InputState>,
    at: Point<Pixels>,
    text: FieldMenuText,
    cut: bool,
    copy: bool,
    paste: bool,
    cursor: MenuCursor,
    focus: FocusHandle,
}

impl FieldMenu {
    fn close(&mut self, action: Option<Box<dyn Action>>, window: &mut Window, cx: &mut App) {
        cx.set_global(FieldMenuHost(None));
        self.input.update(cx, |input, cx| {
            input.hold_for_menu(false);
            input.focus(window, cx)
        });
        if let Some(action) = action {
            window.dispatch_action(action, cx);
        }
        window.refresh();
    }
}

impl KeyboardMenu for FieldMenu {
    fn menu_name(&self) -> &'static str {
        "field-menu"
    }

    fn menu_items(&self, _: &App) -> Vec<MenuItem> {
        let entry = |id: &'static str, (label, keys): &(SharedString, SharedString), on: bool| {
            MenuItem::from(
                MenuEntry::new(id, label.clone())
                    .shortcut(keys.clone())
                    .enabled(on),
            )
        };
        vec![
            entry("field-menu-cut", &self.text.cut, self.cut),
            entry("field-menu-copy", &self.text.copy, self.copy),
            entry("field-menu-paste", &self.text.paste, self.paste),
            MenuItem::Separator,
            entry("field-menu-select-all", &self.text.select_all, true),
        ]
    }

    fn menu_cursor(&self) -> MenuCursor {
        self.cursor
    }

    fn set_menu_cursor(&mut self, cursor: MenuCursor) {
        self.cursor = cursor;
    }

    fn menu_run(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let action: Box<dyn Action> = match row {
            0 => Box::new(Cut),
            1 => Box::new(Copy),
            2 => Box::new(Paste),
            _ => Box::new(SelectAll),
        };
        self.close(Some(action), window, cx);
    }

    fn menu_dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(None, window, cx);
    }
}

impl Render for FieldMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.clone();
        deferred(
            anchored()
                .position(self.at)
                .snap_to_window()
                .child(crate::fade::fade_in(
                    "fade-field-menu",
                    keyboard_menu(self, &focus, cx),
                    cx,
                )),
        )
        .with_priority(2)
    }
}
