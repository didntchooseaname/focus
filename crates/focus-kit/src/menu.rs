use gpui::{
    App, ClickEvent, Context, Div, ElementId, FocusHandle, Global, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, RenderOnce, ScrollHandle, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, actions, div, prelude::FluentBuilder, px,
};

use crate::dir::Flow as _;
use crate::icons::{Icon, icon};
use crate::interaction::Forced;
use crate::tokens::{ActiveTokens, Tokens, size};
use crate::typography::Type;

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

actions!(
    focus_menu,
    [
        MenuPrevious,
        MenuNext,
        MenuFirst,
        MenuLast,
        MenuConfirm,
        MenuDismiss
    ]
);

pub const MENU_CONTEXT: &str = "FocusMenu";

pub(crate) fn bind(cx: &mut App) {
    let context = Some(MENU_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", MenuPrevious, context),
        KeyBinding::new("down", MenuNext, context),
        KeyBinding::new("home", MenuFirst, context),
        KeyBinding::new("end", MenuLast, context),
        KeyBinding::new("pageup", MenuFirst, context),
        KeyBinding::new("pagedown", MenuLast, context),
        KeyBinding::new("enter", MenuConfirm, context),
        KeyBinding::new("space", MenuConfirm, context),
        KeyBinding::new("escape", MenuDismiss, context),
    ]);
}

pub fn floating(t: &Tokens) -> Div {
    div().bg(t.card).border_1().border_color(t.float_border)
}

pub fn menu(t: &Tokens) -> Div {
    floating(t).flex().flex_col().py(size::S1).min_w(px(220.))
}

pub fn menu_separator(t: &Tokens) -> Div {
    div().flex_none().my(size::S1).h(px(1.)).bg(t.line)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuMove {
    Previous,
    Next,
    First,
    Last,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuCursor(Option<usize>);

impl MenuCursor {
    pub fn at(row: usize) -> MenuCursor {
        MenuCursor(Some(row))
    }

    pub fn row(self) -> Option<usize> {
        self.0
    }

    pub fn moved(self, enabled: &[bool], step: MenuMove) -> MenuCursor {
        let count = enabled.len();
        let first = enabled.iter().position(|on| *on);
        let last = enabled.iter().rposition(|on| *on);
        let (Some(first), Some(last)) = (first, last) else {
            return MenuCursor(None);
        };
        let at = self.0.filter(|row| *row < count);
        let row = match (step, at) {
            (MenuMove::First, _) | (MenuMove::Next, None) => first,
            (MenuMove::Last, _) | (MenuMove::Previous, None) => last,
            (MenuMove::Next, Some(at)) => (1..=count)
                .map(|offset| (at + offset) % count)
                .find(|row| enabled[*row])
                .unwrap_or(first),
            (MenuMove::Previous, Some(at)) => (1..=count)
                .map(|offset| (at + count - offset) % count)
                .find(|row| enabled[*row])
                .unwrap_or(last),
        };
        MenuCursor(Some(row))
    }
}

#[derive(Clone)]
pub struct MenuEntry {
    id: SharedString,
    label: SharedString,
    icon: Option<Icon>,
    shortcut: Option<SharedString>,
    enabled: bool,
    danger: bool,
    checked: bool,
}

impl MenuEntry {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        MenuEntry {
            id: id.into(),
            label: label.into(),
            icon: None,
            shortcut: None,
            enabled: true,
            danger: false,
            checked: false,
        }
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
    pub fn id(&self) -> &SharedString {
        &self.id
    }
}

#[derive(Clone)]
pub enum MenuItem {
    Row(MenuEntry),
    Separator,
}

impl From<MenuEntry> for MenuItem {
    fn from(entry: MenuEntry) -> Self {
        MenuItem::Row(entry)
    }
}

pub fn menu_rows(items: &[MenuItem]) -> Vec<&MenuEntry> {
    items
        .iter()
        .filter_map(|item| match item {
            MenuItem::Row(entry) => Some(entry),
            MenuItem::Separator => None,
        })
        .collect()
}

pub trait KeyboardMenu: Sized + 'static {
    fn menu_name(&self) -> &'static str;
    fn menu_items(&self, cx: &App) -> Vec<MenuItem>;
    fn menu_cursor(&self) -> MenuCursor;
    fn set_menu_cursor(&mut self, cursor: MenuCursor);
    fn menu_run(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>);
    fn menu_dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>);
    fn menu_scroll(&self) -> Option<MenuScroll> {
        None
    }
}

#[derive(Clone)]
pub struct MenuScroll {
    pub handle: ScrollHandle,
    pub rows: usize,
    pub pinned: usize,
}

struct DrawnMenu {
    name: &'static str,
    focus: FocusHandle,
    rows: Vec<SharedString>,
    highlighted: Option<usize>,
}

impl Global for DrawnMenu {}

