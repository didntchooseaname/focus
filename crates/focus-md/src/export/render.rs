use std::collections::HashMap;
use std::fmt::Write;

use base64::Engine;

use super::{CodeColors, HtmlOptions, Labels, Resolver, style};
use crate::highlight::{self, Hints, Token};
use crate::html::{self, Token as HtmlToken};
use crate::url::{is_safe_url, local_url};
use crate::{
    Align, Block, BlockKind, Directive, Document, Image, Inline, InlineKind, Link, LinkKind,
    ParseOptions, callout_kind, mermaid, parse, plain_text,
};

const MAX_EMBED_DEPTH: usize = 3;
const MIN_DIAGRAM_SCALE: f32 = 0.75;

struct Config {
    embed_images: bool,
    render_mermaid: bool,
    dark: bool,
    colors: Option<CodeColors>,
    fragment: bool,
    labels: Labels,
    page_rtl: bool,
}

struct Renderer<'a> {
    cfg: Config,
    resolver: &'a dyn Resolver,
    hints: Vec<String>,
    footnotes: Vec<String>,
    heading_ids: HashMap<String, usize>,
    embed_stack: Vec<String>,
    tab_groups: usize,
    most_tabs: usize,
}

pub(super) fn page(source: &str, options: &HtmlOptions, resolver: &dyn Resolver) -> String {
    let doc = parse(source, &ParseOptions::default());
    let cfg = Config {
        embed_images: options.embed_images,
        render_mermaid: options.render_mermaid,
        dark: options.dark,
        colors: Some(options.code_colors),
        fragment: false,
        labels: options.labels.clone(),
        page_rtl: options.lang.to_lowercase().starts_with("ar"),
    };
    let mut r = Renderer::new(cfg, resolver, &doc);
    let mut body = String::new();
    if let Some(front) = &doc.frontmatter {
        r.properties(&front.raw, &mut body);
    }
    r.blocks(&doc.blocks, &mut body);
    r.footnote_section(&doc, &mut body);
    let dir = if options.lang.to_lowercase().starts_with("ar") {
        "rtl"
    } else {
        "ltr"
    };
    let title = esc(&options.title);
    let copy_script = body
        .contains("class=\"copy\"")
        .then(|| copy_script(&options.labels.copied));
    let mut out = String::with_capacity(body.len() + 8192);
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"{}\" dir=\"{dir}\" data-theme=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"{}\">\n<meta name=\"referrer\" content=\"no-referrer\">\n<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n<title>{title}</title>\n<style>\n{}</style>\n</head>\n<body>\n<main class=\"sheet\">\n",
        esc(&options.lang),
        if options.dark { "dark" } else { "light" },
        content_security_policy(copy_script.as_deref()),
        {
            let mut css = style::stylesheet(options.dark, options.code_colors, options.embed_fonts);
            if r.most_tabs > 0 {
                css.push_str(&style::tabs_css(r.most_tabs));
            }
            css
        }
    );
    let repeats_title = matches!(doc.blocks.first().map(|b| &b.kind), Some(BlockKind::Heading { level: 1, id, .. }) if id.trim().eq_ignore_ascii_case(options.title.trim()));
    if !options.title.is_empty() && !repeats_title {
        let _ = writeln!(out, "<h1 class=\"note-title\" dir=\"auto\">{title}</h1>");
    }
    out.push_str(&body);
    out.push_str("</main>\n");
    if let Some(script) = &copy_script {
        let _ = writeln!(out, "<script>{script}</script>");
    }
    out.push_str("</body>\n</html>\n");
    out
}

pub(super) fn fragment(source: &str, resolver: &dyn Resolver) -> String {
    let doc = parse(source, &ParseOptions::default());
    let cfg = Config {
        embed_images: false,
        render_mermaid: false,
        dark: false,
        colors: None,
        fragment: true,
        labels: Labels::default(),
        page_rtl: false,
    };
    let mut r = Renderer::new(cfg, resolver, &doc);
    let mut out = String::new();
    r.blocks(&doc.blocks, &mut out);
    r.footnote_section(&doc, &mut out);
    out
}

fn is_rtl_char(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

fn strong_direction(text: &str) -> Option<&'static str> {
    text.chars()
        .find(|c| c.is_alphabetic())
        .map(|c| if is_rtl_char(c) { "rtl" } else { "ltr" })
}

fn dir_attr(text: &str, page_rtl: bool) -> String {
    match strong_direction(text) {
        Some("rtl") => " dir=\"rtl\"".to_string(),
        Some(_) if page_rtl => " dir=\"ltr\"".to_string(),
        _ => String::new(),
    }
}

fn leading_text(blocks: &[Block]) -> String {
    for block in blocks {
        let text = match &block.kind {
            BlockKind::Paragraph(i) | BlockKind::Heading { inlines: i, .. } => plain_text(i),
            BlockKind::List { items, .. } => items
                .first()
                .map(|i| leading_text(&i.blocks))
                .unwrap_or_default(),
            BlockKind::Quote(b)
            | BlockKind::Callout { blocks: b, .. }
            | BlockKind::Details { blocks: b, .. } => leading_text(b),
            BlockKind::Table { head, rows, .. } => head
                .iter()
                .chain(rows.iter().flatten())
                .map(|c| plain_text(c))
                .collect::<Vec<_>>()
                .join(" "),
            _ => String::new(),
        };
        if strong_direction(&text).is_some() {
            return text;
        }
    }
    String::new()
}

