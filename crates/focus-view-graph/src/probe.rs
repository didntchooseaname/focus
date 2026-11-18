use crate::camera::Camera;
use crate::core::Born;
use crate::plan::{EdgeDraw, NodeDraw, NodeTable, Plan, radius};
use focus_graph::Graph;
use std::time::Instant;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Spread {
    pub median: f32,
    pub p95: f32,
    pub max: f32,
}

fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0, |(k, _)| k)
}

impl Spread {
    fn of(mut values: Vec<f32>) -> Spread {
        if values.is_empty() {
            return Spread::default();
        }
        values.sort_by(f32::total_cmp);
        let at = |q: f32| values[((values.len() - 1) as f32 * q).round() as usize];
        Spread {
            median: at(0.5),
            p95: at(0.95),
            max: at(1.0),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MotionReport {
    pub frames: usize,
    pub interval_ms: Spread,
    pub node_px: Spread,
    pub placed_px: Spread,
    pub jerk_px: Spread,
    pub placed_jerk_px: Spread,
    pub frame_max_px: Spread,
    pub pan_px: Spread,
    pub zoom_pct: Spread,
    pub births: usize,
    pub births_unlinked: usize,
    pub births_per_frame: usize,
    pub birth_gap_px: Spread,
    pub birth_gap_link: Spread,
    pub birth_scale: Spread,
    pub travel_px: Spread,
    pub worst_move_frame: usize,
    pub worst_move_age: u32,
    pub worst_zoom_frame: usize,
    pub births_per_second: usize,
    pub zoom_turns: usize,
    pub links_born: usize,
    pub links_grown: f32,
    pub link_frames: f32,
    pub bright_s: Spread,
    pub spread: SpreadSeries,
    pub end: SpreadFigures,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SpreadFigures {
    pub nodes: usize,
    pub overlap_pct: f32,
    pub link_ratio: f32,
    pub fill_pct: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SpreadSeries {
    pub overlap_pct: Spread,
    pub link_ratio: Spread,
    pub fill_pct: Spread,
}

const SPREAD_EVERY: u32 = 15;
const SPREAD_LEAST: usize = 50;
const ZOOM_TURN: f32 = 0.05;
const BRIGHT: f32 = 0.01;

pub(crate) fn spread_of(nodes: &[NodeDraw], edges: &[EdgeDraw], size: [f32; 2]) -> SpreadFigures {
    let n = nodes.len();
    if n == 0 {
        return SpreadFigures::default();
    }
    let largest = nodes.iter().map(|d| d.radius).fold(1.0f32, f32::max);
    let cell = 2.0 * largest;
    let key = |p: [f32; 2]| ((p[0] / cell).floor() as i32, (p[1] / cell).floor() as i32);
    let mut grid: std::collections::HashMap<(i32, i32), Vec<u32>> =
        std::collections::HashMap::new();
    for (k, d) in nodes.iter().enumerate() {
        grid.entry(key(d.at)).or_default().push(k as u32);
    }
    let mut overlapping = 0usize;
    for d in nodes {
        let (cx, cy) = key(d.at);
        let hit = (cx - 1..=cx + 1).any(|x| {
            (cy - 1..=cy + 1).any(|y| {
                grid.get(&(x, y)).is_some_and(|list| {
                    list.iter().any(|&o| {
                        let o = &nodes[o as usize];
                        o.index != d.index
                            && (o.at[0] - d.at[0]).hypot(o.at[1] - d.at[1]) < o.radius + d.radius
                    })
                })
            })
        });
        overlapping += hit as usize;
    }
    let mut lengths: Vec<f32> = edges
        .iter()
        .map(|e| (e.b[0] - e.a[0]).hypot(e.b[1] - e.a[1]))
        .collect();
    lengths.sort_by(f32::total_cmp);
    let link_ratio = match lengths.len() {
        0 => 0.0,
        m => lengths[(m - 1) * 95 / 100] / lengths[(m - 1) / 2].max(0.5),
    };
    let mut points: Vec<[f32; 2]> = nodes.iter().map(|d| d.at).collect();
    SpreadFigures {
        nodes: n,
        overlap_pct: overlapping as f32 * 100.0 / n as f32,
        link_ratio,
        fill_pct: hull_area(&mut points) * 100.0 / (size[0] * size[1]).max(1.0),
    }
}

fn hull_area(points: &mut [[f32; 2]]) -> f32 {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    if points.len() < 3 {
        return 0.0;
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut hull: Vec<[f32; 2]> = Vec::with_capacity(points.len() * 2);
    for pass in 0..2 {
        let floor = hull.len() + 1;
        let iter: Box<dyn Iterator<Item = &[f32; 2]>> = if pass == 0 {
            Box::new(points.iter())
        } else {
            Box::new(points.iter().rev())
        };
        for &p in iter {
            while hull.len() > floor && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    let m = hull.len();
    (0..m)
        .map(|k| {
            let (a, b) = (hull[k], hull[(k + 1) % m]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f32>()
        .abs()
        / 2.0
}

const SETTLED_FRAMES: u32 = 60;

pub(crate) struct Probe {
    neighbor_start: Vec<u32>,
    neighbors: Vec<u32>,
    last_at: Vec<[f32; 2]>,
    last_move: Vec<[f32; 2]>,
    at: Vec<[f32; 2]>,
    last: Option<(Instant, Camera, u32)>,
    first_frame: Vec<u32>,
    first_at: Vec<[f32; 2]>,
    travel: Vec<f32>,
    intervals: Vec<f32>,
    node_px: Vec<f32>,
    placed_px: Vec<f32>,
    jerk_px: Vec<f32>,
    placed_jerk_px: Vec<f32>,
    frame_max: Vec<f32>,
    pan: Vec<f32>,
    zoom: Vec<f32>,
    gaps: Vec<f32>,
    gap_links: Vec<f32>,
    worst: (f32, u32),
    scales: Vec<f32>,
    unlinked: usize,
    births_per_frame: usize,
    birth_times: Vec<Instant>,
    zoom_turns: usize,
    zoom_way: i8,
    edge_first: Vec<u32>,
    bright_from: Vec<Option<Instant>>,
    bright: Vec<f32>,
    links_born: usize,
    links_grown: usize,
    link_frames: usize,
    samples: Vec<SpreadFigures>,
    last_nodes: Vec<NodeDraw>,
    last_edges: Vec<EdgeDraw>,
    size: [f32; 2],
}

impl Probe {
    pub fn new(graph: &Graph) -> Probe {
        let n = graph.nodes.len();
        let mut count = vec![0u32; n + 1];
        for e in &graph.edges {
            count[e.a as usize + 1] += 1;
            count[e.b as usize + 1] += 1;
        }
        for i in 1..=n {
            count[i] += count[i - 1];
        }
        let mut fill = count.clone();
        let mut neighbors = vec![0u32; count[n] as usize];
        for e in &graph.edges {
            neighbors[fill[e.a as usize] as usize] = e.b;
            fill[e.a as usize] += 1;
            neighbors[fill[e.b as usize] as usize] = e.a;
            fill[e.b as usize] += 1;
        }
        Probe {
            neighbor_start: count,
            neighbors,
            last_at: vec![[f32::NAN; 2]; n],
            last_move: vec![[f32::NAN; 2]; n],
            at: vec![[f32::NAN; 2]; n],
            last: None,
            first_frame: vec![u32::MAX; n],
            first_at: vec![[f32::NAN; 2]; n],
            travel: Vec::new(),
            intervals: Vec::new(),
            node_px: Vec::new(),
            placed_px: Vec::new(),
            jerk_px: Vec::new(),
            placed_jerk_px: Vec::new(),
            frame_max: Vec::new(),
            pan: Vec::new(),
            zoom: Vec::new(),
            gaps: Vec::new(),
            gap_links: Vec::new(),
            worst: (0.0, u32::MAX),
            scales: Vec::new(),
            unlinked: 0,
            births_per_frame: 0,
            birth_times: Vec::new(),
            zoom_turns: 0,
            zoom_way: 0,
            edge_first: vec![u32::MAX; graph.edges.len()],
            bright_from: vec![None; graph.edges.len()],
            bright: Vec::new(),
            links_born: 0,
            links_grown: 0,
            link_frames: 0,
            samples: Vec::new(),
            last_nodes: Vec::new(),
            last_edges: Vec::new(),
            size: [0.0; 2],
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        now: Instant,
        camera: Camera,
        plan: &Plan,
        born: Option<&Born>,
        table: &NodeTable,
        node_scale: f32,
        size: [f32; 2],
    ) {
        self.size = size;
        self.last_nodes.clear();
        self.last_nodes.extend_from_slice(&plan.nodes);
        self.last_edges.clear();
        self.last_edges.extend_from_slice(&plan.edges);
        self.record_links(now, plan, born.is_some());
        let n = self.at.len();
        self.at.iter_mut().for_each(|p| *p = [f32::NAN; 2]);
        let mut moves = vec![[f32::NAN; 2]; n];
        for d in plan.nodes.iter().filter(|d| (d.index as usize) < n) {
            self.at[d.index as usize] = d.at;
        }
        let frame = self.intervals.len() as u32;
        if let Some((then, before, count)) = self.last {
            self.intervals
                .push(now.duration_since(then).as_secs_f32() * 1000.0);
            let dc = [
                camera.center[0] - before.center[0],
                camera.center[1] - before.center[1],
            ];
            self.pan
                .push((dc[0] * dc[0] + dc[1] * dc[1]).sqrt() * camera.zoom);
            let zoom = (camera.zoom / before.zoom.max(f32::EPSILON) - 1.0) * 100.0;
            self.zoom.push(zoom.abs());
            if zoom.abs() > ZOOM_TURN {
                let way = zoom.signum() as i8;
                if self.zoom_way != 0 && way != self.zoom_way {
                    self.zoom_turns += 1;
                }
                self.zoom_way = way;
            }
            let rank =
                |i: u32| born.map_or(0, |b| b.rank.get(i as usize).copied().unwrap_or(u32::MAX));
            let mut frame_max = 0.0f32;
            let mut births = 0;
            let mut link: Option<f32> = None;
            let edges = &plan.edges;
            let mut median_link = || {
                *link.get_or_insert_with(|| {
                    let mut lengths: Vec<f32> = edges
                        .iter()
                        .map(|e| (e.b[0] - e.a[0]).hypot(e.b[1] - e.a[1]))
                        .collect();
                    if lengths.is_empty() {
                        return f32::NAN;
                    }
                    let mid = lengths.len() / 2;
                    *lengths.select_nth_unstable_by(mid, f32::total_cmp).1
                })
            };
            for d in plan.nodes.iter().filter(|d| (d.index as usize) < n) {
                let i = d.index as usize;
                let p = self.last_at[i];
                if !p[0].is_nan() {
                    let step = [d.at[0] - p[0], d.at[1] - p[1]];
                    let moved = step[0].hypot(step[1]);
                    moves[i] = step;
                    let before = self.last_move[i];
                    let jerk = (!before[0].is_nan())
                        .then(|| (step[0] - before[0]).hypot(step[1] - before[1]));
                    self.jerk_px.extend(jerk);
                    self.node_px.push(moved);
                    if self.first_frame[i] != u32::MAX {
                        let age = frame.saturating_sub(self.first_frame[i]);
                        if age >= SETTLED_FRAMES {
                            self.placed_px.push(moved);
                            self.placed_jerk_px.extend(jerk);
                        }
                        if age == SETTLED_FRAMES {
                            let p0 = self.first_at[i];
                            self.travel.push((d.at[0] - p0[0]).hypot(d.at[1] - p0[1]));
                        }
                    }
                    if moved > self.worst.0 {
                        let first = self.first_frame[i];
                        let age = if first == u32::MAX {
                            u32::MAX
                        } else {
                            frame - first
                        };
                        self.worst = (moved, age);
                    }
                    frame_max = frame_max.max(moved);
                    continue;
                }
                if rank(d.index) < count {
                    continue;
                }
                births += 1;
                self.birth_times.push(now);
                self.first_frame[i] = frame;
                self.first_at[i] = d.at;
                let full = radius(table.degree[i], camera.zoom, node_scale);
                self.scales.push(d.radius / full.max(f32::EPSILON));
                let start = self.neighbor_start[i] as usize;
                let end = self.neighbor_start[i + 1] as usize;
                let gap = self.neighbors[start..end]
                    .iter()
                    .filter(|&&j| rank(j) < count)
                    .map(|&j| self.at[j as usize])
                    .filter(|q| !q[0].is_nan())
                    .map(|q| ((d.at[0] - q[0]).powi(2) + (d.at[1] - q[1]).powi(2)).sqrt())
                    .min_by(f32::total_cmp);
                match gap {
                    Some(gap) => {
                        self.gaps.push(gap);
                        let link = median_link();
                        if link > 0.0 {
                            self.gap_links.push(gap / link);
                        }
                    }
                    None => self.unlinked += 1,
                }
            }
            self.frame_max.push(frame_max);
            self.births_per_frame = self.births_per_frame.max(births);
            if frame.is_multiple_of(SPREAD_EVERY) && plan.nodes.len() >= SPREAD_LEAST {
                self.samples.push(spread_of(&plan.nodes, &plan.edges, size));
            }
        }
        std::mem::swap(&mut self.at, &mut self.last_at);
        self.last_move = moves;
        self.last = Some((now, camera, born.map_or(u32::MAX, |b| b.count)));
    }

    fn record_links(&mut self, now: Instant, plan: &Plan, replaying: bool) {
        let frame = self.intervals.len() as u32;
        let mut growing = false;
        for e in &plan.edges {
            let k = e.index as usize;
            let Some(first) = self.edge_first.get_mut(k) else {
                continue;
            };
            if *first == u32::MAX {
                *first = frame;
                if frame > 0 && replaying {
                    self.links_born += 1;
                    self.links_grown += (e.reach < 1.0) as usize;
                }
            }
            growing |= e.reach < 1.0;
            let lit = e.glow > BRIGHT;
            match (lit, self.bright_from[k]) {
                (true, None) => self.bright_from[k] = Some(now),
                (false, Some(from)) => {
                    self.bright.push(now.duration_since(from).as_secs_f32());
                    self.bright_from[k] = None;
                }
                _ => {}
            }
        }
        self.link_frames += growing as usize;
    }

    pub fn end(&self) -> SpreadFigures {
        spread_of(&self.last_nodes, &self.last_edges, self.size)
    }

    pub fn report(&self) -> MotionReport {
        let mut births_per_second = 0;
        let mut from = 0;
        for (k, t) in self.birth_times.iter().enumerate() {
            while t.duration_since(self.birth_times[from]).as_secs_f32() >= 1.0 {
                from += 1;
            }
            births_per_second = births_per_second.max(k + 1 - from);
        }
        let series =
            |f: fn(&SpreadFigures) -> f32| Spread::of(self.samples.iter().map(f).collect());
        MotionReport {
            frames: self.intervals.len(),
            interval_ms: Spread::of(self.intervals.clone()),
            node_px: Spread::of(self.node_px.clone()),
            placed_px: Spread::of(self.placed_px.clone()),
            jerk_px: Spread::of(self.jerk_px.clone()),
            placed_jerk_px: Spread::of(self.placed_jerk_px.clone()),
            frame_max_px: Spread::of(self.frame_max.clone()),
            pan_px: Spread::of(self.pan.clone()),
            zoom_pct: Spread::of(self.zoom.clone()),
            births: self.scales.len(),
            births_unlinked: self.unlinked,
            births_per_frame: self.births_per_frame,
            birth_gap_px: Spread::of(self.gaps.clone()),
            birth_gap_link: Spread::of(self.gap_links.clone()),
            birth_scale: Spread::of(self.scales.clone()),
            travel_px: Spread::of(self.travel.clone()),
            worst_move_frame: argmax(&self.frame_max),
            worst_move_age: self.worst.1,
            worst_zoom_frame: argmax(&self.zoom),
            births_per_second,
            zoom_turns: self.zoom_turns,
            links_born: self.links_born,
            links_grown: self.links_grown as f32 / self.links_born.max(1) as f32,
            link_frames: self.link_frames as f32 / (self.intervals.len() + 1) as f32,
            bright_s: Spread::of(self.bright.clone()),
            spread: SpreadSeries {
                overlap_pct: series(|f| f.overlap_pct),
                link_ratio: series(|f| f.link_ratio),
                fill_pct: series(|f| f.fill_pct),
            },
            end: self.end(),
        }
    }
}
