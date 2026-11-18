use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use focus_core::{LinkTarget, NoteId, NoteMeta};
use focus_md::highlight::{self, Token};
use focus_md::{Align, Block, BlockKind, Directive, Document, Inline, InlineKind, LinkKind};

use focus_i18n::t;

use crate::Resolver;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    pub strong: bool,
    pub emphasis: bool,
    pub strike: bool,
    pub code: bool,
    pub highlight: bool,
    pub tag: bool,
    pub muted: bool,
    pub mono: bool,
    pub link: Option<u16>,
    pub token: Option<Token>,
    pub decor: Option<Decor>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decor {
    Resolved,
    Unresolved,
}

pub fn code_rich(code: &str, lang: &str, ord: u32) -> Rich {
    let mut rich = Rich {
        ord,
        text: code.to_string(),
        ..Rich::default()
    };
    let mut at = 0;
    for span in highlight::highlight(code, lang) {
        if span.range.start > at {
            rich.marks.push((
                at..span.range.start,
                Marks {
                    code: true,
                    ..Marks::default()
                },
            ));
        }
        rich.marks.push((
            span.range.clone(),
            Marks {
                code: true,
                token: Some(span.token),
                ..Marks::default()
            },
        ));
        at = span.range.end;
    }
    if at < code.len() {
        rich.marks.push((
            at..code.len(),
            Marks {
                code: true,
                ..Marks::default()
            },
        ));
    }
    rich
}

pub fn decorated(rich: &Rich, spans: &[(Range<usize>, Decor)]) -> Rich {
    let mut cuts: Vec<usize> = rich
        .marks
        .iter()
        .flat_map(|(r, _)| [r.start, r.end])
        .collect();
    for (range, _) in spans {
        cuts.extend([
            range.start.min(rich.text.len()),
            range.end.min(rich.text.len()),
        ]);
    }
    cuts.extend([0, rich.text.len()]);
    cuts.sort_unstable();
    cuts.dedup();
    let mut marks: Vec<(Range<usize>, Marks)> = Vec::new();
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start == end || !rich.text.is_char_boundary(start) || !rich.text.is_char_boundary(end) {
            continue;
        }
        let mut m = rich
            .marks
            .iter()
            .find(|(r, _)| r.start <= start && end <= r.end)
            .map(|(_, m)| *m)
            .unwrap_or_default();
        m.decor = spans
            .iter()
            .find(|(r, _)| r.start <= start && end <= r.end)
            .map(|(_, d)| *d);
        match marks.last_mut() {
            Some((r, last)) if *last == m && r.end == start => r.end = end,
            _ => marks.push((start..end, m)),
        }
    }
    Rich {
        marks,
        ..rich.clone()
    }
}

#[derive(Clone, Debug)]
pub enum LinkGoal {
    Note { id: NoteId, anchor: Option<String> },
    External(String),
    Attachment(PathBuf),
    Unresolved(String),
    Tag(String),
    Footnote(usize),
}

#[derive(Clone, Debug)]
pub struct LinkSpan {
    pub range: Range<usize>,
    pub goal: LinkGoal,
}

#[derive(Clone, Debug, Default)]
pub struct Rich {
    pub ord: u32,
    pub text: String,
    pub marks: Vec<(Range<usize>, Marks)>,
    pub links: Vec<LinkSpan>,
    pub map: Option<Arc<crate::srcmap::SourceMap>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Neutral,
    Ok,
    Warn,
    Danger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalloutIcon {
    Note,
    Abstract,
    Info,
    Todo,
    Tip,
    Success,
    Question,
    Warning,
    Failure,
    Danger,
    Bug,
    Example,
    Quote,
}

#[derive(Clone, Debug)]
pub enum ImageSource {
    File(PathBuf),
    Url(String),
    Missing(String),
    Pdf(PdfEmbed),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfEmbed {
    pub target: String,
    pub file: Option<(PathBuf, PathBuf)>,
    pub page: usize,
}

#[derive(Clone, Debug)]
pub struct ImageRef {
    pub source: ImageSource,
    pub width: Option<u32>,
    pub caption: Option<String>,
}

pub const EMBED_DEPTH: u8 = 3;

#[derive(Clone, Debug)]
pub enum EmbedBody {
    Content(Vec<Node>),
    TitleOnly,
    Missing,
    AnchorMissing(String),
    Cycle,
    TooDeep,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct ListEntry {
    pub marker: String,
    pub task: Option<char>,
    pub line: u32,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug)]
pub enum HtmlPart {
    Text(Rich),
    Image(ImageRef),
}

#[derive(Clone, Debug)]
pub enum Node {
    Heading {
        level: u8,
        rich: Rich,
    },
    Paragraph(Rich),
    List {
        ordered: bool,
        tight: bool,
        entries: Vec<ListEntry>,
    },
    Code {
        label: String,
        lang: String,
        detected: bool,
        line: u32,
        code: Rich,
    },
    Mermaid {
        source: String,
    },
    Math(Rich),
    Quote(Vec<Node>),
    Callout {
        key: u32,
        family: Family,
        icon: CalloutIcon,
        title: Rich,
        fold: Option<bool>,
        children: Vec<Node>,
    },
    Details {
        key: u32,
        summary: Rich,
        open: bool,
        children: Vec<Node>,
        child_lines: Vec<Range<u32>>,
    },
    Tabs {
        key: u32,
        tabs: Vec<TabNode>,
    },
    Ref(Rich),
    Base {
        caption: Rich,
        sections: Vec<BaseSection>,
        view: Option<u32>,
    },
    Query(crate::query::QueryBlock),
    Directive(String),
    Table {
        align: Vec<Align>,
        head: Vec<Rich>,
        rows: Vec<Vec<Rich>>,
        cells: Vec<Vec<Range<usize>>>,
    },
    Embed {
        label: Rich,
        goal: Option<NoteId>,
        anchor: Option<String>,
        body: EmbedBody,
    },
    Image(ImageRef),
    Rule,
    Html(Vec<HtmlPart>),
    Footnotes(Vec<FootnoteNode>),
}

#[derive(Clone, Debug)]
pub struct BaseSection {
    pub title: Option<Rich>,
    pub table: Node,
}

pub struct Header {
    pub crumbs: Vec<String>,
    pub title: Rich,
    pub props: Vec<Prop>,
    pub tags: Vec<String>,
}

pub struct Prop {
    pub key: String,
    pub value: Rich,
    pub kind: focus_core::PropertyKind,
    pub fits: bool,
    pub checked: Option<bool>,
}

impl Prop {
    pub fn is_field(&self) -> bool {
        self.fits
            && matches!(
                self.kind.kind,
                focus_core::PropertyType::Number
                    | focus_core::PropertyType::Date
                    | focus_core::PropertyType::Datetime
            )
    }

    pub fn is_checkbox(&self) -> bool {
        self.fits && self.kind.kind == focus_core::PropertyType::Checkbox
    }
}

#[derive(Clone, Debug)]
pub struct Item {
    pub node: Node,
    pub lines: Range<u32>,
}

#[derive(Clone, Debug)]
pub struct FootnoteNode {
    pub label: String,
    pub blocks: Vec<Node>,
    pub lines: Range<u32>,
}

#[derive(Clone, Debug)]
pub struct TabNode {
    pub title: String,
    pub title_line: u32,
    pub lines: Range<u32>,
    pub items: Vec<Item>,
}

pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: u32,
    pub item: usize,
    pub inner: bool,
}

pub struct Built {
    pub note: Option<NoteId>,
    pub diagram_targets: std::collections::HashMap<String, (LinkGoal, String)>,
    pub header: Header,
    pub items: Vec<Item>,
    pub headings: Vec<Heading>,
    pub section_end: Vec<usize>,
    pub frontmatter_lines: Option<Range<u32>>,
    pub title_lines: Option<Range<u32>>,
    pub line_count: u32,
    pub source: Option<Arc<str>>,
}

pub fn heading_matches(heading: &str, anchor: &str) -> bool {
    let anchor = anchor.trim_start_matches('#').trim();
    let anchor = anchor.rsplit('#').next().unwrap_or(anchor);
    let text = without_isolates(heading);
    if text.trim().eq_ignore_ascii_case(anchor) {
        return true;
    }
    let slug: String = text
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                Some(c)
            } else if c == ' ' {
                Some('-')
            } else {
                None
            }
        })
        .collect();
    slug == percent_decode(anchor).to_lowercase()
}

impl Built {
    pub fn heading_item(&self, anchor: &str) -> Option<usize> {
        self.headings
            .iter()
            .find(|h| heading_matches(&h.text, anchor))
            .map(|h| h.item)
    }

    pub fn item_at_line(&self, line: u32) -> usize {
        match self.items.binary_search_by(|item| {
            if item.lines.end <= line {
                std::cmp::Ordering::Less
            } else if item.lines.start > line {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }
}

struct Ctx<'a> {
    library: &'a dyn Resolver,
    note: Option<NoteId>,
    note_dir: PathBuf,
    footnote_labels: Vec<String>,
    languages: Vec<String>,
    path: Option<String>,
    ord: u32,
    callout: u32,
    diagram_targets: std::collections::HashMap<String, (LinkGoal, String)>,
    embedding: Vec<NoteId>,
    host_links: Option<&'a dyn crate::HostLinks>,
    text: Option<&'a str>,
    origin: Option<usize>,
}

impl Ctx<'_> {
    fn rich(&mut self) -> Rich {
        let rich = Rich {
            ord: self.ord,
            ..Rich::default()
        };
        self.ord += 1;
        rich
    }

    fn resolve(&self, kind: LinkKind, target: &str, anchor: Option<&String>) -> LinkGoal {
        if kind == LinkKind::Auto || target.contains("://") || target.starts_with("mailto:") {
            return LinkGoal::External(target.to_string());
        }
        if target.is_empty() {
            return match (self.note, anchor) {
                (Some(id), Some(anchor)) => LinkGoal::Note {
                    id,
                    anchor: Some(anchor.clone()),
                },
                _ => LinkGoal::Unresolved(String::new()),
            };
        }
        let unresolved = || {
            self.library.known_file(target).map_or_else(
                || LinkGoal::Unresolved(target.to_string()),
                LinkGoal::Attachment,
            )
        };
        let resolved = match self.note {
            Some(note) => self.library.resolve_link(note, target),
            None => self.library.resolve_from_root(target),
        };
        match resolved {
            LinkTarget::Note {
                id,
                anchor: resolved,
            } => LinkGoal::Note {
                id,
                anchor: resolved.or_else(|| anchor.cloned()),
            },
            LinkTarget::Attachment(path) => LinkGoal::Attachment(path),
            LinkTarget::External(url) => LinkGoal::External(url),
            LinkTarget::Unresolved => unresolved(),
        }
    }

    fn pdf_page(&self, target: &str, anchor: Option<&String>, bytes: &Range<usize>) -> ImageRef {
        let page = anchor
            .and_then(|a| focus_view_media::pdf::page_anchor(a))
            .unwrap_or(0);
        let file = match self.resolve(LinkKind::Wiki, target, None) {
            LinkGoal::Attachment(relative) => Some((self.library.absolute(&relative), relative)),
            _ => None,
        };
        let width = self
            .text
            .and_then(|text| text.get(bytes.clone()))
            .and_then(embed_width);
        ImageRef {
            source: ImageSource::Pdf(PdfEmbed {
                target: target.to_string(),
                file,
                page,
            }),
            width,
            caption: None,
        }
    }

    fn image(&self, src: &str, width: Option<u32>) -> ImageRef {
        let source = if src.starts_with("http://") || src.starts_with("https://") {
            ImageSource::Url(src.to_string())
        } else {
            let decoded = percent_decode(src);
            let relative = self.note_dir.join(&decoded);
            let absolute = self.library.absolute(&relative);
            if absolute.is_file() {
                ImageSource::File(absolute)
            } else if let LinkTarget::Attachment(path) = match self.note {
                Some(note) => self.library.resolve_link(note, &decoded),
                None => self.library.resolve_from_root(&decoded),
            } {
                ImageSource::File(self.library.absolute(&path))
            } else {
                ImageSource::Missing(src.to_string())
            }
        };
        ImageRef {
            source,
            width,
            caption: None,
        }
    }

    fn embed(&mut self, target: &str, anchor: Option<&String>) -> (Option<NoteId>, EmbedBody) {
        let id = match self.resolve(LinkKind::Wiki, target, anchor) {
            LinkGoal::Note { id, .. } => id,
            _ => return (None, EmbedBody::Missing),
        };
        if self.embedding.contains(&id) {
            return (Some(id), EmbedBody::Cycle);
        }
        if self.embedding.len() > EMBED_DEPTH as usize {
            return (Some(id), EmbedBody::TooDeep);
        }
        let (text, meta) = match self.library.note_text(id) {
            None => return (Some(id), EmbedBody::TitleOnly),
            Some(Err(message)) => return (Some(id), EmbedBody::Failed(message)),
            Some(Ok(found)) => found,
        };
        let document = focus_md::parse(&text, &focus_md::ParseOptions::default());
        let Some(selected) = embedded_blocks(&document, &meta, anchor.map(String::as_str)) else {
            return (
                Some(id),
                EmbedBody::AnchorMissing(anchor.cloned().unwrap_or_default()),
            );
        };
        let saved = (
            self.note,
            std::mem::replace(
                &mut self.note_dir,
                meta.path
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_default(),
            ),
        );
        let labels = std::mem::replace(
            &mut self.footnote_labels,
            document.footnotes.iter().map(|f| f.label.clone()).collect(),
        );
        self.note = Some(id);
        self.embedding.push(id);
        let path = self.path.replace(meta.path.to_string_lossy().into_owned());
        let mapping = (self.text.take(), self.origin.take());
        let nodes = blocks(self, selected);
        (self.text, self.origin) = mapping;
        self.path = path;
        self.embedding.pop();
        self.note = saved.0;
        self.note_dir = saved.1;
        self.footnote_labels = labels;
        (Some(id), EmbedBody::Content(nodes))
    }
}

pub(crate) fn embedded_blocks<'d>(
    document: &'d focus_md::Document,
    meta: &NoteMeta,
    anchor: Option<&str>,
) -> Option<&'d [Block]> {
    let blocks = &document.blocks[..];
    match anchor.map(|a| a.trim_start_matches('#')) {
        None | Some("") => {
            let skip = matches!(blocks.first().map(|b| &b.kind), Some(BlockKind::Heading { level: 1, inlines, .. }) if focus_md::plain_text(inlines).trim() == meta.title.trim());
            Some(&blocks[skip as usize..])
        }
        Some(anchor) if anchor.starts_with('^') => {
            let id = &anchor[1..];
            let at = blocks.iter().position(|b| carries_block_id(b, id))?;
            Some(&blocks[at..at + 1])
        }
        Some(anchor) => {
            let wanted = anchor
                .rsplit('#')
                .next()
                .unwrap_or(anchor)
                .trim()
                .to_lowercase();
            let (at, level) = blocks.iter().enumerate().find_map(|(i, b)| match &b.kind {
                BlockKind::Heading {
                    level, id, inlines, ..
                } if id.to_lowercase() == wanted
                    || focus_md::plain_text(inlines).trim().to_lowercase() == wanted =>
                {
                    Some((i, *level))
                }
                _ => None,
            })?;
            let end = blocks[at + 1..]
                .iter()
                .position(|b| matches!(b.kind, BlockKind::Heading { level: l, .. } if l <= level))
                .map_or(blocks.len(), |p| at + 1 + p);
            Some(&blocks[at..end])
        }
    }
}

