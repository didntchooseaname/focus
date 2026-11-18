use std::io::Read as _;
use std::path::{Path, PathBuf};

use focus_kit::{ActiveTokens, Button, ButtonKind, Field, Flow as _, Type, size};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled, Subscription, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};

use crate::settings::schema::Prefs;
use crate::shell::{Overlay, Shell};

pub const MAX_BYTES: usize = 64 * 1024;

pub struct CaptureDialog {
    pub input: Entity<InputState>,
    _events: Subscription,
}

struct Destination {
    note: String,
    daily_folder: String,
    daily_format: String,
    daily_template: Option<focus_core::NoteId>,
    format: String,
}

impl Destination {
    fn read(prefs: &Prefs, daily_template: Option<focus_core::NoteId>) -> Destination {
        Destination {
            note: prefs.text("general.capture_note"),
            daily_folder: prefs.text("general.daily_folder"),
            daily_format: prefs.text("general.daily_format"),
            daily_template,
            format: prefs.text("general.capture_format"),
        }
    }

    fn to(&self) -> focus_view_vault::CaptureTo<'_> {
        if self.note.trim().is_empty() {
            focus_view_vault::CaptureTo::Daily {
                folder: &self.daily_folder,
                format: &self.daily_format,
                template: self.daily_template,
            }
        } else {
            focus_view_vault::CaptureTo::Note(&self.note)
        }
    }

    fn write(
        &self,
        vault: &focus_core::Vault,
        text: &str,
    ) -> focus_core::Result<(focus_core::NoteId, PathBuf)> {
        let (date, time) = now();
        let id = focus_view_vault::capture(vault, self.to(), &self.format, text, date, time)?;
        let path = vault
            .note(id)
            .map(|meta| vault.absolute(&meta.path))
            .unwrap_or_else(|| vault.root().to_path_buf());
        Ok((id, path))
    }
}

