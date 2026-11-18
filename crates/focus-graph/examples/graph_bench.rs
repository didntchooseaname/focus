use focus_core::{OpenOptions, Vault};
use focus_graph::{
    Changes, Edge, EdgeKind, Graph, Node, NodeKind, Params, Simulation, SpatialIndex, Spec, build,
    update,
};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

macro_rules! say {
    ($($t:tt)*) => {
        if writeln!(std::io::stdout().lock(), $($t)*).is_err() {
            std::process::exit(0);
        }
    };
}

fn rss_mb() -> f64 {
    // SAFETY: returns free glibc arena memory to the system; memory in use is untouched.
    unsafe {
        libc::malloc_trim(0);
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
        / 1024.0
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn synthetic(nodes: usize, edges: usize) -> Graph {
    let mut state: u64 = 0x9E3779B97F4A7C15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let per_node = edges / nodes;
    let communities = 200usize;
    let mut ends: Vec<Vec<u32>> = vec![Vec::new(); communities];
    let mut all_edges = Vec::with_capacity(edges);
    for i in 0..nodes as u32 {
        let c = (next() % communities as u64) as usize;
        for _ in 0..per_node {
            let pool = if next() % 10 < 9 && !ends[c].is_empty() {
                &ends[c]
            } else {
                &ends[(next() % communities as u64) as usize]
            };
            if pool.is_empty() {
                continue;
            }
            let j = pool[(next() % pool.len() as u64) as usize];
            if j != i {
                all_edges.push(Edge {
                    a: i,
                    b: j,
                    kind: EdgeKind::Link,
                    mutual: false,
                });
            }
        }
        let made = all_edges.len();
        for e in &all_edges[made.saturating_sub(per_node)..] {
            if e.a == i {
                ends[c].push(e.b);
            }
        }
        ends[c].push(i);
    }
    let mut degree = vec![0u32; nodes];
    for e in &all_edges {
        degree[e.a as usize] += 1;
        degree[e.b as usize] += 1;
    }
    let nodes = (0..nodes)
        .map(|i| Node {
            kind: NodeKind::Tag(i.to_string()),
            label: Arc::from(""),
            degree: degree[i],
            group: 0,
            path: None,
        })
        .collect();
    Graph {
        nodes,
        edges: all_edges,
        groups: vec![String::new()],
    }
}

fn measure_updates(vault: &Vault) -> anyhow::Result<()> {
    let spec = Spec::default();
    let t = Instant::now();
    let mut graph = build(vault, &spec);
    say!(
        "full build: {} nodes, {} edges in {:.0} ms",
        graph.nodes.len(),
        graph.edges.len(),
        ms(t)
    );
    let mut sim = Simulation::new(&graph, &[], Params::default());
    while !sim.is_settled() {
        sim.step(10);
    }
    let hub = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.kind, NodeKind::Note(_)))
        .max_by_key(|(_, n)| n.degree)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let NodeKind::Note(hub_id) = graph.nodes[hub].kind else {
        anyhow::bail!("no note in the vault")
    };
    let hub_path = graph.nodes[hub]
        .path
        .clone()
        .unwrap_or_else(|| Arc::from(std::path::Path::new("")));
    let hub_name = hub_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let new_path = std::path::Path::new("graph-bench-update.md");
    let added = vault.create_note(new_path, &format!("# Bench update\n[[{hub_name}]]\n"))?;
    let original = vault.read(hub_id)?;
    type Step<'a> = (&'static str, Box<dyn Fn() -> anyhow::Result<Changes> + 'a>);
    let steps: [Step; 3] = [
        (
            "note added, linking to the largest hub",
            Box::new(move || {
                Ok(Changes {
                    changed: vec![added],
                    removed: vec![],
                })
            }),
        ),
        (
            "largest hub edited",
            Box::new(|| {
                vault.write(
                    hub_id,
                    &format!("{original}\n[[graph-bench-update]]\n"),
                    None,
                )?;
                Ok(Changes {
                    changed: vec![hub_id],
                    removed: vec![],
                })
            }),
        ),
        (
            "note removed",
            Box::new(|| {
                vault.trash(new_path)?;
                vault.write(hub_id, &original, None)?;
                Ok(Changes {
                    changed: vec![hub_id],
                    removed: vec![added],
                })
            }),
        ),
    ];
    for (what, change) in steps {
        let changes = change()?;
        let t = Instant::now();
        let next = update(vault, &spec, &graph, &changes)
            .ok_or_else(|| anyhow::anyhow!("update refused"))?;
        let update_ms = ms(t);
        let t = Instant::now();
        let full = build(vault, &spec);
        let full_ms = ms(t);
        anyhow::ensure!(
            next.nodes.len() == full.nodes.len() && next.edges.len() == full.edges.len(),
            "update differs from the full build"
        );
        let positions = sim.positions();
        let old: std::collections::HashMap<&std::path::Path, [f32; 2]> = graph
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| Some((n.path.as_deref()?, positions[i])))
            .collect();
        let seed: Vec<(u32, [f32; 2])> = next
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| Some((i as u32, *old.get(n.path.as_deref()?)?)))
            .collect();
        let t = Instant::now();
        let mut warm = Simulation::new(&next, &seed, Params::default());
        let start_ms = ms(t);
        let t = Instant::now();
        let mut layout_steps = 0;
        while !warm.is_settled() {
            warm.step(1);
            layout_steps += 1;
        }
        say!(
            "{what}: update {update_ms:.1} ms, full build {full_ms:.0} ms; layout restart {start_ms:.0} ms, settled in {layout_steps} steps, {:.0} ms",
            ms(t)
        );
        graph = next;
        sim = warm;
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let Some(vault_path) = std::env::args()
        .nth(1)
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
    else {
        anyhow::bail!("usage: graph_bench <vault> [--update]");
    };
    if vault_path.exists() {
        let cache =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/graph-cache/bench");
        let vault = Vault::open(
            &vault_path,
            OpenOptions {
                cache_dir: cache,
                ignore: Vec::new(),
                watch: false,
            },
        )?;
        if std::env::args().any(|a| a == "--update") {
            let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target")
                .canonicalize()?;
            anyhow::ensure!(
                vault_path.canonicalize()?.starts_with(&target),
                "--update writes into the vault: use a vault under {}",
                target.display()
            );
            return measure_updates(&vault);
        }
        for tags in [false, true] {
            let t = Instant::now();
            let graph = build(
                &vault,
                &Spec {
                    tags,
                    ..Spec::default()
                },
            );
            let built = ms(t);
            let mut sim = Simulation::new(&graph, &[], Params::default());
            let mut steps = 0;
            while !sim.is_settled() {
                sim.step(1);
                steps += 1;
            }
            say!(
                "[{}] {} nodes, {} edges{}: settled without position cache in {:.0} ms (build {built:.0} ms, {steps} steps)",
                vault_path.display(),
                graph.nodes.len(),
                graph.edges.len(),
                if tags { ", tags" } else { "" },
                ms(t)
            );
        }
    }

    let base = rss_mb();
    let t = Instant::now();
    let graph = synthetic(100_000, 300_000);
    say!(
        "synthetic graph: {} nodes, {} edges, generated in {:.0} ms",
        graph.nodes.len(),
        graph.edges.len(),
        ms(t)
    );
    let mut sim = Simulation::new(&graph, &[], Params::default());
    let mut times = Vec::new();
    for _ in 0..30 {
        let t = Instant::now();
        sim.step(1);
        times.push(ms(t));
    }
    times.sort_by(f64::total_cmp);
    say!(
        "one layout step, 100 000 nodes and 300 000 edges: median {:.1} ms, p95 {:.1} ms, max {:.1} ms (30 steps, {} rayon threads)",
        times[times.len() / 2],
        times[(times.len() * 95) / 100],
        times[times.len() - 1],
        rayon::current_num_threads()
    );
    let t = Instant::now();
    let index = SpatialIndex::new(sim.positions());
    let index_ms = ms(t);
    let t = Instant::now();
    let mut hits = 0;
    for p in sim.positions().iter().step_by(100) {
        hits += index.nearest(*p, 5.0).is_some() as usize;
    }
    say!(
        "spatial index: built in {index_ms:.1} ms, 1000 nearest queries in {:.2} ms ({hits} hits)",
        ms(t)
    );
    say!(
        "resident memory, graph and simulation of 100 000 nodes, after malloc_trim: +{:.1} MB",
        rss_mb() - base
    );
    Ok(())
}
