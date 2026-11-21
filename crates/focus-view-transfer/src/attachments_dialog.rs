use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;

use focus_core::NoteId;
use focus_i18n::{Arg, t as tr, t_args};
use focus_kit::{
    ActiveTokens as _, Button, ButtonKind, Checkbox, Chip, Flow as _, Icon, Type, ellipsis, icon,
    size,
};
use gpui::{
    AppContext as _, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ObjectFit, ParentElement, Render, ScrollHandle, StatefulInteractiveElement,
    Styled, StyledImage as _, UniformListScrollHandle, Window, div, img, prelude::FluentBuilder,
    px, uniform_list,
};

use crate::attachments::{self, ChangeKind, Orphan, TidyPreview, TrashReport};
use crate::dialog::{
    TransferEvent, TransferServices, escape_closes, focus_on_open, footer_row, modal, notice,
    scroll_keys,
};

const TIDY_ROW: gpui::Pixels = px(44.);
const ORPHAN_ROW: gpui::Pixels = px(48.);
const LIST_HEIGHT: gpui::Pixels = px(260.);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentsMode {
    Tidy(Option<NoteId>),
    Orphans,
}

enum Content {
    Loading,
    Tidy(TidyPreview),
    Orphans {
        list: Vec<Orphan>,
        selected: HashSet<PathBuf>,
        shown: Option<usize>,
    },
    Tidied {
        skipped: Vec<PathBuf>,
        links: usize,
    },
    Trashed(TrashReport),
    Failed(String),
}

pub struct AttachmentsDialog {
    services: TransferServices,
    mode: AttachmentsMode,
    content: Content,
    working: bool,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    body_scroll: ScrollHandle,
}

impl EventEmitter<TransferEvent> for AttachmentsDialog {}

impl Focusable for AttachmentsDialog {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

fn kind_label(kind: ChangeKind) -> String {
    tr(match kind {
        ChangeKind::Rename => "attachments-kind-rename",
        ChangeKind::Move => "attachments-kind-move",
        ChangeKind::Merge => "attachments-kind-merge",
    })
}

impl AttachmentsDialog {
    pub fn new(
        services: TransferServices,
        mode: AttachmentsMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let dialog = AttachmentsDialog {
            services,
            mode,
            content: Content::Loading,
            working: true,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            body_scroll: ScrollHandle::new(),
        };
        focus_on_open(&dialog.focus, window);
        dialog.load(window, cx);
        dialog
    }

    pub fn is_busy(&self) -> bool {
        self.working
    }

