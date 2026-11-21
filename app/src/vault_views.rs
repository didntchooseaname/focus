use std::path::{Path, PathBuf};
use std::sync::Arc;

use focus_view_vault::{
    Bookmarks, BrokenLinks, FileTree, TagTree, TaskList, TreeSettings, VaultViewEvent,
};
use gpui::{AppContext, Context, Entity, EventEmitter, Window};

use crate::settings::schema::Value;
use crate::shell::Shell;

pub struct VaultViews {
    pub tree: Entity<FileTree>,
    pub tags: Entity<TagTree>,
    pub tasks: Entity<TaskList>,
    pub broken: Entity<BrokenLinks>,
    pub bookmarks: Entity<Bookmarks>,
}

const BOOKMARKS: &str = "bookmarks";
const RECENT: &str = "recent_notes";

impl Shell {
    fn new_note_folder(&self, cx: &gpui::App) -> String {
        match self.prefs.read(cx).get("general.new_note_folder") {
            Some(Value::Text(folder)) => folder.to_string(),
            _ => String::new(),
        }
    }

    pub fn tree_settings(&self, cx: &gpui::App) -> TreeSettings {
        let prefs = self.prefs.read(cx);
        TreeSettings {
            new_note_folder: self.new_note_folder(cx),
            confirm_delete: prefs.bool("general.confirm_delete"),
            update_links: prefs.bool("files.update_links"),
            show_other: prefs.bool("files.show_other"),
            trash: if prefs.choice("files.trash") == 1 {
                focus_core::TrashKind::System
            } else {
                focus_core::TrashKind::Vault
            },
        }
    }

    pub fn mount_vault_views(
        &mut self,
        expanded: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let vault = library.vault().clone();
        let settings = self.tree_settings(cx);
        let folder = settings.new_note_folder.clone();
        let tree = cx.new(|cx| FileTree::new(vault.clone(), settings, window, cx));
        if !expanded.is_empty() {
            tree.update(cx, |tree, cx| tree.set_expanded(expanded, cx));
        }
        let root = library.root().to_path_buf();
        let stored = self.store.vault_list(BOOKMARKS, &root);
        let first_time = stored.is_none();
        let recent = self.store.vault_list(RECENT, &root).unwrap_or_default();
        let bookmarks = cx.new(|cx| Bookmarks::new(vault.clone(), stored, recent, cx));
        if first_time {
            let list = bookmarks.read(cx).bookmarks().to_vec();
            self.store.set_vault_list(BOOKMARKS, &root, &list);
        }
        tree.update(cx, |tree, _| tree.set_bookmarks(Some(bookmarks.clone())));
        let views = VaultViews {
            tags: cx.new(|cx| TagTree::new(vault.clone(), cx)),
            tasks: cx.new(|cx| TaskList::new(vault.clone(), cx)),
            broken: cx.new(|cx| BrokenLinks::new(vault, folder, cx)),
            tree,
            bookmarks,
        };
        self.listen_vault_view(&views.tree, true, window, cx);
        self.listen_vault_view(&views.tags, false, window, cx);
        self.listen_vault_view(&views.tasks, false, window, cx);
        self.listen_vault_view(&views.broken, false, window, cx);
        self.listen_vault_view(&views.bookmarks, false, window, cx);
        self._subscriptions.push(cx.subscribe(
            &views.tree,
            |this, _, status: &focus_view_vault::ImportStatus, cx| {
                this.import = status.running.then_some(*status);
                cx.notify();
            },
        ));
        self.vault_views = Some(views);
        self.mount_property_types(window, cx);
        self.apply_excluded_folders(cx);
        if let Some(path) = self
            .last_note(cx)
            .and_then(|id| library.note(id))
            .map(|meta| meta.path.clone())
        {
            self.reveal_in_tree(Some(path), cx);
        }
    }

    fn listen_vault_view<V: EventEmitter<VaultViewEvent>>(
        &mut self,
        view: &Entity<V>,
        from_tree: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._subscriptions.push(cx.subscribe_in(
            view,
            window,
            move |this, _, event: &VaultViewEvent, window, cx| {
                this.on_vault_view_event(event, from_tree, window, cx)
            },
        ));
    }

