use std::path::PathBuf;

use focus_i18n::{Arg, t as tr, t_args};
use focus_kit::{ActiveTokens as _, Button, ButtonKind, Chip, Flow as _, Type, size};
use gpui::{
    AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, ScrollHandle, Styled, Subscription, Window, div,
    prelude::FluentBuilder,
};
use gpui_component::input::{InputEvent, InputState};

use crate::dialog::{
    TransferEvent, TransferServices, enter_confirms, escape_closes, footer_row, modal, notice,
    path_row, picker, scroll_keys, section, single_line,
};
use crate::import::{self, Imported, Preview, SkipReason, Source};

const SHOWN_SKIPS: usize = 6;

enum Stage {
    Form,
    Running,
    Done(Imported),
    Failed(String),
}

pub struct ImportDialog {
    services: TransferServices,
    source: Entity<InputState>,
    destination: Entity<InputState>,
    preview: Option<Result<Preview, String>>,
    previewing: bool,
    generation: u64,
    stage: Stage,
    focus: FocusHandle,
    body_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TransferEvent> for ImportDialog {}

impl Focusable for ImportDialog {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.source.read(cx).focus_handle(cx)
    }
}

fn source_label(source: Source) -> String {
    tr(match source {
        Source::Files => "import-source-files",
        Source::Folder => "import-source-folder",
        Source::Zip => "import-source-zip",
        Source::Notion => "import-source-notion",
    })
}

fn skip_text(reason: &SkipReason) -> String {
    match reason {
        SkipReason::Hidden => tr("import-skip-hidden"),
        SkipReason::Symlink => tr("import-skip-symlink"),
        SkipReason::UnsafePath => tr("import-skip-unsafe"),
        SkipReason::Unsupported => tr("import-skip-unsupported"),
        SkipReason::Special => tr("import-skip-special"),
        SkipReason::TooLarge => tr("import-skip-large"),
        SkipReason::TooDeep => tr("import-skip-deep"),
        SkipReason::Other(text) => text.clone(),
    }
}

impl ImportDialog {
    pub fn new(
        services: TransferServices,
        destination: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let source = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("/path/to/notes.zip")
                .validate(single_line)
        });
        let destination = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(destination.to_string())
                .validate(single_line)
        });
        let subscriptions = [&source, &destination]
            .map(|input| {
                cx.subscribe_in(input, window, |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.refresh_preview(window, cx);
                    }
                })
            })
            .into();
        ImportDialog {
            services,
            source,
            destination,
            preview: None,
            previewing: false,
            generation: 0,
            stage: Stage::Form,
            focus: cx.focus_handle(),
            body_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.stage {
            Stage::Form if self.can_import(cx) => self.start(window, cx),
            Stage::Done(_) => {
                self.generation += 1;
                cx.emit(TransferEvent::Close);
            }
            Stage::Failed(_) => {
                self.stage = Stage::Form;
                cx.notify();
            }
            _ => {}
        }
    }

    pub fn set_source(&mut self, path: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.source.update(cx, |input, cx| {
            input.set_value(path.to_string(), window, cx)
        });
    }

    pub fn is_busy(&self) -> bool {
        self.previewing || matches!(self.stage, Stage::Running)
    }

    fn refresh_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let text = self.source.read(cx).value().trim().to_string();
        self.preview = None;
        self.previewing = false;
        if text.is_empty() {
            cx.notify();
            return;
        }
        self.previewing = true;
        let path = PathBuf::from(text);
        let root = self.services.vault.root().to_path_buf();
        let destination = PathBuf::from(self.destination.read(cx).value().trim());
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    import::preview(&root, &path, &destination).map_err(|e| format!("{e:#}"))
                })
                .await;
            this.update_in(cx, |this, _, cx| {
                if generation == this.generation {
                    this.previewing = false;
                    this.preview = Some(result);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn can_import(&self, cx: &gpui::App) -> bool {
        let destination = self.destination.read(cx).value().to_string();
        matches!(&self.preview, Some(Ok(p)) if p.notes + p.attachments > 0)
            && !destination.contains("..")
            && !destination.starts_with('/')
            && !self.is_busy()
    }

    pub fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Ok(preview)) = &self.preview else {
            return;
        };
        if !self.can_import(cx) {
            return;
        }
        let source = preview.source;
        let input = PathBuf::from(self.source.read(cx).value().trim());
        let destination = PathBuf::from(self.destination.read(cx).value().trim());
        let root = self.services.vault.root().to_path_buf();
        self.stage = Stage::Running;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { import::run(&root, source, &[input], &destination) })
                .await;
            this.update_in(cx, |this, _, cx| {
                this.stage = match result {
                    Ok(imported) => {
                        cx.emit(TransferEvent::VaultChanged);
                        Stage::Done(imported)
                    }
                    Err(error) => Stage::Failed(format!("{error:#}")),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(picker(true, true));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
            {
                this.update_in(cx, |this, window, cx| {
                    this.set_source(&path.display().to_string(), window, cx)
                })
                .ok();
            }
        })
        .detach();
    }

    fn form(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        let t = *cx.tokens();
        let weak = cx.weak_entity();
        let mut preview_block = div().flex().flex_col().gap(size::S2);
        match (&self.preview, self.previewing) {
            (_, true) => {
                preview_block = preview_block.child(
                    div()
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(tr("import-reading")),
                )
            }
            (Some(Err(_)), _) => {
                preview_block = preview_block.child(notice(&t, t.danger, tr("import-unknown")))
            }
            (Some(Ok(p)), _) => {
                preview_block = preview_block
                    .child(div().ui_sm().text_color(t.ink_2).child(t_args(
                        "import-detected",
                        &[("kind", Arg::Str(&source_label(p.source)))],
                    )))
                    .child(
                        div()
                            .flow_row()
                            .flex_wrap()
                            .gap(size::S2)
                            .child(
                                Chip::new()
                                    .key(tr("import-count-notes"))
                                    .value(focus_i18n::format_number(p.notes as i64)),
                            )
                            .child(
                                Chip::new()
                                    .key(tr("import-count-attachments"))
                                    .value(focus_i18n::format_number(p.attachments as i64)),
                            )
                            .child(
                                Chip::new()
                                    .key(tr("import-count-skipped"))
                                    .value(focus_i18n::format_number(p.skipped.len() as i64)),
                            ),
                    )
                    .children(
                        p.samples
                            .iter()
                            .map(|name| div().mono_sm().text_color(t.ink_2).child(name.clone())),
                    )
                    .children(p.skipped.iter().take(SHOWN_SKIPS).map(|(name, why)| {
                        notice(&t, t.warn, format!("{name}: {}", skip_text(why)))
                    }));
            }
            (None, false) => {}
        }
        div()
            .flex()
            .flex_col()
            .gap(size::S3)
            .child(section(
                &t,
                tr("import-source"),
                path_row(
                    &self.source,
                    "import-choose",
                    tr("import-choose"),
                    move |_, window, cx| {
                        weak.update(cx, |this, cx| this.choose(window, cx)).ok();
                    },
                ),
            ))
            .child(section(&t, tr("import-preview"), preview_block))
            .child(section(
                &t,
                tr("import-destination"),
                div()
                    .flex()
                    .flex_col()
                    .gap(size::S1)
                    .child(focus_kit::Field::new(&self.destination))
                    .child(
                        div()
                            .ui_sm()
                            .text_color(t.ink_3)
                            .child(tr("import-destination-hint")),
                    ),
            ))
    }
}