pub fn focus_menu(focus: &FocusHandle, window: &mut Window, cx: &mut App) {
    let focus = focus.clone();
    window.focus(&focus);
    window.defer(cx, move |window, _| window.focus(&focus));
}

pub fn drawn_menu(window: &Window, cx: &App) -> Option<String> {
    let drawn = cx.try_global::<DrawnMenu>()?;
    if !drawn.focus.contains_focused(window, cx) {
        return None;
    }
    let row = drawn
        .highlighted
        .and_then(|row| drawn.rows.get(row))
        .map_or("none".to_string(), |id| id.to_string());
    Some(format!("{} | {row}", drawn.name))
}

fn step<V: KeyboardMenu>(view: &mut V, step: MenuMove, cx: &mut Context<V>) {
    let enabled: Vec<bool> = menu_rows(&view.menu_items(cx))
        .iter()
        .map(|entry| entry.enabled)
        .collect();
    let cursor = view.menu_cursor().moved(&enabled, step);
    view.set_menu_cursor(cursor);
    if let (Some(scroll), Some(row)) = (view.menu_scroll(), cursor.row()) {
        let item = view
            .menu_items(cx)
            .iter()
            .enumerate()
            .filter(|(_, item)| matches!(item, MenuItem::Row(_)))
            .nth(row)
            .map(|(at, _)| at);
        if let Some(item) = item {
            scroll.handle.scroll_to_item(item);
        }
    }
    cx.notify();
}

fn confirm<V: KeyboardMenu>(view: &mut V, window: &mut Window, cx: &mut Context<V>) {
    let Some(row) = view.menu_cursor().row() else {
        return;
    };
    let enabled = menu_rows(&view.menu_items(cx))
        .get(row)
        .is_some_and(|entry| entry.enabled);
    if enabled {
        view.menu_run(row, window, cx);
    }
}

pub fn keyboard_menu<V: KeyboardMenu>(
    view: &V,
    focus: &FocusHandle,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let t = *cx.tokens();
    let name = view.menu_name();
    let items = view.menu_items(cx);
    let highlighted = view.menu_cursor().row();
    let mut rank = 0;
    let mut ids = Vec::new();
    let mut children = Vec::new();
    for item in items {
        match item {
            MenuItem::Separator => children.push(menu_separator(&t).into_any_element()),
            MenuItem::Row(entry) => {
                let row = rank;
                ids.push(entry.id.clone());
                let mut element = MenuRow::new(ElementId::Name(entry.id.clone()), entry.label)
                    .checked(entry.checked)
                    .danger(entry.danger)
                    .disabled(!entry.enabled)
                    .highlighted(highlighted == Some(row))
                    .on_click(cx.listener(move |view: &mut V, _, window, cx| {
                        view.menu_run(row, window, cx)
                    }));
                if let Some(glyph) = entry.icon {
                    element = element.icon(glyph);
                }
                if let Some(keys) = entry.shortcut {
                    element = element.shortcut(keys);
                }
                children.push(element.into_any_element());
                rank += 1;
            }
        }
    }
    cx.set_global(DrawnMenu {
        name,
        focus: focus.clone(),
        rows: ids,
        highlighted,
    });
    if let Some(scroll) = view.menu_scroll() {
        let pinned = children.split_off(children.len().saturating_sub(scroll.pinned));
        let body = div()
            .id("menu-body")
            .flex()
            .flex_col()
            .flex_shrink()
            .max_h(size::ROW * scroll.rows as f32)
            .overflow_y_scroll()
            .track_scroll(&scroll.handle)
            .children(std::mem::take(&mut children));
        children.push(body.into_any_element());
        children.extend(pinned);
    }
    menu(&t)
        .id(name)
        .key_context(MENU_CONTEXT)
        .track_focus(focus)
        .occlude()
        .on_action(
            cx.listener(|view: &mut V, _: &MenuPrevious, _, cx| step(view, MenuMove::Previous, cx)),
        )
        .on_action(cx.listener(|view: &mut V, _: &MenuNext, _, cx| step(view, MenuMove::Next, cx)))
        .on_action(
            cx.listener(|view: &mut V, _: &MenuFirst, _, cx| step(view, MenuMove::First, cx)),
        )
        .on_action(cx.listener(|view: &mut V, _: &MenuLast, _, cx| step(view, MenuMove::Last, cx)))
        .on_action(
            cx.listener(|view: &mut V, _: &MenuConfirm, window, cx| confirm(view, window, cx)),
        )
        .on_action(
            cx.listener(|view: &mut V, _: &MenuDismiss, window, cx| view.menu_dismiss(window, cx)),
        )
        .on_mouse_down_out(cx.listener(|view: &mut V, _, window, cx| view.menu_dismiss(window, cx)))
        .children(children)
}