fn copy_script(copied: &str) -> String {
    format!(
        "document.addEventListener('click',function(e){{var b=e.target.closest('button.copy');if(!b)return;var t=b.closest('figure').querySelector('pre').innerText;if(navigator.clipboard){{navigator.clipboard.writeText(t).then(function(){{var o=b.textContent;b.textContent={};setTimeout(function(){{b.textContent=o}},1200)}})}}}});",
        js_string(copied)
    )
}

fn content_security_policy(script: Option<&str>) -> String {
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    let script_src = match script {
        Some(text) => format!(
            "'sha256-{}'",
            base64::engine::general_purpose::STANDARD.encode(Sha256::digest(text.as_bytes()))
        ),
        None => "'none'".to_string(),
    };
    format!(
        "default-src 'none'; script-src {script_src}; style-src 'unsafe-inline'; img-src data: http: https: file:; font-src data:; base-uri 'none'; form-action 'none'"
    )
}

fn js_string(text: &str) -> String {
    format!(
        "\"{}\"",
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('<', "\\u003c")
            .replace('\n', " ")
    )
}

pub(super) fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

pub(super) fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in text.trim().chars() {
        if c.is_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.extend(c.to_lowercase());
        } else if c.is_whitespace() || c == '-' || c == '_' {
            pending_dash = true;
        }
    }
    out
}

fn is_url(target: &str) -> bool {
    target.contains("://") || target.starts_with("mailto:") || target.starts_with("tel:")
}

fn mime_of(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn percent_encode_path(path: &str) -> String {
    let mut out = String::new();
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~/:#?&=%".contains(&b) {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(v) = text
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn token_class(token: Token) -> &'static str {
    match token {
        Token::Keyword => "t-kw",
        Token::Function => "t-fn",
        Token::Type => "t-ty",
        Token::String => "t-str",
        Token::Number => "t-num",
        Token::Constant => "t-const",
        Token::Variable => "t-var",
        Token::Property => "t-prop",
        Token::Flag => "t-flag",
        Token::Operator => "t-op",
        Token::Punctuation => "t-punct",
        Token::Comment => "t-cmt",
        Token::Prompt => "t-prompt",
        Token::Tag => "t-tag",
        Token::Attribute => "t-attr",
        Token::Inserted => "t-ins",
        Token::Deleted => "t-del",
        Token::Heading => "t-head",
    }
}

fn icon(family: &str) -> &'static str {
    match family {
        "abstract" => {
            "<rect x=\"8\" y=\"2\" width=\"8\" height=\"4\" rx=\"1\"/><path d=\"M16 4h2a2 2 0 012 2v14a2 2 0 01-2 2H6a2 2 0 01-2-2V6a2 2 0 012-2h2\"/>"
        }
        "info" => "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"M12 16v-4M12 8h.01\"/>",
        "todo" => "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"M8 12l3 3 5-6\"/>",
        "tip" => {
            "<path d=\"M8.5 14.5A2.5 2.5 0 0011 12c0-1.4-.5-2-1-3-1.1-2.1-.2-4 2-6 .5 2.5 2 4.9 4 6.5 2 1.6 3 3.5 3 5.5a7 7 0 11-14 0c0-1.2.4-2.4 1-3a2.5 2.5 0 002.5 2.5z\"/>"
        }
        "success" => "<path d=\"M20 6L9 17l-5-5\"/>",
        "question" => {
            "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"M9.1 9a3 3 0 015.8 1c0 2-3 3-3 3M12 17h.01\"/>"
        }
        "warning" => {
            "<path d=\"M10.3 3.9L1.8 18a2 2 0 001.7 3h17a2 2 0 001.7-3L13.7 3.9a2 2 0 00-3.4 0zM12 9v4M12 17h.01\"/>"
        }
        "failure" => "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"M15 9l-6 6M9 9l6 6\"/>",
        "danger" => "<path d=\"M13 2L3 14h9l-1 8 10-12h-9z\"/>",
        "bug" => {
            "<path d=\"M8 2l1.9 1.9M16 2l-1.9 1.9M9 7.1V6a3 3 0 016 0v1.1M12 20a6 6 0 01-6-6v-3a4 4 0 014-4h4a4 4 0 014 4v3a6 6 0 01-6 6zM12 20v-9M6.5 13H2M17.5 13H22M3 21c0-2 1.7-3.9 3.8-4M21 21c0-2-1.7-3.9-3.8-4\"/>"
        }
        "example" => "<path d=\"M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01\"/>",
        "quote" => {
            "<path d=\"M3 21c3 0 7-1 7-8V5c0-1.3-1-2-2-2H4c-1 0-2 1-2 2v6c0 1 1 2 2 2h4M15 21c3 0 7-1 7-8V5c0-1.3-1-2-2-2h-4c-1 0-2 1-2 2v6c0 1 1 2 2 2h4\"/>"
        }
        _ => "<path d=\"M12 20h9\"/><path d=\"M16.5 3.5a2.1 2.1 0 013 3L7 19l-4 1 1-4z\"/>",
    }
}

const CHEVRON: &str = "<svg class=\"chev\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M9 6l6 6-6 6\"/></svg>";

impl<'a> Renderer<'a> {
    fn new(cfg: Config, resolver: &'a dyn Resolver, doc: &Document) -> Self {
        let mut hints = Vec::new();
        collect_languages(&doc.blocks, &mut hints);
        Renderer {
            cfg,
            resolver,
            hints,
            footnotes: Vec::new(),
            heading_ids: HashMap::new(),
            embed_stack: Vec::new(),
            tab_groups: 0,
            most_tabs: 0,
        }
    }

