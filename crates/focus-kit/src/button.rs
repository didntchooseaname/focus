use gpui::{
    AnyView, App, ClickEvent, ElementId, FocusHandle, FontWeight, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};

use crate::dir::Flow as _;
use crate::icons::{Icon, icon};
use crate::interaction::{Forced, inset_ring};
use crate::tokens::{ActiveTokens, size};
use crate::tooltip::Tooltip;
use crate::typography::Type;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    #[default]
    Secondary,
    Ghost,
    Danger,
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<Icon>,
    kind: ButtonKind,
    large: bool,
    disabled: bool,
    forced: Forced,
    focus: Option<FocusHandle>,
    tooltip: Option<SharedString>,
    on_click: Option<ClickHandler>,
}

impl Button {
    pub fn icon_only(id: impl Into<ElementId>, icon: Icon, label: impl Into<SharedString>) -> Self {
        Button {
            label: None,
            icon: Some(icon),
            tooltip: Some(label.into()),
            ..Button::new(id, "")
        }
    }

    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Button {
            id: id.into(),
            label: Some(label.into()),
            icon: None,
            kind: ButtonKind::Secondary,
            large: false,
            disabled: false,
            forced: Forced::Live,
            focus: None,
            tooltip: None,
            on_click: None,
        }
    }
    pub fn kind(mut self, kind: ButtonKind) -> Self {
        self.kind = kind;
        self
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
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
    pub fn forced(mut self, forced: Forced) -> Self {
        self.forced = forced;
        self
    }
    pub fn track_focus(mut self, handle: &FocusHandle) -> Self {
        self.focus = Some(handle.clone());
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

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let focused =
            self.forced.focus() || self.focus.as_ref().is_some_and(|f| f.is_focused(window));
        let (bg, ink, border, hover_bg, hover_ink, hover_border, press_bg) = match self.kind {
            ButtonKind::Primary => (
                t.btn,
                t.btn_ink,
                t.btn,
                t.btn_hover,
                t.btn_ink,
                t.btn_hover,
                t.btn_press,
            ),
            ButtonKind::Secondary => (
                t.bg.opacity(0.),
                t.ink,
                t.line_2,
                t.bg.opacity(0.),
                t.ink,
                t.ink_3,
                t.card_3,
            ),
            ButtonKind::Ghost => (
                t.bg.opacity(0.),
                t.ink_2,
                t.bg.opacity(0.),
                t.card_2,
                t.ink,
                t.card_2,
                t.card_3,
            ),
            ButtonKind::Danger => (
                t.bg.opacity(0.),
                t.danger,
                t.danger,
                t.card_2,
                t.danger,
                t.danger,
                t.card_3,
            ),
        };
        let (bg, ink, border) = if self.disabled {
            match self.kind {
                ButtonKind::Primary => (t.card_3, t.ink_4, t.card_3),
                ButtonKind::Ghost => (bg, t.ink_4, border),
                _ => (bg, t.ink_4, t.line),
            }
        } else if self.forced.pressed() {
            (press_bg, hover_ink, hover_border)
        } else if self.forced.hover() {
            (hover_bg, hover_ink, hover_border)
        } else {
            (bg, ink, border)
        };
        let border = if focused && !self.disabled && self.kind != ButtonKind::Primary {
            t.ink
        } else {
            border
        };
        let live = self.forced == Forced::Live && !self.disabled;
        let height = if self.large {
            size::CONTROL_LG
        } else {
            size::CONTROL
        };
        let icon_size = size::ICON;

        div()
            .id(self.id)
            .relative()
            .flow_row()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .h(height)
            .px(if self.label.is_some() {
                size::S3
            } else {
                px(0.)
            })
            .when(self.label.is_none(), |this| this.w(height))
            .rounded(size::RADIUS)
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_color(ink)
            .ui()
            .when(self.kind == ButtonKind::Primary, |this| {
                this.font_weight(FontWeight::SEMIBOLD)
            })
            .when_some(self.focus, |this, handle| this.track_focus(&handle))
            .when(live, |this| {
                this.cursor_pointer()
                    .hover(move |s| {
                        s.bg(hover_bg)
                            .text_color(hover_ink)
                            .border_color(if focused { t.ink } else { hover_border })
                    })
                    .active(move |s| s.bg(press_bg))
            })
            .when(
                focused && !self.disabled && self.kind == ButtonKind::Primary,
                |this| this.child(inset_ring(t.btn_ink)),
            )
            .when_some(self.icon, |this, glyph| this.child(icon(glyph, icon_size)))
            .when_some(self.label, |this, label| this.child(label))
            .when_some(self.tooltip.filter(|_| !self.disabled), |this, label| {
                this.tooltip(move |_, cx| -> AnyView { Tooltip::view(label.clone(), None, cx) })
            })
            .when_some(self.on_click.filter(|_| !self.disabled), |this, handler| {
                this.on_click(handler)
            })
    }
}

const GROUP: &str = "focus-icon-button";

#[derive(IntoElement)]
pub struct IconButton {
    id: ElementId,
    icon: Icon,
    label: SharedString,
    shortcut: Option<SharedString>,
    active: bool,
    toggled: bool,
    disabled: bool,
    dense: bool,
    forced: Forced,
    on_click: Option<ClickHandler>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: Icon, label: impl Into<SharedString>) -> Self {
        IconButton {
            id: id.into(),
            icon,
            label: label.into(),
            shortcut: None,
            active: false,
            toggled: false,
            disabled: false,
            dense: false,
            forced: Forced::Live,
            on_click: None,
        }
    }
    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
    pub fn toggled(mut self, toggled: bool) -> Self {
        self.toggled = toggled;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn dense(mut self, dense: bool) -> Self {
        self.dense = dense;
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

impl RenderOnce for IconButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let ink = if self.disabled {
            t.ink_4
        } else if self.active || self.toggled || self.forced.hover() || self.forced.pressed() {
            t.ink
        } else {
            t.ink_3
        };
        let bg = if self.disabled {
            t.bg.opacity(0.)
        } else if self.forced.pressed() || self.toggled {
            t.card_3
        } else if self.forced.hover() {
            t.card_2
        } else {
            t.bg.opacity(0.)
        };
        let border = if self.forced.focus() {
            t.ink
        } else {
            t.bg.opacity(0.)
        };
        let live = self.forced == Forced::Live && !self.disabled;
        let glyph_size = if self.dense {
            size::ICON_DENSE
        } else {
            size::ICON
        };
        let tooltip: Option<(SharedString, Option<SharedString>)> =
            (!self.disabled).then(|| (self.label.clone(), self.shortcut.clone()));

        div()
            .id(self.id)
            .flow_row()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(28.))
            .child(
                div()
                    .id("glyph")
                    .flow_row()
                    .items_center()
                    .justify_center()
                    .size(size::ICON_BUTTON)
                    .rounded(size::RADIUS)
                    .border_1()
                    .border_color(border)
                    .bg(bg)
                    .text_color(ink)
                    .when(live, |this| {
                        this.group_hover(GROUP, move |s| s.bg(t.card_2).text_color(t.ink))
                            .group_active(GROUP, move |s| s.bg(t.card_3).text_color(t.ink))
                    })
                    .child(icon(self.icon, glyph_size)),
            )
            .when(live, |this| this.cursor_pointer().group(GROUP))
            .when_some(tooltip, |this, (label, shortcut)| {
                this.tooltip(move |_, cx| -> AnyView {
                    Tooltip::view(label.clone(), shortcut.clone(), cx)
                })
            })
            .when_some(self.on_click.filter(|_| !self.disabled), |this, handler| {
                this.on_click(handler)
            })
    }
}
