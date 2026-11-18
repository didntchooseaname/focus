use crate::camera::{Camera, Follow};
use crate::replay::{FRESH_SECONDS, Glide, Links, born_at, glow, growth, replay_seconds};
use crate::worker::Worker;
use focus_core::{NoteId, Vault};
use focus_graph::{
    Changes, EdgeKind, Graph, NodeKind, Params, Simulation, SpatialIndex, Spec, build,
    load_positions, update,
};
use focus_search::{CancelToken, SearchIndex, SearchOptions, parse_query};
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

const CLICK_SLOP: f32 = 3.0;
const FIT_PADDING: f32 = 48.0;

#[derive(Debug, Clone, Copy, Default)]
pub struct FrameTimes {
    pub plan_ms: f32,
    pub paint_ms: f32,
    pub nodes: usize,
    pub edges: usize,
    pub labels: usize,
}

enum Drag {
    Pan {
        last: [f32; 2],
        moved: f32,
    },
    Node {
        node: u32,
        start: [f32; 2],
        moved: f32,
        new_tab: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Pending {
    Changed(NoteId),
    Removed(NoteId),
    Everything,
}

fn changes_of(pending: &[Pending]) -> Option<Changes> {
    let mut last: HashMap<NoteId, bool> = HashMap::new();
    for p in pending {
        match *p {
            Pending::Changed(id) => last.insert(id, false),
            Pending::Removed(id) => last.insert(id, true),
            Pending::Everything => return None,
        };
    }
    let mut changes = Changes::default();
    for (id, removed) in last {
        if removed {
            changes.removed.push(id)
        } else {
            changes.changed.push(id)
        }
    }
    Some(changes)
}

pub(crate) struct BuildInput {
    pub vault: Arc<Vault>,
    pub search: Option<Arc<SearchIndex>>,
    pub query: Option<String>,
    pub spec: Spec,
    pub params: Params,
    pub cache_dir: Option<PathBuf>,
    pub previous: Arc<Graph>,
    pub previous_positions: Arc<Vec<[f32; 2]>>,
    pub changes: Option<Changes>,
    pub consumed: usize,
}

pub(crate) struct Built {
    pub graph: Arc<Graph>,
    pub table: Arc<crate::plan::NodeTable>,
    pub simulation: Simulation,
    pub spec: Spec,
    pub query: Option<String>,
    pub consumed: usize,
}

pub(crate) fn node_key(graph: &Graph, i: usize) -> Option<String> {
    let n = &graph.nodes[i];
    Some(match &n.kind {
        NodeKind::Note(_) => format!("n:{}", n.path.as_ref()?.to_string_lossy()),
        NodeKind::Attachment(p) => format!("a:{}", p.to_string_lossy()),
        NodeKind::Tag(t) => format!("t:{}", t.to_lowercase()),
        NodeKind::Unresolved(t) => format!("u:{}", t.to_lowercase()),
    })
}

pub(crate) fn build_graph(input: BuildInput) -> Built {
    let built_spec = input.spec.clone();
    let mut spec = input.spec;
    if let (Some(search), Some(query)) = (
        &input.search,
        input.query.as_deref().filter(|q| !q.trim().is_empty()),
    ) {
        let options = SearchOptions {
            max_files: usize::MAX,
            budget_ms: 5_000,
            max_matches_per_file: 0,
            ..Default::default()
        };
        let results = search.search(&parse_query(query), &options, &CancelToken::new());
        spec.only = Some(results.hits.iter().map(|h| h.note).collect());
    }
    let updated = input
        .changes
        .as_ref()
        .and_then(|c| update(&input.vault, &spec, &input.previous, c));
    let incremental = updated.is_some();
    let graph = Arc::new(updated.unwrap_or_else(|| build(&input.vault, &spec)));
    let cached = match &input.cache_dir {
        Some(dir) if spec.around.is_none() && !incremental => load_positions(dir, &graph),
        _ => Vec::new(),
    };
    let mut seed: HashMap<u32, [f32; 2]> = cached.into_iter().collect();
    let previous: HashMap<String, [f32; 2]> = input
        .previous_positions
        .iter()
        .enumerate()
        .take(input.previous.nodes.len())
        .filter_map(|(i, p)| Some((node_key(&input.previous, i)?, *p)))
        .collect();
    for i in 0..graph.nodes.len() {
        if let Some(p) = node_key(&graph, i).and_then(|k| previous.get(&k)) {
            seed.insert(i as u32, *p);
        }
    }
    let seed: Vec<(u32, [f32; 2])> = seed.into_iter().collect();
    Built {
        table: Arc::new(crate::plan::NodeTable::new(&graph)),
        simulation: Simulation::new(&graph, &seed, input.params),
        graph,
        spec: built_spec,
        query: input.query,
        consumed: input.consumed,
    }
}

#[derive(Clone, Default)]
pub(crate) struct Born {
    pub rank: Arc<[u32]>,
    pub count: u32,
    pub fresh: Arc<[(u32, f32)]>,
    pub glow: Arc<[(u32, f32)]>,
}

impl Born {
    pub fn shows(&self, node: u32) -> bool {
        self.rank
            .get(node as usize)
            .is_some_and(|&r| r < self.count)
    }

    pub fn rank(&self, node: u32) -> u32 {
        self.rank.get(node as usize).copied().unwrap_or(u32::MAX)
    }

    pub fn age(&self, node: u32) -> Option<f32> {
        let r = self.rank(node);
        match self.fresh.partition_point(|&(start, _)| start <= r) {
            0 => None,
            k => Some(self.fresh[k - 1].1),
        }
    }

    pub fn growth(&self, node: u32) -> f32 {
        self.age(node).map_or(1.0, growth)
    }

    pub fn glow(&self, node: u32) -> f32 {
        self.glow
            .binary_search_by_key(&node, |&(n, _)| n)
            .map_or(0.0, |k| self.glow[k].1)
    }
}

pub(crate) fn extent(pos: &[[f32; 2]], born: Option<&Born>) -> [f32; 4] {
    let grow = |b: [f32; 4], p: &[f32; 2]| {
        [
            b[0].min(p[0]),
            b[1].min(p[1]),
            b[2].max(p[0]),
            b[3].max(p[1]),
        ]
    };
    let empty = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    let Some(born) = born else {
        return pos.iter().fold(empty, grow);
    };
    let (b, all) = pos
        .iter()
        .enumerate()
        .fold((empty, empty), |(b, all), (i, p)| {
            let b = if born.shows(i as u32) { grow(b, p) } else { b };
            (b, grow(all, p))
        });
    if b[0] > b[2] {
        return b;
    }
    let least = REPLAY_MIN_EXTENT.min((all[2] - all[0]).max(all[3] - all[1])) / 2.0;
    let c = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
    let half = [
        ((b[2] - b[0]) / 2.0).max(least),
        ((b[3] - b[1]) / 2.0).max(least),
    ];
    [
        c[0] - half[0],
        c[1] - half[1],
        c[0] + half[0],
        c[1] + half[1],
    ]
}

const REPLAY_MIN_EXTENT: f32 = 1000.0;
const BIRTH_LEAD: f32 = 2.0 / 60.0;
const HIDDEN_TICKS: u8 = 6;
const REPLAY_MAX_STEP: f32 = 0.1;
const CAMERA_SMOOTH: f32 = 0.5;
const REPLAY_MAX_PAN: f32 = 180.0;
const CAMERA_MAX_STEP: f32 = 1.0 / 30.0;

pub(crate) struct Replay {
    pub born: Born,
    order: Arc<[u32]>,
    links: Links,
    seconds: f32,
    pub times: Vec<i64>,
    pub progress: f32,
    pub playing: bool,
    pub speed: f32,
    asked: u32,
    laid: u32,
    credit: f32,
    revealed: Option<Instant>,
    zoom: Option<f32>,
    births: VecDeque<(u32, Instant)>,
    last: Option<Instant>,
    unseen: u8,
}

impl Replay {
    fn arrive(&mut self, born: Option<usize>) -> Option<&[u32]> {
        let before = self.laid as usize;
        let laid = born?.min(self.asked as usize).max(before);
        self.laid = laid as u32;
        Some(&self.order[before..laid])
    }

    fn reveal(&mut self, now: Instant) {
        let total = self.order.len();
        let rate = total as f32 * self.speed / self.seconds;
        let dt = self.revealed.map_or(0.0, |then| {
            now.saturating_duration_since(then)
                .as_secs_f32()
                .min(REPLAY_MAX_STEP)
        });
        self.revealed = Some(now);
        let allowed = rate * dt;
        self.credit = (self.credit + allowed).min(allowed + 1.0);
        let due = born_at(self.progress, total) as u32;
        let ready = due.min(self.laid).saturating_sub(self.born.count);
        let shown = ready.min(self.credit as u32);
        if shown > 0 {
            self.births.push_back((self.born.count, now));
            self.born.count += shown;
            self.credit -= shown as f32;
        }
    }

    fn refresh(&mut self, now: Instant) -> bool {
        let age = |at: Instant| now.saturating_duration_since(at).as_secs_f32();
        while self
            .births
            .front()
            .is_some_and(|&(_, at)| age(at) >= FRESH_SECONDS)
        {
            self.births.pop_front();
        }
        self.born.fresh = self
            .births
            .iter()
            .map(|&(rank, at)| (rank, age(at)))
            .collect();
        let mut lit: Vec<(u32, f32)> = Vec::new();
        for (k, &(first, at)) in self.births.iter().enumerate() {
            let g = glow(age(at));
            let end = self.births.get(k + 1).map_or(self.born.count, |b| b.0);
            for r in first..end {
                let node = self.order[r as usize];
                let before = lit.len();
                lit.extend(
                    self.links
                        .of_node(node)
                        .iter()
                        .filter(|&&other| self.born.rank(other) < r)
                        .map(|&other| (other, g)),
                );
                if lit.len() > before {
                    lit.push((node, g));
                }
            }
        }
        lit.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)));
        lit.dedup_by_key(|l| l.0);
        self.born.glow = lit.into();
        !self.births.is_empty()
    }

    pub fn date(&self) -> Option<i64> {
        self.born
            .count
            .checked_sub(1)
            .and_then(|k| self.times.get(k as usize))
            .copied()
            .filter(|t| *t != i64::MAX)
    }
}

