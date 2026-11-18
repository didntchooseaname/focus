use crate::camera::Camera;
use crate::core::FrameTimes;
use crate::plan::{
    EdgeDraw, EdgeStyle, Emphasis, Inputs, LABEL_HEIGHT, NodeTable, PLATE_PAD, Plan, Shape, plan,
};
use focus_graph::{Graph, NodeKind, SpatialIndex};
use focus_kit::Tokens;
use focus_kit::typography::{MONO, SANS};
use gpui::{
    App, BorderStyle, Bounds, ContentMask, Hsla, IntoElement, Path, Pixels, Rgba, SharedString,
    Styled, TextRun, Window, canvas, font, point, px, quad, size, transparent_black,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

const LABEL_SIZE: f32 = 12.5;
const LABEL_CHARS: usize = 48;
const DIM: f32 = 0.25;
const ARROW: f32 = 5.0;
const GLOW_STEPS: usize = 16;

pub(crate) struct Snapshot {
    pub graph: Arc<Graph>,
    pub table: Arc<NodeTable>,
    pub positions: Arc<Vec<[f32; 2]>>,
    pub index: Arc<SpatialIndex>,
    pub camera: Rc<Cell<Camera>>,
    pub fit_zoom: Rc<Cell<f32>>,
    pub viewport: Rc<Cell<[f32; 4]>>,
    pub needs_fit: Rc<Cell<bool>>,
    pub focus: Option<u32>,
    pub neighbors: Vec<u32>,
    pub active: Option<u32>,
    pub tokens: Tokens,
    pub frame: Rc<Cell<FrameTimes>>,
    pub label_widths: Rc<RefCell<Vec<f32>>>,
    pub label_scale: f32,
    pub node_scale: f32,
    pub born: Option<crate::core::Born>,
    pub drawn: Rc<Cell<bool>>,
    pub painted: Rc<Cell<u64>>,
    pub probe: Rc<RefCell<Option<crate::probe::Probe>>>,
}

fn is_pictograph(c: char) -> bool {
    matches!(c as u32, 0x2190..=0x2BFF | 0x1F000..=0x1FAFF | 0xFE0E | 0xFE0F | 0x200D | 0x20E3)
}

pub(crate) fn clean_label(raw: &str) -> String {
    raw.chars()
        .filter(|&c| !is_pictograph(c))
        .collect::<String>()
        .trim()
        .to_string()
}

fn label_text(graph: &Graph, i: u32) -> (SharedString, bool) {
    let node = &graph.nodes[i as usize];
    let mono = matches!(node.kind, NodeKind::Tag(_));
    let cleaned = clean_label(&node.label);
    let text: &str = &cleaned;
    let short = match text.char_indices().nth(LABEL_CHARS) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    };
    (short.into(), mono)
}

fn label_run(len: usize, mono: bool, color: Hsla) -> TextRun {
    TextRun {
        len,
        font: font(if mono { MONO } else { SANS }),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn pt(p: [f32; 2], origin: [f32; 2]) -> gpui::Point<Pixels> {
    point(px(p[0] + origin[0]), px(p[1] + origin[1]))
}

fn triangle(
    path: &mut Option<Path<Pixels>>,
    a: gpui::Point<Pixels>,
    b: gpui::Point<Pixels>,
    c: gpui::Point<Pixels>,
) {
    let st = point(0., 1.);
    path.get_or_insert_with(|| Path::new(a))
        .push_triangle((a, b, c), (st, st, st));
}

fn segment(
    path: &mut Option<Path<Pixels>>,
    a: [f32; 2],
    b: [f32; 2],
    width: f32,
    origin: [f32; 2],
) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return;
    }
    let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
    let p0 = pt([a[0] + nx, a[1] + ny], origin);
    let p1 = pt([b[0] + nx, b[1] + ny], origin);
    let p2 = pt([b[0] - nx, b[1] - ny], origin);
    let p3 = pt([a[0] - nx, a[1] - ny], origin);
    triangle(path, p0, p1, p2);
    triangle(path, p0, p2, p3);
}

fn arrow(
    path: &mut Option<Path<Pixels>>,
    from: [f32; 2],
    to: [f32; 2],
    gap: f32,
    origin: [f32; 2],
) {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < gap + ARROW * 2.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let tip = [to[0] - ux * gap, to[1] - uy * gap];
    let base = [tip[0] - ux * ARROW, tip[1] - uy * ARROW];
    let half = ARROW / 2.0;
    triangle(
        path,
        pt(tip, origin),
        pt([base[0] - uy * half, base[1] + ux * half], origin),
        pt([base[0] + uy * half, base[1] - ux * half], origin),
    );
}

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    if t <= 0.0 {
        return a;
    }
    let (a, b) = (a.to_rgb(), b.to_rgb());
    let at = |x: f32, y: f32| x + (y - x) * t;
    Hsla::from(Rgba {
        r: at(a.r, b.r),
        g: at(a.g, b.g),
        b: at(a.b, b.b),
        a: at(a.a, b.a),
    })
}

