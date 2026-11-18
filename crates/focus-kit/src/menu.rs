use gpui::{
    App, ClickEvent, Div, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::dir::Flow as _;
use crate::icons::{Icon, icon};
use crate::interaction::Forced;
use crate::tokens::{ActiveTokens, Tokens, size};
use crate::typography::Type;

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub fn floating(t: &Tokens) -> Div {
    div().bg(t.card).border_1().border_color(t.float_border)
}

pub fn menu(t: &Tokens) -> Div {
    floating(t).flex().flex_col().py(size::S1).min_w(px(220.))
}

pub fn menu_separator(t: &Tokens) -> Div {
    div().my(size::S1).h(px(1.)).bg(t.line)
}

#[derive(IntoElement)]
pub struct MenuRow {
    id: ElementId,
    label: SharedString,
    icon: Option<Icon>,
    shortcut: Option<SharedString>,
    checked: bool,
    danger: bool,
    disabled: bool,
    highlighted: bool,
    current: bool,
    forced: Forced,
    on_click: Option<Handler>,
}

impl MenuRow {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        MenuRow {
            id: id.into(),
            label: label.into(),
            icon: None,
            shortcut: None,
            checked: false,
            danger: false,
            disabled: false,
            highlighted: false,
            current: false,
            forced: Forced::Live,
            on_click: None,
        }
    }
    pub fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn highlighted(mut self, highlighted: bool) -> Self {
        self.highlighted = highlighted;
        self
    }
    pub fn forced(mut self, forced: Forced) -> Self {
        self.forced = forced;
        self
    }
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for MenuRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let ink = if self.disabled {
            t.ink_4
        } else if self.danger {
            t.danger
        } else if self.highlighted || self.current || self.forced.hover() {
            t.ink
        } else {
            t.ink_2
        };
        let bg = if self.disabled {
            None
        } else if self.forced.pressed() || self.current {
            Some(t.card_3)
        } else if self.highlighted || self.forced.hover() {
            Some(t.card_2)
        } else {
            None
        };
        let live = self.forced == Forced::Live && !self.disabled;
        let danger = self.danger;
        div()
            .id(self.id)
            .relative()
            .flow_row()
            .items_center()
            .gap(size::S2)
            .h(size::ROW)
            .mx(size::S1)
            .px(size::S2)
            .rounded(size::RADIUS)
            .ui()
            .text_color(ink)
            .when_some(bg, |this, bg| this.bg(bg))
            .when(live, |this| {
                this.cursor_pointer()
                    .hover(move |s| {
                        s.bg(t.card_2)
                            .text_color(if danger { t.danger } else { t.ink })
                    })
                    .active(move |s| s.bg(t.card_3))
            })
            .when(self.forced.focus() || self.current, |this| {
                this.child(
                    crate::Dir::current()
                        .start(div().absolute(), px(0.))
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .bg(t.ink),
                )
            })
            .child(
                div()
                    .flex_none()
                    .w(size::ICON_DENSE)
                    .text_color(if self.checked { t.ink } else { ink })
                    .when(self.checked, |this| {
                        this.child(icon(Icon::CHECK, size::ICON_DENSE))
                    })
                    .when_some(self.icon.filter(|_| !self.checked), |this, glyph| {
                        this.child(icon(glyph, size::ICON_DENSE))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flow_row()
                    .child(crate::ellipsis(self.label)),
            )
            .when_some(self.shortcut, |this, keys| {
                this.child(div().flex_none().mono_sm().text_color(t.ink_3).child(keys))
            })
            .when_some(self.on_click.filter(|_| !self.disabled), |this, handler| {
                this.on_click(handler)
            })
    }
}