    fn load(&self, window: &mut Window, cx: &mut Context<Self>) {
        let vault = self.services.vault.clone();
        let policy = self.services.attachment_policy.clone();
        let mode = self.mode;
        cx.spawn_in(window, async move |this, cx| {
            let content = cx
                .background_spawn(async move {
                    match mode {
                        AttachmentsMode::Tidy(scope) => {
                            Content::Tidy(attachments::plan_tidy(&vault, scope, &policy))
                        }
                        AttachmentsMode::Orphans => {
                            let list = attachments::orphans(&vault);
                            Content::Orphans {
                                shown: (!list.is_empty()).then_some(0),
                                selected: HashSet::new(),
                                list,
                            }
                        }
                    }
                })
                .await;
            this.update_in(cx, |this, _, cx| {
                this.content = content;
                this.working = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.working {
            return;
        }
        let vault = self.services.vault.clone();
        let work: Box<dyn FnOnce() -> Content + Send> = match &self.content {
            Content::Tidy(preview) if !preview.changes.is_empty() => {
                let preview = preview.clone();
                Box::new(move || match attachments::apply_tidy(&vault, &preview) {
                    Ok(report) => Content::Tidied {
                        skipped: report.skipped,
                        links: report.updated_links,
                    },
                    Err(error) => Content::Failed(error.to_string()),
                })
            }
            Content::Orphans { selected, .. } if !selected.is_empty() => {
                let paths: Vec<PathBuf> = selected.iter().cloned().collect();
                Box::new(move || Content::Trashed(attachments::trash(&vault, &paths)))
            }
            _ => return,
        };
        self.working = true;
        cx.spawn_in(window, async move |this, cx| {
            let content = cx.background_spawn(async move { work() }).await;
            this.update_in(cx, |this, _, cx| {
                this.content = content;
                this.working = false;
                cx.emit(TransferEvent::VaultChanged);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn toggle(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        if let Content::Orphans { selected, .. } = &mut self.content
            && !selected.remove(path)
        {
            selected.insert(path.clone());
        }
        cx.notify();
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        if let Content::Orphans { list, selected, .. } = &mut self.content {
            if selected.len() == list.len() {
                selected.clear();
            } else {
                *selected = list.iter().map(|o| o.path.clone()).collect();
            }
        }
        cx.notify();
    }

    fn show(&mut self, row: usize, cx: &mut Context<Self>) {
        if let Content::Orphans { shown, .. } = &mut self.content {
            *shown = Some(row);
        }
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        cx.emit(TransferEvent::Close);
    }

    fn tidy_body(
        &self,
        preview: &TidyPreview,
        t: focus_kit::Tokens,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        if preview.changes.is_empty() && preview.link_updates == 0 {
            return div()
                .ui()
                .text_color(t.ink_3)
                .child(tr("attachments-tidy-empty"));
        }
        let chips = div()
            .flow_row()
            .flex_wrap()
            .gap(size::S2)
            .child(
                Chip::new()
                    .key(tr("attachments-count-renamed"))
                    .value(preview.count(ChangeKind::Rename).to_string()),
            )
            .child(
                Chip::new()
                    .key(tr("attachments-count-moved"))
                    .value(preview.count(ChangeKind::Move).to_string()),
            )
            .child(
                Chip::new()
                    .key(tr("attachments-count-merged"))
                    .value(preview.count(ChangeKind::Merge).to_string()),
            )
            .child(
                Chip::new()
                    .key(tr("attachments-count-links"))
                    .value(preview.link_updates.to_string()),
            );
        let rows = preview.changes.clone();
        let count = rows.len();
        div().flex().flex_col().gap(size::S3).child(chips).child(
            div().h(LIST_HEIGHT).border_1().border_color(t.line).child(
                uniform_list(
                    "tidy-rows",
                    count,
                    cx.processor(move |_, range: Range<usize>, _, _| {
                        range
                            .map(|i| {
                                let change = &rows[i];
                                let target = match change.kind {
                                    ChangeKind::Merge => t_args(
                                        "attachments-merge-keeps",
                                        &[("path", Arg::Str(&change.to.display().to_string()))],
                                    ),
                                    _ => change.to.display().to_string(),
                                };
                                div()
                                    .id(i)
                                    .flex()
                                    .flex_col()
                                    .justify_center()
                                    .w_full()
                                    .h(TIDY_ROW)
                                    .px(size::S3)
                                    .when(i > 0, |d| d.border_t_1().border_color(t.line))
                                    .child(
                                        div()
                                            .flow_row()
                                            .items_center()
                                            .gap(size::S2)
                                            .child(
                                                div()
                                                    .flex_none()
                                                    .w(px(64.))
                                                    .ui_xs()
                                                    .text_color(t.ink_3)
                                                    .child(kind_label(change.kind)),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .mono_sm()
                                                    .text_color(t.ink_3)
                                                    .child(ellipsis(
                                                        change.from.display().to_string(),
                                                    )),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flow_row()
                                            .pad_start(px(72.))
                                            .mono_sm()
                                            .text_color(t.ink)
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .child(ellipsis(target)),
                                            ),
                                    )
                            })
                            .collect()
                    }),
                )
                .size_full()
                .track_scroll(self.scroll.clone()),
            ),
        )
    }

    fn orphans_body(
        &self,
        list: &[Orphan],
        selected: &HashSet<PathBuf>,
        shown: Option<usize>,
        t: focus_kit::Tokens,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        if list.is_empty() {
            return div()
                .ui()
                .text_color(t.ink_3)
                .child(tr("attachments-orphans-empty"));
        }
        let rows: Vec<(Orphan, bool)> = list
            .iter()
            .map(|o| (o.clone(), selected.contains(&o.path)))
            .collect();
        let root = self.services.vault.root().to_path_buf();
        let count = rows.len();
        let weak = cx.weak_entity();
        let preview_orphan = shown.and_then(|i| list.get(i)).cloned();
        let all = selected.len() == list.len();
        div()
            .flex()
            .flex_col()
            .gap(size::S3)
            .child(
                div()
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(tr("attachments-orphans-hint")),
            )
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .gap(size::S3)
                    .child({
                        let weak = weak.clone();
                        Checkbox::new("orphans-all", all)
                            .label(tr("attachments-select-all"))
                            .on_click(move |_, _, cx| {
                                weak.update(cx, |this, cx| this.select_all(cx)).ok();
                            })
                    })
                    .child(
                        Chip::new()
                            .key(tr("attachments-selected"))
                            .value(selected.len().to_string()),
                    ),
            )
            .child(
                div().h(LIST_HEIGHT).border_1().border_color(t.line).child(
                    uniform_list(
                        "orphan-rows",
                        count,
                        cx.processor({
                            let root = root.clone();
                            move |_, range: Range<usize>, _, cx| {
                                range
                                    .map(|i| {
                                        let (orphan, checked) = &rows[i];
                                        let path = orphan.path.clone();
                                        let toggle_path = orphan.path.clone();
                                        let thumb = if orphan.is_image {
                                            img(root.join(&orphan.path))
                                                .size(px(36.))
                                                .object_fit(ObjectFit::Contain)
                                                .into_any_element()
                                        } else {
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .size(px(36.))
                                                .text_color(t.ink_3)
                                                .child(icon(Icon::FILE, size::ICON))
                                                .into_any_element()
                                        };
                                        let current = shown == Some(i);
                                        div()
                                            .flow_row()
                                            .id(("orphan", i))
                                            .items_center()
                                            .gap(size::S3)
                                            .w_full()
                                            .h(ORPHAN_ROW)
                                            .px(size::S3)
                                            .cursor_pointer()
                                            .when(i > 0, |d| d.border_t_1().border_color(t.line))
                                            .when(current, |d| d.bg(t.card_3))
                                            .when(!current, |d| d.hover(move |s| s.bg(t.card_2)))
                                            .on_click(
                                                cx.listener(move |this, _, _, cx| this.show(i, cx)),
                                            )
                                            .child(
                                                Checkbox::new(("orphan-check", i), *checked)
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            cx.stop_propagation();
                                                            this.toggle(&toggle_path, cx);
                                                        },
                                                    )),
                                            )
                                            .child(div().flex_none().child(thumb))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .mono_sm()
                                                    .text_color(t.ink_2)
                                                    .child(ellipsis(path.display().to_string())),
                                            )
                                            .child(
                                                div()
                                                    .flex_none()
                                                    .mono_sm()
                                                    .text_color(t.ink_3)
                                                    .child(focus_i18n::format_bytes(orphan.bytes)),
                                            )
                                    })
                                    .collect()
                            }
                        }),
                    )
                    .size_full(),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(180.))
                    .border_1()
                    .border_color(t.line)
                    .bg(t.bg_2)
                    .map_preview(preview_orphan, &root, t),
            )
    }
}

trait PreviewExt {
    fn map_preview(
        self,
        orphan: Option<Orphan>,
        root: &std::path::Path,
        t: focus_kit::Tokens,
    ) -> Self;
}

impl PreviewExt for gpui::Div {
    fn map_preview(
        self,
        orphan: Option<Orphan>,
        root: &std::path::Path,
        t: focus_kit::Tokens,
    ) -> Self {
        match orphan {
            Some(o) if o.is_image => self.child(
                img(root.join(&o.path))
                    .max_w(px(500.))
                    .max_h(px(172.))
                    .object_fit(ObjectFit::Contain),
            ),
            Some(_) => self.child(
                div()
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(tr("attachments-preview-none")),
            ),
            None => self,
        }
    }
}

impl Render for AttachmentsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let title = tr(match self.mode {
            AttachmentsMode::Tidy(_) => "attachments-tidy-title",
            AttachmentsMode::Orphans => "attachments-orphans-title",
        });
        let close_button = |label: &str, primary: bool, cx: &mut Context<Self>| {
            Button::new("attachments-close", label.to_string())
                .kind(if primary {
                    ButtonKind::Primary
                } else {
                    ButtonKind::Secondary
                })
                .on_click(cx.listener(|this, _, _, cx| this.close(cx)))
        };
        let (body, footer) = match &self.content {
            Content::Loading => (
                div()
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(tr("attachments-loading")),
                footer_row().child(close_button(&tr("transfer-close"), false, cx)),
            ),
            Content::Tidy(preview) => {
                let enabled = !preview.changes.is_empty() && !self.working;
                let footer = footer_row()
                    .child(close_button(&tr("transfer-cancel"), false, cx))
                    .child(
                        Button::new("attachments-apply", tr("attachments-apply"))
                            .kind(ButtonKind::Primary)
                            .disabled(!enabled)
                            .on_click(cx.listener(|this, _, window, cx| this.apply(window, cx))),
                    );
                (self.tidy_body(preview, t, cx), footer)
            }
            Content::Orphans {
                list,
                selected,
                shown,
            } => {
                let enabled = !selected.is_empty() && !self.working;
                let footer = footer_row()
                    .child(close_button(&tr("transfer-cancel"), false, cx))
                    .child(
                        Button::new("attachments-trash", tr("attachments-trash"))
                            .kind(ButtonKind::Danger)
                            .disabled(!enabled)
                            .on_click(cx.listener(|this, _, window, cx| this.apply(window, cx))),
                    );
                (self.orphans_body(list, selected, *shown, t, cx), footer)
            }
            Content::Tidied { skipped, links } => {
                let body = div()
                    .flex()
                    .flex_col()
                    .gap(size::S2)
                    .child(notice(&t, t.ok, tr("attachments-tidy-done")))
                    .child(
                        Chip::new()
                            .key(tr("attachments-count-links"))
                            .value(links.to_string()),
                    )
                    .when(!skipped.is_empty(), |d| {
                        d.child(div().ui_sm().text_color(t.ink_2).child(t_args(
                            "attachments-skipped",
                            &[("count", Arg::Int(skipped.len() as i64))],
                        )))
                        .children(
                            skipped
                                .iter()
                                .take(6)
                                .map(|p| notice(&t, t.warn, p.display().to_string())),
                        )
                    });
                (
                    body,
                    footer_row().child(close_button(&tr("transfer-close"), true, cx)),
                )
            }
            Content::Trashed(report) => {
                let body = div()
                    .flex()
                    .flex_col()
                    .gap(size::S2)
                    .child(notice(
                        &t,
                        t.ok,
                        t_args(
                            "attachments-trashed",
                            &[("count", Arg::Int(report.trashed.len() as i64))],
                        ),
                    ))
                    .when(!report.failed.is_empty(), |d| {
                        d.children(report.failed.iter().take(6).map(|(p, why)| {
                            notice(&t, t.danger, format!("{}: {why}", p.display()))
                        }))
                    });
                (
                    body,
                    footer_row().child(close_button(&tr("transfer-close"), true, cx)),
                )
            }
            Content::Failed(detail) => (
                div().child(focus_kit::ErrorCard::new(detail.clone())),
                footer_row().child(close_button(&tr("transfer-close"), true, cx)),
            ),
        };
        div()
            .id("attachments-dialog")
            .key_context("AttachmentsDialog")
            .track_focus(&self.focus)
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event, _, cx| {
                if escape_closes(event) && !this.working {
                    this.close(cx);
                } else {
                    let list = this.scroll.0.borrow().base_handle.clone();
                    if (matches!(this.content, Content::Tidy(_)) && scroll_keys(event, &list))
                        || scroll_keys(event, &this.body_scroll)
                    {
                        cx.notify();
                    }
                }
            }))
            .child(modal(
                &t,
                title,
                &self.body_scroll,
                cx.listener(|this, _, _, cx| this.close(cx)),
                body,
                footer,
                cx,
            ))
    }
}
