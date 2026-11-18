use std::path::PathBuf;

use focus_core::{NoteId, Vault};
use focus_i18n::{Arg, t as tr, t_args};
use focus_kit::{
    ActiveTokens as _, Button, ButtonKind, Checkbox, Chip, Flow as _, Segment, Segments, Type, size,
};
use gpui::{
    AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, ScrollHandle, SharedString, Styled, Subscription, Window,
    div, prelude::FluentBuilder, px, relative,
};
use gpui_component::input::InputState;

use crate::dialog::{
    TransferEvent, TransferServices, escape_closes, focus_on_open, footer_row, modal, notice,
    path_row, picker, scroll_keys, section, single_line,
};
use crate::export::{self, Cancel, ExportError, Format, Options, Progress, Report, Request, Scope};

enum Stage {
    Form,
    Running {
        done: usize,
        total: usize,
        packing: bool,
    },
    Done(Report),
    Failed(Failure),
}

enum Failure {
    PdfUnavailable,
    Cancelled,
    Other(String),
}

pub struct ExportDialog {
    services: TransferServices,
    scopes: Vec<(Scope, SharedString)>,
    scope: usize,
    format: Format,
    options: Options,
    destination: Entity<InputState>,
    pdf_available: bool,
    stage: Stage,
    cancel: Option<Cancel>,
    focus: FocusHandle,
    body_scroll: ScrollHandle,
    _subscription: Subscription,
}

impl EventEmitter<TransferEvent> for ExportDialog {}