pub(crate) struct GraphCore {
    pub vault: Arc<Vault>,
    pub search: Option<Arc<SearchIndex>>,
    pub cache_dir: Option<PathBuf>,
    pub spec: Spec,
    pub params: Params,
    pub query: Option<String>,
    pub graph: Arc<Graph>,
    pub table: Arc<crate::plan::NodeTable>,
    worker: Option<Worker>,
    pub positions: Arc<Vec<[f32; 2]>>,
    version: u64,
    pub index: Arc<SpatialIndex>,
    pub camera: Rc<Cell<Camera>>,
    pub fit_zoom: Rc<Cell<f32>>,
    pub viewport: Rc<Cell<[f32; 4]>>,
    pub needs_fit: Rc<Cell<bool>>,
    pub auto_fit: bool,
    following: Follow,
    followed: Option<Instant>,
    pub refit_on_install: bool,
    pub hover: Option<u32>,
    pub selected: Option<u32>,
    pub neighbors: Vec<u32>,
    pub focus_links: (u32, u32),
    pub active: Option<NoteId>,
    pub building: bool,
    pub build_generation: u64,
    pub ticking: bool,
    pub frame_armed: bool,
    pub frame: Rc<Cell<FrameTimes>>,
    pub label_widths: Rc<std::cell::RefCell<Vec<f32>>>,
    pub node_scale: f32,
    drag: Option<Drag>,
    pub clicked: Option<(NoteId, bool)>,
    pub last_build: Option<Instant>,
    pub pending: Vec<Pending>,
    built_from: Option<(Spec, Option<String>)>,
    pub replay: Option<Replay>,
    glide: Option<Glide>,
    pub drawn: Rc<Cell<bool>>,
    pub painted: Rc<Cell<u64>>,
    pub probe: Rc<std::cell::RefCell<Option<crate::probe::Probe>>>,
}

