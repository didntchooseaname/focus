use crate::{Graph, Params};
use rayon::prelude::*;

const VELOCITY_DECAY: f32 = 0.4;
const ALPHA_MIN: f32 = 0.001;
const CHARGE: f32 = 30.0;
const COLLIDE: f32 = 0.7;
const LEAF: usize = 8;
const GROW_ALPHA: f32 = 0.3;
const BIRTH_HEAT: f32 = 3.0;
const NEWBORN_HEAT: f32 = 0.5;
const NEWBORN_COOLING: f32 = 0.95;
const GOLDEN_ANGLE: f32 = 2.399_963;
const GROWTH_SPEED: f32 = 0.25;
const BIRTH_REACH: f32 = 2.0;
const BIRTH_ROOM: f32 = 2.0;
const BIRTH_TRIES: u32 = 9;
const BIRTH_TURN: f32 = 0.3;
const GROWING_HEAT: f32 = 0.001;
const GROWING_HEAT_MAX: f32 = 0.06;
const SHAPE_PULL: f32 = 0.03;
const CLOSE_PULL: f32 = 0.06;
const PULL_EASE: f32 = 0.05;
const CLOSE_STEPS: u32 = 30;
const CLOSE_FROM: f32 = 0.5;

fn degree_weight(degree: u32) -> f32 {
    (degree as f32).sqrt()
}

pub(crate) fn radius_of(degree: u32) -> f32 {
    3.0 + 1.5 * degree_weight(degree)
}

struct Spring {
    other: u32,
    strength: f32,
    share: f32,
    rest: f32,
}

#[derive(Clone, Copy)]
struct Place {
    center: [f32; 2],
    rim: f32,
    turn: f32,
    toward: Option<[f32; 2]>,
    link: Option<f32>,
}

pub(crate) struct Layout {
    pos: Vec<[f32; 2]>,
    vel: Vec<[f32; 2]>,
    pinned: Vec<Option<[f32; 2]>>,
    mass: Vec<f32>,
    radius: Vec<f32>,
    spring_start: Vec<u32>,
    springs: Vec<Spring>,
    params: Params,
    alpha: f32,
    alpha_decay: f32,
    settled: bool,
    tree: Tree,
    moving: Option<Vec<u32>>,
    tree_built: bool,
    growth: Option<Growth>,
    heat: Vec<f32>,
}

struct Growth {
    order: Vec<u32>,
    alive: Vec<bool>,
    born: usize,
    rest: Option<Vec<[f32; 2]>>,
    pull: f32,
    ended: bool,
    closing: u32,
}

#[derive(Clone, Copy)]
struct Shape {
    center: [f32; 2],
    rest_center: [f32; 2],
    scale: f32,
    pull: f32,
}

