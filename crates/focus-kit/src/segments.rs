use std::rc::Rc;

use gpui::{
    App, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::dir::Flow as _;
use crate::icons::{Icon, icon};
use crate::interaction::{Forced, inset_ring};
use crate::keyboard::{self, StepLeft, StepRight};
use crate::tokens::{ActiveTokens, size};
use crate::typography::Type;

type SelectHandler = Rc<dyn Fn(usize, &mut Window, &mut App) + 'static>;

pub struct Segment {
    pub label: SharedString,
    pub icon: Option<Icon>,
    pub note: Option<SharedString>,
}

impl Segment {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Segment {
            label: label.into(),
            icon: None,
            note: None,
        }
    }
}

#[derive(IntoElement)]
pub struct Segments {
    id: ElementId,
    items: Vec<Segment>,
    selected: usize,
    disabled: bool,
    forced: Option<(usize, Forced)>,
    tab_stop: bool,
    on_select: Option<SelectHandler>,
}

impl Segments {
    pub fn new(id: impl Into<ElementId>, items: Vec<Segment>, selected: usize) -> Self {
        Segments {
            id: id.into(),
            items,
            selected,
            disabled: false,
            forced: None,
            tab_stop: true,
            on_select: None,
        }
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn forced(mut self, index: usize, forced: Forced) -> Self {
        self.forced = Some((index, forced));
        self
    }
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }
    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Segments {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let disabled = self.disabled;
        let count = self.items.len();
        let selected = self.selected;
        let steering = (self.forced.is_none() && !disabled)
            .then(|| self.on_select.clone())
            .flatten()
            .map(|handler| {
                let focus = keyboard::control_focus(&self.id, None, self.tab_stop, window, cx);
                (focus, handler)
            });
        let focused = steering
            .as_ref()
            .is_some_and(|(focus, _)| focus.is_focused(window));
        let step = move |left: bool, handler: &SelectHandler, window: &mut Window, cx: &mut App| {
            let target = if keyboard::step_toward_end(left) {
                (selected + 1).min(count.saturating_sub(1))
            } else {
                selected.saturating_sub(1)
            };
            if target != selected {
                handler(target, window, cx);
            }
        };
        div()
            .id(self.id)
            .when_some(steering, |this, (focus, handler)| {
                let leftward = handler.clone();
                this.key_context(keyboard::SEGMENTS)
                    .on_action(move |_: &StepLeft, window, cx| step(true, &leftward, window, cx))
                    .on_action(move |_: &StepRight, window, cx| step(false, &handler, window, cx))
                    .child(keyboard::focus_anchor(&focus))
            })
            .flow_row()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .h(size::CONTROL)
            .p(px(2.))
            .rounded(size::RADIUS)
            .border_1()
            .border_color(if disabled { t.line } else { t.line_2 })
            .children(self.items.into_iter().enumerate().map(|(index, item)| {
                let active = index == self.selected;
                let forced = match self.forced {
                    Some((i, forced)) if i == index => forced,
                    _ => Forced::Live,
                };
                let (bg, ink) = if disabled {
                    (if active { t.card_3 } else { t.bg.opacity(0.) }, t.ink_4)
                } else if active {
                    (
                        if forced.pressed() { t.btn_press } else { t.btn },
                        t.btn_ink,
                    )
                } else if forced.pressed() {
                    (t.card_3, t.ink)
                } else if forced.hover() {
                    (t.card_2, t.ink)
                } else {
                    (t.bg.opacity(0.), t.ink_2)
                };
                let on_select = self.on_select.clone();
                let live = forced == Forced::Live && !disabled;
                div()
                    .id(index)
                    .relative()
                    .flow_row()
                    .items_center()
                    .gap(size::S2)
                    .h(px(22.))
                    .px(size::S3)
                    .rounded(size::RADIUS)
                    .bg(bg)
                    .text_color(ink)
                    .ui()
                    .line_height(px(22.))
                    .when(active, |this| this.font_weight(FontWeight::SEMIBOLD))
                    .when(live && !active, |this| {
                        this.cursor_pointer()
                            .hover(move |s| s.bg(t.card_2).text_color(t.ink))
                            .active(move |s| s.bg(t.card_3))
                    })
                    .when(live && active, |this| {
                        this.active(move |s| s.bg(t.btn_press))
                    })
                    .when(forced.focus() || (focused && active), |this| {
                        this.child(inset_ring(if active { t.btn_ink } else { t.ink }))
                    })
                    .when_some(item.icon, |this, glyph| {
                        this.child(icon(glyph, size::ICON_DENSE))
                    })
                    .child(item.label)
                    .when_some(item.note, |this, note| {
                        this.child(
                            div()
                                .ui_sm()
                                .text_color(if active { t.btn_ink } else { t.ink_3 })
                                .child(note),
                        )
                    })
                    .when_some(on_select.filter(|_| !disabled), |this, handler| {
                        this.on_click(move |_, window, cx| handler(index, window, cx))
                    })
            }))
    }
}