impl GraphCore {
    pub fn new(
        vault: Arc<Vault>,
        search: Option<Arc<SearchIndex>>,
        cache_dir: Option<PathBuf>,
        spec: Spec,
        params: Params,
    ) -> GraphCore {
        GraphCore {
            vault,
            search,
            cache_dir,
            spec,
            params,
            query: None,
            graph: Arc::new(Graph::default()),
            table: Arc::new(crate::plan::NodeTable::new(&Graph::default())),
            worker: None,
            positions: Arc::new(Vec::new()),
            version: 0,
            index: Arc::new(SpatialIndex::new(&[])),
            camera: Rc::new(Cell::new(Camera::default())),
            fit_zoom: Rc::new(Cell::new(1.0)),
            viewport: Rc::new(Cell::new([0.0; 4])),
            needs_fit: Rc::new(Cell::new(true)),
            auto_fit: true,
            following: Follow::default(),
            followed: None,
            refit_on_install: false,
            hover: None,
            selected: None,
            neighbors: Vec::new(),
            focus_links: (0, 0),
            active: None,
            building: false,
            build_generation: 0,
            ticking: false,
            frame_armed: false,
            frame: Rc::new(Cell::new(FrameTimes::default())),
            label_widths: Rc::new(std::cell::RefCell::new(Vec::new())),
            node_scale: 1.0,
            drag: None,
            clicked: None,
            last_build: None,
            pending: Vec::new(),
            built_from: None,
            replay: None,
            glide: None,
            drawn: Rc::new(Cell::new(false)),
            painted: Rc::new(Cell::new(0)),
            probe: Rc::new(std::cell::RefCell::new(None)),
        }
    }