fn diagram_targets(source: &str) -> Vec<String> {
    let clean = |raw: &str| {
        let raw = raw.trim().trim_matches(['"', '\'']);
        let inner = raw
            .strip_prefix("[[")
            .and_then(|t| t.strip_suffix("]]"))
            .unwrap_or(raw);
        inner.split('|').next().unwrap_or(inner).trim().to_string()
    };
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = lower
            .strip_prefix("click ")
            .or_else(|| lower.strip_prefix("link "))
            .map(|_| {
                trimmed
                    .split_once(char::is_whitespace)
                    .map_or("", |(_, r)| r)
            })
        {
            let rest = rest
                .trim()
                .split_once(char::is_whitespace)
                .map_or("", |(_, r)| r)
                .trim();
            let rest = rest.strip_prefix("href ").unwrap_or(rest).trim();
            let target = match rest.strip_prefix('"') {
                Some(inner) => inner.split('"').next().unwrap_or(""),
                None => rest.split_whitespace().next().unwrap_or(""),
            };
            if !target.is_empty() && !rest.starts_with("call ") {
                out.push(clean(target));
            }
            continue;
        }
        let mut at = 0;
        while let Some(open) = trimmed[at..].find("[[").map(|o| at + o) {
            let Some(close) = trimmed[open..].find("]]").map(|c| open + c) else {
                break;
            };
            let shape =
                open > 0 && trimmed[..open].ends_with(|c: char| c.is_alphanumeric() || c == '_');
            if !shape {
                out.push(clean(&trimmed[open..close + 2]));
            }
            at = close + 2;
        }
    }
    out.retain(|t| !t.is_empty());
    out
}

fn carries_block_id(block: &Block, id: &str) -> bool {
    let inline = |items: &[Inline]| {
        items
            .iter()
            .any(|i| matches!(&i.kind, InlineKind::BlockId(b) if b == id))
    };
    match &block.kind {
        BlockKind::Paragraph(items) | BlockKind::Heading { inlines: items, .. } => inline(items),
        BlockKind::List { items, .. } => items
            .iter()
            .any(|item| item.blocks.iter().any(|b| carries_block_id(b, id))),
        BlockKind::Quote(inner)
        | BlockKind::Callout { blocks: inner, .. }
        | BlockKind::Details { blocks: inner, .. } => inner.iter().any(|b| carries_block_id(b, id)),
        BlockKind::Table { head, rows, .. } => {
            head.iter().any(|c| inline(c)) || rows.iter().flatten().any(|c| inline(c))
        }
        _ => false,
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(byte) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub const LRI: char = '\u{2066}';
pub const RLI: char = '\u{2067}';
pub const PDI: char = '\u{2069}';

pub fn without_isolates(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(*c, LRI | RLI | PDI))
        .collect()
}

fn push(rich: &mut Rich, text: &str, marks: Marks) {
    if text.is_empty() {
        return;
    }
    let start = rich.text.len();
    rich.text.push_str(text);
    let end = rich.text.len();
    match rich.marks.last_mut() {
        Some((range, last)) if *last == marks && range.end == start => range.end = end,
        _ => rich.marks.push((start..end, marks)),
    }
}

fn host_text(rich: &mut Rich, text: &str, marks: Marks, host: &dyn crate::HostLinks) {
    let mut at = 0;
    for (range, goal) in host.find(text) {
        if range.start < at
            || range.end > text.len()
            || range.is_empty()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            continue;
        }
        push(rich, &text[at..range.start], marks);
        let index = rich.links.len() as u16;
        let start = rich.text.len();
        push(
            rich,
            &text[range.clone()],
            Marks {
                link: Some(index),
                ..marks
            },
        );
        rich.links.push(LinkSpan {
            range: start..rich.text.len(),
            goal,
        });
        at = range.end;
    }
    push(rich, &text[at..], marks);
}

fn inlines(ctx: &mut Ctx, rich: &mut Rich, items: &[Inline], marks: Marks) {
    for inline in items {
        let shown = rich.text.len();
        match &inline.kind {
            InlineKind::Text(text) => match ctx
                .host_links
                .filter(|_| marks.link.is_none() && !marks.code)
            {
                Some(host) => host_text(rich, text, marks, host),
                None => push(rich, text, marks),
            },
            InlineKind::Code(code) => push(
                rich,
                &format!("{LRI}{code}{PDI}"),
                Marks {
                    code: true,
                    ..marks
                },
            ),
            InlineKind::Strong(inner) => inlines(
                ctx,
                rich,
                inner,
                Marks {
                    strong: true,
                    ..marks
                },
            ),
            InlineKind::Emphasis(inner) => inlines(
                ctx,
                rich,
                inner,
                Marks {
                    emphasis: true,
                    ..marks
                },
            ),
            InlineKind::Strike(inner) => inlines(
                ctx,
                rich,
                inner,
                Marks {
                    strike: true,
                    ..marks
                },
            ),
            InlineKind::Highlight(inner) => inlines(
                ctx,
                rich,
                inner,
                Marks {
                    highlight: true,
                    ..marks
                },
            ),
            InlineKind::Link(link) => {
                let goal = ctx.resolve(link.kind, &link.target, link.anchor.as_ref());
                let index = rich.links.len() as u16;
                let start = rich.text.len();
                inlines(
                    ctx,
                    rich,
                    &link.children,
                    Marks {
                        link: Some(index),
                        ..marks
                    },
                );
                if rich.text.len() == start {
                    push(
                        rich,
                        &link.target,
                        Marks {
                            link: Some(index),
                            ..marks
                        },
                    );
                }
                let end = rich.text.len();
                rich.links.push(LinkSpan {
                    range: start..end,
                    goal,
                });
            }
            InlineKind::Image(image) => push(
                rich,
                &format!(
                    "[{}]",
                    if image.alt.is_empty() {
                        &image.src
                    } else {
                        &image.alt
                    }
                ),
                Marks {
                    muted: true,
                    ..marks
                },
            ),
            InlineKind::Tag(tag) => {
                let index = rich.links.len() as u16;
                let start = rich.text.len();
                let isolate =
                    if matches!(focus_i18n::text_direction(tag), focus_i18n::Direction::Rtl) {
                        RLI
                    } else {
                        LRI
                    };
                push(
                    rich,
                    &format!("{isolate}#{tag}{PDI}"),
                    Marks {
                        tag: true,
                        link: Some(index),
                        ..marks
                    },
                );
                rich.links.push(LinkSpan {
                    range: start..rich.text.len(),
                    goal: LinkGoal::Tag(tag.clone()),
                });
            }
            InlineKind::Math(tex) => push(
                rich,
                tex,
                Marks {
                    code: true,
                    ..marks
                },
            ),
            InlineKind::FootnoteRef(label) => {
                match ctx.footnote_labels.iter().position(|l| l == label) {
                    Some(i) => {
                        let index = rich.links.len() as u16;
                        let start = rich.text.len();
                        push(
                            rich,
                            &format!("[{}]", i + 1),
                            Marks {
                                muted: true,
                                link: Some(index),
                                ..marks
                            },
                        );
                        rich.links.push(LinkSpan {
                            range: start..rich.text.len(),
                            goal: LinkGoal::Footnote(i),
                        });
                    }
                    None => push(
                        rich,
                        &format!("[{label}]"),
                        Marks {
                            muted: true,
                            ..marks
                        },
                    ),
                }
            }
            InlineKind::BlockId(_) => {}
            InlineKind::SoftBreak | InlineKind::HardBreak => push(rich, "\n", marks),
            InlineKind::Html(html) => html_inline(rich, html, marks),
        }
        map_inline(ctx, rich, inline, shown..rich.text.len());
    }
}

fn map_inline(ctx: &Ctx, rich: &mut Rich, inline: &Inline, shown: Range<usize>) {
    use crate::srcmap::{Markup, Piece};
    let Some(origin) = ctx.origin else { return };
    let relative = |r: &Range<usize>| r.start.saturating_sub(origin)..r.end.saturating_sub(origin);
    let source = relative(&inline.source);
    let map = Arc::make_mut(rich.map.get_or_insert_with(Default::default));
    let atom = |map: &mut crate::srcmap::SourceMap, shown: Range<usize>, source: Range<usize>| {
        map.pieces.push(Piece {
            shown,
            source,
            exact: false,
        })
    };
    let around = |children: &[Inline]| {
        let open = source.start
            ..children
                .first()
                .map_or(source.end, |c| relative(&c.source).start);
        let close = children
            .last()
            .map_or(source.end, |c| relative(&c.source).end)..source.end;
        (open, close)
    };
    let revealed_atom = |map: &mut crate::srcmap::SourceMap| {
        if shown.is_empty() {
            return;
        }
        map.pieces.push(Piece {
            shown: shown.clone(),
            source: source.clone(),
            exact: false,
        });
        map.markup.push(Markup {
            element: source.clone(),
            inserts: vec![(shown.start, source.clone())],
            hides: Some(shown.clone()),
            line: false,
            value: false,
        });
    };
    match &inline.kind {
        InlineKind::Text(_) => {
            let exact = inline.exact() && shown.len() == source.len();
            let escaped = exact
                && source.start > 0
                && ctx
                    .text
                    .is_some_and(|t| t.as_bytes().get(origin + source.start - 1) == Some(&b'\\'))
                && map
                    .pieces
                    .last()
                    .is_none_or(|p| p.source.end < source.start);
            let first = rich.text[shown.clone()]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
            if escaped && first > 0 {
                map.pieces.push(Piece {
                    shown: shown.start..shown.start + first,
                    source: source.start - 1..source.start + first,
                    exact: false,
                });
                if shown.len() > first {
                    map.pieces.push(Piece {
                        shown: shown.start + first..shown.end,
                        source: source.start + first..source.end,
                        exact: true,
                    });
                }
            } else {
                map.pieces.push(Piece {
                    shown,
                    source,
                    exact,
                });
            }
        }
        InlineKind::Code(code) => {
            let iso = LRI.len_utf8();
            let ticks = source.len().saturating_sub(code.len()) / 2;
            let inner = source.start + ticks..source.end - ticks;
            atom(
                map,
                shown.start..shown.start + iso,
                inner.start..inner.start,
            );
            map.pieces.push(Piece {
                shown: shown.start + iso..shown.end - iso,
                source: inner.clone(),
                exact: inner.len() == code.len(),
            });
            atom(map, shown.end - iso..shown.end, inner.end..inner.end);
            map.markup.push(Markup {
                element: source.clone(),
                inserts: vec![
                    (shown.start + iso, source.start..inner.start),
                    (shown.end - iso, inner.end..source.end),
                ],
                hides: None,
                line: false,
                value: false,
            });
        }
        InlineKind::Strong(children)
        | InlineKind::Emphasis(children)
        | InlineKind::Strike(children)
        | InlineKind::Highlight(children) => {
            let (open, close) = around(children);
            map.markup.push(Markup {
                element: source.clone(),
                inserts: vec![(shown.start, open), (shown.end, close)],
                hides: None,
                line: false,
                value: false,
            });
        }
        InlineKind::Link(link) if !link.children.is_empty() => {
            let (open, close) = around(&link.children);
            map.markup.push(Markup {
                element: source.clone(),
                inserts: vec![(shown.start, open), (shown.end, close)],
                hides: None,
                line: false,
                value: false,
            });
        }
        InlineKind::Tag(_) => {
            let iso = LRI.len_utf8();
            atom(
                map,
                shown.start..shown.start + iso,
                source.start..source.start,
            );
            map.pieces.push(Piece {
                shown: shown.start + iso..shown.end - iso,
                source: source.clone(),
                exact: shown.len() == source.len() + 2 * iso,
            });
            atom(map, shown.end - iso..shown.end, source.end..source.end);
        }
        InlineKind::Math(tex) if source.len() == tex.len() + 2 => {
            let inner = source.start + 1..source.end - 1;
            map.pieces.push(Piece {
                shown: shown.clone(),
                source: inner.clone(),
                exact: true,
            });
            map.markup.push(Markup {
                element: source.clone(),
                inserts: vec![
                    (shown.start, source.start..inner.start),
                    (shown.end, inner.end..source.end),
                ],
                hides: None,
                line: false,
                value: false,
            });
        }
        InlineKind::BlockId(_) => map.markup.push(Markup {
            element: source.clone(),
            inserts: vec![(shown.start, source.clone())],
            hides: None,
            line: false,
            value: false,
        }),
        InlineKind::SoftBreak | InlineKind::HardBreak => {
            let exact = shown.len() == 1 && source.len() == 1;
            map.pieces.push(Piece {
                shown,
                source,
                exact,
            });
        }
        InlineKind::Link(_)
        | InlineKind::Image(_)
        | InlineKind::Math(_)
        | InlineKind::FootnoteRef(_)
        | InlineKind::Html(_) => revealed_atom(map),
    }
}

fn map_empty(rich: &mut Rich, at: usize) {
    rich.map = Some(Arc::new(crate::srcmap::SourceMap {
        pieces: vec![crate::srcmap::Piece {
            shown: 0..0,
            source: at..at,
            exact: true,
        }],
        markup: Vec::new(),
    }));
}

fn map_heading(ctx: &Ctx, rich: &mut Rich, marker: &Range<usize>, block: &Range<usize>) {
    let Some(origin) = ctx.origin else { return };
    if marker.is_empty() || marker.start > block.start || marker.start < origin {
        return;
    }
    let map = Arc::make_mut(rich.map.get_or_insert_with(Default::default));
    map.markup.push(crate::srcmap::Markup {
        element: marker.start - origin..block.end - origin,
        inserts: vec![(0, marker.start - origin..marker.end - origin)],
        hides: None,
        line: true,
        value: false,
    });
}

fn map_blanks(ctx: &Ctx, rich: &mut Rich) {
    let (Some(origin), Some(text)) = (ctx.origin, ctx.text) else {
        return;
    };
    let shown = rich.text.len();
    let Some(map) = rich.map.as_mut() else { return };
    let Some(end) = map
        .pieces
        .iter()
        .map(|p| p.source.end)
        .chain(
            map.markup
                .iter()
                .flat_map(|m| m.inserts.iter().map(|(_, r)| r.end)),
        )
        .max()
    else {
        return;
    };
    let after = text.as_bytes().get(origin + end..).unwrap_or_default();
    let blanks = after
        .iter()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .count();
    if blanks == 0 || !matches!(after.get(blanks), None | Some(b'\n' | b'\r' | b'|')) {
        return;
    }
    Arc::make_mut(map).markup.push(crate::srcmap::Markup {
        element: end..end + blanks,
        inserts: vec![(shown, end..end + blanks)],
        hides: None,
        line: true,
        value: false,
    });
}

fn map_code(ctx: &Ctx, rich: &mut Rich, content: &Range<usize>) {
    use crate::srcmap::Piece;
    let (Some(origin), Some(text)) = (ctx.origin, ctx.text) else {
        return;
    };
    let Some(source) = text.get(content.clone()) else {
        return;
    };
    let mut map = crate::srcmap::SourceMap::default();
    if source.starts_with(rich.text.as_str()) {
        map.pieces.push(Piece {
            shown: 0..rich.text.len(),
            source: content.start - origin..content.start - origin + rich.text.len(),
            exact: true,
        });
    } else {
        let mut shown = 0;
        let mut at = content.start;
        let mut lines = source.split_inclusive('\n');
        for line in rich.text.split('\n') {
            let Some(source_line) = lines.next() else {
                break;
            };
            let body = source_line.trim_end_matches(['\n', '\r']);
            let end = at + body.len();
            let start = end.saturating_sub(line.len()).max(at);
            if body.ends_with(line) {
                map.pieces.push(Piece {
                    shown: shown..shown + line.len(),
                    source: start - origin..end - origin,
                    exact: true,
                });
            }
            shown += line.len();
            if shown < rich.text.len() {
                map.pieces.push(Piece {
                    shown: shown..shown + 1,
                    source: end - origin..at + source_line.len() - origin,
                    exact: source_line.len() - body.len() == 1,
                });
                shown += 1;
            }
            at += source_line.len();
        }
    }
    rich.map = Some(Arc::new(map));
}