    fn on_vault_view_event(
        &mut self,
        event: &VaultViewEvent,
        from_tree: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            VaultViewEvent::OpenNote {
                note,
                line,
                new_tab,
            } => {
                let tree = self
                    .vault_views
                    .as_ref()
                    .map(|views| gpui::Focusable::focus_handle(views.tree.read(cx), cx));
                let focused = window.focused(cx).filter(|focused| {
                    from_tree
                        && tree.as_ref().is_some_and(|tree| tree != focused)
                        && *focused != self.focus
                });
                self.open_note(*note, *new_tab, window, cx);
                if let Some(focused) = focused {
                    window.focus(&focused);
                }
                if let Some(line) = line {
                    self.pending_line = Some((*note, *line));
                }
            }
            VaultViewEvent::ShowVersions(note) => self.show_versions(*note, window, cx),
            VaultViewEvent::SearchTag(tag) => self.show_search(&format!("tag:#{tag}"), window, cx),
            VaultViewEvent::RenameTag(tag) => self.open_tag_rename(tag, window, cx),
            VaultViewEvent::MergeNote(note) => self.open_merge(Some(*note), window, cx),
            VaultViewEvent::OpenFile { path, .. } if crate::bases::is_base(path) => {
                self.open_base(path, window, cx)
            }
            VaultViewEvent::OpenFile { path, .. } if crate::media::opens(path) => {
                if let Some(absolute) = self.library.as_ref().map(|library| library.absolute(path))
                {
                    self.open_media(&absolute, None, window, cx);
                }
            }
            VaultViewEvent::OpenFile { path, .. } => {
                if let Some(absolute) = self.library.as_ref().map(|library| library.absolute(path))
                {
                    self.open_file(&absolute, cx);
                }
            }
            VaultViewEvent::Notify { message, tone } => {
                let tone = match tone {
                    focus_view_vault::Tone::Ok => focus_kit::Tone::Ok,
                    focus_view_vault::Tone::Danger => focus_kit::Tone::Danger,
                };
                self.notify(tone, crate::notices::NoticeText::Vault(message.clone()), cx);
            }
            VaultViewEvent::BookmarksChanged(list) => self.store_vault_list(BOOKMARKS, list, cx),
            VaultViewEvent::RecentChanged(list) => self.store_vault_list(RECENT, list, cx),
            VaultViewEvent::WriteNote => self.write_new_note(window, cx),
            VaultViewEvent::OpenWithSystem(path) => {
                if let Some(absolute) = self.library.as_ref().map(|library| library.absolute(path))
                {
                    self.open_file(&absolute, cx);
                }
            }
        }
    }

    pub fn vault_views_changed(&mut self, event: &focus_core::VaultEvent, cx: &mut Context<Self>) {
        let Some(views) = &self.vault_views else {
            return;
        };
        views
            .tree
            .update(cx, |view, cx| view.vault_changed(event, cx));
        views
            .tags
            .update(cx, |view, cx| view.vault_changed(event, cx));
        views
            .tasks
            .update(cx, |view, cx| view.vault_changed(event, cx));
        views
            .broken
            .update(cx, |view, cx| view.vault_changed(event, cx));
        views
            .bookmarks
            .update(cx, |view, cx| view.vault_changed(event, cx));
    }

    fn store_vault_list(&mut self, key: &str, list: &[PathBuf], cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        self.store.set_vault_list(key, library.root(), list);
        self.schedule_save(cx);
    }

    pub fn note_opened(&mut self, path: &Path, cx: &mut Context<Self>) {
        if let Some(views) = &self.vault_views {
            views
                .bookmarks
                .update(cx, |bookmarks, cx| bookmarks.opened(path, cx));
        }
    }

    pub fn apply_excluded_folders(&mut self, cx: &mut Context<Self>) {
        let templates = self.text_pref("general.templates", cx);
        let excluded =
            if self.prefs.read(cx).bool("files.hide_templates") && !templates.trim().is_empty() {
                vec![PathBuf::from(templates.trim().trim_matches('/'))]
            } else {
                Vec::new()
            };
        let Some(views) = &self.vault_views else {
            return;
        };
        views.tasks.update(cx, |tasks, cx| {
            tasks.set_excluded_folders(excluded.clone(), cx)
        });
        views
            .broken
            .update(cx, |broken, cx| broken.set_excluded_folders(excluded, cx));
    }

    pub fn vault_view_settings_changed(&mut self, cx: &mut Context<Self>) {
        let settings = self.tree_settings(cx);
        let folder = settings.new_note_folder.clone();
        let Some(views) = &self.vault_views else {
            return;
        };
        views
            .tree
            .update(cx, |tree, cx| tree.set_settings(settings, cx));
        views
            .broken
            .update(cx, |broken, _| broken.set_new_note_folder(folder));
    }

    pub fn reveal_in_tree(&mut self, path: Option<Arc<Path>>, cx: &mut Context<Self>) {
        if let Some(views) = &self.vault_views {
            views.tree.update(cx, |tree, cx| tree.reveal(path, cx));
        }
    }

    pub fn cancel_import(&mut self, cx: &mut Context<Self>) {
        if let Some(views) = &self.vault_views {
            views.tree.update(cx, |tree, _| tree.cancel_import());
        }
    }

    pub fn expanded_folders(&self, cx: &gpui::App) -> Vec<PathBuf> {
        self.vault_views
            .as_ref()
            .map(|views| views.tree.read(cx).expanded())
            .unwrap_or_default()
    }
}

