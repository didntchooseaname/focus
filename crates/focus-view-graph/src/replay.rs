use focus_core::{NoteMeta, PropValue, Vault};
use focus_graph::{Graph, NodeKind};
use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub(crate) const GROW_SECONDS: f32 = 0.6;
pub(crate) const FADE_SECONDS: f32 = 1.0;
pub(crate) const FRESH_SECONDS: f32 = GROW_SECONDS + FADE_SECONDS;
pub(crate) const REPLAY_SECONDS: f32 = 30.0;
const BIRTHS_PER_SECOND: f32 = 100.0;
const MIN_SPAN: f32 = 1.0 / 400.0;
const MAX_SPAN: f32 = 0.25;
const SPAN_STEPS: f32 = 1.5;
const FIRST_SPAN: f32 = SPAN_STEPS / 60.0;

pub(crate) struct Glide {
    from: Arc<Vec<[f32; 2]>>,
    to: Arc<Vec<[f32; 2]>>,
    start: Instant,
    span: f32,
    arrived: Option<Instant>,
}

impl Glide {
    pub fn new(at: Arc<Vec<[f32; 2]>>, now: Instant) -> Glide {
        Glide {
            from: at.clone(),
            to: at,
            start: now,
            span: FIRST_SPAN,
            arrived: None,
        }
    }

    pub fn arrive(&mut self, to: Arc<Vec<[f32; 2]>>, fresh: &[u32], now: Instant) {
        if let Some(then) = self.arrived {
            let gap = SPAN_STEPS
                * now
                    .saturating_duration_since(then)
                    .as_secs_f32()
                    .clamp(MIN_SPAN, MAX_SPAN);
            self.span += (gap - self.span) / 4.0;
        }
        self.arrived = Some(now);
        let (at, _) = self.at(now);
        let mut from = if at.len() == to.len() {
            Arc::unwrap_or_clone(at)
        } else {
            to.to_vec()
        };
        for &i in fresh {
            if let (Some(slot), Some(p)) = (from.get_mut(i as usize), to.get(i as usize)) {
                *slot = *p;
            }
        }
        self.from = Arc::new(from);
        self.to = to;
        self.start = now;
    }

    pub fn at(&self, now: Instant) -> (Arc<Vec<[f32; 2]>>, bool) {
        let s = now.saturating_duration_since(self.start).as_secs_f32() / self.span;
        if s >= 1.0 {
            return (self.to.clone(), false);
        }
        if s <= 0.0 {
            return (self.from.clone(), true);
        }
        let at = self
            .from
            .iter()
            .zip(self.to.iter())
            .map(|(a, b)| [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s])
            .collect();
        (Arc::new(at), true)
    }
}

