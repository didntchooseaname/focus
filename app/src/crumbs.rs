use std::path::{Path, PathBuf};

use focus_core::{NodeKind, NoteId};
use focus_kit::{Icon, MenuCursor, MenuEntry, MenuItem, size};
use gpui::{
    Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Styled, Window,
    actions, div, px,
};

use crate::shell::{LeftTab, Overlay, Shell};

actions!(focus_crumbs, [OpenCrumbMenu]);

#[derive(Clone, Debug, PartialEq)]
pub struct CrumbMenu {
    pub folder: PathBuf,
    pub x: Pixels,
    pub y: Pixels,
}

pub(crate) struct Entry {
    folder: bool,
    path: PathBuf,
    name: String,
    note: Option<NoteId>,
}

pub(crate) enum CrumbCommand {
    Folder(PathBuf),
    Note(Option<NoteId>),
    Reveal(PathBuf),
}

pub(crate) const CRUMB_ROWS: usize = 15;

impl Shell {
    pub fn open_crumb_menu(
        &mut self,
        folder: PathBuf,
        x: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_crumb_menu_at(folder, x, size::TITLE_BAR, window, cx);
    }

    pub fn open_crumb_menu_at(
        &mut self,
        folder: PathBuf,
        x: Pixels,
        y: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.overlay = Overlay::Crumbs(CrumbMenu { folder, x, y });
        self.menu_cursor = MenuCursor::default();
        self.crumb_scroll = gpui::ScrollHandle::new();
        focus_kit::focus_menu(&self.menu_focus, window, cx);
        cx.notify();
    }

    pub fn open_last_crumb_menu(
        &mut self,
        _: &OpenCrumbMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        let editor = editor.read(cx);
        if editor.loose.is_some() {
            return;
        }
        let folder = editor
            .meta
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let rank = folder.iter().count();
        let x = self
            .crumb_starts
            .borrow()
            .get(rank)
            .copied()
            .unwrap_or(size::S12);
        self.open_crumb_menu(folder, x, window, cx);
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

    pub(crate) fn folder_entries(&self, folder: &Path) -> Vec<Entry> {
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

    pub(crate) fn crumb_entries(&self, folder: &Path) -> Vec<(Option<CrumbCommand>, MenuItem)> {
        let mut items: Vec<(Option<CrumbCommand>, MenuItem)> = self
            .folder_entries(folder)
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                let row = MenuEntry::new(
                    format!("crumb-entry-{i}"),
                    entry.name.trim_end_matches(".md").to_string(),
                )
                .icon(if entry.folder {
                    Icon::FOLDER
                } else {
                    Icon::FILE_TEXT
                });
                let command = if entry.folder {
                    CrumbCommand::Folder(entry.path)
                } else {
                    CrumbCommand::Note(entry.note)
                };
                (Some(command), MenuItem::Row(row))
            })
            .collect();
        items.push((None, MenuItem::Separator));
        items.push((
            Some(CrumbCommand::Reveal(folder.to_path_buf())),
            MenuItem::Row(
                MenuEntry::new("crumb-reveal", focus_i18n::t("ui-crumb-reveal"))
                    .icon(Icon::FOLDER_OPEN),
            ),
        ));
        items
    }

    pub(crate) fn run_crumb(
        &mut self,
        command: CrumbCommand,
        menu: CrumbMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            CrumbCommand::Folder(path) => self.open_crumb_menu_at(path, menu.x, menu.y, window, cx),
            CrumbCommand::Note(note) => {
                self.close_overlay(window, cx);
                if let Some(note) = note {
                    self.open_note(note, false, window, cx);
                }
            }
            CrumbCommand::Reveal(folder) => {
                self.close_overlay(window, cx);
                self.reveal_path(&folder, cx);
            }
        }
    }

    pub fn render_crumb_menu(
        &self,
        menu_state: &CrumbMenu,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (x, y) = (menu_state.x, menu_state.y);
        let focus = self.menu_focus.clone();
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
                focus_kit::keyboard_menu(self, &focus, cx)
                    .absolute()
                    .top(y)
                    .left(x - px(8.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                cx,
            ))
            .into_any_element()
    }
}

impl focus_kit::KeyboardMenu for Shell {
    fn menu_name(&self) -> &'static str {
        match self.overlay {
            Overlay::TabMenu(_) => "tab-menu",
            _ => "crumb-menu",
        }
    }

    fn menu_items(&self, cx: &gpui::App) -> Vec<MenuItem> {
        match &self.overlay {
            Overlay::Crumbs(menu) => self
                .crumb_entries(&menu.folder)
                .into_iter()
                .map(|(_, item)| item)
                .collect(),
            Overlay::TabMenu(menu) => self
                .tab_menu_entries(*menu, cx)
                .into_iter()
                .map(|(_, entry)| MenuItem::Row(entry))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn menu_cursor(&self) -> MenuCursor {
        self.menu_cursor
    }

    fn set_menu_cursor(&mut self, cursor: MenuCursor) {
        self.menu_cursor = cursor;
    }

    fn menu_run(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let open = match &self.overlay {
            Overlay::Crumbs(menu) => Some(Ok(menu.clone())),
            Overlay::TabMenu(menu) => Some(Err(*menu)),
            _ => None,
        };
        match open {
            Some(Ok(menu)) => {
                let command = self
                    .crumb_entries(&menu.folder)
                    .into_iter()
                    .filter_map(|(command, _)| command)
                    .nth(row);
                if let Some(command) = command {
                    self.run_crumb(command, menu, window, cx);
                }
            }
            Some(Err(menu)) => {
                let command = self
                    .tab_menu_entries(menu, cx)
                    .into_iter()
                    .nth(row)
                    .map(|(command, _)| command);
                if let Some(command) = command {
                    self.run_tab_command(command, menu, window, cx);
                }
            }
            None => {}
        }
    }

    fn menu_dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_overlay(window, cx);
    }

    fn menu_scroll(&self) -> Option<focus_kit::MenuScroll> {
        matches!(self.overlay, Overlay::Crumbs(_)).then(|| focus_kit::MenuScroll {
            handle: self.crumb_scroll.clone(),
            rows: CRUMB_ROWS,
            pinned: 2,
        })
    }
}
