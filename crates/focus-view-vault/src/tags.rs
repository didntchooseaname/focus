use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use focus_core::{NoteId, Vault, VaultEvent};
use focus_kit::{
    ActiveTokens, KeyboardMenu, MenuCursor, MenuEntry, MenuItem, Tokens, TreeRow, Type,
    keyboard_menu, size,
};
use gpui::{
    AnyElement, App, AppContext, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Pixels, Point, Render,
    ScrollStrategy, Styled, Task, UniformListScrollHandle, Window, div, prelude::FluentBuilder, px,
    uniform_list,
};

use crate::VaultViewEvent;

const RENAME_KEY: &str = "f2";

struct TagRow {
    key: String,
    label: String,
    depth: usize,
    count: u32,
    children: bool,
}

#[derive(Default)]
pub struct TagIndex {
    by_note: HashMap<NoteId, Vec<String>>,
    counts: BTreeMap<String, (String, u32)>,
}

impl TagIndex {
    pub fn load(vault: &Vault) -> TagIndex {
        let mut index = TagIndex::default();
        let mut notes = vault.notes();
        notes.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        for note in notes {
            if !note.tags.is_empty() {
                index.count(&note.tags, true);
                index.by_note.insert(note.id, note.tags.clone());
            }
        }
        index
    }

    pub fn len(&self) -> usize {
        self.counts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    pub fn set_note(&mut self, note: NoteId, tags: Option<Vec<String>>) {
        if let Some(old) = self.by_note.remove(&note) {
            self.count(&old, false);
        }
        if let Some(tags) = tags.filter(|t| !t.is_empty()) {
            self.count(&tags, true);
            self.by_note.insert(note, tags);
        }
    }

    fn count(&mut self, tags: &[String], add: bool) {
        let mut seen = HashSet::new();
        for tag in tags {
            let mut end = tag.len();
            loop {
                let part = &tag[..end];
                let key = part.to_lowercase();
                if seen.insert(key.clone()) {
                    if add {
                        self.counts
                            .entry(key)
                            .or_insert_with(|| (part.to_string(), 0))
                            .1 += 1;
                    } else if let Some(entry) = self.counts.get_mut(&key) {
                        entry.1 = entry.1.saturating_sub(1);
                        if entry.1 == 0 {
                            self.counts.remove(&key);
                        }
                    }
                }
                match part.rfind('/') {
                    Some(cut) => end = cut,
                    None => break,
                }
            }
        }
    }
}

pub struct TagTree {
    vault: Arc<Vault>,
    index: TagIndex,
    expanded: HashSet<String>,
    rows: Vec<TagRow>,
    loading: Option<(Task<()>, bool)>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    selected: Option<usize>,
    menu: Option<(String, Point<Pixels>)>,
    menu_cursor: MenuCursor,
    menu_focus: FocusHandle,
    type_ahead: crate::keys::TypeAhead,
}

impl EventEmitter<VaultViewEvent> for TagTree {}

impl Focusable for TagTree {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TagTree {
    pub fn new(vault: Arc<Vault>, cx: &mut Context<Self>) -> Self {
        let mut tree = TagTree {
            vault,
            index: TagIndex::default(),
            expanded: HashSet::new(),
            rows: Vec::new(),
            loading: None,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            selected: None,
            menu: None,
            menu_cursor: MenuCursor::default(),
            menu_focus: cx.focus_handle(),
            type_ahead: Default::default(),
        };
        tree.load(cx);
        tree
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let vault = self.vault.clone();
        let task = cx.background_spawn(async move { TagIndex::load(&vault) });
        let task = cx.spawn(async move |this, cx| {
            let index = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.loading.take().is_some_and(|(_, stale)| stale) {
                    this.load(cx);
                    return;
                }
                this.index = index;
                this.rebuild();
                cx.notify();
            });
        });
        self.loading = Some((task, false));
    }

    pub fn vault_changed(&mut self, event: &VaultEvent, cx: &mut Context<Self>) {
        if let Some((_, stale)) = &mut self.loading {
            *stale = true;
            return;
        }
        match event {
            VaultEvent::NoteAdded(id)
            | VaultEvent::NoteChanged(id)
            | VaultEvent::NoteMoved { id, .. } => {
                let tags = self.vault.note(*id).map(|n| n.tags.clone());
                self.index.set_note(*id, tags);
            }
            VaultEvent::NoteRemoved { id, .. } => self.index.set_note(*id, None),
            VaultEvent::TreeChanged => return,
        }
        self.rebuild();
        cx.notify();
    }

