use crate::dir::Flow as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, ClickEvent, ElementId, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    Window, div, px,
};

use crate::button::IconButton;
use crate::icons::Icon;
use crate::menu::floating;
use crate::tokens::{ActiveTokens, size};
use crate::typography::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Ok,
    Warn,
    Danger,
}

#[derive(IntoElement)]
pub struct Toast {
    tone: Tone,
    text: SharedString,
    close: Option<IconButton>,
}

impl Toast {
    pub fn new(tone: Tone, text: impl Into<SharedString>) -> Self {
        Toast {
            tone,
            text: text.into(),
            close: None,
        }
    }

    pub fn on_close(
        mut self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.close = Some(
            IconButton::new(id, Icon::CLOSE, label)
                .dense(true)
                .on_click(handler),
        );
        self
    }
}

impl RenderOnce for Toast {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let bar = match self.tone {
            Tone::Neutral => t.ink,
            Tone::Ok => t.ok,
            Tone::Warn => t.warn,
            Tone::Danger => t.danger,
        };
        let dir = crate::Dir::current();
        dir.flow(floating(&t))
            .relative()
            .items_center()
            .h(px(32.))
            .min_w_0()
            .max_w(px(640.))
            .gap(size::S2)
            .map(|this| {
                dir.pad_end(
                    dir.pad_start(this, size::S3 + px(2.)),
                    if self.close.is_some() {
                        px(2.)
                    } else {
                        size::S3
                    },
                )
            })
            .ui()
            .text_color(t.ink)
            .child(
                dir.start(div().absolute(), px(0.))
                    .top_0()
                    .bottom_0()
                    .w(px(2.))
                    .bg(bar),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flow_row()
                    .child(crate::ellipsis(self.text)),
            )
            .children(self.close)
    }
}

pub fn dot(color: gpui::Hsla) -> gpui::Div {
    div().flex_none().size(size::DOT).rounded_full().bg(color)
}
