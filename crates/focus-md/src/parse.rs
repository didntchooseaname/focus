use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::LazyLock;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, LinkType, Options, Parser, Tag};
use regex::Regex;

use crate::origin::{Origin, line_starts};
use crate::{
    Align, Block, BlockKind, Document, Footnote, FootnoteReference, Frontmatter, Image, Inline,
    InlineKind, Link, LinkKind, ListItem, ParseOptions, PropertyLink,
};

const IMAGE_EXTENSIONS: [&str; 9] = [
    "png", "jpg", "jpeg", "gif", "svg", "webp", "bmp", "avif", "ico",
];

pub fn parse(source: &str, options: &ParseOptions) -> Document {
    let body = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut raw = parse_raw(body, options, true);
    if body.len() < source.len() {
        raw.remap(&Origin::shift(source.len() - body.len()));
    }
    let footnote_refs = index_footnotes(&raw.blocks, &mut raw.footnotes);
    Document {
        frontmatter: raw.frontmatter,
        blocks: raw.blocks,
        footnotes: raw.footnotes,
        footnote_refs,
        line_count: raw.line_count,
    }
}

pub(crate) struct Raw {
    pub frontmatter: Option<Frontmatter>,
    pub blocks: Vec<Block>,
    pub footnotes: Vec<Footnote>,
    pub line_count: u32,
}

impl Raw {
    pub fn remap(&mut self, origin: &Origin) {
        if origin.is_identity() {
            return;
        }
        remap_blocks(&mut self.blocks, origin);
        for footnote in &mut self.footnotes {
            remap_blocks(&mut footnote.blocks, origin);
        }
        if let Some(frontmatter) = &mut self.frontmatter {
            frontmatter.bytes = origin.range(&frontmatter.bytes);
        }
    }
}

pub(crate) fn parse_raw(source: &str, options: &ParseOptions, top: bool) -> Raw {
    let (cleaned, uncommented) = strip_comments(source);
    let regions = crate::tabs::find_regions(&cleaned);
    let (masked, unmasked) = crate::tabs::mask(&cleaned, &regions);
    let src: &str = &masked;
    let mut builder = Builder::new(src, options);
    let mut flags = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_WIKILINKS;
    if top {
        flags |= Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    }
    for (event, range) in Parser::new_ext(src, flags).into_offset_iter() {
        builder.event(event, range);
    }
    let mut raw = builder.finish();
    raw.remap(&unmasked);
    if src.contains("tabs") || !regions.is_empty() {
        let mut tabber = crate::tabs::Tabber {
            options,
            footnotes: Vec::new(),
            source: &cleaned,
            line_starts: line_starts(&cleaned),
        };
        raw.blocks = tabber.convert(std::mem::take(&mut raw.blocks));
        for footnote in &mut raw.footnotes {
            footnote.blocks = tabber.convert(std::mem::take(&mut footnote.blocks));
        }
        raw.blocks = tabber.insert(std::mem::take(&mut raw.blocks), regions);
        for footnote in tabber.footnotes {
            if !raw.footnotes.iter().any(|f| f.label == footnote.label) {
                raw.footnotes.push(footnote);
            }
        }
    }
    raw.remap(&uncommented);
    raw
}

pub fn plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    push_plain(inlines, &mut out);
    out
}

fn push_plain(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match &inline.kind {
            InlineKind::Text(t) | InlineKind::Code(t) | InlineKind::Math(t) => out.push_str(t),
            InlineKind::Strong(c)
            | InlineKind::Emphasis(c)
            | InlineKind::Strike(c)
            | InlineKind::Highlight(c) => push_plain(c, out),
            InlineKind::Link(link) => push_plain(&link.children, out),
            InlineKind::Image(image) => out.push_str(&image.alt),
            InlineKind::Tag(t) => {
                out.push('#');
                out.push_str(t);
            }
            InlineKind::SoftBreak | InlineKind::HardBreak => out.push(' '),
            InlineKind::FootnoteRef(_) | InlineKind::BlockId(_) | InlineKind::Html(_) => {}
        }
    }
}

fn strip_comments(src: &str) -> (Cow<'_, str>, Origin) {
    let mut origin = Origin::shift(0);
    if !src.contains("%%") {
        return (Cow::Borrowed(src), origin);
    }
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut fence: Option<(u8, usize)> = None;
    let mut pos = 0;
    while pos < src.len() {
        let line_end = src[pos..].find('\n').map_or(src.len(), |i| pos + i + 1);
        let at_line_start = pos == 0 || bytes[pos - 1] == b'\n';
        if at_line_start {
            let line = src[pos..line_end].trim_start_matches([' ', '\t', '>']);
            if let Some((ch, len)) = fence {
                let run = line.bytes().take_while(|&b| b == ch).count();
                if run >= len && line[run..].trim().is_empty() {
                    fence = None;
                }
                out.push_str(&src[pos..line_end]);
                pos = line_end;
                continue;
            }
            let first = line.bytes().next();
            if let Some(ch @ (b'`' | b'~')) = first {
                let run = line.bytes().take_while(|&b| b == ch).count();
                if run >= 3 && !(ch == b'`' && line[run..].contains('`')) {
                    fence = Some((ch, run));
                    out.push_str(&src[pos..line_end]);
                    pos = line_end;
                    continue;
                }
            }
        }
        let mut i = pos;
        let mut resume = None;
        while i < line_end {
            match bytes[i] {
                b'`' => {
                    let run = bytes[i..line_end]
                        .iter()
                        .take_while(|&&b| b == b'`')
                        .count();
                    let close = find_backtick_run(&bytes[i + run..line_end], run)
                        .map_or(i + run, |off| i + run + off + run);
                    out.push_str(&src[i..close]);
                    i = close;
                }
                b'%' if bytes.get(i + 1) == Some(&b'%') => {
                    if let Some(off) = src[i + 2..].find("%%") {
                        let end = i + 2 + off + 2;
                        for (at, _) in src[i..end].match_indices('\n') {
                            origin.push(out.len(), i + at, true);
                            out.push('\n');
                        }
                        origin.push(out.len(), end, true);
                        resume = Some(end);
                        break;
                    }
                    out.push_str("%%");
                    i += 2;
                }
                _ => {
                    let next = src[i..].chars().next().map_or(1, char::len_utf8);
                    out.push_str(&src[i..i + next]);
                    i += next;
                }
            }
        }
        pos = resume.unwrap_or(line_end);
    }
    (Cow::Owned(out), origin)
}

fn find_backtick_run(hay: &[u8], run: usize) -> Option<usize> {
    let mut i = 0;
    while i < hay.len() {
        if hay[i] == b'`' {
            let n = hay[i..].iter().take_while(|&&b| b == b'`').count();
            if n == run {
                return Some(i);
            }
            i += n;
        } else {
            i += 1;
        }
    }
    None
}

enum FrameKind {
    Root,
    Quote,
    Footnote(String),
    List {
        ordered: bool,
        start: u64,
        items: Vec<ListItem>,
        loose: bool,
    },
    Item {
        task: Option<char>,
        task_end: Option<usize>,
    },
    Paragraph,
    Heading(u8),
    CodeBlock {
        lang: String,
    },
    HtmlBlock,
    Metadata,
    Table {
        align: Vec<Align>,
        head: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
        cells: Vec<Vec<Range<usize>>>,
    },
    Cells {
        cells: Vec<Vec<Inline>>,
        ranges: Vec<Range<usize>>,
    },
    Cell,
    Strong,
    Emphasis,
    Strike,
    Link {
        kind: LinkKind,
        dest: String,
        has_alias: bool,
    },
    Image {
        dest: String,
        has_alias: bool,
        wiki: bool,
    },
    HtmlInline {
        name: String,
        raw: String,
        href: Option<String>,
    },
}

struct Frame {
    kind: FrameKind,
    range: Range<usize>,
    blocks: Vec<Block>,
    inlines: Vec<Inline>,
    loose_range: Option<Range<usize>>,
    embeds: Vec<usize>,
    display_math: Option<String>,
    text: String,
    text_bytes: Option<Range<usize>>,
}

impl Frame {
    fn new(kind: FrameKind, range: Range<usize>) -> Self {
        Frame {
            kind,
            range,
            blocks: Vec::new(),
            inlines: Vec::new(),
            loose_range: None,
            embeds: Vec::new(),
            display_math: None,
            text: String::new(),
            text_bytes: None,
        }
    }

    fn is_container(&self) -> bool {
        matches!(
            self.kind,
            FrameKind::Root | FrameKind::Quote | FrameKind::Item { .. } | FrameKind::Footnote(_)
        )
    }
}

