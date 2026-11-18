use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::dir::Flow as _;
use crate::icons::{Icon, icon};
use crate::interaction::Forced;
use crate::tokens::{ActiveTokens, size};
use crate::typography::Type;

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const GROUP: &str = "focus-note-tab";

#[derive(IntoElement)]
pub struct NoteTab {
    id: ElementId,
    title: SharedString,
    active: bool,
    dirty: bool,
    forced: Forced,
    close_label: SharedString,
    on_click: Option<Handler>,
    on_close: Option<Handler>,
}

impl NoteTab {
    pub fn new(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        close_label: impl Into<SharedString>,
    ) -> Self {
        NoteTab {
            id: id.into(),
            title: title.into(),
            active: false,
            dirty: false,
            forced: Forced::Live,
            close_label: close_label.into(),
            on_click: None,
            on_close: None,
        }
    }
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
    pub fn dirty(mut self, dirty: bool) -> Self {
        self.dirty = dirty;
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
    pub fn on_close(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for NoteTab {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let dir = crate::Dir::current();
        let hovered = self.forced.hover();
        let ink = if self.active || hovered {
            t.ink
        } else {
            t.ink_2
        };
        let close_visible = self.active || hovered;
        dir.flow(div().id(self.id))
            .group(GROUP)
            .relative()
            .flex_shrink()
            .items_center()
            .gap(size::S2)
            .min_w(size::TAB_MIN)
            .max_w(size::TAB_MAX)
            .h(size::TAB_BAR)
            .map(|this| dir.pad_end(dir.pad_start(this, size::S3), px(8.)))
            .map(|this| dir.border_end(this, t.line))
            .border_color(t.line)
            .when(self.forced.pressed(), |this| this.bg(t.card_2))
            .child(
                crate::Dir::across(div().absolute())
                    .bottom_0()
                    .h(px(1.))
                    .bg(if self.active { t.ink } else { t.line }),
            )
            .when(self.forced.focus(), |this| {
                this.child(
                    crate::Dir::inset(div().absolute(), px(2.))
                        .border_1()
                        .border_color(t.ink),
                )
            })
            .text_color(ink)
            .ui()
            .cursor_pointer()
            .when(self.forced == Forced::Live, |this| {
                this.hover(move |s| s.text_color(t.ink))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flow_row()
                    .child(crate::ellipsis(self.title)),
            )
            .child(
                div()
                    .id("close")
                    .flow_row()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(20.))
                    .rounded(size::RADIUS)
                    .text_color(t.ink_3)
                    .map(|this| {
                        if self.dirty && !hovered {
                            this.child(div().size(size::DOT).rounded_full().bg(t.ink))
                                .group_hover(GROUP, |s| s.invisible())
                        } else {
                            this.child(icon(Icon::CLOSE, size::ICON_DENSE))
                                .when(!close_visible, |this| {
                                    this.invisible().group_hover(GROUP, |s| s.visible())
                                })
                                .hover(move |s| s.bg(t.card_2).text_color(t.ink))
                        }
                    })
                    .tooltip({
                        let label = self.close_label.clone();
                        move |_, cx| {
                            crate::tooltip::Tooltip::view(label.clone(), Some("Ctrl W".into()), cx)
                        }
                    })
                    .when_some(self.on_close, |this, handler| {
                        this.on_click(move |event, window, cx| {
                            cx.stop_propagation();
                            handler(event, window, cx)
                        })
                    }),
            )
            .when_some(self.on_click, |this, handler| this.on_click(handler))
    }
}
