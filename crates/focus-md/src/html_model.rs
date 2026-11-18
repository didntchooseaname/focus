use std::ops::Range;

use crate::html::{Token, attr, decode_entities, plain, tokens};
use crate::origin::Origin;
use crate::parse::remap_inlines;
use crate::{Align, Block, BlockKind, Image, Inline, InlineKind, ParseOptions};

pub(crate) fn block_to_model(
    html: &str,
    origin: &Origin,
    lines: &Range<u32>,
    bytes: &Range<usize>,
    options: &ParseOptions,
) -> Option<Vec<Block>> {
    let lead = html.len() - html.trim_start().len();
    let origin = &origin.slice(lead);
    let trimmed = html.trim();
    let lower = trimmed
        .get(..8.min(trimmed.len()))
        .unwrap_or("")
        .to_ascii_lowercase();
    let kind = if lower.starts_with("<figure") {
        figure(trimmed)?
    } else if lower.starts_with("<img")
        || lower.starts_with("<a ")
        || lower.starts_with("<p>")
        || lower.starts_with("<p ")
        || lower.starts_with("<center")
    {
        images_only(trimmed)?
    } else if lower.starts_with("<pre") {
        vec![preformatted(trimmed, origin)?]
    } else if lower.starts_with("<table") {
        vec![table(trimmed, origin, options)?]
    } else {
        return None;
    };
    Some(
        kind.into_iter()
            .map(|kind| Block {
                lines: lines.clone(),
                bytes: bytes.clone(),
                kind,
            })
            .collect(),
    )
}

fn dimension(value: Option<&str>) -> Option<u32> {
    let value = value?.trim().trim_end_matches("px");
    value
        .parse::<f32>()
        .ok()
        .filter(|v| *v > 0.0)
        .map(|v| v.round() as u32)
}

fn image_from(attrs: &[(String, String)], caption: Option<String>) -> Option<Image> {
    let src = attr(attrs, "src")?.to_string();
    if src.is_empty() {
        return None;
    }
    let alt = attr(attrs, "alt").unwrap_or("").to_string();
    let alt = if alt.is_empty() {
        caption.clone().unwrap_or_default()
    } else {
        alt
    };
    Some(Image {
        src,
        alt,
        caption,
        width: dimension(attr(attrs, "width")),
        height: dimension(attr(attrs, "height")),
    })
}

fn figure(html: &str) -> Option<Vec<BlockKind>> {
    let mut image_attrs: Option<Vec<(String, String)>> = None;
    let mut caption = String::new();
    let mut in_caption = false;
    let mut closed = false;
    for (_, token) in tokens(html) {
        match token {
            Token::Open { name, attrs, .. } => match name.as_str() {
                "img" if image_attrs.is_none() => image_attrs = Some(attrs),
                "img" => return None,
                "figcaption" => in_caption = true,
                "figure" | "a" | "p" | "picture" | "source" | "br" | "span" | "center" | "div" => {}
                _ => return None,
            },
            Token::Close { name } => match name.as_str() {
                "figcaption" => in_caption = false,
                "figure" => closed = true,
                _ => {}
            },
            Token::Text(text) if in_caption => {
                caption.push(' ');
                caption.push_str(&decode_entities(text));
            }
            Token::Text(text) => {
                if !text.trim().is_empty() {
                    return None;
                }
            }
            Token::Comment => {}
        }
    }
    let attrs = image_attrs.filter(|_| closed)?;
    let caption = caption.split_whitespace().collect::<Vec<_>>().join(" ");
    let image = image_from(&attrs, (!caption.is_empty()).then_some(caption))?;
    Some(vec![BlockKind::Image(image)])
}

fn images_only(html: &str) -> Option<Vec<BlockKind>> {
    let mut images = Vec::new();
    for (_, token) in tokens(html) {
        match token {
            Token::Open { name, attrs, .. } => match name.as_str() {
                "img" => images.push(BlockKind::Image(image_from(&attrs, None)?)),
                "a" | "p" | "center" | "br" | "div" => {}
                _ => return None,
            },
            Token::Close { .. } | Token::Comment => {}
            Token::Text(text) => {
                if !text.trim().is_empty() {
                    return None;
                }
            }
        }
    }
    (!images.is_empty()).then_some(images)
}