impl Shape {
    fn target(&self, rest: [f32; 2]) -> [f32; 2] {
        [
            self.center[0] + (rest[0] - self.rest_center[0]) * self.scale,
            self.center[1] + (rest[1] - self.rest_center[1]) * self.scale,
        ]
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Growth {
    fn complete(&self) -> bool {
        self.born == self.order.len()
    }

    fn pull_for(&self) -> f32 {
        if self.rest.is_none() {
            return 0.0;
        }
        if self.complete() {
            return CLOSE_PULL;
        }
        let share = self.born as f32 / self.order.len().max(1) as f32;
        SHAPE_PULL * share * share
    }

    fn shape(&self, pos: &[[f32; 2]], rest: &[[f32; 2]]) -> Shape {
        let born = &self.order[..self.born];
        let n = born.len().max(1) as f64;
        let mut c = [0.0f64; 4];
        for &i in born {
            let (p, r) = (pos[i as usize], rest[i as usize]);
            c[0] += p[0] as f64;
            c[1] += p[1] as f64;
            c[2] += r[0] as f64;
            c[3] += r[1] as f64;
        }
        let center = [(c[0] / n) as f32, (c[1] / n) as f32];
        let rest_center = [(c[2] / n) as f32, (c[3] / n) as f32];
        let (mut now, mut then) = (0.0f64, 0.0f64);
        for &i in born {
            let (p, r) = (pos[i as usize], rest[i as usize]);
            now += ((p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2)) as f64;
            then += ((r[0] - rest_center[0]).powi(2) + (r[1] - rest_center[1]).powi(2)) as f64;
        }
        let scale = if then > 0.0 {
            (now / then).sqrt() as f32
        } else {
            1.0
        };
        let share = self.born as f32 / self.order.len().max(1) as f32;
        let w = smoothstep((share - CLOSE_FROM) / (1.0 - CLOSE_FROM));
        let mix = |a: f32, b: f32| a + (b - a) * w;
        Shape {
            center: [
                mix(center[0], rest_center[0]),
                mix(center[1], rest_center[1]),
            ],
            rest_center,
            scale: mix(scale, 1.0),
            pull: self.pull,
        }
    }
}

pub(crate) fn bounds(pos: &[[f32; 2]]) -> [f32; 4] {
    if pos.is_empty() {
        return [0.0; 4];
    }
    pos.iter()
        .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| {
            [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ]
        })
}

fn spiral(k: usize, scale: f32) -> [f32; 2] {
    let r = scale * (0.5 + k as f32).sqrt();
    let a = k as f32 * std::f32::consts::PI * (3.0 - 5f32.sqrt());
    [r * a.cos(), r * a.sin()]
}

fn spread(pos: &[[f32; 2]], born: &[u32]) -> ([f32; 2], f32) {
    if born.is_empty() {
        return ([0.0, 0.0], 0.0);
    }
    let sum = born.iter().fold([0.0f64, 0.0f64], |s, &i| {
        let p = pos[i as usize];
        [s[0] + p[0] as f64, s[1] + p[1] as f64]
    });
    let n = born.len() as f64;
    let center = [(sum[0] / n) as f32, (sum[1] / n) as f32];
    let mut dist: Vec<f32> = born
        .iter()
        .map(|&i| {
            let p = pos[i as usize];
            ((p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2)).sqrt()
        })
        .collect();
    let k = (dist.len() - 1) * 9 / 10;
    let (_, rim, _) = dist.select_nth_unstable_by(k, f32::total_cmp);
    (center, *rim)
}

impl Layout {
    pub fn new(graph: &Graph, seed: &[(u32, [f32; 2])], params: Params) -> Layout {
        let n = graph.nodes.len();
        let mut degree = vec![0u32; n];
        for e in &graph.edges {
            degree[e.a as usize] += 1;
            degree[e.b as usize] += 1;
        }
        let mass: Vec<f32> = degree.iter().map(|&d| 1.0 + degree_weight(d)).collect();
        let radius: Vec<f32> = degree.iter().map(|&d| radius_of(d)).collect();

        let mut lists: Vec<Vec<Spring>> = (0..n).map(|_| Vec::new()).collect();
        for e in &graph.edges {
            let (a, b) = (e.a as usize, e.b as usize);
            if a == b || a >= n || b >= n {
                continue;
            }
            let (da, db) = (degree[a].max(1) as f32, degree[b].max(1) as f32);
            let strength = params.attraction / da.min(db);
            let rest = params.link_distance + radius[a] + radius[b];
            lists[a].push(Spring {
                other: b as u32,
                strength,
                share: db / (da + db),
                rest,
            });
            lists[b].push(Spring {
                other: a as u32,
                strength,
                share: da / (da + db),
                rest,
            });
        }
        let mut spring_start = Vec::with_capacity(n + 1);
        let mut springs = Vec::with_capacity(graph.edges.len() * 2);
        for l in lists {
            spring_start.push(springs.len() as u32);
            springs.extend(l);
        }
        spring_start.push(springs.len() as u32);

        let mut pos: Vec<Option<[f32; 2]>> = vec![None; n];
        for &(i, p) in seed {
            if let Some(slot) = pos.get_mut(i as usize) {
                *slot = Some(p);
            }
        }
        let seeded = pos.iter().filter(|p| p.is_some()).count();
        let fresh: Vec<u32> = (0..n)
            .filter(|&i| pos[i].is_none())
            .map(|i| i as u32)
            .collect();
        let scale = params.link_distance / 3.0;
        let mut spiral_k = 0;
        for i in 0..n {
            if pos[i].is_some() {
                continue;
            }
            let placed: Vec<[f32; 2]> = springs
                [spring_start[i] as usize..spring_start[i + 1] as usize]
                .iter()
                .filter_map(|s| pos[s.other as usize])
                .collect();
            pos[i] = Some(if placed.is_empty() {
                spiral_k += 1;
                spiral(spiral_k - 1, scale)
            } else {
                let c = placed
                    .iter()
                    .fold([0.0, 0.0], |c, p| [c[0] + p[0], c[1] + p[1]]);
                let m = placed.len() as f32;
                let offset = spiral(i % 64 + 1, params.link_distance / 8.0);
                [c[0] / m + offset[0], c[1] / m + offset[1]]
            });
        }
        let moving = (seeded > 0 && fresh.len() * 100 <= n).then_some(fresh);
        let alpha = if moving.is_some() {
            0.5
        } else if n > 0 && seeded * 10 >= n * 9 {
            0.25
        } else {
            1.0
        };
        Layout {
            pos: pos.into_iter().map(|p| p.unwrap_or([0.0, 0.0])).collect(),
            vel: vec![[0.0, 0.0]; n],
            pinned: vec![None; n],
            mass,
            radius,
            spring_start,
            springs,
            params,
            alpha,
            alpha_decay: 1.0 - ALPHA_MIN.powf(1.0 / 300.0),
            settled: n == 0 || moving.as_ref().is_some_and(Vec::is_empty),
            tree: Tree::default(),
            moving,
            tree_built: false,
            growth: None,
            heat: Vec::new(),
        }
    }

    pub fn grow(&mut self, order: Vec<u32>) {
        let n = self.pos.len();
        let rest = match self.growth.take() {
            Some(g) => g.rest,
            None => self.settled.then(|| self.pos.clone()),
        };
        self.growth = Some(Growth {
            order,
            alive: vec![false; n],
            born: 0,
            rest,
            pull: 0.0,
            ended: false,
            closing: 0,
        });
        self.vel.iter_mut().for_each(|v| *v = [0.0, 0.0]);
        self.heat = Vec::new();
        self.moving = None;
    }

