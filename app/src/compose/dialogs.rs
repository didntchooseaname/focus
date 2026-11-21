use std::path::PathBuf;
use std::sync::Arc;

use focus_core::{Placement, TrashKind, Vault};
use focus_kit::Flow as _;
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Field, Icon, IconButton, Tokens, Type, ellipsis, floating,
    size,
};
use gpui::{
    AnyElement, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Task, Window, div, prelude::FluentBuilder, px, relative,
};
use gpui_component::input::{InputEvent, InputState};

use crate::actions::Confirm;

const WIDTH: gpui::Pixels = px(520.);

pub enum DialogEvent {
    Close,
    RenameTag { from: String, to: String },
    Merge,
}

fn modal(
    t: &Tokens,
    id: &'static str,
    title: String,
    on_close: impl Fn(&mut Window, &mut gpui::App) + 'static,
    body: impl IntoElement,
    footer: impl IntoElement,
    cx: &gpui::App,
) -> AnyElement {
    let on_close = std::rc::Rc::new(on_close);
    let close = on_close.clone();
    let card = floating(t)
        .id(id)
        .occlude()
        .flex()
        .flex_col()
        .w(WIDTH)
        .max_w(relative(0.94))
        .child(
            div()
                .flow_row()
                .items_center()
                .gap(size::S2)
                .h(px(48.))
                .px(size::S4)
                .border_b_1()
                .border_color(t.line)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .display_sm()
                        .text_color(t.ink)
                        .child(ellipsis(title)),
                )
                .child(
                    IconButton::new(
                        "compose-dialog-close",
                        Icon::CLOSE,
                        focus_i18n::t("ui-action-close"),
                    )
                    .shortcut(focus_i18n::t("ui-key-escape"))
                    .on_click(move |_, window, cx| close(window, cx)),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(size::S3)
                .px(size::S4)
                .py(size::S4)
                .child(body),
        )
        .child(
            div()
                .flow_row()
                .items_center()
                .justify_end()
                .gap(size::S2)
                .h(px(56.))
                .px(size::S4)
                .border_t_1()
                .border_color(t.line)
                .child(footer),
        );
    let veil = div()
        .id("compose-veil")
        .absolute()
        .inset_0()
        .flow_row()
        .justify_center()
        .items_start()
        .pt(relative(0.16))
        .bg(t.veil)
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            on_close(window, cx)
        })
        .child(card);
    focus_kit::fade_in("fade-compose", veil, cx)
}

fn section(t: &Tokens, label: String, content: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(size::S1)
        .child(div().ui_xs().text_color(t.ink_3).child(label))
        .child(content)
}

fn notice(t: &Tokens, bar: gpui::Hsla, text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .relative()
        .flow_row()
        .items_start()
        .min_h(px(20.))
        .pad_start(size::S3)
        .ui_sm()
        .text_color(t.ink_2)
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(px(2.))
                .bg(bar)
                .start_at(px(0.)),
        )
        .child(div().flex_1().min_w_0().child(text.into()))
}

fn path_row(t: &Tokens, label: String, path: String) -> impl IntoElement {
    div()
        .flow_row()
        .items_center()
        .gap(size::S3)
        .h(px(24.))
        .child(
            div()
                .w(px(96.))
                .flex_none()
                .ui_sm()
                .text_color(t.ink_3)
                .child(focus_kit::truncated(
                    gpui::ElementId::Name(format!("path-label-{label}").into()),
                    label,
                )),
        )
        .child(
            div()
                .flex_shrink()
                .min_w_0()
                .mono()
                .text_color(t.ink)
                .child(ellipsis(path)),
        )
}

enum Figures {
    Counting,
    Ready { notes: usize, occurrences: usize },
    Failed(String),
}