    fn blocks(&mut self, blocks: &[Block], out: &mut String) {
        for block in blocks {
            self.block(block, out);
        }
    }

    fn block(&mut self, block: &Block, out: &mut String) {
        match &block.kind {
            BlockKind::Heading {
                level, id, inlines, ..
            } => {
                let mut slug_id = slug(id);
                let count = self.heading_ids.entry(slug_id.clone()).or_insert(0);
                if *count > 0 {
                    slug_id = format!("{slug_id}-{count}");
                }
                *count += 1;
                let level = (*level).clamp(1, 6);
                let _ = write!(out, "<h{level} id=\"{}\" dir=\"auto\">", esc(&slug_id));
                self.inlines(inlines, out);
                let _ = writeln!(out, "</h{level}>");
            }
            BlockKind::Paragraph(inlines) => {
                out.push_str("<p dir=\"auto\">");
                self.inlines(inlines, out);
                out.push_str("</p>\n");
            }
            BlockKind::List {
                ordered,
                start,
                tight,
                items,
            } => {
                let tag = if *ordered { "ol" } else { "ul" };
                let dir = dir_attr(
                    &leading_text(std::slice::from_ref(block)),
                    self.cfg.page_rtl,
                );
                if *ordered && *start != 1 {
                    let _ = writeln!(out, "<ol start=\"{start}\"{dir}>");
                } else {
                    let _ = writeln!(out, "<{tag}{dir}>");
                }
                for item in items {
                    self.list_item(item, *tight, out);
                }
                let _ = writeln!(out, "</{tag}>");
            }
            BlockKind::CodeBlock { lang, code, .. } => self.code_block(lang, code, out),
            BlockKind::Mermaid { source } => self.mermaid_block(source, out),
            BlockKind::Math { tex } => {
                let _ = writeln!(
                    out,
                    "<div class=\"math-block\"><pre>{}</pre></div>",
                    esc(tex)
                );
            }
            BlockKind::Quote(blocks) => {
                let _ = writeln!(
                    out,
                    "<blockquote{}>",
                    dir_attr(&leading_text(blocks), self.cfg.page_rtl)
                );
                self.blocks(blocks, out);
                out.push_str("</blockquote>\n");
            }
            BlockKind::Callout {
                kind,
                title,
                fold,
                blocks,
                ..
            } => self.callout(kind, title, *fold, blocks, out),
            BlockKind::Table {
                align, head, rows, ..
            } => {
                let dir = dir_attr(
                    &leading_text(std::slice::from_ref(block)),
                    self.cfg.page_rtl,
                );
                self.table(align, head, rows, &dir, out);
            }
            BlockKind::Embed { target, anchor } => self.embed(target, anchor.as_deref(), out),
            BlockKind::Image(image) => self.image_block(image, out),
            BlockKind::Rule => out.push_str("<hr>\n"),
            BlockKind::Html(raw) => self.raw_html(raw, out),
            BlockKind::Details {
                summary,
                open,
                blocks,
            } => {
                let _ = write!(
                    out,
                    "<details class=\"fold\"{}><summary dir=\"auto\">{CHEVRON}<span class=\"label\">",
                    if *open { " open" } else { "" }
                );
                self.inlines(summary, out);
                out.push_str("</span></summary>\n<div class=\"content\">\n");
                self.blocks(blocks, out);
                out.push_str("</div></details>\n");
            }
            BlockKind::Tabs { tabs, .. } => self.tabs(tabs, out),
            BlockKind::Directive(directive) => self.directive(directive, out),
        }
    }

    fn tabs(&mut self, tabs: &[crate::Tab], out: &mut String) {
        if self.cfg.fragment {
            for tab in tabs {
                let _ = writeln!(out, "<h4 dir=\"auto\">{}</h4>", esc(&tab.title));
                self.blocks(&tab.blocks, out);
            }
            return;
        }
        self.tab_groups += 1;
        self.most_tabs = self.most_tabs.max(tabs.len());
        let group = self.tab_groups;
        out.push_str("<div class=\"tabs\">\n");
        for (index, tab) in tabs.iter().enumerate() {
            let _ = writeln!(
                out,
                "<input type=\"radio\" name=\"tabs-{group}\" id=\"tabs-{group}-{index}\" aria-label=\"{}\"{}>",
                esc(&tab.title),
                if index == 0 { " checked" } else { "" }
            );
        }
        out.push_str("<div class=\"tab-bar\">");
        for (index, tab) in tabs.iter().enumerate() {
            let _ = write!(
                out,
                "<label for=\"tabs-{group}-{index}\" dir=\"auto\">{}</label>",
                esc(&tab.title)
            );
        }
        out.push_str("</div>\n<div class=\"tab-panels\">\n");
        for tab in tabs {
            let _ = writeln!(
                out,
                "<section class=\"tab-panel\"><h4 class=\"tab-title\" dir=\"auto\">{}</h4>",
                esc(&tab.title)
            );
            self.blocks(&tab.blocks, out);
            out.push_str("</section>\n");
        }
        out.push_str("</div>\n</div>\n");
    }

