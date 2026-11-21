use std::path::{Path, PathBuf};

use focus_core::{NodeKind, NoteId};
use focus_kit::{ActiveTokens, Icon, MenuRow, menu, menu_separator, size};
use gpui::{
    Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
    StatefulInteractiveElement, Styled, div, px,
};

use crate::shell::{LeftTab, Overlay, Shell};

#[derive(Clone, Debug, PartialEq)]
pub struct CrumbMenu {
    pub folder: PathBuf,
    pub x: Pixels,
    pub y: Pixels,
}

struct Entry {
    folder: bool,
    path: PathBuf,
    name: String,
    note: Option<NoteId>,
}

impl Shell {
    pub fn open_crumb_menu(&mut self, folder: PathBuf, x: Pixels, cx: &mut Context<Self>) {
        self.open_crumb_menu_at(folder, x, size::TITLE_BAR, cx);
    }

    pub fn open_crumb_menu_at(
        &mut self,
        folder: PathBuf,
        x: Pixels,
        y: Pixels,
        cx: &mut Context<Self>,
    ) {
        self.overlay = Overlay::Crumbs(CrumbMenu { folder, x, y });
        cx.notify();
    }

    pub fn reveal_note_in_files(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self
            .active_editor()
            .map(|editor| editor.read(cx).meta.path.clone())
        else {
            return;
        };
        self.reveal_path(&path, cx);
        self.reveal_in_tree(Some(path), cx);
    }

    fn reveal_path(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.layout.left_open = true;
        self.layout.left_tab = LeftTab::Files;
        if let Some(views) = &self.vault_views {
            views.tree.update(cx, |tree, cx| tree.select(path, cx));
        }
        cx.notify();
    }

    fn folder_entries(&self, folder: &Path) -> Vec<Entry> {
        let Some(library) = &self.library else {
            return Vec::new();
        };
        let tree = library.vault().tree();
        let parent = if folder.as_os_str().is_empty() {
            None
        } else {
            tree.nodes
                .iter()
                .position(|n| &*n.path == folder)
                .map(|i| i as u32)
        };
        if parent.is_none() && !folder.as_os_str().is_empty() {
            return Vec::new();
        }
        let mut entries: Vec<Entry> = tree
            .nodes
            .iter()
            .filter(|node| {
                node.parent == parent && matches!(node.kind, NodeKind::Folder | NodeKind::Note)
            })
            .map(|node| Entry {
                folder: node.kind == NodeKind::Folder,
                path: node.path.to_path_buf(),
                name: node.name.to_string(),
                note: node.note,
            })
            .collect();
        entries.sort_by_key(|entry| !entry.folder);
        entries
    }

    pub fn render_crumb_menu(
        &self,
        menu_state: &CrumbMenu,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let t = *cx.tokens();
        let folder = menu_state.folder.clone();
        let (x, y) = (menu_state.x, menu_state.y);
        let entries = self.folder_entries(&folder);
        let rows = entries.into_iter().enumerate().map(|(i, entry)| {
            let icon = if entry.folder {
                Icon::FOLDER
            } else {
                Icon::FILE_TEXT
            };
            let row = MenuRow::new(
                ("crumb-entry", i),
                entry.name.trim_end_matches(".md").to_string(),
            )
            .icon(icon);
            if entry.folder {
                let path = entry.path.clone();
                row.on_click(cx.listener(move |this, _, _, cx| {
                    this.open_crumb_menu_at(path.clone(), x, y, cx)
                }))
            } else {
                let note = entry.note;
                row.on_click(cx.listener(move |this, _, window, cx| {
                    this.close_overlay(window, cx);
                    if let Some(note) = note {
                        this.open_note(note, false, window, cx);
                    }
                }))
            }
        });
        let reveal = {
            let folder = folder.clone();
            MenuRow::new("crumb-reveal", focus_i18n::t("ui-crumb-reveal"))
                .icon(Icon::FOLDER_OPEN)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.close_overlay(window, cx);
                    this.reveal_path(&folder, cx);
                }))
        };
        div()
            .id("crumb-veil")
            .absolute()
            .inset_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_overlay(window, cx)),
            )
            .child(focus_kit::fade_in(
                "fade-crumb-menu",
                menu(&t)
                    .id("crumb-menu")
                    .absolute()
                    .top(y)
                    .left(x - px(8.))
                    .max_h(px(420.))
                    .overflow_y_scroll()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(rows)
                    .child(menu_separator(&t))
                    .child(reveal),
                cx,
            ))
            .into_any_element()
    }
}
