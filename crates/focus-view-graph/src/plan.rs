use crate::camera::Camera;
use crate::replay::{glow as glow_of, growth as growth_of};
use focus_graph::{Graph, NodeKind, SpatialIndex};
use std::collections::HashMap;

const EDGE_BUDGET: usize = 40_000;
const DENSE_NODES: usize = 30_000;
const DENSE_CELL: f32 = 3.0;
const LABEL_BUDGET: usize = 250;
const HOT_LABEL_BUDGET: usize = 40;
pub(crate) const LABEL_HEIGHT: f32 = 18.0;
pub(crate) const PLATE_PAD: f32 = 2.0;
const LABEL_GAP: f32 = 4.0;
const MIN_CHAR_WIDTH: f32 = 3.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    Disc,
    Square,
    Ring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Emphasis {
    Normal,
    Dim,
    Hot,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NodeDraw {
    pub index: u32,
    pub at: [f32; 2],
    pub radius: f32,
    pub opacity: f32,
    pub shape: Shape,
    pub emphasis: Emphasis,
    pub active: bool,
    pub focused: bool,
    pub glow: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EdgeStyle {
    Plain,
    Mutual,
    Dim,
    Hot,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EdgeDraw {
    pub index: u32,
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub style: EdgeStyle,
    pub reach: f32,
    pub glow: f32,
    pub arrow_b: bool,
    pub arrow_a: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LabelDraw {
    pub index: u32,
    pub at: [f32; 2],
    pub hot: bool,
    pub dim: bool,
    pub opacity: f32,
}

#[derive(Debug, Default)]
pub(crate) struct Plan {
    pub nodes: Vec<NodeDraw>,
    pub edges: Vec<EdgeDraw>,
    pub labels: Vec<LabelDraw>,
}

pub(crate) struct NodeTable {
    pub degree: Vec<u32>,
    pub shape: Vec<Shape>,
}

impl NodeTable {
    pub fn new(graph: &Graph) -> NodeTable {
        NodeTable {
            degree: graph.nodes.iter().map(|n| n.degree).collect(),
            shape: graph.nodes.iter().map(|n| shape(&n.kind)).collect(),
        }
    }
}

pub(crate) struct Inputs<'a> {
    pub graph: &'a Graph,
    pub table: &'a NodeTable,
    pub positions: &'a [[f32; 2]],
    pub index: &'a SpatialIndex,
    pub camera: Camera,
    pub fit_zoom: f32,
    pub size: [f32; 2],
    pub focus: Option<u32>,
    pub neighbors: &'a [u32],
    pub active: Option<u32>,
    pub label_width: &'a dyn Fn(u32) -> f32,
    pub label_scale: f32,
    pub node_scale: f32,
    pub born: Option<&'a crate::core::Born>,
}

pub(crate) fn radius(degree: u32, zoom: f32, scale: f32) -> f32 {
    let design = 2.5 + 6.5 * ((degree as f32).sqrt() / 8.0).min(1.0);
    let room = focus_graph::node_radius(degree) * zoom;
    (design.min(room) * scale).max(1.0)
}

fn shape(kind: &NodeKind) -> Shape {
    match kind {
        NodeKind::Tag(_) => Shape::Square,
        NodeKind::Unresolved(_) => Shape::Ring,
        NodeKind::Note(_) | NodeKind::Attachment(_) => Shape::Disc,
    }
}

fn label_min_degree(zoom: f32, fit_zoom: f32, nodes: usize) -> f32 {
    let rel = (zoom / fit_zoom.max(f32::EPSILON)).max(0.01);
    (nodes as f32 / 60.0).clamp(2.0, 40.0) / (rel * rel)
}

pub(crate) fn plan(i: &Inputs) -> Plan {
    let mut out = Plan::default();
    let n = i
        .positions
        .len()
        .min(i.graph.nodes.len())
        .min(i.table.degree.len());
    if n == 0 {
        return out;
    }
    let view = i.camera.visible(i.size, 12.0);
    let mut visible = Vec::new();
    i.index.in_rect(view, &mut visible);
    visible.retain(|&v| (v as usize) < n && i.born.is_none_or(|b| b.shows(v)));
    if visible.len() > n / 4 {
        visible.sort_unstable();
    }
    let is_hot = |v: u32| i.focus == Some(v) || i.neighbors.binary_search(&v).is_ok();
    let growth = |v: u32| i.born.map_or(1.0, |b| b.growth(v));

    let cols = (i.size[0] / DENSE_CELL) as usize + 1;
    let rows = (i.size[1] / DENSE_CELL) as usize + 1;
    let mut occupied: Option<Vec<u64>> =
        (visible.len() > DENSE_NODES).then(|| vec![0u64; (cols * rows).div_ceil(64)]);
    let mut in_view = vec![0u64; n.div_ceil(64)];
    for &v in &visible {
        in_view[v as usize / 64] |= 1 << (v % 64);
    }
    let seen = |v: usize| in_view[v / 64] & (1 << (v % 64)) != 0;
    for &v in &visible {
        let degree = i.table.degree[v as usize];
        let at = i.camera.to_screen(i.positions[v as usize], i.size);
        let hot = is_hot(v);
        if let Some(bits) = occupied.as_mut()
            && degree < 8
            && !hot
        {
            let cell = (at[1].max(0.0) / DENSE_CELL) as usize * cols
                + (at[0].max(0.0) / DENSE_CELL) as usize;
            let (word, bit) = (cell / 64, 1u64 << (cell % 64));
            if word >= bits.len() || bits[word] & bit != 0 {
                continue;
            }
            bits[word] |= bit;
        }
        let emphasis = match i.focus {
            None => Emphasis::Normal,
            Some(_) if hot => Emphasis::Hot,
            Some(_) => Emphasis::Dim,
        };
        let grown = growth(v);
        let glow = match (i.focus, i.born) {
            (None, Some(born)) => born.glow(v),
            _ => 0.0,
        };
        out.nodes.push(NodeDraw {
            index: v,
            at,
            radius: radius(degree, i.camera.zoom, i.node_scale) * grown,
            opacity: grown,
            shape: i.table.shape[v as usize],
            emphasis,
            active: i.active == Some(v),
            focused: i.focus == Some(v),
            glow,
        });
    }

    let mut candidates: Vec<(u32, usize)> = Vec::new();
    for (k, e) in i.graph.edges.iter().enumerate() {
        let (a, b) = (e.a as usize, e.b as usize);
        if a >= n
            || b >= n
            || !(seen(a) || seen(b))
            || i.born
                .is_some_and(|born| !born.shows(e.a) || !born.shows(e.b))
        {
            continue;
        }
        let weight = i.table.degree[a].min(i.table.degree[b]);
        candidates.push((weight, k));
    }
    let cutoff = if candidates.len() > EDGE_BUDGET {
        let mut histogram = [0usize; 65];
        for (w, _) in &candidates {
            histogram[(*w as usize).min(64)] += 1;
        }
        let mut kept = 0;
        let mut cutoff = 65;
        while cutoff > 0 && kept + histogram[cutoff - 1] <= EDGE_BUDGET {
            cutoff -= 1;
            kept += histogram[cutoff];
        }
        cutoff as u32
    } else {
        0
    };
    for (w, k) in candidates {
        let e = &i.graph.edges[k];
        let hot = i.focus.is_some_and(|f| e.a == f || e.b == f);
        if w < cutoff && !hot {
            continue;
        }
        let style = match (i.focus, hot) {
            (_, true) => EdgeStyle::Hot,
            (Some(_), false) => EdgeStyle::Dim,
            (None, _) if e.mutual => EdgeStyle::Mutual,
            (None, _) => EdgeStyle::Plain,
        };
        let (older, younger, arrow_b, arrow_a) = match i.born {
            Some(born) if born.rank(e.a) > born.rank(e.b) => (e.b, e.a, hot && e.mutual, hot),
            _ => (e.a, e.b, hot, hot && e.mutual),
        };
        let age = i.born.and_then(|born| born.age(younger));
        let lit = matches!(style, EdgeStyle::Plain | EdgeStyle::Mutual);
        out.edges.push(EdgeDraw {
            index: k as u32,
            a: i.camera.to_screen(i.positions[older as usize], i.size),
            b: i.camera.to_screen(i.positions[younger as usize], i.size),
            style,
            reach: age.map_or(1.0, growth_of),
            glow: age.filter(|_| lit).map_or(0.0, glow_of),
            arrow_b,
            arrow_a,
        });
    }

    let min_degree = label_min_degree(i.camera.zoom, i.fit_zoom, n) * i.label_scale;
    let mut wanted: Vec<&NodeDraw> = out
        .nodes
        .iter()
        .filter(|d| {
            d.emphasis == Emphasis::Hot
                || d.active
                || i.table.degree[d.index as usize] as f32 >= min_degree
        })
        .collect();
    wanted.sort_by_key(|d| {
        (
            !d.focused,
            !d.active,
            d.emphasis != Emphasis::Hot,
            std::cmp::Reverse(i.table.degree[d.index as usize]),
        )
    });
    let mut placed = Placer::default();
    let mut hot_labels = 0;
    for d in wanted {
        if out.labels.len() >= LABEL_BUDGET {
            break;
        }
        let neighbour = d.emphasis == Emphasis::Hot && !d.focused;
        if neighbour && hot_labels >= HOT_LABEL_BUDGET {
            continue;
        }
        let top = d.at[1] + d.radius + 2.0;
        let width = (i.label_width)(d.index);
        let Some(center) = label_center(d.at[0], width, i.size[0]) else {
            continue;
        };
        let rect = |width: f32| {
            let half = width / 2.0 + PLATE_PAD + LABEL_GAP / 2.0;
            [
                center - half,
                top - LABEL_GAP / 2.0,
                center + half,
                top + LABEL_HEIGHT + LABEL_GAP / 2.0,
            ]
        };
        let chars = i.graph.nodes[d.index as usize]
            .label
            .chars()
            .take(64)
            .count();
        let least = rect(chars as f32 * MIN_CHAR_WIDTH);
        if least[2] < 0.0
            || least[0] > i.size[0]
            || least[1] > i.size[1]
            || least[3] < 0.0
            || !placed.fits(least)
        {
            continue;
        }
        if placed.try_place(rect(width)) {
            hot_labels += neighbour as usize;
            out.labels.push(LabelDraw {
                index: d.index,
                at: [center, top],
                hot: d.emphasis == Emphasis::Hot,
                dim: d.emphasis == Emphasis::Dim,
                opacity: d.opacity,
            });
        }
    }
    out
}

fn label_center(x: f32, width: f32, room: f32) -> Option<f32> {
    let half = width / 2.0 + PLATE_PAD;
    (2.0 * half <= room).then(|| x.clamp(half, room - half))
}

#[derive(Default)]
struct Placer {
    cells: HashMap<(i32, i32), Vec<[f32; 4]>>,
}

fn span(lo: f32, hi: f32) -> std::ops::RangeInclusive<i32> {
    ((lo / 64.0).floor() as i32)..=((hi / 64.0).floor() as i32)
}

impl Placer {
    fn fits(&self, r: [f32; 4]) -> bool {
        span(r[1], r[3]).all(|cy| {
            span(r[0], r[2]).all(|cx| {
                self.cells.get(&(cx, cy)).is_none_or(|list| {
                    !list
                        .iter()
                        .any(|o| r[0] < o[2] && o[0] < r[2] && r[1] < o[3] && o[1] < r[3])
                })
            })
        })
    }

    fn try_place(&mut self, r: [f32; 4]) -> bool {
        if !self.fits(r) {
            return false;
        }
        for cx in span(r[0], r[2]) {
            for cy in span(r[1], r[3]) {
                self.cells.entry((cx, cy)).or_default().push(r);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_stays_inside_the_view() {
        let half = 50.0 + PLATE_PAD;
        assert_eq!(label_center(10.0, 100.0, 300.0), Some(half));
        assert_eq!(label_center(295.0, 100.0, 300.0), Some(300.0 - half));
        assert_eq!(label_center(150.0, 100.0, 300.0), Some(150.0));
        assert_eq!(label_center(150.0, 400.0, 300.0), None);
    }
    use focus_graph::{Edge, EdgeKind, Node};
    use std::sync::Arc;

    fn star(leaves: u32) -> (Graph, Vec<[f32; 2]>) {
        let mut g = Graph::default();
        let mut pos = Vec::new();
        for i in 0..=leaves {
            g.nodes.push(Node {
                kind: NodeKind::Tag(i.to_string()),
                label: Arc::from(format!("n{i}")),
                degree: if i == 0 { leaves } else { 1 },
                group: 0,
                path: None,
            });
            let a = i as f32 * 0.7;
            pos.push(if i == 0 {
                [0.0, 0.0]
            } else {
                [100.0 * a.cos(), 100.0 * a.sin()]
            });
        }
        for i in 1..=leaves {
            g.edges.push(Edge {
                a: i,
                b: 0,
                kind: EdgeKind::Link,
                mutual: i % 2 == 0,
            });
        }
        (g, pos)
    }

    fn inputs<'a>(
        g: &'a Graph,
        table: &'a NodeTable,
        pos: &'a [[f32; 2]],
        index: &'a SpatialIndex,
        camera: Camera,
        focus: Option<u32>,
        neighbors: &'a [u32],
    ) -> Inputs<'a> {
        Inputs {
            graph: g,
            table,
            positions: pos,
            index,
            camera,
            fit_zoom: 1.0,
            size: [800.0, 600.0],
            focus,
            neighbors,
            active: None,
            label_width: &|_| 40.0,
            label_scale: 1.0,
            node_scale: 1.0,
            born: None,
        }
    }

    #[test]
    fn culls_and_dims_around_focus() {
        let (g, pos) = star(20);
        let table = NodeTable::new(&g);
        let index = SpatialIndex::new(&pos);
        let all = plan(&inputs(
            &g,
            &table,
            &pos,
            &index,
            Camera {
                center: [0.0, 0.0],
                zoom: 2.0,
            },
            None,
            &[],
        ));
        assert_eq!(all.nodes.len(), 21);
        assert_eq!(all.edges.len(), 20);
        assert!(all.edges.iter().any(|e| e.style == EdgeStyle::Mutual));
        let far = plan(&inputs(
            &g,
            &table,
            &pos,
            &index,
            Camera {
                center: [5000.0, 5000.0],
                zoom: 2.0,
            },
            None,
            &[],
        ));
        assert!(far.nodes.is_empty() && far.edges.is_empty());
        let neighbors = [0u32];
        let hovered = plan(&inputs(
            &g,
            &table,
            &pos,
            &index,
            Camera {
                center: [0.0, 0.0],
                zoom: 2.0,
            },
            Some(3),
            &neighbors,
        ));
        let hot: Vec<u32> = hovered
            .nodes
            .iter()
            .filter(|d| d.emphasis == Emphasis::Hot)
            .map(|d| d.index)
            .collect();
        assert_eq!(hot.len(), 2);
        assert_eq!(
            hovered
                .edges
                .iter()
                .filter(|e| e.style == EdgeStyle::Hot)
                .count(),
            1
        );
        assert!(
            hovered
                .edges
                .iter()
                .filter(|e| e.style == EdgeStyle::Hot)
                .all(|e| e.arrow_b)
        );
        assert_eq!(
            hovered.labels[0].index, 3,
            "the focused node is labelled first"
        );
    }

    #[test]
    fn labels_never_overlap_and_follow_degree() {
        let (g, pos) = star(200);
        let table = NodeTable::new(&g);
        let index = SpatialIndex::new(&pos);
        let p = plan(&inputs(
            &g,
            &table,
            &pos,
            &index,
            Camera {
                center: [0.0, 0.0],
                zoom: 8.0,
            },
            None,
            &[],
        ));
        assert!(!p.labels.is_empty());
        for (k, a) in p.labels.iter().enumerate() {
            for b in &p.labels[k + 1..] {
                let overlap =
                    (a.at[0] - b.at[0]).abs() < 40.0 && (a.at[1] - b.at[1]).abs() < LABEL_HEIGHT;
                assert!(!overlap, "{a:?} overlaps {b:?}");
            }
        }
        let overview = plan(&inputs(
            &g,
            &table,
            &pos,
            &index,
            Camera {
                center: [0.0, 0.0],
                zoom: 1.0,
            },
            None,
            &[],
        ));
        assert_eq!(
            overview.labels.iter().map(|l| l.index).collect::<Vec<_>>(),
            [0],
            "only the hub at overview"
        );
    }

    #[test]
    fn hovered_hub_labels_its_neighbours_apart_and_capped() {
        let (mut g, pos) = star(200);
        for (k, n) in g.nodes.iter_mut().enumerate().skip(1) {
            n.degree = 1 + (k % 7) as u32;
        }
        let table = NodeTable::new(&g);
        let index = SpatialIndex::new(&pos);
        let neighbors: Vec<u32> = (1..=200).collect();
        let p = plan(&inputs(
            &g,
            &table,
            &pos,
            &index,
            Camera {
                center: [0.0, 0.0],
                zoom: 4.0,
            },
            Some(0),
            &neighbors,
        ));
        assert_eq!(p.labels[0].index, 0);
        let hot: Vec<&LabelDraw> = p.labels.iter().filter(|l| l.hot && l.index != 0).collect();
        assert!(
            !hot.is_empty() && hot.len() <= HOT_LABEL_BUDGET,
            "{}",
            hot.len()
        );
        let width = |_: &LabelDraw| 40.0 + 2.0 * PLATE_PAD;
        for (k, a) in p.labels.iter().enumerate() {
            for b in &p.labels[k + 1..] {
                let apart_x =
                    (a.at[0] - b.at[0]).abs() >= (width(a) + width(b)) / 2.0 + LABEL_GAP - 0.01;
                let apart_y = (a.at[1] - b.at[1]).abs() >= LABEL_HEIGHT + LABEL_GAP - 0.01;
                assert!(apart_x || apart_y, "{a:?} touches {b:?}");
            }
        }
        let degrees: Vec<u32> = hot
            .iter()
            .map(|l| g.nodes[l.index as usize].degree)
            .collect();
        assert!(
            degrees.windows(2).all(|w| w[0] >= w[1]),
            "most connected first: {degrees:?}"
        );
    }

    #[test]
    fn edge_budget_drops_weak_edges_first() {
        let mut g = Graph::default();
        let mut pos = Vec::new();
        let n = 50_000u32;
        for i in 0..n {
            g.nodes.push(Node {
                kind: NodeKind::Tag(String::new()),
                label: Arc::from(""),
                degree: if i < 100 { 100 } else { 2 },
                group: 0,
                path: None,
            });
            pos.push([(i % 300) as f32, (i / 300) as f32]);
        }
        for i in 100..n {
            g.edges.push(Edge {
                a: i,
                b: i % 100,
                kind: EdgeKind::Link,
                mutual: false,
            });
            g.edges.push(Edge {
                a: i,
                b: (i + 1) % n,
                kind: EdgeKind::Link,
                mutual: false,
            });
        }
        let table = NodeTable::new(&g);
        let index = SpatialIndex::new(&pos);
        let p = plan(&Inputs {
            size: [1600.0, 1000.0],
            ..inputs(
                &g,
                &table,
                &pos,
                &index,
                Camera {
                    center: [150.0, 83.0],
                    zoom: 4.0,
                },
                None,
                &[],
            )
        });
        assert!(p.edges.len() <= EDGE_BUDGET, "{}", p.edges.len());
    }
}

#[cfg(test)]
mod timing {
    use super::*;
    use focus_graph::{Edge, EdgeKind, Node};
    use std::sync::Arc;

    #[test]
    #[ignore]
    fn plan_cost() {
        let n = 100_000u32;
        let mut g = Graph::default();
        let mut pos = Vec::new();
        let mut state = 0x9E3779B97F4A7C15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for i in 0..n {
            g.nodes.push(Node {
                kind: NodeKind::Tag(String::new()),
                label: Arc::from(format!("note {i} with a title")),
                degree: 0,
                group: 0,
                path: None,
            });
            pos.push([(next() % 20_000) as f32, (next() % 12_000) as f32]);
        }
        for i in 0..200_000u32 {
            let (a, b) = ((next() % n as u64) as u32, (next() % n as u64) as u32);
            g.edges.push(Edge {
                a,
                b,
                kind: EdgeKind::Link,
                mutual: i % 7 == 0,
            });
            g.nodes[a as usize].degree += 1;
            g.nodes[b as usize].degree += 1;
        }
        let index = SpatialIndex::new(&pos);
        let fit = Camera::fit([0.0, 0.0, 20_000.0, 12_000.0], [1600.0, 1000.0], 48.0);
        for (name, zoom) in [("overview", fit.zoom), ("close-up", fit.zoom * 4.0)] {
            let camera = Camera {
                center: [10_000.0, 6_000.0],
                zoom,
            };
            let width = |_: u32| 80.0;
            let table = NodeTable::new(&g);
            let inputs = Inputs {
                graph: &g,
                table: &table,
                positions: &pos,
                index: &index,
                camera,
                fit_zoom: fit.zoom,
                size: [1600.0, 1000.0],
                focus: None,
                neighbors: &[],
                active: None,
                label_width: &width,
                label_scale: 1.0,
                node_scale: 1.0,
                born: None,
            };
            let t = std::time::Instant::now();
            let mut vis = Vec::new();
            for _ in 0..10 {
                index.in_rect(camera.visible([1600.0, 1000.0], 12.0), &mut vis);
            }
            eprintln!(
                "{name}: in_rect {:.2} ms, {} visible",
                t.elapsed().as_secs_f64() * 100.0,
                vis.len()
            );
            let t = std::time::Instant::now();
            let mut p = Plan::default();
            for _ in 0..10 {
                p = plan(&inputs);
            }
            eprintln!(
                "{name}: {:.2} ms per plan, {} nodes, {} edges, {} labels",
                t.elapsed().as_secs_f64() * 100.0,
                p.nodes.len(),
                p.edges.len(),
                p.labels.len()
            );
        }
    }
}
