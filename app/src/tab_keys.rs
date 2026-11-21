use std::path::PathBuf;

use gpui::{Context, Window};

use crate::groups::TabItem;
use crate::shell::{Pane, Shell};
use crate::tab_order::TabOrder;

const REOPEN_DEPTH: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Closed {
    Note(PathBuf),
    Loose(PathBuf),
    File(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabRef {
    Note(usize),
    Pane(usize),
}

pub fn remember(stack: &mut Vec<Closed>, closed: Closed) {
    stack.retain(|c| *c != closed);
    stack.push(closed);
    let excess = stack.len().saturating_sub(REOPEN_DEPTH);
    stack.drain(..excess);
}

fn rank_of(count: usize, front: Option<usize>, delta: isize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let from = front.unwrap_or(0) as isize;
    Some((from + delta).rem_euclid(count as isize) as usize)
}

pub fn present(tabs: &[gpui::Entity<crate::editor::NoteEditor>], panes: &[Pane]) -> Vec<TabItem> {
    tabs.iter()
        .map(|tab| TabItem::Note(tab.entity_id()))
        .chain(panes.iter().map(|pane| TabItem::Pane(*pane)))
        .collect()
}

impl Shell {
    pub fn sync_order(&mut self) {
        let present = present(&self.tabs, &self.panes);
        self.tab_order.reconcile(&present);
    }

    pub fn drawn_order(&self) -> TabOrder {
        self.tab_order.reconciled(&present(&self.tabs, &self.panes))
    }

    fn item_ref(&self, item: TabItem) -> Option<TabRef> {
        match item {
            TabItem::Note(entity) => self
                .tabs
                .iter()
                .position(|tab| tab.entity_id() == entity)
                .map(TabRef::Note),
            TabItem::Pane(pane) => self.panes.iter().position(|p| *p == pane).map(TabRef::Pane),
        }
    }

    fn tab_order(&self) -> Vec<TabRef> {
        self.drawn_order()
            .items()
            .filter_map(|item| self.item_ref(item))
            .collect()
    }

    pub fn drawn_item(&self, rank: usize) -> Option<TabItem> {
        self.drawn_order().items().nth(rank)
    }

    pub fn pane_label(&self, pane: Pane) -> String {
        match pane {
            Pane::Graph => focus_i18n::t("graph-title"),
            Pane::Base(id) => self
                .base_page(id)
                .and_then(|page| page.path.file_stem())
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default(),
            Pane::Ops(screen) => screen.label(),
            Pane::File(id) => self.file_tab_label(id),
        }
    }

    pub fn front_item(&self) -> Option<TabItem> {
        match (self.active_pane, self.active) {
            (Some(pane), _) => self.panes.get(pane).map(|pane| TabItem::Pane(*pane)),
            (None, Some(note)) => self
                .tabs
                .get(note)
                .map(|tab| TabItem::Note(tab.entity_id())),
            (None, None) => None,
        }
    }

    pub fn active_note_pinned(&self) -> bool {
        self.active
            .and_then(|note| self.tabs.get(note))
            .is_some_and(|tab| self.tab_order.is_pinned(TabItem::Note(tab.entity_id())))
    }

    pub fn place_tab(&mut self, item: TabItem, anchor: Option<TabItem>, pinned: bool) {
        self.sync_order();
        if pinned {
            self.tab_order.insert_pinned(item);
        } else {
            self.tab_order.insert_after(anchor, item);
        }
    }

    pub fn toggle_pin(&mut self, item: TabItem, cx: &mut Context<Self>) {
        self.sync_order();
        if self.tab_order.is_pinned(item) {
            self.tab_order.unpin(item);
        } else if self.tab_order.contains(item) {
            self.tab_order.pin(item);
        }
        self.schedule_save(cx);
        cx.notify();
    }

    pub fn toggle_front_pin(&mut self, cx: &mut Context<Self>) {
        if let Some(item) = self.front_item() {
            self.toggle_pin(item, cx);
        }
    }

    pub fn close_item(&mut self, item: TabItem, window: &mut Window, cx: &mut Context<Self>) {
        match self.item_ref(item) {
            Some(TabRef::Note(index)) => self.close_tab(index, window, cx),
            Some(TabRef::Pane(index)) => self.close_pane(index, window, cx),
            None => {}
        }
    }

    pub fn close_unpinned(&mut self, item: TabItem, window: &mut Window, cx: &mut Context<Self>) {
        if self.tab_order.is_pinned(item) {
            let keys = crate::actions::shortcut("ui-cmd-close-tab");
            self.notify_user(
                focus_kit::Tone::Ok,
                focus_i18n::t_args("ui-tab-pinned-stays", &[("keys", keys.as_str().into())]),
                cx,
            );
            return;
        }
        self.close_item(item, window, cx);
    }

    pub fn drop_on_tab(
        &mut self,
        dragged: &crate::groups::DraggedTab,
        second: bool,
        target: TabItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if dragged.second != second {
            self.move_tab(dragged, second, window, cx);
            return;
        }
        self.focus_group(second, window, cx);
        self.sync_order();
        if self.tab_order.reorder(dragged.item, target) {
            self.schedule_save(cx);
            cx.notify();
        }
    }

    fn front_tab(&self) -> Option<TabRef> {
        self.active_pane
            .map(TabRef::Pane)
            .or(self.active.map(TabRef::Note))
    }

    fn show_tab(&mut self, tab: TabRef, window: &mut Window, cx: &mut Context<Self>) {
        match tab {
            TabRef::Note(index) => self.activate(index, window, cx),
            TabRef::Pane(index) => {
                if let Some(pane) = self.panes.get(index).copied() {
                    self.show_pane(pane, window, cx);
                }
            }
        }
    }

    pub fn step_tab(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let order = self.tab_order();
        let front = self
            .front_tab()
            .and_then(|tab| order.iter().position(|t| *t == tab));
        if let Some(rank) = rank_of(order.len(), front, delta) {
            self.show_tab(order[rank], window, cx);
        }
    }

    pub fn go_to_tab(&mut self, rank: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let order = self.tab_order();
        let tab = match rank {
            Some(rank) => order.get(rank),
            None => order.last(),
        };
        if let Some(tab) = tab.copied() {
            self.show_tab(tab, window, cx);
        }
    }

    pub fn close_front_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(item) = self.front_item() {
            self.close_unpinned(item, window, cx);
        }
    }

    pub fn reopen_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Some(closed) = self.closed.pop() {
            match closed {
                Closed::Note(relative) => {
                    let Some(library) = self.library.clone() else {
                        continue;
                    };
                    if !library.root().join(&relative).is_file() {
                        continue;
                    }
                    if let Some(id) = library.note_by_path(&relative) {
                        self.open_note(id, true, window, cx);
                        return;
                    }
                }
                Closed::Loose(path) => {
                    if path.is_file() {
                        self.open_loose(path, window, cx);
                        return;
                    }
                }
                Closed::File(path) => {
                    if path.is_file() {
                        self.open_media(&path, None, window, cx);
                        return;
                    }
                }
            }
        }
    }

    pub fn closed_tab(&self, index: usize, cx: &gpui::App) -> Option<Closed> {
        let editor = self.tabs.get(index)?.read(cx);
        Some(match &editor.loose {
            Some(path) => Closed::Loose(path.clone()),
            None => Closed::Note(editor.meta.path.to_path_buf()),
        })
    }

    pub fn closed_pane(&self, index: usize) -> Option<Closed> {
        match self.panes.get(index)? {
            Pane::File(id) => Some(Closed::File(self.files.get(*id)?.path.clone())),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_wrap_around_in_both_directions() {
        assert_eq!(rank_of(3, Some(2), 1), Some(0));
        assert_eq!(rank_of(3, Some(0), -1), Some(2));
        assert_eq!(rank_of(3, None, 1), Some(1));
        assert_eq!(rank_of(0, None, 1), None);
    }

    #[test]
    fn the_reopen_stack_is_bounded_and_keeps_one_entry_per_tab() {
        let mut stack = Vec::new();
        for n in 0..30 {
            remember(&mut stack, Closed::Note(PathBuf::from(format!("{n}.md"))));
        }
        assert_eq!(stack.len(), REOPEN_DEPTH);
        assert_eq!(stack.last(), Some(&Closed::Note(PathBuf::from("29.md"))));
        remember(&mut stack, Closed::Note(PathBuf::from("15.md")));
        assert_eq!(stack.len(), REOPEN_DEPTH);
        assert_eq!(stack.last(), Some(&Closed::Note(PathBuf::from("15.md"))));
        assert_eq!(
            stack
                .iter()
                .filter(|c| **c == Closed::Note(PathBuf::from("15.md")))
                .count(),
            1
        );
    }
}