pub struct TagRenameDialog {
    vault: Arc<Vault>,
    from: String,
    pub input: Entity<InputState>,
    figures: Figures,
    existing: bool,
    generation: u64,
    _preview: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DialogEvent> for TagRenameDialog {}

impl Focusable for TagRenameDialog {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl TagRenameDialog {
    pub fn new(
        vault: Arc<Vault>,
        from: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(from.clone()));
        let subscriptions = vec![cx.subscribe(
            &input,
            |this: &mut TagRenameDialog, _, event: &InputEvent, cx| match event {
                InputEvent::Change => this.preview(cx),
                InputEvent::PressEnter { .. } => this.confirm(cx),
                _ => {}
            },
        )];
        let mut dialog = TagRenameDialog {
            vault,
            from,
            input,
            figures: Figures::Counting,
            existing: false,
            generation: 0,
            _preview: None,
            _subscriptions: subscriptions,
        };
        dialog.preview(cx);
        dialog
    }

    fn to(&self, cx: &gpui::App) -> String {
        let typed = self.input.read(cx).value().trim().to_string();
        typed.strip_prefix('#').unwrap_or(&typed).to_string()
    }

    fn valid(&self, cx: &gpui::App) -> bool {
        focus_core::is_valid_tag(&self.to(cx))
    }

    fn preview(&mut self, cx: &mut Context<Self>) {
        let to = self.to(cx);
        self.generation += 1;
        let generation = self.generation;
        if !focus_core::is_valid_tag(&to) {
            self._preview = None;
            cx.notify();
            return;
        }
        let lower = to.to_lowercase();
        self.existing = lower != self.from.to_lowercase()
            && self
                .vault
                .tags()
                .iter()
                .any(|t| t.tag.to_lowercase() == lower);
        self.figures = Figures::Counting;
        let (vault, from) = (self.vault.clone(), self.from.clone());
        let work = cx.background_spawn(async move { vault.preview_rename_tag(&from, &to) });
        self._preview = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.figures = match result {
                    Ok(report) => Figures::Ready {
                        notes: report.notes.len(),
                        occurrences: report.occurrences,
                    },
                    Err(error) => Figures::Failed(crate::errors::vault(&error)),
                };
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn ready(&self, cx: &gpui::App) -> bool {
        self.valid(cx)
            && self.to(cx) != self.from
            && matches!(self.figures, Figures::Ready { notes, .. } if notes > 0)
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if self.ready(cx) {
            let (from, to) = (self.from.clone(), self.to(cx));
            cx.emit(DialogEvent::RenameTag { from, to });
        }
    }
}

impl Render for TagRenameDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let to = self.to(cx);
        let typed = !self.input.read(cx).value().trim().is_empty();
        let error = (typed && !focus_core::is_valid_tag(&to))
            .then(|| SharedString::from(focus_i18n::t("ui-tag-rename-invalid")));
        let figures = match &self.figures {
            _ if error.is_some() || !typed => None,
            Figures::Counting => Some((t.ink_3, focus_i18n::t("ui-tag-rename-counting"))),
            Figures::Ready { notes: 0, .. } => Some((t.ink_3, focus_i18n::t("ui-tag-rename-none"))),
            Figures::Ready { notes, occurrences } => Some((
                t.ink,
                focus_i18n::t_args(
                    "ui-tag-rename-figures",
                    &[
                        ("notes", (*notes as i64).into()),
                        ("occurrences", (*occurrences as i64).into()),
                    ],
                ),
            )),
            Figures::Failed(message) => Some((t.danger, message.clone())),
        };
        let ready = self.ready(cx);
        let body = div()
            .flex()
            .flex_col()
            .gap(size::S3)
            .child(section(
                &t,
                focus_i18n::t("ui-tag-rename-label"),
                Field::new(&self.input).icon(Icon::HASH).error(error),
            ))
            .children(figures.map(|(bar, text)| notice(&t, bar, text)))
            .when(self.existing && ready, |this| {
                this.child(notice(
                    &t,
                    t.ink_3,
                    focus_i18n::t_args("ui-tag-rename-existing", &[("tag", to.as_str().into())]),
                ))
            });
        let footer = div()
            .flow_row()
            .gap(size::S2)
            .child(
                Button::new("tag-rename-cancel", focus_i18n::t("ui-action-cancel"))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(DialogEvent::Close))),
            )
            .child(
                Button::new("tag-rename-confirm", focus_i18n::t("ui-action-rename"))
                    .kind(ButtonKind::Primary)
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.confirm(cx))),
            );
        let entity = cx.entity().downgrade();
        div()
            .absolute()
            .inset_0()
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.confirm(cx)))
            .child(modal(
                &t,
                "tag-rename-dialog",
                focus_i18n::t_args("ui-tag-rename-title", &[("tag", self.from.as_str().into())]),
                move |_, cx| {
                    let _ = entity.update(cx, |_, cx| cx.emit(DialogEvent::Close));
                },
                body,
                footer,
                cx,
            ))
    }
}