impl Focusable for ExportDialog {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

fn offered_scopes(
    vault: &Vault,
    note: Option<NoteId>,
    folder: Option<PathBuf>,
) -> Vec<(Scope, SharedString)> {
    let meta = note.and_then(|id| vault.note(id));
    let mut scopes = Vec::new();
    if let (Some(id), Some(meta)) = (note, &meta) {
        scopes.push((Scope::Note(id), SharedString::from(meta.title.to_string())));
    }
    let folder = folder.or_else(|| {
        meta.as_ref()
            .and_then(|m| m.path.parent().map(PathBuf::from))
    });
    if let Some(folder) = folder.filter(|f| !f.as_os_str().is_empty()) {
        scopes.push((
            Scope::Folder(folder.clone()),
            SharedString::from(folder.display().to_string()),
        ));
    }
    scopes.push((Scope::Vault, SharedString::from(vault.name())));
    scopes
}

impl ExportDialog {
    pub fn new(
        services: TransferServices,
        note: Option<NoteId>,
        folder: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let scopes = offered_scopes(&services.vault, note, folder);
        let destination = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(services.default_directory.display().to_string())
                .validate(single_line)
        });
        let dialog = ExportDialog {
            services,
            scopes,
            scope: 0,
            format: Format::Html,
            options: Options::default(),
            destination,
            pdf_available: export::pdf_available(),
            stage: Stage::Form,
            cancel: None,
            focus: cx.focus_handle(),
            body_scroll: ScrollHandle::new(),
            _subscription: cx.observe_global::<focus_kit::tokens::FocusTheme>(|_, cx| cx.notify()),
        };
        focus_on_open(&dialog.focus, window);
        dialog
    }

    pub fn with_options(mut self, options: Options) -> Self {
        self.options = options;
        self
    }

    pub fn set_pdf_available(&mut self, available: bool, cx: &mut Context<Self>) {
        self.pdf_available = available;
        cx.notify();
    }

    pub fn set_format(&mut self, format: Format, cx: &mut Context<Self>) {
        self.format = format;
        cx.notify();
    }

    pub fn set_scope(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.scopes.len() {
            self.scope = index;
            if self.format == Format::Pdf && !matches!(self.scopes[index].0, Scope::Note(_)) {
                self.format = Format::Html;
            }
            cx.notify();
        }
    }

    pub fn set_destination(&mut self, path: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.destination.update(cx, |input, cx| {
            input.set_value(path.to_string(), window, cx)
        });
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.stage, Stage::Running { .. })
    }

    pub fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() || (self.format == Format::Pdf && !self.pdf_available) {
            return;
        }
        let request = Request {
            scope: self.scopes[self.scope].0.clone(),
            format: self.format,
            options: self.options,
            destination: PathBuf::from(self.destination.read(cx).value().to_string()),
            language: focus_i18n::language(),
        };
        let cancel = Cancel::new();
        self.cancel = Some(cancel.clone());
        self.stage = Stage::Running {
            done: 0,
            total: 0,
            packing: false,
        };
        let vault = self.services.vault.clone();
        let (tx, rx) = smol::channel::unbounded();
        let worker = cx.background_spawn(async move {
            export::run(&vault, &request, &cancel, &mut |p| {
                let _ = tx.send_blocking(p);
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(progress) = rx.recv().await {
                let alive = this.update_in(cx, |this, _, cx| {
                    this.stage = match progress {
                        Progress::Notes { done, total } => Stage::Running {
                            done,
                            total,
                            packing: false,
                        },
                        Progress::Packing => Stage::Running {
                            done: 0,
                            total: 0,
                            packing: true,
                        },
                    };
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
            let result = worker.await;
            this.update_in(cx, |this, _, cx| {
                this.cancel = None;
                this.stage = match result {
                    Ok(report) => Stage::Done(report),
                    Err(ExportError::PdfUnavailable) => Stage::Failed(Failure::PdfUnavailable),
                    Err(ExportError::Cancelled) => Stage::Failed(Failure::Cancelled),
                    Err(ExportError::Other(error)) => {
                        Stage::Failed(Failure::Other(format!("{error:#}")))
                    }
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
        cx.emit(TransferEvent::Close);
    }

    fn choose_destination(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(picker(false, true));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
            {
                this.update_in(cx, |this, window, cx| {
                    this.set_destination(&path.display().to_string(), window, cx)
                })
                .ok();
            }
        })
        .detach();
    }

    fn form(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        let t = *cx.tokens();
        let scope_is_note = matches!(self.scopes[self.scope].0, Scope::Note(_));
        let scope_labels: Vec<Segment> = self
            .scopes
            .iter()
            .map(|(scope, _)| {
                Segment::new(tr(match scope {
                    Scope::Note(_) => "export-scope-note",
                    Scope::Folder(_) => "export-scope-folder",
                    Scope::Vault => "export-scope-vault",
                }))
            })
            .collect();
        let formats = [Format::Html, Format::Markdown, Format::Pdf, Format::Zip];
        let format_labels = vec![
            Segment::new(tr("export-format-html")),
            Segment::new(tr("export-format-markdown")),
            Segment::new(tr("export-format-pdf")),
            Segment::new(tr("export-format-zip")),
        ];
        let selected_format = formats.iter().position(|f| *f == self.format).unwrap_or(0);
        let weak = cx.weak_entity();
        let weak_format = weak.clone();
        let options = self.options;
        let toggle = |id: &'static str, label: String, checked: bool, on: fn(&mut Options)| {
            let weak = weak.clone();
            Checkbox::new(id, checked)
                .label(label)
                .on_click(move |_, _, cx| {
                    weak.update(cx, |this, cx| {
                        on(&mut this.options);
                        cx.notify();
                    })
                    .ok();
                })
        };
        let theme_segments = Segments::new(
            "export-theme",
            vec![
                Segment::new(tr("export-theme-dark")),
                Segment::new(tr("export-theme-light")),
            ],
            if options.dark { 0 } else { 1 },
        )
        .on_select({
            let weak = weak.clone();
            move |index, _, cx| {
                weak.update(cx, |this, cx| {
                    this.options.dark = index == 0;
                    cx.notify();
                })
                .ok();
            }
        });
        let code_segments = Segments::new(
            "export-code",
            vec![
                Segment::new(tr("export-code-soft")),
                Segment::new(tr("export-code-mono")),
            ],
            if options.soft_code_colors { 0 } else { 1 },
        )
        .on_select({
            let weak = weak.clone();
            move |index, _, cx| {
                weak.update(cx, |this, cx| {
                    this.options.soft_code_colors = index == 0;
                    cx.notify();
                })
                .ok();
            }
        });
        let showing_pdf_message = self.format == Format::Pdf && !self.pdf_available;
        let weak_scope = weak.clone();
        let weak_choose = weak.clone();
        div()
            .flex()
            .flex_col()
            .gap(size::S3)
            .child(section(
                &t,
                tr("export-scope"),
                div()
                    .flex()
                    .flex_col()
                    .gap(size::S1)
                    .child(
                        Segments::new("export-scope-choice", scope_labels, self.scope).on_select(
                            move |index, _, cx| {
                                weak_scope
                                    .update(cx, |this, cx| this.set_scope(index, cx))
                                    .ok();
                            },
                        ),
                    )
                    .child(
                        div()
                            .ui_sm()
                            .text_color(t.ink_2)
                            .child(self.scopes[self.scope].1.clone()),
                    ),
            ))
            .child(section(
                &t,
                tr("export-format"),
                Segments::new("export-format-choice", format_labels, selected_format)
                    .disabled(false)
                    .on_select(move |index, _, cx| {
                        weak_format
                            .update(cx, |this, cx| {
                                let format = formats[index];
                                if format != Format::Pdf
                                    || matches!(this.scopes[this.scope].0, Scope::Note(_))
                                {
                                    this.set_format(format, cx);
                                }
                            })
                            .ok();
                    }),
            ))
            .when(!scope_is_note, |d| {
                d.child(
                    div()
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(tr("export-pdf-one-note")),
                )
            })
            .when(showing_pdf_message, |d| {
                let weak = weak.clone();
                d.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(size::S2)
                        .child(notice(&t, t.warn, tr("export-pdf-unavailable")))
                        .child(div().flow_row().child(
                            Button::new("export-use-html", tr("export-use-html")).on_click(
                                move |_, _, cx| {
                                    weak.update(cx, |this, cx| this.set_format(Format::Html, cx))
                                        .ok();
                                },
                            ),
                        )),
                )
            })
            .when(matches!(self.format, Format::Html | Format::Pdf), |d| {
                d.child(section(
                    &t,
                    tr("export-options"),
                    div()
                        .flex()
                        .flex_col()
                        .gap(size::S2)
                        .child(
                            div()
                                .flow_row()
                                .items_center()
                                .gap(size::S3)
                                .child(div().ui().text_color(t.ink_2).child(tr("export-theme")))
                                .child(theme_segments),
                        )
                        .child(
                            div()
                                .flow_row()
                                .items_center()
                                .gap(size::S3)
                                .child(
                                    div()
                                        .ui()
                                        .text_color(t.ink_2)
                                        .child(tr("export-code-colors")),
                                )
                                .child(code_segments),
                        )
                        .when(self.format == Format::Html, |d| {
                            d.child(toggle(
                                "export-images",
                                tr("export-embed-images"),
                                options.embed_images,
                                |o| o.embed_images = !o.embed_images,
                            ))
                            .child(toggle(
                                "export-fonts",
                                tr("export-embed-fonts"),
                                options.embed_fonts,
                                |o| o.embed_fonts = !o.embed_fonts,
                            ))
                        })
                        .child(toggle(
                            "export-mermaid",
                            tr("export-mermaid"),
                            options.render_mermaid,
                            |o| o.render_mermaid = !o.render_mermaid,
                        )),
                ))
            })
            .child(section(
                &t,
                tr("export-destination"),
                path_row(
                    &self.destination,
                    "export-choose",
                    tr("export-choose"),
                    move |_, window, cx| {
                        weak_choose
                            .update(cx, |this, cx| this.choose_destination(window, cx))
                            .ok();
                    },
                ),
            ))
    }

    fn report(&self, report: &Report, t: &focus_kit::Tokens, cx: &mut Context<Self>) -> gpui::Div {
        let folder = report.folder.clone();
        let weak = cx.weak_entity();
        div()
            .flex()
            .flex_col()
            .gap(size::S3)
            .child(notice(
                t,
                t.ok,
                t_args(
                    "export-done",
                    &[("path", Arg::Str(&report.folder.display().to_string()))],
                ),
            ))
            .child(
                div()
                    .flow_row()
                    .flex_wrap()
                    .gap(size::S2)
                    .child(
                        Chip::new()
                            .key(tr("export-count-notes"))
                            .value(focus_i18n::format_number(report.notes as i64)),
                    )
                    .child(
                        Chip::new()
                            .key(tr("export-count-files"))
                            .value(focus_i18n::format_number(report.files as i64)),
                    )
                    .child(
                        Chip::new()
                            .key(tr("export-count-size"))
                            .value(focus_i18n::format_bytes(report.bytes)),
                    ),
            )
            .when(!report.skipped.is_empty(), |d| {
                d.child(
                    div()
                        .ui_sm()
                        .text_color(t.ink_2)
                        .child(tr("export-skipped")),
                )
                .children(report.skipped.iter().map(|(title, why)| {
                    notice(
                        t,
                        t.warn,
                        format!(
                            "{title}: {}",
                            if why == export::OUTSIDE_VAULT {
                                tr("export-skip-outside")
                            } else {
                                why.clone()
                            }
                        ),
                    )
                }))
            })
            .child(div().flow_row().child(
                Button::new("export-open-folder", tr("export-open-folder")).on_click(
                    move |_, _, cx| {
                        weak.update(cx, |_, cx| {
                            cx.emit(TransferEvent::RevealFolder(folder.clone()))
                        })
                        .ok();
                    },
                ),
            ))
    }
}

impl Render for ExportDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let weak = cx.weak_entity();
        let (body, footer) = match &self.stage {
            Stage::Form => {
                let blocked = self.format == Format::Pdf && !self.pdf_available;
                let footer = footer_row()
                    .child(
                        Button::new("export-cancel", tr("transfer-cancel"))
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    )
                    .child(
                        Button::new("export-start", tr("export-start"))
                            .kind(ButtonKind::Primary)
                            .disabled(blocked)
                            .on_click(cx.listener(|this, _, window, cx| this.start(window, cx))),
                    );
                (self.form(cx), footer)
            }
            Stage::Running {
                done,
                total,
                packing,
            } => {
                let fraction = if *total == 0 {
                    0.0
                } else {
                    *done as f32 / *total as f32
                };
                let text = if *packing {
                    tr("export-running-packing")
                } else {
                    t_args(
                        "export-running-notes",
                        &[
                            ("done", Arg::Int(*done as i64)),
                            ("total", Arg::Int(*total as i64)),
                        ],
                    )
                };
                let body = div()
                    .flex()
                    .flex_col()
                    .gap(size::S2)
                    .child(div().ui().text_color(t.ink_2).child(text))
                    .child(
                        div()
                            .h(px(2.))
                            .w_full()
                            .bg(t.line)
                            .child(div().h_full().w(relative(fraction)).bg(t.ink)),
                    );
                let footer =
                    footer_row().child(Button::new("export-stop", tr("transfer-cancel")).on_click(
                        cx.listener(|this, _, _, _| {
                            if let Some(cancel) = &this.cancel {
                                cancel.cancel();
                            }
                        }),
                    ));
                (body, footer)
            }
            Stage::Done(report) => {
                let body = self.report(report, &t, cx);
                let footer = footer_row().child(
                    Button::new("export-close", tr("transfer-close"))
                        .kind(ButtonKind::Primary)
                        .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                );
                (body, footer)
            }
            Stage::Failed(failure) => {
                let (bar, text) = match failure {
                    Failure::PdfUnavailable => (t.warn, tr("export-pdf-unavailable")),
                    Failure::Cancelled => (t.ink_3, tr("export-cancelled")),
                    Failure::Other(detail) => (
                        t.danger,
                        t_args("export-failed", &[("detail", Arg::Str(detail))]),
                    ),
                };
                let weak_back = weak.clone();
                let body = div()
                    .flex()
                    .flex_col()
                    .gap(size::S2)
                    .child(if bar == t.danger {
                        focus_kit::ErrorCard::new(text).into_any_element()
                    } else {
                        notice(&t, bar, text).into_any_element()
                    });
                let footer = footer_row()
                    .child(
                        Button::new("export-close", tr("transfer-close"))
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    )
                    .child(
                        Button::new("export-again", tr("export-back"))
                            .kind(ButtonKind::Primary)
                            .on_click(move |_, _, cx| {
                                weak_back
                                    .update(cx, |this, cx| {
                                        this.stage = Stage::Form;
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                    );
                (body, footer)
            }
        };
        div()
            .id("export-dialog")
            .key_context("ExportDialog")
            .track_focus(&self.focus)
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event, _, cx| {
                if escape_closes(event) {
                    this.close(cx);
                } else if scroll_keys(event, &this.body_scroll) {
                    cx.notify();
                }
            }))
            .child(modal(
                &t,
                tr("export-title"),
                &self.body_scroll,
                cx.listener(|this, _, _, cx| this.close(cx)),
                body,
                footer,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_of_the_note_is_offered_unless_it_is_the_root() {
        let (vault, _) = crate::export::scratch_vault(
            "offered-scopes",
            &[("Top.md", b"# Top\n"), ("sub/Deep.md", b"# Deep\n")],
        );
        let id = |title: &str| {
            vault
                .summaries()
                .iter()
                .find(|s| &*s.title == title)
                .map(|s| s.id)
                .expect("note")
        };
        let kinds = |scopes: Vec<(Scope, SharedString)>| {
            scopes.into_iter().map(|(s, _)| s).collect::<Vec<_>>()
        };
        assert_eq!(
            kinds(offered_scopes(&vault, Some(id("Deep")), None)),
            [
                Scope::Note(id("Deep")),
                Scope::Folder("sub".into()),
                Scope::Vault
            ]
        );
        assert_eq!(
            kinds(offered_scopes(&vault, Some(id("Top")), None)),
            [Scope::Note(id("Top")), Scope::Vault]
        );
        assert_eq!(
            kinds(offered_scopes(&vault, None, Some("sub".into()))),
            [Scope::Folder("sub".into()), Scope::Vault]
        );
    }
}
