use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use crate::origin::Origin;
use crate::parse::{parse_raw, shift_lines};
use crate::{
    Block, BlockKind, Directive, Footnote, Inline, InlineKind, Link, LinkKind, ParseOptions, Tab,
    TabsForm,
};

#[derive(Clone, Copy)]
struct Fence {
    ch: u8,
    len: usize,
}

pub(crate) fn bare(line: &str) -> &str {
    let mut rest = line;
    loop {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        match trimmed.strip_prefix('>') {
            Some(after) => rest = after,
            None => return trimmed,
        }
    }
}

fn open_fence(line: &str) -> Option<Fence> {
    let text = bare(line);
    let ch = *text.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = text.bytes().take_while(|&b| b == ch).count();
    (len >= 3 && !(ch == b'`' && text[len..].contains('`'))).then_some(Fence { ch, len })
}

fn closes(line: &str, fence: Fence) -> bool {
    let text = bare(line);
    let run = text.bytes().take_while(|&b| b == fence.ch).count();
    run >= fence.len && text[run..].trim().is_empty()
}

fn directive_line(line: &str) -> Option<(&str, &str)> {
    let inner = line.trim().strip_prefix("{{#")?.strip_suffix("}}")?;
    let name_len = inner
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        .count();
    (name_len > 0 && !inner.contains("}}")).then(|| (&inner[..name_len], inner[name_len..].trim()))
}

fn tab_title(args: &str) -> String {
    static NAME: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"name\s*=\s*(?:"([^"]*)"|'([^']*)'|(\S+))"#).expect("tab name pattern")
    });
    match NAME.captures(args) {
        Some(c) => c
            .get(1)
            .or_else(|| c.get(2))
            .or_else(|| c.get(3))
            .map_or("", |m| m.as_str())
            .trim()
            .to_string(),
        None => args.trim().to_string(),
    }
}

pub(crate) struct Region {
    pub lines: Range<u32>,
    pub tabs: Vec<RegionTab>,
}

pub(crate) struct RegionTab {
    pub title: String,
    pub lines: Range<u32>,
    pub title_line: u32,
    pub body: Range<u32>,
}

pub(crate) fn find_regions(src: &str) -> Vec<Region> {
    if !src.contains("{{#tabs") {
        return Vec::new();
    }
    let lines: Vec<&str> = src.split('\n').collect();
    let mut regions = Vec::new();
    let mut fence: Option<Fence> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(open) = fence {
            if closes(line, open) {
                fence = None;
            }
        } else if let Some(open) = open_fence(line) {
            fence = Some(open);
        } else if matches!(directive_line(line), Some(("tabs", _)))
            && let Some(region) = scan_region(&lines, i)
        {
            i = region.lines.end as usize;
            regions.push(region);
            continue;
        }
        i += 1;
    }
    regions
}