fn html_inline(rich: &mut Rich, html: &str, marks: Marks) {
    let trimmed = html.trim();
    if trimmed.starts_with("<!--") {
        return;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("<br") {
        push(rich, "\n", marks);
    } else {
        push(
            rich,
            trimmed,
            Marks {
                code: true,
                muted: true,
                ..marks
            },
        );
    }
}

fn html_block(ctx: &mut Ctx, html: &str) -> Node {
    let mut parts = Vec::new();
    let mut rest = html;
    let mut current = ctx.rich();
    let raw = Marks {
        code: true,
        ..Marks::default()
    };
    while !rest.is_empty() {
        let lower = rest.to_ascii_lowercase();
        if lower.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |end| &rest[end + 3..]);
        } else if lower.starts_with("<br") {
            push(&mut current, "\n", raw);
            rest = rest.find('>').map_or("", |end| &rest[end + 1..]);
        } else if lower.starts_with("<img") {
            let end = rest.find('>').map_or(rest.len(), |e| e + 1);
            let tag = &rest[..end];
            let attribute = |name: &str| {
                let lower = tag.to_ascii_lowercase();
                let at = lower.find(&format!("{name}="))? + name.len() + 1;
                let quote = tag[at..].chars().next()?;
                let value = if quote == '"' || quote == '\'' {
                    let inner = &tag[at + 1..];
                    &inner[..inner.find(quote)?]
                } else {
                    tag[at..]
                        .split(|c: char| c.is_whitespace() || c == '>')
                        .next()?
                };
                Some(value.to_string())
            };
            if let Some(src) = attribute("src") {
                if !current.text.trim().is_empty() {
                    parts.push(HtmlPart::Text(std::mem::replace(&mut current, ctx.rich())));
                }
                let width = attribute("width").and_then(|w| w.trim_end_matches("px").parse().ok());
                parts.push(HtmlPart::Image(ctx.image(&src, width)));
            }
            rest = &rest[end..];
        } else {
            let next = rest[1..].find('<').map_or(rest.len(), |i| i + 1);
            push(&mut current, &rest[..next], raw);
            rest = &rest[next..];
        }
    }
    if !current.text.trim().is_empty() {
        current.text = current.text.trim_end().to_string();
        current.marks.retain_mut(|(range, _)| {
            range.end = range.end.min(current.text.len());
            range.start < range.end
        });
        parts.push(HtmlPart::Text(current));
    }
    Node::Html(parts)
}

pub fn callout_style(kind: &str) -> (Family, CalloutIcon, String) {
    let (family, english) = focus_md::callout_kind(kind);
    let key = format!("reading-callout-{}", kind.trim().to_lowercase());
    let label = match t(&key) {
        missing if missing == key => english,
        label => label,
    };
    let (tone, icon) = match family {
        "abstract" => (Family::Neutral, CalloutIcon::Abstract),
        "info" => (Family::Neutral, CalloutIcon::Info),
        "todo" => (Family::Neutral, CalloutIcon::Todo),
        "tip" => (Family::Ok, CalloutIcon::Tip),
        "success" => (Family::Ok, CalloutIcon::Success),
        "question" => (Family::Neutral, CalloutIcon::Question),
        "warning" => (Family::Warn, CalloutIcon::Warning),
        "failure" => (Family::Danger, CalloutIcon::Failure),
        "danger" => (Family::Danger, CalloutIcon::Danger),
        "bug" => (Family::Danger, CalloutIcon::Bug),
        "example" => (Family::Neutral, CalloutIcon::Example),
        "quote" => (Family::Neutral, CalloutIcon::Quote),
        _ => (Family::Neutral, CalloutIcon::Note),
    };
    (tone, icon, label)
}

fn blocks(ctx: &mut Ctx, items: &[Block]) -> Vec<Node> {
    items.iter().map(|b| node(ctx, b)).collect()
}

fn node(ctx: &mut Ctx, block: &Block) -> Node {
    match &block.kind {
        BlockKind::Heading {
            level,
            inlines: content,
            marker,
            ..
        } => {
            let mut rich = ctx.rich();
            inlines(ctx, &mut rich, content, Marks::default());
            map_heading(ctx, &mut rich, marker, &block.bytes);
            map_blanks(ctx, &mut rich);
            Node::Heading {
                level: *level,
                rich,
            }
        }
        BlockKind::Paragraph(content) => {
            let mut rich = ctx.rich();
            inlines(ctx, &mut rich, content, Marks::default());
            map_blanks(ctx, &mut rich);
            Node::Paragraph(rich)
        }
        BlockKind::List {
            ordered,
            start,
            tight,
            items,
        } => {
            let entries = items
                .iter()
                .enumerate()
                .map(|(i, item)| ListEntry {
                    marker: if *ordered {
                        format!("{}.", start + i as u64)
                    } else {
                        "•".into()
                    },
                    task: item.task,
                    line: item.lines.start,
                    children: match ctx.origin {
                        Some(origin) if item.blocks.is_empty() => {
                            let mut rich = ctx.rich();
                            map_empty(&mut rich, item.marker.end.saturating_sub(origin));
                            vec![Node::Paragraph(rich)]
                        }
                        _ => blocks(ctx, &item.blocks),
                    },
                })
                .collect();
            Node::List {
                ordered: *ordered,
                tight: *tight,
                entries,
            }
        }
        BlockKind::CodeBlock { lang, code, .. } if crate::query::is_query(lang) => Node::Query(
            crate::query::QueryBlock::new(code, block.lines.start, ctx.note, ctx.library),
        ),
        BlockKind::CodeBlock {
            lang,
            code,
            content,
        } => {
            let line = block.lines.start;
            let code = code.strip_suffix('\n').unwrap_or(code);
            let (lang_id, detected) = if lang.is_empty() {
                let languages: Vec<&str> = ctx.languages.iter().map(String::as_str).collect();
                let hints = highlight::Hints {
                    languages: &languages,
                    path: ctx.path.as_deref(),
                };
                match highlight::detect_in_note(code, &hints) {
                    Some(found) => (found.lang.to_string(), true),
                    None => (String::new(), false),
                }
            } else {
                (lang.clone(), false)
            };
            let label = if lang_id.is_empty() {
                String::new()
            } else {
                match highlight::canonical(&lang_id) {
                    Some(id) => t(&format!("reading-language-{id}")),
                    None => lang_id.clone(),
                }
            };
            let ord = ctx.rich().ord;
            let mut rich = code_rich(code, &lang_id, ord);
            map_code(ctx, &mut rich, content);
            Node::Code {
                label,
                lang: lang_id,
                detected,
                line,
                code: rich,
            }
        }
        BlockKind::Mermaid { source } => {
            for target in diagram_targets(source) {
                if ctx.diagram_targets.contains_key(&target) {
                    continue;
                }
                let (path, anchor) = target
                    .split_once('#')
                    .map_or((target.as_str(), None), |(p, a)| (p, Some(a.to_string())));
                let goal = ctx.resolve(LinkKind::Wiki, path.trim(), anchor.as_ref());
                let title = match &goal {
                    LinkGoal::Note { id, .. } => ctx
                        .library
                        .note_title(*id)
                        .unwrap_or_else(|| target.clone()),
                    _ => target.clone(),
                };
                ctx.diagram_targets.insert(target, (goal, title));
            }
            Node::Mermaid {
                source: source.clone(),
            }
        }
        BlockKind::Math { tex } => {
            let mut rich = ctx.rich();
            push(
                &mut rich,
                tex.trim(),
                Marks {
                    code: true,
                    ..Marks::default()
                },
            );
            Node::Math(rich)
        }
        BlockKind::Quote(inner) => Node::Quote(blocks(ctx, inner)),
        BlockKind::Callout {
            kind,
            title,
            fold,
            blocks: inner,
            ..
        } => {
            let (family, icon, default_title) = callout_style(kind);
            let key = ctx.callout;
            ctx.callout += 1;
            let mut rich = ctx.rich();
            if title.is_empty() {
                push(&mut rich, &default_title, Marks::default());
            } else {
                inlines(ctx, &mut rich, title, Marks::default());
                map_blanks(ctx, &mut rich);
            }
            let children = blocks(ctx, inner);
            Node::Callout {
                key,
                family,
                icon,
                title: rich,
                fold: *fold,
                children,
            }
        }
        BlockKind::Table {
            align,
            head,
            rows,
            cells,
        } => {
            let cell = |ctx: &mut Ctx, content: &[Inline]| {
                let mut rich = ctx.rich();
                inlines(ctx, &mut rich, content, Marks::default());
                map_blanks(ctx, &mut rich);
                rich
            };
            let mut head: Vec<Rich> = head.iter().map(|c| cell(ctx, c)).collect();
            let mut rows: Vec<Vec<Rich>> = rows
                .iter()
                .map(|row| row.iter().map(|c| cell(ctx, c)).collect())
                .collect();
            let cells: Vec<Vec<Range<usize>>> = match ctx.origin {
                Some(origin) => cells
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(|c| c.start.saturating_sub(origin)..c.end.saturating_sub(origin))
                            .collect()
                    })
                    .collect(),
                None => Vec::new(),
            };
            for (rich, range) in std::iter::once(&mut head)
                .chain(rows.iter_mut())
                .flat_map(|row| row.iter_mut())
                .zip(cells.iter().flatten())
            {
                if rich.text.is_empty() {
                    map_empty(rich, range.start);
                }
            }
            Node::Table {
                align: align.clone(),
                head,
                rows,
                cells,
            }
        }
        BlockKind::Embed { target, anchor } if is_extension(target, "canvas") => {
            let mut label = ctx.rich();
            push(
                &mut label,
                target,
                Marks {
                    strong: true,
                    ..Marks::default()
                },
            );
            Node::Embed {
                label,
                goal: None,
                anchor: None,
                body: EmbedBody::Failed(t("reading-canvas-unsupported")),
            }
        }
        BlockKind::Embed { target, anchor } if is_extension(target, "base") => {
            let view = anchor.as_deref().map(str::trim).filter(|v| !v.is_empty());
            let found = match ctx.resolve(LinkKind::Wiki, target, None) {
                LinkGoal::Attachment(path) => {
                    let this = ctx.path.as_deref().map(std::path::Path::new);
                    ctx.library.base_table(&path, view, this)
                }
                _ => None,
            };
            match found {
                Some(Ok(table)) => base_node(ctx, target, &table),
                Some(Err(error)) => {
                    let mut label = ctx.rich();
                    push(
                        &mut label,
                        target,
                        Marks {
                            strong: true,
                            ..Marks::default()
                        },
                    );
                    Node::Embed {
                        label,
                        goal: None,
                        anchor: anchor.clone(),
                        body: EmbedBody::Failed(base_error_text(&error)),
                    }
                }
                None => {
                    let mut label = ctx.rich();
                    push(
                        &mut label,
                        target,
                        Marks {
                            strong: true,
                            ..Marks::default()
                        },
                    );
                    Node::Embed {
                        label,
                        goal: None,
                        anchor: anchor.clone(),
                        body: EmbedBody::Missing,
                    }
                }
            }
        }
        BlockKind::Embed { target, anchor } if is_extension(target, "pdf") => {
            Node::Image(ctx.pdf_page(target, anchor.as_ref(), &block.bytes))
        }
        BlockKind::Embed { target, anchor } => {
            let (goal, body) = ctx.embed(target, anchor.as_ref());
            let mut rich = ctx.rich();
            let label = match anchor {
                Some(anchor) => format!("{target} › {anchor}"),
                None => target.clone(),
            };
            push(
                &mut rich,
                &label,
                Marks {
                    strong: true,
                    ..Marks::default()
                },
            );
            Node::Embed {
                label: rich,
                goal,
                anchor: anchor.clone(),
                body,
            }
        }
        BlockKind::Image(image) => Node::Image(ImageRef {
            caption: image.caption.clone().filter(|c| !c.trim().is_empty()),
            ..ctx.image(&image.src, image.width)
        }),
        BlockKind::Rule => Node::Rule,
        BlockKind::Html(html) => html_block(ctx, html),
        BlockKind::Details {
            summary,
            open,
            blocks: inner,
        } => {
            let key = ctx.callout;
            ctx.callout += 1;
            let mut rich = ctx.rich();
            inlines(ctx, &mut rich, summary, Marks::default());
            map_blanks(ctx, &mut rich);
            if rich.text.is_empty() {
                push(&mut rich, &t("reading-details"), Marks::default());
            }
            let children = blocks(ctx, inner);
            Node::Details {
                key,
                summary: rich,
                open: *open,
                children,
                child_lines: inner.iter().map(|b| b.lines.clone()).collect(),
            }
        }
        BlockKind::Tabs { tabs, .. } => {
            let key = ctx.callout;
            ctx.callout += 1;
            let tabs = tabs
                .iter()
                .map(|tab| TabNode {
                    title: tab.title.clone(),
                    title_line: tab.title_line,
                    lines: tab.lines.clone(),
                    items: tab
                        .blocks
                        .iter()
                        .map(|b| Item {
                            node: node(ctx, b),
                            lines: b.lines.clone(),
                        })
                        .collect(),
                })
                .collect();
            Node::Tabs { key, tabs }
        }
        BlockKind::Directive(Directive::Ref { path }) => {
            let (target, anchor) = match path.split_once('#') {
                Some((target, anchor)) => (
                    target.trim(),
                    Some(anchor.trim().to_string()).filter(|a| !a.is_empty()),
                ),
                None => (path.trim(), None),
            };
            let goal = ctx.resolve(LinkKind::Markdown, target, anchor.as_ref());
            let title = match &goal {
                LinkGoal::Note { id, .. } => ctx.library.note_title(*id),
                _ => None,
            }
            .unwrap_or_else(|| {
                std::path::Path::new(target)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| target.to_string())
            });
            let mut rich = ctx.rich();
            rich.links.push(LinkSpan {
                range: 0..title.len(),
                goal,
            });
            push(
                &mut rich,
                &title,
                Marks {
                    link: Some(0),
                    ..Marks::default()
                },
            );
            Node::Ref(rich)
        }
        BlockKind::Directive(Directive::Include { path }) => {
            let target = path.trim().split(':').next().unwrap_or_default();
            match ctx.embed(target, None) {
                (Some(goal), body) => {
                    let mut label = ctx.rich();
                    push(
                        &mut label,
                        target,
                        Marks {
                            strong: true,
                            ..Marks::default()
                        },
                    );
                    Node::Embed {
                        label,
                        goal: Some(goal),
                        anchor: None,
                        body,
                    }
                }
                (None, _) => Node::Directive(format!("{{{{#include {}}}}}", path.trim())),
            }
        }
        BlockKind::Directive(Directive::Other { name, text }) => {
            Node::Directive(if text.trim().is_empty() {
                format!("{{{{#{name}}}}}")
            } else {
                format!("{{{{#{name} {}}}}}", text.trim())
            })
        }
    }
}

fn embed_width(source: &str) -> Option<u32> {
    let inside = source.trim().strip_prefix("![[")?.strip_suffix("]]")?;
    let (_, size) = inside.rsplit_once('|')?;
    let digits: String = size
        .trim()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok().filter(|w| *w > 0)
}

fn is_extension(target: &str, extension: &str) -> bool {
    std::path::Path::new(target)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(extension))
}

fn base_node(ctx: &mut Ctx, file: &str, table: &focus_bases::Table) -> Node {
    let (caption, sections) = base_sections(ctx, file, table);
    Node::Base {
        caption,
        sections,
        view: None,
    }
}

