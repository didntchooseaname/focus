use std::sync::{Arc, LazyLock};

use mermaid_rs_renderer::{
    DiagramKind, LayoutConfig, RenderOptions, Theme as EngineTheme, compute_layout,
    parse_mermaid_strict, render_svg as engine_render_svg,
};
use regex::Regex;
use resvg::{tiny_skia, usvg};

#[derive(Debug, Clone)]
pub struct Theme {
    pub background: String,
    pub node_fill: String,
    pub node_stroke: String,
    pub text: String,
    pub muted_text: String,
    pub edge: String,
    pub font_family: String,
    pub font_size: f32,
}

impl Theme {
    pub fn dark() -> Self {
        Theme {
            background: "#070707".into(),
            node_fill: "#121213".into(),
            node_stroke: "#303033".into(),
            text: "#f5f5f6".into(),
            muted_text: "#b4b4ba".into(),
            edge: "#7d7d85".into(),
            font_family: "Inter".into(),
            font_size: 14.0,
        }
    }

    pub fn light() -> Self {
        Theme {
            background: "#ffffff".into(),
            node_fill: "#ffffff".into(),
            node_stroke: "#d6d6da".into(),
            text: "#0b0b0c".into(),
            muted_text: "#4b4b52".into(),
            edge: "#6b6b73".into(),
            font_family: "Inter".into(),
            font_size: 14.0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unsupported diagram type: {0}")]
    Unsupported(String),
    #[error("invalid Mermaid syntax: {0}")]
    Syntax(String),
    #[error("rendering failed: {0}")]
    Render(String),
}

#[derive(Debug, Clone)]
pub struct Svg {
    pub svg: String,
    pub width: f32,
    pub height: f32,
    pub diagram: String,
    pub links: Vec<DiagramLink>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagramLink {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub target: String,
    pub external: bool,
}

struct Directives {
    source: String,
    clicks: Vec<(String, String)>,
    labels: Vec<(String, String)>,
}

fn unquote(token: &str) -> &str {
    token.trim().trim_matches(['"', '\''])
}

fn click_directive(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    let rest = if lower.starts_with("click ") {
        &trimmed[6..]
    } else if lower.starts_with("link ") {
        &trimmed[5..]
    } else {
        return None;
    };
    let rest = rest.trim();
    let (id, rest) = rest.split_once(char::is_whitespace)?;
    let rest = rest.trim();
    let rest = match rest.get(..5) {
        Some(head) if head.eq_ignore_ascii_case("href ") => rest[5..].trim(),
        _ => rest,
    };
    if rest.to_ascii_lowercase().starts_with("call ")
        || rest.to_ascii_lowercase().starts_with("callback")
    {
        return None;
    }
    let target = match rest.strip_prefix('"') {
        Some(inner) => inner.split('"').next()?,
        None => rest.split_whitespace().next()?,
    };
    Some((id.to_string(), clean_target(unquote(target))))
}

fn clean_target(target: &str) -> String {
    let inner = target
        .trim()
        .strip_prefix("[[")
        .and_then(|t| t.strip_suffix("]]"))
        .unwrap_or(target.trim());
    inner.split('|').next().unwrap_or(inner).trim().to_string()
}

fn extract_directives(source: &str) -> Directives {
    static LABEL_LINK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(^|[^\w])\[\[([^\[\]\n|]+)(?:\|([^\[\]\n]+))?\]\]")
            .expect("wiki label pattern")
    });
    let mut out = String::with_capacity(source.len());
    let (mut clicks, mut labels) = (Vec::new(), Vec::new());
    for line in source.split_inclusive('\n') {
        let ending = &line[line.trim_end_matches(['\n', '\r']).len()..];
        let body = &line[..line.len() - ending.len()];
        if let Some(click) = click_directive(body) {
            clicks.push(click);
        } else if !body.trim_start().starts_with("%%") && body.contains("[[") {
            let rewritten = LABEL_LINK.replace_all(body, |c: &regex::Captures| {
                let target = c[2].trim().to_string();
                let shown = c
                    .get(3)
                    .map_or_else(|| target.clone(), |m| m.as_str().trim().to_string());
                labels.push((shown.clone(), target));
                format!("{}{shown}", &c[1])
            });
            out.push_str(&rewritten);
            out.push_str(ending);
            continue;
        }
        out.push_str(if click_directive(body).is_some() {
            ""
        } else {
            body
        });
        out.push_str(ending);
    }
    Directives {
        source: out,
        clicks,
        labels,
    }
}

fn is_external(target: &str) -> bool {
    target.contains("://") || target.starts_with("mailto:")
}

#[derive(Debug, Clone)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn diagram_name(kind: DiagramKind) -> Option<&'static str> {
    Some(match kind {
        DiagramKind::Flowchart => "flowchart",
        DiagramKind::Sequence => "sequence",
        DiagramKind::Class => "class",
        DiagramKind::State => "state",
        DiagramKind::Er => "er",
        DiagramKind::Gantt => "gantt",
        DiagramKind::Pie => "pie",
        DiagramKind::Mindmap => "mindmap",
        DiagramKind::Timeline => "timeline",
        _ => return None,
    })
}