pub struct MergeDialog {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub placement: Placement,
    pub new_note: bool,
    pub trash: TrashKind,
    links: Option<Result<usize, String>>,
    focus: FocusHandle,
    default_button: FocusHandle,
    _count: Option<Task<()>>,
}

impl EventEmitter<DialogEvent> for MergeDialog {}

impl Focusable for MergeDialog {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.default_button.clone()
    }
}

impl MergeDialog {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: PathBuf,
        destination: PathBuf,
        placement: Placement,
        new_note: bool,
        trash: TrashKind,
        count: Task<Result<usize, String>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let task = cx.spawn(async move |this, cx| {
            let links = count.await;
            let _ = this.update(cx, |this, cx| {
                this.links = Some(links);
                cx.notify();
            });
        });
        MergeDialog {
            source,
            destination,
            placement,
            new_note,
            trash,
            links: None,
            focus: cx.focus_handle(),
            default_button: cx.focus_handle(),
            _count: Some(task),
        }
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if matches!(self.links, Some(Ok(_))) {
            cx.emit(DialogEvent::Merge);
        }
    }
}

fn stem(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl Render for MergeDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let (source, destination) = (stem(&self.source), stem(&self.destination));
        let names = [
            ("source", source.as_str().into()),
            ("destination", destination.as_str().into()),
        ];
        let what = focus_i18n::t_args(
            match (self.new_note, self.placement) {
                (true, _) => "ui-merge-what-new",
                (false, Placement::End) => "ui-merge-what-end",
                (false, Placement::Start) => "ui-merge-what-start",
            },
            &names,
        );
        let (bar, links) = match &self.links {
            None => (t.ink_3, focus_i18n::t("ui-merge-counting")),
            Some(Ok(links)) => (
                t.ink,
                focus_i18n::t_args(
                    "ui-merge-links",
                    &[
                        ("links", (*links as i64).into()),
                        ("destination", destination.as_str().into()),
                    ],
                ),
            ),
            Some(Err(message)) => (t.danger, message.clone()),
        };
        let body = div()
            .flex()
            .flex_col()
            .gap(size::S3)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(path_row(
                        &t,
                        focus_i18n::t("ui-merge-source"),
                        self.source.display().to_string(),
                    ))
                    .child(path_row(
                        &t,
                        focus_i18n::t("ui-merge-destination"),
                        self.destination.display().to_string(),
                    )),
            )
            .child(div().ui().text_color(t.ink_2).child(what))
            .child(notice(&t, bar, links))
            .when(!self.new_note, |this| {
                this.child(notice(
                    &t,
                    t.ink_3,
                    focus_i18n::t_args(
                        match self.trash {
                            TrashKind::Vault => "ui-merge-trash",
                            TrashKind::System => "ui-merge-trash-system",
                        },
                        &names,
                    ),
                ))
            });
        let ready = matches!(self.links, Some(Ok(_)));
        let footer = div()
            .flow_row()
            .gap(size::S2)
            .child(
                Button::new("merge-cancel", focus_i18n::t("ui-action-cancel"))
                    .when(!ready, |button| button.track_focus(&self.default_button))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(DialogEvent::Close))),
            )
            .child(
                Button::new("merge-confirm", focus_i18n::t("ui-action-merge"))
                    .kind(ButtonKind::Primary)
                    .disabled(!ready)
                    .track_focus(&self.default_button)
                    .on_click(cx.listener(|this, _, _, cx| this.confirm(cx))),
            );
        let entity = cx.entity().downgrade();
        div()
            .absolute()
            .inset_0()
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.confirm(cx)))
            .child(modal(
                &t,
                "merge-dialog",
                focus_i18n::t("ui-merge-title"),
                move |_, cx| {
                    let _ = entity.update(cx, |_, cx| cx.emit(DialogEvent::Close));
                },
                body,
                footer,
                cx,
            ))
    }
}
