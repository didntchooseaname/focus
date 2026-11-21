use focus_core::NoteId;
use focus_kit::{ActiveTokens, Type, size};
use gpui::{
    AppContext as _, Context, Entity, EntityId, IntoElement, ParentElement, Pixels, Render,
    SharedString, Styled, Window, div, px,
};

use crate::editor::NoteEditor;
use crate::shell::{Overlay, Pane, Shell};

pub const SHARE: f32 = 0.5;
const SHARE_MIN: f32 = 0.2;
const SHARE_MAX: f32 = 0.8;

#[derive(Default)]
pub struct Group {
    pub tabs: Vec<Entity<NoteEditor>>,
    pub active: Option<usize>,
    pub history: Vec<NoteId>,
    pub history_pos: usize,
    pub panes: Vec<Pane>,
    pub active_pane: Option<usize>,
}

impl Group {
    fn is_empty(&self) -> bool {
        self.tabs.is_empty() && self.panes.is_empty()
    }
}

pub struct Split {
    pub other: Group,
    pub focused_second: bool,
    pub share: f32,
}

pub fn clamp_share(share: f32) -> f32 {
    share.clamp(SHARE_MIN, SHARE_MAX)
}

#[derive(Clone)]
pub struct DraggedTab {
    pub second: bool,
    pub item: TabItem,
    pub title: SharedString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabItem {
    Note(EntityId),
    Pane(Pane),
}

pub struct TabGhost(pub SharedString);

impl Render for TabGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        focus_kit::floating(&t)
            .flex()
            .items_center()
            .h(size::CONTROL)
            .px(size::S3)
            .max_w(size::TAB_MAX)
            .ui()
            .text_color(t.ink)
            .child(focus_kit::ellipsis(self.0.clone()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabMenu {
    pub second: bool,
    pub index: usize,
    pub x: Pixels,
    pub y: Pixels,
}

impl Shell {
    pub fn focused_second(&self) -> bool {
        self.split
            .as_ref()
            .is_some_and(|split| split.focused_second)
    }

    fn swap_groups(&mut self) {
        let Some(split) = &mut self.split else { return };
        let other = &mut split.other;
        std::mem::swap(&mut self.tabs, &mut other.tabs);
        std::mem::swap(&mut self.active, &mut other.active);
        std::mem::swap(&mut self.history, &mut other.history);
        std::mem::swap(&mut self.history_pos, &mut other.history_pos);
        std::mem::swap(&mut self.panes, &mut other.panes);
        std::mem::swap(&mut self.active_pane, &mut other.active_pane);
        split.focused_second = !split.focused_second;
    }

    pub fn focus_group(&mut self, second: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.split.is_none() || self.focused_second() == second {
            return;
        }
        if let Some(editor) = self.active_editor().cloned() {
            editor.update(cx, |editor, cx| editor.close_block(true, window, cx));
        }
        self.find = None;
        self.slash = None;
        self.swap_groups();
        match (self.active_pane, self.active) {
            (Some(pane), _) => {
                let pane = self.panes[pane];
                self.show_pane(pane, window, cx);
            }
            (None, Some(active)) => self.activate(active, window, cx),
            (None, None) => window.focus(&self.focus),
        }
        cx.notify();
    }

    pub fn twin_of(
        &self,
        editor: &Entity<NoteEditor>,
        cx: &gpui::App,
    ) -> Option<Entity<NoteEditor>> {
        let id = editor.read(cx).id;
        let split = self.split.as_ref()?;
        self.tabs
            .iter()
            .chain(split.other.tabs.iter())
            .find(|tab| tab.entity_id() != editor.entity_id() && tab.read(cx).id == id)
            .cloned()
    }

    pub fn in_other_group(&self, id: NoteId, cx: &gpui::App) -> Option<Entity<NoteEditor>> {
        self.split
            .as_ref()?
            .other
            .tabs
            .iter()
            .find(|tab| tab.read(cx).id == id)
            .cloned()
    }

    pub fn sync_twin(
        &mut self,
        editor: &Entity<NoteEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(twin) = self.twin_of(editor, cx) else {
            return;
        };
        let Some(shared) = editor.read(cx).shared(cx) else {
            return;
        };
        if twin.read(cx).behind(&shared, cx) {
            twin.update(cx, |twin, cx| twin.follow(&shared, window, cx));
        }
    }

    pub fn hand_lead(&mut self, editor: &Entity<NoteEditor>, cx: &mut Context<Self>) {
        if editor.read(cx).follower {
            return;
        }
        if let Some(twin) = self.twin_of(editor, cx) {
            twin.update(cx, |twin, cx| twin.lead(cx));
        }
    }

    pub fn drop_empty_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(split) = &self.split else { return };
        if self.tabs.is_empty() && self.panes.is_empty() {
            self.swap_groups();
            self.split = None;
            match (self.active_pane, self.active) {
                (Some(pane), _) => {
                    let pane = self.panes[pane];
                    self.show_pane(pane, window, cx);
                }
                (None, Some(active)) => self.activate(active, window, cx),
                (None, None) => window.focus(&self.focus),
            }
        } else if split.other.is_empty() {
            self.split = None;
        }
        cx.notify();
    }

    pub fn split_right(
        &mut self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (item, title) = match (index, self.active_pane) {
            (None, Some(pane)) => {
                let pane = self.panes[pane];
                (TabItem::Pane(pane), SharedString::default())
            }
            (Some(index), _) => match self.tabs.get(index) {
                Some(editor) => (TabItem::Note(editor.entity_id()), SharedString::default()),
                None => return,
            },
            (None, None) => match self.active.and_then(|i| self.tabs.get(i)) {
                Some(editor) => (TabItem::Note(editor.entity_id()), SharedString::default()),
                None => return,
            },
        };
        let alone = self.tabs.len() + self.panes.len() == 1;
        if self.split.is_none() {
            self.split = Some(Split {
                other: Group::default(),
                focused_second: false,
                share: SHARE,
            });
        }
        let target = !self.focused_second();
        let dragged = DraggedTab {
            second: self.focused_second(),
            item,
            title,
        };
        match item {
            TabItem::Note(entity) if alone => {
                let Some(editor) = self.tabs.iter().find(|t| t.entity_id() == entity).cloned()
                else {
                    return;
                };
                let (id, meta, loose, view) = {
                    let editor = editor.read(cx);
                    (
                        editor.id,
                        editor.meta.clone(),
                        editor.loose.clone(),
                        editor.view,
                    )
                };
                self.focus_group(target, window, cx);
                if let Some(index) = self.tabs.iter().position(|tab| tab.read(cx).id == id) {
                    self.activate(index, window, cx);
                } else {
                    self.open_meta(meta, loose, true, window, cx);
                    self.set_view(view, window, cx);
                }
            }
            TabItem::Pane(_) if alone => {
                self.drop_empty_group(window, cx);
            }
            _ => self.move_tab(&dragged, target, window, cx),
        }
    }

    pub fn move_tab(
        &mut self,
        dragged: &DraggedTab,
        to_second: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if dragged.second == to_second || self.split.is_none() {
            return;
        }
        self.focus_group(dragged.second, window, cx);
        let carried = match dragged.item {
            TabItem::Note(entity) => {
                let Some(index) = self.tabs.iter().position(|t| t.entity_id() == entity) else {
                    return;
                };
                let editor = self.tabs.remove(index);
                self.active = match self.active {
                    _ if self.tabs.is_empty() => None,
                    Some(active) if active > index => Some(active - 1),
                    Some(active) => Some(active.min(self.tabs.len() - 1)),
                    None => None,
                };
                if let Some(active) = self.active {
                    self.activate(active, window, cx);
                }
                Some(editor)
            }
            TabItem::Pane(pane) => {
                let Some(index) = self.panes.iter().position(|p| *p == pane) else {
                    return;
                };
                self.panes.remove(index);
                self.active_pane = match self.active_pane {
                    Some(active) if active == index => None,
                    Some(active) if active > index => Some(active - 1),
                    other => other,
                };
                None
            }
        };
        self.focus_group(to_second, window, cx);
        match (dragged.item, carried) {
            (TabItem::Note(_), Some(editor)) => {
                let id = editor.read(cx).id;
                if let Some(index) = self.tabs.iter().position(|tab| tab.read(cx).id == id) {
                    if !editor.read(cx).follower {
                        self.tabs[index].update(cx, |twin, cx| twin.lead(cx));
                    }
                    self.activate(index, window, cx);
                } else {
                    let index = match self.active {
                        Some(active) => active + 1,
                        None => self.tabs.len(),
                    };
                    self.tabs.insert(index, editor);
                    self.activate(index, window, cx);
                }
            }
            (TabItem::Pane(pane), _) => self.show_pane(pane, window, cx),
            _ => {}
        }
        self.drop_empty_group(window, cx);
    }

    pub fn drag_divider(&mut self, x: Pixels, window: &Window) {
        let viewport = window.viewport_size();
        let left = if self.layout.left_open {
            self.layout.left_w
        } else {
            px(0.)
        };
        let right = if self.layout.right_open {
            self.layout.right_w
        } else {
            px(0.)
        };
        let width = viewport.width - left - right;
        let Some(split) = &mut self.split else { return };
        if width <= px(0.) {
            return;
        }
        let start = if focus_kit::Dir::current().rtl {
            viewport.width - x - right
        } else {
            x - left
        };
        split.share = clamp_share(start / width);
    }

    pub fn open_tab_menu(&mut self, menu: TabMenu, cx: &mut Context<Self>) {
        self.overlay = Overlay::TabMenu(menu);
        cx.notify();
    }

    pub fn render_tab_menu(&self, menu: TabMenu, cx: &mut Context<Self>) -> gpui::AnyElement {
        use gpui::{InteractiveElement as _, MouseButton, prelude::FluentBuilder as _};
        let t = *cx.tokens();
        let TabMenu {
            second,
            index,
            x,
            y,
        } = menu;
        let split = focus_kit::MenuRow::new("tab-menu-split", focus_i18n::t("ui-cmd-split-right"))
            .shortcut(crate::actions::shortcut("ui-cmd-split-right"))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.close_overlay(window, cx);
                this.focus_group(second, window, cx);
                this.split_right(Some(index), window, cx);
            }));
        let close = focus_kit::MenuRow::new("tab-menu-close", focus_i18n::t("ui-cmd-close-tab"))
            .shortcut(crate::actions::shortcut("ui-cmd-close-tab"))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.close_overlay(window, cx);
                this.focus_group(second, window, cx);
                this.close_tab(index, window, cx);
            }));
        let tabs = match &self.split {
            Some(split) if self.focused_second() != second => &split.other.tabs,
            _ => &self.tabs,
        };
        let note = tabs.get(index).map(|tab| tab.read(cx).id);
        let in_vault = note
            .and_then(|note| self.library.as_ref()?.note(note))
            .is_some();
        let versions =
            focus_kit::MenuRow::new("tab-menu-versions", focus_i18n::t("ui-menu-versions"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.close_overlay(window, cx);
                    if let Some(note) = note {
                        this.show_versions(note, window, cx);
                    }
                }));
        div()
            .id("tab-menu-veil")
            .absolute()
            .inset_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_overlay(window, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, window, cx| this.close_overlay(window, cx)),
            )
            .child(focus_kit::fade_in(
                "fade-tab-menu",
                focus_kit::menu(&t)
                    .id("tab-menu")
                    .absolute()
                    .top(y)
                    .left(x)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(split)
                    .when(in_vault, |menu| menu.child(versions))
                    .child(close),
                cx,
            ))
            .into_any_element()
    }

    pub fn tab_ghost(title: SharedString, cx: &mut gpui::App) -> Entity<TabGhost> {
        cx.new(|_| TabGhost(title))
    }
}
