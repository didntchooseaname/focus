use crate::camera::Camera;
use crate::core::{Born, extent};
use crate::plan::{Inputs, NodeTable, plan};
use crate::probe::{Spread, SpreadFigures, spread_of};
use focus_core::{OpenOptions, Vault};
use focus_graph::{Params, Simulation, SpatialIndex, Spec};
use std::path::PathBuf;
use std::sync::Arc;

const SIZE: [f32; 2] = [1600.0, 1000.0];

fn figures(
    graph: &focus_graph::Graph,
    positions: &[[f32; 2]],
    born: Option<&Born>,
) -> SpreadFigures {
    let table = NodeTable::new(graph);
    let index = SpatialIndex::new(positions);
    let camera = Camera::fit(extent(positions, born), SIZE, 48.0);
    let p = plan(&Inputs {
        graph,
        table: &table,
        positions,
        index: &index,
        camera,
        fit_zoom: camera.zoom,
        size: SIZE,
        focus: None,
        neighbors: &[],
        active: None,
        label_width: &|_| 40.0,
        label_scale: 1.0,
        node_scale: 1.0,
        born,
    });
    spread_of(&p.nodes, &p.edges, SIZE)
}

fn overlapping_by_age(
    graph: &focus_graph::Graph,
    positions: &[[f32; 2]],
    born: &Born,
    birth_frame: &[usize],
    frame: usize,
) -> (usize, usize, f32) {
    let table = NodeTable::new(graph);
    let index = SpatialIndex::new(positions);
    let camera = Camera::fit(extent(positions, Some(born)), SIZE, 48.0);
    let p = plan(&Inputs {
        graph,
        table: &table,
        positions,
        index: &index,
        camera,
        fit_zoom: camera.zoom,
        size: SIZE,
        focus: None,
        neighbors: &[],
        active: None,
        label_width: &|_| 40.0,
        label_scale: 1.0,
        node_scale: 1.0,
        born: Some(born),
    });
    let (mut young, mut old) = (0, 0);
    for a in &p.nodes {
        let hit = p.nodes.iter().any(|b| {
            b.index != a.index && (a.at[0] - b.at[0]).hypot(a.at[1] - b.at[1]) < a.radius + b.radius
        });
        if hit {
            if frame.saturating_sub(birth_frame[a.index as usize]) < 60 {
                young += 1;
            } else {
                old += 1;
            }
        }
    }
    (young, old, camera.zoom)
}

fn spread(values: Vec<f32>) -> String {
    let mut v = values;
    v.sort_by(f32::total_cmp);
    if v.is_empty() {
        return "-".into();
    }
    let at = |q: f32| v[((v.len() - 1) as f32 * q).round() as usize];
    let s = Spread {
        median: at(0.5),
        p95: at(0.95),
        max: at(1.0),
    };
    format!("{:.2} {:.2} {:.2}", s.median, s.p95, s.max)
}

#[test]
#[ignore]
fn replay_layout() {
    let Ok(root) = std::env::var("FOCUS_REPLAY_VAULT") else {
        eprintln!("FOCUS_REPLAY_VAULT not set");
        return;
    };
    let speed: f32 = std::env::var("FOCUS_REPLAY_SPEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);
    let steps: u32 = std::env::var("FOCUS_REPLAY_STEPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let root = PathBuf::from(root);
    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/focus-view-graph-tests/quality")
        .join(root.file_name().unwrap_or_default());
    let vault: Arc<Vault> = Vault::open(
        &root,
        OpenOptions {
            cache_dir: cache,
            ignore: Vec::new(),
            watch: false,
        },
    )
    .unwrap();
    let graph = focus_graph::build(&vault, &Spec::default());
    let n = graph.nodes.len();
    let mut sim = Simulation::new(&graph, &[], Params::default());
    while !sim.is_settled() {
        sim.step(10);
    }
    let rest = sim.positions().to_vec();
    let at_rest = figures(&graph, &rest, None);
    let (order, _) = crate::replay::birth_order(&vault, &graph);
    let mut rank = vec![0u32; n];
    for (k, &i) in order.iter().enumerate() {
        rank[i as usize] = k as u32;
    }
    let rank: Arc<[u32]> = rank.into();
    let frames = (crate::replay::replay_seconds(n) * 60.0 / speed) as usize;
    sim.grow(order.clone());
    let (mut overlap, mut ratio, mut fill) = (Vec::new(), Vec::new(), Vec::new());
    let mut placed_move = Vec::new();
    let mut last = sim.positions().to_vec();
    let mut last_camera = Camera::default();
    let mut frame = 0usize;
    let mut born_count = 0usize;
    let mut birth_frame = vec![usize::MAX; n];
    loop {
        let count = crate::replay::born_at((frame + 1) as f32 / frames as f32, n);
        sim.grow_to(count);
        sim.step(steps);
        let born = Born {
            rank: rank.clone(),
            count: sim.born().unwrap_or(n) as u32,
            ..Born::default()
        };
        let pos = sim.positions();
        let camera = Camera::fit(extent(pos, Some(&born)), SIZE, 48.0);
        let moved = (0..born_count)
            .map(|k| {
                let i = order[k] as usize;
                let (a, b) = (
                    camera.to_screen(pos[i], SIZE),
                    last_camera.to_screen(last[i], SIZE),
                );
                (a[0] - b[0]).hypot(a[1] - b[1])
            })
            .fold(0.0f32, f32::max);
        placed_move.push(moved);
        for k in born_count..born.count as usize {
            birth_frame[order[k] as usize] = frame;
        }
        born_count = born.count as usize;
        last = pos.to_vec();
        last_camera = camera;
        if frame.is_multiple_of(15) && born.count >= 50 {
            let f = figures(&graph, pos, Some(&born));
            overlap.push(f.overlap_pct);
            ratio.push(f.link_ratio);
            fill.push(f.fill_pct);
        }
        if (frame + 1).is_multiple_of((frames / 10).max(1)) {
            let f = figures(&graph, pos, Some(&born));
            let (young, old, zoom) = overlapping_by_age(&graph, pos, &born, &birth_frame, frame);
            println!("  overlapping: {young} born in the last second, {old} older, zoom {zoom:.3}");
            println!(
                "at {:>3}%: {} nodes, overlap {:.2}%, link ratio {:.2}, fill {:.1}%, largest placed move {:.2} px",
                (frame + 1) * 100 / frames,
                born.count,
                f.overlap_pct,
                f.link_ratio,
                f.fill_pct,
                placed_move
                    .iter()
                    .rev()
                    .take(frames / 10)
                    .copied()
                    .fold(0.0f32, f32::max)
            );
        }
        frame += 1;
        if count == n {
            break;
        }
    }
    sim.end_growth();
    let mut tail = 0;
    while !sim.is_settled() && tail < 20_000 {
        sim.step(1);
        tail += 1;
    }
    let end = sim.positions().to_vec();
    let at_end = figures(&graph, &end, None);
    let gap = end
        .iter()
        .zip(&rest)
        .map(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]))
        .fold(0.0f32, f32::max);
    println!(
        "replay {} nodes {} edges, speed {speed}, {frames} frames, {tail} steps after the last birth",
        n,
        graph.edges.len()
    );
    println!(
        "during: overlap_pct {}; link_ratio {}; fill_pct {}; placed move px/frame {}",
        spread(overlap),
        spread(ratio),
        spread(fill),
        spread(placed_move)
    );
    println!("end:  {at_end:?}");
    println!("rest: {at_rest:?}");
    println!("end to rest, largest, layout units: {gap:.2}");
}
