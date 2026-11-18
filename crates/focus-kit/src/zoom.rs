use gpui::{App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px};

use crate::{ActiveTokens, IconButton, Type, floating, size};

#[derive(IntoElement)]
pub struct ZoomControls {
    percent: SharedString,
    out: IconButton,
    zoom_in: IconButton,
    fit: IconButton,
    fit_width: Option<IconButton>,
    bare: bool,
}

impl ZoomControls {
    pub fn new(
        percent: impl Into<SharedString>,
        out: IconButton,
        zoom_in: IconButton,
        fit: IconButton,
    ) -> Self {
        ZoomControls {
            percent: percent.into(),
            out,
            zoom_in,
            fit,
            fit_width: None,
            bare: false,
        }
    }

    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    pub fn fit_width(mut self, button: IconButton) -> Self {
        self.fit_width = Some(button);
        self
    }
}

impl RenderOnce for ZoomControls {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let frame = if self.bare {
            div()
        } else {
            floating(&t).p(size::S1)
        };
        frame
            .flex()
            .items_center()
            .gap(size::S1)
            .child(self.out)
            .child(
                div()
                    .mono_sm()
                    .text_color(t.ink_3)
                    .min_w(px(48.))
                    .flex()
                    .justify_center()
                    .child(self.percent),
            )
            .child(self.zoom_in)
            .child(self.fit)
            .children(self.fit_width)
    }
}
