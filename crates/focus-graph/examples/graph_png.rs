use focus_core::{OpenOptions, Vault};
use focus_graph::{NodeKind, Params, Simulation, Spec, build};
use std::path::PathBuf;
use std::time::Instant;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

const SIZE: u32 = 2400;
const MARGIN: f32 = 40.0;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let tags = args.iter().any(|a| a == "--tags");
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let [vault_path, out] = positional.as_slice() else {
        anyhow::bail!("usage: graph_png <vault> <out.png> [--tags]")
    };
    let root = PathBuf::from(vault_path);
    let label = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/graph-cache")
        .join(label);
    let vault = Vault::open(
        &root,
        OpenOptions {
            cache_dir: cache,
            ignore: Vec::new(),
            watch: false,
        },
    )?;

    let t = Instant::now();
    let graph = build(
        &vault,
        &Spec {
            tags,
            ..Spec::default()
        },
    );
    let build_ms = t.elapsed().as_secs_f64() * 1000.0;
    let t = Instant::now();
    let mut sim = Simulation::new(&graph, &[], Params::default());
    let mut steps = 0;
    while !sim.is_settled() && steps < 5000 {
        sim.step(10);
        steps += 10;
    }
    let layout_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "{} nodes, {} edges; build {build_ms:.0} ms; layout {layout_ms:.0} ms, {steps} steps, settled {}",
        graph.nodes.len(),
        graph.edges.len(),
        sim.is_settled()
    );

    let pos = sim.positions();
    let b = sim.bounds();
    let scale = (SIZE as f32 - 2.0 * MARGIN) / (b[2] - b[0]).max(b[3] - b[1]).max(1.0);
    let map = |p: [f32; 2]| {
        (
            (p[0] - b[0]) * scale + MARGIN,
            (p[1] - b[1]) * scale + MARGIN,
        )
    };
    let mut pixmap = Pixmap::new(SIZE, SIZE).ok_or_else(|| anyhow::anyhow!("pixmap allocation"))?;
    pixmap.fill(Color::BLACK);

    let mut edge_paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    for mutual in [false, true] {
        let mut pb = PathBuilder::new();
        for e in graph.edges.iter().filter(|e| e.mutual == mutual) {
            let (x0, y0) = map(pos[e.a as usize]);
            let (x1, y1) = map(pos[e.b as usize]);
            pb.move_to(x0, y0);
            pb.line_to(x1, y1);
        }
        if let Some(path) = pb.finish() {
            let alpha = if mutual { 150 } else { 70 };
            edge_paint.set_color_rgba8(200, 200, 200, alpha);
            let stroke = Stroke {
                width: if mutual { 1.2 } else { 0.7 },
                ..Stroke::default()
            };
            pixmap.stroke_path(&path, &edge_paint, &stroke, Transform::identity(), None);
        }
    }

    let mut node_paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    node_paint.set_color_rgba8(235, 235, 235, 255);
    for (i, n) in graph.nodes.iter().enumerate() {
        let (x, y) = map(pos[i]);
        let r = ((3.0 + 1.5 * (n.degree as f32).sqrt()) * scale).max(1.2);
        match n.kind {
            NodeKind::Tag(_) => {
                if let Some(rect) = Rect::from_xywh(x - r, y - r, 2.0 * r, 2.0 * r) {
                    pixmap.fill_rect(rect, &node_paint, Transform::identity(), None);
                }
            }
            NodeKind::Unresolved(_) => {
                if let Some(path) = PathBuilder::from_circle(x, y, r) {
                    pixmap.stroke_path(
                        &path,
                        &node_paint,
                        &Stroke {
                            width: 1.0,
                            ..Stroke::default()
                        },
                        Transform::identity(),
                        None,
                    );
                }
            }
            _ => {
                if let Some(path) = PathBuilder::from_circle(x, y, r) {
                    pixmap.fill_path(
                        &path,
                        &node_paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }
    }
    pixmap.save_png(out.as_str())?;
    println!("written {out}");
    Ok(())
}
