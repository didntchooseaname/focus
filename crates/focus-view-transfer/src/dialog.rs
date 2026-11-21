use std::path::PathBuf;
use std::sync::Arc;

use focus_core::{AttachmentPolicy, Vault};
use focus_kit::{Button, Field, Flow as _, Icon, IconButton, Tokens, Type, floating, size};
use gpui::{
    AnyElement, App, ClickEvent, Div, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, PathPromptOptions, Pixels, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Window, div, point, px, relative,
};
use gpui_component::input::InputState;

pub const WIDTH: Pixels = px(560.);

#[derive(Clone)]
pub struct TransferServices {
    pub vault: Arc<Vault>,
    pub default_directory: PathBuf,
    pub attachment_policy: AttachmentPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEvent {
    Close,
    RevealFolder(PathBuf),
    VaultChanged,
}

pub(crate) fn footer_row() -> Div {
    div().flow_row().items_center().gap(size::S2).justify_end()
}

pub(crate) fn scroll_keys(event: &KeyDownEvent, scroll: &ScrollHandle) -> bool {
    let m = event.keystroke.modifiers;
    if m.alt || m.shift || m.platform {
        return false;
    }
    let (page, longest) = (
        scroll.bounds().size.height * 0.9,
        scroll.max_offset().height,
    );
    let mut offset = scroll.offset();
    match (event.keystroke.key.as_str(), m.control) {
        ("pagedown", false) => offset.y = (offset.y - page).max(-longest),
        ("pageup", false) => offset.y = (offset.y + page).min(px(0.)),
        ("home", _) => offset.y = px(0.),
        ("end", _) => offset.y = -longest,
        _ => return false,
    }
    scroll.set_offset(point(offset.x, offset.y));
    true
}

pub(crate) fn modal(
    t: &Tokens,
    title: String,
    scroll: &ScrollHandle,
    close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    body: impl IntoElement,
    footer: impl IntoElement,
    cx: &App,
) -> AnyElement {
    let card = floating(t)
        .flex()
        .flex_col()
        .w(WIDTH)
        .max_w(relative(0.94))
        .max_h(relative(0.88))
        .child(
            div()
                .flow_row()
                .flex_none()
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
                        .child(title),
                )
                .child(
                    IconButton::new("dialog-close", Icon::CLOSE, focus_i18n::t("transfer-close"))
                        .shortcut(focus_i18n::keys_text("escape"))
                        .on_click(close),
                ),
        )
        .child(
            div()
                .id("dialog-body")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap(size::S3)
                .px(size::S4)
                .py(size::S3)
                .overflow_y_scroll()
                .track_scroll(scroll)
                .child(body),
        )
        .child(
            div()
                .flex_none()
                .px(size::S4)
                .h(px(56.))
                .flex()
                .items_center()
                .border_t_1()
                .border_color(t.line)
                .child(div().flex_1().child(footer)),
        );
    focus_kit::fade_in(
        "fade-transfer",
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_start()
            .justify_center()
            .pt(relative(0.1))
            .bg(t.veil)
            .child(card),
        cx,
    )
}

pub(crate) fn section(t: &Tokens, label: String, content: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(size::S1)
        .child(div().ui_xs().text_color(t.ink_3).child(label))
        .child(content)
}

pub(crate) fn notice(t: &Tokens, bar: gpui::Hsla, text: impl Into<SharedString>) -> Div {
    div()
        .flow_row()
        .relative()
        .items_start()
        .min_h(px(24.))
        .pad_start(size::S3)
        .ui_sm()
        .text_color(t.ink_2)
        .child((div().absolute().top_0().bottom_0().w(px(2.)).bg(bar)).start_at(px(0.)))
        .child(div().flex_1().min_w_0().child(text.into()))
}

pub(crate) fn path_row(
    input: &Entity<InputState>,
    button_id: &'static str,
    label: String,
    on_choose: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    div()
        .flow_row()
        .items_center()
        .gap(size::S2)
        .child(div().flex_1().min_w_0().child(Field::new(input)))
        .child(Button::new(button_id, label).on_click(on_choose))
}

pub(crate) fn single_line(text: &str, _: &mut gpui::Context<InputState>) -> bool {
    !text.contains(['\n', '\r'])
}

pub(crate) fn picker(files: bool, directories: bool) -> PathPromptOptions {
    PathPromptOptions {
        files,
        directories,
        multiple: false,
        prompt: None,
    }
}

pub(crate) fn escape_closes(event: &KeyDownEvent) -> bool {
    event.keystroke.key == "escape" && !event.keystroke.modifiers.modified()
}

pub(crate) fn focus_on_open(focus: &FocusHandle, window: &mut Window) {
    window.focus(focus);
}
