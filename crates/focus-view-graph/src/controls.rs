use crate::view::{GraphSettings, GraphView};
use focus_kit::{ActiveTokens, Checkbox, Field, Icon, IconButton, Tokens, Type, size};
use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, StatefulInteractiveElement, Styled, Window, canvas, div, px,
};
use std::cell::Cell;
use std::rc::Rc;

const PANEL_WIDTH: f32 = 288.0;
const FOLDERS_HEIGHT: f32 = 168.0;
const TRACK_HEIGHT: f32 = 16.0;
const THUMB: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Force {
    Repulsion,
    Distance,
    Gravity,
}

impl Force {
    const ALL: [Force; 3] = [Force::Repulsion, Force::Distance, Force::Gravity];

    fn range(self) -> (f32, f32) {
        match self {
            Force::Repulsion => (0.1, 4.0),
            Force::Distance => (10.0, 120.0),
            Force::Gravity => (0.0, 0.3),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Force::Repulsion => "graph-force-repulsion",
            Force::Distance => "graph-force-distance",
            Force::Gravity => "graph-force-gravity",
        }
    }

    fn get(self, s: &GraphSettings) -> f32 {
        match self {
            Force::Repulsion => s.repulsion,
            Force::Distance => s.link_distance,
            Force::Gravity => s.gravity,
        }
    }

