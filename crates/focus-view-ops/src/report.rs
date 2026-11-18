use std::path::PathBuf;

use focus_kit::{ActiveTokens, Button, ButtonKind, Checkbox, Field, Icon, Type, size};
use focus_ops::report;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::input::InputState;

use crate::event::{NoticeTone, OpsEvent, ReportFormat};
use crate::state::{OpsState, error_text};
use crate::ui::{hatched_empty, screen_header, section_label, t_};

enum Phase {
    Idle,
    Building,
    Built,
    Writing,
    Written(PathBuf),
}

pub struct ReportView {
    state: Entity<OpsState>,
    include_journal: bool,
    include_credentials: bool,
    mask_secrets: bool,
    author: Entity<InputState>,
    generation: u64,
    markdown: Option<String>,
    view: Entity<focus_view_reading::ReadingView>,
    _view_events: gpui::Subscription,
    phase: Phase,
    sysreptor: Option<Result<(PathBuf, String), String>>,
}

impl EventEmitter<OpsEvent> for ReportView {}

impl ReportView {
    pub fn new(state: Entity<OpsState>, window: &mut Window, cx: &mut Context<Self>) -> ReportView {
        let (default_author, mask_secrets) = {
            let settings = &state.read(cx).settings;
            (settings.author.clone(), settings.mask_secrets)
        };
        let author = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-report-author"))
                .default_value(default_author)
        });
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let (view, view_events) = crate::rich::rendering_view(900., cx);
        ReportView {
            state,
            include_journal: true,
            include_credentials: false,
            mask_secrets,
            author,
            generation: focus_i18n::generation(),
            markdown: None,
            view,
            _view_events: view_events,
            phase: Phase::Idle,
            sysreptor: None,
        }
    }

    pub fn export_sysreptor(&mut self, cx: &mut Context<Self>) {
        use focus_sysreptor::{Options, Secrets};
        let Some(engagement) = self
            .state
            .read(cx)
            .active
            .as_ref()
            .map(|a| a.engagement.clone())
        else {
            return;
        };
        let secrets = if self.mask_secrets {
            Secrets::Masked
        } else {
            Secrets::Clear
        };
        let options = Options {
            secrets,
            credentials: self.include_credentials,
            author: self.author.read(cx).value().trim().to_string(),
            ..Options::default()
        };
        self.sysreptor = None;
        cx.spawn(async move |this, cx| {
            let written = cx
                .background_spawn(async move {
                    let archive = focus_sysreptor::export_project(&engagement, &options)
                        .map_err(|e| e.to_string())?;
                    let folder = engagement.dir.join("SysReptor");
                    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                    let path = folder.join(&archive.file_name);
                    focus_sysreptor::write_output(&path, &archive.bytes, secrets)
                        .map_err(|e| e.to_string())?;
                    Ok::<_, String>((path, archive.summary.to_text()))
                })
                .await;
            this.update(cx, |this, cx| {
                this.sysreptor = Some(written);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn build(&mut self, cx: &mut Context<Self>) {
        let Some(engagement) = self
            .state
            .read(cx)
            .active
            .as_ref()
            .map(|a| a.engagement.clone())
        else {
            return;
        };
        let options = report::Options {
            include_journal: self.include_journal,
            include_credentials: self.include_credentials,
            mask_secrets: self.mask_secrets,
            author: self.author.read(cx).value().trim().to_string(),
        };
        self.phase = Phase::Building;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let built = cx
                .background_spawn(async move { report::build(&engagement, &options) })
                .await;
            this.update(cx, |this, cx| {
                match built {
                    Ok(markdown) => {
                        crate::rich::set_text(&this.view, markdown.clone(), cx);
                        this.markdown = Some(markdown);
                        this.phase = Phase::Built;
                    }
                    Err(error) => {
                        this.phase = Phase::Idle;
                        cx.emit(OpsEvent::Notify {
                            tone: NoticeTone::Danger,
                            text: error_text(&error),
                        });
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn write(&mut self, cx: &mut Context<Self>) {
        let (Some(markdown), Some(dir)) = (
            self.markdown.clone(),
            self.state
                .read(cx)
                .active
                .as_ref()
                .map(|a| a.engagement.dir.clone()),
        ) else {
            return;
        };
        self.phase = Phase::Writing;
        let private = self.include_credentials && !self.mask_secrets;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let path = dir.join("Report.md");
            let target = path.clone();
            let written = cx
                .background_spawn(async move {
                    focus_ops::keep_version(&target)?;
                    let temporary = target.with_extension("md.tmp");
                    let write = || -> std::io::Result<()> {
                        use std::io::Write as _;
                        let _ = std::fs::remove_file(&temporary);
                        let mut options = std::fs::OpenOptions::new();
                        options.write(true).create_new(true);
                        #[cfg(unix)]
                        std::os::unix::fs::OpenOptionsExt::mode(
                            &mut options,
                            if private { 0o600 } else { 0o644 },
                        );
                        #[cfg(not(unix))]
                        let _ = private;
                        options.open(&temporary)?.write_all(markdown.as_bytes())?;
                        std::fs::rename(&temporary, &target)
                    };
                    write().map_err(|source| focus_ops::OpsError::Io {
                        path: target.clone(),
                        source,
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                match written {
                    Ok(()) => {
                        this.phase = Phase::Written(path);
                        this.state.update(cx, |state, cx| state.reload(cx));
                    }
                    Err(error) => {
                        this.phase = Phase::Built;
                        cx.emit(OpsEvent::Notify {
                            tone: NoticeTone::Danger,
                            text: error_text(&error),
                        });
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn export(&mut self, format: ReportFormat, cx: &mut Context<Self>) {
        if let Phase::Written(path) = &self.phase {
            cx.emit(OpsEvent::ExportReport {
                path: path.clone(),
                format,
            });
        }
    }

    pub fn has_preview(&self) -> bool {
        self.markdown.is_some()
    }

    pub fn is_written(&self) -> bool {
        matches!(self.phase, Phase::Written(_))
    }
}

impl Render for ReportView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        if self.generation != focus_i18n::generation() {
            self.generation = focus_i18n::generation();
            self.author.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-report-author"), window, cx)
            });
        }
        if self.state.read(cx).active.is_none() {
            return div()
                .id("ops-report")
                .size_full()
                .bg(t.bg)
                .flex()
                .child(hatched_empty(&t, t_("ops-no-engagement"), None));
        }
        let busy = matches!(self.phase, Phase::Building | Phase::Writing);
        let option = |id: &'static str,
                      checked: bool,
                      label: String,
                      cx: &mut Context<Self>,
                      toggle: fn(&mut ReportView)| {
            Checkbox::new(id, checked)
                .label(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    toggle(this);
                    cx.notify();
                }))
        };
        let warning = (self.include_credentials && !self.mask_secrets).then(|| {
            div()
                .ui_sm()
                .text_color(t.warn)
                .child(t_("ops-report-secrets-warning"))
        });
        let toolbar = div()
            .flex()
            .flex_col()
            .gap(size::S3)
            .child(
                crate::ui::row()
                    .flex_wrap()
                    .items_center()
                    .gap(size::S4)
                    .child(option(
                        "ops-report-journal",
                        self.include_journal,
                        t_("ops-report-include-journal"),
                        cx,
                        |this| this.include_journal = !this.include_journal,
                    ))
                    .child(option(
                        "ops-report-credentials",
                        self.include_credentials,
                        t_("ops-report-include-credentials"),
                        cx,
                        |this| this.include_credentials = !this.include_credentials,
                    ))
                    .child(option(
                        "ops-report-mask",
                        self.mask_secrets,
                        t_("ops-report-mask-secrets"),
                        cx,
                        |this| this.mask_secrets = !this.mask_secrets,
                    ))
                    .child(div().w(px(220.)).child(Field::new(&self.author))),
            )
            .children(warning)
            .child(
                crate::ui::row()
                    .items_center()
                    .gap(size::S2)
                    .child(
                        Button::new("ops-report-build", t_("ops-report-build"))
                            .kind(if self.markdown.is_none() {
                                ButtonKind::Primary
                            } else {
                                ButtonKind::Secondary
                            })
                            .icon(Icon::FILE_TEXT)
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| this.build(cx))),
                    )
                    .child(
                        Button::new("ops-report-write", t_("ops-report-write"))
                            .kind(ButtonKind::Primary)
                            .disabled(busy || self.markdown.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.write(cx))),
                    )
                    .child(
                        Button::new("ops-report-html", t_("ops-report-export-html"))
                            .disabled(!self.is_written())
                            .on_click(
                                cx.listener(|this, _, _, cx| this.export(ReportFormat::Html, cx)),
                            ),
                    )
                    .child(
                        Button::new("ops-report-pdf", t_("ops-report-export-pdf"))
                            .disabled(!self.is_written())
                            .on_click(
                                cx.listener(|this, _, _, cx| this.export(ReportFormat::Pdf, cx)),
                            ),
                    )
                    .child(
                        Button::new("ops-report-sysreptor", t_("ops-report-export-sysreptor"))
                            .icon(Icon::FILE_INPUT)
                            .on_click(cx.listener(|this, _, _, cx| this.export_sysreptor(cx))),
                    )
                    .children(match &self.phase {
                        Phase::Written(path) => {
                            Some(div().ui_sm().text_color(t.ok).child(t_args_written(path)))
                        }
                        _ => None,
                    }),
            );

        let preview: AnyElement = if self.markdown.is_none() {
            hatched_empty(
                &t,
                t_(if matches!(self.phase, Phase::Building) {
                    "ops-loading"
                } else {
                    "ops-report-empty"
                }),
                None,
            )
            .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap(size::S2)
                .child(section_label(&t, t_("ops-report-preview")))
                .child(
                    div()
                        .id("ops-report-preview")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .p(size::S3)
                        .rounded(size::RADIUS)
                        .border_1()
                        .border_color(t.line)
                        .bg(t.bg_2)
                        .child(self.view.clone()),
                )
                .into_any_element()
        };

        let exported = self.sysreptor.as_ref().map(|result| match result {
            Ok((path, summary)) => div()
                .flex()
                .flex_col()
                .gap(size::S1)
                .child(div().ui_sm().text_color(t.ok).child(focus_i18n::t_args(
                    "ops-sysreptor-written",
                    &[(
                        "path",
                        focus_i18n::Arg::from(path.display().to_string().as_str()),
                    )],
                )))
                .child(div().mono_sm().text_color(t.ink_2).child(summary.clone())),
            Err(message) => div().ui_sm().text_color(t.danger).child(focus_i18n::t_args(
                "ops-sysreptor-failed",
                &[("detail", focus_i18n::Arg::from(message.as_str()))],
            )),
        });

        div()
            .id("ops-report")
            .flex()
            .flex_col()
            .size_full()
            .gap(size::S4)
            .p(size::S6)
            .bg(t.bg)
            .child(screen_header(&t, t_("ops-section-report"), None))
            .child(toolbar)
            .children(exported)
            .child(preview)
    }
}

fn t_args_written(path: &std::path::Path) -> String {
    focus_i18n::t_args(
        "ops-report-written",
        &[(
            "file",
            focus_i18n::Arg::from(
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
                    .as_str(),
            ),
        )],
    )
}
