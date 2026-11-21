use std::path::{Path, PathBuf};
use std::sync::Arc;

use focus_core::{Vault, VaultEvent};
use focus_kit::{
    ActiveTokens, Disclosure, Flow as _, Icon, IconButton, Tokens, TreeRow, Type, size,
};
use gpui::{
    AnyElement, App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Window, div,
};

use crate::VaultViewEvent;
use crate::keys::{PageNext, PagePrevious, SelectFirst, SelectLast, Step};
use crate::tree::{OpenSelected, SelectNext, SelectPrevious, Trash};

pub(crate) const CONTEXT: &str = "FocusBookmarks";

pub const RECENT_LIMIT: usize = 20;

pub(crate) fn bind(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", SelectPrevious, c),
        KeyBinding::new("down", SelectNext, c),
        KeyBinding::new("enter", OpenSelected, c),
        KeyBinding::new("delete", Trash, c),
    ]);
}

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Header(bool),
    Bookmark(usize),
    Recent(usize),
    Empty(bool),
}

pub struct Bookmarks {
    vault: Arc<Vault>,
    bookmarks: Vec<PathBuf>,
    recent: Vec<PathBuf>,
    folded: [bool; 2],
    rows: Vec<Row>,
    selected: Option<usize>,
    scroll: ScrollHandle,
    focus: FocusHandle,
}

impl EventEmitter<VaultViewEvent> for Bookmarks {}

impl Focusable for Bookmarks {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Bookmarks {
    pub fn new(
        vault: Arc<Vault>,
        stored: Option<Vec<PathBuf>>,
        recent: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let bookmarks = stored.unwrap_or_else(|| vault.obsidian().bookmarks);
        let mut list = Bookmarks {
            vault,
            bookmarks,
            recent,
            folded: [false; 2],
            rows: Vec::new(),
            selected: None,
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
        };
        list.recent.truncate(RECENT_LIMIT);
        list.rebuild();
        list
    }

    pub fn bookmarks(&self) -> &[PathBuf] {
        &self.bookmarks
    }

    pub fn recent(&self) -> &[PathBuf] {
        &self.recent
    }

    pub fn is_bookmarked(&self, path: &Path) -> bool {
        self.bookmarks.iter().any(|p| p == path)
    }

    pub fn toggle(&mut self, path: &Path, cx: &mut Context<Self>) {
        match self.bookmarks.iter().position(|p| p == path) {
            Some(at) => {
                self.bookmarks.remove(at);
            }
            None => self.bookmarks.push(path.to_path_buf()),
        }
        self.bookmarks_changed(cx);
    }

    pub fn opened(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.recent.first().is_some_and(|p| p == path) {
            return;
        }
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(RECENT_LIMIT);
        self.recent_changed(cx);
    }

    pub fn vault_changed(&mut self, event: &VaultEvent, cx: &mut Context<Self>) {
        match event {
            VaultEvent::NoteMoved { from, to, .. } => {
                if rename(&mut self.bookmarks, from, to) {
                    self.bookmarks_changed(cx);
                }
                if rename(&mut self.recent, from, to) {
                    self.recent_changed(cx);
                }
            }
            VaultEvent::NoteRemoved { path, .. } => {
                if remove(&mut self.bookmarks, path) {
                    self.bookmarks_changed(cx);
                }
                if remove(&mut self.recent, path) {
                    self.recent_changed(cx);
                }
            }
            VaultEvent::NoteAdded(_) | VaultEvent::NoteChanged(_) | VaultEvent::TreeChanged => {}
        }
    }

    fn bookmarks_changed(&mut self, cx: &mut Context<Self>) {
        self.rebuild();
        cx.emit(VaultViewEvent::BookmarksChanged(self.bookmarks.clone()));
        cx.notify();
    }

    fn recent_changed(&mut self, cx: &mut Context<Self>) {
        self.rebuild();
        cx.emit(VaultViewEvent::RecentChanged(self.recent.clone()));
        cx.notify();
    }

    fn rebuild(&mut self) {
        let selected = self.selected.and_then(|r| self.rows.get(r).copied());
        self.rows.clear();
        for (section, len) in [(false, self.bookmarks.len()), (true, self.recent.len())] {
            self.rows.push(Row::Header(section));
            if self.folded[section as usize] {
                continue;
            }
            if len == 0 {
                self.rows.push(Row::Empty(section));
            }
            self.rows.extend((0..len).map(|i| {
                if section {
                    Row::Recent(i)
                } else {
                    Row::Bookmark(i)
                }
            }));
        }
        self.selected = selected.and_then(|row| self.rows.iter().position(|r| *r == row));
    }

    fn path_of(&self, row: Row) -> Option<&PathBuf> {
        match row {
            Row::Bookmark(i) => self.bookmarks.get(i),
            Row::Recent(i) => self.recent.get(i),
            Row::Header(_) | Row::Empty(_) => None,
        }
    }

    fn open(&mut self, row: Row, new_tab: bool, cx: &mut Context<Self>) {
        let Some(path) = self.path_of(row).cloned() else {
            return;
        };
        match self.vault.note_by_path(&path) {
            Some(note) => cx.emit(VaultViewEvent::OpenNote {
                note,
                line: None,
                new_tab,
            }),
            None if self.vault.tree().nodes.iter().any(|n| *n.path == *path) => {
                cx.emit(VaultViewEvent::OpenFile { path, new_tab })
            }
            None => {}
        }
    }

