use gpui::{
    App, Entity, Focusable, InteractiveElement, IntoElement, MouseButton, ParentElement,
    RenderOnce, SharedString, Styled, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::input::{Input, InputState};

use crate::dir::Flow as _;
use crate::icons::{Icon, icon};
use crate::interaction::Forced;
use crate::tokens::{ActiveTokens, size};
use crate::typography::{SANS, Type};

#[derive(IntoElement)]
pub struct Field {
    state: Entity<InputState>,
    icon: Option<Icon>,
    suffix: Option<gpui::AnyElement>,
    large: bool,
    disabled: bool,
    error: Option<SharedString>,
    forced: Forced,
}

impl Field {
    pub fn new(state: &Entity<InputState>) -> Self {
        Field {
            state: state.clone(),
            icon: None,
            suffix: None,
            large: false,
            disabled: false,
            error: None,
            forced: Forced::Live,
        }
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn suffix(mut self, suffix: impl IntoElement) -> Self {
        self.suffix = Some(suffix.into_any_element());
        self
    }
    pub fn large(mut self, large: bool) -> Self {
        self.large = large;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn error(mut self, error: Option<SharedString>) -> Self {
        self.error = error;
        self
    }
    pub fn forced(mut self, forced: Forced) -> Self {
        self.forced = forced;
        self
    }
}

impl RenderOnce for Field {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let handle = self.state.read(cx).focus_handle(cx);
        crate::keyboard::name_focus(&handle, "field", cx);
        let focused = self.forced.focus() || handle.is_focused(window);
        let border = if self.disabled {
            t.line
        } else if self.error.is_some() {
            t.danger
        } else if focused {
            t.ink
        } else if self.forced.hover() {
            t.ink_3
        } else {
            t.line_2
        };
        let height = if self.large {
            size::CONTROL_LG
        } else {
            size::CONTROL
        };
        let live =
            self.forced == Forced::Live && !self.disabled && self.error.is_none() && !focused;
        div()
            .flex()
            .flex_col()
            .gap(size::S1)
            .min_w_0()
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .gap(size::S2)
                    .h(height)
                    .px(size::S2)
                    .rounded(size::RADIUS)
                    .border_1()
                    .border_color(border)
                    .bg(t.bg_2)
                    .font_family(SANS)
                    .text_color(if self.disabled { t.ink_4 } else { t.ink })
                    .when(live, |this| this.hover(move |s| s.border_color(t.ink_3)))
                    .on_mouse_down(MouseButton::Left, {
                        let state = self.state.clone();
                        let disabled = self.disabled;
                        move |_, window, cx| {
                            if !disabled && !window.default_prevented() {
                                state.update(cx, |input, cx| input.focus(window, cx));
                                window.prevent_default();
                            }
                        }
                    })
                    .when_some(self.icon, |this, glyph| {
                        this.child(icon(glyph, size::ICON_DENSE).text_color(t.ink_3))
                    })
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.state)
                                .appearance(false)
                                .disabled(self.disabled)
                                .text_size(px(13.))
                                .line_height(px(20.))
                                .px_0()
                                .py_0()
                                .h(px(20.)),
                        ),
                    )
                    .when_some(self.suffix, |this, suffix| this.child(suffix)),
            )
            .when_some(self.error, |this, message| {
                this.child(div().ui_sm().text_color(t.danger).child(message))
            })
    }
}

#[derive(IntoElement)]
pub struct TextArea {
    state: Entity<InputState>,
    disabled: bool,
    forced: Forced,
}

impl TextArea {
    pub fn new(state: &Entity<InputState>) -> Self {
        TextArea {
            state: state.clone(),
            disabled: false,
            forced: Forced::Live,
        }
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn forced(mut self, forced: Forced) -> Self {
        self.forced = forced;
        self
    }
}

impl RenderOnce for TextArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let handle = self.state.read(cx).focus_handle(cx);
        crate::keyboard::name_focus(&handle, "field", cx);
        let focused = self.forced.focus() || handle.is_focused(window);
        let border = if self.disabled {
            t.line
        } else if focused {
            t.ink
        } else if self.forced.hover() {
            t.ink_3
        } else {
            t.line_2
        };
        let live = self.forced == Forced::Live && !self.disabled && !focused;
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .px(size::S2)
            .py(size::S1)
            .rounded(size::RADIUS)
            .border_1()
            .border_color(border)
            .bg(t.bg_2)
            .font_family(SANS)
            .text_color(if self.disabled { t.ink_4 } else { t.ink })
            .when(live, |this| this.hover(move |s| s.border_color(t.ink_3)))
            .child(
                Input::new(&self.state)
                    .appearance(false)
                    .disabled(self.disabled)
                    .text_size(px(13.))
                    .line_height(px(20.))
                    .px_0()
                    .py_0(),
            )
    }
}