    fn set(self, s: &mut GraphSettings, v: f32) {
        match self {
            Force::Repulsion => s.repulsion = v,
            Force::Distance => s.link_distance = v,
            Force::Gravity => s.gravity = v,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

pub(crate) type SliderBounds = Rc<[Cell<[f32; 4]>; 3]>;

pub(crate) fn new_slider_bounds() -> SliderBounds {
    Rc::new([
        Cell::new([0.0; 4]),
        Cell::new([0.0; 4]),
        Cell::new([0.0; 4]),
    ])
}

fn value_at(force: Force, bounds: [f32; 4], x: f32) -> f32 {
    let (lo, hi) = force.range();
    let f = ((x - bounds[0]) / bounds[2].max(1.0)).clamp(0.0, 1.0);
    lo + (hi - lo) * f
}

pub(crate) fn drag_slider(
    view: &mut GraphView,
    force: Force,
    pointer: [f32; 2],
    cx: &mut Context<GraphView>,
) {
    let bounds = view.slider_bounds[force.index()].get();
    let mut settings = view.settings.clone();
    force.set(&mut settings, value_at(force, bounds, pointer[0]));
    view.set_settings(settings, cx);
}

fn format_value(force: Force, v: f32) -> String {
    match force {
        Force::Distance => focus_i18n::format_decimal(v as f64, 0),
        Force::Repulsion => focus_i18n::format_decimal(v as f64, 2),
        Force::Gravity => focus_i18n::format_decimal(v as f64, 3),
    }
}

fn slider(view: &GraphView, force: Force, t: &Tokens, cx: &mut Context<GraphView>) -> AnyElement {
    let (lo, hi) = force.range();
    let value = force.get(&view.settings);
    let fraction = ((value - lo) / (hi - lo)).clamp(0.0, 1.0);
    let bounds = view.slider_bounds.clone();
    let (track, thumb) = (
        t.line_2,
        if view.slider == Some(force) {
            t.ink
        } else {
            t.ink_2
        },
    );
    div()
        .id(("graph-slider", force.index()))
        .relative()
        .w_full()
        .h(px(TRACK_HEIGHT))
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.slider = Some(force);
                drag_slider(
                    this,
                    force,
                    [f32::from(e.position.x), f32::from(e.position.y)],
                    cx,
                );
            }),
        )
        .child(
            canvas(
                move |b, _, _| {
                    bounds[force.index()].set([
                        f32::from(b.origin.x),
                        f32::from(b.origin.y),
                        f32::from(b.size.width),
                        f32::from(b.size.height),
                    ])
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(TRACK_HEIGHT / 2.0))
                .h(px(1.))
                .bg(track),
        )
        .child(
            div()
                .absolute()
                .top(px((TRACK_HEIGHT - THUMB) / 2.0))
                .left(gpui::relative(fraction))
                .ml(px(-THUMB / 2.0))
                .size(px(THUMB))
                .rounded(size::RADIUS)
                .bg(thumb),
        )
        .into_any_element()
}

fn row() -> gpui::Div {
    match focus_i18n::direction() {
        focus_i18n::Direction::Ltr => div().flex().flex_row(),
        focus_i18n::Direction::Rtl => div().flex().flex_row_reverse(),
    }
}

fn rule(t: &Tokens) -> impl IntoElement {
    div().h(px(1.)).w_full().bg(t.line)
}

fn toggle(
    id: &'static str,
    key: &str,
    checked: bool,
    apply: fn(&mut GraphSettings, bool),
    cx: &mut Context<GraphView>,
) -> AnyElement {
    Checkbox::new(id, checked)
        .label(focus_i18n::t(key))
        .on_click(cx.listener(move |this, _, _, cx| {
            let mut s = this.settings.clone();
            apply(&mut s, !checked);
            this.set_settings(s, cx);
        }))
        .into_any_element()
}

fn top_folders(view: &GraphView) -> Vec<String> {
    let tree = view.core.vault.tree();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(node) = tree.nodes.get(i) {
        if node.depth == 0 && node.kind == focus_core::NodeKind::Folder && node.note_count > 0 {
            out.push(node.name.to_string());
        }
        i = (node.subtree_end as usize).max(i + 1);
    }
    out
}

fn folders(view: &GraphView, t: &Tokens, cx: &mut Context<GraphView>) -> Option<AnyElement> {
    let names = top_folders(view);
    if names.is_empty() {
        return None;
    }
    let hidden = &view.settings.hidden_folders;
    let boxes = names.into_iter().enumerate().map(|(k, name)| {
        let shown = !hidden.contains(&name);
        Checkbox::new(("graph-folder", k), shown)
            .label(name.clone())
            .on_click(cx.listener(move |this, _, _, cx| {
                let mut s = this.settings.clone();
                if shown {
                    s.hidden_folders.push(name.clone());
                    s.hidden_folders.sort();
                } else {
                    s.hidden_folders.retain(|f| *f != name);
                }
                this.set_settings(s, cx);
            }))
            .into_any_element()
    });
    Some(
        div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .p(size::S3)
            .child(
                row().child(
                    div()
                        .ui_xs()
                        .text_color(t.ink_3)
                        .child(focus_i18n::t("graph-filter-folders")),
                ),
            )
            .child(
                div()
                    .id("graph-folders")
                    .flex()
                    .flex_col()
                    .gap(size::S2)
                    .max_h(px(FOLDERS_HEIGHT))
                    .overflow_y_scroll()
                    .children(boxes),
            )
            .into_any_element(),
    )
}

pub(crate) fn panel(
    view: &GraphView,
    _window: &mut Window,
    cx: &mut Context<GraphView>,
) -> AnyElement {
    let t = *cx.tokens();
    let open = view.panel_open;
    let button = IconButton::new(
        "graph-controls",
        Icon::SLIDERS,
        focus_i18n::t("graph-controls"),
    )
    .active(open)
    .on_click(cx.listener(|this, _, _, cx| {
        this.panel_open = !this.panel_open;
        cx.notify();
    }));
    if !open {
        return div().child(button).into_any_element();
    }
    let (nodes, edges) = view.counts();
    let stats = focus_i18n::t_args(
        "graph-stats",
        &[
            ("notes", focus_i18n::Arg::Int(nodes as i64)),
            ("links", focus_i18n::Arg::Int(edges as i64)),
        ],
    );
    let missed = view.find_missed.as_ref().map(|q| {
        focus_i18n::t_args("graph-find-none", &[("query", focus_i18n::Arg::Str(q))]).into()
    });
    let s = &view.settings;
    div()
        .w(px(PANEL_WIDTH))
        .flex()
        .flex_col()
        .bg(t.card)
        .border_1()
        .border_color(t.float_border)
        .on_mouse_down(MouseButton::Left, |_, _, cx: &mut App| {
            cx.stop_propagation()
        })
        .child(
            row()
                .items_center()
                .gap(size::S2)
                .p(size::S1)
                .child(button)
                .child(
                    div()
                        .ui_strong()
                        .text_color(t.ink)
                        .child(focus_i18n::t("graph-title")),
                ),
        )
        .child(rule(&t))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .p(size::S3)
                .child(Field::new(&view.query_input).icon(Icon::SEARCH))
                .child(toggle(
                    "graph-tags",
                    "graph-show-tags",
                    s.tags,
                    |s, v| s.tags = v,
                    cx,
                ))
                .child(toggle(
                    "graph-unresolved",
                    "graph-show-unresolved",
                    s.unresolved,
                    |s, v| s.unresolved = v,
                    cx,
                ))
                .child(toggle(
                    "graph-attachments",
                    "graph-show-attachments",
                    s.attachments,
                    |s, v| s.attachments = v,
                    cx,
                ))
                .child(toggle(
                    "graph-orphans",
                    "graph-filter-orphans",
                    s.orphans,
                    |s, v| s.orphans = v,
                    cx,
                )),
        )
        .children(folders(view, &t, cx).map(|f| div().child(rule(&t)).child(f)))
        .child(rule(&t))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .p(size::S3)
                .child(
                    row().child(
                        div()
                            .ui_xs()
                            .text_color(t.ink_3)
                            .child(focus_i18n::t("graph-forces")),
                    ),
                )
                .children(Force::ALL.map(|force| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(size::S1)
                        .child(
                            row()
                                .items_center()
                                .justify_between()
                                .gap(size::S2)
                                .child(
                                    div()
                                        .ui_sm()
                                        .text_color(t.ink_2)
                                        .truncate()
                                        .child(focus_i18n::t(force.key())),
                                )
                                .child(
                                    div()
                                        .mono_sm()
                                        .text_color(t.ink_3)
                                        .flex_none()
                                        .child(format_value(force, force.get(s))),
                                ),
                        )
                        .child(slider(view, force, &t, cx))
                })),
        )
        .child(rule(&t))
        .child(
            div().p(size::S3).child(
                Field::new(&view.find_input)
                    .icon(Icon::CROSSHAIR)
                    .error(missed),
            ),
        )
        .child(rule(&t))
        .child(
            row()
                .px(size::S3)
                .py(size::S2)
                .mono_sm()
                .text_color(t.ink_3)
                .child(stats),
        )
        .into_any_element()
}