fn today() -> (focus_view_vault::Date, focus_view_vault::Time) {
    use chrono::{Datelike, Timelike};
    let now = chrono::Local::now();
    (
        focus_view_vault::Date {
            year: now.year(),
            month: now.month(),
            day: now.day(),
        },
        focus_view_vault::Time {
            hour: now.hour(),
            minute: now.minute(),
        },
    )
}

impl Shell {
    fn text_pref(&self, key: &str, cx: &gpui::App) -> String {
        match self.prefs.read(cx).get(key) {
            Some(Value::Text(text)) => text.to_string(),
            _ => String::new(),
        }
    }

    pub fn open_daily_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let (folder, format) = (
            self.text_pref("general.daily_folder", cx),
            self.text_pref("general.daily_format", cx),
        );
        let (date, time) = today();
        let template = self.daily_template(cx);
        match focus_view_vault::daily_note(library.vault(), &folder, &format, date, time, template)
        {
            Ok((id, _)) => self.open_note(id, false, window, cx),
            Err(error) => self.notify(
                focus_kit::Tone::Danger,
                crate::notices::NoticeText::Vault(focus_view_vault::error_message(&error)),
                cx,
            ),
        }
    }

    pub fn choose_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let templates =
            focus_view_vault::templates(library.vault(), &self.text_pref("general.templates", cx));
        self.open_palette("", window, cx);
        if let crate::shell::Overlay::Palette { view, .. } = &self.overlay {
            view.update(cx, |palette, cx| {
                palette.choose_template(templates, window, cx)
            });
        }
    }

    pub fn note_from_host_template(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(library), Some((name, text))) = (
            self.library.clone(),
            crate::templates::host_templates().get(index),
        ) else {
            return;
        };
        let (date, time) = today();
        let folder = self.new_note_folder(cx);
        match focus_view_vault::note_from_text(library.vault(), text, &folder, name, date, time) {
            Ok(id) => self.open_note(id, true, window, cx),
            Err(error) => self.notify(
                focus_kit::Tone::Danger,
                crate::notices::NoticeText::Vault(focus_view_vault::error_message(&error)),
                cx,
            ),
        }
    }

    pub fn note_from_template(
        &mut self,
        template: focus_core::NoteId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let (date, time) = today();
        let folder = self.new_note_folder(cx);
        match focus_view_vault::note_from_template(
            library.vault(),
            template,
            &folder,
            &focus_i18n::t("ui-note-untitled"),
            date,
            time,
        ) {
            Ok(id) => self.open_note(id, true, window, cx),
            Err(error) => self.notify(
                focus_kit::Tone::Danger,
                crate::notices::NoticeText::Vault(focus_view_vault::error_message(&error)),
                cx,
            ),
        }
    }
}
