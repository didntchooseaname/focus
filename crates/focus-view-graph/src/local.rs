use crate::core::GraphCore;
use crate::surface::{GraphHost, rebuild, surface};
use crate::view::{GraphEvent, at_top_start};
use focus_core::{NoteId, Vault, VaultEvent};
use focus_graph::{Params, Spec};
use focus_kit::{ActiveTokens, Segment, Segments, Type};
use gpui::{
    Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, Window, div, prelude::FluentBuilder,
};
use std::sync::Arc;

pub struct LocalGraph {
    core: GraphCore,
    depth: u8,
    focus: FocusHandle,
}

impl EventEmitter<GraphEvent> for LocalGraph {}

impl Focusable for LocalGraph {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl GraphHost for LocalGraph {
    fn core(&self) -> &GraphCore {
        &self.core
    }
    fn core_mut(&mut self) -> &mut GraphCore {
        &mut self.core
    }
    fn focus(&self) -> &FocusHandle {
        &self.focus
    }
}

impl LocalGraph {
    pub fn new(vault: Arc<Vault>, depth: u8, cx: &mut Context<Self>) -> Self {
        let spec = Spec {
            around: None,
            ..Spec::default()
        };
        LocalGraph {
            core: GraphCore::new(vault, None, None, spec, Params::default()),
            depth: depth.clamp(1, 3),
            focus: cx.focus_handle(),
        }
    }

    pub fn set_active_note(&mut self, note: Option<NoteId>, cx: &mut Context<Self>) {
        if self.core.active == note {
            return;
        }
        self.core.active = note;
        self.core.selected = None;
        self.refresh(cx);
    }

    pub fn set_depth(&mut self, depth: u8, cx: &mut Context<Self>) {
        let depth = depth.clamp(1, 3);
        if depth != self.depth {
            self.depth = depth;
            self.refresh(cx);
        }
    }

    pub fn depth(&self) -> u8 {
        self.depth
    }

    pub fn vault_changed(&mut self, event: &VaultEvent, cx: &mut Context<Self>) {
        if !matches!(event, VaultEvent::TreeChanged) {
            self.refresh(cx);
        }
    }

    pub fn is_idle(&self) -> bool {
        !self.core.building && self.core.is_settled() && !self.core.ticking
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.core.spec.around = self.core.active.map(|id| (id, self.depth));
        self.core.clear();
        if self.core.spec.around.is_none() {
            cx.notify();
            return;
        }
        self.core.needs_fit.set(true);
        rebuild(self, cx);
    }
}

impl Render for LocalGraph {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::surface::follow_frames(self, window, cx);
        let t = *cx.tokens();
        let empty = !self.core.building && self.core.graph.nodes.is_empty();
        let depth = self.depth;
        let segments: Vec<Segment> = (1..=3).map(|d| Segment::new(d.to_string())).collect();
        let weak = cx.entity().downgrade();
        surface(self, "graph-local", 0.0, cx)
            .when(empty, |el| {
                el.child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .p(focus_kit::size::S4)
                        .child(
                            div()
                                .ui_sm()
                                .text_color(t.ink_3)
                                .child(focus_i18n::t("graph-empty")),
                        ),
                )
            })
            .child(
                at_top_start(div().absolute())
                    .occlude()
                    .flex()
                    .when(
                        focus_i18n::direction() == focus_i18n::Direction::Rtl,
                        |el| el.flex_row_reverse(),
                    )
                    .items_center()
                    .gap(focus_kit::size::S2)
                    .child(div().ui_xs().text_color(t.ink_3).child(focus_i18n::t_args(
                        "graph-depth",
                        &[("depth", focus_i18n::Arg::Int(depth as i64))],
                    )))
                    .child(
                        Segments::new("graph-depth", segments, (depth - 1) as usize).on_select(
                            move |i, _, cx| {
                                let _ = weak.update(cx, |this, cx| this.set_depth(i as u8 + 1, cx));
                            },
                        ),
                    ),
            )
    }
}