fn edge_color(t: &Tokens, style: EdgeStyle) -> Hsla {
    match style {
        EdgeStyle::Plain => t.line_2,
        EdgeStyle::Mutual => t.ink_4,
        EdgeStyle::Dim => t.line_2.opacity(DIM),
        EdgeStyle::Hot => t.ink_3,
    }
}

fn paint_edges(edges: &[EdgeDraw], t: &Tokens, origin: [f32; 2], window: &mut Window) {
    const STYLES: [EdgeStyle; 4] = [
        EdgeStyle::Dim,
        EdgeStyle::Plain,
        EdgeStyle::Mutual,
        EdgeStyle::Hot,
    ];
    let mut paths: [Option<Path<Pixels>>; 4] = Default::default();
    let mut fresh: [[Option<Path<Pixels>>; GLOW_STEPS]; 4] = Default::default();
    let mut heads: [Option<Path<Pixels>>; 4] = Default::default();
    for e in edges {
        let style = match e.style {
            EdgeStyle::Dim => 0,
            EdgeStyle::Plain => 1,
            EdgeStyle::Mutual => 2,
            EdgeStyle::Hot => 3,
        };
        let step = (e.glow * GLOW_STEPS as f32).round() as usize;
        let path = match step {
            0 if e.reach >= 1.0 => &mut paths[style],
            _ => &mut fresh[style][step.clamp(1, GLOW_STEPS) - 1],
        };
        let reach = e.reach.clamp(0.0, 1.0);
        let end = [
            e.a[0] + (e.b[0] - e.a[0]) * reach,
            e.a[1] + (e.b[1] - e.a[1]) * reach,
        ];
        segment(path, e.a, end, 1.0, origin);
        if e.arrow_b {
            arrow(&mut heads[style], e.a, e.b, 12.0, origin);
        }
        if e.arrow_a {
            arrow(&mut heads[style], e.b, e.a, 12.0, origin);
        }
    }
    for (style, (path, heads)) in STYLES.iter().zip(paths.into_iter().zip(heads)) {
        if let Some(path) = path {
            window.paint_path(path, edge_color(t, *style));
        }
        if let Some(heads) = heads {
            window.paint_path(heads, t.ink_3);
        }
    }
    for (style, steps) in STYLES.iter().zip(fresh) {
        let rest = edge_color(t, *style);
        for (step, path) in steps.into_iter().enumerate() {
            if let Some(path) = path {
                window.paint_path(
                    path,
                    mix(rest, t.ink_2, (step + 1) as f32 / GLOW_STEPS as f32),
                );
            }
        }
    }
}

fn paint_frame(
    s: &Snapshot,
    plan: &Plan,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let t = &s.tokens;
    let origin = [f32::from(bounds.origin.x), f32::from(bounds.origin.y)];
    window.paint_layer(bounds, |window| paint_edges(&plan.edges, t, origin, window));
    window.paint_layer(bounds, |window| paint_nodes(plan, t, origin, window));
    window.paint_layer(bounds, |window| {
        paint_labels(s, plan, t, origin, window, cx)
    });
}

fn paint_nodes(plan: &Plan, t: &Tokens, origin: [f32; 2], window: &mut Window) {
    for d in &plan.nodes {
        let r = d.radius;
        let dim = d.emphasis == Emphasis::Dim;
        let base = if d.emphasis == Emphasis::Hot {
            t.ink
        } else {
            mix(t.ink_2, t.ink, d.glow)
        };
        let fill = if d.active {
            t.btn
        } else if dim {
            base.opacity(DIM)
        } else {
            base
        }
        .opacity(d.opacity);
        let at = pt(d.at, origin);
        if d.focused || d.active {
            let ring = r + 3.0;
            let color = if d.active && !d.focused { t.btn } else { t.ink }.opacity(d.opacity);
            let ring_bounds = Bounds::new(
                point(at.x - px(ring), at.y - px(ring)),
                size(px(ring * 2.0), px(ring * 2.0)),
            );
            window.paint_quad(quad(
                ring_bounds,
                px(ring),
                transparent_black(),
                px(1.),
                color,
                BorderStyle::Solid,
            ));
        }
        let (half, radius) = match d.shape {
            Shape::Square => (r * 0.886, 0.0),
            Shape::Disc | Shape::Ring => (r, r),
        };
        let b = Bounds::new(
            point(at.x - px(half), at.y - px(half)),
            size(px(half * 2.0), px(half * 2.0)),
        );
        match d.shape {
            Shape::Ring => {
                let color = if dim {
                    t.ink_4.opacity(DIM)
                } else if d.emphasis == Emphasis::Hot {
                    t.ink
                } else {
                    mix(t.ink_4, t.ink, d.glow)
                }
                .opacity(d.opacity);
                window.paint_quad(quad(
                    b,
                    px(radius),
                    transparent_black(),
                    px(1.),
                    color,
                    BorderStyle::Solid,
                ));
            }
            _ => window.paint_quad(quad(
                b,
                px(radius),
                fill,
                px(0.),
                transparent_black(),
                BorderStyle::Solid,
            )),
        }
    }
}

