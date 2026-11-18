use crate::core::{GraphCore, build_graph};
use crate::paint::{Snapshot, clean_label, graph_canvas};
use crate::view::GraphEvent;
use focus_kit::{ActiveTokens, Tokens, Type, size};
use gpui::{
    AnyElement, AppContext as _, Context, EventEmitter, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement,
    ScrollWheelEvent, Stateful, Styled, div, prelude::FluentBuilder, px,
};

const TOOLTIP_WIDTH: f32 = 320.0;
const TOOLTIP_HEIGHT: f32 = 72.0;
const TOOLTIP_OFFSET: f32 = 8.0;

pub(crate) trait GraphHost: EventEmitter<GraphEvent> + Sized + 'static {
    fn core(&self) -> &GraphCore;
    fn core_mut(&mut self) -> &mut GraphCore;
    fn focus(&self) -> &FocusHandle;
    fn pointer_moved(&mut self, _pointer: [f32; 2], _cx: &mut Context<Self>) -> bool {
        false
    }
    fn pointer_released(&mut self) -> bool {
        false
    }
}

pub(crate) fn rebuild<V: GraphHost>(view: &mut V, cx: &mut Context<V>) {
    let core = view.core_mut();
    core.building = true;
    core.build_generation += 1;
    let generation = core.build_generation;
    let input = core.build_input();
    let task = cx.background_spawn(async move { build_graph(input) });
    cx.spawn(async move |this, cx| {
        let built = task.await;
        let _ = this.update(cx, |this, cx| {
            if this.core().build_generation != generation {
                return;
            }
            this.core_mut().install(built);
            ensure_ticking(this, cx);
            cx.notify();
        });
    })
    .detach();
    cx.notify();
}

pub(crate) fn ensure_ticking<V: GraphHost>(view: &mut V, cx: &mut Context<V>) {
    view.core_mut().ticking = true;
    cx.notify();
}

pub(crate) fn follow_frames<V: GraphHost>(
    view: &mut V,
    window: &mut gpui::Window,
    cx: &mut Context<V>,
) {
    let core = view.core_mut();
    if !core.ticking || core.frame_armed {
        return;
    }
    core.frame_armed = true;
    let entity = cx.entity().downgrade();
    window.on_next_frame(move |_, cx| {
        let _ = entity.update(cx, |this, cx| {
            let core = this.core_mut();
            core.frame_armed = false;
            if !core.tick() {
                core.ticking = false;
            }
            cx.notify();
        });
    });
}

fn position(p: gpui::Point<gpui::Pixels>) -> [f32; 2] {
    [f32::from(p.x), f32::from(p.y)]
}

fn on_key<V: GraphHost>(view: &mut V, event: &KeyDownEvent, cx: &mut Context<V>) {
    const STEP: f32 = 48.0;
    let core = view.core_mut();
    match event.keystroke.key.as_str() {
        "left" => core.pan_by([STEP, 0.0]),
        "right" => core.pan_by([-STEP, 0.0]),
        "up" => core.pan_by([0.0, STEP]),
        "down" => core.pan_by([0.0, -STEP]),
        "+" | "=" => core.zoom_by(1.25),
        "-" => core.zoom_by(0.8),
        "0" => core.fit(),
        "escape" => {
            core.selected = None;
            core.refresh_neighbors();
        }
        _ => return,
    }
    cx.stop_propagation();
    cx.notify();
}

fn snapshot<V: GraphHost>(view: &V, label_scale: f32, cx: &gpui::App) -> Snapshot {
    let core = view.core();
    Snapshot {
        graph: core.graph.clone(),
        table: core.table.clone(),
        positions: core.positions.clone(),
        index: core.index.clone(),
        camera: core.camera.clone(),
        fit_zoom: core.fit_zoom.clone(),
        viewport: core.viewport.clone(),
        needs_fit: core.needs_fit.clone(),
        focus: core.focus(),
        neighbors: core.neighbors.clone(),
        active: core.active_node(),
        tokens: *cx.tokens(),
        frame: core.frame.clone(),
        label_widths: core.label_widths.clone(),
        label_scale,
        node_scale: core.node_scale,
        born: core.born().cloned(),
        drawn: core.drawn.clone(),
        painted: core.painted.clone(),
        probe: core.probe.clone(),
    }
}