fn base_sections(
    ctx: &mut Ctx,
    file: &str,
    table: &focus_bases::Table,
) -> (Rich, Vec<BaseSection>) {
    let muted = Marks {
        muted: true,
        ..Marks::default()
    };
    let mut caption = ctx.rich();
    let name = table.view.clone().unwrap_or_else(|| {
        std::path::Path::new(file)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    push(&mut caption, &name, Marks::default());
    let shown: usize = table.groups.iter().map(|group| group.rows.len()).sum();
    if shown < table.matched {
        let count = focus_i18n::t_args(
            "reading-base-count",
            &[("shown", shown.into()), ("total", table.matched.into())],
        );
        push(&mut caption, &format!("  {}", isolated(&count)), muted);
    }
    let sections = table
        .groups
        .iter()
        .map(|group| {
            let title = group.key.as_ref().map(|key| {
                let mut rich = ctx.rich();
                match key {
                    focus_bases::Value::Null => {
                        push(&mut rich, &t("reading-base-group-none"), muted)
                    }
                    key => {
                        let (text, marks) = base_cell(&Ok(key.clone()));
                        push(&mut rich, &text, marks);
                    }
                }
                rich
            });
            let head = table
                .columns
                .iter()
                .map(|column| {
                    let mut rich = ctx.rich();
                    let label = match column.id.strip_prefix("file.") {
                        Some(field) if !column.named => {
                            match t(&format!("reading-base-file-{field}")) {
                                missing if missing.starts_with("reading-base-file-") => {
                                    column.label.clone()
                                }
                                label => label,
                            }
                        }
                        _ => column.label.clone(),
                    };
                    push(&mut rich, &label, Marks::default());
                    rich
                })
                .collect();
            let mut rows: Vec<Vec<Rich>> = group
                .rows
                .iter()
                .map(|row| {
                    row.cells
                        .iter()
                        .zip(&table.columns)
                        .map(|(cell, column)| {
                            let mut rich = ctx.rich();
                            match cell {
                                Ok(value) if column.link => {
                                    let text = base_value(value);
                                    rich.links.push(LinkSpan {
                                        range: 0..text.len(),
                                        goal: LinkGoal::Note {
                                            id: row.note,
                                            anchor: None,
                                        },
                                    });
                                    push(
                                        &mut rich,
                                        &text,
                                        Marks {
                                            link: Some(0),
                                            ..Marks::default()
                                        },
                                    );
                                }
                                cell => {
                                    let (text, marks) = base_cell(cell);
                                    push(&mut rich, &text, marks);
                                }
                            }
                            rich
                        })
                        .collect()
                })
                .collect();
            if !group.summaries.is_empty() {
                let at = if group.key.is_some() { 0 } else { rows.len() };
                rows.insert(
                    at,
                    group
                        .summaries
                        .iter()
                        .map(|summary| {
                            let mut rich = ctx.rich();
                            if let Some(summary) = summary {
                                let label = match base_summary_key(&summary.name) {
                                    Some(key) if summary.built_in => t(key),
                                    _ => summary.name.clone(),
                                };
                                if summary.value == Ok(focus_bases::Value::Null) {
                                    push(&mut rich, &label, muted);
                                } else {
                                    push(&mut rich, &format!("{label} "), muted);
                                    let (text, marks) = base_cell(&summary.value);
                                    push(&mut rich, &isolated(&text), marks);
                                }
                            }
                            rich
                        })
                        .collect(),
                );
            }
            BaseSection {
                title,
                table: Node::Table {
                    align: vec![Align::None; table.columns.len()],
                    head,
                    rows,
                    cells: Vec::new(),
                },
            }
        })
        .collect();
    (caption, sections)
}

fn isolated(text: &str) -> String {
    let isolate = match focus_i18n::text_direction(text) {
        focus_i18n::Direction::Rtl => RLI,
        focus_i18n::Direction::Ltr => LRI,
    };
    format!("{isolate}{text}{PDI}")
}

fn base_cell(cell: &focus_bases::Drawn) -> (String, Marks) {
    match cell {
        Ok(value) => (
            base_value(value),
            Marks {
                mono: matches!(value, focus_bases::Value::Date(_)),
                ..Marks::default()
            },
        ),
        Err(error) => (
            base_cell_error(error),
            Marks {
                muted: true,
                ..Marks::default()
            },
        ),
    }
}

fn base_summary_key(name: &str) -> Option<&'static str> {
    Some(match name {
        "Average" => "reading-base-summary-average",
        "Min" => "reading-base-summary-min",
        "Max" => "reading-base-summary-max",
        "Sum" => "reading-base-summary-sum",
        "Range" => "reading-base-summary-range",
        "Median" => "reading-base-summary-median",
        "Stddev" => "reading-base-summary-stddev",
        "Earliest" => "reading-base-summary-earliest",
        "Latest" => "reading-base-summary-latest",
        "Checked" => "reading-base-summary-checked",
        "Unchecked" => "reading-base-summary-unchecked",
        "Empty" => "reading-base-summary-empty",
        "Filled" => "reading-base-summary-filled",
        "Unique" => "reading-base-summary-unique",
        _ => return None,
    })
}

fn base_cell_error(error: &focus_bases::EvalError) -> String {
    use focus_bases::EvalError as E;
    match error {
        E::Operator {
            operator,
            left,
            right,
        } => focus_i18n::t_args(
            "reading-base-error-operator",
            &[
                ("operator", (*operator).into()),
                ("left", (*left).into()),
                ("right", (*right).into()),
            ],
        ),
        E::Negate(on) => focus_i18n::t_args("reading-base-error-negate", &[("type", (*on).into())]),
        E::NoMethod { method, on } => focus_i18n::t_args(
            "reading-base-error-no-method",
            &[("type", (*on).into()), ("name", (*method).into())],
        ),
        E::NoField { field, on } => focus_i18n::t_args(
            "reading-base-error-no-field",
            &[("type", (*on).into()), ("name", field.as_str().into())],
        ),
        E::NoIndex { on, key } => focus_i18n::t_args(
            "reading-base-error-no-index",
            &[("type", (*on).into()), ("key", (*key).into())],
        ),
        E::Argument {
            function,
            position,
            expected,
            found,
        } => focus_i18n::t_args(
            "reading-base-error-argument",
            &[
                ("function", (*function).into()),
                ("position", (*position).into()),
                ("found", (*found).into()),
                ("expected", (*expected).into()),
            ],
        ),
        E::Parse { what, text } => focus_i18n::t_args(
            "reading-base-error-parse",
            &[("text", text.as_str().into()), ("type", (*what).into())],
        ),
        E::Format(token) => focus_i18n::t_args(
            "reading-base-error-format",
            &[("token", token.as_str().into())],
        ),
        E::DateRange => t("reading-base-error-date-range"),
        E::TooLarge { function } => focus_i18n::t_args(
            "reading-base-error-too-large",
            &[("function", (*function).into())],
        ),
        E::UnknownFormula(name) => focus_i18n::t_args(
            "reading-base-error-unknown-formula",
            &[("name", name.as_str().into())],
        ),
        E::UnknownType(name) => focus_i18n::t_args(
            "reading-base-error-unknown-type",
            &[("name", name.as_str().into())],
        ),
        E::TooDeep => t("reading-base-error-too-deep"),
    }
}

fn base_value(value: &focus_bases::Value) -> String {
    use focus_bases::Value;
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => t(if *b {
            "reading-base-true"
        } else {
            "reading-base-false"
        }),
        Value::Number(n) if n.fract() == 0. && n.abs() < 1e15 => {
            focus_i18n::format_number(*n as i64)
        }
        Value::Number(n) => focus_i18n::format_decimal(*n, 2),
        Value::Text(text) => text.clone(),
        Value::Date(time) => short_datetime(*time),
        Value::List(items) => items.iter().map(base_value).collect::<Vec<_>>().join(", "),
        Value::Map(entries) => entries
            .iter()
            .map(|(k, v)| format!("{k}: {}", base_value(v)))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn short_datetime(time: std::time::SystemTime) -> String {
    let secs = time
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as libc::time_t;
    // SAFETY: `tm` is plain data that `localtime_r` fills; both pointers are valid for the
    // call, which is reentrant.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    #[cfg(unix)]
    let failed = unsafe { libc::localtime_r(&secs, &mut tm) }.is_null()
        && unsafe { libc::gmtime_r(&secs, &mut tm) }.is_null();
    #[cfg(windows)]
    let failed = unsafe { libc::localtime_s(&mut tm, &secs) } != 0
        && unsafe { libc::gmtime_s(&mut tm, &secs) } != 0;
    if failed {
        return secs.to_string();
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
}

pub fn base_error_text(error: &focus_bases::BasesError) -> String {
    use focus_bases::BasesError as E;
    match error {
        E::Io { path, .. } | E::NotABase(path) => focus_i18n::t_args(
            "reading-base-unreadable",
            &[("path", path.to_string_lossy().as_ref().into())],
        ),
        E::TooLarge(bytes) => focus_i18n::t_args(
            "reading-base-too-large",
            &[("size", focus_i18n::format_bytes(*bytes).as_str().into())],
        ),
        E::Yaml(detail) | E::Invalid(detail) => focus_i18n::t_args(
            "reading-base-invalid",
            &[("detail", detail.as_str().into())],
        ),
        E::UnknownView(view) => focus_i18n::t_args(
            "reading-base-unknown-view",
            &[("view", view.as_str().into())],
        ),
        E::NoView => t("reading-base-no-view"),
        E::UnsupportedViewType(kind) => {
            focus_i18n::t_args("reading-base-view-type", &[("type", kind.as_str().into())])
        }
        E::Unsupported {
            expression,
            feature,
            subject,
        } => {
            use focus_bases::Feature as F;
            let key = match feature {
                F::Function => "reading-base-unsupported-function",
                F::Method => "reading-base-unsupported-method",
                F::Property => "reading-base-unsupported-property",
                F::Formula => "reading-base-unsupported-formula",
                F::This => "reading-base-unsupported-this",
                F::Index => "reading-base-unsupported-index",
                F::SortDirection => "reading-base-unsupported-sort-direction",
            };
            focus_i18n::t_args(
                key,
                &[
                    ("expression", expression.as_str().into()),
                    ("subject", subject.as_str().into()),
                ],
            )
        }
        E::Syntax {
            expression,
            message,
        } => focus_i18n::t_args(
            "reading-base-syntax",
            &[
                ("expression", expression.as_str().into()),
                ("detail", message.as_str().into()),
            ],
        ),
    }
}

pub fn build_base_page(path: &std::path::Path, library: &dyn Resolver) -> Built {
    let mut ctx = Ctx {
        library,
        note: None,
        note_dir: path.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
        footnote_labels: Vec::new(),
        languages: Vec::new(),
        path: Some(path.to_string_lossy().into_owned()),
        ord: 0,
        callout: 0,
        diagram_targets: std::collections::HashMap::new(),
        embedding: Vec::new(),
        host_links: None,
        text: None,
        origin: None,
    };
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut title = ctx.rich();
    push(&mut title, &stem, Marks::default());
    let crumbs = path
        .parent()
        .map(|p| p.iter().map(|s| s.to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    let header = Header {
        crumbs,
        title,
        props: Vec::new(),
        tags: Vec::new(),
    };
    let file = path.to_string_lossy().into_owned();
    let mut items = Vec::new();
    let mut headings = Vec::new();
    let mut section_end = Vec::new();
    let mut line = 0u32;
    let paragraph = |ctx: &mut Ctx, text: String| {
        let mut rich = ctx.rich();
        push(
            &mut rich,
            &text,
            Marks {
                muted: true,
                ..Marks::default()
            },
        );
        Node::Paragraph(rich)
    };
    match library.base_views(path, Some(path)) {
        None => items.push(Item {
            node: paragraph(
                &mut ctx,
                focus_i18n::t_args("reading-base-unreadable", &[("path", file.as_str().into())]),
            ),
            lines: 0..0,
        }),
        Some(Err(error)) => items.push(Item {
            node: paragraph(&mut ctx, base_error_text(&error)),
            lines: 0..0,
        }),
        Some(Ok(views)) => {
            for (i, (name, result)) in views.iter().enumerate() {
                ctx.ord = 0;
                let mut heading = ctx.rich();
                let name = name.clone().unwrap_or_else(|| {
                    focus_i18n::t_args("reading-base-view", &[("number", (i + 1).into())])
                });
                push(&mut heading, &name, Marks::default());
                let view_heading = headings.len();
                headings.push(Heading {
                    level: 2,
                    text: name.clone(),
                    line,
                    item: items.len(),
                    inner: false,
                });
                section_end.push(items.len());
                items.push(Item {
                    node: Node::Heading {
                        level: 2,
                        rich: heading,
                    },
                    lines: line..line,
                });
                ctx.ord = 0;
                match result {
                    Ok(table) if table.groups.iter().any(|group| group.key.is_some()) => {
                        let (caption, sections) = base_sections(&mut ctx, &file, table);
                        for (section, group) in sections.into_iter().zip(&table.groups) {
                            let rows = group.rows.len() as u32;
                            let title = section.title.unwrap_or_default();
                            headings.push(Heading {
                                level: 3,
                                text: without_isolates(&title.text),
                                line,
                                item: items.len(),
                                inner: false,
                            });
                            section_end.push(items.len() + 2);
                            items.push(Item {
                                node: Node::Heading {
                                    level: 3,
                                    rich: title,
                                },
                                lines: line..line,
                            });
                            items.push(Item {
                                node: Node::Base {
                                    caption: Rich::default(),
                                    sections: vec![BaseSection {
                                        title: None,
                                        table: section.table,
                                    }],
                                    view: Some(i as u32),
                                },
                                lines: line..line + rows,
                            });
                            line += rows;
                        }
                        items.push(Item {
                            node: Node::Base {
                                caption,
                                sections: Vec::new(),
                                view: Some(i as u32),
                            },
                            lines: line..line,
                        });
                    }
                    Ok(table) => {
                        let rows: usize = table.groups.iter().map(|group| group.rows.len()).sum();
                        let rows = rows as u32;
                        items.push(Item {
                            node: base_node(&mut ctx, &file, table),
                            lines: line..line + rows,
                        });
                        line += rows;
                    }
                    Err(error) => items.push(Item {
                        node: paragraph(&mut ctx, base_error_text(error)),
                        lines: line..line,
                    }),
                }
                section_end[view_heading] = items.len();
            }
        }
    }
    Built {
        note: None,
        diagram_targets: std::collections::HashMap::new(),
        header,
        items,
        headings,
        section_end,
        frontmatter_lines: None,
        title_lines: None,
        line_count: 0,
        source: None,
    }
}

pub fn build(document: &Document, meta: Option<&NoteMeta>, library: &dyn Resolver) -> Built {
    build_with_links(document, meta, library, None)
}

pub fn build_with_links(
    document: &Document,
    meta: Option<&NoteMeta>,
    library: &dyn Resolver,
    host_links: Option<&dyn crate::HostLinks>,
) -> Built {
    build_document(document, meta, library, host_links, None)
}

fn build_document(
    document: &Document,
    meta: Option<&NoteMeta>,
    library: &dyn Resolver,
    host_links: Option<&dyn crate::HostLinks>,
    text: Option<&str>,
) -> Built {
    let note_dir = meta
        .and_then(|m| m.path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();
    let mut ctx = Ctx {
        library,
        note: meta.map(|m| m.id),
        note_dir,
        footnote_labels: document.footnotes.iter().map(|f| f.label.clone()).collect(),
        languages: declared_languages(&document.blocks),
        path: meta.map(|m| m.path.to_string_lossy().into_owned()),
        ord: 0,
        callout: 0,
        diagram_targets: std::collections::HashMap::new(),
        embedding: meta.map(|m| vec![m.id]).unwrap_or_default(),
        host_links,
        text: None,
        origin: None,
    };
    ctx.text = text;
    let title_text = meta
        .map(|m| {
            text.and_then(|_| first_h1(document, m))
                .unwrap_or_else(|| m.title.to_string())
        })
        .unwrap_or_default();
    let mut title = ctx.rich();
    push(&mut title, &title_text, Marks::default());
    let crumbs = meta
        .and_then(|m| m.path.parent())
        .map(|p| p.iter().map(|s| s.to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    let current = text.map(|text| match frontmatter_len(text) {
        Some(end) => focus_core::parse_meta(&text[..end]).props,
        None => Vec::new(),
    });
    let (mut props, tags) = meta
        .map(|m| props_of(&mut ctx, current.as_deref().unwrap_or(&m.props)))
        .unwrap_or_default();
    if let Some(text) = text {
        map_props(&mut props, text);
    }
    let mut header = Header {
        crumbs,
        title,
        props,
        tags,
    };

    let mut items = Vec::new();
    let mut skipped_title = false;
    let mut title_lines = None;
    for block in &document.blocks {
        if !skipped_title
            && let BlockKind::Heading {
                level: 1,
                inlines: content,
                ..
            } = &block.kind
            && focus_md::plain_text(content).trim() == title_text.trim()
        {
            skipped_title = true;
            title_lines = Some(block.lines.clone());
            if text.is_some() {
                header.title = mapped_title(&mut ctx, block, header.title.ord);
            }
            continue;
        }
        ctx.ord = 0;
        ctx.origin = text.map(|text| line_start(text, block.bytes.start));
        let node = node(&mut ctx, block);
        items.push(Item {
            node,
            lines: block.lines.clone(),
        });
    }
    if !document.footnotes.is_empty() {
        ctx.ord = 0;
        let notes = document
            .footnotes
            .iter()
            .map(|f| {
                let lines = f.blocks.first().map_or(0, |b| b.lines.start)
                    ..f.blocks.iter().map(|b| b.lines.end).max().unwrap_or(0);
                FootnoteNode {
                    label: f.label.clone(),
                    blocks: blocks(&mut ctx, &f.blocks),
                    lines,
                }
            })
            .collect();
        items.push(Item {
            node: Node::Footnotes(notes),
            lines: document.line_count..document.line_count,
        });
    }
    let (headings, section_end) = index_headings(&items);
    Built {
        note: meta.map(|m| m.id),
        diagram_targets: std::mem::take(&mut ctx.diagram_targets),
        header,
        items,
        headings,
        section_end,
        frontmatter_lines: document.frontmatter.as_ref().map(|f| f.lines.clone()),
        title_lines,
        line_count: document.line_count,
        source: None,
    }
}

pub fn build_text(
    text: &str,
    meta: Option<&NoteMeta>,
    library: &dyn Resolver,
    host_links: Option<&dyn crate::HostLinks>,
) -> Built {
    let document = focus_md::parse(text, &focus_md::ParseOptions::default());
    Built {
        source: Some(text.into()),
        ..build_document(&document, meta, library, host_links, Some(text))
    }
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at.min(text.len())].rfind('\n').map_or(0, |i| i + 1)
}

fn line_offset(text: &str, line: u32) -> usize {
    if line == 0 {
        return 0;
    }
    text.match_indices('\n')
        .nth(line as usize - 1)
        .map_or(text.len(), |(i, _)| i + 1)
}

fn first_h1(document: &Document, meta: &NoteMeta) -> Option<String> {
    let file = meta.path.file_name()?.to_string_lossy().to_lowercase();
    document.blocks.iter().find_map(|block| match &block.kind {
        BlockKind::Heading {
            level: 1,
            inlines: content,
            ..
        } => {
            let text = focus_md::plain_text(content).trim().to_string();
            (!text.is_empty() && text.to_lowercase() != file).then_some(text)
        }
        _ => None,
    })
}

fn mapped_title(ctx: &mut Ctx, block: &Block, ord: u32) -> Rich {
    let BlockKind::Heading {
        inlines: content,
        marker,
        ..
    } = &block.kind
    else {
        return Rich {
            ord,
            ..Rich::default()
        };
    };
    ctx.origin = Some(0);
    let mut rich = Rich {
        ord,
        ..Rich::default()
    };
    inlines(ctx, &mut rich, content, Marks::default());
    map_heading(ctx, &mut rich, marker, &block.bytes);
    map_blanks(ctx, &mut rich);
    rich.marks.clear();
    rich.links.clear();
    rich
}

pub(crate) fn shift_lines(node: &mut Node, delta: i64) {
    let shift = |line: &mut u32| *line = (*line as i64 + delta).max(0) as u32;
    let shift_range = |lines: &mut Range<u32>| {
        lines.start = (lines.start as i64 + delta).max(0) as u32;
        lines.end = (lines.end as i64 + delta).max(0) as u32;
    };
    match node {
        Node::List { entries, .. } => {
            for entry in entries {
                shift(&mut entry.line);
                entry
                    .children
                    .iter_mut()
                    .for_each(|c| shift_lines(c, delta));
            }
        }
        Node::Code { line, .. } => shift(line),
        Node::Quote(children) | Node::Callout { children, .. } => {
            children.iter_mut().for_each(|c| shift_lines(c, delta))
        }
        Node::Details {
            children,
            child_lines,
            ..
        } => {
            children.iter_mut().for_each(|c| shift_lines(c, delta));
            child_lines.iter_mut().for_each(shift_range);
        }
        Node::Tabs { tabs, .. } => {
            for tab in tabs {
                shift(&mut tab.title_line);
                shift_range(&mut tab.lines);
                for item in &mut tab.items {
                    shift_range(&mut item.lines);
                    shift_lines(&mut item.node, delta);
                }
            }
        }
        Node::Footnotes(notes) => {
            for note in notes {
                shift_range(&mut note.lines);
                note.blocks.iter_mut().for_each(|b| shift_lines(b, delta));
            }
        }
        _ => {}
    }
}

fn keyed(node: &Node) -> u32 {
    match node {
        Node::Callout { children, .. } | Node::Details { children, .. } => {
            1 + children.iter().map(keyed).sum::<u32>()
        }
        Node::Tabs { tabs, .. } => {
            1 + tabs
                .iter()
                .flat_map(|t| t.items.iter())
                .map(|i| keyed(&i.node))
                .sum::<u32>()
        }
        Node::Quote(children) => children.iter().map(keyed).sum(),
        Node::List { entries, .. } => entries
            .iter()
            .flat_map(|e| e.children.iter())
            .map(keyed)
            .sum(),
        _ => 0,
    }
}

pub fn build_items(
    text: &str,
    lines: Range<u32>,
    first: usize,
    previous: &Built,
    meta: Option<&NoteMeta>,
    library: &dyn Resolver,
    host_links: Option<&dyn crate::HostLinks>,
) -> Vec<Item> {
    let start = line_offset(text, lines.start);
    let end = line_offset(text, lines.end).max(start);
    let fragment = &text[start..end];
    let document = focus_md::parse(fragment, &focus_md::ParseOptions::default());
    let footnote_labels = previous
        .items
        .iter()
        .find_map(|item| match &item.node {
            Node::Footnotes(notes) => Some(notes.iter().map(|n| n.label.clone()).collect()),
            _ => None,
        })
        .unwrap_or_default();
    let mut ctx = Ctx {
        library,
        note: meta.map(|m| m.id),
        note_dir: meta
            .and_then(|m| m.path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_default(),
        footnote_labels,
        languages: declared_languages(&document.blocks),
        path: meta.map(|m| m.path.to_string_lossy().into_owned()),
        ord: 0,
        callout: previous.items[..first.min(previous.items.len())]
            .iter()
            .map(|i| keyed(&i.node))
            .sum(),
        diagram_targets: std::collections::HashMap::new(),
        embedding: meta.map(|m| vec![m.id]).unwrap_or_default(),
        host_links,
        text: Some(fragment),
        origin: None,
    };
    document
        .blocks
        .iter()
        .map(|block| {
            ctx.ord = 0;
            ctx.origin = Some(line_start(fragment, block.bytes.start));
            let mut node = node(&mut ctx, block);
            shift_lines(&mut node, lines.start as i64);
            Item {
                node,
                lines: block.lines.start + lines.start..block.lines.end + lines.start,
            }
        })
        .collect()
}

impl Built {
    pub fn splice_items(&mut self, range: Range<usize>, items: Vec<Item>, delta: i64) {
        let added = items.len();
        let end = range.start + added;
        self.items.splice(range, items);
        for item in &mut self.items[end..] {
            item.lines.start = (item.lines.start as i64 + delta).max(0) as u32;
            item.lines.end = (item.lines.end as i64 + delta).max(0) as u32;
            shift_lines(&mut item.node, delta);
        }
        self.line_count = (self.line_count as i64 + delta).max(0) as u32;
        let (headings, section_end) = index_headings(&self.items);
        self.headings = headings;
        self.section_end = section_end;
    }
}

fn index_headings(items: &[Item]) -> (Vec<Heading>, Vec<usize>) {
    let mut headings = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if let Node::Heading { level, rich } = &item.node {
            headings.push(Heading {
                level: *level,
                text: without_isolates(&rich.text),
                line: item.lines.start,
                item: index,
                inner: false,
            });
        }
        tab_headings(&item.node, index, &mut headings);
    }
    let section_end = headings
        .iter()
        .enumerate()
        .map(|(i, h)| {
            if h.inner {
                h.item + 1
            } else {
                headings[i + 1..]
                    .iter()
                    .find(|n| !n.inner && n.level <= h.level)
                    .map_or(items.len(), |n| n.item)
            }
        })
        .collect();
    (headings, section_end)
}

fn tab_headings(node: &Node, item: usize, out: &mut Vec<Heading>) {
    let Node::Tabs { tabs, .. } = node else {
        return;
    };
    for inner in tabs.iter().flat_map(|tab| &tab.items) {
        if let Node::Heading { level, rich } = &inner.node {
            out.push(Heading {
                level: *level,
                text: without_isolates(&rich.text),
                line: inner.lines.start,
                item,
                inner: true,
            });
        }
        tab_headings(&inner.node, item, out);
    }
}

fn declared_languages(items: &[Block]) -> Vec<String> {
    let mut out = Vec::new();
    for block in items {
        match &block.kind {
            BlockKind::CodeBlock { lang, .. } if !lang.is_empty() && !out.contains(lang) => {
                out.push(lang.clone())
            }
            BlockKind::Quote(inner)
            | BlockKind::Callout { blocks: inner, .. }
            | BlockKind::Details { blocks: inner, .. } => {
                for lang in declared_languages(inner) {
                    if !out.contains(&lang) {
                        out.push(lang);
                    }
                }
            }
            BlockKind::List { items, .. } => {
                for item in items {
                    for lang in declared_languages(&item.blocks) {
                        if !out.contains(&lang) {
                            out.push(lang);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn props_of(ctx: &mut Ctx, values: &[(String, focus_core::PropValue)]) -> (Vec<Prop>, Vec<String>) {
    use focus_core::{PropValue, PropertyType};
    fn text(value: &PropValue) -> String {
        match value {
            PropValue::Null => String::new(),
            PropValue::Bool(b) => b.to_string(),
            PropValue::Number(n) => n.to_string(),
            PropValue::Text(t) => t.clone(),
            PropValue::List(items) => items
                .iter()
                .map(text)
                .collect::<Vec<_>>()
                .join(&focus_i18n::t("fmt-list-separator")),
            PropValue::Map(entries) => entries
                .iter()
                .map(|(k, v)| format!("{k}: {}", text(v)))
                .collect::<Vec<_>>()
                .join(&focus_i18n::t("fmt-list-separator")),
        }
    }
    let written = ctx.text.map(frontmatter_values).unwrap_or_default();
    let mut props = Vec::new();
    let mut tags = Vec::new();
    for (key, value) in values {
        if key == "tags" {
            let mut rich = ctx.rich();
            push(&mut rich, &text(value), Marks::default());
            let kind = ctx.library.property_type(key, value);
            props.push(Prop {
                key: key.clone(),
                value: rich,
                kind,
                fits: kind.kind.fits(value),
                checked: None,
            });
        }
        match (key.as_str(), value) {
            ("tags" | "tag", PropValue::List(items)) => {
                tags.extend(items.iter().map(text).filter(|t| !t.is_empty()))
            }
            ("tags" | "tag", value) => tags.extend(
                text(value)
                    .split([',', ' '])
                    .filter(|t| !t.is_empty())
                    .map(str::to_string),
            ),
            ("title", _) => {}
            _ => {
                let kind = ctx.library.property_type(key, value);
                let fits = kind.kind.fits(value);
                let mut rich = ctx.rich();
                let mono = Marks {
                    mono: true,
                    ..Marks::default()
                };
                let checked = match (kind.kind, value) {
                    (PropertyType::Checkbox, PropValue::Bool(b)) => Some(*b),
                    _ => None,
                };
                match (kind.kind, value) {
                    (PropertyType::Checkbox, _) if fits => {}
                    (PropertyType::Date | PropertyType::Datetime, PropValue::Text(t)) if fits => {
                        push(&mut rich, &shown_date(t), mono)
                    }
                    (PropertyType::Number, PropValue::Number(n)) => {
                        let shown = match (ctx.text, written.get(key.as_str()).map(Vec::as_slice)) {
                            (Some(source), Some([range])) => {
                                let line = &source[range.clone()];
                                line.split(" #").next().unwrap_or(line).trim().to_string()
                            }
                            _ => n.to_string(),
                        };
                        push(&mut rich, &shown, mono)
                    }
                    _ => linked_text(ctx, &mut rich, &text(value)),
                }
                props.push(Prop {
                    key: key.clone(),
                    value: rich,
                    kind,
                    fits,
                    checked,
                });
            }
        }
    }
    (props, tags)
}

fn shown_date(text: &str) -> String {
    match focus_core::parse_property_date(text, text.contains('T')) {
        Ok(date) if date.time.is_some() => focus_i18n::format_datetime_at(date.unix_seconds(), 0),
        Ok(date) => focus_i18n::format_date_at(date.unix_seconds(), 0),
        Err(_) => text.to_string(),
    }
}

fn frontmatter_len(text: &str) -> Option<usize> {
    let mut lines = text.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_start_matches('\u{feff}').trim_end() != "---" {
        return None;
    }
    let mut end = first.len();
    for line in lines {
        end += line.len();
        if matches!(line.trim_end(), "---" | "...") {
            return Some(end);
        }
    }
    None
}

fn frontmatter_values(text: &str) -> std::collections::HashMap<&str, Vec<Range<usize>>> {
    let mut out = std::collections::HashMap::new();
    let Some(end) = frontmatter_len(text) else {
        return out;
    };
    let unquoted = |range: Range<usize>| {
        let value = &text[range.clone()];
        let quoted = value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')));
        if quoted {
            range.start + 1..range.end - 1
        } else {
            range
        }
    };
    let mut list: Option<&str> = None;
    let mut fresh = false;
    let mut next = 0;
    for raw in text[..end].split_inclusive('\n') {
        let at = next;
        next += raw.len();
        if at == 0 {
            continue;
        }
        let row = raw.trim_end_matches(['\n', '\r']);
        let line = row.trim_end();
        let bare = line.trim_start();
        let empty = bare == "-";
        if let Some(key) = list
            && let Some(item) = bare.strip_prefix("- ").or(empty.then_some(""))
        {
            let from = if empty {
                at + (line.len() + 1).min(row.len())
            } else {
                at + line.len() - item.len()
            };
            let ranges = out.entry(key).or_insert_with(Vec::new);
            if std::mem::take(&mut fresh) {
                ranges.clear();
            }
            ranges.push(unquoted(from..from.max(at + line.len())));
            continue;
        }
        list = None;
        if line.starts_with([' ', '\t', '#']) {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if value.trim().is_empty() {
            list = Some(key.trim());
            fresh = true;
            let end = at + raw.trim_end_matches(['\n', '\r']).len();
            let empty = end..end;
            out.insert(key.trim(), vec![empty]);
            continue;
        }
        let from = at + key.len() + 1 + (value.len() - value.trim_start().len());
        out.insert(key.trim(), vec![unquoted(from..at + line.len())]);
    }
    out
}

fn map_props(props: &mut [Prop], text: &str) {
    use crate::srcmap::{Markup, Piece, SourceMap};
    let values = frontmatter_values(text);
    let separator = focus_i18n::t("fmt-list-separator");
    for prop in props {
        let Some(ranges) = values.get(prop.key.as_str()) else {
            continue;
        };
        if prop.value.text.is_empty() {
            if let [range] = ranges.as_slice()
                && range.is_empty()
                && !prop.is_checkbox()
            {
                prop.value.map = Some(Arc::new(SourceMap {
                    pieces: vec![Piece {
                        shown: 0..0,
                        source: range.clone(),
                        exact: true,
                    }],
                    markup: Vec::new(),
                }));
            }
            continue;
        }
        let field = prop.is_field() && prop.kind.kind != focus_core::PropertyType::Number;
        let rich = &mut prop.value;
        if field {
            if let [range] = ranges.as_slice()
                && !rich.text.is_empty()
            {
                let shown = 0..rich.text.len();
                rich.map = Some(Arc::new(SourceMap {
                    pieces: vec![Piece {
                        shown: shown.clone(),
                        source: range.clone(),
                        exact: false,
                    }],
                    markup: vec![Markup {
                        element: range.clone(),
                        inserts: vec![(0, range.clone())],
                        hides: Some(shown),
                        line: false,
                        value: true,
                    }],
                }));
            }
            continue;
        }
        let mut map = SourceMap::default();
        let mut shown = 0;
        let mut matches = true;
        for (index, range) in ranges.iter().enumerate() {
            if index > 0 {
                let between = shown..shown + separator.len();
                matches &= rich.text.get(between.clone()) == Some(separator.as_str());
                map.pieces.push(Piece {
                    shown: between.clone(),
                    source: range.start..range.start,
                    exact: false,
                });
                shown = between.end;
            }
            let value = &text[range.clone()];
            matches &= rich.text.get(shown..shown + value.len()) == Some(value);
            map.pieces.push(Piece {
                shown: shown..shown + value.len(),
                source: range.clone(),
                exact: true,
            });
            shown += value.len();
        }
        if matches && shown == rich.text.len() && !rich.text.is_empty() {
            rich.map = Some(Arc::new(map));
        }
    }
}

fn linked_text(ctx: &Ctx, rich: &mut Rich, text: &str) {
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        let Some(close) = rest[open..].find("]]").map(|c| open + c) else {
            break;
        };
        push(rich, &rest[..open], Marks::default());
        let inner = &rest[open + 2..close];
        let (target, alias) = inner
            .split_once('|')
            .map_or((inner, None), |(t, a)| (t, Some(a)));
        let (path, anchor) = target
            .split_once('#')
            .map_or((target, None), |(p, a)| (p, Some(a.to_string())));
        let goal = ctx.resolve(LinkKind::Wiki, path.trim(), anchor.as_ref());
        let index = rich.links.len() as u16;
        let start = rich.text.len();
        push(
            rich,
            alias.unwrap_or(target).trim(),
            Marks {
                link: Some(index),
                ..Marks::default()
            },
        );
        rich.links.push(LinkSpan {
            range: start..rich.text.len(),
            goal,
        });
        rest = &rest[close + 2..];
    }
    push(rich, rest, Marks::default());
}

pub fn texts_of(node: &Node, out: &mut Vec<(u32, String)>) {
    let add = |rich: &Rich, out: &mut Vec<(u32, String)>| out.push((rich.ord, rich.text.clone()));
    match node {
        Node::Heading { rich, .. } | Node::Paragraph(rich) | Node::Math(rich) => add(rich, out),
        Node::Code { code, .. } => add(code, out),
        Node::List { entries, .. } => entries
            .iter()
            .flat_map(|e| e.children.iter())
            .for_each(|n| texts_of(n, out)),
        Node::Quote(children) => children.iter().for_each(|n| texts_of(n, out)),
        Node::Callout {
            title: heading,
            children,
            ..
        }
        | Node::Details {
            summary: heading,
            children,
            ..
        } => {
            add(heading, out);
            children.iter().for_each(|n| texts_of(n, out));
        }
        Node::Table { head, rows, .. } => {
            head.iter().for_each(|r| add(r, out));
            rows.iter().flatten().for_each(|r| add(r, out));
        }
        Node::Tabs { tabs, .. } => tabs
            .iter()
            .flat_map(|tab| &tab.items)
            .for_each(|item| texts_of(&item.node, out)),
        Node::Ref(rich) => add(rich, out),
        Node::Base {
            caption, sections, ..
        } => {
            for section in sections {
                section.title.iter().for_each(|title| add(title, out));
                texts_of(&section.table, out);
            }
            if !caption.text.is_empty() {
                add(caption, out);
            }
        }
        Node::Directive(_) | Node::Query(_) => {}
        Node::Embed { label, body, .. } => {
            add(label, out);
            if let EmbedBody::Content(nodes) = body {
                nodes.iter().for_each(|n| texts_of(n, out));
            }
        }
        Node::Html(parts) => parts.iter().for_each(|p| {
            if let HtmlPart::Text(r) = p {
                add(r, out)
            }
        }),
        Node::Footnotes(notes) => notes
            .iter()
            .flat_map(|n| n.blocks.iter())
            .for_each(|n| texts_of(n, out)),
        Node::Mermaid { .. } | Node::Image(_) | Node::Rule => {}
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use focus_core::{LinkTarget, NoteId};

    use super::*;
    use crate::Resolver;

    struct Known;

    impl Resolver for Known {
        fn resolve_link(&self, _: NoteId, _: &str) -> LinkTarget {
            LinkTarget::Unresolved
        }
        fn absolute(&self, relative: &Path) -> PathBuf {
            relative.to_path_buf()
        }
        fn known_file(&self, target: &str) -> Option<PathBuf> {
            (target == "/engagement/Hosts/dc01.md").then(|| PathBuf::from(target))
        }
    }

    #[test]
    fn headings_in_tabs_are_listed_and_footnote_definitions_know_their_lines() {
        let text = "## Avant\n\n````tabs\ntab: Linux\n\nTexte.\n\ntab: Windows\n\n### Dans Windows\n\nTexte.\n````\n\n## Apres\n\nTexte[^n].\n\n[^n]: La note.\n";
        let built = build_text(text, None, &Known, None);
        let listed: Vec<(&str, u32, bool)> = built
            .headings
            .iter()
            .map(|h| (h.text.as_str(), h.line, h.inner))
            .collect();
        assert_eq!(
            listed,
            [
                ("Avant", 0, false),
                ("Dans Windows", 9, true),
                ("Apres", 14, false)
            ]
        );
        assert_eq!(built.section_end[0], built.headings[2].item);
        assert_eq!(built.source.as_deref(), Some(text));
        let Some(Node::Footnotes(notes)) = built.items.last().map(|i| &i.node) else {
            panic!("no footnotes")
        };
        assert_eq!(notes[0].lines, 18..19);
    }

    #[test]
    fn inline_code_is_isolated_and_copied_without_the_isolates() {
        let document = focus_md::parse("نص `a-1:[2]` نص", &focus_md::ParseOptions::default());
        let built = build(&document, None, &Known);
        let Node::Paragraph(rich) = &built.items[0].node else {
            panic!("a paragraph")
        };
        assert_eq!(rich.text, "نص \u{2066}a-1:[2]\u{2069} نص");
        let (range, marks) = rich
            .marks
            .iter()
            .find(|(_, m)| m.code)
            .expect("a code span");
        assert_eq!(&rich.text[range.clone()], "\u{2066}a-1:[2]\u{2069}");
        assert!(marks.code);
        assert_eq!(without_isolates(&rich.text), "نص a-1:[2] نص");
    }

    #[test]
    fn an_arabic_tag_is_isolated_right_to_left() {
        let document = focus_md::parse(
            "استخدم الوسم #مراجعة هنا",
            &focus_md::ParseOptions::default(),
        );
        let built = build(&document, None, &Known);
        let Node::Paragraph(rich) = &built.items[0].node else {
            panic!("a paragraph")
        };
        assert_eq!(rich.text, "استخدم الوسم \u{2067}#مراجعة\u{2069} هنا");
        assert_eq!(without_isolates(&rich.text), "استخدم الوسم #مراجعة هنا");
    }

    #[test]
    fn a_tag_is_isolated_left_to_right_and_stays_a_link() {
        let document = focus_md::parse(
            "الوسم #red-team في النص",
            &focus_md::ParseOptions::default(),
        );
        let built = build(&document, None, &Known);
        let Node::Paragraph(rich) = &built.items[0].node else {
            panic!("a paragraph")
        };
        assert_eq!(rich.text, "الوسم \u{2066}#red-team\u{2069} في النص");
        assert!(matches!(&rich.links[0].goal, LinkGoal::Tag(t) if t == "red-team"));
        assert_eq!(
            &rich.text[rich.links[0].range.clone()],
            "\u{2066}#red-team\u{2069}"
        );
    }

    struct Hosts;

    impl crate::HostLinks for Hosts {
        fn find(&self, text: &str) -> Vec<(Range<usize>, LinkGoal)> {
            text.match_indices("10.10.10.5")
                .map(|(at, m)| {
                    (
                        at..at + m.len(),
                        LinkGoal::Note {
                            id: 3,
                            anchor: None,
                        },
                    )
                })
                .collect()
        }
    }

    #[test]
    fn host_links_are_drawn_as_links_outside_code_and_links() {
        let document = focus_md::parse(
            "scan 10.10.10.5 then `10.10.10.5` and [10.10.10.5](x)",
            &focus_md::ParseOptions::default(),
        );
        let built = build_with_links(&document, None, &Known, Some(&Hosts));
        let Node::Paragraph(rich) = &built.items[0].node else {
            panic!("a paragraph")
        };
        let hosts: Vec<&str> = rich
            .links
            .iter()
            .filter(|l| matches!(l.goal, LinkGoal::Note { id: 3, .. }))
            .map(|l| &rich.text[l.range.clone()])
            .collect();
        assert_eq!(hosts, ["10.10.10.5"]);
        assert_eq!(rich.links.len(), 2);
        assert!(rich.text.starts_with("scan 10.10.10.5 then"));
    }

    struct Bases;

    fn base_column(id: &str, label: &str, link: bool) -> focus_bases::Column {
        focus_bases::Column {
            id: id.into(),
            label: label.into(),
            named: !link,
            link,
        }
    }

    fn base_text(text: &str) -> focus_bases::Drawn {
        Ok(focus_bases::Value::Text(text.into()))
    }

    fn base_summary(
        name: &str,
        built_in: bool,
        value: focus_bases::Drawn,
    ) -> focus_bases::TableSummary {
        focus_bases::TableSummary {
            name: name.into(),
            built_in,
            value,
        }
    }

    fn grouped_base() -> focus_bases::Table {
        use focus_bases::{EvalError, Table, TableGroup, TableRow, Value};
        let row =
            |note, name: &str, status: focus_bases::Drawn, hours: focus_bases::Drawn| TableRow {
                note,
                cells: vec![base_text(name), status, hours],
            };
        let failed = Err(EvalError::Operator {
            operator: "*",
            left: "string",
            right: "number",
        });
        let summaries = |count: f64, sum: focus_bases::Drawn| {
            vec![
                Some(base_summary("Count", false, Ok(Value::Number(count)))),
                None,
                Some(base_summary("Sum", true, sum)),
            ]
        };
        Table {
            view: Some("Board".into()),
            columns: vec![
                base_column("file.name", "name", true),
                base_column("note.status", "Status", false),
                base_column("formula.hours", "Hours", false),
            ],
            groups: vec![
                TableGroup {
                    key: Some(Value::Text("draft".into())),
                    rows: vec![
                        row(4, "Alpha", base_text("draft"), Ok(Value::Number(2.0))),
                        row(5, "Beta", base_text("draft"), failed.clone()),
                    ],
                    summaries: summaries(2.0, failed),
                },
                TableGroup {
                    key: Some(Value::Null),
                    rows: vec![row(6, "Gamma", Ok(Value::Null), Ok(Value::Number(1.5)))],
                    summaries: summaries(1.0, Ok(Value::Number(1.5))),
                },
                TableGroup {
                    key: Some(Value::Text("waiting".into())),
                    rows: Vec::new(),
                    summaries: summaries(0.0, Ok(Value::Null)),
                },
            ],
            matched: 9,
        }
    }

    impl Resolver for Bases {
        fn resolve_link(&self, _: NoteId, target: &str) -> LinkTarget {
            LinkTarget::Attachment(PathBuf::from(target))
        }
        fn resolve_from_root(&self, target: &str) -> LinkTarget {
            LinkTarget::Attachment(PathBuf::from(target))
        }
        fn absolute(&self, relative: &Path) -> PathBuf {
            relative.to_path_buf()
        }
        fn base_table(
            &self,
            path: &Path,
            view: Option<&str>,
            _: Option<&Path>,
        ) -> Option<Result<focus_bases::Table, focus_bases::BasesError>> {
            use focus_bases::{Table, TableGroup, TableRow, Value};
            if path == Path::new("Board.base") {
                return Some(Ok(grouped_base()));
            }
            if path != Path::new("Ops.base") {
                return Some(Err(focus_bases::BasesError::Unsupported {
                    expression: "file.links".into(),
                    feature: focus_bases::Feature::Property,
                    subject: "file.links".into(),
                }));
            }
            Some(Ok(Table {
                view: view.map(str::to_string),
                columns: vec![
                    base_column("file.name", "name", true),
                    base_column("file.mtime", "Modifiée", false),
                ],
                groups: vec![TableGroup {
                    key: None,
                    rows: vec![TableRow {
                        note: 4,
                        cells: vec![
                            base_text("Kerberoasting"),
                            Ok(Value::Date(std::time::UNIX_EPOCH)),
                        ],
                    }],
                    summaries: Vec::new(),
                }],
                matched: 3,
            }))
        }
        fn base_views(
            &self,
            _: &Path,
            _: Option<&Path>,
        ) -> Option<Result<crate::BaseViews, focus_bases::BasesError>> {
            let plain = self.base_table(Path::new("Ops.base"), Some("Ops"), None)?;
            Some(Ok(vec![
                (Some("Board".into()), Ok(grouped_base())),
                (Some("Ops".into()), plain),
                (
                    Some("Cards".into()),
                    Err(focus_bases::BasesError::UnsupportedViewType("cards".into())),
                ),
                (
                    None,
                    Err(focus_bases::BasesError::UnsupportedViewType("list".into())),
                ),
            ]))
        }
    }

    fn row_texts(row: &[Rich]) -> Vec<String> {
        row.iter()
            .map(|cell| without_isolates(&cell.text))
            .collect()
    }

    fn all_muted(rich: &Rich) -> bool {
        !rich.marks.is_empty() && rich.marks.iter().all(|(_, marks)| marks.muted)
    }

    type Section<'a> = (Option<&'a Rich>, &'a [Rich], &'a [Vec<Rich>]);

    fn base_parts(node: &Node) -> (&Rich, Vec<Section<'_>>) {
        let Node::Base {
            caption, sections, ..
        } = node
        else {
            panic!("a base: {node:?}")
        };
        let sections = sections
            .iter()
            .map(|section| {
                let Node::Table { head, rows, .. } = &section.table else {
                    panic!("a table")
                };
                (section.title.as_ref(), &head[..], &rows[..])
            })
            .collect();
        (caption, sections)
    }

    #[test]
    fn a_base_embed_is_its_table_and_a_canvas_says_it_is_not_supported() {
        let document = focus_md::parse(
            "![[Ops.base#A traiter]]\n\n![[Plan.canvas]]\n\n![[Autre.base]]\n",
            &focus_md::ParseOptions::default(),
        );
        let built = build(&document, None, &Bases);
        let (caption, sections) = base_parts(&built.items[0].node);
        assert_eq!(without_isolates(&caption.text), "A traiter  1 of 3");
        assert_eq!(sections.len(), 1);
        let (title, head, rows) = sections[0];
        assert!(title.is_none());
        assert_eq!(rows.len(), 1);
        assert_eq!(
            head.iter().map(|h| h.text.as_str()).collect::<Vec<_>>(),
            ["Name", "Modifiée"]
        );
        assert!(matches!(
            rows[0][0].links[0].goal,
            LinkGoal::Note { id: 4, .. }
        ));
        assert!(rows[0][1].marks.iter().all(|(_, m)| m.mono));
        let date = rows[0][1].text.as_bytes();
        assert_eq!(date.len(), 16, "{}", rows[0][1].text);
        assert!(date.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b' ',
            13 => *c == b':',
            _ => c.is_ascii_digit(),
        }));
        assert!(matches!(
            &built.items[1].node,
            Node::Embed {
                body: EmbedBody::Failed(_),
                ..
            }
        ));
        assert!(matches!(
            &built.items[2].node,
            Node::Embed {
                body: EmbedBody::Failed(_),
                ..
            }
        ));
    }

    #[test]
    fn a_grouped_base_is_a_table_per_group_with_its_summaries() {
        let document = focus_md::parse("![[Board.base]]\n", &focus_md::ParseOptions::default());
        let built = build(&document, None, &Bases);
        let (caption, sections) = base_parts(&built.items[0].node);
        assert_eq!(without_isolates(&caption.text), "Board  3 of 9");
        let titles: Vec<_> = sections
            .iter()
            .map(|(title, _, _)| title.expect("a title").text.as_str())
            .collect();
        assert_eq!(titles, ["draft", "No value", "waiting"]);
        assert!(!all_muted(sections[0].0.unwrap()));
        assert!(all_muted(sections[1].0.unwrap()));
        for (_, head, _) in &sections {
            assert_eq!(row_texts(head), ["Name", "Status", "Hours"]);
        }
        let (_, _, draft) = sections[0];
        assert_eq!(draft.len(), 3);
        assert_eq!(
            row_texts(&draft[0]),
            ["Count 2", "", "Sum * does not apply to string and number"]
        );
        assert_eq!(draft[0][0].text, format!("Count {LRI}2{PDI}"));
        assert!(draft[0][0].links.is_empty());
        let (label, marks) = &draft[0][0].marks[0];
        assert_eq!(
            (&draft[0][0].text[label.clone()], marks.muted),
            ("Count ", true)
        );
        assert!(!draft[0][0].marks.last().unwrap().1.muted);
        assert!(all_muted(&draft[0][2]));
        assert_eq!(row_texts(&draft[1]), ["Alpha", "draft", "2"]);
        assert_eq!(
            row_texts(&draft[2]),
            ["Beta", "draft", "* does not apply to string and number"]
        );
        assert!(all_muted(&draft[2][2]));
        assert!(matches!(
            draft[2][0].links[0].goal,
            LinkGoal::Note { id: 5, .. }
        ));
        let (_, _, none) = sections[1];
        assert_eq!(row_texts(&none[0]), ["Count 1", "", "Sum 1.50"]);
        assert_eq!(row_texts(&none[1]), ["Gamma", "", "1.50"]);
        let (_, _, waiting) = sections[2];
        assert_eq!(waiting.len(), 1);
        assert_eq!(row_texts(&waiting[0]), ["Count 0", "", "Sum"]);
        let mut texts = Vec::new();
        texts_of(&built.items[0].node, &mut texts);
        let mut ords: Vec<u32> = texts.iter().map(|(ord, _)| *ord).collect();
        let count = ords.len();
        ords.sort_unstable();
        ords.dedup();
        assert_eq!(ords.len(), count);
        assert_eq!(count, 1 + 3 + 3 * 3 + (3 + 2 + 1) * 3);
    }

    #[test]
    fn every_documented_summary_has_a_label() {
        use focus_bases::{Table, TableGroup, Value};
        let names = [
            ("Average", "Average"),
            ("Min", "Min"),
            ("Max", "Max"),
            ("Sum", "Sum"),
            ("Range", "Range"),
            ("Median", "Median"),
            ("Stddev", "Standard deviation"),
            ("Earliest", "Earliest"),
            ("Latest", "Latest"),
            ("Checked", "Checked"),
            ("Unchecked", "Unchecked"),
            ("Empty", "Empty"),
            ("Filled", "Filled"),
            ("Unique", "Unique"),
        ];
        let table = Table {
            view: None,
            columns: names
                .iter()
                .map(|(name, _)| base_column(&format!("note.{name}"), name, false))
                .chain([base_column("note.own", "own", false)])
                .collect(),
            groups: vec![TableGroup {
                key: None,
                rows: Vec::new(),
                summaries: names
                    .iter()
                    .map(|(name, _)| Some(base_summary(name, true, Ok(Value::Number(3.0)))))
                    .chain([Some(base_summary("Stddev", false, Ok(Value::Bool(true))))])
                    .collect(),
            }],
            matched: 0,
        };
        let mut ctx = Ctx {
            library: &Bases,
            note: None,
            note_dir: PathBuf::new(),
            footnote_labels: Vec::new(),
            languages: Vec::new(),
            path: None,
            ord: 0,
            callout: 0,
            diagram_targets: std::collections::HashMap::new(),
            embedding: Vec::new(),
            host_links: None,
            text: None,
            origin: None,
        };
        let node = base_node(&mut ctx, "Sums.base", &table);
        let (caption, sections) = base_parts(&node);
        assert_eq!(caption.text, "Sums");
        let (_, _, rows) = sections[0];
        let expected: Vec<String> = names
            .iter()
            .map(|(_, label)| format!("{label} 3"))
            .chain(["Stddev yes".to_string()])
            .collect();
        assert_eq!(row_texts(&rows[0]), expected);
        for (name, _) in names {
            let key = base_summary_key(name).expect(name);
            assert_ne!(t(key), key);
        }
        assert_eq!(base_summary_key("Count"), None);
    }

    #[test]
    fn the_summary_row_of_a_view_without_groups_is_its_last_row() {
        use focus_bases::{Table, TableGroup, TableRow, Value};
        let table = Table {
            view: Some("Hours".into()),
            columns: vec![
                base_column("file.name", "name", true),
                base_column("note.hours", "Hours", false),
            ],
            groups: vec![TableGroup {
                key: None,
                rows: [(4, "Alpha", 2.0), (5, "Beta", 3.0)]
                    .into_iter()
                    .map(|(note, name, hours)| TableRow {
                        note,
                        cells: vec![base_text(name), Ok(Value::Number(hours))],
                    })
                    .collect(),
                summaries: vec![
                    None,
                    Some(base_summary("Sum", true, Ok(Value::Number(5.0)))),
                ],
            }],
            matched: 2,
        };
        let mut ctx = Ctx {
            library: &Bases,
            note: None,
            note_dir: PathBuf::new(),
            footnote_labels: Vec::new(),
            languages: Vec::new(),
            path: None,
            ord: 0,
            callout: 0,
            diagram_targets: std::collections::HashMap::new(),
            embedding: Vec::new(),
            host_links: None,
            text: None,
            origin: None,
        };
        let node = base_node(&mut ctx, "Hours.base", &table);
        let (_, sections) = base_parts(&node);
        let (title, _, rows) = sections[0];
        assert!(title.is_none());
        let texts: Vec<_> = rows.iter().map(|row| row_texts(row)).collect();
        assert_eq!(texts, [["Alpha", "2"], ["Beta", "3"], ["", "Sum 5"]]);
    }

    #[test]
    fn the_page_of_a_base_makes_headings_of_its_views_and_of_their_groups() {
        let built = build_base_page(Path::new("Work/Board.base"), &Bases);
        let outline: Vec<_> = built
            .headings
            .iter()
            .map(|h| (h.level, h.text.as_str(), h.item))
            .collect();
        assert_eq!(
            outline,
            [
                (2, "Board", 0),
                (3, "draft", 1),
                (3, "No value", 3),
                (3, "waiting", 5),
                (2, "Ops", 8),
                (2, "Cards", 10),
                (2, "View 4", 12),
            ]
        );
        assert_eq!(built.items.len(), 14);
        assert_eq!(built.section_end, [8, 3, 5, 7, 10, 12, 14]);
        for (item, first) in [(2, "Count 2"), (4, "Count 1"), (6, "Count 0")] {
            let Node::Base {
                caption,
                sections,
                view,
            } = &built.items[item].node
            else {
                panic!("a group table: {:?}", built.items[item].node)
            };
            assert_eq!((caption.text.as_str(), *view), ("", Some(0)));
            assert_eq!(sections.len(), 1);
            assert!(sections[0].title.is_none());
            let (_, head, rows) = base_parts(&built.items[item].node).1[0];
            assert_eq!(row_texts(head), ["Name", "Status", "Hours"]);
            assert_eq!(row_texts(&rows[0])[0], first);
        }
        let Node::Heading { level: 3, rich } = &built.items[3].node else {
            panic!("a group title")
        };
        assert!(all_muted(rich));
        let Node::Base {
            caption,
            sections,
            view,
        } = &built.items[7].node
        else {
            panic!("the caption")
        };
        assert_eq!(
            (
                without_isolates(&caption.text).as_str(),
                sections.len(),
                *view
            ),
            ("Board  3 of 9", 0, Some(0))
        );
        let (caption, sections) = base_parts(&built.items[9].node);
        assert_eq!(without_isolates(&caption.text), "Ops  1 of 3");
        assert_eq!(sections.len(), 1);
        assert!(matches!(built.items[9].node, Node::Base { view: None, .. }));
        let lines: Vec<_> = built.items.iter().map(|item| item.lines.clone()).collect();
        assert_eq!(
            lines,
            [
                0..0,
                0..0,
                0..2,
                2..2,
                2..3,
                3..3,
                3..3,
                3..3,
                3..3,
                3..4,
                4..4,
                4..4,
                4..4,
                4..4,
            ]
        );
        for (item, text) in [
            (11, "cards views are not supported, only tables."),
            (13, "list views are not supported, only tables."),
        ] {
            let Node::Paragraph(rich) = &built.items[item].node else {
                panic!("a line: {:?}", built.items[item].node)
            };
            assert_eq!(rich.text, text);
            assert!(all_muted(rich));
        }
    }

    #[derive(Default)]
    struct BaseHosts(std::sync::Mutex<Vec<(PathBuf, Option<PathBuf>)>>);

    impl Resolver for BaseHosts {
        fn resolve_link(&self, _: NoteId, target: &str) -> LinkTarget {
            match target {
                "Inner" => LinkTarget::Note {
                    id: 2,
                    anchor: None,
                },
                _ => LinkTarget::Attachment(PathBuf::from(target)),
            }
        }
        fn resolve_from_root(&self, target: &str) -> LinkTarget {
            LinkTarget::Attachment(PathBuf::from(target))
        }
        fn absolute(&self, relative: &Path) -> PathBuf {
            relative.to_path_buf()
        }
        fn note_text(&self, note: NoteId) -> Option<Result<(String, Arc<NoteMeta>), String>> {
            (note == 2).then(|| {
                Ok((
                    "![[Inner.base]]\n".to_string(),
                    Arc::new(host_meta(2, "Deep/Inner.md")),
                ))
            })
        }
        fn base_table(
            &self,
            path: &Path,
            _: Option<&str>,
            this: Option<&Path>,
        ) -> Option<Result<focus_bases::Table, focus_bases::BasesError>> {
            self.0
                .lock()
                .unwrap()
                .push((path.to_path_buf(), this.map(Path::to_path_buf)));
            Some(Err(focus_bases::BasesError::NoView))
        }
        fn base_views(
            &self,
            path: &Path,
            this: Option<&Path>,
        ) -> Option<Result<crate::BaseViews, focus_bases::BasesError>> {
            self.0
                .lock()
                .unwrap()
                .push((path.to_path_buf(), this.map(Path::to_path_buf)));
            Some(Ok(Vec::new()))
        }
    }

    fn host_meta(id: NoteId, path: &str) -> NoteMeta {
        NoteMeta {
            id,
            path: Arc::from(Path::new(path)),
            title: Arc::from("Host"),
            aliases: Vec::new(),
            tags: Vec::new(),
            headings: Vec::new(),
            blocks: Vec::new(),
            props: Vec::new(),
            links: Vec::new(),
            words: 0,
            tasks_open: 0,
            tasks_done: 0,
            size: 0,
            modified: std::time::SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn this_is_the_base_on_its_page_and_the_note_that_holds_an_embed() {
        let hosts = BaseHosts::default();
        let document = focus_md::parse(
            "![[Work.base#Board]]\n\n![[Inner]]\n",
            &focus_md::ParseOptions::default(),
        );
        build(&document, Some(&host_meta(1, "Notes/Host.md")), &hosts);
        build(&document, None, &hosts);
        build_base_page(Path::new("Work/Board.base"), &hosts);
        let seen = hosts.0.lock().unwrap().clone();
        let of = |base: &str, this: Option<&str>| (PathBuf::from(base), this.map(PathBuf::from));
        assert_eq!(
            seen,
            [
                of("Work.base", Some("Notes/Host.md")),
                of("Inner.base", Some("Deep/Inner.md")),
                of("Work.base", None),
                of("Work/Board.base", Some("Work/Board.base")),
            ]
        );
    }

    #[test]
    fn every_error_of_a_cell_has_its_text() {
        use focus_bases::EvalError as E;
        let texts = [
            (
                E::Negate("string"),
                "The minus sign does not apply to string",
            ),
            (
                E::NoMethod {
                    method: "lower",
                    on: "number",
                },
                "number has no function lower",
            ),
            (
                E::NoField {
                    field: "year".into(),
                    on: "string",
                },
                "string has no field year",
            ),
            (
                E::NoIndex {
                    on: "number",
                    key: "string",
                },
                "number cannot be read by string",
            ),
            (
                E::Argument {
                    function: "repeat",
                    position: 1,
                    expected: "number",
                    found: "string",
                },
                "repeat: argument 1 is string, not number",
            ),
            (
                E::Parse {
                    what: "date",
                    text: "soon".into(),
                },
                "soon is not a date",
            ),
            (
                E::Format("LL".into()),
                "The date format LL depends on the language and is not supported",
            ),
            (E::DateRange, "The date is out of range"),
            (
                E::TooLarge { function: "repeat" },
                "The result of repeat is too large",
            ),
            (E::UnknownFormula("age".into()), "No formula named age"),
            (E::UnknownType("thing".into()), "Unknown type thing"),
            (E::TooDeep, "The expression is nested too deeply"),
        ];
        for (error, text) in texts {
            assert_eq!(without_isolates(&base_cell_error(&error)), text);
        }
    }

    fn assert_exact(rich: &Rich, text: &str, origin: usize) {
        let map = rich.map.as_ref().expect("a source map");
        for piece in map.pieces.iter().filter(|p| p.exact) {
            assert_eq!(
                &rich.text[piece.shown.clone()],
                &text[origin + piece.source.start..origin + piece.source.end],
                "{piece:?}"
            );
        }
    }

    #[test]
    fn texts_built_from_their_note_map_back_to_its_bytes() {
        let text = "# Title\n\nSome **bold** and a [[Note|alias]], `code` and #tag[^1].\nSecond line.\n\n## Part *two*\n\n[^1]: Note.\n";
        let built = build_text(text, None, &Known, None);
        let Node::Paragraph(rich) = &built.items[1].node else {
            panic!("a paragraph second")
        };
        let origin = text.find("Some").expect("the paragraph");
        assert_exact(rich, text, origin);
        let map = rich.map.as_ref().expect("a source map");
        let bold = rich.text.find("bold").expect("bold");
        assert_eq!(
            map.to_source(bold),
            Some(text.find("bold").unwrap() - origin)
        );
        let (shown, _) = crate::srcmap::reveal(
            rich,
            map,
            text.find("old").unwrap() - origin,
            &text[origin..],
        )
        .expect("the caret in the bold run");
        assert!(shown.text.contains("**bold**"), "{}", shown.text);
        let (shown, _) = crate::srcmap::reveal(
            rich,
            map,
            text.find("alias").unwrap() + 2 - origin,
            &text[origin..],
        )
        .expect("the caret in the link");
        assert!(shown.text.contains("[[Note|alias]]"), "{}", shown.text);
        let second = rich.text.find("Second").expect("second line");
        assert_eq!(
            map.to_source(second),
            Some(text.find("Second").unwrap() - origin)
        );
        let Node::Heading { rich: heading, .. } = &built.items[2].node else {
            panic!("a heading third")
        };
        let origin = text.find("## Part").expect("the heading");
        assert_exact(heading, text, origin);
        let (shown, _) =
            crate::srcmap::reveal(heading, heading.map.as_ref().unwrap(), 4, &text[origin..])
                .expect("the caret on the heading line");
        assert!(shown.text.starts_with("## Part"), "{}", shown.text);
    }

    #[test]
    fn blanks_typed_at_the_end_of_a_text_take_the_caret() {
        let text =
            "# Title \n\nSome **bold** \n\n- item\t\n\n| a  | b |\n|---|---|\n| c | d |\n\nLast \n";
        let built = build_text(text, None, &Known, None);
        let shows = |rich: &Rich, origin: usize, caret: usize| {
            let map = rich.map.as_ref().expect("a source map");
            let caret = caret - origin;
            assert!(
                map.stops()
                    .iter()
                    .any(|s| s.start <= caret && caret <= s.end),
                "no stop at {caret} in {map:?}"
            );
            let (shown, shown_map) =
                crate::srcmap::reveal(rich, map, caret, &text[origin..]).expect("the blanks show");
            let at = shown_map.to_shown(caret).expect("the caret is drawn");
            (shown.text, at)
        };
        let Node::Heading { rich: heading, .. } = &built.items[0].node else {
            panic!("a heading first")
        };
        assert_eq!(
            shows(heading, 0, "# Title ".len()),
            ("# Title ".to_string(), 8)
        );
        let paragraph = text.find("Some").unwrap();
        let Node::Paragraph(rich) = &built.items[1].node else {
            panic!("a paragraph second")
        };
        assert_eq!(
            shows(rich, paragraph, text.find(" \n\n- item").unwrap() + 1),
            ("Some bold ".to_string(), 10)
        );
        let list = text.find("- item").unwrap();
        let Node::List { entries, .. } = &built.items[2].node else {
            panic!("a list third")
        };
        let Node::Paragraph(item) = &entries[0].children[0] else {
            panic!("an item paragraph")
        };
        assert_eq!(
            shows(item, list, text.find("\t").unwrap() + 1),
            ("item\t".to_string(), 5)
        );
        let table = text.find("| a").unwrap();
        let Node::Table { head, .. } = &built.items[3].node else {
            panic!("a table fourth")
        };
        assert_eq!(shows(&head[0], table, table + 4), ("a  ".to_string(), 2));
        let map = rich.map.as_ref().unwrap();
        assert!(map.markup.iter().any(|m| m.line && m.element.len() == 1));
        let Node::Paragraph(last) = &built.items[4].node else {
            panic!("a paragraph last")
        };
        let last_origin = text.find("Last").unwrap();
        assert_eq!(
            shows(last, last_origin, text.len() - 1),
            ("Last ".to_string(), 5)
        );
        let lines = "One \ntwo.\n";
        let built = build_text(lines, None, &Known, None);
        let Node::Paragraph(rich) = &built.items[0].node else {
            panic!("a paragraph")
        };
        assert!(rich.map.as_ref().unwrap().markup.iter().all(|m| !m.line));
    }

    #[test]
    fn items_built_alone_are_those_of_the_whole_note() {
        let text = "Intro.\n\n- one **two**\n- three\n\n```rust\nlet x = 1;\n```\n\nLast *one*.\n";
        let whole = build_text(text, None, &Known, None);
        for (index, item) in whole.items.iter().enumerate() {
            let alone = build_items(text, item.lines.clone(), index, &whole, None, &Known, None);
            assert_eq!(alone.len(), 1, "{:?}", item.lines);
            assert_eq!(alone[0].lines, item.lines);
            let mut a = Vec::new();
            let mut b = Vec::new();
            texts_of(&alone[0].node, &mut a);
            texts_of(&item.node, &mut b);
            assert_eq!(a, b);
        }
        let Node::List { entries, .. } = &whole.items[1].node else {
            panic!("a list")
        };
        assert_eq!(entries[1].line, 3);
        let Node::Paragraph(rich) = &entries[0].children[0] else {
            panic!("an item paragraph")
        };
        assert_exact(rich, text, text.find("- one").unwrap());
        let Node::Code { code, .. } = &whole.items[2].node else {
            panic!("code")
        };
        assert_exact(code, text, text.find("```rust").unwrap());
    }

    #[test]
    fn a_change_in_place_moves_the_lines_below() {
        let text = "One.\n\nTwo.\n\n## Three\n";
        let mut built = build_text(text, None, &Known, None);
        let changed = "One.\nmore\n\nTwo.\n\n## Three\n";
        let items = build_items(changed, 0..2, 0, &built, None, &Known, None);
        built.splice_items(0..1, items, 1);
        let lines: Vec<_> = built.items.iter().map(|i| i.lines.clone()).collect();
        assert_eq!(lines, [0..2, 3..4, 5..6]);
        assert_eq!(built.headings[0].line, 5);
        assert_eq!(built.line_count, 6);
    }

    struct Pdfs;

    impl Resolver for Pdfs {
        fn resolve_link(&self, _: NoteId, target: &str) -> LinkTarget {
            self.resolve_from_root(target)
        }
        fn resolve_from_root(&self, target: &str) -> LinkTarget {
            if target == "report.pdf" {
                LinkTarget::Attachment(PathBuf::from("Files/report.pdf"))
            } else {
                LinkTarget::Unresolved
            }
        }
        fn absolute(&self, relative: &Path) -> PathBuf {
            Path::new("/vault").join(relative)
        }
    }

    #[test]
    fn a_pdf_embed_is_a_page_with_the_width_written() {
        let text = "![[report.pdf]]\n\n![[report.pdf#page=3|300]]\n\n![[missing.pdf]]\n";
        let built = build_text(text, None, &Pdfs, None);
        let embeds: Vec<(Option<PathBuf>, usize, Option<u32>)> = built
            .items
            .iter()
            .map(|item| match &item.node {
                Node::Image(ImageRef {
                    source: ImageSource::Pdf(embed),
                    width,
                    ..
                }) => (embed.file.as_ref().map(|f| f.1.clone()), embed.page, *width),
                other => panic!("not a PDF embed: {other:?}"),
            })
            .collect();
        assert_eq!(
            embeds,
            vec![
                (Some(PathBuf::from("Files/report.pdf")), 0, None),
                (Some(PathBuf::from("Files/report.pdf")), 2, Some(300)),
                (None, 0, None),
            ]
        );
        let Node::Image(ImageRef {
            source: ImageSource::Pdf(embed),
            ..
        }) = &built.items[0].node
        else {
            unreachable!()
        };
        assert_eq!(
            embed.file.as_ref().map(|f| f.0.clone()),
            Some(PathBuf::from("/vault/Files/report.pdf"))
        );
    }

    #[test]
    fn the_width_of_an_embed_is_read_as_an_image_reads_it() {
        assert_eq!(embed_width("![[report.pdf|300]]"), Some(300));
        assert_eq!(embed_width("![[report.pdf#page=2|240x100]]"), Some(240));
        assert_eq!(embed_width("![[report.pdf]]"), None);
        assert_eq!(embed_width("![[report.pdf|wide]]"), None);
    }

    fn first_goal(markdown: &str) -> LinkGoal {
        let document = focus_md::parse(markdown, &focus_md::ParseOptions::default());
        let built = build(&document, None, &Known);
        let Node::Paragraph(rich) = &built.items[0].node else {
            panic!("a paragraph")
        };
        rich.links[0].goal.clone()
    }

    #[test]
    fn a_known_file_resolves_without_a_vault_note() {
        assert!(
            matches!(first_goal("see [[/engagement/Hosts/dc01.md|dc01]]"), LinkGoal::Attachment(p) if p == Path::new("/engagement/Hosts/dc01.md"))
        );
    }

    struct Rooted;

    impl Resolver for Rooted {
        fn resolve_link(&self, _: NoteId, _: &str) -> LinkTarget {
            LinkTarget::Unresolved
        }
        fn absolute(&self, relative: &Path) -> PathBuf {
            relative.to_path_buf()
        }
        fn resolve_from_root(&self, target: &str) -> LinkTarget {
            if target == "Target" {
                LinkTarget::Note {
                    id: 7,
                    anchor: None,
                }
            } else {
                LinkTarget::Unresolved
            }
        }
    }

    #[test]
    fn a_text_without_a_note_resolves_from_the_root() {
        let document = focus_md::parse(
            "see [[Target#Part]] and [[Nothing]]",
            &focus_md::ParseOptions::default(),
        );
        let built = build(&document, None, &Rooted);
        let Node::Paragraph(rich) = &built.items[0].node else {
            panic!("a paragraph")
        };
        assert!(
            matches!(&rich.links[0].goal, LinkGoal::Note { id: 7, anchor: Some(a) } if a == "Part")
        );
        assert!(matches!(&rich.links[1].goal, LinkGoal::Unresolved(t) if t == "Nothing"));
    }

    #[test]
    fn diagram_targets_are_read_as_mermaid_svg_names_them() {
        let source = "flowchart LR\n  A[\"[[dc01|Domain controller]]\"] --> B[[Subroutine]]\n  click A \"[[Hosts/dc01]]\"\n  click B \"Target.md#Second section\"\n  click C call f()\n";
        assert_eq!(
            diagram_targets(source),
            ["dc01", "Hosts/dc01", "Target.md#Second section"]
        );
    }

    #[test]
    fn headings_answer_to_their_text_and_their_slug() {
        assert!(heading_matches(
            "PostgreSQL Comments",
            "postgresql-comments"
        ));
        assert!(heading_matches(
            "PostgreSQL Comments",
            "#PostgreSQL Comments"
        ));
        assert!(heading_matches(
            "Using COPY TO/FROM PROGRAM",
            "using-copy-tofrom-program"
        ));
        assert!(!heading_matches("PostgreSQL Blind", "postgresql-comments"));
    }

    #[test]
    fn an_unknown_target_stays_unresolved() {
        assert!(matches!(first_goal("see [[Nothing]]"), LinkGoal::Unresolved(t) if t == "Nothing"));
    }
}

#[cfg(test)]
mod props_tests {
    use std::path::{Path, PathBuf};
    use std::time::SystemTime;

    use focus_core::{LinkTarget, NoteId, PropValue, PropertyKind, PropertyType, TypeOrigin};

    use super::*;
    use crate::Resolver;

    struct Typed;

    impl Resolver for Typed {
        fn resolve_link(&self, _: NoteId, _: &str) -> LinkTarget {
            LinkTarget::Unresolved
        }
        fn absolute(&self, relative: &Path) -> PathBuf {
            relative.to_path_buf()
        }
        fn property_type(&self, name: &str, value: &PropValue) -> PropertyKind {
            let chosen = match name {
                "due" | "late" | "when" => Some(PropertyType::Date),
                "count" | "n" => Some(PropertyType::Number),
                "stamp" => Some(PropertyType::Datetime),
                "flag" => Some(PropertyType::Checkbox),
                "people" => Some(PropertyType::Multitext),
                _ => None,
            };
            focus_core::property_type(name, value, chosen, None)
        }
    }

    fn meta() -> NoteMeta {
        NoteMeta {
            id: 1,
            path: Arc::from(Path::new("Note.md")),
            title: Arc::from("Note"),
            aliases: Vec::new(),
            tags: Vec::new(),
            headings: Vec::new(),
            blocks: Vec::new(),
            props: Vec::new(),
            links: Vec::new(),
            words: 0,
            tasks_open: 0,
            tasks_done: 0,
            size: 0,
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    const NOTE: &str = "---\ndone: true\nopen:\ncount: 1234.5\ndue: 2020-08-21\nat: 2020-08-21T10:30:00\nname: plain\ncast:\n  - a\n  - b\nlate: 21/08/2020\ntags:\n  - x\n---\n# Note\n";

    fn prop<'a>(built: &'a Built, key: &str) -> &'a Prop {
        built
            .header
            .props
            .iter()
            .find(|p| p.key == key)
            .unwrap_or_else(|| panic!("no property {key}"))
    }

    #[test]
    fn properties_draw_by_their_type() {
        let built = build_text(NOTE, Some(&meta()), &Typed, None);
        let done = prop(&built, "done");
        assert!(done.is_checkbox());
        assert_eq!((done.checked, done.value.text.as_str()), (Some(true), ""));
        let open = prop(&built, "open");
        assert_eq!(open.kind.kind, PropertyType::Text);
        let count = prop(&built, "count");
        assert!(count.is_field());
        assert_eq!(count.kind.origin, TypeOrigin::Focus);
        assert_eq!(count.value.text, "1234.5");
        let start = NOTE.find("1234.5").unwrap();
        let map = count.value.map.clone().expect("a number is mapped as text");
        assert_eq!(map.to_source(2), Some(start + 2));
        let due = prop(&built, "due");
        assert!(due.is_field());
        assert_eq!(
            due.value.text,
            focus_i18n::format_date_at(
                focus_core::parse_property_date("2020-08-21", false)
                    .unwrap()
                    .unix_seconds(),
                0
            )
        );
        let at = prop(&built, "at");
        assert_eq!(
            (at.kind.kind, at.kind.origin),
            (PropertyType::Datetime, TypeOrigin::Value)
        );
        assert!(at.is_field());
        assert_eq!(prop(&built, "name").value.text, "plain");
        let cast = prop(&built, "cast");
        assert_eq!(cast.kind.kind, PropertyType::Multitext);
        assert!(cast.value.map.is_some(), "list values edited one by one");
        assert_eq!(built.header.tags, ["x"]);
    }

    #[test]
    fn a_field_shows_its_value_as_written_under_the_caret() {
        let built = build_text(NOTE, Some(&meta()), &Typed, None);
        let due = prop(&built, "due");
        let map = due.value.map.clone().expect("a field has a map");
        let start = NOTE.find("2020-08-21").unwrap();
        assert_eq!(map.span(), Some(start..start + 10));
        let (shown, shown_map) =
            crate::srcmap::reveal(&due.value, &map, start + 4, NOTE).expect("revealed");
        assert_eq!(shown.text, "2020-08-21");
        assert!(shown.marks.iter().all(|(_, m)| m.mono && !m.muted));
        assert_eq!(shown_map.to_source(4), Some(start + 4));
        assert!(crate::srcmap::reveal(&due.value, &map, 0, NOTE).is_none());
    }

    const EMPTY: &str =
        "---\nwhen:\nstamp: \nn:\nnote:\npeople:\naliases:\nflag:\ncast:\n  - a\n---\n";

    #[test]
    fn an_empty_value_is_an_empty_field_where_the_value_goes() {
        let built = build_text(EMPTY, Some(&meta()), &Typed, None);
        for (key, at) in [
            ("when", "when:".len()),
            ("stamp", "stamp: ".len()),
            ("n", "n:".len()),
            ("note", "note:".len()),
            ("people", "people:".len()),
            ("aliases", "aliases:".len()),
        ] {
            let prop = prop(&built, key);
            let line = EMPTY.find(&format!("\n{key}:")).unwrap() + 1;
            assert!(prop.fits && prop.value.text.is_empty(), "{key}");
            let map = prop.value.map.clone().unwrap_or_else(|| panic!("{key}"));
            assert_eq!(map.span(), Some(line + at..line + at), "{key}");
            assert_eq!(map.to_source(0), Some(line + at), "{key}");
        }
        assert!(prop(&built, "when").is_field());
        let flag = prop(&built, "flag");
        assert!(flag.is_checkbox() && flag.value.map.is_none());
        assert_eq!(flag.checked, None);
        let cast = prop(&built, "cast").value.map.clone().expect("mapped");
        let a = EMPTY.find("- a").unwrap() + 2;
        assert_eq!(cast.span(), Some(a..a + 1));
    }

    #[test]
    fn tags_has_a_field_empty_or_filled() {
        let note = "---\ntags:\nnext: x\n---\n";
        let built = build_text(note, Some(&meta()), &Typed, None);
        let tags = prop(&built, "tags");
        assert_eq!(tags.kind.kind, PropertyType::Tags);
        assert!(tags.fits && tags.value.text.is_empty());
        let end = "---\ntags:".len();
        assert_eq!(
            tags.value.map.as_ref().and_then(|m| m.span()),
            Some(end..end)
        );
        assert!(built.header.tags.is_empty());
        let built = build_text(NOTE, Some(&meta()), &Typed, None);
        let tags = prop(&built, "tags");
        assert_eq!(tags.value.text, "x");
        let x = NOTE.find("- x").unwrap() + 2;
        assert_eq!(
            tags.value.map.as_ref().and_then(|m| m.span()),
            Some(x..x + 1)
        );
        assert_eq!(built.header.tags, ["x"]);
    }

    #[test]
    fn an_empty_element_of_a_list_takes_the_caret_after_its_dash() {
        let note = "---\ncast:\n  - a\n  - \ntags:\n  - x\n  - \n---\n";
        let built = build_text(note, Some(&meta()), &Typed, None);
        let cast = prop(&built, "cast");
        let map = cast.value.map.clone().expect("mapped");
        let empty = note.find("  - \ntags").unwrap() + 4;
        assert_eq!(map.span().map(|s| s.end), Some(empty));
        assert_eq!(map.to_source(cast.value.text.len()), Some(empty));
        assert_eq!(built.header.tags, ["x"]);
    }

    #[test]
    fn a_value_out_of_its_type_shows_as_written_and_marked() {
        let built = build_text(NOTE, Some(&meta()), &Typed, None);
        let late = prop(&built, "late");
        assert_eq!(late.kind.kind, PropertyType::Date);
        assert!(!late.fits && !late.is_field());
        assert_eq!(late.value.text, "21/08/2020");
        let start = NOTE.find("21/08/2020").unwrap();
        let map = late.value.map.clone().expect("as written, mapped as text");
        assert_eq!(map.to_source(3), Some(start + 3));
    }
}
