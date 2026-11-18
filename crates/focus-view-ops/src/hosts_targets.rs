use std::io::Read as _;

use focus_i18n::{Arg, t_args};
use focus_kit::{Button, ButtonKind, Field, Icon, Tokens, Type, size};
use focus_ops::targets::{self, Preview, Reason};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Styled, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::input::{Input, InputState};

use super::HostsTable;
use crate::state::error_text;
use crate::ui::{section_label, t_};

const SHOWN_ROWS: usize = 6;

pub(super) struct Inputs {
    pub address: Entity<InputState>,
    pub hostname: Entity<InputState>,
    pub os: Entity<InputState>,
    pub tags: Entity<InputState>,
    pub paste: Entity<InputState>,
}

impl Inputs {
    pub(super) fn new(window: &mut Window, cx: &mut Context<HostsTable>) -> Inputs {
        let line = |key: &str, window: &mut Window, cx: &mut Context<HostsTable>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .validate(crate::ui::single_line)
                    .placeholder(t_(key))
            })
        };
        Inputs {
            address: line("ops-targets-address", window, cx),
            hostname: line("ops-targets-hostname", window, cx),
            os: line("ops-targets-os", window, cx),
            tags: line("ops-targets-tags", window, cx),
            paste: cx.new(|cx| {
                InputState::new(window, cx)
                    .multi_line(true)
                    .auto_grow(5, 10)
                    .placeholder(t_("ops-targets-paste-placeholder"))
            }),
        }
    }
}

impl Inputs {
    pub(super) fn placeholders(&self) -> [(&Entity<InputState>, &'static str); 5] {
        [
            (&self.address, "ops-targets-address"),
            (&self.hostname, "ops-targets-hostname"),
            (&self.os, "ops-targets-os"),
            (&self.tags, "ops-targets-tags"),
            (&self.paste, "ops-targets-paste-placeholder"),
        ]
    }
}

pub(super) enum Message {
    Done(String),
    Failed(String),
}

fn reason_key(reason: Reason) -> &'static str {
    match reason {
        Reason::NotAnAddress => "ops-targets-reason-address",
        Reason::NotAName => "ops-targets-reason-name",
        Reason::ExtraText => "ops-targets-reason-extra",
        Reason::TooLong => "ops-targets-reason-long",
        Reason::Repeated => "ops-targets-reason-repeated",
        Reason::TooMany => "ops-targets-reason-many",
        Reason::NoAddress => "ops-targets-reason-none",
    }
}