    fn rebuild(&mut self) {
        self.rows = tag_rows(&self.index.counts, &self.expanded);
    }

    pub fn toggle(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(key) {
            self.expanded.insert(key.to_string());
        }
        self.rebuild();
        cx.notify();
    }

    fn full(&self, row: usize) -> Option<String> {
        let key = &self.rows.get(row)?.key;
        self.index.counts.get(key).map(|(label, _)| label.clone())
    }

    fn open_menu(
        &mut self,
        row: usize,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(full) = self.full(row) {
            self.selected = Some(row);
            self.menu = Some((full, at));
            self.menu_cursor = MenuCursor::default();
            focus_kit::focus_menu(&self.menu_focus, window, cx);
            cx.notify();
        }
    }

    fn rename(&mut self, tag: String, cx: &mut Context<Self>) {
        self.menu = None;
        cx.emit(VaultViewEvent::RenameTag(tag));
        cx.notify();
    }

    fn row_point(&self, row: usize) -> Option<Point<Pixels>> {
        let handle = self.scroll.0.borrow().base_handle.clone();
        let list = handle.bounds();
        let y = list.top()
            + size::S1 * 2.
            + size::list_row() * row as f32
            + handle.offset().y
            + size::list_row() / 2.;
        (y > list.top() && y < list.bottom()).then(|| gpui::point(list.left() + px(64.), y))
    }

    fn type_ahead_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_none()
            && let Some(typed) = crate::keys::TypeAhead::typed_char(event)
        {
            let names = self
                .rows
                .iter()
                .enumerate()
                .map(|(row, tag)| (row, tag.label.as_str()));
            let found =
                self.type_ahead
                    .seek(typed, std::time::Instant::now(), names, self.selected);
            if let Some(row) = found {
                self.selected = Some(row);
                self.scroll.scroll_to_item(row, ScrollStrategy::Center);
                cx.notify();
            }
            cx.stop_propagation();
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keys = &event.keystroke;
        let plain = !keys.modifiers.modified();
        match keys.key.as_str() {
            RENAME_KEY if plain => {
                if let Some(tag) = self.selected.and_then(|row| self.full(row)) {
                    self.rename(tag, cx);
                }
            }
            key if (key == "f10" && keys.modifiers.shift) || (key == "menu" && plain) => {
                if let Some(row) = self.selected
                    && let Some(at) = self.row_point(row)
                {
                    self.open_menu(row, at, window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    pub fn script_menu(&mut self, tag: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let key = tag.to_lowercase();
        let Some(row) = self.rows.iter().position(|r| r.key == key) else {
            return false;
        };
        let Some(at) = self.row_point(row) else {
            return false;
        };
        self.open_menu(row, at, window, cx);
        true
    }

    pub fn script_rename(&mut self, cx: &mut Context<Self>) -> bool {
        match self.menu.clone() {
            Some((tag, _)) => {
                self.rename(tag, cx);
                true
            }
            None => false,
        }
    }

    fn render_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_, at) = self.menu.clone()?;
        let focus = self.menu_focus.clone();
        Some(
            gpui::deferred(gpui::anchored().position(at).snap_to_window().child(
                focus_kit::fade_in("fade-tag-menu", keyboard_menu(self, &focus, cx), cx),
            ))
            .with_priority(1)
            .into_any_element(),
        )
    }

    fn render_row(&self, row: usize, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let r = &self.rows[row];
        let full = self
            .index
            .counts
            .get(&r.key)
            .map(|(label, _)| label.clone())
            .unwrap_or_default();
        let key = r.key.clone();
        let open = self.expanded.contains(&r.key);
        div()
            .relative()
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.open_menu(row, event.position, window, cx);
                }),
            )
            .child(
                TreeRow::new(("tag", row), format!("#{}", r.label))
                    .depth(r.depth)
                    .when(r.children, |this| this.folder(open))
                    .count(r.count)
                    .focused(self.selected == Some(row))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = Some(row);
                        cx.emit(VaultViewEvent::SearchTag(full.clone()));
                    })),
            )
            .when(r.children, |this| {
                this.child(
                    focus_kit::Dir::current()
                        .start(div().absolute(), px(8. + 14. * r.depth as f32))
                        .top_0()
                        .w(px(20.))
                        .h(size::list_row())
                        .cursor_pointer()
                        .hover(move |s| s.bg(t.card_2))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle(&key, cx);
                            }),
                        ),
                )
            })
            .into_any_element()
    }
}