pub fn render_svg(source: &str, theme: &Theme) -> Result<Svg, Error> {
    let directives = extract_directives(source);
    let parsed =
        parse_mermaid_strict(&directives.source).map_err(|e| Error::Syntax(e.to_string()))?;
    let kind = parsed.graph.kind;
    let Some(diagram) = diagram_name(kind) else {
        return Err(Error::Unsupported(format!("{kind:?}").to_lowercase()));
    };
    let drawn_from_nodes = matches!(
        kind,
        DiagramKind::Flowchart
            | DiagramKind::Class
            | DiagramKind::State
            | DiagramKind::Er
            | DiagramKind::Mindmap
    );
    if drawn_from_nodes && parsed.graph.nodes.is_empty() {
        return Err(Error::Syntax("no node is declared".into()));
    }
    let options = RenderOptions {
        theme: engine_theme(theme),
        layout: engine_layout(theme, kind),
    };
    let layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
    let mut svg = engine_render_svg(&layout, &options.theme, &options.layout);
    if kind == DiagramKind::Timeline {
        svg = repaint_timeline(&svg, theme);
    }
    let (width, height) =
        svg_size(&svg).ok_or_else(|| Error::Render("SVG without dimensions".into()))?;
    let mut links: Vec<DiagramLink> = Vec::new();
    let mut add = |node: &mermaid_rs_renderer::layout::NodeLayout, target: &str| {
        if node.hidden || links.iter().any(|l| l.x == node.x && l.y == node.y) {
            return;
        }
        links.push(DiagramLink {
            x: node.x,
            y: node.y,
            width: node.width,
            height: node.height,
            target: target.to_string(),
            external: is_external(target),
        });
    };
    for (id, target) in &directives.clicks {
        if let Some(node) = layout.nodes.get(id) {
            add(node, target);
        }
    }
    for (shown, target) in &directives.labels {
        for node in layout
            .nodes
            .values()
            .filter(|n| n.label.lines.join(" ").contains(shown.as_str()))
        {
            add(node, target);
        }
    }
    links.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    Ok(Svg {
        svg,
        width,
        height,
        diagram: diagram.to_string(),
        links,
    })
}

fn svg_size(svg: &str) -> Option<(f32, f32)> {
    let root = &svg[svg.find("<svg")?..];
    let root = &root[..root.find('>')?];
    let attribute = |name: &str| -> Option<f32> {
        let key = format!(" {name}=\"");
        let start = root.find(&key)? + key.len();
        let value = &root[start..start + root[start..].find('"')?];
        value.trim_end_matches("px").parse().ok()
    };
    if let (Some(w), Some(h)) = (attribute("width"), attribute("height")) {
        return Some((w, h));
    }
    let view_box = {
        let start = root.find(" viewBox=\"")? + 10;
        &root[start..start + root[start..].find('"')?]
    };
    let numbers: Vec<f32> = view_box
        .split([' ', ','])
        .filter_map(|n| n.parse().ok())
        .collect();
    (numbers.len() == 4).then(|| (numbers[2], numbers[3]))
}

fn engine_layout(theme: &Theme, kind: DiagramKind) -> LayoutConfig {
    let mut layout = LayoutConfig::default();
    match kind {
        DiagramKind::Mindmap => {
            let mindmap = &mut layout.mindmap;
            mindmap.section_colors = vec![theme.node_fill.clone()];
            mindmap.section_label_colors = vec![theme.text.clone()];
            mindmap.section_line_colors = vec![theme.edge.clone()];
            mindmap.root_fill = Some(theme.node_fill.clone());
            mindmap.root_text = Some(theme.text.clone());
            mindmap.node_stroke = Some(theme.node_stroke.clone());
            mindmap.root_stroke = Some(theme.text.clone());
            mindmap.stroke_width = 1.5;
            mindmap.edge_color = Some(theme.edge.clone());
            mindmap.edge_depth_base_width = 1.5;
            mindmap.edge_depth_step = 0.0;
            mindmap.divider_line_width = 0.0;
        }
        DiagramKind::Er => {
            layout.node_spacing = 80.0;
            layout.rank_spacing = 120.0;
        }
        DiagramKind::Pie => {
            layout.pie.margin = 44.0;
            layout.pie.legend_spacing = 6.0;
        }
        _ => {}
    }
    layout
}

