mod build;
mod layout;
mod spatial;
mod store;

use focus_core::{NoteId, Vault};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Note(NoteId),
    Tag(String),
    Unresolved(String),
    Attachment(Arc<Path>),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub label: Arc<str>,
    pub degree: u32,
    pub group: u16,
    pub path: Option<Arc<Path>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    Link,
    Embed,
    Tagged,
    TagParent,
    Relation,
}

#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub a: u32,
    pub b: u32,
    pub kind: EdgeKind,
    pub mutual: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub groups: Vec<String>,
}

impl Graph {
    pub fn neighbors(&self, node: u32) -> Vec<u32> {
        let mut out: Vec<u32> = self
            .edges
            .iter()
            .filter_map(|e| {
                if e.a == node {
                    Some(e.b)
                } else if e.b == node {
                    Some(e.a)
                } else {
                    None
                }
            })
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
    pub fn node_of_note(&self, note: NoteId) -> Option<u32> {
        self.nodes
            .iter()
            .position(|n| n.kind == NodeKind::Note(note))
            .map(|i| i as u32)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub tags: bool,
    pub attachments: bool,
    pub unresolved: bool,
    pub orphans: bool,
    pub only: Option<Vec<NoteId>>,
    pub around: Option<(NoteId, u8)>,
    pub hidden_folders: Vec<String>,
    pub relations: Vec<(PathBuf, PathBuf)>,
}

impl Default for Spec {
    fn default() -> Self {
        Spec {
            tags: false,
            attachments: false,
            unresolved: false,
            orphans: true,
            only: None,
            around: None,
            hidden_folders: Vec::new(),
            relations: Vec::new(),
        }
    }
}

pub fn build(vault: &Vault, spec: &Spec) -> Graph {
    build::build(vault, spec)
}

pub use build::Changes;

pub fn update(vault: &Vault, spec: &Spec, previous: &Graph, changes: &Changes) -> Option<Graph> {
    build::update(vault, spec, previous, changes)
}

#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub repulsion: f32,
    pub attraction: f32,
    pub link_distance: f32,
    pub gravity: f32,
    pub theta: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            repulsion: 1.0,
            attraction: 1.0,
            link_distance: 30.0,
            gravity: 0.05,
            theta: 0.9,
        }
    }
}

pub struct Simulation {
    inner: layout::Layout,
}

impl Simulation {
    pub fn new(graph: &Graph, seed: &[(u32, [f32; 2])], params: Params) -> Simulation {
        Simulation {
            inner: layout::Layout::new(graph, seed, params),
        }
    }
    pub fn step(&mut self, iterations: u32) -> f32 {
        self.inner.step(iterations)
    }
    pub fn is_settled(&self) -> bool {
        self.inner.is_settled()
    }
    pub fn positions(&self) -> &[[f32; 2]] {
        self.inner.positions()
    }
    pub fn pin(&mut self, node: u32, position: Option<[f32; 2]>) {
        self.inner.pin(node, position)
    }
    pub fn reheat(&mut self, params: Params) {
        self.inner.reheat(params)
    }
    pub fn grow(&mut self, order: Vec<u32>) {
        self.inner.grow(order)
    }
    pub fn grow_to(&mut self, count: usize) {
        self.inner.grow_to(count)
    }
    pub fn end_growth(&mut self) {
        self.inner.end_growth()
    }
    pub fn born(&self) -> Option<usize> {
        self.inner.born()
    }
    pub fn bounds(&self) -> [f32; 4] {
        layout::bounds(self.inner.positions())
    }
}

pub struct SpatialIndex {
    inner: spatial::Grid,
}

impl SpatialIndex {
    pub fn new(positions: &[[f32; 2]]) -> SpatialIndex {
        SpatialIndex {
            inner: spatial::Grid::new(positions),
        }
    }
    pub fn nearest(&self, point: [f32; 2], radius: f32) -> Option<u32> {
        self.inner.nearest(point, radius)
    }
    pub fn in_rect(&self, rect: [f32; 4], out: &mut Vec<u32>) {
        self.inner.in_rect(rect, out)
    }
}

pub fn node_radius(degree: u32) -> f32 {
    layout::radius_of(degree)
}

pub fn load_positions(cache_dir: &Path, graph: &Graph) -> Vec<(u32, [f32; 2])> {
    store::load(cache_dir, graph)
}
pub fn save_positions(
    cache_dir: &Path,
    graph: &Graph,
    positions: &[[f32; 2]],
) -> anyhow::Result<()> {
    store::save(cache_dir, graph, positions)
}

#[cfg(test)]
mod tests;
