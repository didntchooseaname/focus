use super::*;
use focus_core::OpenOptions;
use std::path::PathBuf;

fn put(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

fn vault(name: &str) -> (PathBuf, Arc<Vault>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/focus-graph-tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let root = dir.join("vault");
    put(
        &root,
        "a.md",
        "# A\n[[b]] [c](sub/c.md) [[missing]] ![[img.png]] #red/ad\n",
    );
    put(&root, "b.md", "# B\n[[a]] #red\n");
    put(&root, "sub/c.md", "# C\n[[d]]\n");
    put(&root, "sub/d.md", "# D\n");
    put(&root, "lonely.md", "# Lonely\n");
    put(&root, "img.png", "PNG");
    let v = Vault::open(
        &root,
        OpenOptions {
            cache_dir: dir.join("cache"),
            ignore: Vec::new(),
            watch: false,
        },
    )
    .unwrap();
    (dir, v)
}

fn label_of(g: &Graph, i: u32) -> &str {
    &g.nodes[i as usize].label
}

fn edge_labels(g: &Graph) -> Vec<(String, String, EdgeKind, bool)> {
    let mut v: Vec<_> = g
        .edges
        .iter()
        .map(|e| {
            (
                label_of(g, e.a).to_string(),
                label_of(g, e.b).to_string(),
                e.kind,
                e.mutual,
            )
        })
        .collect();
    v.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    v
}

#[test]
fn notes_and_links() {
    let (_dir, v) = vault("notes");
    let g = build(&v, &Spec::default());
    assert_eq!(g.nodes.len(), 5);
    assert_eq!(
        edge_labels(&g),
        [
            ("A".into(), "B".into(), EdgeKind::Link, true),
            ("A".into(), "C".into(), EdgeKind::Link, false),
            ("C".into(), "D".into(), EdgeKind::Link, false)
        ]
    );
    let a = g
        .node_of_note(v.note_by_path(Path::new("a.md")).unwrap())
        .unwrap();
    assert_eq!(g.nodes[a as usize].degree, 2);
    let mut n: Vec<&str> = g
        .neighbors(a)
        .into_iter()
        .map(|i| label_of(&g, i))
        .collect();
    n.sort();
    assert_eq!(n, ["B", "C"]);
    assert_eq!(g.groups, ["", "sub"]);
    assert_eq!(
        g.groups[g.nodes[g
            .node_of_note(v.note_by_path(Path::new("sub/d.md")).unwrap())
            .unwrap() as usize]
            .group as usize],
        "sub"
    );

    let no_orphans = build(
        &v,
        &Spec {
            orphans: false,
            ..Spec::default()
        },
    );
    assert!(!no_orphans.nodes.iter().any(|n| &*n.label == "Lonely"));
    assert_eq!(no_orphans.edges.len(), 3);
}

#[test]
fn tags_unresolved_attachments() {
    let (_dir, v) = vault("extras");
    let g = build(
        &v,
        &Spec {
            tags: true,
            unresolved: true,
            attachments: true,
            ..Spec::default()
        },
    );
    let labels = edge_labels(&g);
    assert!(labels.contains(&("A".into(), "#red/ad".into(), EdgeKind::Tagged, false)));
    assert!(labels.contains(&("#red/ad".into(), "#red".into(), EdgeKind::TagParent, false)));
    assert!(labels.contains(&("B".into(), "#red".into(), EdgeKind::Tagged, false)));
    assert!(labels.contains(&("A".into(), "missing".into(), EdgeKind::Link, false)));
    assert!(labels.contains(&("A".into(), "img.png".into(), EdgeKind::Embed, false)));
    assert_eq!(
        g.nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Tag(_)))
            .count(),
        2
    );
    assert_eq!(
        labels.iter().filter(|l| l.2 == EdgeKind::TagParent).count(),
        1
    );
}

#[test]
fn a_canvas_links_from_its_attachment_node() {
    let (dir, v) = vault("canvas");
    drop(v);
    let root = dir.join("vault");
    put(
        &root,
        "board.canvas",
        r#"{"nodes":[{"type":"file","file":"sub/d.md"},{"type":"file","file":"img.png"},{"type":"text","text":"[[b]]"}]}"#,
    );
    let v = Vault::open(
        &root,
        OpenOptions {
            cache_dir: dir.join("cache"),
            ignore: Vec::new(),
            watch: false,
        },
    )
    .unwrap();
    let with = build(
        &v,
        &Spec {
            attachments: true,
            ..Spec::default()
        },
    );
    let labels = edge_labels(&with);
    assert!(labels.contains(&("board.canvas".into(), "D".into(), EdgeKind::Embed, false)));
    assert!(labels.contains(&(
        "board.canvas".into(),
        "img.png".into(),
        EdgeKind::Embed,
        false
    )));
    assert_eq!(labels.iter().filter(|l| l.0 == "board.canvas").count(), 2);
    let without = build(&v, &Spec::default());
    assert!(!without.nodes.iter().any(|n| &*n.label == "board.canvas"));
}