#[derive(IntoElement)]
pub struct MenuRow {
    id: ElementId,
    label: SharedString,
    icon: Option<Icon>,
    shortcut: Option<SharedString>,
    detail: Option<SharedString>,
    tooltip: Option<SharedString>,
    checked: bool,
    danger: bool,
    disabled: bool,
    highlighted: bool,
    current: bool,
    forced: Forced,
    on_click: Option<Handler>,
}

impl MenuRow {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        MenuRow {
            id: id.into(),
            label: label.into(),
            icon: None,
            shortcut: None,
            detail: None,
            tooltip: None,
            checked: false,
            danger: false,
            disabled: false,
            highlighted: false,
            current: false,
            forced: Forced::Live,
            on_click: None,
        }
    }
    pub fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn highlighted(mut self, highlighted: bool) -> Self {
        self.highlighted = highlighted;
        self
    }
    pub fn forced(mut self, forced: Forced) -> Self {
        self.forced = forced;
        self
    }
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for MenuRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let ink = if self.disabled {
            t.ink_4
        } else if self.danger {
            t.danger
        } else if self.highlighted || self.current || self.forced.hover() {
            t.ink
        } else {
            t.ink_2
        };
        let keyboard = !self.disabled && (self.highlighted || self.forced.focus());
        let bg = if self.disabled {
            None
        } else if keyboard || self.forced.pressed() || self.current {
            Some(t.card_3)
        } else if self.forced.hover() {
            Some(t.card_2)
        } else {
            None
        };
        let live = self.forced == Forced::Live && !self.disabled;
        let danger = self.danger;
        let hover_bg = if keyboard || self.current {
            t.card_3
        } else {
            t.card_2
        };
        div()
            .id(self.id)
            .relative()
            .flow_row()
            .items_center()
            .gap(size::S2)
            .flex_none()
            .h(size::ROW)
            .mx(size::S1)
            .px(size::S2)
            .rounded(size::RADIUS)
            .ui()
            .text_color(ink)
            .when_some(bg, |this, bg| this.bg(bg))
            .when(live, |this| {
                this.cursor_pointer()
                    .hover(move |s| {
                        s.bg(hover_bg)
                            .text_color(if danger { t.danger } else { t.ink })
                    })
                    .active(move |s| s.bg(t.card_3))
            })
            .when(keyboard, |this| {
                this.child(
                    crate::Dir::current()
                        .start(div().absolute(), px(0.))
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .bg(t.ink),
                )
            })
            .child(
                div()
                    .flex_none()
                    .w(size::ICON_DENSE)
                    .text_color(if self.checked { t.ink } else { ink })
                    .when(self.checked, |this| {
                        this.child(icon(Icon::CHECK, size::ICON_DENSE))
                    })
                    .when_some(self.icon.filter(|_| !self.checked), |this, glyph| {
                        this.child(icon(glyph, size::ICON_DENSE))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flow_row()
                    .items_baseline()
                    .gap(size::S2)
                    .child(
                        div()
                            .flex_shrink_0()
                            .max_w_full()
                            .child(crate::ellipsis(self.label)),
                    )
                    .when_some(self.detail, |this, detail| {
                        this.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .mono_sm()
                                .text_color(t.ink_3)
                                .child(crate::ellipsis(detail)),
                        )
                    }),
            )
            .when_some(self.tooltip, |this, tip| {
                this.tooltip(move |_, cx| crate::tooltip::Tooltip::view(tip.clone(), None, cx))
            })
            .when_some(self.shortcut, |this, keys| {
                this.child(div().flex_none().mono_sm().text_color(t.ink_3).child(keys))
            })
            .when_some(self.on_click.filter(|_| !self.disabled), |this, handler| {
                this.on_click(handler)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{MenuCursor, MenuMove};

    #[test]
    fn the_cursor_skips_disabled_rows_and_wraps() {
        let enabled = [true, false, true, true, false];
        let start = MenuCursor::default();
        let first = start.moved(&enabled, MenuMove::Next);
        assert_eq!(first.row(), Some(0));
        let second = first.moved(&enabled, MenuMove::Next);
        assert_eq!(second.row(), Some(2));
        let wrapped = second
            .moved(&enabled, MenuMove::Next)
            .moved(&enabled, MenuMove::Next);
        assert_eq!(wrapped.row(), Some(0));
        assert_eq!(start.moved(&enabled, MenuMove::Previous).row(), Some(3));
        assert_eq!(first.moved(&enabled, MenuMove::Previous).row(), Some(3));
        assert_eq!(second.moved(&enabled, MenuMove::Last).row(), Some(3));
        assert_eq!(wrapped.moved(&enabled, MenuMove::First).row(), Some(0));
    }

    #[test]
    fn a_menu_without_an_enabled_row_has_no_cursor() {
        let cursor = MenuCursor::default().moved(&[false, false], MenuMove::Next);
        assert_eq!(cursor.row(), None);
        assert_eq!(MenuCursor::default().moved(&[], MenuMove::Last).row(), None);
    }
}