fn paint_labels(
    s: &Snapshot,
    plan: &Plan,
    t: &Tokens,
    origin: [f32; 2],
    window: &mut Window,
    cx: &mut App,
) {
    let text = window.text_system().clone();
    for l in &plan.labels {
        let (label, mono) = label_text(&s.graph, l.index);
        let color = if l.hot {
            t.ink
        } else if l.dim {
            t.ink_2.opacity(DIM)
        } else {
            t.ink_2
        }
        .opacity(l.opacity);
        let run = label_run(label.len(), mono, color);
        let line = text.shape_line(label, px(LABEL_SIZE), &[run], None);
        let width = f32::from(line.width);
        let x = l.at[0] - width / 2.0;
        let plate = Bounds::new(
            pt([x - PLATE_PAD, l.at[1]], origin),
            size(px(width + 2.0 * PLATE_PAD), px(LABEL_HEIGHT)),
        );
        window.paint_quad(quad(
            plate,
            px(0.),
            t.bg.opacity(if l.dim { DIM } else { 0.85 } * l.opacity),
            px(0.),
            transparent_black(),
            BorderStyle::Solid,
        ));
        let _ = line.paint(pt([x, l.at[1]], origin), px(LABEL_HEIGHT), window, cx);
    }
}

pub(crate) fn graph_canvas(s: Snapshot) -> impl IntoElement {
    canvas(
        move |bounds, window, _cx| {
            let started = Instant::now();
            let size = [f32::from(bounds.size.width), f32::from(bounds.size.height)];
            s.viewport.set([
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                size[0],
                size[1],
            ]);
            if s.needs_fit.get() && !s.positions.is_empty() && size[0] > 0.0 {
                let camera = Camera::fit(
                    crate::core::extent(&s.positions, s.born.as_ref()),
                    size,
                    48.0,
                );
                s.camera.set(camera);
                s.fit_zoom.set(camera.zoom);
                s.needs_fit.set(false);
            }
            let text = window.text_system().clone();
            let (graph, widths) = (s.graph.clone(), s.label_widths.clone());
            let measure = move |i: u32| {
                let mut widths = widths.borrow_mut();
                match widths.get(i as usize) {
                    Some(w) if !w.is_nan() => *w,
                    _ => {
                        let (label, mono) = label_text(&graph, i);
                        let w = f32::from(
                            text.layout_line(
                                &label,
                                px(LABEL_SIZE),
                                &[label_run(label.len(), mono, transparent_black())],
                                None,
                            )
                            .width,
                        );
                        if let Some(slot) = widths.get_mut(i as usize) {
                            *slot = w;
                        }
                        w
                    }
                }
            };
            let p = plan(&Inputs {
                graph: &s.graph,
                table: &s.table,
                positions: &s.positions,
                index: &s.index,
                camera: s.camera.get(),
                fit_zoom: s.fit_zoom.get(),
                size,
                focus: s.focus,
                neighbors: &s.neighbors,
                active: s.active,
                label_width: &measure,
                label_scale: s.label_scale,
                node_scale: s.node_scale,
                born: s.born.as_ref(),
            });
            let plan_ms = started.elapsed().as_secs_f32() * 1000.0;
            if let Some(probe) = s.probe.borrow_mut().as_mut() {
                probe.record(
                    Instant::now(),
                    s.camera.get(),
                    &p,
                    s.born.as_ref(),
                    &s.table,
                    s.node_scale,
                    size,
                );
            }
            (s, p, plan_ms)
        },
        move |bounds, (s, plan, plan_ms), window, cx| {
            let started = Instant::now();
            s.drawn.set(true);
            s.painted.set(s.painted.get() + 1);
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                paint_frame(&s, &plan, bounds, window, cx)
            });
            s.frame.set(FrameTimes {
                plan_ms,
                paint_ms: started.elapsed().as_secs_f32() * 1000.0,
                nodes: plan.nodes.len(),
                edges: plan.edges.len(),
                labels: plan.labels.len(),
            });
        },
    )
    .size_full()
}
