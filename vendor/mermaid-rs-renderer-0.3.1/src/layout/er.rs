//! FOCUS: layout of entity relationship diagrams.
//!
//! The general layout packs entities so tightly that a relation label covers the boxes and
//! hides the cardinalities. Entities keep their sizes and are placed again in rows (breadth
//! first from the first entity) with room between rows for the label and the cardinality
//! symbols.

use std::collections::{BTreeMap, HashMap, VecDeque};

use super::types::{EdgeLayout, Layout, NodeLayout};
use crate::ir::Graph;
use crate::theme::Theme;

const PADDING: f32 = 24.0;
const NODE_GAP: f32 = 64.0;
/// Room kept at each end of a relation for its cardinality symbol.
const SYMBOL_SPACE: f32 = 30.0;
const PARALLEL_SPACING: f32 = 26.0;

pub(super) fn relayout_er(layout: &mut Layout, graph: &Graph, theme: &Theme) {
    if layout.nodes.is_empty() {
        return;
    }
    let order = |id: &str| graph.node_order.get(id).copied().unwrap_or(usize::MAX);
    let mut ids: Vec<String> = layout.nodes.keys().cloned().collect();
    ids.sort_by_key(|id| (order(id), id.clone()));

    let mut neighbors: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &layout.edges {
        if edge.from != edge.to {
            neighbors.entry(edge.from.as_str()).or_default().push(edge.to.as_str());
            neighbors.entry(edge.to.as_str()).or_default().push(edge.from.as_str());
        }
    }

    let mut rank: HashMap<&str, usize> = HashMap::new();
    for start in &ids {
        if rank.contains_key(start.as_str()) {
            continue;
        }
        rank.insert(start.as_str(), 0);
        let mut queue = VecDeque::from([start.as_str()]);
        while let Some(current) = queue.pop_front() {
            let next_rank = rank[current] + 1;
            for &next in neighbors.get(current).map(Vec::as_slice).unwrap_or(&[]) {
                if !rank.contains_key(next) {
                    rank.insert(next, next_rank);
                    queue.push_back(next);
                }
            }
        }
    }
    let rank_count = rank.values().copied().max().unwrap_or(0) + 1;
    let mut rows: Vec<Vec<&str>> = vec![Vec::new(); rank_count];
    for id in &ids {
        rows[rank[id.as_str()]].push(id.as_str());
    }

    // Crossing reduction: each row sorted by the mean position of its neighbours in the previous row.
    let mut column: HashMap<&str, f32> = HashMap::new();
    for (i, id) in rows[0].iter().enumerate() {
        column.insert(id, i as f32);
    }
    for r in 1..rank_count {
        let mut keyed: Vec<(f32, &str)> = rows[r]
            .iter()
            .enumerate()
            .map(|(i, &id)| {
                let above: Vec<f32> = neighbors
                    .get(id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .filter(|n| rank[**n] + 1 == r)
                    .filter_map(|n| column.get(n).copied())
                    .collect();
                let key = if above.is_empty() { i as f32 } else { above.iter().sum::<f32>() / above.len() as f32 };
                (key, id)
            })
            .collect();
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
        rows[r] = keyed.iter().map(|&(_, id)| id).collect();
        for (i, id) in rows[r].iter().enumerate() {
            column.insert(id, i as f32);
        }
    }

    let label_height = layout
        .edges
        .iter()
        .filter_map(|e| e.label.as_ref().map(|l| l.height))
        .fold(0.0_f32, f32::max);
    let rank_gap = (label_height + SYMBOL_SPACE * 2.0 + theme.font_size).max(96.0);

    let widths: Vec<f32> = rows
        .iter()
        .map(|row| row.iter().map(|id| layout.nodes[*id].width).sum::<f32>() + NODE_GAP * row.len().saturating_sub(1) as f32)
        .collect();
    let max_width = widths.iter().copied().fold(0.0_f32, f32::max);
    let mut y = PADDING;
    let mut placed: BTreeMap<String, (f32, f32)> = BTreeMap::new();
    for (r, row) in rows.iter().enumerate() {
        let mut x = PADDING + (max_width - widths[r]) / 2.0;
        let mut row_height = 0.0_f32;
        for id in row {
            let node = &layout.nodes[*id];
            placed.insert((*id).to_string(), (x, y));
            x += node.width + NODE_GAP;
            row_height = row_height.max(node.height);
        }
        y += row_height + rank_gap;
    }
    for (id, (x, y)) in &placed {
        if let Some(node) = layout.nodes.get_mut(id) {
            node.x = *x;
            node.y = *y;
        }
    }
    layout.width = max_width + PADDING * 2.0;
    layout.height = y - rank_gap + PADDING;

    // Edges: segment between box borders, offset when several relations join the same pair.
    let mut pair_total: HashMap<(String, String), usize> = HashMap::new();
    for edge in &layout.edges {
        *pair_total.entry(pair_key(edge)).or_default() += 1;
    }
    let mut pair_seen: HashMap<(String, String), usize> = HashMap::new();
    let nodes = &layout.nodes;
    for edge in &mut layout.edges {
        let (Some(from), Some(to)) = (nodes.get(&edge.from), nodes.get(&edge.to)) else { continue };
        if edge.from == edge.to {
            edge.points = self_loop(from);
            edge.label_anchor = Some((from.x + from.width + 70.0, from.y + from.height / 2.0));
            continue;
        }
        let key = pair_key(edge);
        let index = pair_seen.entry(key.clone()).or_default();
        let total = pair_total[&key];
        let shift = (*index as f32 - (total as f32 - 1.0) / 2.0) * PARALLEL_SPACING;
        *index += 1;
        let a = center(from);
        let b = center(to);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = (dx * dx + dy * dy).sqrt().max(1.0);
        let (px, py) = (-dy / length * shift, dx / length * shift);
        let start = border(from, (b.0 + px, b.1 + py), (a.0 + px, a.1 + py));
        let end = border(to, (a.0 + px, a.1 + py), (b.0 + px, b.1 + py));
        edge.points = vec![start, end];
        edge.label_anchor = Some(((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0));
    }
}

fn pair_key(edge: &EdgeLayout) -> (String, String) {
    if edge.from <= edge.to {
        (edge.from.clone(), edge.to.clone())
    } else {
        (edge.to.clone(), edge.from.clone())
    }
}

fn center(node: &NodeLayout) -> (f32, f32) {
    (node.x + node.width / 2.0, node.y + node.height / 2.0)
}

fn border(node: &NodeLayout, toward: (f32, f32), origin: (f32, f32)) -> (f32, f32) {
    let c = center(node);
    let (dx, dy) = (toward.0 - origin.0, toward.1 - origin.1);
    let exit = |delta: f32, offset: f32, half: f32| {
        if delta.abs() > 1e-3 { (half * delta.signum() - offset) / delta } else { f32::MAX }
    };
    let s = exit(dx, origin.0 - c.0, node.width / 2.0)
        .min(exit(dy, origin.1 - c.1, node.height / 2.0))
        .max(0.0);
    (origin.0 + dx * s, origin.1 + dy * s)
}

fn self_loop(node: &NodeLayout) -> Vec<(f32, f32)> {
    let right = node.x + node.width;
    vec![
        (right, node.y + node.height * 0.3),
        (right + 40.0, node.y + node.height * 0.3),
        (right + 40.0, node.y + node.height * 0.7),
        (right, node.y + node.height * 0.7),
    ]
}