impl HostsTable {
    pub(super) fn show_targets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.adding = !self.adding;
        self.target_preview = None;
        self.target_message = None;
        if self.adding {
            self.target_inputs
                .address
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn active_dir(&self, cx: &Context<Self>) -> Option<std::path::PathBuf> {
        self.state
            .read(cx)
            .active
            .as_ref()
            .map(|a| a.engagement.dir.clone())
    }

    fn add_manual(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let read = |input: &Entity<InputState>| input.read(cx).value().to_string();
        let inputs = &self.target_inputs;
        let parsed = targets::manual(
            &read(&inputs.address),
            &read(&inputs.hostname),
            &read(&inputs.os),
            &read(&inputs.tags),
        );
        let Some(dir) = self.active_dir(cx) else {
            return;
        };
        let preview = targets::preview(&dir, parsed);
        if let Some(refused) = preview.refused.first() {
            self.target_message = Some(Message::Failed(t_(reason_key(refused.reason))));
            cx.notify();
        } else if preview.create.is_empty() {
            self.target_message = Some(Message::Failed(t_("ops-targets-known")));
            cx.notify();
        } else {
            for input in [&inputs.address, &inputs.hostname, &inputs.os, &inputs.tags] {
                input.update(cx, |input, cx| input.set_value("", window, cx));
            }
            self.write_targets(preview.create, cx);
        }
    }

    pub(super) fn preview_pasted(&mut self, cx: &mut Context<Self>) {
        let text = self.target_inputs.paste.read(cx).value().to_string();
        self.preview_text(text, cx);
    }

    pub(super) fn preview_text(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(dir) = self.active_dir(cx) else {
            return;
        };
        self.busy = true;
        self.target_message = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    targets::parse(&text).map(|parsed| targets::preview(&dir, parsed))
                })
                .await;
            this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(preview) => this.target_preview = Some(preview),
                    Err(error) => this.target_message = Some(Message::Failed(error_text(&error))),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn choose_targets_file(&mut self, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(mut paths))) = chosen.await else {
                return;
            };
            let Some(path) = paths.pop() else { return };
            let read = cx
                .background_spawn(async move {
                    let mut bytes = Vec::new();
                    std::fs::File::open(&path)
                        .and_then(|file| {
                            file.take(targets::MAX_BYTES as u64 + 1)
                                .read_to_end(&mut bytes)
                        })
                        .map(|_| bytes)
                        .map_err(|source| focus_ops::OpsError::Io { path, source })
                })
                .await;
            this.update(cx, |this, cx| match read {
                Ok(bytes) => this.preview_text(String::from_utf8_lossy(&bytes).into_owned(), cx),
                Err(error) => {
                    this.target_message = Some(Message::Failed(error_text(&error)));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn confirm_targets(&mut self, cx: &mut Context<Self>) {
        if let Some(preview) = self.target_preview.take() {
            self.write_targets(preview.create, cx);
        }
    }

    fn write_targets(&mut self, create: Vec<targets::Target>, cx: &mut Context<Self>) {
        let Some(dir) = self.active_dir(cx) else {
            return;
        };
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { targets::create(&dir, &create) })
                .await;
            this.update(cx, |this, cx| {
                this.busy = false;
                this.target_message = Some(match result {
                    Ok(report) => Message::Done(t_args(
                        "ops-targets-created",
                        &[("count", Arg::from(report.created.len()))],
                    )),
                    Err(error) => Message::Failed(error_text(&error)),
                });
                this.state.update(cx, |state, cx| state.reload(cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn preview_block(&self, t: &Tokens, preview: &Preview, cx: &mut Context<Self>) -> AnyElement {
        let summary = t_args(
            "ops-targets-preview",
            &[
                ("create", Arg::from(preview.create.len())),
                ("known", Arg::from(preview.known.len())),
                ("refused", Arg::from(preview.refused.len())),
            ],
        );
        let line = |text: String, color| {
            div()
                .ui_sm()
                .text_color(color)
                .child(focus_kit::ellipsis(text))
        };
        let more = |total: usize| {
            (total > SHOWN_ROWS).then(|| {
                div().ui_xs().text_color(t.ink_3).child(t_args(
                    "ops-targets-more",
                    &[("count", Arg::from(total - SHOWN_ROWS))],
                ))
            })
        };
        div()
            .flex()
            .flex_col()
            .gap(size::S1)
            .child(div().ui_strong().text_color(t.ink).child(summary))
            .children(preview.create.iter().take(SHOWN_ROWS).map(|target| {
                line(
                    format!(
                        "{} {}",
                        target.address,
                        target.hostname.clone().unwrap_or_default()
                    ),
                    t.ink_2,
                )
                .mono_sm()
            }))
            .children(more(preview.create.len()))
            .when(!preview.known.is_empty(), |this| {
                this.child(section_label(t, t_("ops-targets-known-title")))
                    .children(
                        preview
                            .known
                            .iter()
                            .take(SHOWN_ROWS)
                            .map(|target| line(target.address.clone(), t.ink_3).mono_sm()),
                    )
                    .children(more(preview.known.len()))
            })
            .when(!preview.refused.is_empty(), |this| {
                this.child(section_label(t, t_("ops-targets-refused-title")))
                    .children(preview.refused.iter().take(SHOWN_ROWS).map(|r| {
                        line(
                            t_args(
                                "ops-targets-refused-line",
                                &[
                                    ("line", Arg::from(r.line)),
                                    ("text", Arg::from(r.text.as_str())),
                                    ("reason", Arg::from(t_(reason_key(r.reason)).as_str())),
                                ],
                            ),
                            t.warn,
                        )
                    }))
                    .children(more(preview.refused.len()))
            })
            .child(
                crate::ui::row().child(
                    Button::new(
                        "ops-targets-confirm",
                        t_args(
                            "ops-targets-confirm",
                            &[("count", Arg::from(preview.create.len()))],
                        ),
                    )
                    .kind(ButtonKind::Primary)
                    .disabled(self.busy || preview.create.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.confirm_targets(cx))),
                ),
            )
            .into_any_element()
    }

    pub(super) fn targets_panel(
        &self,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let inputs = &self.target_inputs;
        let has_text = !inputs.paste.read(cx).value().trim().is_empty();
        let message = self.target_message.as_ref().map(|message| match message {
            Message::Done(text) => div().ui_sm().text_color(t.ok).child(text.clone()),
            Message::Failed(text) => div().ui_sm().text_color(t.danger).child(text.clone()),
        });
        div()
            .id("ops-targets-panel")
            .flex()
            .flex_col()
            .gap(size::S2)
            .p(size::S3)
            .border_1()
            .border_color(t.line_2)
            .rounded(size::RADIUS)
            .child(section_label(t, t_("ops-targets-manual")))
            .child(
                crate::ui::row()
                    .flex_wrap()
                    .gap(size::S2)
                    .child(div().w(px(210.)).child(Field::new(&inputs.address)))
                    .child(div().w(px(150.)).child(Field::new(&inputs.hostname)))
                    .child(div().w(px(150.)).child(Field::new(&inputs.os)))
                    .child(div().w(px(210.)).child(Field::new(&inputs.tags)))
                    .child(
                        Button::new("ops-targets-add", t_("ops-targets-add"))
                            .icon(Icon::PLUS)
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.add_manual(window, cx)),
                            ),
                    ),
            )
            .child(section_label(t, t_("ops-targets-list")))
            .child(
                div()
                    .ui_xs()
                    .text_color(t.ink_3)
                    .child(t_("ops-targets-hint")),
            )
            .child(
                div()
                    .px(size::S2)
                    .py(size::S1)
                    .rounded(size::RADIUS)
                    .border_1()
                    .border_color(t.line_2)
                    .bg(t.bg_2)
                    .child(
                        Input::new(&inputs.paste)
                            .appearance(false)
                            .px_0()
                            .py_0()
                            .text_size(px(12.5))
                            .line_height(px(18.)),
                    ),
            )
            .child(
                crate::ui::row()
                    .gap(size::S2)
                    .child(
                        Button::new("ops-targets-preview", t_("ops-targets-preview-button"))
                            .disabled(self.busy || !has_text)
                            .on_click(cx.listener(|this, _, _, cx| this.preview_pasted(cx))),
                    )
                    .child(
                        Button::new("ops-targets-file", t_("ops-targets-file"))
                            .icon(Icon::FILE_INPUT)
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.choose_targets_file(cx))),
                    )
                    .child(Button::new("ops-targets-close", t_("ops-close")).on_click(
                        cx.listener(|this, _, window, cx| this.show_targets(window, cx)),
                    )),
            )
            .children(message)
            .children(
                self.target_preview
                    .as_ref()
                    .map(|preview| self.preview_block(t, preview, cx)),
            )
    }
}