#[test]
fn local_and_restricted_graphs() {
    let (_dir, v) = vault("local");
    let d = v.note_by_path(Path::new("sub/d.md")).unwrap();
    let one = build(
        &v,
        &Spec {
            around: Some((d, 1)),
            ..Spec::default()
        },
    );
    let mut names: Vec<&str> = one.nodes.iter().map(|n| &*n.label).collect();
    names.sort();
    assert_eq!(names, ["C", "D"]);
    let two = build(
        &v,
        &Spec {
            around: Some((d, 2)),
            ..Spec::default()
        },
    );
    assert_eq!(two.nodes.len(), 3);
    let a = v.note_by_path(Path::new("a.md")).unwrap();
    let b = v.note_by_path(Path::new("b.md")).unwrap();
    let only = build(
        &v,
        &Spec {
            only: Some(vec![a, b]),
            ..Spec::default()
        },
    );
    assert_eq!(
        edge_labels(&only),
        [("A".into(), "B".into(), EdgeKind::Link, true)]
    );
}

#[test]
fn positions_round_trip_and_warm_start() {
    let (dir, v) = vault("positions");
    let cache = dir.join("cache");
    let g = build(
        &v,
        &Spec {
            tags: true,
            ..Spec::default()
        },
    );
    let mut sim = Simulation::new(&g, &[], Params::default());
    while !sim.is_settled() {
        sim.step(50);
    }
    save_positions(&cache, &g, sim.positions()).unwrap();
    let seed = load_positions(&cache, &g);
    assert_eq!(seed.len(), g.nodes.len());
    for (i, p) in &seed {
        assert_eq!(*p, sim.positions()[*i as usize]);
    }
    put(v.root(), "e.md", "# E\n[[d]]\n");
    let v2 = Vault::open(
        v.root(),
        OpenOptions {
            cache_dir: cache.clone(),
            ignore: Vec::new(),
            watch: false,
        },
    )
    .unwrap();
    let g2 = build(
        &v2,
        &Spec {
            tags: true,
            ..Spec::default()
        },
    );
    let seed2 = load_positions(&cache, &g2);
    assert_eq!(seed2.len(), g2.nodes.len() - 1);
    let sim2 = Simulation::new(&g2, &seed2, Params::default());
    let e = g2
        .node_of_note(v2.note_by_path(Path::new("e.md")).unwrap())
        .unwrap() as usize;
    let dn = g2
        .node_of_note(v2.note_by_path(Path::new("sub/d.md")).unwrap())
        .unwrap() as usize;
    let p = sim2.positions();
    let dist = ((p[e][0] - p[dn][0]).powi(2) + (p[e][1] - p[dn][1]).powi(2)).sqrt();
    assert!(dist < Params::default().link_distance * 2.0, "{dist}");
    std::fs::write(cache.join("graph-positions.bin"), b"garbage").unwrap();
    assert!(load_positions(&cache, &g2).is_empty());

    let index = SpatialIndex::new(sim.positions());
    let first = sim.positions()[0];
    assert_eq!(index.nearest(first, 1.0), Some(0));
    let mut out = Vec::new();
    index.in_rect(sim.bounds(), &mut out);
    assert_eq!(out.len(), g.nodes.len());
}

fn key(g: &Graph, i: u32) -> String {
    let n = &g.nodes[i as usize];
    match &n.kind {
        NodeKind::Note(_) => format!(
            "n:{}",
            n.path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        NodeKind::Tag(t) => format!("t:{}", t.to_lowercase()),
        NodeKind::Unresolved(t) => format!("u:{}", t.to_lowercase()),
        NodeKind::Attachment(p) => format!("a:{}", p.to_string_lossy()),
    }
}

type Shape = (
    Vec<(String, String, u32)>,
    Vec<(String, String, EdgeKind, bool)>,
);

fn shape(g: &Graph) -> Shape {
    let mut nodes: Vec<_> = (0..g.nodes.len() as u32)
        .map(|i| {
            (
                key(g, i),
                g.nodes[i as usize].label.to_string(),
                g.nodes[i as usize].degree,
            )
        })
        .collect();
    nodes.sort();
    let mut edges: Vec<_> = g
        .edges
        .iter()
        .map(|e| {
            let (a, b) = (key(g, e.a), key(g, e.b));
            let (a, b) = if e.mutual && b < a { (b, a) } else { (a, b) };
            (a, b, e.kind, e.mutual)
        })
        .collect();
    edges.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)));
    (nodes, edges)
}

