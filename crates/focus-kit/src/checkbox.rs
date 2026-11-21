use std::rc::Rc;

use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::dir::Flow as _;
use crate::icons::{Icon, icon};
use crate::interaction::Forced;
use crate::keyboard::{self, ClickHandler, with_press};
use crate::tokens::{ActiveTokens, size};
use crate::typography::Type;

const GROUP: &str = "focus-checkbox";

#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    checked: bool,
    label: Option<SharedString>,
    disabled: bool,
    forced: Forced,
    tab_stop: bool,
    on_click: Option<ClickHandler>,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>, checked: bool) -> Self {
        Checkbox {
            id: id.into(),
            checked,
            label: None,
            disabled: false,
            forced: Forced::Live,
            tab_stop: true,
            on_click: None,
        }
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn forced(mut self, forced: Forced) -> Self {
        self.forced = forced;
        self
    }
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Checkbox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let pressable = (self.forced == Forced::Live && !self.disabled)
            .then_some(self.on_click)
            .flatten()
            .map(|handler| {
                let focus = keyboard::control_focus(&self.id, None, self.tab_stop, window, cx);
                (focus, handler)
            });
        let focused = self.forced.focus()
            || pressable
                .as_ref()
                .is_some_and(|(focus, _)| focus.is_focused(window));
        let (fill, border) = if self.disabled {
            (
                if self.checked {
                    t.card_3
                } else {
                    t.bg.opacity(0.)
                },
                t.line,
            )
        } else if self.checked {
            (
                if self.forced.pressed() {
                    t.btn_press
                } else {
                    t.btn
                },
                t.btn,
            )
        } else if self.forced.pressed() {
            (t.card_3, t.ink_3)
        } else if self.forced.hover() {
            (t.bg.opacity(0.), t.ink_3)
        } else {
            (t.bg.opacity(0.), t.line_2)
        };
        let live = self.forced == Forced::Live && !self.disabled;
        let checked = self.checked;
        div()
            .id(self.id)
            .group(GROUP)
            .flow_row()
            .items_center()
            .gap(size::S2)
            .when(live, |this| this.cursor_pointer())
            .child(
                div()
                    .relative()
                    .flow_row()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(size::CHECKBOX)
                    .rounded(size::RADIUS)
                    .border_1()
                    .border_color(border)
                    .bg(fill)
                    .text_color(if self.disabled { t.ink_4 } else { t.btn_ink })
                    .when(live && !checked, |this| {
                        this.group_hover(GROUP, move |s| s.border_color(t.ink_3))
                    })
                    .when(checked, |this| {
                        this.child(icon(Icon::CHECK, size::ICON_SMALL))
                    })
                    .when(focused, |this| {
                        this.child(
                            crate::Dir::inset(div().absolute(), px(-3.))
                                .rounded(size::RADIUS)
                                .border_1()
                                .border_color(t.ink),
                        )
                    }),
            )
            .when_some(self.label, |this, label| {
                this.child(
                    div()
                        .ui()
                        .text_color(if self.disabled { t.ink_4 } else { t.ink_2 })
                        .child(label),
                )
            })
            .when_some(pressable, |this, (focus, handler)| {
                with_press(this, keyboard::CHECKBOX, &focus, handler)
            })
    }
}