struct Builder<'a> {
    src: &'a str,
    line_starts: Vec<usize>,
    options: ParseOptions,
    stack: Vec<Frame>,
    frontmatter: Option<Frontmatter>,
    footnote_defs: Vec<Footnote>,
    escaped: Vec<usize>,
}

impl<'a> Builder<'a> {
    fn new(src: &'a str, options: &ParseOptions) -> Self {
        Builder {
            src,
            line_starts: line_starts(src),
            options: options.clone(),
            stack: vec![Frame::new(FrameKind::Root, 0..src.len())],
            frontmatter: None,
            footnote_defs: Vec::new(),
            escaped: Vec::new(),
        }
    }

    fn text(&self, text: String, range: Range<usize>) -> Inline {
        let exact = self.src.get(range.clone()) == Some(text.as_str());
        Inline::new(InlineKind::Text(text), range, exact)
    }

    fn line_end(&self, offset: usize) -> usize {
        self.end_of_line(self.line_of(offset))
    }

    fn end_of_line(&self, line: u32) -> usize {
        let next = self
            .line_starts
            .get(line as usize + 1)
            .map_or(self.src.len(), |&s| s - 1);
        next - usize::from(self.src[..next].ends_with('\r'))
    }

    fn line_of(&self, offset: usize) -> u32 {
        (self.line_starts.partition_point(|&s| s <= offset) - 1) as u32
    }

    fn lines(&self, range: &Range<usize>) -> (Range<u32>, Range<usize>) {
        let start = self.line_of(range.start);
        let end = range.end.min(self.src.len()).max(range.start);
        let content_end = range.start + self.src[range.start..end].trim_end().len();
        if content_end == range.start {
            return (start..start + 1, range.start..range.start);
        }
        let last = self.line_of(content_end - 1);
        let bytes_end = self.end_of_line(last).clamp(content_end, end);
        (start..last + 1, range.start..bytes_end)
    }

    fn line_count(&self) -> u32 {
        let n = self.line_starts.len() as u32;
        if self.src.is_empty() || self.src.ends_with('\n') {
            n - 1
        } else {
            n
        }
    }

    fn top(&mut self) -> &mut Frame {
        self.stack
            .last_mut()
            .expect("the root frame is never popped")
    }