fn scan_region(lines: &[&str], start: usize) -> Option<Region> {
    struct Open {
        title: String,
        marker: usize,
        body: usize,
    }
    let close = |tabs: &mut Vec<RegionTab>, open: Open, end: usize, body_end: usize| {
        tabs.push(RegionTab {
            title: open.title,
            lines: open.marker as u32..end as u32,
            title_line: open.marker as u32,
            body: open.body as u32..body_end as u32,
        });
    };
    let (mut depth, mut fence) = (1, None::<Fence>);
    let (mut tabs, mut current) = (Vec::new(), None::<Open>);
    for (j, line) in lines.iter().enumerate().skip(start + 1) {
        if let Some(open) = fence {
            if closes(line, open) {
                fence = None;
            }
            continue;
        }
        if let Some(open) = open_fence(line) {
            fence = Some(open);
            if depth == 1 && current.is_none() {
                current = Some(Open {
                    title: String::new(),
                    marker: j,
                    body: j,
                });
            }
            continue;
        }
        match directive_line(line) {
            Some(("tabs", _)) => depth += 1,
            Some(("endtabs", _)) => {
                depth -= 1;
                if depth == 0 {
                    if let Some(open) = current.take() {
                        close(&mut tabs, open, j, j);
                    }
                    return Some(Region {
                        lines: start as u32..j as u32 + 1,
                        tabs,
                    });
                }
            }
            Some(("tab", args)) if depth == 1 => {
                if let Some(open) = current.take() {
                    close(&mut tabs, open, j, j);
                }
                current = Some(Open {
                    title: tab_title(args),
                    marker: j,
                    body: j + 1,
                });
            }
            Some(("endtab", _)) if depth == 1 => {
                if let Some(open) = current.take() {
                    close(&mut tabs, open, j + 1, j);
                }
            }
            _ if depth == 1 && current.is_none() && !line.trim().is_empty() => {
                current = Some(Open {
                    title: String::new(),
                    marker: j,
                    body: j,
                })
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn mask(src: &str, regions: &[Region]) -> (String, Origin) {
    let mut origin = Origin::shift(0);
    if regions.is_empty() {
        return (src.to_string(), origin);
    }
    let mut out = String::with_capacity(src.len());
    let mut at = 0;
    for (index, line) in src.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        origin.push(out.len(), at, true);
        if !regions.iter().any(|r| r.lines.contains(&(index as u32))) {
            out.push_str(line);
        }
        at += line.len() + 1;
    }
    (out, origin)
}

fn fenced_tabs(code: &str, first_line: u32) -> Option<Vec<FencedTab>> {
    let mut tabs: Vec<FencedTab> = Vec::new();
    let mut fence: Option<Fence> = None;
    let mut saw_title = false;
    let end_of_content = first_line + code.lines().count() as u32;
    for (index, line) in code.lines().enumerate() {
        let n = first_line + index as u32;
        if fence.is_none()
            && let Some(title) = line
                .strip_prefix("tab:")
                .or_else(|| line.strip_prefix("== ").filter(|t| !t.trim().is_empty()))
        {
            if let Some(last) = tabs.last_mut() {
                last.end = n;
            }
            tabs.push(FencedTab {
                title: title.trim().to_string(),
                title_line: n,
                body_first: n + 1,
                body: Vec::new(),
                end: end_of_content,
            });
            saw_title = true;
            continue;
        }
        match fence {
            Some(open) if closes(line, open) => fence = None,
            Some(_) => {}
            None => fence = open_fence(line),
        }
        if tabs.is_empty() {
            if line.trim().is_empty() {
                continue;
            }
            tabs.push(FencedTab {
                title: String::new(),
                title_line: n,
                body_first: n,
                body: Vec::new(),
                end: end_of_content,
            });
        }
        if let Some(last) = tabs.last_mut() {
            last.body.push(line.to_string());
        }
    }
    saw_title.then_some(tabs)
}

struct FencedTab {
    title: String,
    title_line: u32,
    body_first: u32,
    body: Vec<String>,
    end: u32,
}

pub(crate) struct Tabber<'o> {
    pub options: &'o ParseOptions,
    pub footnotes: Vec<Footnote>,
    pub source: &'o str,
    pub line_starts: Vec<usize>,
}

impl Tabber<'_> {
    fn content(&mut self, text: &str, first_line: u32) -> Vec<Block> {
        let mut raw = parse_raw(text, self.options, false);
        raw.remap(&Origin::of_lines(
            self.source,
            &self.line_starts,
            first_line,
            text,
        ));
        shift_lines(&mut raw.blocks, first_line);
        for footnote in &mut raw.footnotes {
            shift_lines(&mut footnote.blocks, first_line);
        }
        self.footnotes.extend(raw.footnotes);
        raw.blocks
    }

    pub fn convert(&mut self, blocks: Vec<Block>) -> Vec<Block> {
        blocks
            .into_iter()
            .map(|mut block| {
                match &mut block.kind {
                    BlockKind::Quote(inner)
                    | BlockKind::Callout { blocks: inner, .. }
                    | BlockKind::Details { blocks: inner, .. } => {
                        *inner = self.convert(std::mem::take(inner))
                    }
                    BlockKind::List { items, .. } => {
                        for item in items {
                            item.blocks = self.convert(std::mem::take(&mut item.blocks));
                        }
                    }
                    BlockKind::CodeBlock { lang, code, .. } if lang == "tabs" => {
                        if let Some(found) = fenced_tabs(code, block.lines.start + 1) {
                            let tabs = found
                                .into_iter()
                                .map(|t| Tab {
                                    title: t.title,
                                    lines: t.title_line..t.end,
                                    title_line: t.title_line,
                                    blocks: self.content(&t.body.join("\n"), t.body_first),
                                })
                                .collect();
                            block.kind = BlockKind::Tabs {
                                form: TabsForm::Fenced,
                                tabs,
                            };
                        }
                    }
                    _ => {}
                }
                block
            })
            .collect()
    }

    pub fn insert(&mut self, mut blocks: Vec<Block>, regions: Vec<Region>) -> Vec<Block> {
        let lines: Vec<&str> = self.source.split('\n').collect();
        for region in regions {
            let first = self.line_starts[region.lines.start as usize];
            let last = region.lines.end as usize - 1;
            let bytes = first + lines[region.lines.start as usize].len()
                - bare(lines[region.lines.start as usize]).len()
                ..self.line_starts[last] + lines[last].trim_end().len();
            let tabs = region
                .tabs
                .into_iter()
                .map(|t| {
                    let text = lines[t.body.start as usize..(t.body.end as usize).min(lines.len())]
                        .join("\n");
                    Tab {
                        title: t.title,
                        lines: t.lines,
                        title_line: t.title_line,
                        blocks: self.content(&text, t.body.start),
                    }
                })
                .collect();
            let at = blocks
                .iter()
                .position(|b| b.lines.start >= region.lines.start)
                .unwrap_or(blocks.len());
            blocks.insert(
                at,
                Block {
                    lines: region.lines,
                    bytes,
                    kind: BlockKind::Tabs {
                        form: TabsForm::Mdbook,
                        tabs,
                    },
                },
            );
        }
        blocks
    }
}

pub(crate) enum Piece {
    Text {
        first: usize,
        text: String,
    },
    Directive {
        first: usize,
        last: usize,
        directive: Directive,
    },
}

pub(crate) fn split_directives(paragraph: &str) -> Option<Vec<Piece>> {
    static INLINE_REF: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^\{\{#ref\s*\}\}(.*)\{\{#endref\s*\}\}$").expect("ref pattern")
    });
    let lines: Vec<&str> = paragraph
        .split('\n')
        .enumerate()
        .map(|(i, l)| if i == 0 { l.trim() } else { bare(l).trim_end() })
        .collect();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut found = false;
    let mut text: Option<(usize, Vec<&str>)> = None;
    let mut i = 0;
    let flush = |text: &mut Option<(usize, Vec<&str>)>, pieces: &mut Vec<Piece>| {
        if let Some((first, collected)) = text.take() {
            pieces.push(Piece::Text {
                first,
                text: collected.join("\n"),
            });
        }
    };
    while i < lines.len() {
        let line = lines[i];
        let single = INLINE_REF.captures(line).map(|c| Directive::Ref {
            path: c[1].trim().to_string(),
        });
        let directive = if single.is_some() {
            single.map(|d| (d, i))
        } else if let Some((name, args)) = directive_line(line) {
            let closing = format!("end{name}");
            let closer = (matches!(name, "ref" | "file") && args.is_empty())
                .then(|| {
                    (i + 1..lines.len()).find(
                        |&j| matches!(directive_line(lines[j]), Some((n, "")) if n == closing),
                    )
                })
                .flatten();
            Some(match (name, closer) {
                ("ref", Some(end)) => (
                    Directive::Ref {
                        path: lines[i + 1..end]
                            .iter()
                            .map(|l| l.trim())
                            .filter(|l| !l.is_empty())
                            .collect::<Vec<_>>()
                            .join(" "),
                    },
                    end,
                ),
                ("file", Some(end)) => (
                    Directive::Other {
                        name: name.to_string(),
                        text: lines[i + 1..end]
                            .iter()
                            .map(|l| l.trim())
                            .filter(|l| !l.is_empty())
                            .collect::<Vec<_>>()
                            .join(" "),
                    },
                    end,
                ),
                ("include", _) => (
                    Directive::Include {
                        path: args.to_string(),
                    },
                    i,
                ),
                _ => (
                    Directive::Other {
                        name: name.to_string(),
                        text: args.to_string(),
                    },
                    i,
                ),
            })
        } else {
            None
        };
        match directive {
            Some((directive, last)) => {
                flush(&mut text, &mut pieces);
                pieces.push(Piece::Directive {
                    first: i,
                    last,
                    directive,
                });
                found = true;
                i = last + 1;
            }
            None => {
                text.get_or_insert((i, Vec::new())).1.push(line);
                i += 1;
            }
        }
    }
    flush(&mut text, &mut pieces);
    found.then_some(pieces)
}

pub(crate) fn inline_refs(inlines: Vec<Inline>) -> Vec<Inline> {
    static REF: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\{\{#ref\s*\}\}(.*?)\{\{#endref\s*\}\}").expect("ref pattern")
    });
    let mut out = Vec::with_capacity(inlines.len());
    for inline in inlines {
        match &inline.kind {
            InlineKind::Text(text) if inline.exact() && text.contains("{{#ref") => {
                let base = inline.source.start;
                let piece = |range: std::ops::Range<usize>| {
                    let source = base + range.start..base + range.end;
                    Inline::new(InlineKind::Text(text[range].to_string()), source, true)
                };
                let mut last = 0;
                for found in REF.captures_iter(text) {
                    let whole = found.get(0).expect("whole match");
                    if whole.start() > last {
                        out.push(piece(last..whole.start()));
                    }
                    let path_bytes = found.get(1).expect("path group").range();
                    let path = found[1].trim();
                    let (target, anchor) = path
                        .split_once('#')
                        .map_or((path, None), |(t, a)| (t, Some(a.to_string())));
                    let shown = target
                        .rsplit('/')
                        .next()
                        .unwrap_or(target)
                        .trim_end_matches(".md")
                        .to_string();
                    let link = Link {
                        kind: LinkKind::Markdown,
                        target: target.to_string(),
                        anchor,
                        children: vec![Inline::new(
                            InlineKind::Text(shown),
                            base + path_bytes.start..base + path_bytes.end,
                            false,
                        )],
                    };
                    let source = base + whole.start()..base + whole.end();
                    out.push(Inline::new(InlineKind::Link(link), source, false));
                    last = whole.end();
                }
                if last < text.len() {
                    out.push(piece(last..text.len()));
                }
            }
            _ => out.push(inline),
        }
    }
    out
}