    fn directive(&mut self, directive: &Directive, out: &mut String) {
        match directive {
            Directive::Ref { path } => {
                let name = path.split('#').next().unwrap_or(path);
                let target = name.strip_suffix(".md").unwrap_or(name);
                let shown = target.rsplit('/').next().unwrap_or(target);
                match self
                    .resolver
                    .link(target, path.split_once('#').map(|(_, a)| a))
                {
                    Some(url) => {
                        let _ = writeln!(
                            out,
                            "<p class=\"directive\"><a href=\"{}\">{}</a></p>",
                            esc(&local_url(url)),
                            esc(shown)
                        );
                    }
                    None => {
                        let _ = writeln!(
                            out,
                            "<p class=\"directive\"><span class=\"unresolved\">{}</span></p>",
                            esc(shown)
                        );
                    }
                }
            }
            Directive::Include { path } => {
                let content = if self.embed_stack.is_empty() {
                    self.resolver.include(path)
                } else {
                    None
                };
                match content {
                    Some(content) => {
                        let doc = parse(&content, &ParseOptions::default());
                        out.push_str("<div class=\"include\">\n");
                        self.embed_stack.push(format!("include:{path}"));
                        self.blocks(&doc.blocks, out);
                        self.embed_stack.pop();
                        out.push_str("</div>\n");
                    }
                    None => {
                        let _ = writeln!(
                            out,
                            "<p class=\"directive muted\">{}</p>",
                            esc(&format!("include {path}"))
                        );
                    }
                }
            }
            Directive::Other { name, text } => {
                let _ = writeln!(
                    out,
                    "<p class=\"directive muted\">{}</p>",
                    esc(format!("{name} {text}").trim())
                );
            }
        }
    }

    fn list_item(&mut self, item: &crate::ListItem, tight: bool, out: &mut String) {
        let mut body = String::new();
        for block in &item.blocks {
            match (&block.kind, tight) {
                (BlockKind::Paragraph(inlines), true) => {
                    self.inlines(inlines, &mut body);
                    body.push('\n');
                }
                _ => self.block(block, &mut body),
            }
        }
        match item.task {
            Some(state) => {
                let shown = if state == 'x' || state == 'X' || state == ' ' {
                    String::new()
                } else {
                    esc(&state.to_string())
                };
                let state_attr = if state == 'X' { 'x' } else { state };
                let _ = writeln!(
                    out,
                    "<li class=\"task\" data-state=\"{}\"><span class=\"box\" aria-hidden=\"true\">{}</span><div class=\"body\" dir=\"auto\">{}</div></li>",
                    esc(&state_attr.to_string()),
                    if state == 'x' || state == 'X' {
                        "<svg viewBox=\"0 0 24 24\" width=\"10\" height=\"10\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"3.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M20 6L9 17l-5-5\"/></svg>".to_string()
                    } else {
                        shown
                    },
                    body.trim_end()
                );
            }
            None => {
                let _ = writeln!(out, "<li dir=\"auto\">{}</li>", body.trim_end());
            }
        }
    }

    fn table(
        &mut self,
        align: &[Align],
        head: &[Vec<Inline>],
        rows: &[Vec<Vec<Inline>>],
        dir: &str,
        out: &mut String,
    ) {
        let style = |i: usize| match align.get(i) {
            Some(Align::Left) => " style=\"text-align:left\"",
            Some(Align::Center) => " style=\"text-align:center\"",
            Some(Align::Right) => " style=\"text-align:right\"",
            _ => "",
        };
        let _ = write!(
            out,
            "<div class=\"table-wrap\"{dir}><table{dir}>\n<thead><tr>"
        );
        for (i, cell) in head.iter().enumerate() {
            let _ = write!(out, "<th dir=\"auto\"{}>", style(i));
            self.inlines(cell, out);
            out.push_str("</th>");
        }
        out.push_str("</tr></thead>\n<tbody>\n");
        for row in rows {
            out.push_str("<tr>");
            for (i, cell) in row.iter().enumerate() {
                let _ = write!(out, "<td dir=\"auto\"{}>", style(i));
                self.inlines(cell, out);
                out.push_str("</td>");
            }
            out.push_str("</tr>\n");
        }
        out.push_str("</tbody></table></div>\n");
    }

    fn callout(
        &mut self,
        kind: &str,
        title: &[Inline],
        fold: Option<bool>,
        blocks: &[Block],
        out: &mut String,
    ) {
        let (family, _) = callout_kind(kind);
        let mut label = String::new();
        if title.is_empty() {
            label.push_str(&esc(&self.cfg.labels.callout_title(kind)));
        } else {
            self.inlines(title, &mut label);
        }
        if self.cfg.fragment {
            let _ = write!(out, "<blockquote>\n<p><strong>{label}</strong></p>\n");
            self.blocks(blocks, out);
            out.push_str("</blockquote>\n");
            return;
        }
        let svg = format!(
            "<svg class=\"icon\" viewBox=\"0 0 24 24\" aria-hidden=\"true\">{}</svg>",
            icon(family)
        );
        let dir = if title.is_empty() {
            dir_attr(&leading_text(blocks), self.cfg.page_rtl)
        } else {
            dir_attr(&plain_text(title), self.cfg.page_rtl)
        };
        let attrs = format!(
            "class=\"callout\" data-family=\"{family}\" data-kind=\"{}\"{dir}",
            esc(&kind.to_lowercase())
        );
        match fold {
            Some(open) => {
                let _ = write!(
                    out,
                    "<details {attrs}{}><summary dir=\"auto\">{svg}<span class=\"label\">{label}</span>{CHEVRON}</summary>\n<div class=\"content\">\n",
                    if open { " open" } else { "" }
                );
                self.blocks(blocks, out);
                out.push_str("</div></details>\n");
            }
            None => {
                let _ = write!(
                    out,
                    "<div {attrs}><div class=\"title\" dir=\"auto\">{svg}<span class=\"label\">{label}</span></div>\n<div class=\"content\">\n"
                );
                self.blocks(blocks, out);
                out.push_str("</div></div>\n");
            }
        }
    }