    fn event(&mut self, event: Event<'_>, range: Range<usize>) {
        let inline = |kind: InlineKind, range: Range<usize>| Inline::new(kind, range, false);
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(_) => self.close(),
            Event::Text(text) => self.text_event(&text, range),
            Event::Code(code) => {
                self.push_inline(inline(InlineKind::Code(code.into_string()), range))
            }
            Event::InlineMath(tex) => {
                self.push_inline(inline(InlineKind::Math(tex.into_string()), range))
            }
            Event::DisplayMath(tex) => {
                let tex = tex.trim().to_string();
                let frame = self.top();
                if matches!(frame.kind, FrameKind::Paragraph) && frame.inlines.is_empty() {
                    frame.display_math = Some(tex);
                } else {
                    self.push_inline(inline(InlineKind::Math(tex), range));
                }
            }
            Event::Html(html) => {
                let frame = self.top();
                if matches!(frame.kind, FrameKind::HtmlBlock) {
                    frame.text.push_str(&html);
                } else {
                    self.push_inline(inline(InlineKind::Html(html.into_string()), range));
                }
            }
            Event::InlineHtml(html) => {
                let html = html.into_string();
                if html.starts_with("<!--") {
                } else if is_known_html_tag(&html) {
                    self.html_inline(html, range);
                } else {
                    let text = self.text(html, range);
                    self.push_inline(text);
                }
            }
            Event::FootnoteReference(label) => {
                self.push_inline(inline(InlineKind::FootnoteRef(label.into_string()), range));
            }
            Event::SoftBreak => {
                let kind = if self.options.hard_breaks {
                    InlineKind::HardBreak
                } else {
                    InlineKind::SoftBreak
                };
                self.push_inline(inline(kind, range));
            }
            Event::HardBreak => self.push_inline(inline(InlineKind::HardBreak, range)),
            Event::Rule => {
                self.flush_loose();
                let (lines, bytes) = self.lines(&range);
                self.push_block(Block {
                    lines,
                    bytes,
                    kind: BlockKind::Rule,
                });
            }
            Event::TaskListMarker(done) => {
                if let FrameKind::Item { task, task_end } = &mut self.top().kind {
                    *task = Some(if done { 'x' } else { ' ' });
                    *task_end = Some(range.end);
                }
            }
        }
    }

    fn text_event(&mut self, text: &str, range: Range<usize>) {
        let frame = self.top();
        if matches!(
            frame.kind,
            FrameKind::CodeBlock { .. } | FrameKind::Metadata
        ) {
            frame.text.push_str(text);
            frame.text_bytes = Some(match &frame.text_bytes {
                Some(bytes) => bytes.start..range.end,
                None => range,
            });
            return;
        }
        if range.start > 0
            && self.src.as_bytes()[range.start - 1] == b'\\'
            && text.starts_with(['#', '='])
        {
            self.escaped.push(range.start);
        }
        let text = self.text(text.to_string(), range);
        self.push_inline(text);
    }

    fn push_inline(&mut self, inline: Inline) {
        let frame = self.top();
        if frame.is_container() {
            let range = inline.source.clone();
            frame.loose_range = Some(match &frame.loose_range {
                Some(r) => r.start.min(range.start)..r.end.max(range.end),
                None => range,
            });
        }
        if let Some(last) = frame.inlines.last_mut()
            && inline.exact()
            && last.exact()
            && last.source.end == inline.source.start
            && let (InlineKind::Text(new), InlineKind::Text(text)) = (&inline.kind, &mut last.kind)
        {
            text.push_str(new);
            last.source.end = inline.source.end;
            return;
        }
        frame.inlines.push(inline);
    }

    fn html_inline(&mut self, html: String, range: Range<usize>) {
        let token = crate::html::tokens(&html)
            .into_iter()
            .next()
            .map(|(_, t)| t);
        match token {
            Some(crate::html::Token::Open {
                name,
                attrs,
                self_closing,
                ..
            }) => match name.as_str() {
                "br" => self.push_inline(Inline::new(InlineKind::HardBreak, range, false)),
                "img" => match attr_image(&attrs) {
                    Some(image) => {
                        self.push_inline(Inline::new(InlineKind::Image(image), range, false))
                    }
                    None => self.push_inline(Inline::new(InlineKind::Html(html), range, false)),
                },
                "em" | "i" | "strong" | "b" | "code" | "mark" | "del" | "s" | "strike"
                    if !self_closing =>
                {
                    self.stack.push(Frame::new(
                        FrameKind::HtmlInline {
                            name,
                            raw: html,
                            href: None,
                        },
                        range,
                    ));
                }
                "a" if !self_closing => {
                    match crate::html::attr(&attrs, "href").filter(|h| !h.is_empty()) {
                        Some(href) => {
                            let href = href.to_string();
                            self.stack.push(Frame::new(
                                FrameKind::HtmlInline {
                                    name,
                                    raw: html,
                                    href: Some(href),
                                },
                                range,
                            ));
                        }
                        None => self.push_inline(Inline::new(InlineKind::Html(html), range, false)),
                    }
                }
                _ => self.push_inline(Inline::new(InlineKind::Html(html), range, false)),
            },
            Some(crate::html::Token::Close { name }) => {
                let matches_top = matches!(&self.top().kind, FrameKind::HtmlInline { name: open, .. } if *open == name);
                if matches_top {
                    let frame = self.stack.pop().expect("length checked");
                    self.close_html_inline(frame, range);
                } else {
                    self.push_inline(Inline::new(InlineKind::Html(html), range, false));
                }
            }
            _ => self.push_inline(Inline::new(InlineKind::Html(html), range, false)),
        }
    }

    fn close_html_inline(&mut self, frame: Frame, range: Range<usize>) {
        let FrameKind::HtmlInline { name, href, .. } = frame.kind else {
            return;
        };
        let source = frame.range.start..range.end;
        let children = self.finish_inlines(frame.inlines, true);
        let kind = match name.as_str() {
            "em" | "i" => InlineKind::Emphasis(children),
            "strong" | "b" => InlineKind::Strong(children),
            "mark" => InlineKind::Highlight(children),
            "del" | "s" | "strike" => InlineKind::Strike(children),
            "code" => InlineKind::Code(plain_text(&children)),
            _ => {
                let link = self.make_link(
                    LinkKind::Markdown,
                    href.unwrap_or_default(),
                    true,
                    children,
                    &source,
                );
                InlineKind::Link(link)
            }
        };
        self.push_inline(Inline::new(kind, source, false));
    }

    fn unwrap_html_inlines(&mut self) {
        while matches!(
            self.stack.last().map(|f| &f.kind),
            Some(FrameKind::HtmlInline { .. })
        ) {
            let frame = self.stack.pop().expect("length checked");
            let range = frame.range.clone();
            let FrameKind::HtmlInline { raw, .. } = frame.kind else {
                continue;
            };
            self.push_inline(Inline::new(InlineKind::Html(raw), range, false));
            for inline in frame.inlines {
                self.push_inline(inline);
            }
        }
    }

    fn push_block(&mut self, block: Block) {
        self.top().blocks.push(block);
    }

    fn flush_loose(&mut self) {
        let frame = self.top();
        if !frame.is_container() || frame.inlines.is_empty() {
            return;
        }
        let inlines = std::mem::take(&mut frame.inlines);
        let embeds = std::mem::take(&mut frame.embeds);
        let range = frame.loose_range.take().unwrap_or(0..0);
        let blocks = self.paragraph_blocks(inlines, &embeds, None, range);
        self.top().blocks.extend(blocks);
    }

    fn start(&mut self, tag: Tag<'_>, range: Range<usize>) {
        let kind = match tag {
            Tag::Paragraph => {
                self.flush_loose();
                let n = self.stack.len();
                if n >= 2
                    && matches!(self.stack[n - 1].kind, FrameKind::Item { .. })
                    && let FrameKind::List { loose, .. } = &mut self.stack[n - 2].kind
                {
                    *loose = true;
                }
                FrameKind::Paragraph
            }
            Tag::Heading { level, .. } => {
                self.flush_loose();
                FrameKind::Heading(level as u8)
            }
            Tag::BlockQuote(_) => {
                self.flush_loose();
                FrameKind::Quote
            }
            Tag::CodeBlock(kind) => {
                self.flush_loose();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").to_lowercase()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                FrameKind::CodeBlock { lang }
            }
            Tag::HtmlBlock => {
                self.flush_loose();
                FrameKind::HtmlBlock
            }
            Tag::List(start) => {
                self.flush_loose();
                FrameKind::List {
                    ordered: start.is_some(),
                    start: start.unwrap_or(1),
                    items: Vec::new(),
                    loose: false,
                }
            }
            Tag::Item => FrameKind::Item {
                task: None,
                task_end: None,
            },
            Tag::FootnoteDefinition(label) => {
                self.flush_loose();
                FrameKind::Footnote(label.into_string())
            }
            Tag::Table(aligns) => {
                self.flush_loose();
                let align = aligns
                    .iter()
                    .map(|a| match a {
                        Alignment::None => Align::None,
                        Alignment::Left => Align::Left,
                        Alignment::Center => Align::Center,
                        Alignment::Right => Align::Right,
                    })
                    .collect();
                FrameKind::Table {
                    align,
                    head: Vec::new(),
                    rows: Vec::new(),
                    cells: Vec::new(),
                }
            }
            Tag::TableHead | Tag::TableRow => FrameKind::Cells {
                cells: Vec::new(),
                ranges: Vec::new(),
            },
            Tag::TableCell => FrameKind::Cell,
            Tag::Emphasis => FrameKind::Emphasis,
            Tag::Strong => FrameKind::Strong,
            Tag::Strikethrough => FrameKind::Strike,
            Tag::Link {
                link_type,
                dest_url,
                ..
            } => {
                let (kind, has_alias) = match link_type {
                    LinkType::WikiLink { has_pothole } => (LinkKind::Wiki, has_pothole),
                    LinkType::Autolink | LinkType::Email => (LinkKind::Auto, true),
                    _ => (LinkKind::Markdown, true),
                };
                let dest =
                    wiki_destination(dest_url.into_string(), kind == LinkKind::Wiki && has_alias);
                FrameKind::Link {
                    kind,
                    dest,
                    has_alias,
                }
            }
            Tag::Image {
                link_type,
                dest_url,
                ..
            } => {
                let (wiki, has_alias) = match link_type {
                    LinkType::WikiLink { has_pothole } => (true, has_pothole),
                    _ => (false, true),
                };
                let dest = wiki_destination(dest_url.into_string(), wiki && has_alias);
                FrameKind::Image {
                    dest,
                    has_alias,
                    wiki,
                }
            }
            Tag::MetadataBlock(_) => FrameKind::Metadata,
            _ => FrameKind::Emphasis,
        };
        self.stack.push(Frame::new(kind, range));
    }

    fn close(&mut self) {
        if self.stack.len() <= 1 {
            return;
        }
        self.unwrap_html_inlines();
        self.flush_loose();
        let frame = self.stack.pop().expect("length checked");
        let range = frame.range.clone();
        let (lines, bytes) = self.lines(&range);
        match frame.kind {
            FrameKind::Root => {}
            FrameKind::Paragraph => {
                let Frame {
                    inlines,
                    embeds,
                    display_math,
                    ..
                } = frame;
                let blocks = self.paragraph_blocks(inlines, &embeds, display_math, range);
                self.top().blocks.extend(blocks);
            }
            FrameKind::Heading(level) => {
                let mut inlines = self.finish_inlines(frame.inlines, true);
                take_block_id(&mut inlines);
                let id = plain_text(&inlines)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                let marker = self.heading_marker(&bytes);
                self.push_block(Block {
                    lines,
                    bytes,
                    kind: BlockKind::Heading {
                        level,
                        id,
                        inlines,
                        marker,
                    },
                });
            }
            FrameKind::Quote => {
                let block = make_quote(frame.blocks, lines, bytes);
                self.push_block(block);
            }
            FrameKind::Footnote(label) => {
                self.footnote_defs.push(Footnote {
                    label,
                    blocks: frame.blocks,
                    number: 0,
                    references: Vec::new(),
                });
            }
            FrameKind::List {
                ordered,
                start,
                items,
                loose,
            } => {
                let kind = BlockKind::List {
                    ordered,
                    start,
                    tight: !loose,
                    items,
                };
                self.push_block(Block { lines, bytes, kind });
            }
            FrameKind::Item { task, task_end } => {
                let mut blocks = frame.blocks;
                let (task, task_end) = match task {
                    Some(state) => (Some(state), task_end),
                    None => match take_custom_task(&mut blocks) {
                        Some((state, end)) => (Some(state), Some(end)),
                        None => (None, None),
                    },
                };
                let item = ListItem {
                    lines,
                    marker: self.item_marker(range.start, task_end),
                    task,
                    blocks,
                };
                if let FrameKind::List { items, .. } = &mut self.top().kind {
                    items.push(item);
                }
            }
            FrameKind::CodeBlock { lang } => {
                let mut code = frame.text;
                let mut content = frame.text_bytes.unwrap_or_else(|| {
                    let next = self
                        .line_starts
                        .get(lines.start as usize + 1)
                        .map_or(self.src.len(), |&s| s);
                    let at = next.clamp(bytes.start, bytes.end);
                    at..at
                });
                if code.ends_with('\n') {
                    code.pop();
                    if self.src[..content.end].ends_with('\n') && content.end > content.start {
                        content.end -= 1;
                    }
                }
                let bytes = bytes.start..bytes.end.max(content.end);
                let kind = if lang == "mermaid" || lang == "mermaidjs" {
                    BlockKind::Mermaid { source: code }
                } else {
                    BlockKind::CodeBlock {
                        lang,
                        code,
                        content,
                    }
                };
                self.push_block(Block { lines, bytes, kind });
            }
            FrameKind::HtmlBlock => {
                let html = frame.text.trim_end_matches('\n').to_string();
                if is_comment_only(&html) {
                } else if let Some(blocks) = crate::html_model::block_to_model(
                    &html,
                    &Origin::of_lines(self.src, &self.line_starts, lines.start, &html),
                    &lines,
                    &bytes,
                    &self.options,
                ) {
                    for block in blocks {
                        self.push_block(block);
                    }
                } else {
                    self.push_block(Block {
                        lines,
                        bytes,
                        kind: BlockKind::Html(html),
                    });
                }
            }
            FrameKind::Metadata => {
                let raw = frame.text.trim_end_matches('\n').to_string();
                let links = property_links(&raw, lines.start + 1);
                self.frontmatter = Some(Frontmatter {
                    raw,
                    lines,
                    bytes,
                    links,
                });
            }
            FrameKind::Table {
                align,
                head,
                rows,
                cells,
            } => {
                self.push_block(Block {
                    lines,
                    bytes,
                    kind: BlockKind::Table {
                        align,
                        head,
                        rows,
                        cells,
                    },
                });
            }
            FrameKind::Cells { cells, ranges } => {
                let end = bytes.end;
                let ranges = ranges
                    .into_iter()
                    .map(|r| if r.start > end { end..end } else { r })
                    .collect();
                if let FrameKind::Table {
                    head,
                    rows,
                    cells: all,
                    ..
                } = &mut self.top().kind
                {
                    if head.is_empty() && rows.is_empty() {
                        *head = cells;
                    } else {
                        rows.push(cells);
                    }
                    all.push(ranges);
                }
            }
            FrameKind::Cell => {
                let cell = self.finish_inlines(frame.inlines, true);
                let content = cell_content(self.src, &range);
                if let FrameKind::Cells { cells, ranges } = &mut self.top().kind {
                    cells.push(cell);
                    ranges.push(content);
                }
            }
            FrameKind::Strong => {
                let children = self.finish_inlines(frame.inlines, true);
                self.push_inline(Inline::new(InlineKind::Strong(children), range, false));
            }
            FrameKind::Emphasis => {
                let children = self.finish_inlines(frame.inlines, true);
                self.push_inline(Inline::new(InlineKind::Emphasis(children), range, false));
            }
            FrameKind::Strike => {
                let children = self.finish_inlines(frame.inlines, true);
                self.push_inline(Inline::new(InlineKind::Strike(children), range, false));
            }
            FrameKind::Link {
                kind,
                dest,
                has_alias,
            } => {
                let link = self.make_link(kind, dest, has_alias, frame.inlines, &range);
                self.push_inline(Inline::new(InlineKind::Link(link), range, false));
            }
            FrameKind::Image {
                dest,
                has_alias,
                wiki,
            } => {
                self.close_image(dest, has_alias, wiki, frame.inlines, range);
            }
            FrameKind::HtmlInline { .. } => {}
        }
    }

    fn heading_marker(&self, bytes: &Range<usize>) -> Range<usize> {
        let text = &self.src[bytes.clone()];
        let lead = text.len() - text.trim_start_matches([' ', '\t']).len();
        let hashes = text[lead..].bytes().take_while(|&b| b == b'#').count();
        let line = text[lead + hashes..].split('\n').next().unwrap_or("");
        if (1..=6).contains(&hashes) && (line.is_empty() || line.starts_with([' ', '\t', '\r'])) {
            let spaces = line
                .bytes()
                .take_while(|&b| b == b' ' || b == b'\t')
                .count();
            let start = bytes.start + lead;
            return start..start + hashes + spaces;
        }
        let underline = text.trim_end();
        let run = underline
            .bytes()
            .rev()
            .take_while(|&b| b == b'=' || b == b'-')
            .count();
        let end = bytes.start + underline.len();
        end - run..end
    }

    fn item_marker(&self, start: usize, task_end: Option<usize>) -> Range<usize> {
        let rest = &self.src[start..];
        let line = &rest.as_bytes()[..line_len(rest)];
        let mut end = line.iter().take_while(|b| b.is_ascii_digit()).count();
        let sign = if end > 0 {
            b".)".as_slice()
        } else {
            b"-*+".as_slice()
        };
        if line.get(end).is_some_and(|b| sign.contains(b)) {
            end += 1;
        }
        let task = task_end
            .map_or(0, |e| e.saturating_sub(start))
            .min(line.len());
        let mut end = end.max(task);
        end += line[end..]
            .iter()
            .take_while(|&&b| b == b' ' || b == b'\t')
            .count();
        start..start + end
    }

    fn make_link(
        &mut self,
        kind: LinkKind,
        dest: String,
        has_alias: bool,
        children: Vec<Inline>,
        source: &Range<usize>,
    ) -> Link {
        let (target, anchor) = match kind {
            LinkKind::Wiki => split_anchor(&dest),
            LinkKind::Markdown if !is_url(&dest) => split_anchor(&dest),
            _ => (dest, None),
        };
        let children = if kind == LinkKind::Wiki && !has_alias {
            let shown = match &anchor {
                Some(a) => format!("{target} > {a}"),
                None => target.clone(),
            };
            let inner = source.start + 2..source.end.saturating_sub(2).max(source.start + 2);
            vec![self.text(shown, span(&children).unwrap_or(inner))]
        } else {
            self.finish_inlines(children, false)
        };
        Link {
            kind,
            target,
            anchor,
            children,
        }
    }

    fn close_image(
        &mut self,
        dest: String,
        has_alias: bool,
        wiki: bool,
        children: Vec<Inline>,
        range: Range<usize>,
    ) {
        let alt_text = plain_text(&children);
        if wiki && !is_image_target(&dest) {
            let (target, anchor) = split_anchor(&dest);
            let shown = if has_alias { alt_text } else { dest.clone() };
            let inner = range.start + 3..range.end.saturating_sub(2).max(range.start + 3);
            let link = Link {
                kind: LinkKind::Wiki,
                target,
                anchor,
                children: vec![self.text(shown, span(&children).unwrap_or(inner))],
            };
            self.push_inline(Inline::new(InlineKind::Link(link), range, false));
            let frame = self.top();
            let index = frame.inlines.len() - 1;
            frame.embeds.push(index);
            return;
        }
        let (alt, width, height) =
            split_image_size(if wiki && !has_alias { "" } else { &alt_text });
        let alt = if alt.is_empty() && wiki {
            dest.clone()
        } else {
            alt
        };
        self.push_inline(Inline::new(
            InlineKind::Image(Image {
                src: dest,
                alt,
                caption: None,
                width,
                height,
            }),
            range,
            false,
        ));
    }

    fn finish_inlines(&mut self, inlines: Vec<Inline>, allow_text_extensions: bool) -> Vec<Inline> {
        let inlines = if allow_text_extensions {
            expand_text(inlines, &self.escaped)
        } else {
            inlines
        };
        highlight(inlines, &self.escaped)
    }

    fn paragraph_blocks(
        &mut self,
        inlines: Vec<Inline>,
        embeds: &[usize],
        display_math: Option<String>,
        range: Range<usize>,
    ) -> Vec<Block> {
        let (lines, bytes) = self.lines(&range);
        if range.end > range.start
            && self.src[range.clone()].contains("{{#")
            && let Some(pieces) = crate::tabs::split_directives(&self.src[range.clone()])
        {
            return self.directive_blocks(pieces, lines.start);
        }
        if let Some(tex) = display_math
            && inlines.iter().all(is_blank_text)
        {
            return vec![Block {
                lines,
                bytes,
                kind: BlockKind::Math { tex },
            }];
        }
        if let Some(blocks) = split_media_lines(&inlines, embeds, &lines, &bytes) {
            return blocks;
        }
        let mut inlines = self.finish_inlines(inlines, true);
        take_block_id(&mut inlines);
        let inlines = crate::tabs::inline_refs(inlines);
        vec![Block {
            lines,
            bytes,
            kind: BlockKind::Paragraph(inlines),
        }]
    }

    fn line_content(&self, line: u32) -> Range<usize> {
        let start = self.line_starts[line as usize];
        let end = self.line_end(start);
        let text = &self.src[start..end];
        let bare = crate::tabs::bare(text);
        let from = start + text.len() - bare.len();
        from..from + bare.trim_end().len()
    }

    fn directive_blocks(&mut self, pieces: Vec<crate::tabs::Piece>, first_line: u32) -> Vec<Block> {
        let mut out = Vec::new();
        for piece in pieces {
            match piece {
                crate::tabs::Piece::Text { first, text } => {
                    let line = first_line + first as u32;
                    let mut raw = parse_raw(&text, &self.options, false);
                    raw.remap(&Origin::of_lines(self.src, &self.line_starts, line, &text));
                    shift_lines(&mut raw.blocks, line);
                    self.footnote_defs.extend(raw.footnotes);
                    out.extend(raw.blocks);
                }
                crate::tabs::Piece::Directive {
                    first,
                    last,
                    directive,
                } => {
                    let (first, last) = (first_line + first as u32, first_line + last as u32);
                    out.push(Block {
                        lines: first..last + 1,
                        bytes: self.line_content(first).start..self.line_content(last).end,
                        kind: BlockKind::Directive(directive),
                    });
                }
            }
        }
        out
    }

    fn finish(mut self) -> Raw {
        while self.stack.len() > 1 {
            self.close();
        }
        self.unwrap_html_inlines();
        self.flush_loose();
        let root = self.stack.pop().expect("the root frame remains");
        let line_count = self.line_count();
        let mut footnotes = self.footnote_defs;
        let mut blocks = root.blocks;
        if self.src.to_ascii_lowercase().contains("<details") {
            let lines = Lines {
                src: self.src,
                starts: &self.line_starts,
                options: &self.options,
            };
            blocks = group_details(blocks, &lines);
            for footnote in &mut footnotes {
                footnote.blocks = group_details(std::mem::take(&mut footnote.blocks), &lines);
            }
        }
        Raw {
            frontmatter: self.frontmatter,
            blocks,
            footnotes,
            line_count,
        }
    }
}