    pub fn start_replay(&mut self, order: Vec<u32>, times: Vec<i64>, links: Links, speed: f32) {
        let Some(worker) = &self.worker else { return };
        if order.len() != self.graph.nodes.len() {
            return;
        }
        let mut rank = vec![0u32; order.len()];
        for (k, &node) in order.iter().enumerate() {
            rank[node as usize] = k as u32;
        }
        worker.grow(order.clone());
        let seconds = replay_seconds(order.len());
        self.replay = Some(Replay {
            born: Born {
                rank: rank.into(),
                ..Born::default()
            },
            order: order.into(),
            links,
            seconds,
            times,
            progress: 0.0,
            playing: true,
            speed,
            asked: 0,
            laid: 0,
            credit: 0.0,
            revealed: None,
            zoom: None,
            births: VecDeque::new(),
            last: None,
            unseen: 0,
        });
        self.glide = Some(Glide::new(self.positions.clone(), Instant::now()));
        self.hover = None;
        self.selected = None;
        self.refresh_neighbors();
        self.auto_fit = true;
    }

    pub fn end_replay(&mut self) {
        if self.replay.take().is_some() {
            self.glide = None;
            if let Some(worker) = &self.worker {
                worker.end_growth();
            }
            self.needs_fit.set(true);
            self.auto_fit = true;
        }
    }

    pub fn born(&self) -> Option<&Born> {
        self.replay.as_ref().map(|r| &r.born)
    }

    fn advance(&mut self, now: Instant) -> bool {
        let Some(replay) = self.replay.as_mut().filter(|r| r.playing) else {
            return false;
        };
        if self.drawn.replace(false) {
            replay.unseen = 0;
        } else {
            replay.unseen += 1;
            if replay.unseen >= HIDDEN_TICKS {
                replay.playing = false;
                replay.last = None;
                return false;
            }
        }
        let elapsed = replay
            .last
            .map_or(0.0, |last| {
                now.saturating_duration_since(last).as_secs_f32()
            })
            .min(REPLAY_MAX_STEP);
        replay.last = Some(now);
        let pace = replay.speed / replay.seconds;
        replay.progress = (replay.progress + elapsed * pace).min(1.0);
        let total = replay.times.len();
        let count = born_at(replay.progress + BIRTH_LEAD * pace, total);
        if count as u32 != replay.asked {
            replay.asked = count as u32;
            if let Some(worker) = &self.worker {
                worker.grow_to(count);
            }
        }
        if replay.progress >= 1.0 && replay.born.count as usize == total && replay.births.is_empty()
        {
            self.replay = None;
            if let Some(worker) = &self.worker {
                worker.end_growth();
            }
            return false;
        }
        true
    }

    pub fn set_replay_playing(&mut self, playing: bool) {
        if let Some(replay) = &mut self.replay {
            replay.playing = playing;
            replay.last = None;
            replay.unseen = 0;
        }
    }

    pub fn build_input(&self) -> BuildInput {
        let same = self
            .built_from
            .as_ref()
            .is_some_and(|(spec, query)| *spec == self.spec && *query == self.query);
        BuildInput {
            vault: self.vault.clone(),
            search: self.search.clone(),
            query: self.query.clone(),
            spec: self.spec.clone(),
            params: self.params,
            cache_dir: self.cache_dir.clone(),
            previous: self.graph.clone(),
            previous_positions: self.positions.clone(),
            changes: if same && !self.pending.is_empty() {
                changes_of(&self.pending)
            } else {
                None
            },
            consumed: self.pending.len(),
        }
    }

    pub fn install(&mut self, built: Built) {
        let first = self.graph.nodes.is_empty();
        let worker = Worker::spawn(
            built.graph.clone(),
            built.simulation,
            self.cache_dir
                .clone()
                .filter(|_| self.spec.around.is_none()),
        );
        self.pending.drain(..built.consumed.min(self.pending.len()));
        self.built_from = Some((built.spec, built.query));
        self.table = built.table;
        self.graph = built.graph;
        *self.label_widths.borrow_mut() = vec![f32::NAN; self.graph.nodes.len()];
        self.positions = worker.positions();
        self.version = worker.version();
        self.index = Arc::new(SpatialIndex::new(&self.positions));
        self.worker = Some(worker);
        self.hover = None;
        self.selected = self
            .selected
            .filter(|&s| (s as usize) < self.graph.nodes.len());
        self.refresh_neighbors();
        if first || self.spec.around.is_some() || std::mem::take(&mut self.refit_on_install) {
            self.needs_fit.set(true);
            self.auto_fit = true;
        }
        self.building = false;
        self.last_build = Some(Instant::now());
        self.replay = None;
        self.glide = None;
    }

