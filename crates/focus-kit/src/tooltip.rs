use gpui::{
    AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, div, prelude::FluentBuilder,
};

use crate::dir::Flow as _;
use crate::kbd::Kbd;
use crate::tokens::{ActiveTokens, size};
use crate::typography::Type;

pub struct Tooltip {
    label: SharedString,
    shortcut: Option<SharedString>,
}

impl Tooltip {
    pub fn view(label: SharedString, shortcut: Option<SharedString>, cx: &mut App) -> AnyView {
        cx.new(|_| Tooltip { label, shortcut }).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        tooltip_body(&t, self.label.clone(), self.shortcut.clone())
    }
}

pub fn tooltip_body(
    t: &crate::Tokens,
    label: SharedString,
    shortcut: Option<SharedString>,
) -> gpui::Div {
    div()
        .flow_row()
        .items_center()
        .gap(size::S2)
        .px(size::S2)
        .py(size::S1)
        .bg(t.card)
        .border_1()
        .border_color(t.float_border)
        .ui_sm()
        .text_color(t.ink_2)
        .child(label)
        .when_some(shortcut, |this, keys| this.child(Kbd::new(keys)))
}