    pub fn grow_to(&mut self, count: usize) {
        let Some(mut growth) = self.growth.take() else {
            return;
        };
        let count = count.min(growth.order.len());
        if count > growth.born {
            let (center, rim) = spread(&self.pos, &growth.order[..growth.born]);
            let shape = growth
                .rest
                .as_deref()
                .filter(|_| growth.born >= 3)
                .map(|rest| (growth.shape(&self.pos, rest), rest));
            let link = self.link_length(&growth.alive, &growth.order[..growth.born]);
            if self.heat.len() != self.pos.len() {
                self.heat = vec![0.0; self.pos.len()];
            }
            for k in growth.born..count {
                let i = growth.order[k] as usize;
                let place = Place {
                    center,
                    rim,
                    turn: k as f32 * GOLDEN_ANGLE,
                    toward: shape.map(|(s, rest)| s.target(rest[i])),
                    link,
                };
                self.pos[i] = self.birth_place(i, &growth.alive, place);
                self.vel[i] = [0.0, 0.0];
                self.heat[i] = NEWBORN_HEAT;
                growth.alive[i] = true;
            }
            let births = (count - growth.born) as f32;
            let share = births / count as f32;
            growth.born = count;
            let growing = if self.settled {
                0.0
            } else {
                (self.alpha + GROWING_HEAT * births).min(GROWING_HEAT_MAX)
            };
            self.alpha = self
                .alpha
                .max((BIRTH_HEAT * share * GROW_ALPHA).min(GROW_ALPHA))
                .max(growing);
            self.settled = false;
        }
        self.growth = Some(growth);
    }

    fn link_length(&self, alive: &[bool], born: &[u32]) -> Option<f32> {
        let mut lengths: Vec<f32> = Vec::new();
        let few = born.len() < 1024;
        let own = born.iter().flat_map(|&i| {
            self.spring_start[i as usize] as usize..self.spring_start[i as usize + 1] as usize
        });
        let sampled = (0..self.springs.len()).step_by((self.springs.len() / 2048).max(1));
        let springs: Box<dyn Iterator<Item = usize>> = if few {
            Box::new(own)
        } else {
            Box::new(sampled)
        };
        for k in springs {
            let i = self
                .spring_start
                .partition_point(|&start| start as usize <= k)
                - 1;
            let j = self.springs[k].other as usize;
            if alive[i] && alive[j] {
                let (p, q) = (self.pos[i], self.pos[j]);
                lengths.push((p[0] - q[0]).hypot(p[1] - q[1]));
            }
        }
        if lengths.is_empty() {
            return None;
        }
        let mid = lengths.len() / 2;
        let (_, m, _) = lengths.select_nth_unstable_by(mid, f32::total_cmp);
        Some(*m)
    }

    fn birth_place(&self, i: usize, alive: &[bool], place: Place) -> [f32; 2] {
        let Place {
            center,
            rim,
            turn,
            toward,
            link,
        } = place;
        let born = |i: usize| {
            self.springs[self.spring_start[i] as usize..self.spring_start[i + 1] as usize]
                .iter()
                .filter(|s| alive[s.other as usize])
        };
        let (sum, m) = born(i).fold(([0.0f32; 2], 0.0f32), |(c, m), s| {
            let p = self.pos[s.other as usize];
            ([c[0] + p[0], c[1] + p[1]], m + 1.0)
        });
        let c = [sum[0] / m.max(1.0), sum[1] / m.max(1.0)];
        let d = |s: &&Spring| {
            let p = self.pos[s.other as usize];
            (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2)
        };
        let Some(spring) = born(i).min_by(|a, b| d(a).total_cmp(&d(b))) else {
            if let Some(t) = toward {
                let v = [t[0] - center[0], t[1] - center[1]];
                let l = v[0].hypot(v[1]);
                let most = rim + self.params.link_distance / 2.0;
                if l <= most || l < 1e-3 {
                    return t;
                }
                return [center[0] + v[0] / l * most, center[1] + v[1] / l * most];
            }
            let r = rim + self.params.link_distance / 2.0;
            return [center[0] + r * turn.cos(), center[1] + r * turn.sin()];
        };
        let anchor = self.pos[spring.other as usize];
        let length = link.map_or(spring.rest, |l| l.max(spring.rest));
        if let Some(t) = toward {
            let v = [t[0] - anchor[0], t[1] - anchor[1]];
            let l = v[0].hypot(v[1]);
            if l > 1e-3 {
                let clear = |p: [f32; 2]| {
                    born(spring.other as usize).all(|s| {
                        let q = self.pos[s.other as usize];
                        let room = BIRTH_ROOM * (self.radius[i] + self.radius[s.other as usize]);
                        (p[0] - q[0]).hypot(p[1] - q[1]) >= room
                    })
                };
                let base = v[1].atan2(v[0]);
                let r = l.min(BIRTH_REACH * length);
                for far in [1.0, 1.5] {
                    for k in 0..BIRTH_TRIES {
                        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
                        let a = base + side * k.div_ceil(2) as f32 * BIRTH_TURN;
                        let p = [anchor[0] + r * far * a.cos(), anchor[1] + r * far * a.sin()];
                        if clear(p) {
                            return p;
                        }
                    }
                }
            }
        }
        let mut around: Vec<f32> = born(spring.other as usize)
            .map(|s| {
                let p = self.pos[s.other as usize];
                (p[1] - anchor[1]).atan2(p[0] - anchor[0])
            })
            .collect();
        around.sort_by(f32::total_cmp);
        let tau = std::f32::consts::TAU;
        let widest = (0..around.len())
            .map(|k| {
                let next = around.get(k + 1).map_or(around[0] + tau, |&b| b);
                (next - around[k], around[k])
            })
            .max_by(|x, y| x.0.total_cmp(&y.0));
        let angle = match widest {
            Some((gap, from)) => from + gap / 2.0,
            None => {
                let out = [anchor[0] - center[0], anchor[1] - center[1]];
                if out[0].abs() + out[1].abs() > 1e-3 {
                    out[1].atan2(out[0])
                } else {
                    turn
                }
            }
        };
        [
            anchor[0] + length * angle.cos(),
            anchor[1] + length * angle.sin(),
        ]
    }

