use focus_core::NoteId;
use focus_view_transfer::{
    AttachmentsDialog, AttachmentsMode, ExportDialog, ImportDialog, TransferEvent, TransferServices,
};
use gpui::{AnyView, AppContext, Context, Entity, EventEmitter, Focusable, Subscription, Window};

use crate::shell::{Overlay, Shell};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transfer {
    Export { vault: bool },
    Import,
    Tidy,
    Orphans,
}

impl Shell {
    fn transfer_services(&self, cx: &gpui::App) -> Option<TransferServices> {
        let library = self.library.as_ref()?;
        let mut policy = self.store.settings.attachments.clone();
        policy.readable_names = self.prefs.read(cx).bool("editor.paste_names");
        let home = focus_core::settings::home_dir().unwrap_or_else(|| "/".into());
        Some(TransferServices {
            vault: library.vault().clone(),
            default_directory: home,
            attachment_policy: policy,
        })
    }

    pub fn open_transfer(&mut self, which: Transfer, window: &mut Window, cx: &mut Context<Self>) {
        let Some(services) = self.transfer_services(cx) else {
            return;
        };
        let note: Option<NoteId> = self.last_note(cx);
        let (view, subscription): (AnyView, Subscription) = match which {
            Transfer::Export { vault } => {
                let note = if vault { None } else { note };
                self.listen_transfer(
                    cx.new(|cx| ExportDialog::new(services, note, None, window, cx)),
                    window,
                    cx,
                )
            }
            Transfer::Import => {
                let folder = match self.prefs.read(cx).get("general.new_note_folder") {
                    Some(crate::settings::schema::Value::Text(folder)) => folder.to_string(),
                    _ => String::new(),
                };
                self.listen_transfer(
                    cx.new(|cx| ImportDialog::new(services, &folder, window, cx)),
                    window,
                    cx,
                )
            }
            Transfer::Tidy => self.listen_transfer(
                cx.new(|cx| {
                    AttachmentsDialog::new(services, AttachmentsMode::Tidy(note), window, cx)
                }),
                window,
                cx,
            ),
            Transfer::Orphans => self.listen_transfer(
                cx.new(|cx| AttachmentsDialog::new(services, AttachmentsMode::Orphans, window, cx)),
                window,
                cx,
            ),
        };
        self.overlay = Overlay::Transfer {
            view,
            _events: subscription,
        };
        cx.notify();
    }

    fn listen_transfer<V: EventEmitter<TransferEvent> + Focusable + gpui::Render>(
        &mut self,
        dialog: Entity<V>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (AnyView, Subscription) {
        dialog.read(cx).focus_handle(cx).focus(window);
        let subscription = cx.subscribe_in(
            &dialog,
            window,
            |this, _, event: &TransferEvent, window, cx| match event {
                TransferEvent::Close => this.close_overlay(window, cx),
                TransferEvent::RevealFolder(path) => this.open_file(path, cx),
                TransferEvent::VaultChanged => {
                    let notes: Vec<NoteId> =
                        this.all_tabs().iter().map(|tab| tab.read(cx).id).collect();
                    this.reload_notes(&notes, window, cx);
                }
            },
        );
        (dialog.into(), subscription)
    }

    pub fn copy_note(&mut self, html: bool, cx: &mut Context<Self>) {
        let (Some(library), Some(note)) = (self.library.clone(), self.last_note(cx)) else {
            return;
        };
        let result = if html {
            focus_view_transfer::export::copy_html(library.vault(), note)
        } else {
            focus_view_transfer::export::copy_markdown(library.vault(), note)
        };
        match result {
            Ok(text) => {
                focus_kit::clipboard::copy_text(cx, text);
                self.notify_user(
                    focus_kit::Tone::Ok,
                    focus_i18n::t(if html {
                        "ui-copied-html"
                    } else {
                        "ui-copied-markdown"
                    }),
                    cx,
                );
            }
            Err(error) => {
                eprintln!("focus: copying the note: {error:#}");
                self.notify_user(
                    focus_kit::Tone::Danger,
                    focus_i18n::t_args(
                        "errors-copy-note",
                        &[("detail", format!("{error:#}").as_str().into())],
                    ),
                    cx,
                );
            }
        }
    }
}
