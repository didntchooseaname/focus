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
    close_shortcut: Option<SharedString>,
    on_click: Option<Handler>,
    on_close: Option<Handler>,
    pin: Option<Pin>,
}

struct Pin {
    label: SharedString,
    shortcut: Option<SharedString>,
    on_unpin: Option<Handler>,
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
            close_shortcut: None,
            on_click: None,
            on_close: None,
            pin: None,
        }
    }
    pub fn pinned(
        mut self,
        unpin_label: impl Into<SharedString>,
        shortcut: Option<SharedString>,
    ) -> Self {
        self.pin = Some(Pin {
            label: unpin_label.into(),
            shortcut,
            on_unpin: None,
        });
        self
    }
    pub fn on_unpin(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        if let Some(pin) = &mut self.pin {
            pin.on_unpin = Some(Box::new(handler));
        }
        self
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
    pub fn close_shortcut(mut self, keys: impl Into<SharedString>) -> Self {
        self.close_shortcut = Some(keys.into());
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
        let pinned = self.pin.is_some();
        dir.flow(div().id(self.id))
            .group(GROUP)
            .relative()
            .flex_shrink()
            .items_center()
            .gap(size::S2)
            .min_w(size::TAB_MIN)
            .max_w(size::TAB_MAX)
            .h(size::TAB_BAR)
            .map(|this| dir.pad_end(dir.pad_start(this, size::S3), size::S1))
            .map(|this| dir.border_end(this, t.line))
            .border_color(t.line)
            .when(self.forced.pressed(), |this| this.bg(t.card_3))
            .when(self.forced == Forced::Live, |this| {
                this.active(move |s| s.bg(t.card_3))
            })
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
            .when_some(self.pin, |this, pin| {
                this.child(pin_control(pin, self.dirty, hovered, &t))
            })
            .child(
                div()
                    .id("close")
                    .flow_row()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .relative()
                    .size(size::ICON_BUTTON)
                    .when(pinned, |this| this.hidden())
                    .rounded(size::RADIUS)
                    .text_color(t.ink_3)
                    .map(|this| {
                        if self.dirty && !hovered {
                            this.child(
                                div()
                                    .size(size::DOT)
                                    .rounded_full()
                                    .bg(t.ink)
                                    .group_hover(GROUP, |s| s.invisible()),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .flow_row()
                                    .items_center()
                                    .justify_center()
                                    .invisible()
                                    .group_hover(GROUP, |s| s.visible())
                                    .child(icon(Icon::CLOSE, size::ICON_DENSE)),
                            )
                            .hover(move |s| s.bg(t.card_2).text_color(t.ink))
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
                        let keys = self.close_shortcut.clone();
                        move |_, cx| crate::tooltip::Tooltip::view(label.clone(), keys.clone(), cx)
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

fn pin_control(
    pin: Pin,
    dirty: bool,
    hovered: bool,
    t: &crate::tokens::Tokens,
) -> impl IntoElement {
    let (ink, ink_3, card_2) = (t.ink, t.ink_3, t.card_2);
    let dot_at_rest = dirty && !hovered;
    div()
        .id("pin")
        .relative()
        .flow_row()
        .flex_none()
        .items_center()
        .justify_center()
        .size(size::ICON_BUTTON)
        .rounded(size::RADIUS)
        .text_color(if hovered { ink } else { ink_3 })
        .hover(move |s| s.bg(card_2).text_color(ink))
        .when(dot_at_rest, |this| {
            this.child(
                div()
                    .size(size::DOT)
                    .rounded_full()
                    .bg(ink)
                    .group_hover(GROUP, |s| s.invisible()),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flow_row()
                    .items_center()
                    .justify_center()
                    .invisible()
                    .group_hover(GROUP, |s| s.visible())
                    .child(icon(Icon::PIN, size::ICON_DENSE)),
            )
        })
        .when(!dot_at_rest, |this| {
            this.child(icon(Icon::PIN, size::ICON_DENSE))
        })
        .tooltip({
            let (label, shortcut) = (pin.label.clone(), pin.shortcut.clone());
            move |_, cx| crate::tooltip::Tooltip::view(label.clone(), shortcut.clone(), cx)
        })
        .when_some(pin.on_unpin, |this, handler| {
            this.on_click(move |event, window, cx| {
                cx.stop_propagation();
                handler(event, window, cx)
            })
        })
}