    pub fn end_growth(&mut self) {
        let Some(growth) = self.growth.as_mut() else {
            return;
        };
        if growth.complete() && !self.settled {
            growth.ended = true;
            return;
        }
        let Some(growth) = self.growth.take() else {
            return;
        };
        match growth.rest {
            Some(rest) => self.restore(rest),
            None if !growth.complete() => {
                self.alpha = self.alpha.max(GROW_ALPHA);
                self.settled = self.pos.is_empty();
            }
            None => {}
        }
    }

    fn restore(&mut self, rest: Vec<[f32; 2]>) {
        self.pos = rest;
        self.vel.iter_mut().for_each(|v| *v = [0.0, 0.0]);
        self.heat = Vec::new();
        self.settled = true;
    }

    pub fn born(&self) -> Option<usize> {
        self.growth.as_ref().map(|g| g.born)
    }

    pub fn positions(&self) -> &[[f32; 2]] {
        &self.pos
    }

    pub fn is_settled(&self) -> bool {
        self.settled
    }

    pub fn pin(&mut self, node: u32, position: Option<[f32; 2]>) {
        let i = node as usize;
        if i >= self.pos.len() {
            return;
        }
        self.pinned[i] = position;
        if let Some(p) = position {
            self.pos[i] = p;
            self.vel[i] = [0.0, 0.0];
            self.moving = None;
            self.alpha = self.alpha.max(0.1);
            self.settled = false;
        }
    }

    pub fn reheat(&mut self, params: Params) {
        let rest_changed = params.link_distance != self.params.link_distance
            || params.attraction != self.params.attraction;
        if rest_changed {
            let ratio = params.attraction / self.params.attraction.max(f32::EPSILON);
            for s in &mut self.springs {
                s.rest += params.link_distance - self.params.link_distance;
                s.strength *= ratio;
            }
        }
        self.params = params;
        self.moving = None;
        self.alpha = self.alpha.max(0.5);
        self.settled = self.pos.is_empty();
    }

    pub fn step(&mut self, iterations: u32) -> f32 {
        let mut last = 0.0;
        for _ in 0..iterations {
            if self.settled {
                break;
            }
            last = self.tick();
        }
        last
    }

    fn tick(&mut self) -> f32 {
        let n = self.pos.len();
        let mut forces = 1.0;
        if let Some(g) = self.growth.as_mut() {
            g.pull += (g.pull_for() - g.pull) * PULL_EASE;
            if g.complete() && g.rest.is_some() {
                g.closing += 1;
                forces = 1.0 - (g.closing as f32 / CLOSE_STEPS as f32).min(1.0);
            }
        }
        let alive = self.growth.as_ref().map(|g| g.alive.as_slice());
        if self.moving.is_none() || !self.tree_built {
            self.tree.build(&self.pos, &self.mass, alive);
            self.tree_built = true;
        }
        let shape = self
            .growth
            .as_ref()
            .and_then(|g| Some((g.shape(&self.pos, g.rest.as_deref()?), g.rest.as_deref()?)));
        let alpha = self.alpha;
        let p = self.params;
        let theta2 = p.theta * p.theta;
        let (pos, vel, mass, radius, tree) =
            (&self.pos, &self.vel, &self.mass, &self.radius, &self.tree);
        let (springs, starts) = (&self.springs, &self.spring_start);
        let heat = &self.heat;
        let order: &[u32] = self.moving.as_deref().unwrap_or(&tree.order);
        let by_order: Vec<[f32; 2]> = order
            .par_iter()
            .map_init(Vec::new, |stack, &i| {
                let i = i as usize;
                let alpha = heat.get(i).map_or(alpha, |&h| alpha.max(h));
                let charge = CHARGE * p.repulsion * alpha;
                let mut d = if forces > 0.0 {
                    tree.repulse(i, Bodies { pos, mass, radius }, charge, theta2, stack)
                } else {
                    [0.0; 2]
                };
                let pi = [pos[i][0] + vel[i][0], pos[i][1] + vel[i][1]];
                for s in &springs[starts[i] as usize..starts[i + 1] as usize] {
                    let j = s.other as usize;
                    if alive.is_some_and(|a| !a[j]) {
                        continue;
                    }
                    let x = [pos[j][0] + vel[j][0] - pi[0], pos[j][1] + vel[j][1] - pi[1]];
                    let l = (x[0] * x[0] + x[1] * x[1]).sqrt().max(1e-3);
                    let f = (l - s.rest) / l * alpha * s.strength * s.share;
                    d[0] += x[0] * f;
                    d[1] += x[1] * f;
                }
                d[0] -= pos[i][0] * p.gravity * alpha;
                d[1] -= pos[i][1] * p.gravity * alpha;
                d[0] *= forces;
                d[1] *= forces;
                if let Some((shape, rest)) = &shape {
                    let t = shape.target(rest[i]);
                    d[0] += (t[0] - pos[i][0]) * shape.pull;
                    d[1] += (t[1] - pos[i][1]) * shape.pull;
                }
                d
            })
            .collect();
        let max_speed = p.link_distance
            * if self.growth.is_some() {
                GROWTH_SPEED
            } else {
                4.0
            };
        if let Some(moving) = &self.moving {
            let mut max = 0.0f32;
            for (&i, d) in moving.iter().zip(by_order) {
                let i = i as usize;
                max = max.max(integrate(
                    &mut self.pos[i],
                    &mut self.vel[i],
                    d,
                    self.pinned[i],
                    max_speed,
                ));
            }
            return self.cool(max);
        }
        let mut deltas = vec![[0.0f32; 2]; n];
        for (&i, d) in tree.order.iter().zip(by_order) {
            deltas[i as usize] = d;
        }
        let max = self
            .pos
            .par_iter_mut()
            .zip(self.vel.par_iter_mut())
            .zip(deltas.par_iter())
            .zip(self.pinned.par_iter())
            .map(|(((x, v), d), pin)| integrate(x, v, *d, *pin, max_speed))
            .reduce(|| 0.0, f32::max);
        self.cool(max)
    }

