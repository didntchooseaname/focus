use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use focus_core::{LinkOut, NodeKind, TrashKind, TreeSnapshot, Vault, VaultEvent};
use focus_kit::Flow as _;
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Field, Icon, KeyboardMenu, MENU_CONTEXT, MenuConfirm,
    MenuCursor, MenuDismiss, MenuEntry, MenuItem, MenuNext, MenuPrevious, MenuRow, Tokens, TreeRow,
    Type, keyboard_menu, menu, menu_separator, size,
};
use gpui::{
    AnyElement, App, AppContext as _, Context, DragMoveEvent, Entity, EventEmitter, ExternalPaths,
    FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, KeyDownEvent, MouseButton,
    MouseDownEvent, ParentElement, Pixels, Point, Render, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, UniformListScrollHandle, Window,
    actions, div, prelude::FluentBuilder, px, uniform_list,
};
use gpui_component::input::{Input, InputEvent, InputState};

use crate::{Bookmarks, Message, Tone, VaultViewEvent, check_name, error_message};
use focus_core::{ImportMode, SkipReason};

pub(crate) const CONTEXT: &str = "FocusFileTree";

actions!(
    focus_file_tree,
    [
        SelectPrevious,
        SelectNext,
        Collapse,
        Expand,
        OpenSelected,
        Rename,
        Trash,
        NewNote,
        NewFolder,
        Duplicate,
        MoveTo,
        CopyPath,
        CopyLink,
        FocusFilter,
        ContextMenu,
        Dismiss,
        CopyFiles,
        PasteFiles
    ]
);

const KEYS: [&str; 18] = [
    "up",
    "down",
    "left",
    "right",
    "enter",
    "f2",
    "delete",
    "ctrl-n",
    "ctrl-shift-n",
    "ctrl-d",
    "ctrl-m",
    "ctrl-shift-c",
    "ctrl-alt-c",
    "ctrl-f",
    "shift-f10",
    "escape",
    "ctrl-c",
    "ctrl-v",
];

pub(crate) fn bind(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new(KEYS[0], SelectPrevious, c),
        KeyBinding::new(KEYS[1], SelectNext, c),
        KeyBinding::new(KEYS[2], Collapse, c),
        KeyBinding::new(KEYS[3], Expand, c),
        KeyBinding::new(KEYS[4], OpenSelected, c),
        KeyBinding::new(KEYS[5], Rename, c),
        KeyBinding::new(KEYS[6], Trash, c),
        KeyBinding::new(KEYS[7], NewNote, c),
        KeyBinding::new(KEYS[8], NewFolder, c),
        KeyBinding::new(KEYS[9], Duplicate, c),
        KeyBinding::new(KEYS[10], MoveTo, c),
        KeyBinding::new(KEYS[11], CopyPath, c),
        KeyBinding::new(KEYS[12], CopyLink, c),
        KeyBinding::new(KEYS[13], FocusFilter, c),
        KeyBinding::new(KEYS[14], ContextMenu, c),
        KeyBinding::new(KEYS[15], Dismiss, c),
        KeyBinding::new(KEYS[16], CopyFiles, c),
        KeyBinding::new(KEYS[17], PasteFiles, c),
        KeyBinding::new("menu", ContextMenu, c),
    ]);
}

fn shortcut(action: usize) -> String {
    focus_i18n::keys_text(KEYS[action])
}

#[derive(Clone, Debug, PartialEq)]
pub struct TreeSettings {
    pub new_note_folder: String,
    pub confirm_delete: bool,
    pub update_links: bool,
    pub show_other: bool,
    pub trash: TrashKind,
}

impl Default for TreeSettings {
    fn default() -> Self {
        TreeSettings {
            new_note_folder: String::new(),
            confirm_delete: true,
            update_links: true,
            show_other: false,
            trash: TrashKind::Vault,
        }
    }
}

#[derive(Clone)]
struct DraggedPath(Arc<Path>);

struct DragGhost(SharedString);

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        focus_kit::floating(&t)
            .px(size::S2)
            .py(size::S1)
            .ui_sm()
            .text_color(t.ink)
            .child(self.0.clone())
    }
}

struct Renaming {
    path: Arc<Path>,
    input: Entity<InputState>,
    focused: bool,
    then_write: bool,
    _subscription: Subscription,
}

struct Moving {
    path: Arc<Path>,
    input: Entity<InputState>,
    selected: usize,
    _subscription: Subscription,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportStatus {
    pub files: usize,
    pub bytes: u64,
    pub running: bool,
}

struct Importing {
    cancel: Arc<std::sync::atomic::AtomicBool>,
    files: Arc<std::sync::atomic::AtomicUsize>,
    bytes: Arc<std::sync::atomic::AtomicU64>,
    _task: Task<()>,
    _ticker: Task<()>,
}

impl Importing {
    fn status(&self, running: bool) -> ImportStatus {
        use std::sync::atomic::Ordering;
        ImportStatus {
            files: self.files.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            running,
        }
    }
}

const IMPORT_STATUS_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

pub struct FileTree {
    vault: Arc<Vault>,
    importing: Option<Importing>,
    copied: Option<PathBuf>,
    snapshot: Arc<TreeSnapshot>,
    expanded: HashSet<Arc<Path>>,
    rows: Vec<u32>,
    active: Option<Arc<Path>>,
    selected: Option<Arc<Path>>,
    settings: TreeSettings,
    filter: Entity<InputState>,
    renaming: Option<Renaming>,
    menu: Option<(Option<Arc<Path>>, Point<Pixels>)>,
    menu_cursor: MenuCursor,
    menu_focus: FocusHandle,
    confirm: Option<Arc<Path>>,
    confirm_scope: FocusHandle,
    confirm_primary: FocusHandle,
    moving: Option<Moving>,
    scroll: UniformListScrollHandle,
    selected_bounds: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<Pixels>>>>,
    paste_ready: Option<bool>,
    _paste_check: Option<Task<()>>,
    focus: FocusHandle,
    language: u64,
    bookmarks: Option<Entity<Bookmarks>>,
    filtering: Option<Task<()>>,
    type_ahead: crate::keys::TypeAhead,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<VaultViewEvent> for FileTree {}
impl EventEmitter<ImportStatus> for FileTree {}

impl Focusable for FileTree {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn stem_len(kind: NodeKind, name: &str) -> usize {
    match kind {
        NodeKind::Note | NodeKind::Folder => name.len(),
        _ => name.rfind('.').filter(|dot| *dot > 0).unwrap_or(name.len()),
    }
}

fn display_name(kind: NodeKind, name: &str) -> String {
    if kind == NodeKind::Note {
        name.strip_suffix(".md").unwrap_or(name).to_string()
    } else {
        name.to_string()
    }
}

fn contains_folded(name: &str, filter: &str, buffer: &mut String) -> bool {
    buffer.clear();
    if name.is_ascii() {
        buffer.push_str(name);
        buffer.make_ascii_lowercase();
    } else {
        buffer.extend(name.chars().flat_map(char::to_lowercase));
    }
    buffer.contains(filter)
}

pub fn visible_rows(
    snapshot: &TreeSnapshot,
    expanded: &HashSet<Arc<Path>>,
    filter: &str,
    show_other: bool,
) -> Vec<u32> {
    let nodes = &snapshot.nodes;
    let shown = |node: &focus_core::TreeNode| {
        node.kind != NodeKind::Other || show_other || node.link_out.is_some()
    };
    let mut keep = vec![filter.is_empty(); nodes.len()];
    if !filter.is_empty() {
        let mut buffer = String::new();
        for i in (0..nodes.len()).rev() {
            let node = &nodes[i];
            let name = if node.kind == NodeKind::Note {
                node.name.strip_suffix(".md").unwrap_or(&node.name)
            } else {
                &node.name
            };
            if shown(node) && contains_folded(name, filter, &mut buffer) {
                keep[i] = true;
            }
            if keep[i]
                && let Some(p) = node.parent
            {
                keep[p as usize] = true;
            }
        }
    }
    let mut rows = Vec::new();
    let mut i = 0usize;
    while i < nodes.len() {
        let node = &nodes[i];
        let visible = shown(node) && keep[i];
        if visible {
            rows.push(i as u32);
        }
        if node.kind == NodeKind::Folder
            && !(visible
                && if filter.is_empty() {
                    expanded.contains(&node.path)
                } else {
                    keep[i]
                })
        {
            i = node.subtree_end as usize;
        } else {
            i += 1;
        }
    }
    rows
}

impl FileTree {
    pub fn new(
        vault: Arc<Vault>,
        settings: TreeSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| {
            InputState::new(window, cx).placeholder(focus_i18n::t("vault-filter-placeholder"))
        });
        let subscriptions =
            vec![
                cx.subscribe(&filter, |this: &mut FileTree, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.rebuild(cx);
                        cx.notify();
                    }
                }),
            ];
        let mut tree = FileTree {
            snapshot: vault.tree(),
            vault,
            importing: None,
            copied: None,
            expanded: HashSet::new(),
            rows: Vec::new(),
            active: None,
            selected: None,
            settings,
            filter,
            renaming: None,
            menu: None,
            menu_cursor: MenuCursor::default(),
            menu_focus: cx.focus_handle(),
            confirm: None,
            confirm_scope: cx.focus_handle(),
            confirm_primary: cx.focus_handle(),
            moving: None,
            scroll: UniformListScrollHandle::new(),
            selected_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            paste_ready: None,
            _paste_check: None,
            focus: cx.focus_handle(),
            language: focus_i18n::generation(),
            bookmarks: None,
            filtering: None,
            type_ahead: Default::default(),
            _subscriptions: subscriptions,
        };
        tree.rebuild(cx);
        tree
    }