fn property_links(raw: &str, first_line: u32) -> Vec<PropertyLink> {
    static WIKI: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\[\[([^\[\]|#]+)(?:#([^\[\]|]*))?(?:\|([^\[\]]*))?\]\]")
            .expect("wiki link pattern")
    });
    let mut key = String::new();
    let mut links = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        if !line.starts_with([' ', '\t', '-', '#'])
            && let Some((name, _)) = line.split_once(':')
        {
            key = name.trim().trim_matches(['"', '\'']).to_string();
        }
        for captures in WIKI.captures_iter(line) {
            let target = captures[1].trim().to_string();
            let display = captures
                .get(3)
                .map(|m| m.as_str().trim().to_string())
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| target.clone());
            let anchor = captures
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .filter(|a| !a.is_empty());
            links.push(PropertyLink {
                key: key.clone(),
                line: first_line + index as u32,
                target,
                anchor,
                display,
            });
        }
    }
    links
}

fn index_footnotes(blocks: &[Block], footnotes: &mut [Footnote]) -> Vec<FootnoteReference> {
    let mut found: Vec<(String, u32)> = Vec::new();
    visit_blocks(blocks, &mut |label, line| {
        found.push((label.to_string(), line))
    });
    let mut order: Vec<String> = Vec::new();
    let number_of =
        |label: &str, order: &mut Vec<String>| match order.iter().position(|l| l == label) {
            Some(i) => i as u32 + 1,
            None => {
                order.push(label.to_string());
                order.len() as u32
            }
        };
    for (label, _) in &found {
        number_of(label, &mut order);
    }
    let mut at = 0;
    while at < order.len() {
        if let Some(note) = footnotes.iter().find(|f| f.label == order[at]) {
            let mut inner = Vec::new();
            visit_blocks(&note.blocks, &mut |label, line| {
                inner.push((label.to_string(), line))
            });
            for (label, _) in &inner {
                number_of(label, &mut order);
            }
            found.extend(inner);
        }
        at += 1;
    }
    for note in footnotes.iter() {
        number_of(&note.label, &mut order);
    }
    for note in footnotes.iter_mut() {
        note.number = order
            .iter()
            .position(|l| *l == note.label)
            .map_or(0, |i| i as u32 + 1);
    }
    footnotes.sort_by_key(|f| f.number);
    let mut seen: HashMap<&str, u32> = HashMap::new();
    let refs: Vec<FootnoteReference> = found
        .iter()
        .map(|(label, line)| {
            let occurrence = seen
                .entry(label.as_str())
                .and_modify(|n| *n += 1)
                .or_insert(0);
            FootnoteReference {
                label: label.clone(),
                number: order
                    .iter()
                    .position(|l| l == label)
                    .map_or(0, |i| i as u32 + 1),
                definition: footnotes.iter().position(|f| f.label == *label),
                occurrence: *occurrence,
                line: *line,
            }
        })
        .collect();
    for (index, reference) in refs.iter().enumerate() {
        if let Some(definition) = reference.definition {
            footnotes[definition].references.push(index);
        }
    }
    refs
}