fn tooltip(core: &GraphCore, t: &Tokens) -> Option<AnyElement> {
    let i = core.tooltip_node()?;
    let node = core.graph.nodes.get(i as usize)?;
    let [_, _, width, height] = core.viewport.get();
    let camera = core.camera.get();
    let at = camera.to_screen(*core.positions.get(i as usize)?, [width, height]);
    let r = crate::plan::radius(node.degree, camera.zoom, core.node_scale);
    let (incoming, outgoing) = core.focus_links;
    let counts = focus_i18n::t_args(
        "graph-node-degree",
        &[
            ("incoming", focus_i18n::Arg::Int(incoming as i64)),
            ("outgoing", focus_i18n::Arg::Int(outgoing as i64)),
        ],
    );
    let el = div()
        .absolute()
        .w(px(TOOLTIP_WIDTH))
        .flex()
        .flex_col()
        .gap(size::S1)
        .px(size::S2)
        .py(size::S1)
        .bg(t.card)
        .border_1()
        .border_color(t.float_border)
        .child(
            div()
                .ui_sm()
                .text_color(t.ink)
                .truncate()
                .child(clean_label(&node.label)),
        )
        .when_some(node.path.as_ref(), |el, path| {
            el.child(
                div()
                    .mono_sm()
                    .text_color(t.ink_3)
                    .truncate()
                    .child(path.to_string_lossy().into_owned()),
            )
        })
        .child(div().ui_xs().text_color(t.ink_3).child(counts));
    let el = if at[0] + r + TOOLTIP_WIDTH <= width {
        el.left(px(at[0] + r))
    } else {
        el.right(px((width - at[0] + r).max(0.0)))
    };
    let el = if at[1] - r - TOOLTIP_OFFSET >= TOOLTIP_HEIGHT {
        el.bottom(px(height - at[1] + r + TOOLTIP_OFFSET))
    } else {
        el.top(px(at[1] + r + crate::plan::LABEL_HEIGHT + TOOLTIP_OFFSET))
    };
    Some(el.into_any_element())
}

pub(crate) fn surface<V: GraphHost>(
    view: &V,
    id: &'static str,
    label_scale: f32,
    cx: &mut Context<V>,
) -> Stateful<gpui::Div> {
    let t = *cx.tokens();
    div()
        .id(id)
        .relative()
        .size_full()
        .overflow_hidden()
        .bg(t.bg)
        .track_focus(view.focus())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this: &mut V, e: &MouseDownEvent, window, cx| {
                window.focus(this.focus());
                let new_tab = e.modifiers.control || e.modifiers.platform;
                if this
                    .core_mut()
                    .mouse_down(position(e.position), e.click_count, new_tab)
                {
                    cx.notify();
                }
            }),
        )
        .on_mouse_move(cx.listener(|this: &mut V, e: &MouseMoveEvent, _, cx| {
            let pointer = position(e.position);
            if this.pointer_moved(pointer, cx) {
                return;
            }
            let (changed, follow) = this
                .core_mut()
                .mouse_move(pointer, e.pressed_button == Some(MouseButton::Left));
            if follow {
                ensure_ticking(this, cx);
            }
            if changed {
                cx.notify();
            }
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this: &mut V, _, _, cx| {
                if this.pointer_released() {
                    return;
                }
                if this.core_mut().mouse_up() {
                    if let Some((note, new_tab)) = this.core_mut().clicked.take() {
                        cx.emit(GraphEvent::OpenNote { note, new_tab });
                    }
                    ensure_ticking(this, cx);
                    cx.notify();
                }
            }),
        )
        .on_scroll_wheel(cx.listener(|this: &mut V, e: &ScrollWheelEvent, _, cx| {
            let d = e.delta.pixel_delta(px(20.));
            this.core_mut().scroll(position(e.position), f32::from(d.y));
            cx.notify();
        }))
        .on_key_down(cx.listener(|this: &mut V, e: &KeyDownEvent, _, cx| on_key(this, e, cx)))
        .child(graph_canvas(snapshot(view, label_scale, cx)))
        .children(tooltip(view.core(), &t))
}