fn repaint_timeline(svg: &str, theme: &Theme) -> String {
    let mut out = svg.to_string();
    for pastel in [
        "#ECECFF", "#FFE6CC", "#D5E8D4", "#F8CECC", "#FFF2CC", "#E1D5E7",
    ] {
        out = out.replace(
            &format!("fill=\"{pastel}\""),
            &format!("fill=\"{}\"", theme.node_fill),
        );
    }
    out
}

fn engine_theme(theme: &Theme) -> EngineTheme {
    let mut t = EngineTheme::modern();
    let shade = |t: f32| mix(&theme.node_fill, &theme.text, t);
    t.font_family = theme.font_family.clone();
    t.font_size = theme.font_size;
    t.background = theme.background.clone();
    t.primary_color = theme.node_fill.clone();
    t.primary_text_color = theme.text.clone();
    t.primary_border_color = theme.node_stroke.clone();
    t.line_color = theme.edge.clone();
    t.text_color = theme.text.clone();
    t.secondary_color = shade(0.08);
    t.tertiary_color = shade(0.16);
    t.edge_label_background = theme.background.clone();
    t.cluster_background = theme.background.clone();
    t.cluster_border = theme.node_stroke.clone();
    t.sequence_actor_fill = theme.node_fill.clone();
    t.sequence_actor_border = theme.node_stroke.clone();
    t.sequence_actor_line = theme.edge.clone();
    t.sequence_note_fill = shade(0.08);
    t.sequence_note_border = theme.node_stroke.clone();
    t.sequence_activation_fill = shade(0.16);
    t.sequence_activation_border = theme.node_stroke.clone();
    const PIE_TONES: [f32; 8] = [0.92, 0.74, 0.58, 0.44, 0.84, 0.66, 0.51, 0.38];
    for (i, color) in t.pie_colors.iter_mut().enumerate() {
        *color = mix(
            &theme.background,
            &theme.text,
            PIE_TONES[i % PIE_TONES.len()],
        );
    }
    t.pie_title_text_size = theme.font_size * 1.15;
    t.pie_section_text_size = theme.font_size;
    t.pie_legend_text_size = theme.font_size;
    t.pie_opacity = 1.0;
    t.pie_title_text_color = theme.text.clone();
    t.pie_section_text_color = theme.background.clone();
    t.pie_legend_text_color = theme.text.clone();
    t.pie_stroke_color = theme.background.clone();
    t.pie_outer_stroke_color = theme.node_stroke.clone();
    t
}

fn parse_hex(color: &str) -> Option<[f32; 3]> {
    let hex = color.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok().map(f32::from);
    Some([channel(0)?, channel(2)?, channel(4)?])
}

fn mix(from: &str, to: &str, amount: f32) -> String {
    let (Some(a), Some(b)) = (parse_hex(from), parse_hex(to)) else {
        return from.to_string();
    };
    let amount = amount.clamp(0.0, 1.0);
    let c = |i: usize| (a[i] + (b[i] - a[i]) * amount).round() as u8;
    format!("#{:02x}{:02x}{:02x}", c(0), c(1), c(2))
}

pub fn init_fonts(fonts: &[&[u8]]) {
    mermaid_rs_renderer::init_fonts(fonts);
}

pub fn rasterize(svg: &Svg, scale: f32, fonts: &[&[u8]]) -> Result<Raster, Error> {
    if !(scale.is_finite() && scale > 0.0) {
        return Err(Error::Render(format!("invalid scale: {scale}")));
    }
    let mut db = mermaid_rs_renderer::font_database();
    if !fonts.is_empty() {
        let extended = Arc::make_mut(&mut db);
        for font in fonts {
            extended.load_font_data(font.to_vec());
        }
    }
    let options = usvg::Options {
        fontdb: db,
        ..usvg::Options::default()
    };
    let tree =
        usvg::Tree::from_str(&svg.svg, &options).map_err(|e| Error::Render(e.to_string()))?;
    let width = (tree.size().width() * scale).ceil() as u32;
    let height = (tree.size().height() * scale).ceil() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| Error::Render(format!("invalid image size: {width}x{height}")))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut rgba = Vec::with_capacity(pixmap.pixels().len() * 4);
    for pixel in pixmap.pixels() {
        let c = pixel.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok(Raster {
        width,
        height,
        rgba,
    })
}