fn visit_blocks(blocks: &[Block], found: &mut dyn FnMut(&str, u32)) {
    for block in blocks {
        let line = block.lines.start;
        match &block.kind {
            BlockKind::Heading { inlines, .. } | BlockKind::Paragraph(inlines) => {
                visit_inlines(inlines, line, found)
            }
            BlockKind::List { items, .. } => {
                for item in items {
                    visit_blocks(&item.blocks, found);
                }
            }
            BlockKind::Quote(inner) => visit_blocks(inner, found),
            BlockKind::Callout { title, blocks, .. } => {
                visit_inlines(title, line, found);
                visit_blocks(blocks, found);
            }
            BlockKind::Details {
                summary, blocks, ..
            } => {
                visit_inlines(summary, line, found);
                visit_blocks(blocks, found);
            }
            BlockKind::Tabs { tabs, .. } => {
                for tab in tabs {
                    visit_blocks(&tab.blocks, found);
                }
            }
            BlockKind::Table { head, rows, .. } => {
                for cell in head.iter().chain(rows.iter().flatten()) {
                    visit_inlines(cell, line, found);
                }
            }
            BlockKind::Image(image) => visit_caption(image, line, found),
            _ => {}
        }
    }
}

fn visit_inlines(inlines: &[Inline], line: u32, found: &mut dyn FnMut(&str, u32)) {
    for inline in inlines {
        match &inline.kind {
            InlineKind::FootnoteRef(label) => found(label, line),
            InlineKind::Strong(c)
            | InlineKind::Emphasis(c)
            | InlineKind::Strike(c)
            | InlineKind::Highlight(c) => visit_inlines(c, line, found),
            InlineKind::Link(link) => visit_inlines(&link.children, line, found),
            InlineKind::Image(image) => visit_caption(image, line, found),
            _ => {}
        }
    }
}

fn visit_caption(image: &Image, line: u32, found: &mut dyn FnMut(&str, u32)) {
    static REFERENCE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\[\^([^\]\s]+)\]").expect("footnote pattern"));
    if let Some(caption) = &image.caption {
        for captures in REFERENCE.captures_iter(caption) {
            found(&captures[1], line);
        }
    }
}

fn line_len(text: &str) -> usize {
    let end = text.find('\n').unwrap_or(text.len());
    end - usize::from(text[..end].ends_with('\r'))
}

fn cell_content(src: &str, cell: &Range<usize>) -> Range<usize> {
    let text = &src[cell.clone()];
    let trimmed = text.trim_matches([' ', '\t']);
    if trimmed.is_empty() {
        let at = cell.start + usize::from(text.starts_with([' ', '\t']));
        return at..at;
    }
    let start = cell.start + text.len() - text.trim_start_matches([' ', '\t']).len();
    start..start + trimmed.len()
}

fn is_blank_text(inline: &Inline) -> bool {
    matches!(&inline.kind, InlineKind::Text(t) if t.trim().is_empty())
}

fn span(inlines: &[Inline]) -> Option<Range<usize>> {
    Some(inlines.first()?.source.start..inlines.last()?.source.end)
}

fn split_media_lines(
    inlines: &[Inline],
    embeds: &[usize],
    lines: &Range<u32>,
    bytes: &Range<usize>,
) -> Option<Vec<Block>> {
    let mut blocks = Vec::new();
    let mut breaks = 0;
    for (i, inline) in inlines.iter().enumerate() {
        let kind = match &inline.kind {
            InlineKind::Image(image) => BlockKind::Image(image.clone()),
            InlineKind::Link(link) if embeds.contains(&i) => BlockKind::Embed {
                target: link.target.clone(),
                anchor: link.anchor.clone(),
            },
            InlineKind::SoftBreak | InlineKind::HardBreak => {
                breaks += 1;
                continue;
            }
            _ if is_blank_text(inline) => continue,
            _ => return None,
        };
        let line = lines.start + breaks;
        blocks.push(Block {
            lines: line..line + 1,
            bytes: inline.source.clone(),
            kind,
        });
    }
    if blocks.is_empty() {
        return None;
    }
    if blocks.len() == 1 {
        blocks[0].lines = lines.clone();
        blocks[0].bytes = bytes.clone();
    }
    Some(blocks)
}

fn is_url(s: &str) -> bool {
    s.contains("://") || s.starts_with("mailto:")
}