#[test]
fn update_matches_a_full_build() {
    let (_dir, v) = vault("update");
    for spec in [
        Spec {
            tags: true,
            unresolved: true,
            attachments: true,
            ..Spec::default()
        },
        Spec {
            orphans: false,
            ..Spec::default()
        },
        Spec {
            tags: true,
            hidden_folders: vec!["sub".into()],
            ..Spec::default()
        },
    ] {
        let mut g = build(&v, &spec);
        let step = |changes: Changes, g: &mut Graph| {
            let next = update(&v, &spec, g, &changes).unwrap();
            assert_eq!(
                shape(&next),
                shape(&build(&v, &spec)),
                "{spec:?} {changes:?}"
            );
            *g = next;
        };
        let m = v
            .create_note(Path::new("missing.md"), "# Missing\n[[lonely]] #red/new\n")
            .unwrap();
        step(
            Changes {
                changed: vec![m],
                removed: vec![],
            },
            &mut g,
        );
        let b = v.note_by_path(Path::new("b.md")).unwrap();
        v.write(b, "# B renamed\n[[sub/c]]\n", None).unwrap();
        step(
            Changes {
                changed: vec![b],
                removed: vec![],
            },
            &mut g,
        );
        let c = v.note_by_path(Path::new("sub/c.md")).unwrap();
        v.rename(Path::new("sub/c.md"), Path::new("c.md"), false)
            .unwrap();
        step(
            Changes {
                changed: vec![c],
                removed: vec![],
            },
            &mut g,
        );
        v.trash(Path::new("missing.md")).unwrap();
        step(
            Changes {
                changed: vec![],
                removed: vec![m],
            },
            &mut g,
        );
        v.rename(Path::new("c.md"), Path::new("sub/c.md"), false)
            .unwrap();
        step(
            Changes {
                changed: vec![c],
                removed: vec![],
            },
            &mut g,
        );
        v.write(b, "# B\n[[a]] #red\n", None).unwrap();
        step(
            Changes {
                changed: vec![b],
                removed: vec![],
            },
            &mut g,
        );
    }
    assert!(
        update(
            &v,
            &Spec {
                only: Some(Vec::new()),
                ..Spec::default()
            },
            &Graph::default(),
            &Changes::default()
        )
        .is_none()
    );
}

#[test]
fn relations_from_the_caller_are_edges_of_their_own() {
    let (_dir, v) = vault("relations");
    let spec = Spec {
        relations: vec![
            ("sub/d.md".into(), "lonely.md".into()),
            ("sub/d.md".into(), "absent.md".into()),
            ("a.md".into(), "a.md".into()),
        ],
        ..Spec::default()
    };
    let g = build(&v, &spec);
    let relations: Vec<_> = edge_labels(&g)
        .into_iter()
        .filter(|e| e.2 == EdgeKind::Relation)
        .collect();
    assert_eq!(
        relations,
        [("D".into(), "Lonely".into(), EdgeKind::Relation, false)]
    );
    let no_orphans = build(
        &v,
        &Spec {
            orphans: false,
            ..spec.clone()
        },
    );
    assert!(
        no_orphans.nodes.iter().any(|n| &*n.label == "Lonely"),
        "a relation is a link for the orphan filter"
    );
    let d = v.note_by_path(Path::new("sub/d.md")).unwrap();
    v.rename(Path::new("lonely.md"), Path::new("alone.md"), false)
        .unwrap();
    let moved = Spec {
        relations: vec![("sub/d.md".into(), "alone.md".into())],
        ..Spec::default()
    };
    let lonely = v.note_by_path(Path::new("alone.md")).unwrap();
    let next = update(
        &v,
        &moved,
        &g,
        &Changes {
            changed: vec![lonely, d],
            removed: vec![],
        },
    )
    .unwrap();
    assert_eq!(shape(&next), shape(&build(&v, &moved)));
    let hidden = Spec {
        orphans: false,
        ..moved.clone()
    };
    let without = build(
        &v,
        &Spec {
            orphans: false,
            ..Spec::default()
        },
    );
    let next = update(
        &v,
        &hidden,
        &without,
        &Changes {
            changed: vec![d],
            removed: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        shape(&next),
        shape(&build(&v, &hidden)),
        "an orphan comes back with a relation"
    );
}