pub(crate) fn now() -> (focus_view_vault::Date, focus_view_vault::Time) {
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

fn template_path(text: &str) -> Option<PathBuf> {
    let path = Path::new(text.trim().trim_matches('/'));
    let clean = path.components().next().is_some()
        && path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
    clean.then(|| path.to_path_buf())
}

pub(crate) fn find_template(vault: &focus_core::Vault, text: &str) -> Option<focus_core::NoteId> {
    let path = template_path(text)?;
    vault
        .note_by_path(&path)
        .or_else(|| vault.note_by_path(&PathBuf::from(format!("{}.md", path.display()))))
}

fn command_text(text: &str) -> Result<String, String> {
    let text = if text == "-" {
        let mut read = String::new();
        std::io::stdin()
            .take(MAX_BYTES as u64 + 1)
            .read_to_string(&mut read)
            .map_err(|error| format!("reading the standard input: {error}"))?;
        read
    } else {
        text.to_string()
    };
    if text.len() > MAX_BYTES {
        return Err(format!("the text is longer than {MAX_BYTES} bytes"));
    }
    let text = text.trim_end_matches(['\n', '\r']).to_string();
    if text.trim().is_empty() {
        return Err("the text is empty".into());
    }
    Ok(text)
}

pub fn run_command(text: &str) -> i32 {
    let text = match command_text(text) {
        Ok(text) => text,
        Err(message) => {
            eprintln!("focus: capture: {message}");
            return 2;
        }
    };
    if let Some(socket) = crate::instance::socket_path()
        && let Some(answer) = crate::instance::capture_over(&socket, &text)
    {
        let answer = answer.trim_end();
        if let Some(path) = answer.strip_prefix("written ") {
            eprintln!("focus: capture: line added to {path}");
            return 0;
        }
        if let Some(reason) = answer.strip_prefix("failed ") {
            eprintln!("focus: capture: nothing written: {reason}");
            return 1;
        }
    }
    write_without_window(&text)
}

fn write_without_window(text: &str) -> i32 {
    let (store, error) = crate::session::Store::load(false);
    if let Some(error) = error {
        eprintln!("focus: capture: nothing written: the settings cannot be read: {error}");
        return 1;
    }
    let Some(root) = store.settings.last_vault.clone().filter(|p| p.is_dir()) else {
        eprintln!(
            "focus: capture: nothing written: no vault was opened yet, open one in Focus first"
        );
        return 2;
    };
    let mut prefs = Prefs::new();
    prefs.load(store.values());
    let ignore: Vec<String> = prefs
        .text("files.exclude")
        .split([',', '\n'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    let external = if prefs.bool("files.follow_external_links") {
        focus_core::ExternalLinks::Follow
    } else {
        focus_core::ExternalLinks::Skip
    };
    let cache_dir = focus_core::settings::cache_dir_for(&root);
    crate::private::log_failure(&cache_dir, crate::private::private_dir(&cache_dir));
    let vault = match focus_core::Vault::open_with(
        &root,
        focus_core::OpenOptions {
            cache_dir,
            ignore,
            watch: false,
        },
        external,
    ) {
        Ok(vault) => vault,
        Err(error) => {
            eprintln!(
                "focus: capture: nothing written: opening {}: {error}",
                root.display()
            );
            return 1;
        }
    };
    if let Err(error) = vault.set_history(
        &focus_core::settings::history_dir_for(vault.root()),
        store.settings.history,
    ) {
        eprintln!("focus: capture: the note history is not kept: {error}");
    }
    let template = find_template(&vault, &prefs.text("general.daily_template"));
    match Destination::read(&prefs, template).write(&vault, text) {
        Ok((_, path)) => {
            eprintln!("focus: capture: line added to {}", path.display());
            0
        }
        Err(error) => {
            eprintln!("focus: capture: nothing written: {error}");
            1
        }
    }
}

impl Shell {
    pub fn open_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.library.is_none() {
            return self.notify_user(
                focus_kit::Tone::Warn,
                focus_i18n::t("ui-capture-no-vault"),
                cx,
            );
        }
        let input = cx.new(|cx| InputState::new(window, cx));
        let events = cx.subscribe_in(
            &input,
            window,
            |this, input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().to_string();
                    this.close_overlay(window, cx);
                    if !text.trim().is_empty() {
                        this.capture_text(text, None, window, cx);
                    }
                }
            },
        );
        input.update(cx, |input, cx| input.focus(window, cx));
        self.overlay = Overlay::Capture(CaptureDialog {
            input,
            _events: events,
        });
        cx.notify();
    }

    pub(crate) fn capture_text(
        &mut self,
        text: String,
        reply: Option<std::sync::mpsc::Sender<String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answer = |reply: &Option<std::sync::mpsc::Sender<String>>, text: String| {
            if let Some(reply) = reply {
                let _ = reply.send(text);
            }
        };
        let Some(library) = self.library.clone() else {
            return answer(&reply, "novault\n".into());
        };
        let destination = Destination::read(self.prefs.read(cx), self.daily_template(cx));
        let vault = library.vault().clone();
        let task = cx.background_spawn(async move { destination.write(&vault, &text) });
        self.pending += 1;
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            match &result {
                Ok((_, path)) => answer(&reply, format!("written {}\n", path.display())),
                Err(error) => answer(&reply, format!("failed {error}\n")),
            }
            let _ = this.update_in(cx, |this, window, cx| {
                this.pending -= 1;
                match result {
                    Ok((id, _)) => {
                        this.poll_history_error(cx);
                        this.reload_notes(&[id], window, cx);
                        let path = library
                            .note(id)
                            .map(|meta| meta.path.display().to_string())
                            .unwrap_or_default();
                        this.notify_user(
                            focus_kit::Tone::Ok,
                            focus_i18n::t_args(
                                "ui-capture-done",
                                &[("path", path.as_str().into())],
                            ),
                            cx,
                        );
                    }
                    Err(error) => this.notify_user(
                        focus_kit::Tone::Danger,
                        focus_i18n::t_args(
                            "ui-capture-failed",
                            &[("detail", crate::errors::vault(&error).as_str().into())],
                        ),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn submit_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Overlay::Capture(dialog) = &self.overlay else {
            return;
        };
        let text = dialog.input.read(cx).value().to_string();
        self.close_overlay(window, cx);
        if !text.trim().is_empty() {
            self.capture_text(text, None, window, cx);
        }
    }

    pub fn render_capture(&self, input: Entity<InputState>, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        let note = self.prefs.read(cx).text("general.capture_note");
        let target = if note.trim().is_empty() {
            focus_i18n::t("ui-capture-to-daily")
        } else {
            focus_i18n::t_args("ui-capture-to-note", &[("path", note.trim().into())])
        };
        div()
            .id("capture-veil")
            .absolute()
            .inset_0()
            .flow_row()
            .items_start()
            .justify_center()
            .pt(px(160.))
            .bg(t.veil)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_overlay(window, cx)),
            )
            .child(
                focus_kit::floating(&t)
                    .id("capture-dialog")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .flex()
                    .flex_col()
                    .gap(size::S3)
                    .p(size::S4)
                    .w(px(560.))
                    .max_w_full()
                    .child(
                        div()
                            .display_sm()
                            .text_color(t.ink)
                            .child(focus_i18n::t("ui-cmd-quick-capture")),
                    )
                    .child(div().ui_sm().text_color(t.ink_3).child(target))
                    .child(Field::new(&input))
                    .child(
                        div()
                            .flow_row()
                            .justify_end()
                            .gap(size::S2)
                            .child(
                                Button::new("capture-cancel", focus_i18n::t("ui-action-cancel"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_overlay(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("capture-add", focus_i18n::t("ui-capture-add"))
                                    .kind(ButtonKind::Primary)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.submit_capture(window, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_text_is_trimmed_and_bounded() {
        assert_eq!(command_text("Call Sam\n").unwrap(), "Call Sam");
        assert!(command_text("  \n").is_err());
        assert!(command_text(&"x".repeat(MAX_BYTES + 1)).is_err());
        assert_eq!(
            template_path("/Templates/Day/"),
            Some(PathBuf::from("Templates/Day"))
        );
        assert_eq!(template_path("../Day"), None);
        assert_eq!(template_path(""), None);
    }
}