    pub fn clear(&mut self) {
        self.worker = None;
        self.graph = Arc::new(Graph::default());
        self.table = Arc::new(crate::plan::NodeTable::new(&self.graph));
        self.positions = Arc::new(Vec::new());
        self.index = Arc::new(SpatialIndex::new(&[]));
        self.hover = None;
        self.selected = None;
        self.neighbors.clear();
    }

    pub fn is_settled(&self) -> bool {
        self.worker.as_ref().is_none_or(Worker::is_settled)
    }

    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        let replaying = self.advance(now);
        let Some(worker) = &self.worker else {
            return false;
        };
        let settled = worker.is_settled();
        let version = worker.version();
        let stepped = version != self.version;
        let mut moved = stepped;
        if stepped {
            self.version = version;
            let (positions, born) = worker.snapshot();
            match (&mut self.replay, &mut self.glide) {
                (Some(replay), Some(glide)) => {
                    if let Some(fresh) = replay.arrive(born) {
                        glide.arrive(positions, fresh, now);
                    }
                }
                (None, Some(glide)) => glide.arrive(positions, &[], now),
                (_, None) => self.positions = positions,
            }
        }
        let mut reindex = stepped;
        if let Some(glide) = &self.glide {
            let (positions, gliding) = glide.at(now);
            moved |= gliding || !Arc::ptr_eq(&positions, &self.positions);
            self.positions = positions;
            if self.replay.is_none() && !gliding && settled {
                self.glide = None;
                reindex = true;
            }
        }
        let growing = self.replay.as_mut().is_some_and(|r| {
            r.reveal(now);
            r.refresh(now)
        });
        if reindex {
            self.index = Arc::new(SpatialIndex::new(&self.positions));
        }
        let following = self.follow(now);
        moved || growing || replaying || following || !settled || self.drag.is_some()
    }

    fn follow(&mut self, now: Instant) -> bool {
        let dt = self
            .followed
            .replace(now)
            .map_or(0.0, |then| {
                now.saturating_duration_since(then).as_secs_f32()
            })
            .min(CAMERA_MAX_STEP);
        let size = self.size();
        if !self.auto_fit || self.needs_fit.get() || self.positions.is_empty() || size[0] <= 0.0 {
            self.following = Follow::default();
            return false;
        }
        let mut target = Camera::fit(extent(&self.positions, self.born()), size, FIT_PADDING);
        if let Some(replay) = self.replay.as_mut() {
            let widest = replay.zoom.get_or_insert(target.zoom);
            *widest = widest.min(target.zoom);
            target.zoom = *widest;
        }
        self.fit_zoom.set(target.zoom);
        if self.glide.is_none() && self.following.is_still() {
            self.camera.set(target);
            return false;
        }
        let max_pan = self.replay.as_ref().map(|_| REPLAY_MAX_PAN);
        let (camera, moving) =
            self.following
                .step(self.camera.get(), target, CAMERA_SMOOTH, dt, max_pan);
        self.camera.set(camera);
        moving
    }

    pub fn reheat(&mut self, params: Params) {
        self.params = params;
        if let Some(w) = &self.worker {
            w.reheat(params);
        }
    }

    fn size(&self) -> [f32; 2] {
        let v = self.viewport.get();
        [v[2], v[3]]
    }

    fn local(&self, window: [f32; 2]) -> [f32; 2] {
        let v = self.viewport.get();
        [window[0] - v[0], window[1] - v[1]]
    }

    pub fn fit(&self) {
        if self.positions.is_empty() {
            return;
        }
        let size = self.size();
        if size[0] <= 0.0 {
            self.needs_fit.set(true);
            return;
        }
        let camera = Camera::fit(extent(&self.positions, self.born()), size, FIT_PADDING);
        self.fit_zoom.set(camera.zoom);
        self.camera.set(camera);
        self.needs_fit.set(false);
    }

    pub fn hit(&self, window: [f32; 2]) -> Option<u32> {
        let camera = self.camera.get();
        let size = self.size();
        let world = camera.to_world(self.local(window), size);
        let reach = 12.0 / camera.zoom;
        let node = self
            .index
            .nearest(world, reach)
            .filter(|&n| self.born().is_none_or(|b| b.shows(n)))?;
        let p = camera.to_screen(self.positions[node as usize], size);
        let s = self.local(window);
        let degree = self.graph.nodes.get(node as usize)?.degree;
        let r = crate::plan::radius(degree, camera.zoom, self.node_scale) + 3.0;
        ((p[0] - s[0]).powi(2) + (p[1] - s[1]).powi(2) <= r * r).then_some(node)
    }

    pub fn focus(&self) -> Option<u32> {
        self.hover.or(self.selected)
    }

    pub fn refresh_neighbors(&mut self) {
        self.neighbors.clear();
        self.focus_links = (0, 0);
        let Some(f) = self.focus() else { return };
        for e in &self.graph.edges {
            let (other, from_focus) = match (e.a == f, e.b == f) {
                (true, _) => (e.b, true),
                (_, true) => (e.a, false),
                _ => continue,
            };
            self.neighbors.push(other);
            if e.kind == EdgeKind::TagParent {
                continue;
            }
            self.focus_links.0 += (!from_focus || e.mutual) as u32;
            self.focus_links.1 += (from_focus || e.mutual) as u32;
        }
        self.neighbors.sort_unstable();
        self.neighbors.dedup();
    }

    pub fn tooltip_node(&self) -> Option<u32> {
        self.hover.filter(|_| self.drag.is_none())
    }

    pub fn active_node(&self) -> Option<u32> {
        self.active.and_then(|id| self.graph.node_of_note(id))
    }

    pub fn mouse_down(&mut self, window: [f32; 2], click_count: usize, new_tab: bool) -> bool {
        self.auto_fit = false;
        if let Some(node) = self.hit(window) {
            self.drag = Some(Drag::Node {
                node,
                start: window,
                moved: 0.0,
                new_tab,
            });
            return false;
        }
        if click_count >= 2 {
            self.fit();
            return true;
        }
        self.drag = Some(Drag::Pan {
            last: window,
            moved: 0.0,
        });
        let changed = self.selected.take().is_some();
        if changed {
            self.refresh_neighbors();
        }
        changed
    }

    pub fn mouse_move(&mut self, window: [f32; 2], pressed: bool) -> (bool, bool) {
        let camera = self.camera.get();
        let world = camera.to_world(self.local(window), self.size());
        match &mut self.drag {
            Some(Drag::Pan { last, moved }) if pressed => {
                let d = [window[0] - last[0], window[1] - last[1]];
                *moved += d[0].abs() + d[1].abs();
                *last = window;
                let mut c = camera;
                c.pan(d);
                self.camera.set(c);
                (true, false)
            }
            Some(Drag::Node {
                node, start, moved, ..
            }) if pressed => {
                *moved = moved.max((window[0] - start[0]).abs() + (window[1] - start[1]).abs());
                if *moved < CLICK_SLOP {
                    return (false, false);
                }
                let node = *node;
                if let Some(w) = &self.worker {
                    w.pin(node, Some(world));
                }
                (true, true)
            }
            _ => {
                let hover = self.hit(window);
                if hover != self.hover {
                    self.hover = hover;
                    self.refresh_neighbors();
                    return (true, false);
                }
                (false, false)
            }
        }
    }

    pub fn mouse_up(&mut self) -> bool {
        match self.drag.take() {
            Some(Drag::Node {
                node,
                moved,
                new_tab,
                ..
            }) => {
                if moved < CLICK_SLOP {
                    self.selected = Some(node);
                    self.refresh_neighbors();
                    if let Some(focus_graph::NodeKind::Note(id)) =
                        self.graph.nodes.get(node as usize).map(|n| n.kind.clone())
                    {
                        self.clicked = Some((id, new_tab));
                    }
                } else if let Some(w) = &self.worker {
                    w.pin(node, None);
                }
                true
            }
            Some(Drag::Pan { .. }) | None => false,
        }
    }

    pub fn scroll(&mut self, window: [f32; 2], dy: f32) {
        self.auto_fit = false;
        let mut c = self.camera.get();
        c.zoom_at(self.local(window), (-dy * 0.002).exp(), self.size());
        self.camera.set(c);
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.auto_fit = false;
        let size = self.size();
        let mut c = self.camera.get();
        c.zoom_at([size[0] / 2.0, size[1] / 2.0], factor, size);
        self.camera.set(c);
    }

    pub fn pan_by(&mut self, d: [f32; 2]) {
        self.auto_fit = false;
        let mut c = self.camera.get();
        c.pan(d);
        self.camera.set(c);
    }

    pub fn zoom_percent(&self) -> i64 {
        (self.camera.get().zoom / self.fit_zoom.get().max(f32::EPSILON) * 100.0).round() as i64
    }

    pub fn center_on(&mut self, node: u32) {
        self.auto_fit = false;
        let Some(p) = self.positions.get(node as usize) else {
            return;
        };
        let mut c = self.camera.get();
        c.center = *p;
        c.zoom = c.zoom.max(self.fit_zoom.get() * 3.0);
        self.camera.set(c);
        self.selected = Some(node);
        self.refresh_neighbors();
    }

    pub fn find(&self, query: &str) -> Option<u32> {
        let q = query.trim().trim_start_matches('#').to_lowercase();
        if q.is_empty() {
            return None;
        }
        let rank = |label: &str| {
            let l = label.trim_start_matches('#').to_lowercase();
            if l == q {
                Some(0)
            } else if l.starts_with(&q) {
                Some(1)
            } else if l.contains(&q) {
                Some(2)
            } else {
                None
            }
        };
        self.graph
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| rank(&n.label).map(|r| (r, std::cmp::Reverse(n.degree), i as u32)))
            .min()
            .map(|(_, _, i)| i)
    }

    pub fn hover_label(&mut self, label: &str) -> bool {
        self.auto_fit = false;
        let found = self.find(label);
        self.hover = found;
        self.refresh_neighbors();
        found.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focus_core::OpenOptions;

    fn vault(name: &str) -> Arc<Vault> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/focus-view-graph-tests")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("vault");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.md"), "# Alpha\n[[b]] [[c]]\n").unwrap();
        std::fs::write(root.join("b.md"), "# Beta\n[[a]]\n").unwrap();
        std::fs::write(root.join("c.md"), "# Gamma\n").unwrap();
        Vault::open(
            &root,
            OpenOptions {
                cache_dir: dir.join("cache"),
                ignore: Vec::new(),
                watch: false,
            },
        )
        .unwrap()
    }

    #[test]
    fn rebuild_keeps_known_positions_and_seeds_new_nodes_by_key() {
        let v = vault("keep-positions");
        let mut core = GraphCore::new(v.clone(), None, None, Spec::default(), Params::default());
        core.install(build_graph(core.build_input()));
        assert_eq!(core.graph.nodes.len(), 3);
        let moved: Vec<[f32; 2]> = (0..3).map(|i| [100.0 * i as f32, 7.0]).collect();
        core.positions = Arc::new(moved.clone());
        std::fs::write(v.root().join("d.md"), "# Delta\n[[a]]\n").unwrap();
        let v2 = Vault::open(
            v.root(),
            OpenOptions {
                cache_dir: v.root().join("../cache2"),
                ignore: Vec::new(),
                watch: false,
            },
        )
        .unwrap();
        core.vault = v2;
        let built = build_graph(core.build_input());
        assert_eq!(built.graph.nodes.len(), 4);
        for i in 0..4 {
            let key = node_key(&built.graph, i).unwrap();
            if let Some(old) = (0..3).find(|&j| node_key(&core.graph, j).unwrap() == key) {
                assert_eq!(built.simulation.positions()[i], moved[old]);
            }
        }
    }

    #[test]
    fn vault_changes_update_the_installed_graph() {
        let v = vault("update");
        let mut core = GraphCore::new(v.clone(), None, None, Spec::default(), Params::default());
        core.install(build_graph(core.build_input()));
        let d = v
            .create_note(std::path::Path::new("d.md"), "# Delta\n[[c]]\n")
            .unwrap();
        core.pending.push(Pending::Changed(d));
        let input = core.build_input();
        assert!(input.changes.is_some(), "same spec and query: an update");
        core.install(build_graph(input));
        assert!(core.pending.is_empty());
        assert_eq!(core.graph.nodes.len(), 4);
        core.pending.push(Pending::Removed(d));
        core.pending.push(Pending::Everything);
        assert!(
            core.build_input().changes.is_none(),
            "attachments changed: a full build"
        );
        core.pending.clear();
        core.spec.orphans = false;
        assert!(
            core.build_input().changes.is_none(),
            "another spec: a full build"
        );
    }

    #[test]
    fn births_come_one_after_the_other_at_an_even_rhythm() {
        for (n, speed) in [(82usize, 1.0f32), (2500, 1.0), (2500, 4.0), (100_000, 1.0)] {
            let order: Vec<u32> = (0..n as u32).collect();
            let seconds = replay_seconds(n);
            let mut replay = Replay {
                born: Born {
                    rank: order.clone().into(),
                    ..Born::default()
                },
                order: order.into(),
                links: Links::of(&Graph::default()),
                seconds,
                times: vec![0; n],
                progress: 0.0,
                playing: true,
                speed,
                asked: 0,
                laid: 0,
                credit: 0.0,
                revealed: None,
                zoom: None,
                births: VecDeque::new(),
                last: None,
                unseen: 0,
            };
            let start = Instant::now();
            let pace = speed / seconds;
            let mut per_frame = Vec::new();
            let mut frame = 0u32;
            while replay.born.count as usize != n {
                frame += 1;
                replay.progress = (frame as f32 / 60.0 * pace).min(1.0);
                replay.laid = born_at(replay.progress + BIRTH_LEAD * pace, n) as u32;
                let before = replay.born.count;
                replay.reveal(start + std::time::Duration::from_secs_f32(frame as f32 / 60.0));
                per_frame.push(replay.born.count - before);
                assert!(frame < 60 * 2000, "{n} notes never all born");
            }
            let rate = n as f32 * pace;
            let most = per_frame.iter().copied().max().unwrap_or(0);
            assert!(
                most as f32 <= (rate / 60.0).ceil(),
                "{n} notes at speed {speed}: {most} births in one frame at {rate} a second"
            );
            let second = per_frame
                .windows(60)
                .map(|w| w.iter().sum::<u32>())
                .max()
                .unwrap_or(0);
            assert!(
                second as f32 <= 100.0 * speed + 1.0,
                "{n} notes at speed {speed}: {second} births in one second"
            );
            let steady = &per_frame[60.min(per_frame.len())..per_frame.len().saturating_sub(60)];
            let (low, high) = steady
                .iter()
                .fold((u32::MAX, 0), |(lo, hi), &c| (lo.min(c), hi.max(c)));
            assert!(
                steady.is_empty() || high - low <= 1,
                "{n} notes at speed {speed}: from {low} to {high} births a frame"
            );
        }
    }

    #[test]
    #[ignore]
    fn replay_frame_cost() {
        let n = 100_000usize;
        let at = |k: f32| -> Arc<Vec<[f32; 2]>> {
            Arc::new(
                (0..n)
                    .map(|i| [(i % 400) as f32 * 30.0 + k, (i / 400) as f32 * 30.0])
                    .collect(),
            )
        };
        let born = Born {
            rank: (0..n as u32).collect::<Vec<_>>().into(),
            count: n as u32 / 2,
            fresh: Arc::new([(n as u32 / 2 - 50, 0.5)]),
            glow: Arc::new([]),
        };
        let start = Instant::now();
        let mut glide = Glide::new(at(0.0), start);
        let ms = |t: Instant| t.elapsed().as_secs_f32() * 1000.0;
        let (mut glides, mut extents, mut indexes) = (Vec::new(), Vec::new(), Vec::new());
        for frame in 1..=60u32 {
            let now = start + std::time::Duration::from_micros(frame as u64 * 3_125);
            let next = (frame % 5 == 0).then(|| at(frame as f32));
            let begun = Instant::now();
            if let Some(next) = next {
                glide.arrive(next, &[], now);
            }
            let (positions, _) = glide.at(now);
            glides.push(ms(begun));
            let begun = Instant::now();
            let bounds = extent(&positions, Some(&born));
            extents.push(ms(begun));
            let begun = Instant::now();
            let index = SpatialIndex::new(&positions);
            indexes.push(ms(begun));
            assert!(bounds[0] <= bounds[2] && index.nearest(positions[0], 1.0).is_some());
        }
        let median = |mut v: Vec<f32>| {
            v.sort_by(f32::total_cmp);
            v[v.len() / 2]
        };
        println!(
            "replay frame work on {n} nodes, median: glide {:.2} ms, extent {:.2} ms, spatial index {:.2} ms",
            median(glides),
            median(extents),
            median(indexes)
        );
    }

    #[test]
    fn find_prefers_exact_then_prefix_then_degree() {
        let v = vault("find");
        let mut core = GraphCore::new(v, None, None, Spec::default(), Params::default());
        core.install(build_graph(core.build_input()));
        let label = |i: u32| core.graph.nodes[i as usize].label.to_string();
        assert_eq!(core.find("alpha").map(label), Some("Alpha".into()));
        assert_eq!(
            core.find("ta").map(label),
            Some("Beta".into()),
            "only Beta contains it"
        );
        assert_eq!(core.find("zzz"), None);
    }
}