fn tag_rows(counts: &BTreeMap<String, (String, u32)>, expanded: &HashSet<String>) -> Vec<TagRow> {
    let keys: Vec<&String> = counts.keys().collect();
    let mut rows = Vec::new();
    for (i, key) in keys.iter().enumerate() {
        let mut shown = true;
        let mut end = key.len();
        while let Some(cut) = key[..end].rfind('/') {
            if !expanded.contains(&key[..cut]) {
                shown = false;
                break;
            }
            end = cut;
        }
        if !shown {
            continue;
        }
        let children = keys
            .get(i + 1)
            .is_some_and(|next| next.starts_with(&format!("{key}/")));
        let (label, count) = &counts[*key];
        let leaf = label.rsplit('/').next().unwrap_or(label).to_string();
        rows.push(TagRow {
            key: (*key).clone(),
            label: leaf,
            depth: key.matches('/').count(),
            count: *count,
            children,
        });
    }
    rows
}

impl Render for TagTree {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        if self.selected.is_none() && !self.rows.is_empty() && self.focus.is_focused(window) {
            self.selected = Some(0);
        }
        crate::keys::keyed(div().id("tag-tree"), cx)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_key_down(cx.listener(Self::type_ahead_key))
            .relative()
            .size_full()
            .bg(t.bg)
            .when(self.rows.is_empty() && self.loading.is_none(), |this| {
                this.child(
                    div()
                        .p(size::S4)
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(focus_i18n::t("vault-tags-empty")),
                )
            })
            .child(
                uniform_list(
                    "tag-rows",
                    self.rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range.map(|row| this.render_row(row, &t, cx)).collect()
                    }),
                )
                .size_full()
                .py(size::S1)
                .track_scroll(self.scroll.clone()),
            )
            .child(focus_kit::scrollbar(
                "tag-tree-scrollbar",
                self.scroll.clone(),
            ))
            .children(self.render_menu(cx))
    }
}

impl crate::keys::Keyed for TagTree {
    fn selection(&mut self) -> (usize, &mut Option<usize>, &UniformListScrollHandle) {
        (self.rows.len(), &mut self.selected, &self.scroll)
    }

    fn activate(&mut self, row: usize, cx: &mut Context<Self>) {
        if let Some(full) = self
            .rows
            .get(row)
            .and_then(|r| self.index.counts.get(&r.key))
            .map(|(label, _)| label.clone())
        {
            cx.emit(VaultViewEvent::SearchTag(full));
        }
    }
}

impl KeyboardMenu for TagTree {
    fn menu_name(&self) -> &'static str {
        "tag-menu"
    }

    fn menu_items(&self, _: &App) -> Vec<MenuItem> {
        vec![MenuItem::from(
            MenuEntry::new("tag-menu-rename", focus_i18n::t("vault-menu-rename"))
                .shortcut(focus_i18n::keys_text(RENAME_KEY)),
        )]
    }

    fn menu_cursor(&self) -> MenuCursor {
        self.menu_cursor
    }

    fn set_menu_cursor(&mut self, cursor: MenuCursor) {
        self.menu_cursor = cursor;
    }

    fn menu_run(&mut self, _: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((tag, _)) = self.menu.clone() {
            window.focus(&self.focus);
            self.rename(tag, cx);
        }
    }

    fn menu_dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        window.focus(&self.focus);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtags_nest_and_a_note_counts_once() {
        let mut index = TagIndex::default();
        index.set_note(1, Some(vec!["red/ad".into(), "red".into()]));
        index.set_note(2, Some(vec!["Red/web".into()]));
        index.set_note(3, Some(vec!["blue".into(), "gone".into()]));
        index.set_note(3, Some(vec!["blue".into()]));
        let counts = &index.counts;
        assert!(!counts.contains_key("gone"));
        let top: Vec<(String, u32, bool)> = tag_rows(counts, &HashSet::new())
            .into_iter()
            .map(|r| (r.label, r.count, r.children))
            .collect();
        assert_eq!(
            top,
            [("blue".to_string(), 1, false), ("red".to_string(), 2, true)]
        );
        let open = HashSet::from(["red".to_string()]);
        let all: Vec<(String, usize)> = tag_rows(counts, &open)
            .into_iter()
            .map(|r| (r.label, r.depth))
            .collect();
        assert_eq!(
            all,
            [
                ("blue".to_string(), 0),
                ("red".to_string(), 0),
                ("ad".to_string(), 1),
                ("web".to_string(), 1)
            ]
        );
    }
}