    fn code_block(&mut self, lang: &str, code: &str, out: &mut String) {
        let declared = highlight::canonical(lang);
        let hint_refs: Vec<&str> = self.hints.iter().map(String::as_str).collect();
        let (shown, id, detected) = match declared {
            Some(id) => (highlight::label(id).map(str::to_string), Some(id), false),
            None if !lang.is_empty() => (Some(lang.to_string()), None, false),
            None => match highlight::detect_in_note(
                code,
                &Hints {
                    languages: &hint_refs,
                    path: None,
                },
            ) {
                Some(found) => (
                    highlight::label(found.lang).map(str::to_string),
                    Some(found.lang),
                    true,
                ),
                None => (None, None, false),
            },
        };
        let mut body = String::with_capacity(code.len() + code.len() / 2);
        let spans = match (self.cfg.colors, id) {
            (Some(_), Some(id)) => highlight::highlight(code, id),
            _ => Vec::new(),
        };
        let mut at = 0;
        for span in &spans {
            body.push_str(&esc(&code[at..span.range.start]));
            let _ = write!(
                body,
                "<span class=\"{}\">{}</span>",
                token_class(span.token),
                esc(&code[span.range.clone()])
            );
            at = span.range.end;
        }
        body.push_str(&esc(&code[at..]));
        if self.cfg.fragment {
            let class = id
                .map(|l| format!(" class=\"language-{l}\""))
                .unwrap_or_default();
            let _ = writeln!(out, "<pre><code{class}>{}</code></pre>", esc(code));
            return;
        }
        let label = shown
            .map(|l| {
                format!(
                    "<span>{}{}</span>",
                    esc(&l),
                    if detected {
                        format!(
                            "<span class=\"detected\">{}</span>",
                            esc(&self.cfg.labels.detected)
                        )
                    } else {
                        String::new()
                    }
                )
            })
            .unwrap_or_else(|| "<span></span>".to_string());
        let _ = writeln!(
            out,
            "<figure class=\"code\" dir=\"ltr\"><header>{label}<button type=\"button\" class=\"copy\">{}</button></header><pre><code>{body}</code></pre></figure>",
            esc(&self.cfg.labels.copy)
        );
    }

    fn mermaid_block(&mut self, source: &str, out: &mut String) {
        if self.cfg.render_mermaid && !self.cfg.fragment {
            let theme = if self.cfg.dark {
                mermaid::Theme::dark()
            } else {
                mermaid::Theme::light()
            };
            match mermaid::render_svg(source, &theme) {
                Ok(svg) => {
                    let root_end = svg.svg.find('>').unwrap_or(0);
                    let sized = format!(" width=\"{:.0}\" height=\"{:.0}\"", svg.width, svg.height);
                    let fitted = format!(
                        "<svg style=\"width:{w:.0}px;max-width:100%;min-width:{min:.0}px;height:auto\"",
                        w = svg.width,
                        min = svg.width * MIN_DIAGRAM_SCALE
                    );
                    let root = svg.svg[..root_end]
                        .replacen(" width=\"100%\"", &sized, 1)
                        .replacen("<svg", &fitted, 1);
                    let mut body = format!("{root}{}", &svg.svg[root_end..]);
                    if let Some(end) = body.rfind("</svg>") {
                        body.insert_str(end, &self.diagram_anchors(&svg.links));
                    }
                    let _ = writeln!(
                        out,
                        "<figure class=\"mermaid\"><header><span>{}</span><code>{}</code></header><div class=\"diagram\">{}</div></figure>",
                        esc(&self.cfg.labels.diagram),
                        esc(&svg.diagram),
                        body
                    );
                    return;
                }
                Err(error) => {
                    let mut code = String::new();
                    self.code_block("", source, &mut code);
                    let _ = writeln!(
                        out,
                        "{code}<p class=\"mermaid-error\">{}</p>",
                        esc(&error.to_string())
                    );
                    return;
                }
            }
        }
        self.code_block("", source, out);
    }

    fn diagram_anchors(&self, links: &[mermaid::DiagramLink]) -> String {
        let mut out = String::new();
        for link in links {
            let href = if link.external {
                is_safe_url(&link.target, false).then(|| link.target.clone())
            } else {
                let (name, anchor) = match link.target.split_once('#') {
                    Some((name, anchor)) => (name, Some(anchor)),
                    None => (link.target.as_str(), None),
                };
                let name = name.strip_suffix(".md").unwrap_or(name);
                self.resolver.link(name, anchor).map(local_url)
            };
            if let Some(href) = href {
                let _ = write!(
                    out,
                    "<a href=\"{}\"><rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"transparent\"/></a>",
                    esc(&href),
                    link.x,
                    link.y,
                    link.width,
                    link.height
                );
            }
        }
        out
    }

    fn image_src(&self, src: &str) -> String {
        if is_url(src) || src.starts_with("data:") {
            return if is_safe_url(src, true) {
                esc(src)
            } else {
                String::new()
            };
        }
        if self.cfg.embed_images {
            let decoded = percent_decode(src);
            if let Some(path) = self
                .resolver
                .image(&decoded)
                .or_else(|| self.resolver.image(src))
                && let Ok(bytes) = std::fs::read(&path)
            {
                return format!(
                    "data:{};base64,{}",
                    mime_of(&path),
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                );
            }
        }
        esc(&local_url(percent_encode_path(&percent_decode(src))))
    }

