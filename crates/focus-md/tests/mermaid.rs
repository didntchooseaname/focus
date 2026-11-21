use focus_md::mermaid::{Error, Theme, rasterize, render_svg};

const CASES: [(&str, &str); 9] = [
    (
        "flowchart",
        "flowchart LR\n A[Début] --> B{Test ?}\n B -- oui --> C[Fin]\n B -- non --> A",
    ),
    (
        "sequence",
        "sequenceDiagram\n participant A\n participant B\n A->>B: bonjour\n B-->>A: salut",
    ),
    (
        "state",
        "stateDiagram-v2\n [*] --> Un\n Un --> Deux\n Deux --> [*]",
    ),
    (
        "class",
        "classDiagram\n class Hote {\n +String ip\n +scanner()\n }\n Hote --> Service",
    ),
    (
        "er",
        "erDiagram\n A ||--o{ B : contient\n B ||--o{ C : expose",
    ),
    (
        "gantt",
        "gantt\n title T\n dateFormat YYYY-MM-DD\n section A\n Tâche :a1, 2026-01-01, 3d",
    ),
    ("pie", "pie title P\n \"a\" : 1\n \"b\" : 2"),
    ("mindmap", "mindmap\n  root((Racine))\n    Un\n    Deux"),
    ("timeline", "timeline\n title T\n 09h : Début\n 10h : Fin"),
];

#[test]
fn supported_types_render_and_rasterize() {
    for (name, source) in CASES {
        let svg = render_svg(source, &Theme::dark()).unwrap_or_else(|e| panic!("{name} : {e}"));
        assert_eq!(svg.diagram, name);
        assert!(
            svg.width > 10.0 && svg.height > 10.0,
            "{name} : {}x{}",
            svg.width,
            svg.height
        );
        let raster = rasterize(&svg, 1.0, &[]).unwrap_or_else(|e| panic!("{name} : {e}"));
        assert_eq!(
            raster.rgba.len(),
            (raster.width * raster.height * 4) as usize
        );
    }
}

#[test]
fn background_color_is_the_theme_background() {
    let svg = render_svg(CASES[0].1, &Theme::dark()).unwrap();
    let raster = rasterize(&svg, 1.0, &[]).unwrap();
    assert_eq!(&raster.rgba[..4], &[0x07, 0x07, 0x07, 0xff]);
    let svg = render_svg(CASES[0].1, &Theme::light()).unwrap();
    let raster = rasterize(&svg, 1.0, &[]).unwrap();
    assert_eq!(&raster.rgba[..4], &[0xff, 0xff, 0xff, 0xff]);
}

#[test]
fn unsupported_and_invalid() {
    assert!(matches!(
        render_svg("gitGraph\n commit", &Theme::dark()),
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        render_svg("flowchart LR\n A --> \n --> B[", &Theme::dark()),
        Err(Error::Syntax(_))
    ));
    assert!(render_svg("", &Theme::dark()).is_err());
    assert!(render_svg("pas un diagramme", &Theme::dark()).is_err());
}

#[test]
fn rasterize_rejects_bad_scale() {
    let svg = render_svg(CASES[0].1, &Theme::dark()).unwrap();
    assert!(rasterize(&svg, 0.0, &[]).is_err());
    assert!(rasterize(&svg, f32::NAN, &[]).is_err());
}

#[test]
fn gantt_axis_never_repeats_a_date() {
    for days in [1, 2, 3, 5, 12] {
        let source = format!(
            "gantt\n title T\n dateFormat YYYY-MM-DD\n section S\n Task :a1, 2026-01-05, {days}d"
        );
        let svg = render_svg(&source, &Theme::dark()).unwrap().svg;
        for day in 5..=17 {
            let date = format!(">2026-01-{day:02}<");
            assert!(
                svg.matches(&date).count() <= 1,
                "{days}d: {date} appears twice"
            );
        }
    }
}

#[test]
fn click_directives_and_wiki_links_in_labels_become_diagram_links() {
    let source = "flowchart LR\n  A[Start] --> B[\"See [[Other note|the other]]\"]\n  B --> C[Plain]\n  C --> D[Web]\n  click A \"[[Target note#Part]]\"\n  click D href \"https://example.com/x\" \"tip\"\n  click C call callback()\n";
    let svg = render_svg(source, &Theme::dark()).unwrap();
    let targets: Vec<(&str, bool)> = svg
        .links
        .iter()
        .map(|l| (l.target.as_str(), l.external))
        .collect();
    assert_eq!(targets.len(), 3, "{targets:?}");
    assert!(targets.contains(&("Target note#Part", false)));
    assert!(targets.contains(&("Other note", false)));
    assert!(targets.contains(&("https://example.com/x", true)));
    assert!(
        svg.svg.contains("the other") && !svg.svg.contains("[["),
        "labels are shown without brackets"
    );
    assert!(
        !svg.svg.contains("<a "),
        "links are given as rectangles, not baked into the picture"
    );
    let a = svg
        .links
        .iter()
        .find(|l| l.target == "Target note#Part")
        .unwrap();
    let drawn = format!(
        "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\"",
        a.x, a.y, a.width, a.height
    );
    assert!(
        svg.svg.contains(&drawn),
        "the node is drawn where the link says: {drawn}"
    );
    assert!(
        a.width > 10.0 && a.x + a.width <= svg.width + 1.0 && a.y + a.height <= svg.height + 1.0
    );
}

#[test]
fn subroutine_shape_is_not_a_link() {
    let svg = render_svg(
        "flowchart LR\n  A[[Subroutine]] --> B[End]\n",
        &Theme::dark(),
    )
    .unwrap();
    assert!(svg.links.is_empty());
    assert!(svg.svg.contains("Subroutine"));
}

#[test]
fn chains_of_open_links_keep_each_node_label() {
    let svg = render_svg(
        "flowchart LR\n  A[\"Left bar\"] --- B[\"Tabs\"] --- C[\"Right bar\"]\n  B --- D[Bottom]\n",
        &Theme::dark(),
    )
    .unwrap();
    for label in ["Left bar", "Tabs", "Right bar", "Bottom"] {
        assert!(svg.svg.contains(&format!(">{label}<")), "{label} missing");
    }
    assert!(
        !svg.svg.contains("B[") && !svg.svg.contains("&quot;"),
        "a node shows its source as label"
    );
}

#[test]
fn an_unclosed_node_is_a_syntax_error() {
    let source = "flowchart TD\n    A[Start --> B{Broken\n";
    assert!(matches!(
        render_svg(source, &Theme::dark()),
        Err(Error::Syntax(_))
    ));
}