fn is_image_target(target: &str) -> bool {
    let name = target.split(['#', '^']).next().unwrap_or(target);
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| IMAGE_EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

fn wiki_destination(mut dest: String, with_alias: bool) -> String {
    if with_alias && dest.ends_with('\\') {
        dest.pop();
    }
    dest
}

fn split_anchor(dest: &str) -> (String, Option<String>) {
    if let Some((target, anchor)) = dest.split_once('#') {
        let anchor = (!anchor.is_empty()).then(|| anchor.to_string());
        return (target.to_string(), anchor);
    }
    match dest.find('^') {
        Some(i) if i > 0 => (dest[..i].to_string(), Some(dest[i..].to_string())),
        _ => (dest.to_string(), None),
    }
}

fn split_image_size(alt: &str) -> (String, Option<u32>, Option<u32>) {
    let Some((head, tail)) = alt.rsplit_once('|').or_else(|| {
        alt.chars()
            .all(|c| c.is_ascii_digit() || c == 'x')
            .then_some(("", alt))
    }) else {
        return (alt.to_string(), None, None);
    };
    let (w, h) = match tail.split_once('x') {
        Some((w, h)) => (w.parse::<u32>().ok(), h.parse::<u32>().ok()),
        None => (tail.parse::<u32>().ok(), None),
    };
    if w.is_none() {
        return (alt.to_string(), None, None);
    }
    (head.to_string(), w, h)
}

fn sub_text(inline: &Inline, piece: Range<usize>) -> Inline {
    let InlineKind::Text(text) = &inline.kind else {
        return inline.clone();
    };
    let source = if inline.exact() {
        inline.source.start + piece.start..inline.source.start + piece.end
    } else {
        inline.source.clone()
    };
    Inline::new(
        InlineKind::Text(text[piece].to_string()),
        source,
        inline.exact(),
    )
}

fn exact_text(inline: &Inline) -> Option<&str> {
    match &inline.kind {
        InlineKind::Text(text) if inline.exact() => Some(text),
        _ => None,
    }
}

fn take_block_id(inlines: &mut Vec<Inline>) {
    let Some(text) = inlines.last().and_then(exact_text) else {
        return;
    };
    let trimmed = text.trim_end();
    let start = trimmed
        .char_indices()
        .rfind(|&(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let token = &trimmed[start..];
    let Some(id) = token.strip_prefix('^') else {
        return;
    };
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return;
    }
    let preceded_ok = start > 0
        || inlines.len() == 1
        || matches!(
            inlines.get(inlines.len().wrapping_sub(2)).map(|i| &i.kind),
            Some(InlineKind::SoftBreak | InlineKind::HardBreak)
        );
    if !preceded_ok {
        return;
    }
    let id = id.to_string();
    let head_len = trimmed[..start].trim_end().len();
    let token_range = start..trimmed.len();
    let Some(last) = inlines.pop() else {
        return;
    };
    if head_len > 0 {
        inlines.push(sub_text(&last, 0..head_len));
    }
    let source = last.source.start + token_range.start..last.source.start + token_range.end;
    inlines.push(Inline::new(InlineKind::BlockId(id), source, false));
}

fn take_custom_task(blocks: &mut [Block]) -> Option<(char, usize)> {
    let Some(Block {
        kind: BlockKind::Paragraph(inlines),
        ..
    }) = blocks.first_mut()
    else {
        return None;
    };
    let first = inlines.first()?;
    let text = exact_text(first)?;
    let mut chars = text.chars();
    if chars.next() != Some('[') {
        return None;
    }
    let state = chars.next().filter(|c| !c.is_whitespace() && *c != ']')?;
    if chars.next() != Some(']') {
        return None;
    }
    let rest = chars.as_str();
    if !(rest.is_empty() || rest.starts_with(' ')) {
        return None;
    }
    let box_end = first.source.start + text.len() - rest.len();
    let kept = text.len() - rest.trim_start().len();
    let rest = sub_text(first, kept..text.len());
    if rest.source.is_empty() {
        inlines.remove(0);
    } else {
        inlines[0] = rest;
    }
    Some((state, box_end))
}

fn make_quote(mut blocks: Vec<Block>, lines: Range<u32>, bytes: Range<usize>) -> Block {
    let quote = |blocks: Vec<Block>| Block {
        lines: lines.clone(),
        bytes: bytes.clone(),
        kind: BlockKind::Quote(blocks),
    };
    let Some(Block {
        lines: first_lines,
        bytes: first_bytes,
        kind: BlockKind::Paragraph(inlines),
    }) = blocks.first()
    else {
        return quote(blocks);
    };
    let Some(first) = inlines.first() else {
        return quote(blocks);
    };
    let Some(text) = exact_text(first) else {
        return quote(blocks);
    };
    let Some(after_marker) = text.strip_prefix("[!") else {
        return quote(blocks);
    };
    let Some(close) = after_marker.find(']') else {
        return quote(blocks);
    };
    let kind = &after_marker[..close];
    if kind.is_empty()
        || !kind
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return quote(blocks);
    }
    let kind = kind.to_lowercase();
    let rest = &after_marker[close + 1..];
    let (fold, rest) = match rest.chars().next() {
        Some('+') => (Some(true), &rest[1..]),
        Some('-') => (Some(false), &rest[1..]),
        _ => (None, rest),
    };
    let marker_len = text.len() - rest.len();
    let marker = first.source.start..first.source.start + marker_len;
    let title_from = text.len() - rest.trim_start().len();
    let first_start = first_lines.start;
    let first_end = first_lines.end;
    let body_end = first_bytes.end;
    let mut title = Vec::new();
    let mut body = Vec::new();
    if title_from < text.len() {
        title.push(sub_text(first, title_from..text.len()));
    }
    let mut in_title = true;
    for inline in inlines.iter().skip(1) {
        if in_title && matches!(inline.kind, InlineKind::SoftBreak | InlineKind::HardBreak) {
            in_title = false;
        } else if in_title {
            title.push(inline.clone());
        } else {
            body.push(inline.clone());
        }
    }
    if let Some(last) = title.last_mut()
        && let InlineKind::Text(text) = &last.kind
    {
        let kept = text.trim_end().len();
        *last = sub_text(last, 0..kept);
    }
    blocks.remove(0);
    if let Some(body_start) = body.first().map(|i| i.source.start) {
        blocks.insert(
            0,
            Block {
                lines: first_start + 1..first_end,
                bytes: body_start..body_end,
                kind: BlockKind::Paragraph(body),
            },
        );
    }
    Block {
        lines,
        bytes,
        kind: BlockKind::Callout {
            kind,
            title,
            fold,
            blocks,
            marker,
        },
    }
}

fn expand_text(inlines: Vec<Inline>, escaped: &[usize]) -> Vec<Inline> {
    let wanted =
        |inline: &Inline| exact_text(inline).is_some_and(|t| t.contains('#') || t.contains("http"));
    if !inlines.iter().any(wanted) {
        return inlines;
    }
    let mut out = Vec::with_capacity(inlines.len() + 2);
    for inline in inlines {
        if wanted(&inline) {
            let after_break = matches!(
                out.last().map(|i: &Inline| &i.kind),
                None | Some(InlineKind::SoftBreak | InlineKind::HardBreak)
            );
            split_text(&inline, after_break, escaped, &mut out);
        } else {
            out.push(inline);
        }
    }
    out
}

fn split_text(inline: &Inline, start_ok: bool, escaped: &[usize], out: &mut Vec<Inline>) {
    let Some(text) = exact_text(inline) else {
        out.push(inline.clone());
        return;
    };
    let base = inline.source.start;
    let bytes = text.as_bytes();
    let mut plain_from = 0;
    let mut i = 0;
    while i < bytes.len() {
        let found = match bytes[i] {
            b'#' if escaped.binary_search(&(base + i)).is_err() => {
                let prev_ok = if i == 0 {
                    start_ok
                } else {
                    text[..i]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_whitespace() || c == '(')
                };
                if prev_ok {
                    tag_end(text, i).map(|end| (end, true))
                } else {
                    None
                }
            }
            b'h' => {
                let prev_ok = text[..i]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !c.is_alphanumeric());
                if prev_ok
                    && (text[i..].starts_with("http://") || text[i..].starts_with("https://"))
                {
                    url_end(text, i).map(|end| (end, false))
                } else {
                    None
                }
            }
            _ => None,
        };
        let Some((end, is_tag)) = found else {
            i += 1;
            continue;
        };
        if plain_from < i {
            out.push(sub_text(inline, plain_from..i));
        }
        let source = base + i..base + end;
        if is_tag {
            let tag = InlineKind::Tag(text[i + 1..end].to_string());
            out.push(Inline::new(tag, source, false));
        } else {
            let url = text[i..end].to_string();
            let link = Link {
                kind: LinkKind::Auto,
                target: url.clone(),
                anchor: None,
                children: vec![sub_text(inline, i..end)],
            };
            out.push(Inline::new(InlineKind::Link(link), source, false));
        }
        plain_from = end;
        i = end;
    }
    if plain_from < text.len() {
        out.push(sub_text(inline, plain_from..text.len()));
    }
}