    fn image_tag(&self, image: &Image, class: &str) -> String {
        let mut tag = format!(
            "<img{} src=\"{}\" alt=\"{}\"",
            if class.is_empty() {
                String::new()
            } else {
                format!(" class=\"{class}\"")
            },
            self.image_src(&image.src),
            esc(&image.alt)
        );
        if let Some(w) = image.width {
            let _ = write!(tag, " width=\"{w}\"");
        }
        if let Some(h) = image.height {
            let _ = write!(tag, " height=\"{h}\"");
        }
        tag.push_str(" loading=\"lazy\">");
        tag
    }

    fn image_block(&mut self, image: &Image, out: &mut String) {
        let caption = image
            .caption
            .as_deref()
            .map(|c| format!("<figcaption dir=\"auto\">{}</figcaption>", esc(c)))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "<figure class=\"image\">{}{caption}</figure>",
            self.image_tag(image, "")
        );
    }

    fn embed(&mut self, target: &str, anchor: Option<&str>, out: &mut String) {
        let url = self.resolver.link(target, anchor);
        let title = match &url {
            Some(url) => format!(
                "<a href=\"{}\">{}</a>",
                esc(&local_url(url.clone())),
                esc(target)
            ),
            None => esc(target),
        };
        let content = if self.embed_stack.len() < MAX_EMBED_DEPTH
            && !self.embed_stack.iter().any(|t| t == target)
        {
            self.resolver.embed(target)
        } else {
            None
        };
        let Some(content) = content else {
            let _ = writeln!(
                out,
                "<div class=\"embed missing\"><div class=\"embed-title\">{title}</div></div>"
            );
            return;
        };
        let doc = parse(&content, &ParseOptions::default());
        let blocks = match anchor {
            Some(a) => select_anchor(&doc.blocks, a),
            None => doc.blocks.clone(),
        };
        let _ = writeln!(
            out,
            "<div class=\"embed\"><div class=\"embed-title\" dir=\"auto\">{title}</div>"
        );
        self.embed_stack.push(target.to_string());
        self.blocks(&blocks, out);
        self.embed_stack.pop();
        out.push_str("</div>\n");
    }

    fn raw_html(&mut self, raw: &str, out: &mut String) {
        match sanitize(raw) {
            Some(clean) => {
                out.push_str(&clean);
                out.push('\n');
            }
            None => {
                let _ = writeln!(
                    out,
                    "<pre class=\"raw-html\" dir=\"ltr\"><code>{}</code></pre>",
                    esc(raw)
                );
            }
        }
    }

    fn properties(&mut self, raw: &str, out: &mut String) {
        let props = parse_properties(raw);
        if props.is_empty() {
            return;
        }
        let _ = write!(
            out,
            "<details class=\"props\" open><summary>{CHEVRON}{}</summary><div class=\"row\">",
            esc(&self.cfg.labels.properties)
        );
        for (key, values) in props {
            let is_tags = key == "tags" || key == "tag";
            for value in values {
                if is_tags {
                    let _ = write!(
                        out,
                        "<span class=\"tag\" dir=\"auto\">#{}</span>",
                        esc(value.trim_start_matches('#'))
                    );
                } else {
                    let mono =
                        value.len() >= 8
                            && value.as_bytes().iter().take(10).all(|b| {
                                b.is_ascii_digit() || *b == b'-' || *b == b':' || *b == b'T'
                            });
                    let _ = write!(
                        out,
                        "<span class=\"pill\"><span class=\"k\">{}</span><span class=\"v{}\" dir=\"auto\">{}</span></span>",
                        esc(&key),
                        if mono { " mono" } else { "" },
                        self.property_text(&value)
                    );
                }
            }
        }
        out.push_str("</div></details>\n");
    }

    fn property_text(&self, value: &str) -> String {
        static WIKI: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            regex::Regex::new(r"\[\[([^\[\]|#]+)(?:#([^\[\]|]*))?(?:\|([^\[\]]*))?\]\]")
                .expect("wiki link pattern")
        });
        let mut out = String::new();
        let mut last = 0;
        for captures in WIKI.captures_iter(value) {
            let whole = captures.get(0).expect("the whole match");
            out.push_str(&esc(&value[last..whole.start()]));
            let target = captures[1].trim();
            let shown = captures
                .get(3)
                .map(|m| m.as_str().trim())
                .filter(|d| !d.is_empty())
                .unwrap_or(target);
            let anchor = captures
                .get(2)
                .map(|m| m.as_str().trim())
                .filter(|a| !a.is_empty());
            match self.resolver.link(target, anchor) {
                Some(href) => {
                    let _ = write!(out, "<a href=\"{}\">{}</a>", esc(&href), esc(shown));
                }
                None => out.push_str(&esc(shown)),
            }
            last = whole.end();
        }
        out.push_str(&esc(&value[last..]));
        out
    }

    fn footnote_section(&mut self, doc: &Document, out: &mut String) {
        if self.footnotes.is_empty() {
            return;
        }
        let _ = writeln!(
            out,
            "<section class=\"footnotes\" aria-label=\"{}\"><ol>",
            esc(&self.cfg.labels.footnotes)
        );
        let used = std::mem::take(&mut self.footnotes);
        for (n, label) in used.iter().enumerate() {
            let Some(note) = doc.footnotes.iter().find(|f| &f.label == label) else {
                continue;
            };
            let mut body = String::new();
            self.blocks(&note.blocks, &mut body);
            let back = format!(
                " <a href=\"#fnref-{}\" aria-label=\"{}\">{}</a>",
                n + 1,
                esc(&self.cfg.labels.back_to_text),
                esc(&self.cfg.labels.back_to_text)
            );
            let body = match body.trim_end().strip_suffix("</p>") {
                Some(head) => format!("{head}{back}</p>"),
                None => format!("{body}<p>{back}</p>"),
            };
            let _ = writeln!(out, "<li id=\"fn-{}\">{}</li>", n + 1, body);
        }
        out.push_str("</ol></section>\n");
    }

    fn inlines(&mut self, inlines: &[Inline], out: &mut String) {
        for inline in inlines {
            match &inline.kind {
                InlineKind::Text(t) => out.push_str(&esc(t)),
                InlineKind::Code(c) => {
                    let _ = write!(out, "<code dir=\"ltr\">{}</code>", esc(c));
                }
                InlineKind::Strong(c) => self.wrap("strong", c, out),
                InlineKind::Emphasis(c) => self.wrap("em", c, out),
                InlineKind::Strike(c) => self.wrap("del", c, out),
                InlineKind::Highlight(c) => {
                    out.push_str(if self.cfg.fragment {
                        "<mark>"
                    } else {
                        "<mark class=\"hl\">"
                    });
                    self.inlines(c, out);
                    out.push_str("</mark>");
                }
                InlineKind::Link(link) => self.link(link, out),
                InlineKind::Image(image) => {
                    out.push_str(&self.image_tag(image, "inline"));
                }
                InlineKind::Tag(t) => {
                    let _ = write!(out, "<span class=\"tag\" dir=\"auto\">#{}</span>", esc(t));
                }
                InlineKind::Math(m) => {
                    let _ = write!(out, "<span class=\"math\" dir=\"ltr\">{}</span>", esc(m));
                }
                InlineKind::FootnoteRef(label) => {
                    let n = match self.footnotes.iter().position(|l| l == label) {
                        Some(i) => i + 1,
                        None => {
                            self.footnotes.push(label.clone());
                            self.footnotes.len()
                        }
                    };
                    let _ = write!(
                        out,
                        "<sup class=\"fn\" id=\"fnref-{n}\"><a href=\"#fn-{n}\">{n}</a></sup>"
                    );
                }
                InlineKind::BlockId(id) => {
                    let _ = write!(out, "<span id=\"blk-{}\"></span>", esc(id));
                }
                InlineKind::SoftBreak => out.push('\n'),
                InlineKind::HardBreak => out.push_str("<br>\n"),
                InlineKind::Html(raw) => out.push_str(&sanitize_inline(raw)),
            }
        }
    }

    fn wrap(&mut self, tag: &str, children: &[Inline], out: &mut String) {
        let _ = write!(out, "<{tag}>");
        self.inlines(children, out);
        let _ = write!(out, "</{tag}>");
    }

    fn link(&mut self, link: &Link, out: &mut String) {
        let mut label = String::new();
        self.inlines(&link.children, &mut label);
        let anchor_id = |a: &str| match a.strip_prefix('^') {
            Some(block) => format!("blk-{block}"),
            None => slug(a),
        };
        let (href, external) = match link.kind {
            LinkKind::Auto => (
                Some(link.target.clone()).filter(|u| is_safe_url(u, false)),
                true,
            ),
            LinkKind::Markdown if is_url(&link.target) => (
                Some(link.target.clone()).filter(|u| is_safe_url(u, false)),
                link.target.starts_with("http"),
            ),
            LinkKind::Markdown | LinkKind::Wiki if link.target.is_empty() => (
                link.anchor.as_deref().map(|a| format!("#{}", anchor_id(a))),
                false,
            ),
            LinkKind::Wiki => (
                self.resolver
                    .link(&link.target, link.anchor.as_deref())
                    .map(local_url),
                false,
            ),
            LinkKind::Markdown => {
                let decoded = percent_decode(&link.target);
                let stripped = decoded.strip_suffix(".md").unwrap_or(&decoded);
                match self.resolver.link(stripped, link.anchor.as_deref()) {
                    Some(url) => (Some(local_url(url)), false),
                    None if decoded.ends_with(".md") || !decoded.contains('.') => (None, false),
                    None => (Some(local_url(percent_encode_path(&link.target))), false),
                }
            }
        };
        match href {
            Some(href) => {
                let _ = write!(
                    out,
                    "<a href=\"{}\"{}>{label}</a>",
                    esc(&href),
                    if external && !self.cfg.fragment {
                        " class=\"external\" target=\"_blank\" rel=\"noopener noreferrer\""
                    } else {
                        ""
                    }
                );
            }
            None => {
                let _ = write!(out, "<span class=\"unresolved\">{label}</span>");
            }
        }
    }
}

