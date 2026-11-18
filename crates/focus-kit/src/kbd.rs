use gpui::{App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px};

use crate::tokens::{ActiveTokens, size};
use crate::typography::Type;

#[derive(IntoElement)]
pub struct Kbd {
    keys: SharedString,
}

impl Kbd {
    pub fn new(keys: impl Into<SharedString>) -> Self {
        Kbd { keys: keys.into() }
    }
}

impl RenderOnce for Kbd {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.tokens();
        div()
            .flex_none()
            .px(px(5.))
            .py(px(1.))
            .rounded(size::RADIUS)
            .border_1()
            .border_color(t.line_2)
            .mono_sm()
            .text_color(t.ink_3)
            .child(self.keys)
    }
}