fn tag_end(text: &str, start: usize) -> Option<usize> {
    let body = &text[start + 1..];
    let len = body
        .char_indices()
        .find(|&(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '/')))
        .map_or(body.len(), |(i, _)| i);
    let tag = body[..len].trim_end_matches('/');
    if tag.is_empty() || tag.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(start + 1 + tag.len())
}

fn url_end(text: &str, start: usize) -> Option<usize> {
    let rest = &text[start..];
    let mut end = rest
        .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`'))
        .unwrap_or(rest.len());
    loop {
        let url = &rest[..end];
        let Some(last) = url.chars().next_back() else {
            break;
        };
        let opens = |open: char| url.matches(open).count();
        let strip = match last {
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '*' | '_' => true,
            ')' => opens(')') > opens('('),
            ']' => opens(']') > opens('['),
            _ => false,
        };
        if !strip {
            break;
        }
        end -= last.len_utf8();
    }
    let scheme = if rest.starts_with("https") { 8 } else { 7 };
    (end > scheme).then_some(start + end)
}

fn highlight(list: Vec<Inline>, escaped: &[usize]) -> Vec<Inline> {
    if !list
        .iter()
        .any(|i| exact_text(i).is_some_and(|t| t.contains("==")))
    {
        return list;
    }
    let mut done = Vec::new();
    let mut rest = list;
    loop {
        let Some((i, p)) = find_opener(&rest, escaped) else {
            done.extend(rest);
            return done;
        };
        let Some((j, q)) = find_closer(&rest, i, p + 2, escaped) else {
            done.extend(rest);
            return done;
        };
        let mut items = rest.into_iter();
        done.extend(items.by_ref().take(i));
        let Some(first) = items.next() else {
            return done;
        };
        let first_len = exact_text(&first).map_or(0, str::len);
        if p > 0 {
            done.push(sub_text(&first, 0..p));
        }
        let mut inner = Vec::new();
        let (tail, end) = if i == j {
            inner.push(sub_text(&first, p + 2..q));
            (
                sub_text(&first, q + 2..first_len),
                first.source.start + q + 2,
            )
        } else {
            inner.push(sub_text(&first, p + 2..first_len));
            inner.extend(items.by_ref().take(j - i - 1));
            let Some(last) = items.next() else {
                return done;
            };
            let last_len = exact_text(&last).map_or(0, str::len);
            inner.push(sub_text(&last, 0..q));
            (sub_text(&last, q + 2..last_len), last.source.start + q + 2)
        };
        inner.retain(|x| !matches!(&x.kind, InlineKind::Text(t) if t.is_empty()));
        let source = first.source.start + p..end;
        done.push(Inline::new(InlineKind::Highlight(inner), source, false));
        rest = Vec::new();
        if !tail.source.is_empty() {
            rest.push(tail);
        }
        rest.extend(items);
    }
}

fn is_escaped(escaped: &[usize], inline: &Inline, at: usize) -> bool {
    let at = inline.source.start + at;
    escaped.binary_search(&at).is_ok() || escaped.binary_search(&(at + 1)).is_ok()
}

fn find_opener(list: &[Inline], escaped: &[usize]) -> Option<(usize, usize)> {
    for (i, inline) in list.iter().enumerate() {
        let Some(text) = exact_text(inline) else {
            continue;
        };
        let mut from = 0;
        while let Some(off) = text[from..].find("==") {
            let p = from + off;
            let after = &text[p + 2..];
            let ok = match after.chars().next() {
                Some(c) => !c.is_whitespace() && c != '=',
                None => i + 1 < list.len(),
            };
            if ok && !is_escaped(escaped, inline, p) {
                return Some((i, p));
            }
            from = p + 2;
        }
    }
    None
}

fn find_closer(
    list: &[Inline],
    i: usize,
    from: usize,
    escaped: &[usize],
) -> Option<(usize, usize)> {
    for (j, inline) in list.iter().enumerate().skip(i) {
        let Some(text) = exact_text(inline) else {
            continue;
        };
        let mut at = if j == i { from } else { 0 };
        while let Some(off) = text[at..].find("==") {
            let q = at + off;
            let before = text[..q].chars().next_back();
            let ok = match before {
                Some(c) => !c.is_whitespace() && c != '=',
                None => j > i,
            };
            let empty = j == i && q == from;
            if ok && !empty && !is_escaped(escaped, inline, q) {
                return Some((j, q));
            }
            at = q + 2;
        }
    }
    None
}

pub(crate) struct Lines<'a> {
    pub src: &'a str,
    pub starts: &'a [usize],
    pub options: &'a ParseOptions,
}

impl Lines<'_> {
    fn origin(&self, block: &Block, html: &str) -> Origin {
        Origin::of_lines(self.src, self.starts, block.lines.start, html)
    }
}

fn group_details(blocks: Vec<Block>, lines: &Lines) -> Vec<Block> {
    let blocks: Vec<Block> = blocks
        .into_iter()
        .map(|mut block| {
            match &mut block.kind {
                BlockKind::Quote(inner) => *inner = group_details(std::mem::take(inner), lines),
                BlockKind::Callout { blocks: inner, .. } => {
                    *inner = group_details(std::mem::take(inner), lines)
                }
                BlockKind::List { items, .. } => {
                    for item in items {
                        item.blocks = group_details(std::mem::take(&mut item.blocks), lines);
                    }
                }
                _ => {}
            }
            block
        })
        .collect();
    let mut out = Vec::with_capacity(blocks.len());
    let mut i = 0;
    while i < blocks.len() {
        if let BlockKind::Html(html) = &blocks[i].kind
            && html
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("<details")
            && let Some((details, consumed)) = build_details(&blocks, i, lines)
        {
            out.push(details);
            i += consumed;
            continue;
        }
        out.push(blocks[i].clone());
        i += 1;
    }
    out
}

fn count_tags(html: &str) -> (usize, usize) {
    let lower = html.to_ascii_lowercase();
    (
        lower.matches("<details").count(),
        lower.matches("</details>").count(),
    )
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(needle)
}

fn blocks_of(text: &str, first_line: u32, origin: &Origin, options: &ParseOptions) -> Vec<Block> {
    let mut blocks = crate::parse::parse(text, options).blocks;
    remap_blocks(&mut blocks, origin);
    shift_lines(&mut blocks, first_line);
    blocks
}

pub(crate) fn shift_lines(blocks: &mut [Block], delta: u32) {
    for block in blocks {
        block.lines = block.lines.start + delta..block.lines.end + delta;
        match &mut block.kind {
            BlockKind::Tabs { tabs, .. } => {
                for tab in tabs {
                    tab.lines = tab.lines.start + delta..tab.lines.end + delta;
                    tab.title_line += delta;
                    shift_lines(&mut tab.blocks, delta);
                }
            }
            BlockKind::Quote(inner)
            | BlockKind::Callout { blocks: inner, .. }
            | BlockKind::Details { blocks: inner, .. } => shift_lines(inner, delta),
            BlockKind::List { items, .. } => {
                for item in items {
                    item.lines = item.lines.start + delta..item.lines.end + delta;
                    shift_lines(&mut item.blocks, delta);
                }
            }
            _ => {}
        }
    }
}

fn summary_inlines(text: &str, origin: &Origin, options: &ParseOptions) -> Vec<Inline> {
    let lead = text.len() - text.trim_start().len();
    match crate::parse::parse(text.trim(), options)
        .blocks
        .into_iter()
        .next()
        .map(|b| b.kind)
    {
        Some(BlockKind::Paragraph(mut inlines)) => {
            remap_inlines(&mut inlines, &origin.slice(lead));
            inlines
        }
        _ => Vec::new(),
    }
}

fn build_details(blocks: &[Block], start: usize, lines: &Lines) -> Option<(Block, usize)> {
    let BlockKind::Html(html) = &blocks[start].kind else {
        return None;
    };
    let options = lines.options;
    let origin = lines.origin(&blocks[start], html);
    let open_at = find_ci(html, "<details")?;
    let tag_end = open_at + html[open_at..].find('>')?;
    let open = html[open_at + 8..tag_end]
        .to_ascii_lowercase()
        .split_whitespace()
        .any(|a| a.starts_with("open"));
    let first_line = blocks[start].lines.start;
    let line_of = |index: usize| first_line + html[..index].matches('\n').count() as u32;
    let mut summary = Vec::new();
    let mut rest_start = tag_end + 1;
    let after_tag = html[rest_start..].trim_start();
    if after_tag.to_ascii_lowercase().starts_with("<summary") {
        let summary_open = html.len() - after_tag.len();
        let content_start = summary_open + after_tag.find('>')? + 1;
        let close = content_start + find_ci(&html[content_start..], "</summary>")?;
        summary = summary_inlines(
            &html[content_start..close],
            &origin.slice(content_start),
            options,
        );
        rest_start = close + "</summary>".len();
    }
    let rest = &html[rest_start..];
    let (opens, closes) = count_tags(rest);
    let mut depth = 1 + opens as isize - closes as isize;
    if depth <= 0 {
        let close_at = rest_start + rest.to_ascii_lowercase().rfind("</details>")?;
        let trailing = &html[close_at + "</details>".len()..];
        if !trailing.trim().is_empty() {
            return None;
        }
        let inner = &html[rest_start..close_at];
        let inner_blocks = if inner.trim().is_empty() {
            Vec::new()
        } else {
            let at = rest_start + inner.len() - inner.trim_start().len();
            blocks_of(inner.trim(), line_of(at), &origin.slice(at), options)
        };
        let block = Block {
            lines: blocks[start].lines.clone(),
            bytes: blocks[start].bytes.clone(),
            kind: BlockKind::Details {
                summary,
                open,
                blocks: inner_blocks,
            },
        };
        return Some((block, 1));
    }
    let mut inner: Vec<Block> = Vec::new();
    if !rest.trim().is_empty() {
        let at = rest_start + rest.len() - rest.trim_start().len();
        inner.extend(blocks_of(
            rest.trim(),
            line_of(at),
            &origin.slice(at),
            options,
        ));
    }
    let mut j = start + 1;
    if summary.is_empty()
        && rest.trim().is_empty()
        && let Some(
            block @ Block {
                kind: BlockKind::Html(next),
                ..
            },
        ) = blocks.get(j)
    {
        let lower = next.trim().to_ascii_lowercase();
        if lower.starts_with("<summary") && lower.ends_with("</summary>") {
            let content_start = next.find('>')? + 1;
            let close = next.to_ascii_lowercase().rfind("</summary>")?;
            let origin = lines.origin(block, next).slice(content_start);
            summary = summary_inlines(&next[content_start..close], &origin, options);
            j += 1;
        }
    }
    while j < blocks.len() {
        if let BlockKind::Html(text) = &blocks[j].kind {
            let (o, c) = count_tags(text);
            depth += o as isize - c as isize;
            if depth <= 0 {
                if !text
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("</details>")
                    || !text.trim()["</details>".len()..].trim().is_empty()
                {
                    return None;
                }
                let inner = group_details(inner, lines);
                return Some((
                    Block {
                        lines: blocks[start].lines.start..blocks[j].lines.end,
                        bytes: blocks[start].bytes.start..blocks[j].bytes.end,
                        kind: BlockKind::Details {
                            summary,
                            open,
                            blocks: inner,
                        },
                    },
                    j - start + 1,
                ));
            }
        }
        inner.push(blocks[j].clone());
        j += 1;
    }
    None
}

pub(crate) fn remap_blocks(blocks: &mut [Block], origin: &Origin) {
    for block in blocks {
        block.bytes = origin.range(&block.bytes);
        match &mut block.kind {
            BlockKind::Heading {
                inlines, marker, ..
            } => {
                *marker = origin.range(marker);
                remap_inlines(inlines, origin);
            }
            BlockKind::Paragraph(inlines) => remap_inlines(inlines, origin),
            BlockKind::List { items, .. } => {
                for item in items {
                    item.marker = origin.range(&item.marker);
                    remap_blocks(&mut item.blocks, origin);
                }
            }
            BlockKind::CodeBlock { content, .. } => *content = origin.range(content),
            BlockKind::Quote(inner) => remap_blocks(inner, origin),
            BlockKind::Callout {
                title,
                blocks: inner,
                marker,
                ..
            } => {
                *marker = origin.range(marker);
                remap_inlines(title, origin);
                remap_blocks(inner, origin);
            }
            BlockKind::Table {
                head, rows, cells, ..
            } => {
                for cell in head.iter_mut().chain(rows.iter_mut().flatten()) {
                    remap_inlines(cell, origin);
                }
                for range in cells.iter_mut().flatten() {
                    *range = origin.range(range);
                }
            }
            BlockKind::Details {
                summary,
                blocks: inner,
                ..
            } => {
                remap_inlines(summary, origin);
                remap_blocks(inner, origin);
            }
            BlockKind::Tabs { tabs, .. } => {
                for tab in tabs {
                    remap_blocks(&mut tab.blocks, origin);
                }
            }
            BlockKind::Mermaid { .. }
            | BlockKind::Math { .. }
            | BlockKind::Embed { .. }
            | BlockKind::Image(_)
            | BlockKind::Rule
            | BlockKind::Html(_)
            | BlockKind::Directive(_) => {}
        }
    }
}

pub(crate) fn remap_inlines(inlines: &mut Vec<Inline>, origin: &Origin) {
    let cut = |inline: &Inline| inline.exact() && origin.breaks(&inline.source).next().is_some();
    if inlines.iter().any(cut) {
        let mut pieces = Vec::with_capacity(inlines.len() + 1);
        for inline in inlines.drain(..) {
            if !cut(&inline) {
                pieces.push(inline);
                continue;
            }
            let start = inline.source.start;
            let mut from = start;
            for at in origin
                .breaks(&inline.source)
                .chain(std::iter::once(inline.source.end))
            {
                pieces.push(sub_text(&inline, from - start..at - start));
                from = at;
            }
        }
        *inlines = pieces;
    }
    for inline in inlines {
        let exact = inline.exact() && origin.exact(&inline.source);
        inline.source = origin.range(&inline.source);
        match &mut inline.kind {
            InlineKind::Strong(children)
            | InlineKind::Emphasis(children)
            | InlineKind::Strike(children)
            | InlineKind::Highlight(children) => remap_inlines(children, origin),
            InlineKind::Link(link) => remap_inlines(&mut link.children, origin),
            _ => {}
        }
        inline.exact = exact;
    }
}

fn is_comment_only(html: &str) -> bool {
    let mut rest = html.trim();
    if !rest.starts_with("<!--") {
        return false;
    }
    while let Some(after) = rest.strip_prefix("<!--") {
        let Some(end) = after.find("-->") else {
            return false;
        };
        rest = after[end + 3..].trim_start();
    }
    rest.is_empty()
}

const KNOWN_HTML_TAGS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "area",
    "article",
    "aside",
    "audio",
    "b",
    "base",
    "bdi",
    "bdo",
    "blockquote",
    "body",
    "br",
    "button",
    "canvas",
    "caption",
    "center",
    "cite",
    "code",
    "col",
    "colgroup",
    "data",
    "datalist",
    "dd",
    "del",
    "details",
    "dfn",
    "dialog",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "fieldset",
    "figcaption",
    "figure",
    "font",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "i",
    "iframe",
    "img",
    "input",
    "ins",
    "kbd",
    "label",
    "legend",
    "li",
    "link",
    "main",
    "map",
    "mark",
    "marquee",
    "menu",
    "meta",
    "meter",
    "nav",
    "noscript",
    "object",
    "ol",
    "optgroup",
    "option",
    "output",
    "p",
    "param",
    "picture",
    "pre",
    "progress",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "script",
    "section",
    "select",
    "small",
    "source",
    "span",
    "strike",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "svg",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "track",
    "tt",
    "u",
    "ul",
    "var",
    "video",
    "wbr",
    "xmp",
];

fn is_known_html_tag(tag: &str) -> bool {
    let inner = tag.trim_start_matches('<').trim_start_matches('/');
    let name: String = inner
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_ascii_lowercase();
    KNOWN_HTML_TAGS.contains(&name.as_str())
}

fn attr_image(attrs: &[(String, String)]) -> Option<Image> {
    let src = crate::html::attr(attrs, "src")
        .filter(|s| !s.is_empty())?
        .to_string();
    let size = |key: &str| {
        crate::html::attr(attrs, key)
            .and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok())
            .filter(|v| *v > 0.0)
            .map(|v| v.round() as u32)
    };
    Some(Image {
        src,
        alt: crate::html::attr(attrs, "alt").unwrap_or("").to_string(),
        caption: None,
        width: size("width"),
        height: size("height"),
    })
}