fn collect_languages(blocks: &[Block], out: &mut Vec<String>) {
    for block in blocks {
        match &block.kind {
            BlockKind::CodeBlock { lang, .. } if !lang.is_empty() => out.push(lang.clone()),
            BlockKind::List { items, .. } => {
                items.iter().for_each(|i| collect_languages(&i.blocks, out))
            }
            BlockKind::Tabs { tabs, .. } => {
                tabs.iter().for_each(|t| collect_languages(&t.blocks, out))
            }
            BlockKind::Quote(b)
            | BlockKind::Callout { blocks: b, .. }
            | BlockKind::Details { blocks: b, .. } => collect_languages(b, out),
            _ => {}
        }
    }
}

pub(super) fn select_anchor(blocks: &[Block], anchor: &str) -> Vec<Block> {
    if let Some(id) = anchor.strip_prefix('^') {
        let has_id = |inlines: &[Inline]| {
            inlines
                .iter()
                .any(|i| matches!(&i.kind, InlineKind::BlockId(b) if b == id))
        };
        return blocks
            .iter()
            .filter(|b| match &b.kind {
                BlockKind::Paragraph(i) | BlockKind::Heading { inlines: i, .. } => has_id(i),
                BlockKind::List { items, .. } => items.iter().any(|it| {
                    it.blocks
                        .iter()
                        .any(|b| matches!(&b.kind, BlockKind::Paragraph(i) if has_id(i)))
                }),
                _ => false,
            })
            .cloned()
            .collect();
    }
    let wanted = slug(anchor);
    let Some(start) = blocks
        .iter()
        .position(|b| matches!(&b.kind, BlockKind::Heading { id, .. } if slug(id) == wanted))
    else {
        return Vec::new();
    };
    let BlockKind::Heading { level, .. } = &blocks[start].kind else {
        return Vec::new();
    };
    let end = blocks[start + 1..]
        .iter()
        .position(|b| matches!(&b.kind, BlockKind::Heading { level: l, .. } if l <= level))
        .map_or(blocks.len(), |o| start + 1 + o);
    blocks[start..end].to_vec()
}

