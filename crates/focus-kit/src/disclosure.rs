use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::dir::{Dir, Flow as _};
use crate::icons::{Icon, icon};
use crate::tokens::{ActiveTokens, size};
use crate::typography::Type;

type ToggleHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct Disclosure {
    id: ElementId,
    label: SharedString,
    open: bool,
    icon: Option<Icon>,
    detail: Option<SharedString>,
    tooltip: Option<SharedString>,
    on_toggle: Option<ToggleHandler>,
}

impl Disclosure {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, open: bool) -> Self {
        Disclosure {
            id: id.into(),
            label: label.into(),
            open,
            icon: None,
            detail: None,
            tooltip: None,
            on_toggle: None,
        }
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
    pub fn on_toggle(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Disclosure {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = *cx.tokens();
        let glyph = self.icon.unwrap_or(if self.open {
            Icon::CHEVRON_DOWN
        } else {
            Dir::current().forward()
        });
        div()
            .id(self.id)
            .flow_row()
            .items_center()
            .gap(size::S1)
            .h(px(22.))
            .min_w_0()
            .cursor_pointer()
            .text_color(t.ink_3)
            .hover(move |s| s.text_color(t.ink_2))
            .when_some(self.tooltip, |this, tip| {
                this.tooltip(move |_, cx| crate::Tooltip::view(tip.clone(), None, cx))
            })
            .when_some(self.on_toggle, |this, handler| this.on_click(handler))
            .child(icon(glyph, size::ICON_SMALL))
            .child(div().min_w_0().ui_sm().child(crate::ellipsis(self.label)))
            .when_some(self.detail, |this, detail| {
                this.child(div().flex_none().mono_sm().child(detail))
            })
    }
}