    pub fn set_settings(&mut self, settings: TreeSettings, cx: &mut Context<Self>) {
        if self.settings != settings {
            self.settings = settings;
            self.rebuild(cx);
            cx.notify();
        }
    }

    pub fn set_bookmarks(&mut self, bookmarks: Option<Entity<Bookmarks>>) {
        self.bookmarks = bookmarks;
    }

    pub fn expanded(&self) -> Vec<PathBuf> {
        self.expanded.iter().map(|p| p.to_path_buf()).collect()
    }

    pub fn set_expanded(
        &mut self,
        folders: impl IntoIterator<Item = PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.expanded = folders
            .into_iter()
            .map(|p| Arc::from(p.as_path()))
            .collect();
        self.rebuild(cx);
        cx.notify();
    }

    pub fn vault_changed(&mut self, event: &VaultEvent, cx: &mut Context<Self>) {
        if let VaultEvent::NoteMoved { from, to, .. } = event {
            let (from, to): (Arc<Path>, Arc<Path>) =
                (Arc::from(from.as_path()), Arc::from(to.as_path()));
            for path in [&mut self.selected, &mut self.active].into_iter().flatten() {
                if *path == from {
                    *path = to.clone();
                }
            }
        }
        self.snapshot = self.vault.tree();
        if self
            .renaming
            .as_ref()
            .is_some_and(|r| !self.snapshot.nodes.iter().any(|n| n.path == r.path))
        {
            self.renaming = None;
        }
        self.rebuild(cx);
        cx.notify();
    }