fn parse_properties(raw: &str) -> Vec<(String, Vec<String>)> {
    let mut props: Vec<(String, Vec<String>)> = Vec::new();
    let clean = |v: &str| v.trim().trim_matches(['"', '\'']).to_string();
    for line in raw.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(item) = line.trim_start().strip_prefix("- ") {
            if (line.starts_with(' ') || line.starts_with('-'))
                && let Some(last) = props.last_mut()
            {
                last.1.push(clean(item));
            }
            continue;
        }
        if line.starts_with(' ') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        let values = if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']'))
        {
            inner
                .split(',')
                .map(clean)
                .filter(|v| !v.is_empty())
                .collect()
        } else if value.is_empty() {
            Vec::new()
        } else {
            vec![clean(value)]
        };
        props.push((key.trim().to_string(), values));
    }
    props.retain(|(_, v)| !v.is_empty());
    props
}

const INLINE_TAGS: &[&str] = &[
    "kbd", "sub", "sup", "br", "mark", "span", "small", "b", "i", "u", "em", "strong", "code",
    "del", "s", "abbr", "cite", "q", "wbr", "ins", "var", "samp",
];
const BLOCK_TAGS: &[&str] = &[
    "div",
    "p",
    "details",
    "summary",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "td",
    "th",
    "caption",
    "ul",
    "ol",
    "li",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "pre",
    "hr",
    "center",
    "blockquote",
    "figure",
    "figcaption",
    "dl",
    "dt",
    "dd",
    "a",
    "img",
];

fn clean_attrs(name: &str, attrs: &[(String, String)]) -> String {
    let mut out = String::new();
    for (key, value) in attrs {
        let keep = match (name, key.as_str()) {
            ("a", "href") => is_safe_url(value, false),
            ("img", "src") => is_safe_url(value, true),
            ("img", "alt" | "width" | "height")
            | ("td" | "th", "colspan" | "rowspan" | "align")
            | ("details", "open")
            | ("ol", "start") => true,
            _ => false,
        };
        if keep {
            let _ = write!(out, " {key}=\"{}\"", esc(value));
        }
    }
    out
}

fn sanitize_inline(raw: &str) -> String {
    let mut out = String::new();
    for (_, token) in html::tokens(raw) {
        match token {
            HtmlToken::Open { name, attrs, .. }
                if INLINE_TAGS.contains(&name.as_str()) || name == "a" || name == "img" =>
            {
                let _ = write!(out, "<{name}{}>", clean_attrs(&name, &attrs));
            }
            HtmlToken::Close { name } if INLINE_TAGS.contains(&name.as_str()) || name == "a" => {
                let _ = write!(out, "</{name}>");
            }
            HtmlToken::Comment => {}
            HtmlToken::Text(t) => out.push_str(&esc(t)),
            HtmlToken::Open { raw, .. } => out.push_str(&esc(raw)),
            HtmlToken::Close { name } => out.push_str(&esc(&format!("</{name}>"))),
        }
    }
    out
}

fn sanitize(raw: &str) -> Option<String> {
    let mut out = String::new();
    for (_, token) in html::tokens(raw) {
        match token {
            HtmlToken::Open {
                name,
                attrs,
                self_closing,
                ..
            } => {
                if !(INLINE_TAGS.contains(&name.as_str()) || BLOCK_TAGS.contains(&name.as_str())) {
                    return None;
                }
                let _ = write!(
                    out,
                    "<{name}{}{}>",
                    clean_attrs(&name, &attrs),
                    if self_closing { "/" } else { "" }
                );
            }
            HtmlToken::Close { name } => {
                if !(INLINE_TAGS.contains(&name.as_str()) || BLOCK_TAGS.contains(&name.as_str())) {
                    return None;
                }
                let _ = write!(out, "</{name}>");
            }
            HtmlToken::Comment => {}
            HtmlToken::Text(t) => out.push_str(&esc(&html::decode_entities(t))),
        }
    }
    Some(out)
}

#[allow(dead_code)]
fn plain_of(inlines: &[Inline]) -> String {
    plain_text(inlines)
}