    fn cool(&mut self, max: f32) -> f32 {
        self.alpha += -self.alpha * self.alpha_decay;
        let mut warmest = 0.0f32;
        for h in &mut self.heat {
            *h *= NEWBORN_COOLING;
            warmest = warmest.max(*h);
        }
        if warmest < self.alpha.max(ALPHA_MIN) {
            self.heat = Vec::new();
        }
        let still = max < 0.002 * self.params.link_distance;
        let closing = self
            .growth
            .as_ref()
            .is_some_and(|g| g.complete() && g.rest.is_some());
        let cool = if closing {
            still
        } else {
            self.alpha < ALPHA_MIN || (self.alpha < 0.05 && still)
        };
        if self.heat.is_empty() && cool {
            self.settled = self.pinned.iter().all(Option::is_none);
            if self.settled
                && let Some(growth) = self.growth.as_mut()
                && growth.complete()
            {
                if let Some(rest) = &growth.rest {
                    self.pos.copy_from_slice(rest);
                    self.vel.iter_mut().for_each(|v| *v = [0.0, 0.0]);
                }
                if growth.ended {
                    self.growth = None;
                }
            }
        }
        max
    }
}

fn integrate(
    x: &mut [f32; 2],
    v: &mut [f32; 2],
    d: [f32; 2],
    pin: Option<[f32; 2]>,
    max_speed: f32,
) -> f32 {
    if let Some(p) = pin {
        *x = p;
        *v = [0.0, 0.0];
        return 0.0;
    }
    v[0] = (v[0] + d[0]) * (1.0 - VELOCITY_DECAY);
    v[1] = (v[1] + d[1]) * (1.0 - VELOCITY_DECAY);
    let speed = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if speed > max_speed {
        v[0] *= max_speed / speed;
        v[1] *= max_speed / speed;
    }
    x[0] += v[0];
    x[1] += v[1];
    speed.min(max_speed)
}

#[derive(Clone, Copy)]
struct Cell {
    com: [f32; 2],
    mass: f32,
    center: [f32; 2],
    half: f32,
    start: u32,
    end: u32,
    children: [u32; 4],
    leaf: bool,
}

#[derive(Clone, Copy)]
struct Bodies<'a> {
    pos: &'a [[f32; 2]],
    mass: &'a [f32],
    radius: &'a [f32],
}

#[derive(Default)]
struct Tree {
    cells: Vec<Cell>,
    order: Vec<u32>,
    scratch: Vec<u32>,
}