pub(crate) fn growth(age: f32) -> f32 {
    let t = (age / GROW_SECONDS).clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub(crate) fn glow(age: f32) -> f32 {
    let t = ((age - GROW_SECONDS) / FADE_SECONDS).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

pub(crate) fn replay_seconds(nodes: usize) -> f32 {
    REPLAY_SECONDS.max(nodes as f32 / BIRTHS_PER_SECOND)
}

pub(crate) fn born_at(progress: f32, total: usize) -> usize {
    ((progress.clamp(0.0, 1.0) * total as f32).ceil() as usize).min(total)
}

pub(crate) struct Links {
    start: Vec<u32>,
    list: Vec<u32>,
}

impl Links {
    pub fn of(graph: &Graph) -> Links {
        let n = graph.nodes.len();
        let mut start = vec![0u32; n + 1];
        for e in &graph.edges {
            start[e.a as usize + 1] += 1;
            start[e.b as usize + 1] += 1;
        }
        for i in 1..=n {
            start[i] += start[i - 1];
        }
        let mut fill = start.clone();
        let mut list = vec![0u32; start[n] as usize];
        for e in &graph.edges {
            list[fill[e.a as usize] as usize] = e.b;
            fill[e.a as usize] += 1;
            list[fill[e.b as usize] as usize] = e.a;
            fill[e.b as usize] += 1;
        }
        Links { start, list }
    }

    pub fn of_node(&self, i: u32) -> &[u32] {
        let i = i as usize;
        match (self.start.get(i), self.start.get(i + 1)) {
            (Some(&a), Some(&b)) => &self.list[a as usize..b as usize],
            _ => &[],
        }
    }
}

pub(crate) fn birth_order(vault: &Vault, graph: &Graph) -> (Vec<u32>, Vec<i64>) {
    let mut time: Vec<i64> = graph
        .nodes
        .iter()
        .map(|node| match &node.kind {
            NodeKind::Note(id) => vault
                .note(*id)
                .map_or(i64::MAX, |meta| note_time(vault.root(), &meta)),
            NodeKind::Attachment(path) => file_time(&vault.root().join(path)).unwrap_or(i64::MAX),
            NodeKind::Tag(_) | NodeKind::Unresolved(_) => i64::MAX,
        })
        .collect();
    for edge in &graph.edges {
        let (a, b) = (edge.a as usize, edge.b as usize);
        let (ta, tb) = (time[a], time[b]);
        if !is_note(graph, a) {
            time[a] = ta.min(tb);
        }
        if !is_note(graph, b) {
            time[b] = tb.min(ta);
        }
    }
    let mut order: Vec<u32> = (0..graph.nodes.len() as u32).collect();
    order.sort_by_key(|&i| (time[i as usize], i));
    linked_first(graph, &time, &mut order);
    let times = order.iter().map(|&i| time[i as usize]).collect();
    (order, times)
}

fn linked_first(graph: &Graph, time: &[i64], order: &mut [u32]) {
    let n = graph.nodes.len();
    let links = Links::of(graph);
    let linked = |i: u32| links.of_node(i);
    let mut state = vec![0u8; n];
    let mut queue = std::collections::VecDeque::new();
    let mut run = 0;
    while run < order.len() {
        let t = time[order[run] as usize];
        let end = run + order[run..].partition_point(|&i| time[i as usize] == t);
        let members = order[run..end].to_vec();
        let mut placed = run;
        let mut next = 0;
        queue.clear();
        for &i in &members {
            if linked(i).iter().any(|&j| state[j as usize] == 2) {
                state[i as usize] = 1;
                queue.push_back(i);
            }
        }
        while placed < end {
            let i = match queue.pop_front() {
                Some(i) => i,
                None => {
                    while state[members[next] as usize] != 0 {
                        next += 1;
                    }
                    members[next]
                }
            };
            state[i as usize] = 2;
            order[placed] = i;
            placed += 1;
            for &j in linked(i) {
                if state[j as usize] == 0 && time[j as usize] == t {
                    state[j as usize] = 1;
                    queue.push_back(j);
                }
            }
        }
        run = end;
    }
}

fn is_note(graph: &Graph, i: usize) -> bool {
    matches!(graph.nodes[i].kind, NodeKind::Note(_))
}

fn note_time(root: &Path, meta: &NoteMeta) -> i64 {
    let created = meta
        .props
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("created"))
        .and_then(|(_, value)| match value {
            PropValue::Text(text) => parse_date(text),
            _ => None,
        });
    created
        .or_else(|| file_time(&root.join(&*meta.path)))
        .unwrap_or_else(|| seconds(meta.modified).unwrap_or(i64::MAX))
}

fn file_time(path: &Path) -> Option<i64> {
    let metadata = std::fs::metadata(path).ok()?;
    metadata
        .created()
        .ok()
        .or_else(|| metadata.modified().ok())
        .and_then(seconds)
}

fn seconds(time: SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

pub(crate) fn parse_date(text: &str) -> Option<i64> {
    let text = text.trim();
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    if text.get(4..5)? != "-" || text.get(7..8)? != "-" {
        return None;
    }
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let clock = match text.get(10..11) {
        Some("T" | " ") if text.get(13..14) == Some(":") => {
            number(11..13)? * 3600 + number(14..16)? * 60
        }
        _ => 0,
    };
    Some(days_from_civil(year, month, day) * 86_400 + clock)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use focus_core::OpenOptions;
    use focus_graph::{Edge, EdgeKind, Node};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn glide_moves_at_every_frame_without_jump_and_newborns_start_in_place() {
        let start = Instant::now();
        let step = |k: u32| {
            Arc::new(vec![
                [10.0 * k as f32, 0.0],
                [500.0 + (k >= 3) as u32 as f32 * 100.0, 0.0],
            ])
        };
        let mut glide = Glide::new(step(0), start);
        let mut drawn: Vec<[f32; 2]> = step(0).to_vec();
        let mut moves = Vec::new();
        let mut next_step = 1;
        for frame in 1..=320 {
            let now = start + Duration::from_secs_f64(frame as f64 / 320.0);
            while (next_step as f64) / 60.0 <= frame as f64 / 320.0 {
                let fresh: &[u32] = if next_step == 3 { &[1] } else { &[] };
                glide.arrive(step(next_step), fresh, now);
                next_step += 1;
            }
            let (at, _) = glide.at(now);
            moves.push(at[0][0] - drawn[0][0]);
            if next_step > 3 {
                assert_eq!(
                    at[1][0], 600.0,
                    "a newborn does not slide from where it waited"
                );
            }
            drawn = at.to_vec();
        }
        let per_frame = 10.0 * 60.0 / 320.0;
        let steady = &moves[20..];
        assert!(
            steady.iter().all(|&m| m > 0.0 && m < per_frame * 2.0),
            "{steady:?}"
        );
        let total: f32 = moves.iter().sum();
        assert!(
            (total - 10.0 * 60.0).abs() < 10.0 * 2.0,
            "one second of glide covers one second of steps, a step or two behind: {total}"
        );
    }

    #[test]
    fn newborns_grow_in_fast_then_settle() {
        assert_eq!(growth(0.0), 0.0);
        assert_eq!(growth(GROW_SECONDS), 1.0);
        assert_eq!(growth(10.0), 1.0);
        let samples: Vec<f32> = (0..=12)
            .map(|k| growth(k as f32 * GROW_SECONDS / 12.0))
            .collect();
        assert!(samples.windows(2).all(|w| w[1] >= w[0]));
        assert!(
            samples[6] > 0.8,
            "most of the growth in the first half: {}",
            samples[6]
        );
    }

    #[test]
    fn births_sharing_a_date_are_spread_over_the_replay() {
        let total = 2500;
        let frames = 30 * 60;
        let counts: Vec<usize> = (0..=frames)
            .map(|f| born_at(f as f32 / frames as f32, total))
            .collect();
        assert_eq!(counts[0], 0);
        assert_eq!(counts[frames], total);
        assert!(counts.windows(2).all(|w| w[1] >= w[0] && w[1] - w[0] <= 2));
    }

    #[test]
    fn nodes_born_at_the_same_time_follow_their_links() {
        let mut graph = Graph::default();
        for i in 0..6 {
            graph.nodes.push(Node {
                kind: NodeKind::Tag(i.to_string()),
                label: Arc::from(""),
                degree: 0,
                group: 0,
                path: None,
            });
        }
        for (a, b) in [(0, 3), (3, 1), (1, 4), (4, 2), (5, 2)] {
            graph.edges.push(Edge {
                a,
                b,
                kind: EdgeKind::Link,
                mutual: false,
            });
        }
        let time = [7, 7, 7, 7, 7, 1];
        let mut order: Vec<u32> = (0..6).collect();
        order.sort_by_key(|&i| (time[i as usize], i));
        linked_first(&graph, &time, &mut order);
        assert_eq!(order, [5, 2, 4, 1, 3, 0]);
    }

    #[test]
    fn dates_parse_as_written_in_the_notes() {
        assert_eq!(parse_date("1970-01-01"), Some(0));
        assert_eq!(parse_date("2024-01-16T17:19:43.369Z"), Some(1_705_425_540));
        assert_eq!(parse_date("2024-01-27 19:07"), Some(1_706_382_420));
        assert_eq!(parse_date("2024-02-29"), Some(1_709_164_800));
        assert_eq!(parse_date("16/01/2024"), None);
        assert_eq!(parse_date("2024-13-01"), None);
    }

    #[test]
    fn notes_are_born_by_creation_date_and_tags_with_their_first_note() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/focus-view-graph-tests/birth");
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("vault");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("late.md"),
            "---\ncreated: 2024-05-01\n---\n# Late\n#red\n",
        )
        .unwrap();
        std::fs::write(
            root.join("early.md"),
            "---\ncreated: 2023-01-01T10:00\n---\n# Early\n[[late]] #red\n",
        )
        .unwrap();
        std::fs::write(
            root.join("middle.md"),
            "---\ncreated: 2023-06-01\n---\n# Middle\n[[early]]\n",
        )
        .unwrap();
        let vault: Arc<Vault> = Vault::open(
            &root,
            OpenOptions {
                cache_dir: dir.join("cache"),
                ignore: Vec::new(),
                watch: false,
            },
        )
        .unwrap();
        let graph = focus_graph::build(
            &vault,
            &focus_graph::Spec {
                tags: true,
                ..focus_graph::Spec::default()
            },
        );
        let (order, times) = birth_order(&vault, &graph);
        let labels: Vec<String> = order
            .iter()
            .map(|&i| graph.nodes[i as usize].label.to_string())
            .collect();
        assert_eq!(labels.first().map(String::as_str), Some("Early"));
        let at = |label: &str| {
            labels
                .iter()
                .position(|l| l.trim_start_matches('#') == label)
                .unwrap_or(usize::MAX)
        };
        assert!(
            at("Early") < at("Middle") && at("Middle") < at("Late"),
            "{labels:?}"
        );
        assert_eq!(
            times[at("red")],
            times[at("Early")],
            "a tag is born with its first note: {labels:?}"
        );
        assert!(times.windows(2).all(|w| w[0] <= w[1]));
    }
}