fn preformatted(html: &str, origin: &Origin) -> Option<BlockKind> {
    let mut lang = String::new();
    let mut code = String::new();
    let mut depth = 0;
    let mut finished = false;
    let mut text: Option<Range<usize>> = None;
    for (range, token) in tokens(html) {
        if finished {
            if matches!(token, Token::Text(t) if t.trim().is_empty()) {
                continue;
            }
            return None;
        }
        match token {
            Token::Open { name, attrs, .. } => match name.as_str() {
                "pre" => depth += 1,
                "code" => {
                    if let Some(class) = attr(&attrs, "class")
                        && let Some(found) = class.split_whitespace().find_map(|c| {
                            c.strip_prefix("language-")
                                .or_else(|| c.strip_prefix("lang-"))
                        })
                    {
                        lang = found.to_lowercase();
                    }
                }
                "br" => code.push('\n'),
                _ => {}
            },
            Token::Close { name } if name == "pre" => {
                depth -= 1;
                finished = depth == 0;
            }
            Token::Text(t) => {
                code.push_str(&decode_entities(t));
                text = Some(text.map_or(range.clone(), |r| r.start..range.end));
            }
            _ => {}
        }
    }
    if !finished {
        return None;
    }
    let code = code
        .strip_prefix('\n')
        .unwrap_or(&code)
        .trim_end_matches('\n')
        .to_string();
    let content = text.map_or(0..0, |r| {
        let inner = &html[r.clone()];
        let start = r.start + inner.len() - inner.trim_start_matches('\n').len();
        start..(r.start + inner.trim_end_matches('\n').len()).max(start)
    });
    Some(BlockKind::CodeBlock {
        lang,
        code,
        content: origin.range(&content),
    })
}

fn table(html: &str, origin: &Origin, options: &ParseOptions) -> Option<BlockKind> {
    let toks = tokens(html);
    let mut rows: Vec<(bool, Vec<Range<usize>>)> = Vec::new();
    let mut current: Option<(bool, Vec<Range<usize>>)> = None;
    let mut cell_start: Option<(usize, bool)> = None;
    let mut closed = false;
    let mut tables = 0;
    for (range, token) in &toks {
        match token {
            Token::Open { name, attrs, .. } => match name.as_str() {
                "table" => {
                    tables += 1;
                    if tables > 1 {
                        return None;
                    }
                }
                "thead" | "tbody" | "tfoot" | "caption" | "colgroup" | "col" => {}
                "tr" => {
                    if current.is_some() || cell_start.is_some() {
                        return None;
                    }
                    current = Some((false, Vec::new()));
                }
                "td" | "th" => {
                    let span = |key: &str| {
                        attr(attrs, key)
                            .and_then(|v| v.parse::<u32>().ok())
                            .unwrap_or(1)
                    };
                    if current.is_none()
                        || cell_start.is_some()
                        || span("colspan") > 1
                        || span("rowspan") > 1
                    {
                        return None;
                    }
                    cell_start = Some((range.end, name == "th"));
                }
                _ if cell_start.is_some() => {}
                _ => return None,
            },
            Token::Close { name } => match name.as_str() {
                "td" | "th" => {
                    let (start, header) = cell_start.take()?;
                    let row = current.as_mut()?;
                    row.0 |= header;
                    row.1.push(start..range.start);
                }
                "tr" => rows.push(current.take()?),
                "table" => closed = true,
                _ => {}
            },
            Token::Text(text) => {
                if cell_start.is_none() && !text.trim().is_empty() {
                    return None;
                }
            }
            Token::Comment => {}
        }
    }
    if !closed || rows.is_empty() || current.is_some() || cell_start.is_some() {
        return None;
    }
    let columns = rows.iter().map(|r| r.1.len()).max().unwrap_or(0);
    let trimmed = |cell: &Range<usize>| {
        let text = &html[cell.clone()];
        let start = cell.start + text.len() - text.trim_start().len();
        start..start + text.trim().len()
    };
    let to_cells = |row: &(bool, Vec<Range<usize>>)| -> Vec<Vec<Inline>> {
        let mut cells: Vec<Vec<Inline>> = row
            .1
            .iter()
            .map(|c| cell_inlines(&html[trimmed(c)], &origin.slice(trimmed(c).start), options))
            .collect();
        cells.resize(columns, Vec::new());
        cells
    };
    let ranges = |row: &(bool, Vec<Range<usize>>)| -> Vec<Range<usize>> {
        let mut cells: Vec<Range<usize>> =
            row.1.iter().map(|c| origin.range(&trimmed(c))).collect();
        let end = cells.last().map_or(0, |c| c.end);
        cells.resize(columns, end..end);
        cells
    };
    let head = to_cells(&rows[0]);
    let body = rows[1..].iter().map(to_cells).collect();
    Some(BlockKind::Table {
        align: vec![Align::None; columns],
        head,
        rows: body,
        cells: rows.iter().map(ranges).collect(),
    })
}

fn cell_inlines(text: &str, origin: &Origin, options: &ParseOptions) -> Vec<Inline> {
    if text.is_empty() {
        return Vec::new();
    }
    let doc = crate::parse::parse(
        text,
        &ParseOptions {
            hard_breaks: options.hard_breaks,
        },
    );
    let mut out = Vec::new();
    for block in doc.blocks {
        match block.kind {
            BlockKind::Paragraph(inlines) => {
                if !out.is_empty() {
                    out.push(Inline::new(InlineKind::HardBreak, block.bytes, false));
                }
                out.extend(inlines);
            }
            _ => {
                let shown = InlineKind::Text(plain(text));
                return vec![Inline::new(shown, origin.range(&(0..text.len())), false)];
            }
        }
    }
    remap_inlines(&mut out, origin);
    out
}
