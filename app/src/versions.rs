use std::path::PathBuf;

use focus_core::NoteId;
use focus_view_history::{VersionsDialog, VersionsEvent};
use gpui::{AppContext, Context, Entity, Window};

use crate::editor::Load;
use crate::shell::{Overlay, Shell};

impl Shell {
    pub fn show_active_versions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(note) = self.last_note(cx) {
            self.show_versions(note, window, cx);
        }
    }

    pub fn show_versions(&mut self, note: NoteId, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .library
            .as_ref()
            .is_none_or(|library| library.note(note).is_none())
        {
            self.notify_user(
                focus_kit::Tone::Warn,
                focus_i18n::t("ui-versions-outside"),
                cx,
            );
            return;
        }
        let writes: Vec<_> = self
            .all_tabs()
            .into_iter()
            .filter(|tab| {
                let editor = tab.read(cx);
                editor.id == note
                    && editor.dirty
                    && !editor.follower
                    && matches!(editor.load, Load::Ready)
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|tab| tab.update(cx, |editor, cx| editor.save(false, cx)))
            .collect();
        cx.spawn_in(window, async move |this, cx| {
            for write in writes {
                write.await;
            }
            let _ = this.update_in(cx, |this, window, cx| this.open_versions(note, window, cx));
        })
        .detach();
    }

    fn open_versions(&mut self, note: NoteId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let vault = library.vault().clone();
        let view = cx.new(|cx| VersionsDialog::new(vault, note, window, cx));
        let events = cx.subscribe_in(
            &view,
            window,
            |this, _, event: &VersionsEvent, window, cx| match event {
                VersionsEvent::Close => this.close_overlay(window, cx),
                VersionsEvent::Restored { path } => {
                    if let Some(note) = this
                        .library
                        .as_ref()
                        .and_then(|library| library.note_by_path(path))
                    {
                        this.reload_notes(&[note], window, cx);
                    }
                }
                VersionsEvent::Notify { tone, text } => this.notify_user(*tone, text.clone(), cx),
                VersionsEvent::DeleteCopy { path } => this.delete_copy(path.clone(), window, cx),
            },
        );
        self.overlay = Overlay::Versions {
            view,
            _events: events,
        };
        cx.notify();
    }

    fn delete_copy(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let kind = self.tree_settings(cx).trash;
        let task = cx.background_spawn(async move { library.vault().trash_with(&path, kind) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                match result {
                    Ok(()) => this.notify_user(
                        focus_kit::Tone::Ok,
                        focus_i18n::t("history-copy-deleted"),
                        cx,
                    ),
                    Err(error) => {
                        this.notify_user(focus_kit::Tone::Danger, crate::errors::vault(&error), cx)
                    }
                }
                if let Some(view) = this.versions_dialog().cloned() {
                    view.update(cx, |dialog, cx| dialog.reload(cx));
                }
            });
        })
        .detach();
    }

    pub fn versions_dialog(&self) -> Option<&Entity<VersionsDialog>> {
        match &self.overlay {
            Overlay::Versions { view, .. } => Some(view),
            _ => None,
        }
    }
}