impl Render for ImportDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let close = |this: &mut Self, cx: &mut Context<Self>| {
            this.generation += 1;
            cx.emit(TransferEvent::Close);
        };
        let (body, footer) = match &self.stage {
            Stage::Form => {
                let footer = footer_row()
                    .child(
                        Button::new("import-cancel", tr("transfer-cancel"))
                            .on_click(cx.listener(move |this, _, _, cx| close(this, cx))),
                    )
                    .child(
                        Button::new("import-start", tr("import-start"))
                            .kind(ButtonKind::Primary)
                            .disabled(!self.can_import(cx))
                            .on_click(cx.listener(|this, _, window, cx| this.start(window, cx))),
                    );
                (self.form(cx), footer)
            }
            Stage::Running => (
                div().ui().text_color(t.ink_2).child(tr("import-running")),
                footer_row(),
            ),
            Stage::Done(imported) => {
                let body =
                    div()
                        .flex()
                        .flex_col()
                        .gap(size::S3)
                        .child(notice(
                            &t,
                            t.ok,
                            t_args(
                                "import-done",
                                &[("count", Arg::Int(imported.notes.len() as i64))],
                            ),
                        ))
                        .child(
                            div()
                                .flow_row()
                                .flex_wrap()
                                .gap(size::S2)
                                .child(Chip::new().key(tr("import-count-attachments")).value(
                                    focus_i18n::format_number(imported.attachments.len() as i64),
                                ))
                                .child(Chip::new().key(tr("import-count-skipped")).value(
                                    focus_i18n::format_number(imported.skipped.len() as i64),
                                )),
                        )
                        .when(!imported.skipped.is_empty(), |d| {
                            d.child(
                                div()
                                    .ui_sm()
                                    .text_color(t.ink_2)
                                    .child(tr("import-skipped-title")),
                            )
                            .children(
                                imported
                                    .skipped
                                    .iter()
                                    .take(SHOWN_SKIPS)
                                    .map(|(path, why)| {
                                        notice(
                                            &t,
                                            t.warn,
                                            format!("{}: {}", path.display(), skip_text(why)),
                                        )
                                    }),
                            )
                        });
                (
                    body,
                    footer_row().child(
                        Button::new("import-close", tr("transfer-close"))
                            .kind(ButtonKind::Primary)
                            .on_click(cx.listener(move |this, _, _, cx| close(this, cx))),
                    ),
                )
            }
            Stage::Failed(detail) => {
                let body = focus_kit::ErrorCard::new(t_args(
                    "import-failed",
                    &[("detail", Arg::Str(detail))],
                ));
                let footer = footer_row()
                    .child(
                        Button::new("import-close", tr("transfer-close"))
                            .on_click(cx.listener(move |this, _, _, cx| close(this, cx))),
                    )
                    .child(
                        Button::new("import-back", tr("export-back"))
                            .kind(ButtonKind::Primary)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stage = Stage::Form;
                                cx.notify();
                            })),
                    );
                (div().child(body), footer)
            }
        };
        div()
            .id("import-dialog")
            .key_context("ImportDialog")
            .track_focus(&self.focus)
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(move |this, event, window, cx| {
                if escape_closes(event) && !matches!(this.stage, Stage::Running) {
                    close(this, cx);
                } else if enter_confirms(event) {
                    this.confirm(window, cx);
                } else if scroll_keys(event, &this.body_scroll) {
                    cx.notify();
                }
            }))
            .child(modal(
                &t,
                tr("import-title"),
                &self.body_scroll,
                cx.listener(move |this, _, _, cx| close(this, cx)),
                body,
                footer,
                cx,
            ))
    }
}
