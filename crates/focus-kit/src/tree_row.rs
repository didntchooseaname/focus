use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};

use crate::dir::Flow as _;
use crate::icons::{Icon, icon};
use crate::interaction::Forced;
use crate::tokens::{ActiveTokens, size};
use crate::typography::Type;

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const LEAD: gpui::Pixels = px(8.);

#[derive(IntoElement)]
pub struct TreeRow {
    id: ElementId,
    name: SharedString,
    depth: usize,
    folder: Option<bool>,
    count: Option<u32>,
    icon: Option<Icon>,
    trailing: Option<AnyElement>,
    active: bool,
    focused: bool,
    forced: Forced,
    on_click: Option<Handler>,
}

impl TreeRow {
    pub fn new(id: impl Into<ElementId>, name: impl Into<SharedString>) -> Self {
        TreeRow {
            id: id.into(),
            name: name.into(),
            depth: 0,
            folder: None,
            count: None,
            icon: None,
            trailing: None,
            active: false,
            focused: false,
            forced: Forced::Live,
            on_click: None,
        }
    }
    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }
    pub fn folder(mut self, expanded: bool) -> Self {
        self.folder = Some(expanded);
        self
    }
    pub fn count(mut self, count: u32) -> Self {
        self.count = Some(count);
        self
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing = Some(element.into_any_element());
        self
    }
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
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

impl RenderOnce for TreeRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let is_folder = self.folder.is_some();
        let bg = if self.active || self.forced.pressed() {
            Some(t.card_3)
        } else if self.forced.hover() {
            Some(t.card_2)
        } else {
            None
        };
        let ink = if self.active || is_folder || self.forced.hover() {
            t.ink
        } else {
            t.ink_2
        };
        let bar = self.active || self.focused || self.forced.focus();
        let dir = crate::Dir::current();
        dir.flow(div().id(self.id))
            .relative()
            .items_center()
            .w_full()
            .h(size::ROW)
            .map(|this| dir.pad_end(dir.pad_start(this, LEAD), size::S3))
            .when_some(bg, |this, bg| this.bg(bg))
            .when(self.forced == Forced::Live && !self.active, |this| {
                this.hover(move |s| s.bg(t.card_2))
                    .active(move |s| s.bg(t.card_3))
            })
            .cursor_pointer()
            .ui()
            .text_color(ink)
            .children((0..self.depth).map(|_| {
                div()
                    .flex_none()
                    .w(size::INDENT)
                    .h_full()
                    .flow_row()
                    .justify_center()
                    .child(div().w(px(1.)).h_full().bg(t.line))
            }))
            .child(
                div()
                    .flex_none()
                    .w(size::ICON_DENSE)
                    .text_color(t.ink_3)
                    .when_some(self.folder, |this, open| {
                        this.child(icon(
                            if open {
                                Icon::CHEVRON_DOWN
                            } else {
                                dir.forward()
                            },
                            size::ICON_DENSE,
                        ))
                    })
                    .when_some(self.icon.filter(|_| !is_folder), |this, glyph| {
                        this.child(icon(glyph, size::ICON_DENSE))
                    }),
            )
            .child(
                dir.margin_start(div().flex_1().min_w_0().flow_row(), px(6.))
                    .child(crate::ellipsis(self.name)),
            )
            .when_some(self.count, |this, count| {
                this.child(
                    dir.margin_start(div().flex_none(), size::S2)
                        .mono_sm()
                        .text_color(t.ink_4)
                        .child(count.to_string()),
                )
            })
            .when_some(self.trailing, |this, trailing| this.child(trailing))
            .when(bar, |this| {
                this.child(
                    dir.start(div().absolute(), px(0.))
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .bg(t.ink),
                )
            })
            .when_some(self.on_click, |this, handler| this.on_click(handler))
    }
}
