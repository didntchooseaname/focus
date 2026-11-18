//! Compact conversation and status markers.
//!
//! This is the Marker API from the component catalog, backported to the
//! crates.io 0.5.1 layout so applications pinned to GPUI 0.2.2 can use it.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, AnyElement, App, IntoElement, ParentElement, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, ease_in_out, prelude::FluentBuilder as _,
    px,
};

use crate::{ActiveTheme as _, Sizable as _, StyledExt as _, h_flex, spinner::Spinner};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum MarkerVariant {
    #[default]
    Plain,
    Separator,
    Border,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum MarkerLoadingStyle {
    #[default]
    Spinner,
    Shimmer,
}

#[derive(IntoElement)]
pub struct MarkerIcon {
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl MarkerIcon {
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }
}

impl ParentElement for MarkerIcon {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for MarkerIcon {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MarkerIcon {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .size_4()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .children(self.children)
            .refine_style(&self.style)
    }
}

#[derive(IntoElement)]
pub struct MarkerContent {
    style: StyleRefinement,
    text: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl MarkerContent {
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            text: None,
            children: Vec::new(),
        }
    }

    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = Some(text.into());
        self
    }
}

impl ParentElement for MarkerContent {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for MarkerContent {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MarkerContent {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        h_flex()
            .min_w_0()
            .gap_2()
            .when_some(self.text, |this, text| this.child(text))
            .children(self.children)
            .refine_style(&self.style)
    }
}

#[derive(IntoElement)]
pub struct Marker {
    style: StyleRefinement,
    separator_style: StyleRefinement,
    variant: MarkerVariant,
    loading: bool,
    loading_style: MarkerLoadingStyle,
    icon: Option<MarkerIcon>,
    content: Option<MarkerContent>,
    children: Vec<AnyElement>,
}

impl Marker {
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            separator_style: StyleRefinement::default(),
            variant: MarkerVariant::Plain,
            loading: false,
            loading_style: MarkerLoadingStyle::Spinner,
            icon: None,
            content: None,
            children: Vec::new(),
        }
    }

    pub fn with_variant(mut self, variant: MarkerVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn with_loading_style(mut self, style: MarkerLoadingStyle) -> Self {
        self.loading_style = style;
        self
    }

    pub fn separator_style(mut self, style: StyleRefinement) -> Self {
        self.separator_style = style;
        self
    }

    pub fn icon(mut self, icon: MarkerIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn content(mut self, content: MarkerContent) -> Self {
        self.content = Some(content);
        self
    }
}

impl ParentElement for Marker {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Marker {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Marker {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let separator = || {
            div()
                .h(px(1.))
                .flex_1()
                .bg(cx.theme().border)
                .refine_style(&self.separator_style)
        };
        let spinner = self.loading
            && self.loading_style == MarkerLoadingStyle::Spinner
            && self.icon.is_none();
        let shimmer = self.loading && self.loading_style == MarkerLoadingStyle::Shimmer;

        h_flex()
            .w_full()
            .min_h_4()
            .gap_2()
            .items_center()
            .text_xs()
            .when(self.variant == MarkerVariant::Separator, |this| {
                this.child(separator())
            })
            .when(spinner, |this| this.child(Spinner::new().xsmall()))
            .when_some(self.icon, |this, icon| this.child(icon))
            .when_some(self.content, |this, content| {
                let content = div().min_w_0().child(content);
                this.child(if shimmer {
                    content
                        .with_animation(
                            "marker-shimmer",
                            Animation::new(Duration::from_secs(2))
                                .repeat()
                                .with_easing(ease_in_out),
                            |this, delta| this.opacity(0.55 + 0.45 * delta),
                        )
                        .into_any_element()
                } else {
                    content.into_any_element()
                })
            })
            .children(self.children)
            .when(self.variant == MarkerVariant::Separator, |this| {
                this.child(separator())
            })
            .when(self.variant == MarkerVariant::Border, |this| {
                this.pb_1().border_b_1().border_color(cx.theme().border)
            })
            .refine_style(&self.style)
    }
}
