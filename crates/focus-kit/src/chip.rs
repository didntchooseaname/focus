use gpui::{
    App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px,
};

use crate::dir::Flow as _;
use crate::tokens::{ActiveTokens, size};
use crate::typography::Type;

enum Part {
    Key(SharedString),
    Value(SharedString),
}

#[derive(IntoElement, Default)]
pub struct Chip {
    parts: Vec<Part>,
    tag: Option<SharedString>,
}

impl Chip {
    pub fn new() -> Self {
        Chip::default()
    }
    pub fn key(mut self, key: impl Into<SharedString>) -> Self {
        self.parts.push(Part::Key(key.into()));
        self
    }
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.parts.push(Part::Value(value.into()));
        self
    }
    pub fn tag(name: impl Into<SharedString>) -> Self {
        Chip {
            parts: Vec::new(),
            tag: Some(name.into()),
        }
    }
}

impl RenderOnce for Chip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let mut chip = div()
            .flow_row()
            .flex_none()
            .items_baseline()
            .gap(size::S1)
            .px(px(6.))
            .py(px(2.))
            .rounded(size::RADIUS)
            .bg(t.card_2)
            .ui_sm();
        if let Some(tag) = self.tag {
            return crate::Dir::current()
                .of_text(&tag)
                .flow(chip)
                .gap(px(1.))
                .child(div().text_color(t.ink_3).child("#"))
                .child(div().mono_sm_in_row().text_color(t.ink_2).child(tag));
        }
        for part in self.parts {
            chip = match part {
                Part::Key(text) => chip.child(div().text_color(t.ink_3).child(text)),
                Part::Value(text) => chip.child(
                    div()
                        .text_color(t.ink)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(text),
                ),
            };
        }
        chip
    }
}

type RemoveHandler = Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct ContextChip {
    id: gpui::ElementId,
    label: SharedString,
    count: Option<SharedString>,
    tooltip: Option<SharedString>,
    remove_label: SharedString,
    on_remove: Option<RemoveHandler>,
}

impl ContextChip {
    pub fn new(
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        remove_label: impl Into<SharedString>,
    ) -> Self {
        ContextChip {
            id: id.into(),
            label: label.into(),
            count: None,
            tooltip: None,
            remove_label: remove_label.into(),
            on_remove: None,
        }
    }
    pub fn count(mut self, count: impl Into<SharedString>) -> Self {
        self.count = Some(count.into());
        self
    }
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
    pub fn on_remove(
        mut self,
        handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_remove = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for ContextChip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui::{
            InteractiveElement as _, StatefulInteractiveElement as _, prelude::FluentBuilder as _,
        };
        let t = *cx.tokens();
        let remove_label = self.remove_label.clone();
        div()
            .id(self.id)
            .flow_row()
            .items_center()
            .gap(size::S1)
            .h(px(22.))
            .max_w_full()
            .min_w_0()
            .pad_start(size::S2)
            .pad_end(px(2.))
            .rounded(size::RADIUS)
            .bg(t.card_2)
            .ui_sm()
            .when_some(self.tooltip, |this, tip| {
                this.tooltip(move |_, cx| crate::Tooltip::view(tip.clone(), None, cx))
            })
            .child(
                div()
                    .min_w_0()
                    .text_color(t.ink)
                    .child(crate::ellipsis(self.label)),
            )
            .when_some(self.count, |this, count| {
                this.child(div().flex_none().mono_sm().text_color(t.ink_3).child(count))
            })
            .child(
                div()
                    .id("remove")
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(18.))
                    .rounded(size::RADIUS)
                    .cursor_pointer()
                    .text_color(t.ink_3)
                    .hover(move |s| s.bg(t.card_3).text_color(t.ink))
                    .tooltip(move |_, cx| crate::Tooltip::view(remove_label.clone(), None, cx))
                    .when_some(self.on_remove, |this, handler| this.on_click(handler))
                    .child(crate::icon(crate::Icon::CLOSE, size::ICON_SMALL)),
            )
    }
}
