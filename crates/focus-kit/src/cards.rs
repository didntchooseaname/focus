use gpui::{
    AnyElement, App, Div, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window,
    div, prelude::FluentBuilder, px,
};

use crate::dir::Flow as _;
use crate::icons::{Icon, icon};
use crate::tokens::{ActiveTokens, Tokens, size};
use crate::typography::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffMark {
    Same,
    Added,
    Removed,
}

fn diff_color(t: &Tokens, mark: DiffMark) -> Option<gpui::Hsla> {
    match mark {
        DiffMark::Added => Some(t.ok),
        DiffMark::Removed => Some(t.danger),
        DiffMark::Same => None,
    }
}

pub fn diff_row(t: &Tokens, mark: DiffMark) -> Div {
    div()
        .relative()
        .flex()
        .flex_row()
        .when_some(diff_color(t, mark), |this, color| {
            this.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(2.))
                    .bg(color),
            )
        })
}

pub fn diff_marker(t: &Tokens, mark: DiffMark) -> Div {
    let sign = match mark {
        DiffMark::Added => "+",
        DiffMark::Removed => "-",
        DiffMark::Same => " ",
    };
    div()
        .flex_none()
        .text_color(diff_color(t, mark).unwrap_or(t.ink_3))
        .child(sign)
}

#[derive(IntoElement)]
pub struct DiffCard {
    title: SharedString,
    path: Option<SharedString>,
    note: Option<SharedString>,
    rows: Vec<(DiffMark, SharedString)>,
    more: Option<SharedString>,
    counts: Option<(usize, usize)>,
    footer: Vec<AnyElement>,
}

impl DiffCard {
    pub fn new(title: impl Into<SharedString>) -> Self {
        DiffCard {
            title: title.into(),
            path: None,
            note: None,
            rows: Vec::new(),
            more: None,
            counts: None,
            footer: Vec::new(),
        }
    }
    pub fn path(mut self, path: impl Into<SharedString>) -> Self {
        self.path = Some(path.into());
        self
    }
    pub fn note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }
    pub fn rows(mut self, rows: impl IntoIterator<Item = (DiffMark, SharedString)>) -> Self {
        self.rows = rows.into_iter().collect();
        self
    }
    pub fn more(mut self, more: impl Into<SharedString>) -> Self {
        self.more = Some(more.into());
        self
    }
    pub fn counts(mut self, added: usize, removed: usize) -> Self {
        self.counts = Some((added, removed));
        self
    }
    pub fn footer(mut self, element: impl IntoElement) -> Self {
        self.footer.push(element.into_any_element());
        self
    }
}

impl RenderOnce for DiffCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let rows =
            self.rows.into_iter().map(|(mark, text)| {
                diff_row(&t, mark)
                    .gap(size::S1)
                    .px(size::S2)
                    .mono_sm()
                    .child(diff_marker(&t, mark).w(px(8.)))
                    .child(div().flex_1().min_w_0().text_color(t.ink_body).child(
                        if text.is_empty() {
                            SharedString::from(" ")
                        } else {
                            text
                        },
                    ))
            });
        let has_rows = rows.len() > 0;
        div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .min_w_0()
            .p(size::S3)
            .rounded(size::RADIUS)
            .border_1()
            .border_color(t.line_2)
            .bg(t.card)
            .child(div().ui_strong().text_start().child(self.title))
            .when_some(self.path, |this, path| {
                this.child(
                    div()
                        .flex()
                        .mono_sm()
                        .text_color(t.ink_2)
                        .child(crate::ellipsis(path)),
                )
            })
            .when_some(self.note, |this, note| {
                this.child(div().ui().text_start().text_color(t.ink_body).child(note))
            })
            .when(has_rows, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .py(size::S1)
                        .rounded(size::RADIUS)
                        .border_1()
                        .border_color(t.line)
                        .bg(t.bg_2)
                        .children(rows)
                        .when_some(self.more, |this, more| {
                            this.child(div().px(size::S2).mono_sm().text_color(t.ink_3).child(more))
                        }),
                )
            })
            .when_some(self.counts, |this, (added, removed)| {
                this.child(
                    div()
                        .mono_sm()
                        .text_color(t.ink_3)
                        .child(format!("+{added} -{removed}")),
                )
            })
            .when(!self.footer.is_empty(), |this| {
                this.child(
                    div()
                        .flow_row()
                        .flex_wrap()
                        .gap(size::S2)
                        .children(self.footer),
                )
            })
    }
}

#[derive(IntoElement)]
pub struct ErrorCard {
    message: SharedString,
    actions: Vec<AnyElement>,
}

impl ErrorCard {
    pub fn new(message: impl Into<SharedString>) -> Self {
        ErrorCard {
            message: message.into(),
            actions: Vec::new(),
        }
    }
    pub fn action(mut self, element: impl IntoElement) -> Self {
        self.actions.push(element.into_any_element());
        self
    }
}

impl RenderOnce for ErrorCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .min_w_0()
            .p(size::S3)
            .rounded(size::RADIUS)
            .border_1()
            .border_color(t.danger)
            .child(
                div()
                    .flow_row()
                    .items_start()
                    .gap(size::S2)
                    .child(
                        icon(Icon::ALERT_CIRCLE, size::ICON)
                            .text_color(t.danger)
                            .mt(px(2.)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .ui()
                            .text_start()
                            .text_color(t.ink)
                            .child(self.message),
                    ),
            )
            .when(!self.actions.is_empty(), |this| {
                this.child(
                    div()
                        .flow_row()
                        .flex_wrap()
                        .gap(size::S2)
                        .children(self.actions),
                )
            })
    }
}