    pub fn create_note_to_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.create(false, None, window, cx);
        if let Some(renaming) = self.renaming.as_mut() {
            renaming.then_write = true;
        }
    }

    pub fn focus_on_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active.is_some() {
            self.selected = self.active.clone();
        }
        window.focus(&self.focus);
        cx.notify();
    }

    pub fn reveal(&mut self, relative: Option<Arc<Path>>, cx: &mut Context<Self>) {
        self.active = relative.clone();
        if let Some(path) = relative
            && let Some(index) = self.snapshot.nodes.iter().position(|n| n.path == path)
        {
            let mut parent = self.snapshot.nodes[index].parent;
            while let Some(p) = parent {
                let folder = &self.snapshot.nodes[p as usize];
                self.expanded.insert(folder.path.clone());
                parent = folder.parent;
            }
            self.rebuild(cx);
            if let Some(row) = self.rows.iter().position(|r| *r as usize == index) {
                self.scroll.scroll_to_item(row, ScrollStrategy::Center);
            }
        }
        cx.notify();
    }

    pub fn script_press(
        &mut self,
        path: &Path,
        right: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let row = if path.as_os_str().is_empty() {
            None
        } else {
            let Some(row) = self
                .rows
                .iter()
                .position(|&i| &*self.snapshot.nodes[i as usize].path == path)
            else {
                return false;
            };
            Some(row)
        };
        let Some(at) = self.row_point(row.unwrap_or(self.rows.len())) else {
            return false;
        };
        window.focus(&self.focus);
        match (row.map(|r| self.rows[r]), right) {
            (Some(index), false) => self.click_row(index, false, window, cx),
            (Some(index), true) => {
                let path = self.snapshot.nodes[index as usize].path.clone();
                self.row_menu(path, at, window, cx);
            }
            (None, true) => self.empty_menu(at, window, cx),
            (None, false) => {}
        }
        true
    }

    fn merge(&mut self, note: focus_core::NoteId, cx: &mut Context<Self>) {
        self.menu = None;
        cx.emit(VaultViewEvent::MergeNote(note));
        cx.notify();
    }

    pub fn script_merge(&mut self, cx: &mut Context<Self>) -> bool {
        let note = self
            .menu
            .as_ref()
            .and_then(|(target, _)| target.as_ref())
            .and_then(|path| self.node(path))
            .filter(|n| n.kind == NodeKind::Note)
            .and_then(|n| n.note);
        match note {
            Some(note) => {
                self.merge(note, cx);
                true
            }
            None => false,
        }
    }

    pub fn script_open_system(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((Some(target), _)) = self.menu.clone() else {
            return false;
        };
        let file = self
            .node(&target)
            .is_some_and(|n| !matches!(n.kind, NodeKind::Note | NodeKind::Folder));
        if !file {
            return false;
        }
        self.menu = None;
        cx.emit(VaultViewEvent::OpenWithSystem(target.to_path_buf()));
        cx.notify();
        true
    }

    pub fn script_paste(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((target, _)) = self.menu.clone() else {
            return false;
        };
        if self.paste_ready != Some(true) {
            return false;
        }
        let folder = self.paste_folder(target.as_deref());
        self.paste_files(folder, cx);
        true
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

    pub fn select(&mut self, relative: &Path, cx: &mut Context<Self>) -> bool {
        let Some(index) = self
            .snapshot
            .nodes
            .iter()
            .position(|n| &*n.path == relative)
        else {
            return false;
        };
        let mut parent = self.snapshot.nodes[index].parent;
        while let Some(p) = parent {
            let folder = &self.snapshot.nodes[p as usize];
            self.expanded.insert(folder.path.clone());
            parent = folder.parent;
        }
        self.rebuild(cx);
        self.selected = Some(self.snapshot.nodes[index].path.clone());
        if let Some(row) = self.selected_row() {
            self.scroll.scroll_to_item(row, ScrollStrategy::Center);
        }
        cx.notify();
        true
    }

    fn filter_text(&self, cx: &App) -> String {
        self.filter.read(cx).value().trim().to_lowercase()
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let filter = self.filter_text(cx);
        if filter.is_empty() {
            self.filtering = None;
            self.rows = visible_rows(&self.snapshot, &self.expanded, "", self.settings.show_other);
            return;
        }
        let (snapshot, show_other) = (self.snapshot.clone(), self.settings.show_other);
        let task = cx.background_spawn(async move {
            visible_rows(&snapshot, &HashSet::new(), &filter, show_other)
        });
        self.filtering = Some(cx.spawn(async move |this, cx| {
            let rows = task.await;
            let _ = this.update(cx, |this, cx| {
                this.filtering = None;
                this.rows = rows;
                cx.notify();
            });
        }));
    }

    fn node(&self, path: &Path) -> Option<&focus_core::TreeNode> {
        self.snapshot.nodes.iter().find(|n| &*n.path == path)
    }

    fn selected_row(&self) -> Option<usize> {
        let path = self.selected.as_ref()?;
        self.rows
            .iter()
            .position(|r| self.snapshot.nodes[*r as usize].path == *path)
    }

    fn step(&mut self, step: crate::keys::Step, cx: &mut Context<Self>) {
        let rows: Vec<usize> = (0..self.rows.len()).collect();
        if let Some(row) = crate::keys::step(
            &rows,
            self.selected_row(),
            crate::keys::page(&self.scroll),
            step,
        ) {
            self.select_row(row, cx);
        }
    }

    fn select_row(&mut self, row: usize, cx: &mut Context<Self>) {
        if let Some(&index) = self.rows.get(row) {
            self.selected = Some(self.snapshot.nodes[index as usize].path.clone());
            self.scroll.scroll_to_item(row, ScrollStrategy::Center);
            cx.notify();
        }
    }

    fn toggle(&mut self, path: Arc<Path>, cx: &mut Context<Self>) {
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
        self.rebuild(cx);
        cx.notify();
    }

    fn notify(&self, message: Message, tone: Tone, cx: &mut Context<Self>) {
        cx.emit(VaultViewEvent::Notify { message, tone });
    }

    fn target_folder(&self, at: Option<&Path>) -> PathBuf {
        match at.and_then(|p| self.node(p)) {
            Some(node) if node.kind == NodeKind::Folder => node.path.to_path_buf(),
            Some(node) => node
                .path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            None => PathBuf::from(self.settings.new_note_folder.trim_matches('/')),
        }
    }

    fn create(
        &mut self,
        folder_kind: bool,
        at: Option<Arc<Path>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folder = self.target_folder(at.as_deref());
        let base = focus_i18n::t(if folder_kind {
            "vault-new-folder-name"
        } else {
            "vault-new-note-name"
        });
        let mut n = 0;
        let created = loop {
            let name = if n == 0 {
                base.clone()
            } else {
                format!("{base} {n}")
            };
            let path = folder.join(if folder_kind {
                name
            } else {
                format!("{name}.md")
            });
            let result = if folder_kind {
                self.vault.create_folder(&path).map(|_| None)
            } else {
                self.vault.create_note(&path, "").map(Some)
            };
            match result {
                Err(focus_core::VaultError::AlreadyExists(_)) => n += 1,
                Ok(note) => break Ok((path, note)),
                Err(error) => break Err(error),
            }
        };
        match created {
            Ok((path, note)) => {
                if !folder.as_os_str().is_empty() {
                    self.expanded.insert(Arc::from(folder.as_path()));
                }
                self.snapshot = self.vault.tree();
                let path: Arc<Path> = Arc::from(path.as_path());
                self.selected = Some(path.clone());
                self.rebuild(cx);
                if let Some(id) = note {
                    cx.emit(VaultViewEvent::OpenNote {
                        note: id,
                        line: None,
                        new_tab: false,
                    });
                }
                self.start_rename(path, window, cx);
            }
            Err(error) => self.notify(error_message(&error), Tone::Danger, cx),
        }
    }

    fn start_rename(&mut self, path: Arc<Path>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(node) = self
            .node(&path)
            .filter(|n| n.link_out != Some(LinkOut::NotFollowed))
        else {
            return;
        };
        let name = display_name(node.kind, &node.name);
        let stem = stem_len(node.kind, &name);
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut FileTree, _, event: &InputEvent, window, cx| match event {
                InputEvent::Focus => {
                    if let Some(renaming) = this.renaming.as_mut() {
                        renaming.focused = true;
                    }
                }
                InputEvent::PressEnter { .. } => this.commit_rename(window, cx),
                InputEvent::Blur if this.renaming.as_ref().is_some_and(|r| r.focused) => {
                    this.commit_rename(window, cx)
                }
                _ => {}
            },
        );
        input.update(cx, |state, cx| {
            state.focus(window, cx);
            state.set_selection(0, stem, cx);
        });
        self.renaming = Some(Renaming {
            path,
            input,
            focused: false,
            then_write: false,
            _subscription: subscription,
        });
        self.menu = None;
        cx.notify();
    }

    fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(renaming) = self.renaming.take() else {
            return;
        };
        let write = renaming.then_write;
        self.rename_to(renaming, window, cx);
        if write {
            cx.emit(VaultViewEvent::WriteNote);
        }
    }

    fn rename_to(&mut self, renaming: Renaming, window: &mut Window, cx: &mut Context<Self>) {
        let name = renaming.input.read(cx).value().trim().to_string();
        window.focus(&self.focus);
        let Some(node) = self.node(&renaming.path) else {
            return;
        };
        if name == display_name(node.kind, &node.name) {
            cx.notify();
            return;
        }
        let file = match check_name(&name, node.kind == NodeKind::Note) {
            Ok(file) => file,
            Err(reason) => {
                self.notify(reason, Tone::Danger, cx);
                cx.notify();
                return;
            }
        };
        let to = renaming
            .path
            .parent()
            .map(|p| p.join(&file))
            .unwrap_or_else(|| PathBuf::from(&file));
        self.relocate(renaming.path, to, "vault-renamed", cx);
    }

    fn relocate(
        &mut self,
        from: Arc<Path>,
        to: PathBuf,
        key: &'static str,
        cx: &mut Context<Self>,
    ) {
        match self.vault.rename(&from, &to, self.settings.update_links) {
            Ok(report) => {
                let to: Arc<Path> = Arc::from(to.as_path());
                if self.selected.as_ref() == Some(&from) {
                    self.selected = Some(to.clone());
                }
                if let Some(parent) = to.parent().filter(|p| !p.as_os_str().is_empty()) {
                    self.expanded.insert(Arc::from(parent));
                }
                self.snapshot = self.vault.tree();
                self.rebuild(cx);
                let name = to
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.notify(
                    Message::new(key)
                        .text_arg("name", name)
                        .count_arg("count", report.updated_notes.len()),
                    Tone::Ok,
                    cx,
                );
            }
            Err(error) => self.notify(error_message(&error), Tone::Danger, cx),
        }
        cx.notify();
    }

    fn request_trash(&mut self, path: Arc<Path>, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        if self.settings.confirm_delete {
            self.selected = Some(path.clone());
            self.confirm = Some(path);
            window.focus(&self.confirm_primary);
            cx.notify();
        } else {
            self.trash(path, cx);
        }
    }

    fn trash(&mut self, path: Arc<Path>, cx: &mut Context<Self>) {
        self.confirm = None;
        match self.vault.trash_with(&path, self.settings.trash) {
            Ok(()) => {
                let row = self.selected_row();
                self.snapshot = self.vault.tree();
                self.rebuild(cx);
                if let Some(row) = row {
                    self.selected = None;
                    self.select_row(row.min(self.rows.len().saturating_sub(1)), cx);
                }
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.notify(
                    Message::new("vault-trashed").text_arg("name", name),
                    Tone::Ok,
                    cx,
                );
            }
            Err(error) => self.notify(error_message(&error), Tone::Danger, cx),
        }
        cx.notify();
    }

    fn duplicate(&mut self, path: Arc<Path>, cx: &mut Context<Self>) {
        self.menu = None;
        let Some(id) = self.node(&path).and_then(|n| n.note) else {
            return;
        };
        match self.vault.duplicate(id) {
            Ok(copy) => {
                self.snapshot = self.vault.tree();
                self.selected = self.vault.note(copy).map(|m| m.path.clone());
                self.rebuild(cx);
                let name = self
                    .selected
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.notify(
                    Message::new("vault-duplicated").text_arg("name", name),
                    Tone::Ok,
                    cx,
                );
            }
            Err(error) => self.notify(error_message(&error), Tone::Danger, cx),
        }
        cx.notify();
    }

    fn copy(&mut self, path: Arc<Path>, link: bool, cx: &mut Context<Self>) {
        self.menu = None;
        let text = if link {
            let config = self.vault.obsidian();
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let slashed = path.to_string_lossy().replace('\\', "/");
            if config.use_markdown_links {
                format!("[{stem}]({})", slashed.replace(' ', "%20"))
            } else {
                format!("[[{}]]", slashed.strip_suffix(".md").unwrap_or(&slashed))
            }
        } else {
            path.to_string_lossy().into_owned()
        };
        focus_kit::clipboard::copy_text(cx, text);
        self.notify(
            Message::new(if link {
                "vault-copied-link"
            } else {
                "vault-copied-path"
            }),
            Tone::Ok,
            cx,
        );
        cx.notify();
    }

    fn folders(&self, query: &str) -> Vec<Arc<Path>> {
        let query = query.trim().to_lowercase();
        std::iter::once(Arc::from(Path::new("")))
            .chain(
                self.snapshot
                    .nodes
                    .iter()
                    .filter(|n| n.kind == NodeKind::Folder)
                    .map(|n| n.path.clone()),
            )
            .filter(|p| query.is_empty() || p.to_string_lossy().to_lowercase().contains(&query))
            .collect()
    }

    fn start_move(&mut self, path: Arc<Path>, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        if self.unread_link(&path) {
            return;
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(focus_i18n::t("vault-move-placeholder"))
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut FileTree, input, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    if let Some(moving) = this.moving.as_mut() {
                        moving.selected = 0;
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    let query = input.read(cx).value().to_string();
                    let chosen = this
                        .moving
                        .as_ref()
                        .and_then(|m| this.folders(&query).get(m.selected).cloned());
                    if let Some(folder) = chosen {
                        this.move_into(folder, cx);
                    }
                }
                _ => {}
            },
        );
        input.update(cx, |state, cx| state.focus(window, cx));
        self.moving = Some(Moving {
            path,
            input,
            selected: 0,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn move_into(&mut self, folder: Arc<Path>, cx: &mut Context<Self>) {
        let Some(path) = self.moving.take().map(|m| m.path) else {
            return;
        };
        self.drop_into(path, folder, cx);
    }

    pub fn import(
        &mut self,
        sources: Vec<PathBuf>,
        folder: PathBuf,
        mode: ImportMode,
        cx: &mut Context<Self>,
    ) {
        use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
        if self.importing.is_some() || sources.is_empty() {
            return;
        }
        let (cancel, files, bytes) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicU64::new(0)),
        );
        let vault = self.vault.clone();
        let (flag, counted, weighed) = (cancel.clone(), files.clone(), bytes.clone());
        let into = folder.clone();
        let work = cx.background_spawn(async move {
            vault.import(&sources, &into, mode, &flag, &|p| {
                counted.store(p.files, Ordering::Relaxed);
                weighed.store(p.bytes, Ordering::Relaxed);
            })
        });
        let task = cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(done) = this.importing.take() {
                    cx.emit(done.status(false));
                }
                match result {
                    Ok(report) => {
                        let place = if folder.as_os_str().is_empty() {
                            "/".to_string()
                        } else {
                            folder.display().to_string()
                        };
                        let key = if report.cancelled {
                            "vault-import-cancelled"
                        } else {
                            "vault-imported"
                        };
                        this.notify(
                            Message::new(key)
                                .count_arg("count", report.files)
                                .text_arg("folder", place),
                            Tone::Ok,
                            cx,
                        );
                        if let Some((path, reason)) = report.skipped.first() {
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            let why = match reason {
                                SkipReason::SymbolicLink => "vault-import-skip-link",
                                SkipReason::Special => "vault-import-skip-special",
                                SkipReason::Failed(_) => "vault-import-skip-failed",
                            };
                            this.notify(
                                Message::new(why)
                                    .count_arg("count", report.skipped.len())
                                    .text_arg("name", name),
                                Tone::Danger,
                                cx,
                            );
                        }
                    }
                    Err(error) => this.notify(error_message(&error), Tone::Danger, cx),
                }
                cx.notify();
            });
        });
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(IMPORT_STATUS_EVERY).await;
                let running = this
                    .update(cx, |this, cx| {
                        this.importing
                            .as_ref()
                            .map(|i| i.status(true))
                            .map(|status| cx.emit(status))
                            .is_some()
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        });
        self.importing = Some(Importing {
            cancel,
            files,
            bytes,
            _task: task,
            _ticker: ticker,
        });
        cx.emit(ImportStatus {
            files: 0,
            bytes: 0,
            running: true,
        });
    }

    fn drag_out(&self, path: &Path, window: &mut Window) {
        let absolute = self.vault.root().join(path);
        if let Some(offer) = focus_kit::file_list::Offer::new(&[absolute], false) {
            window.start_data_drag(
                offer
                    .types
                    .into_iter()
                    .map(|(mime, body)| (mime.to_string(), body))
                    .collect(),
            );
        }
    }

    fn copy_file(&mut self, path: Arc<Path>, cx: &mut Context<Self>) {
        self.menu = None;
        let absolute = self.vault.root().join(&*path);
        focus_kit::clipboard::copy_files(cx, vec![absolute.clone()], false);
        self.copied = Some(absolute);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.notify(
            Message::new("vault-copied-file").text_arg("name", name),
            Tone::Ok,
            cx,
        );
        cx.notify();
    }

    fn paste_folder(&self, at: Option<&Path>) -> PathBuf {
        match at {
            Some(path) => self.target_folder(Some(path)),
            None => PathBuf::new(),
        }
    }

    fn paste_files(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        self.menu = None;
        let copied = self
            .copied
            .clone()
            .filter(|_| !focus_kit::clipboard::Tools::detect().is_available());
        cx.spawn(async move |this, cx| {
            let read = focus_kit::clipboard::read_files(cx).await;
            let _ = this.update(cx, |this, cx| match read {
                Ok(Some(files)) if !files.paths.is_empty() => {
                    let mode = if files.cut {
                        ImportMode::Move
                    } else {
                        ImportMode::Copy
                    };
                    this.import(files.paths, folder, mode, cx);
                }
                Ok(Some(files)) if files.refused > 0 => this.notify(
                    Message::new("vault-paste-remote").count_arg("count", files.refused),
                    Tone::Danger,
                    cx,
                ),
                Ok(_) => match copied {
                    Some(path) => this.import(vec![path], folder, ImportMode::Copy, cx),
                    None => this.notify(Message::new("vault-paste-empty"), Tone::Danger, cx),
                },
                Err(error) => this.notify(
                    Message::new("vault-paste-failed").text_arg("detail", error.to_string()),
                    Tone::Danger,
                    cx,
                ),
            });
        })
        .detach();
    }

    pub fn cancel_import(&mut self) {
        if let Some(importing) = &self.importing {
            importing
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn drop_into(&mut self, path: Arc<Path>, folder: Arc<Path>, cx: &mut Context<Self>) {
        if folder.starts_with(&path) || path.parent() == Some(&folder) {
            return;
        }
        let Some(name) = path.file_name() else { return };
        let to = folder.join(name);
        self.relocate(path, to, "vault-moved", cx);
    }

    fn type_ahead_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.moving.is_none()
            && self.renaming.is_none()
            && self.focus.is_focused(window)
            && let Some(typed) = crate::keys::TypeAhead::typed_char(event)
        {
            let nodes = &self.snapshot.nodes;
            let names = self
                .rows
                .iter()
                .enumerate()
                .map(|(row, index)| (row, &*nodes[*index as usize].name));
            let found =
                self.type_ahead
                    .seek(typed, std::time::Instant::now(), names, self.selected_row());
            if let Some(row) = found {
                self.select_row(row, cx);
            }
            cx.stop_propagation();
        }
    }

    fn step_mover(&mut self, step: isize, cx: &mut Context<Self>) {
        let Some(query) = self
            .moving
            .as_ref()
            .map(|m| m.input.read(cx).value().to_string())
        else {
            return;
        };
        let count = self.folders(&query).len().max(1) as isize;
        if let Some(moving) = self.moving.as_mut() {
            moving.selected = (moving.selected as isize + step).rem_euclid(count) as usize;
            cx.notify();
        }
    }

    fn confirm_mover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(moving) = self.moving.as_ref() else {
            return;
        };
        let query = moving.input.read(cx).value().to_string();
        let chosen = self.folders(&query).get(moving.selected).cloned();
        window.focus(&self.focus);
        match chosen {
            Some(folder) => self.move_into(folder, cx),
            None => self.moving = None,
        }
        cx.notify();
    }

    fn action_target(&self) -> Option<Arc<Path>> {
        self.selected.clone()
    }

    fn unread_link(&self, path: &Path) -> bool {
        self.node(path)
            .is_some_and(|n| n.link_out == Some(LinkOut::NotFollowed))
    }

    fn open(&mut self, index: u32, new_tab: bool, cx: &mut Context<Self>) {
        let node = &self.snapshot.nodes[index as usize];
        let path = node.path.clone();
        self.selected = Some(path.clone());
        match (node.kind, node.note) {
            (NodeKind::Folder, _) => self.toggle(path, cx),
            (_, Some(id)) => cx.emit(VaultViewEvent::OpenNote {
                note: id,
                line: None,
                new_tab,
            }),
            _ if node.link_out == Some(LinkOut::NotFollowed) => {}
            _ => cx.emit(VaultViewEvent::OpenFile {
                path: path.to_path_buf(),
                new_tab,
            }),
        }
        cx.notify();
    }

    fn render_row(&mut self, row: usize, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let index = self.rows[row];
        let node = &self.snapshot.nodes[index as usize];
        let folder = node.kind == NodeKind::Folder;
        let path = node.path.clone();
        let depth = node.depth as usize;
        if let Some(renaming) = self.renaming.as_ref().filter(|r| r.path == path) {
            return focus_kit::Dir::current()
                .pad_start(
                    div().w_full().h(size::list_row()).flex().items_center(),
                    px(8. + 14. * depth as f32 + 17.),
                )
                .pad_end(size::S2)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h(size::list_row() - size::S1)
                        .px(size::S1)
                        .flex()
                        .items_center()
                        .rounded(size::RADIUS)
                        .border_1()
                        .border_color(t.ink)
                        .bg(t.bg_2)
                        .ui()
                        .child(
                            Input::new(&renaming.input)
                                .appearance(false)
                                .px_0()
                                .py_0()
                                .ui(),
                        ),
                )
                .into_any_element();
        }
        let name = display_name(node.kind, &node.name);
        let active = self.active.as_ref() == Some(&path);
        let focused = self.selected.as_ref() == Some(&path);
        let drop_target: Arc<Path> = if folder {
            path.clone()
        } else {
            path.parent()
                .map(Arc::from)
                .unwrap_or_else(|| Arc::from(Path::new("")))
        };
        let drop_target_outer = drop_target.clone();
        let ghost: SharedString = name.clone().into();
        let menu_path = path.clone();
        let bounds = self.selected_bounds.clone();
        let unread = node.link_out == Some(LinkOut::NotFollowed);
        let tip: SharedString = path.display().to_string().into();
        div()
            .id(("tree-row", index as usize))
            .relative()
            .tooltip(move |_, cx| focus_kit::tooltip::Tooltip::view(tip.clone(), None, cx))
            .when(focused, |this| {
                this.child(
                    gpui::canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
            })
            .on_drag(DraggedPath(path.clone()), move |_, _, _, cx| {
                cx.new(|_| DragGhost(ghost.clone()))
            })
            .drag_over::<DraggedPath>({
                let hover = t.card_2;
                move |style, _, _, _| style.bg(hover)
            })
            .on_drop(cx.listener(move |this, dragged: &DraggedPath, _, cx| {
                this.drop_into(dragged.0.clone(), drop_target.clone(), cx)
            }))
            .drag_over::<ExternalPaths>({
                let hover = t.card_2;
                move |style, _, _, _| style.bg(hover)
            })
            .on_drop({
                let folder = drop_target_outer.clone();
                cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                    this.import(
                        paths.paths().to_vec(),
                        folder.to_path_buf(),
                        ImportMode::Copy,
                        cx,
                    )
                })
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.row_menu(menu_path.clone(), event.position, window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                TreeRow::new(("node", index as usize), name)
                    .depth(depth)
                    .when(folder, |r| {
                        r.folder(self.expanded.contains(&path) || !self.filter_text(cx).is_empty())
                            .count(node.note_count)
                    })
                    .active(active)
                    .muted(unread)
                    .focused(focused)
                    .when(node.link_out == Some(LinkOut::Followed), |r| {
                        r.trailing(
                            div()
                                .id(("outside", index as usize))
                                .flex_none()
                                .text_color(t.ink_4)
                                .child(focus_kit::icons::icon(Icon::EXTERNAL, size::ICON_DENSE))
                                .tooltip(|_, cx| {
                                    focus_kit::tooltip::Tooltip::view(
                                        focus_i18n::t("vault-outside").into(),
                                        None,
                                        cx,
                                    )
                                }),
                        )
                    })
                    .when(node.link_out == Some(LinkOut::NotFollowed), |r| {
                        r.trailing(
                            div()
                                .flex_none()
                                .ui_xs()
                                .text_color(t.ink_4)
                                .child(focus_i18n::t("vault-outside-skipped")),
                        )
                    })
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            this.click_row(index, event.modifiers().control, window, cx)
                        }),
                    ),
            )
            .into_any_element()
    }

    fn click_row(
        &mut self,
        index: u32,
        new_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus);
        self.open(index, new_tab, cx);
    }

    fn row_menu(
        &mut self,
        path: Arc<Path>,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected = Some(path.clone());
        self.open_menu(Some(path), at, window, cx);
    }

    fn empty_menu(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu(None, at, window, cx);
    }

    fn open_menu(
        &mut self,
        target: Option<Arc<Path>>,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = Some((target, at));
        self.menu_cursor = MenuCursor::default();
        focus_kit::focus_menu(&self.menu_focus, window, cx);
        self.paste_ready = None;
        let copied = self.copied.is_some();
        self._paste_check = Some(cx.spawn(async move |this, cx| {
            let ready = match focus_kit::clipboard::offers_files(cx).await {
                Ok(Some(offered)) => offered,
                Ok(None) => copied,
                Err(_) => true,
            };
            let _ = this.update(cx, |this, cx| {
                this.paste_ready = Some(ready);
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn menu_entries(&self, cx: &App) -> Vec<Option<(TreeCommand, MenuEntry)>> {
        let Some((target, _)) = self.menu.clone() else {
            return Vec::new();
        };
        let node = target.as_ref().and_then(|p| self.node(p));
        let is_note = node.is_some_and(|n| n.kind == NodeKind::Note);
        let entry = |id: &'static str, key: &str| MenuEntry::new(id, focus_i18n::t(key));
        let mut items = Vec::new();
        if is_note && let Some(note) = node.and_then(|n| n.note) {
            items.push(Some((
                TreeCommand::OpenTab(note),
                entry("menu-open-tab", "vault-menu-open-tab"),
            )));
            items.push(Some((
                TreeCommand::Versions(note),
                entry("menu-versions", "vault-menu-versions"),
            )));
            items.push(None);
        }
        if let Some(path) = target
            .clone()
            .filter(|_| node.is_some_and(|n| !matches!(n.kind, NodeKind::Note | NodeKind::Folder)))
        {
            items.push(Some((
                TreeCommand::OpenSystem(path.to_path_buf()),
                entry("menu-open-system", "ui-cmd-open-system"),
            )));
            items.push(None);
        }
        items.push(Some((
            TreeCommand::NewNote,
            entry("menu-new-note", "vault-menu-new-note").shortcut(shortcut(7)),
        )));
        items.push(Some((
            TreeCommand::NewFolder,
            entry("menu-new-folder", "vault-menu-new-folder").shortcut(shortcut(8)),
        )));
        let paste = |at: &Option<Arc<Path>>| {
            Some((
                TreeCommand::Paste(self.paste_folder(at.as_deref())),
                entry("menu-paste", "vault-menu-paste")
                    .shortcut(shortcut(17))
                    .enabled(self.paste_ready == Some(true)),
            ))
        };
        if let Some(path) = target.clone().filter(|p| !self.unread_link(p)) {
            items.push(None);
            items.push(Some((
                TreeCommand::Rename(path.clone()),
                entry("menu-rename", "vault-menu-rename").shortcut(shortcut(5)),
            )));
            if is_note {
                items.push(Some((
                    TreeCommand::Duplicate(path.clone()),
                    entry("menu-duplicate", "vault-menu-duplicate").shortcut(shortcut(9)),
                )));
            }
            items.push(Some((
                TreeCommand::Move(path.clone()),
                entry("menu-move", "vault-menu-move").shortcut(shortcut(10)),
            )));
            if let Some(note) = node.and_then(|n| n.note).filter(|_| is_note) {
                items.push(Some((
                    TreeCommand::Merge(note),
                    entry("menu-merge", "vault-menu-merge"),
                )));
            }
            items.push(None);
            items.push(Some((
                TreeCommand::Copy(path.clone()),
                entry("menu-copy", "vault-menu-copy").shortcut(shortcut(16)),
            )));
            items.push(paste(&target));
            items.push(Some((
                TreeCommand::CopyPath(path.clone()),
                entry("menu-copy-path", "vault-menu-copy-path").shortcut(shortcut(11)),
            )));
            if is_note {
                items.push(Some((
                    TreeCommand::CopyLink(path.clone()),
                    entry("menu-copy-link", "vault-menu-copy-link").shortcut(shortcut(12)),
                )));
                if let Some(bookmarks) = &self.bookmarks {
                    let marked = bookmarks.read(cx).is_bookmarked(&path);
                    items.push(Some((
                        TreeCommand::Bookmark(path.clone()),
                        entry(
                            "menu-bookmark",
                            if marked {
                                "vault-menu-unbookmark"
                            } else {
                                "vault-menu-bookmark"
                            },
                        ),
                    )));
                }
            }
            items.push(None);
            items.push(Some((
                TreeCommand::Trash(path),
                entry("menu-trash", "vault-menu-trash")
                    .shortcut(shortcut(6))
                    .danger(true),
            )));
        } else if target.is_none() {
            items.push(None);
            items.push(paste(&None));
        }
        items
    }

    fn run_command(&mut self, command: TreeCommand, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.menu.take().and_then(|(target, _)| target);
        match command {
            TreeCommand::OpenTab(note) => cx.emit(VaultViewEvent::OpenNote {
                note,
                line: None,
                new_tab: true,
            }),
            TreeCommand::Versions(note) => cx.emit(VaultViewEvent::ShowVersions(note)),
            TreeCommand::OpenSystem(path) => cx.emit(VaultViewEvent::OpenWithSystem(path)),
            TreeCommand::NewNote => self.create(false, target, window, cx),
            TreeCommand::NewFolder => self.create(true, target, window, cx),
            TreeCommand::Rename(path) => self.start_rename(path, window, cx),
            TreeCommand::Duplicate(path) => self.duplicate(path, cx),
            TreeCommand::Move(path) => self.start_move(path, window, cx),
            TreeCommand::Merge(note) => self.merge(note, cx),
            TreeCommand::Copy(path) => self.copy_file(path, cx),
            TreeCommand::Paste(folder) => self.paste_files(folder, cx),
            TreeCommand::CopyPath(path) => self.copy(path, false, cx),
            TreeCommand::CopyLink(path) => self.copy(path, true, cx),
            TreeCommand::Bookmark(path) => {
                if let Some(bookmarks) = self.bookmarks.clone() {
                    bookmarks.update(cx, |bookmarks, cx| bookmarks.toggle(&path, cx));
                }
            }
            TreeCommand::Trash(path) => self.request_trash(path, window, cx),
        }
        if self.menu_focus.is_focused(window) {
            window.focus(&self.focus);
        }
        cx.notify();
    }

    fn render_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_, at) = self.menu.clone()?;
        let focus = self.menu_focus.clone();
        Some(
            gpui::deferred(gpui::anchored().position(at).snap_to_window().child(
                focus_kit::fade_in("fade-tree-menu", keyboard_menu(self, &focus, cx), cx),
            ))
            .with_priority(1)
            .into_any_element(),
        )
    }

    fn render_confirm(&self, t: &Tokens, cx: &mut Context<Self>) -> Option<AnyElement> {
        let path = self.confirm.clone()?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let target = path.clone();
        let at = match self
            .selected_bounds
            .get()
            .filter(|_| self.selected.as_ref() == Some(&path))
        {
            Some(row) => row.bottom_left() + gpui::point(px(24.), px(0.)),
            None => gpui::point(px(24.), px(72.)),
        };
        let card = focus_kit::contain_tab(
            focus_kit::floating(t).track_focus(&self.confirm_scope),
            &self.confirm_scope,
        );
        Some(focus_kit::fade_in(
            "fade-trash",
            div().absolute().inset_0().occlude().bg(t.veil).child(
                gpui::deferred(
                    gpui::anchored()
                        .position(at)
                        .snap_to_window()
                        .child(focus_kit::fade_in(
                            "fade-trash-card",
                            card.min_w(px(240.))
                                .max_w(px(360.))
                                .flex()
                                .flex_col()
                                .gap(size::S3)
                                .p(size::S4)
                                .child(div().ui().text_color(t.ink).child(focus_i18n::t_args(
                                    "vault-trash-question",
                                    &[("name", name.as_str().into())],
                                )))
                                .child(div().ui_sm().text_color(t.ink_3).child(focus_i18n::t(
                                    if self.settings.trash == TrashKind::System {
                                        "vault-trash-detail-system"
                                    } else {
                                        "vault-trash-detail"
                                    },
                                )))
                                .child(
                                    focus_kit::Dir::current()
                                        .row()
                                        .justify_end()
                                        .gap(size::S2)
                                        .child(
                                            Button::new(
                                                "trash-cancel",
                                                focus_i18n::t("vault-cancel"),
                                            )
                                            .kind(ButtonKind::Secondary)
                                            .on_click(
                                                cx.listener(|this, _, window, cx| {
                                                    this.confirm = None;
                                                    window.focus(&this.focus);
                                                    cx.notify();
                                                }),
                                            ),
                                        )
                                        .child(
                                            Button::new(
                                                "trash-confirm",
                                                focus_i18n::t("vault-menu-trash"),
                                            )
                                            .kind(ButtonKind::Danger)
                                            .track_focus(&self.confirm_primary)
                                            .on_click(
                                                cx.listener(move |this, _, window, cx| {
                                                    window.focus(&this.focus);
                                                    this.trash(target.clone(), cx);
                                                }),
                                            ),
                                        ),
                                ),
                            cx,
                        )),
                )
                .with_priority(1),
            ),
            cx,
        ))
    }

    fn render_mover(&self, t: &Tokens, cx: &mut Context<Self>) -> Option<AnyElement> {
        let moving = self.moving.as_ref()?;
        let found = self.folders(moving.input.read(cx).value().as_ref());
        let first = moving.selected.saturating_sub(7);
        let rows = found
            .iter()
            .enumerate()
            .skip(first)
            .take(8)
            .map(|(i, folder)| {
                let folder = folder.clone();
                let (label, parent) = if folder.as_os_str().is_empty() {
                    (focus_i18n::t("vault-root"), None)
                } else {
                    (
                        folder
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        folder
                            .parent()
                            .filter(|parent| !parent.as_os_str().is_empty())
                            .map(|parent| parent.to_string_lossy().into_owned()),
                    )
                };
                let mut row = MenuRow::new(("move-to", i), label).icon(Icon::FOLDER);
                if let Some(parent) = parent {
                    row = row
                        .detail(parent)
                        .tooltip(folder.to_string_lossy().into_owned());
                }
                row.highlighted(i == moving.selected).on_click(cx.listener(
                    move |this, _, _, cx| {
                        if let Some(path) = this.moving.take().map(|m| m.path) {
                            this.drop_into(path, folder.clone(), cx);
                        }
                    },
                ))
            });
        Some(focus_kit::fade_in(
            "fade-move-menu",
            div()
                .absolute()
                .top(size::S1)
                .left(size::S1)
                .right(size::S1)
                .occlude()
                .key_context(MENU_CONTEXT)
                .on_action(cx.listener(|this, _: &MenuPrevious, _, cx| this.step_mover(-1, cx)))
                .on_action(cx.listener(|this, _: &MenuNext, _, cx| this.step_mover(1, cx)))
                .on_action(
                    cx.listener(|this, _: &MenuConfirm, window, cx| this.confirm_mover(window, cx)),
                )
                .on_action(cx.listener(|this, _: &MenuDismiss, window, cx| {
                    this.moving = None;
                    window.focus(&this.focus);
                    cx.notify();
                }))
                .child(
                    menu(t)
                        .child(
                            div()
                                .px(size::S2)
                                .pb(size::S1)
                                .child(Field::new(&moving.input).icon(Icon::FOLDER)),
                        )
                        .child(menu_separator(t))
                        .children(rows)
                        .when(found.is_empty(), |this| {
                            this.child(
                                div()
                                    .px(size::S3)
                                    .py(size::S1)
                                    .ui_sm()
                                    .text_color(t.ink_3)
                                    .child(focus_i18n::t("vault-move-none")),
                            )
                        }),
                ),
            cx,
        ))
    }
}

impl Render for FileTree {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.language != focus_i18n::generation() {
            self.language = focus_i18n::generation();
            self.filter.update(cx, |state, cx| {
                state.set_placeholder(focus_i18n::t("vault-filter-placeholder"), window, cx)
            });
        }
        let t = *cx.tokens();
        let empty = self.rows.is_empty() && self.filtering.is_none();
        let filtered = !self.filter_text(cx).is_empty();
        div()
            .id("file-tree")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.bg)
            .on_key_down(cx.listener(Self::type_ahead_key))
            .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| {
                let row = this.selected_row().map_or(0, |r| r.saturating_sub(1));
                this.select_row(row, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| {
                let row = this
                    .selected_row()
                    .map_or(0, |r| (r + 1).min(this.rows.len().saturating_sub(1)));
                this.select_row(row, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::keys::PagePrevious, _, cx| {
                this.step(crate::keys::Step::PagePrevious, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::keys::PageNext, _, cx| {
                this.step(crate::keys::Step::PageNext, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::keys::SelectFirst, _, cx| {
                this.step(crate::keys::Step::First, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::keys::SelectLast, _, cx| {
                this.step(crate::keys::Step::Last, cx)
            }))
            .on_action(cx.listener(|this, _: &Collapse, _, cx| {
                let Some(path) = this.selected.clone() else {
                    return;
                };
                if this.expanded.contains(&path) {
                    this.toggle(path, cx);
                } else if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    this.selected = Some(Arc::from(parent));
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &Expand, _, cx| {
                let Some(path) = this.selected.clone() else {
                    return;
                };
                if this.node(&path).is_some_and(|n| n.kind == NodeKind::Folder)
                    && !this.expanded.contains(&path)
                {
                    this.toggle(path, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpenSelected, window, cx| {
                if this.renaming.is_some() {
                    this.commit_rename(window, cx);
                    return;
                }
                if let Some(path) = this.confirm.clone() {
                    window.focus(&this.focus);
                    this.trash(path, cx);
                    return;
                }
                if this.moving.is_some() {
                    this.confirm_mover(window, cx);
                    return;
                }
                if let Some(index) = this.selected_row().map(|r| this.rows[r]) {
                    this.open(index, false, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Rename, window, cx| {
                if let Some(path) = this.action_target() {
                    this.start_rename(path, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Trash, window, cx| {
                if let Some(path) = this.action_target() {
                    this.request_trash(path, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NewNote, window, cx| {
                this.create(false, this.action_target(), window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewFolder, window, cx| {
                this.create(true, this.action_target(), window, cx)
            }))
            .on_action(cx.listener(|this, _: &Duplicate, _, cx| {
                if let Some(path) = this.action_target() {
                    this.duplicate(path, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &MoveTo, window, cx| {
                if let Some(path) = this.action_target() {
                    this.start_move(path, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &CopyPath, _, cx| {
                if let Some(path) = this.action_target() {
                    this.copy(path, false, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &CopyFiles, _, cx| {
                if let Some(path) = this.action_target() {
                    this.copy_file(path, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PasteFiles, _, cx| {
                let folder = this.paste_folder(this.selected.as_deref());
                this.paste_files(folder, cx);
            }))
            .on_action(cx.listener(|this, _: &CopyLink, _, cx| {
                if let Some(path) = this.action_target() {
                    this.copy(path, true, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FocusFilter, window, cx| {
                this.filter.update(cx, |state, cx| state.focus(window, cx))
            }))
            .on_action(cx.listener(|this, _: &ContextMenu, window, cx| {
                let at = match (this.selected_row(), this.selected_bounds.get()) {
                    (Some(_), Some(row)) => row.bottom_left() + gpui::point(px(24.), px(0.)),
                    _ => gpui::point(px(24.), px(72.)),
                };
                this.open_menu(this.selected.clone(), at, window, cx);
            }))
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                if let Some(renaming) = this.renaming.take() {
                    window.focus(&this.focus);
                    if renaming.then_write {
                        cx.emit(VaultViewEvent::WriteNote);
                    }
                } else if this.moving.take().is_some() || this.confirm.is_some() {
                    window.focus(&this.focus);
                }
                this.menu = None;
                this.confirm = None;
                cx.notify();
            }))
            .child(
                div()
                    .flex_none()
                    .p(size::S2)
                    .child(Field::new(&self.filter).icon(Icon::SEARCH)),
            )
            .child(
                div()
                    .id("tree-body")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .on_drop(cx.listener(|this, dragged: &DraggedPath, _, cx| {
                        this.drop_into(dragged.0.clone(), Arc::from(Path::new("")), cx)
                    }))
                    .on_drag_move(cx.listener(
                        |this, event: &DragMoveEvent<DraggedPath>, window, cx| {
                            if outside(event.event.position, window.viewport_size()) {
                                let path = event.drag(cx).0.clone();
                                this.drag_out(&path, window);
                            }
                        },
                    ))
                    .drag_over::<ExternalPaths>({
                        let hover = t.bg_2;
                        move |style, _, _, _| style.bg(hover)
                    })
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                        this.import(paths.paths().to_vec(), PathBuf::new(), ImportMode::Copy, cx)
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            this.empty_menu(event.position, window, cx)
                        }),
                    )
                    .when(empty, |this| {
                        this.child(div().p(size::S4).ui_sm().text_color(t.ink_3).child(
                            focus_i18n::t(if filtered {
                                "vault-filter-none"
                            } else {
                                "vault-tree-empty"
                            }),
                        ))
                    })
                    .child(
                        uniform_list(
                            "file-tree-rows",
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
                        "file-tree-scrollbar",
                        self.scroll.clone(),
                    ))
                    .children(self.render_mover(&t, cx)),
            )
            .children(self.render_menu(cx))
            .children(self.render_confirm(&t, cx))
    }
}

fn outside(position: Point<Pixels>, size: gpui::Size<Pixels>) -> bool {
    position.x < px(0.)
        || position.y < px(0.)
        || position.x >= size.width
        || position.y >= size.height
}

#[derive(Clone)]
enum TreeCommand {
    OpenTab(focus_core::NoteId),
    Versions(focus_core::NoteId),
    OpenSystem(PathBuf),
    NewNote,
    NewFolder,
    Rename(Arc<Path>),
    Duplicate(Arc<Path>),
    Move(Arc<Path>),
    Merge(focus_core::NoteId),
    Copy(Arc<Path>),
    Paste(PathBuf),
    CopyPath(Arc<Path>),
    CopyLink(Arc<Path>),
    Bookmark(Arc<Path>),
    Trash(Arc<Path>),
}

impl KeyboardMenu for FileTree {
    fn menu_name(&self) -> &'static str {
        "tree-menu"
    }

    fn menu_items(&self, cx: &App) -> Vec<MenuItem> {
        self.menu_entries(cx)
            .into_iter()
            .map(|item| match item {
                Some((_, entry)) => MenuItem::Row(entry),
                None => MenuItem::Separator,
            })
            .collect()
    }

    fn menu_cursor(&self) -> MenuCursor {
        self.menu_cursor
    }

    fn set_menu_cursor(&mut self, cursor: MenuCursor) {
        self.menu_cursor = cursor;
    }

    fn menu_run(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let command = self
            .menu_entries(cx)
            .into_iter()
            .flatten()
            .nth(row)
            .map(|(command, _)| command);
        if let Some(command) = command {
            self.run_command(command, window, cx);
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
    use super::{contains_folded, outside, stem_len};
    use focus_core::NodeKind;
    use gpui::{point, px, size};

    #[test]
    fn a_rename_selects_the_name_without_its_extension() {
        assert_eq!(stem_len(NodeKind::Note, "Evidence handling"), 17);
        assert_eq!(stem_len(NodeKind::Folder, "v1.2 reports"), 12);
        assert_eq!(stem_len(NodeKind::Attachment, "scope-map.png"), 9);
        assert_eq!(stem_len(NodeKind::Attachment, "archive.tar.gz"), 11);
        assert_eq!(stem_len(NodeKind::Other, ".gitignore"), 10);
        assert_eq!(stem_len(NodeKind::Other, "README"), 6);
    }

    #[test]
    fn a_drag_leaves_the_window_past_any_edge() {
        let window = size(px(800.), px(600.));
        assert!(!outside(point(px(0.), px(0.)), window));
        assert!(!outside(point(px(799.), px(599.)), window));
        for position in [
            point(px(-1.), px(10.)),
            point(px(10.), px(-1.)),
            point(px(800.), px(10.)),
            point(px(10.), px(600.)),
        ] {
            assert!(outside(position, window), "{position:?}");
        }
    }

    #[test]
    fn the_filter_ignores_case_in_and_out_of_ascii() {
        let mut buffer = String::new();
        assert!(contains_folded("Kerberoasting SPN", "spn", &mut buffer));
        assert!(!contains_folded("Kerberoasting", "spn", &mut buffer));
        assert!(contains_folded("Élévation", "élé", &mut buffer));
        assert!(contains_folded("Note", "", &mut buffer));
    }
}