    fn fold(&mut self, section: bool, cx: &mut Context<Self>) {
        self.folded[section as usize] = !self.folded[section as usize];
        self.rebuild();
        cx.notify();
    }

    fn step(&mut self, step: crate::keys::Step, cx: &mut Context<Self>) {
        let items: Vec<usize> = (0..self.rows.len())
            .filter(|r| matches!(self.rows[*r], Row::Bookmark(_) | Row::Recent(_)))
            .collect();
        if let Some(row) =
            crate::keys::step(&items, self.selected, rows_per_page(&self.scroll), step)
        {
            self.selected = Some(row);
            self.scroll.scroll_to_item(row);
            cx.notify();
        }
    }

    fn render_row(&self, row: usize, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let kind = self.rows[row];
        match kind {
            Row::Header(section) => {
                let (key, count) = if section {
                    ("vault-recent", self.recent.len())
                } else {
                    ("vault-bookmarks", self.bookmarks.len())
                };
                div()
                    .h(size::list_row())
                    .px(size::S2)
                    .flow_row()
                    .items_center()
                    .child(
                        Disclosure::new(
                            ("bookmark-section", section as usize),
                            focus_i18n::t(key),
                            !self.folded[section as usize],
                        )
                        .detail(count.to_string())
                        .on_toggle(cx.listener(move |this, _, _, cx| this.fold(section, cx))),
                    )
                    .into_any_element()
            }
            Row::Empty(section) => div()
                .py(size::S2)
                .pad_start(size::S2 + size::INDENT + size::ICON_DENSE + size::S2)
                .pad_end(size::S3)
                .text_start()
                .ui_sm()
                .text_color(t.ink_3)
                .child(focus_i18n::t(if section {
                    "vault-recent-empty"
                } else {
                    "vault-bookmarks-empty"
                }))
                .into_any_element(),
            Row::Bookmark(_) | Row::Recent(_) => {
                let Some(path) = self.path_of(kind) else {
                    return div().into_any_element();
                };
                let meta = self
                    .vault
                    .note_by_path(path)
                    .and_then(|id| self.vault.note(id));
                let name: SharedString = match &meta {
                    Some(meta) => meta.title.to_string().into(),
                    None => path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                        .into(),
                };
                let bookmark = matches!(kind, Row::Bookmark(_));
                let mut item = TreeRow::new(("bookmark-row", row), name)
                    .depth(1)
                    .icon(if bookmark {
                        Icon::BOOKMARK
                    } else {
                        Icon::CLOCK
                    })
                    .muted(meta.is_none())
                    .focused(self.selected == Some(row))
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                        this.selected = Some(row);
                        this.open(kind, event.modifiers().control, cx);
                    }));
                if bookmark {
                    let path = path.clone();
                    item = item.trailing(
                        IconButton::new(
                            ("bookmark-remove", row),
                            Icon::CLOSE,
                            focus_i18n::t("vault-bookmark-remove"),
                        )
                        .dense(true)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle(&path, cx);
                        })),
                    );
                }
                item.into_any_element()
            }
        }
    }
}

fn rename(list: &mut [PathBuf], from: &Path, to: &Path) -> bool {
    let mut changed = false;
    for path in list.iter_mut().filter(|p| p.as_path() == from) {
        *path = to.to_path_buf();
        changed = true;
    }
    changed
}

fn remove(list: &mut Vec<PathBuf>, gone: &Path) -> bool {
    let before = list.len();
    list.retain(|p| p != gone);
    list.len() != before
}

impl Render for Bookmarks {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        div()
            .id("bookmarks")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| this.step(Step::Previous, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.step(Step::Next, cx)))
            .on_action(
                cx.listener(|this, _: &PagePrevious, _, cx| this.step(Step::PagePrevious, cx)),
            )
            .on_action(cx.listener(|this, _: &PageNext, _, cx| this.step(Step::PageNext, cx)))
            .on_action(cx.listener(|this, _: &SelectFirst, _, cx| this.step(Step::First, cx)))
            .on_action(cx.listener(|this, _: &SelectLast, _, cx| this.step(Step::Last, cx)))
            .on_action(cx.listener(|this, _: &OpenSelected, _, cx| {
                if let Some(row) = this.selected.and_then(|r| this.rows.get(r).copied()) {
                    this.open(row, false, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Trash, _, cx| {
                if let Some(Row::Bookmark(i)) =
                    this.selected.and_then(|r| this.rows.get(r).copied())
                    && let Some(path) = this.bookmarks.get(i).cloned()
                {
                    this.toggle(&path, cx);
                }
            }))
            .relative()
            .size_full()
            .bg(t.bg)
            .child(
                div()
                    .id("bookmark-rows")
                    .size_full()
                    .py(size::S1)
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children((0..self.rows.len()).map(|row| self.render_row(row, &t, cx))),
            )
            .child(focus_kit::scrollbar(
                "bookmarks-scrollbar",
                self.scroll.clone(),
            ))
    }
}

fn rows_per_page(scroll: &ScrollHandle) -> usize {
    ((scroll.bounds().size.height / size::list_row()).floor() as usize)
        .saturating_sub(1)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moved_note_keeps_its_place_and_a_removed_one_leaves() {
        let mut list = vec![PathBuf::from("a.md"), PathBuf::from("b.md")];
        assert!(rename(&mut list, Path::new("a.md"), Path::new("x/a.md")));
        assert!(!rename(&mut list, Path::new("c.md"), Path::new("d.md")));
        assert!(remove(&mut list, Path::new("b.md")));
        assert_eq!(list, [PathBuf::from("x/a.md")]);
    }
}