impl Tree {
    fn build(&mut self, pos: &[[f32; 2]], mass: &[f32], alive: Option<&[bool]>) {
        self.cells.clear();
        self.order.clear();
        match alive {
            Some(alive) => self
                .order
                .extend((0..pos.len() as u32).filter(|&i| alive[i as usize])),
            None => self.order.extend(0..pos.len() as u32),
        }
        let n = self.order.len();
        self.scratch.resize(n, 0);
        if n == 0 {
            return;
        }
        let b = self
            .order
            .iter()
            .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, &i| {
                let p = pos[i as usize];
                [
                    b[0].min(p[0]),
                    b[1].min(p[1]),
                    b[2].max(p[0]),
                    b[3].max(p[1]),
                ]
            });
        let half = ((b[2] - b[0]).max(b[3] - b[1]) / 2.0).max(1.0) * 1.0001;
        let center = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        self.cells.push(Cell {
            com: [0.0; 2],
            mass: 0.0,
            center,
            half,
            start: 0,
            end: n as u32,
            children: [u32::MAX; 4],
            leaf: true,
        });
        self.split(0, pos, mass, 0);
    }

    fn split(&mut self, cell: usize, pos: &[[f32; 2]], mass: &[f32], depth: u32) {
        let Cell {
            center,
            half,
            start,
            end,
            ..
        } = self.cells[cell];
        let (s, e) = (start as usize, end as usize);
        if e - s <= LEAF || depth >= 24 {
            let (mut m, mut cx, mut cy) = (0.0f32, 0.0f32, 0.0f32);
            for &i in &self.order[s..e] {
                let i = i as usize;
                m += mass[i];
                cx += pos[i][0] * mass[i];
                cy += pos[i][1] * mass[i];
            }
            let c = &mut self.cells[cell];
            c.mass = m;
            c.com = [cx / m.max(f32::EPSILON), cy / m.max(f32::EPSILON)];
            return;
        }
        let quadrant = |i: u32| -> usize {
            let p = pos[i as usize];
            (p[0] >= center[0]) as usize | (((p[1] >= center[1]) as usize) << 1)
        };
        let mut counts = [0usize; 4];
        for &i in &self.order[s..e] {
            counts[quadrant(i)] += 1;
        }
        let mut at = [
            s,
            s + counts[0],
            s + counts[0] + counts[1],
            s + counts[0] + counts[1] + counts[2],
        ];
        let bounds_q = at;
        for k in s..e {
            let i = self.order[k];
            let q = quadrant(i);
            self.scratch[at[q]] = i;
            at[q] += 1;
        }
        self.order[s..e].copy_from_slice(&self.scratch[s..e]);
        let h = half / 2.0;
        let mut children = [u32::MAX; 4];
        for q in 0..4 {
            if counts[q] == 0 {
                continue;
            }
            let c = [
                center[0] + if q & 1 == 1 { h } else { -h },
                center[1] + if q & 2 == 2 { h } else { -h },
            ];
            let start = bounds_q[q] as u32;
            children[q] = self.cells.len() as u32;
            self.cells.push(Cell {
                com: [0.0; 2],
                mass: 0.0,
                center: c,
                half: h,
                start,
                end: start + counts[q] as u32,
                children: [u32::MAX; 4],
                leaf: true,
            });
        }
        for &child in children.iter().filter(|&&c| c != u32::MAX) {
            self.split(child as usize, pos, mass, depth + 1);
        }
        let (mut m, mut cx, mut cy) = (0.0f32, 0.0f32, 0.0f32);
        for &child in children.iter().filter(|&&c| c != u32::MAX) {
            let c = &self.cells[child as usize];
            m += c.mass;
            cx += c.com[0] * c.mass;
            cy += c.com[1] * c.mass;
        }
        let c = &mut self.cells[cell];
        c.children = children;
        c.leaf = false;
        c.mass = m;
        c.com = [cx / m.max(f32::EPSILON), cy / m.max(f32::EPSILON)];
    }

    fn repulse(
        &self,
        i: usize,
        bodies: Bodies,
        charge: f32,
        theta2: f32,
        stack: &mut Vec<u32>,
    ) -> [f32; 2] {
        let Bodies { pos, mass, radius } = bodies;
        let mut d = [0.0f32, 0.0f32];
        if self.cells.is_empty() {
            return d;
        }
        let p = pos[i];
        let ri = radius[i];
        stack.clear();
        stack.push(0);
        while let Some(c) = stack.pop() {
            let cell = &self.cells[c as usize];
            let dx = p[0] - cell.com[0];
            let dy = p[1] - cell.com[1];
            let dist2 = dx * dx + dy * dy;
            let width = cell.half * 2.0;
            let leaf = cell.leaf;
            if !leaf && width * width < theta2 * dist2 {
                let f = charge * cell.mass / dist2;
                d[0] += dx * f;
                d[1] += dy * f;
                continue;
            }
            if leaf {
                for &j in &self.order[cell.start as usize..cell.end as usize] {
                    let j = j as usize;
                    if j == i {
                        continue;
                    }
                    let (mut dx, mut dy) = (p[0] - pos[j][0], p[1] - pos[j][1]);
                    let mut dist2 = dx * dx + dy * dy;
                    if dist2 < 1e-6 {
                        let a = (i.wrapping_mul(2654435761) ^ j) as f32;
                        dx = a.cos() * 0.1;
                        dy = a.sin() * 0.1;
                        dist2 = 0.01;
                    }
                    let f = charge * mass[j] / dist2.max(1.0);
                    d[0] += dx * f;
                    d[1] += dy * f;
                    let reach = ri + radius[j];
                    if dist2 < reach * reach {
                        let dist = dist2.sqrt();
                        let push = (reach - dist) / dist * 0.5 * COLLIDE;
                        d[0] += dx * push;
                        d[1] += dy * push;
                    }
                }
                continue;
            }
            stack.extend(cell.children.iter().copied().filter(|&ch| ch != u32::MAX));
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, EdgeKind, Node, NodeKind};
    use std::sync::Arc;

    fn graph(n: usize, edges: &[(u32, u32)]) -> Graph {
        let mut g = Graph::default();
        for i in 0..n {
            g.nodes.push(Node {
                kind: NodeKind::Tag(i.to_string()),
                label: Arc::from(""),
                degree: 0,
                group: 0,
                path: None,
            });
        }
        for &(a, b) in edges {
            g.edges.push(Edge {
                a,
                b,
                kind: EdgeKind::Link,
                mutual: false,
            });
        }
        g
    }

    #[test]
    fn settles_with_linked_nodes_closer_than_unlinked() {
        let mut edges = Vec::new();
        for c in 0..2u32 {
            for i in 1..10u32 {
                edges.push((c * 10, c * 10 + i));
            }
        }
        let g = graph(20, &edges);
        let mut l = Layout::new(&g, &[], Params::default());
        let mut steps = 0;
        while !l.is_settled() && steps < 2000 {
            l.step(10);
            steps += 10;
        }
        assert!(l.is_settled(), "not settled after {steps} steps");
        let p = l.positions();
        let dist =
            |a: usize, b: usize| ((p[a][0] - p[b][0]).powi(2) + (p[a][1] - p[b][1]).powi(2)).sqrt();
        let linked = dist(0, 1);
        let across = dist(0, 10);
        assert!(linked < across, "{linked} >= {across}");
        for a in 0..20 {
            for b in a + 1..20 {
                let deg = |i: usize| if i.is_multiple_of(10) { 9 } else { 1 };
                let min = radius_of(deg(a)) + radius_of(deg(b));
                assert!(
                    dist(a, b) > min * 0.8,
                    "{a} and {b} overlap: {} < {min}",
                    dist(a, b)
                );
            }
        }
        assert_eq!(l.step(5), 0.0, "a settled layout does not move");
    }

    #[test]
    fn isolated_nodes_stay_within_reach() {
        let g = graph(200, &[(0, 1)]);
        let mut l = Layout::new(&g, &[], Params::default());
        l.step(1000);
        let b = bounds(l.positions());
        assert!(b[2] - b[0] < 2000.0 && b[3] - b[1] < 2000.0, "{b:?}");
    }

    #[test]
    fn only_new_nodes_move_after_a_small_change() {
        let edges: Vec<(u32, u32)> = (1..300).map(|i| (i / 2, i)).collect();
        let g = graph(300, &edges);
        let mut l = Layout::new(&g, &[], Params::default());
        while !l.is_settled() {
            l.step(10);
        }
        let before = l.positions().to_vec();
        let seed: Vec<(u32, [f32; 2])> = (0..299).map(|i| (i, before[i as usize])).collect();
        let mut warm = Layout::new(&g, &seed, Params::default());
        let mut steps = 0;
        while !warm.is_settled() {
            warm.step(1);
            steps += 1;
        }
        assert!(steps < 300, "{steps} steps");
        assert_eq!(&warm.positions()[..299], &before[..299]);
        let (p, q) = (warm.positions()[299], warm.positions()[149]);
        let d = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt();
        assert!(d < Params::default().link_distance * 2.0, "{d}");
    }

    #[test]
    fn warm_start_and_pin() {
        let g = graph(3, &[(0, 1), (1, 2)]);
        let seed = [
            (0, [100.0, 100.0]),
            (1, [130.0, 100.0]),
            (2, [160.0, 100.0]),
        ];
        let mut l = Layout::new(&g, &seed, Params::default());
        assert!(l.is_settled(), "a fully seeded layout starts at rest");
        l.pin(1, Some([0.0, 0.0]));
        l.step(50);
        assert_eq!(l.positions()[1], [0.0, 0.0]);
        assert!(!l.is_settled(), "a pinned node keeps the layout awake");
        l.pin(1, None);
        l.step(3000);
        assert!(l.is_settled());
    }

    #[test]
    fn a_growing_graph_moves_only_the_nodes_born() {
        let edges: Vec<(u32, u32)> = (1..40).map(|i| (i / 3, i)).collect();
        let g = graph(40, &edges);
        let mut l = Layout::new(&g, &[], Params::default());
        while !l.is_settled() {
            l.step(10);
        }
        let before = l.positions().to_vec();
        l.grow((0..40).collect());
        l.grow_to(10);
        l.step(30);
        assert_eq!(
            &l.positions()[10..],
            &before[10..],
            "the unborn stay where they were"
        );
        let near = |a: usize, b: usize| {
            ((l.positions()[a][0] - l.positions()[b][0]).powi(2)
                + (l.positions()[a][1] - l.positions()[b][1]).powi(2))
            .sqrt()
        };
        assert!(
            near(9, 3) < Params::default().link_distance * 3.0,
            "a newborn starts by its parent: {}",
            near(9, 3)
        );
        l.grow_to(40);
        l.end_growth();
        let mut steps = 0;
        while !l.is_settled() && steps < 3000 {
            l.step(10);
            steps += 10;
        }
        assert!(l.is_settled(), "settles once grown");
    }

    #[test]
    fn newborns_are_born_next_to_a_linked_node_in_free_room() {
        let edges: Vec<(u32, u32)> = (1..9).map(|i| (0, i)).collect();
        let g = graph(10, &edges);
        let mut l = Layout::new(&g, &[], Params::default());
        l.grow((0..10).collect());
        for k in 1..=9 {
            l.grow_to(k);
        }
        let hub = l.pos[0];
        let rest = Params::default().link_distance + radius_of(8) + radius_of(1);
        let mut angles: Vec<f32> = (1..9)
            .map(|i| {
                let p = l.pos[i];
                let d = (p[0] - hub[0]).hypot(p[1] - hub[1]);
                assert!((d - rest).abs() < 1e-3, "leaf {i} at {d}, rest {rest}");
                (p[1] - hub[1]).atan2(p[0] - hub[0])
            })
            .collect();
        angles.sort_by(f32::total_cmp);
        let tau = std::f32::consts::TAU;
        let narrowest = (0..8)
            .map(|k| angles.get(k + 1).map_or(angles[0] + tau, |&b| b) - angles[k])
            .fold(f32::MAX, f32::min);
        assert!(
            narrowest > tau / 8.0 * 0.99,
            "leaves share the room: {angles:?}"
        );
        let center = l.pos[..9]
            .iter()
            .fold([0.0, 0.0], |c, p| [c[0] + p[0] / 9.0, c[1] + p[1] / 9.0]);
        let from_center = |p: [f32; 2]| (p[0] - center[0]).hypot(p[1] - center[1]);
        l.grow_to(10);
        assert!(
            (1..9).all(|i| from_center(l.pos[9]) > from_center(l.pos[i])),
            "a node linked to nothing is born on the rim"
        );
    }

    #[test]
    fn a_birth_hardly_moves_the_nodes_already_placed() {
        let edges: Vec<(u32, u32)> = (1..300).map(|i| (i / 3, i)).collect();
        let g = graph(300, &edges);
        let mut l = Layout::new(&g, &[], Params::default());
        l.grow((0..300).collect());
        l.grow_to(299);
        while !l.is_settled() {
            l.step(10);
        }
        let before = l.positions().to_vec();
        l.grow_to(300);
        let mut steps = 0;
        while !l.is_settled() && steps < 3000 {
            l.step(1);
            steps += 1;
        }
        assert!(l.is_settled(), "settles after a birth");
        let moved = (0..299)
            .map(|i| (l.pos[i][0] - before[i][0]).hypot(l.pos[i][1] - before[i][1]))
            .fold(0.0f32, f32::max);
        assert!(
            moved < Params::default().link_distance / 3.0,
            "a placed node moved {moved} units"
        );
    }

    fn settled_tree(n: u32, across: bool) -> (Graph, Layout) {
        let mut edges: Vec<(u32, u32)> = (1..n).map(|i| (i / 3, i)).collect();
        if across {
            edges.extend(
                (0..n / 10)
                    .map(|k| (k * 7 % n, (k * 13 + 5) % n))
                    .filter(|(a, b)| a != b),
            );
        }
        let g = graph(n as usize, &edges);
        let mut l = Layout::new(&g, &[], Params::default());
        while !l.is_settled() {
            l.step(10);
        }
        (g, l)
    }

    #[test]
    fn a_newborn_is_born_by_a_linked_note_toward_its_place_at_rest() {
        let (_, mut l) = settled_tree(600, false);
        let rest = l.pos.clone();
        l.grow((0..600).collect());
        let (mut toward, mut checked) = (0, 0);
        for k in 1..600u32 {
            l.grow_to(k as usize + 1);
            l.step(1);
            let (p, q) = (l.pos[k as usize], l.pos[(k / 3) as usize]);
            let born = [p[0] - q[0], p[1] - q[1]];
            let length = born[0].hypot(born[1]);
            let links: Vec<f32> = (1..k)
                .map(|j| {
                    let (a, b) = (l.pos[j as usize], l.pos[(j / 3) as usize]);
                    (a[0] - b[0]).hypot(a[1] - b[1])
                })
                .collect();
            let longest = links.iter().copied().fold(0.0f32, f32::max);
            assert!(
                k < 4 || length <= longest * 1.5,
                "node {k} born {length} from its parent, links up to {longest}"
            );
            if k >= 30 {
                let (r, s) = (rest[k as usize], rest[(k / 3) as usize]);
                let then = [r[0] - s[0], r[1] - s[1]];
                let cos = (born[0] * then[0] + born[1] * then[1])
                    / (length * then[0].hypot(then[1])).max(f32::EPSILON);
                toward += (cos > 0.5) as u32;
                checked += 1;
            }
        }
        assert!(
            toward * 10 >= checked * 8,
            "{toward} of {checked} newborns on the side their place at rest is"
        );
    }

    #[test]
    fn a_replay_ends_exactly_on_the_layout_at_rest_without_a_jolt() {
        let (_, mut l) = settled_tree(600, true);
        let rest = l.pos.clone();
        let link = Params::default().link_distance;
        l.grow((0..600).collect());
        let (mut born, mut steps) = (0, 0);
        let mut last_moves = std::collections::VecDeque::new();
        loop {
            born = (born + 2).min(600);
            l.grow_to(born);
            let before = l.pos.clone();
            l.step(1);
            steps += 1;
            let moved = (0..600)
                .map(|i| (l.pos[i][0] - before[i][0]).hypot(l.pos[i][1] - before[i][1]))
                .fold(0.0f32, f32::max);
            last_moves.push_back(moved);
            if last_moves.len() > 60 {
                last_moves.pop_front();
            }
            if born == 600 && l.is_settled() {
                break;
            }
            assert!(steps < 5000, "the replay does not come to rest");
        }
        assert_eq!(l.pos, rest, "the replay ends on the layout at rest");
        let snap = last_moves.back().copied().unwrap_or(0.0);
        assert!(snap < 0.05 * link, "last step {snap} units");
        let calm = last_moves.iter().copied().fold(0.0f32, f32::max);
        assert!(
            calm < 0.5 * link,
            "a node moved {calm} units in a step of the last second"
        );
        l.end_growth();
        assert!(l.born().is_none() && l.is_settled());
        assert_eq!(l.pos, rest);
    }

    #[test]
    fn a_replay_ended_early_gives_the_layout_at_rest_back_at_once() {
        let (_, mut l) = settled_tree(300, true);
        let rest = l.pos.clone();
        l.grow((0..300).collect());
        l.grow_to(120);
        l.step(40);
        l.end_growth();
        assert!(l.born().is_none() && l.is_settled());
        assert_eq!(l.pos, rest);
    }

    #[test]
    fn barnes_hut_matches_exact_sum() {
        let n = 400;
        let g = graph(n, &[]);
        let l = Layout::new(&g, &[], Params::default());
        let mut tree = Tree::default();
        tree.build(&l.pos, &l.mass, None);
        let mut stack = Vec::new();
        for i in [0usize, 57, 399] {
            let bodies = Bodies {
                pos: &l.pos,
                mass: &l.mass,
                radius: &l.radius,
            };
            let approx = tree.repulse(i, bodies, 1.0, 0.81, &mut stack);
            let exact = tree.repulse(i, bodies, 1.0, 0.0, &mut stack);
            let err = ((approx[0] - exact[0]).powi(2) + (approx[1] - exact[1]).powi(2)).sqrt();
            let norm = (exact[0].powi(2) + exact[1].powi(2)).sqrt();
            assert!(
                err <= 0.1 * norm + 1e-4,
                "node {i}: {approx:?} against {exact:?}"
            );
        }
    }
}
