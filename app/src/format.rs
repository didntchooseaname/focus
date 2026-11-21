use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inline {
    Bold,
    Italic,
    Strike,
    Highlight,
    Code,
}

impl Inline {
    pub const ALL: [Inline; 5] = [
        Inline::Bold,
        Inline::Italic,
        Inline::Strike,
        Inline::Highlight,
        Inline::Code,
    ];

    fn mark(self) -> &'static str {
        match self {
            Inline::Bold => "**",
            Inline::Italic => "*",
            Inline::Strike => "~~",
            Inline::Highlight => "==",
            Inline::Code => "`",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub range: Range<usize>,
    pub text: String,
    pub select: Range<usize>,
}

type Span = (Range<usize>, Range<usize>);

fn escaped(line: &[u8], i: usize) -> bool {
    line[..i].iter().rev().take_while(|b| **b == b'\\').count() % 2 == 1
}

fn runs(line: &str, byte: u8, masked: &[Range<usize>]) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == byte && !escaped(bytes, i) && !masked.iter().any(|m| m.contains(&i)) {
            let start = i;
            while i < bytes.len() && bytes[i] == byte {
                i += 1;
            }
            out.push((start, i - start));
        } else {
            i += 1;
        }
    }
    out
}

fn code_spans(line: &str) -> Vec<Span> {
    let found = runs(line, b'`', &[]);
    let mut spans = Vec::new();
    let mut i = 0;
    while i < found.len() {
        let (start, len) = found[i];
        match found[i + 1..].iter().position(|(_, l)| *l == len) {
            Some(j) => {
                let (close, _) = found[i + 1 + j];
                spans.push((start..start + len, close..close + len));
                i += j + 2;
            }
            None => i += 1,
        }
    }
    spans
}

fn spans(line: &str, kind: Inline) -> Vec<Span> {
    let code = code_spans(line);
    if kind == Inline::Code {
        return code;
    }
    let masked: Vec<Range<usize>> = code.iter().map(|(o, c)| o.start..c.end).collect();
    let pair = |marks: Vec<Range<usize>>| -> Vec<Span> {
        marks
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[open, close]| (open.clone(), close.clone()))
            .collect()
    };
    match kind {
        Inline::Strike | Inline::Highlight => {
            let byte = kind.mark().as_bytes()[0];
            pair(
                runs(line, byte, &masked)
                    .into_iter()
                    .filter(|(_, len)| *len == 2)
                    .map(|(start, len)| start..start + len)
                    .collect(),
            )
        }
        Inline::Bold => {
            let stars: Vec<(usize, usize)> = runs(line, b'*', &masked)
                .into_iter()
                .filter(|(_, len)| *len == 2 || *len == 3)
                .collect();
            stars
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[(a, alen), (b, _)]| {
                    let open = if alen == 3 { a + 1..a + 3 } else { a..a + 2 };
                    (open, b..b + 2)
                })
                .collect()
        }
        Inline::Italic => {
            let stars: Vec<(usize, usize)> = runs(line, b'*', &masked)
                .into_iter()
                .filter(|(_, len)| *len == 1 || *len == 3)
                .collect();
            let mut out: Vec<Span> = stars
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[(a, _), (b, blen)]| (a..a + 1, b + blen - 1..b + blen))
                .collect();
            let bytes = line.as_bytes();
            let word = |i: usize| bytes.get(i).is_some_and(|b| b.is_ascii_alphanumeric());
            let unders: Vec<Range<usize>> = runs(line, b'_', &masked)
                .into_iter()
                .filter(|(start, len)| {
                    *len == 1 && !(start.checked_sub(1).is_some_and(word) && word(start + 1))
                })
                .map(|(start, _)| start..start + 1)
                .collect();
            out.extend(pair(unders));
            out
        }
        Inline::Code => unreachable!("returned above"),
    }
}

fn prefix(line: &str) -> usize {
    let mut at = line.len() - line.trim_start().len();
    loop {
        let rest = &line[at..];
        let before = at;
        if let Some(after) = rest.strip_prefix('>') {
            at += 1 + (after.len() - after.trim_start().len());
            let rest = &line[at..];
            if rest.starts_with("[!")
                && let Some(close) = rest.find(']')
            {
                at += close + 1;
                let rest = &line[at..];
                at += rest.len() - rest.trim_start_matches(['+', '-']).trim_start().len();
            }
        } else if let Some(level) = rest.find(|c| c != '#').filter(|n| (1..=6).contains(n))
            && rest[level..].starts_with(' ')
        {
            at += level + 1;
        } else if let Some(after) = ["- ", "* ", "+ "].iter().find_map(|m| rest.strip_prefix(m)) {
            at += 2;
            if after.len() >= 4 && after.starts_with('[') && after[2..].starts_with("] ") {
                at += 4;
            }
        } else {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if (1..10).contains(&digits)
                && rest[digits..].starts_with(['.', ')'])
                && rest[digits + 1..].starts_with(' ')
            {
                at += digits + 2;
            }
        }
        if at == before {
            return at;
        }
    }
}

fn parts(text: &str, range: &Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
    while start <= range.end && start <= text.len() {
        let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
        let line = text[start..end].trim_end_matches('\r');
        let content = start + prefix(line);
        let from = range.start.max(content);
        let to = range.end.min(start + line.len());
        if from < to {
            let piece = &text[from..to];
            let lead = piece.len() - piece.trim_start().len();
            let trail = piece.len() - piece.trim_end().len();
            if from + lead < to - trail {
                out.push(from + lead..to - trail);
            }
        }
        if end >= text.len() {
            break;
        }
        start = end + 1;
    }
    out
}

fn holding(text: &str, part: &Range<usize>, kind: Inline) -> Option<Span> {
    let line_start = text[..part.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |i| line_start + i);
    let line = &text[line_start..line_end];
    spans(line, kind).into_iter().find_map(|(open, close)| {
        let open = line_start + open.start..line_start + open.end;
        let close = line_start + close.start..line_start + close.end;
        let within = open.start <= part.start && part.end <= close.end;
        let covers = part.start < close.start && part.end > open.end;
        (within && covers).then_some((open, close))
    })
}

pub fn active(text: &str, selection: &Range<usize>, kind: Inline) -> bool {
    let parts = parts(text, selection);
    !parts.is_empty() && parts.iter().all(|p| holding(text, p, kind).is_some())
}

struct Op {
    at: usize,
    delete: usize,
    insert: &'static str,
}

fn shift(ops: &[Op], x: usize, start: bool) -> usize {
    let mut moved = x as isize;
    for op in ops {
        let delta = op.insert.len() as isize - op.delete as isize;
        if x < op.at || (!start && x == op.at) {
            continue;
        }
        if x >= op.at + op.delete {
            moved += delta;
        } else {
            moved += op.at as isize + op.insert.len() as isize - x as isize;
        }
    }
    moved.max(0) as usize
}

pub fn toggle(text: &str, selection: Range<usize>, kind: Inline) -> Option<Change> {
    let mark = kind.mark();
    if selection.is_empty() {
        let at = selection.start;
        return Some(Change {
            range: at..at,
            text: format!("{mark}{mark}"),
            select: at + mark.len()..at + mark.len(),
        });
    }
    let parts = parts(text, &selection);
    if parts.is_empty() {
        return None;
    }
    let remove = parts.iter().all(|p| holding(text, p, kind).is_some());
    let mut ops: Vec<Op> = Vec::new();
    for part in &parts {
        if remove {
            let Some((open, close)) = holding(text, part, kind) else {
                continue;
            };
            let inner = open.end..close.start;
            let (from, to) = (part.start.max(inner.start), part.end.min(inner.end));
            let left = text[inner.start..from].trim_end();
            if !left.is_empty() {
                ops.push(Op {
                    at: inner.start + left.len(),
                    delete: 0,
                    insert: mark_of(text, &open),
                });
            } else {
                ops.push(Op {
                    at: open.start,
                    delete: open.len(),
                    insert: "",
                });
            }
            let right = &text[to..inner.end];
            if !right.trim().is_empty() {
                ops.push(Op {
                    at: to + right.len() - right.trim_start().len(),
                    delete: 0,
                    insert: mark_of(text, &close),
                });
            } else {
                ops.push(Op {
                    at: close.start,
                    delete: close.len(),
                    insert: "",
                });
            }
        } else {
            if holding(text, part, kind).is_some() {
                continue;
            }
            let line_start = text[..part.start].rfind('\n').map_or(0, |i| i + 1);
            let line_end = text[line_start..]
                .find('\n')
                .map_or(text.len(), |i| line_start + i);
            let mut from = part.start;
            let mut to = part.end;
            let mut inner = Vec::new();
            for (open, close) in spans(&text[line_start..line_end], kind) {
                let (open, close) = (
                    line_start + open.start..line_start + open.end,
                    line_start + close.start..line_start + close.end,
                );
                if close.end <= from || open.start >= to {
                    continue;
                }
                from = from.min(open.start);
                to = to.max(close.end);
                inner.push(open);
                inner.push(close);
            }
            ops.push(Op {
                at: from,
                delete: 0,
                insert: mark,
            });
            for m in inner {
                ops.push(Op {
                    at: m.start,
                    delete: m.len(),
                    insert: "",
                });
            }
            ops.push(Op {
                at: to,
                delete: 0,
                insert: mark,
            });
        }
    }
    if ops.is_empty() {
        return None;
    }
    ops.sort_by_key(|op| (op.at, op.delete == 0 && op.insert.is_empty()));
    let range = ops.first()?.at..ops.iter().map(|op| op.at + op.delete).max()?;
    let mut out = String::new();
    let mut at = range.start;
    for op in &ops {
        out.push_str(&text[at..op.at.max(at)]);
        out.push_str(op.insert);
        at = at.max(op.at + op.delete);
    }
    out.push_str(&text[at..range.end]);
    let select = shift(&ops, selection.start, true)..shift(&ops, selection.end, false);
    Some(Change {
        range,
        text: out,
        select,
    })
}

fn mark_of(text: &str, range: &Range<usize>) -> &'static str {
    match &text[range.clone()] {
        "**" => "**",
        "__" => "__",
        "~~" => "~~",
        "==" => "==",
        "_" => "_",
        "``" => "``",
        "`" => "`",
        _ => "*",
    }
}

pub fn link(text: &str, selection: Range<usize>) -> Change {
    if let Some(change) = unlink(text, selection.clone()) {
        return change;
    }
    let inner = &text[selection.clone()];
    let written = format!("[{inner}]()");
    let caret = selection.start + written.len() - 1;
    Change {
        range: selection,
        text: written,
        select: caret..caret,
    }
}

fn unlink(text: &str, selection: Range<usize>) -> Option<Change> {
    let plain = |label: &str| !label.contains(['[', ']', '\n']);
    let whole = if selection.is_empty() {
        let caret = selection.start;
        if !text[..caret].ends_with("](") || !text[caret..].starts_with(')') {
            return None;
        }
        let close = caret - 2;
        let open = text[..close].rfind('[')?;
        open..caret + 1
    } else {
        let chosen = &text[selection.clone()];
        let target = chosen.strip_prefix('[')?.strip_suffix(')')?;
        let (_, url) = target.split_once("](")?;
        if url.contains(['(', ')', '\n']) {
            return None;
        }
        selection
    };
    let inside = &text[whole.start + 1..];
    let label = &inside[..inside.find("](")?];
    if !plain(label) {
        return None;
    }
    let start = whole.start;
    Some(Change {
        range: whole,
        text: label.to_string(),
        select: start..start + label.len(),
    })
}

pub fn prefix_line(text: &str, caret: usize, prefix: &str) -> Change {
    let start = text[..caret].rfind('\n').map_or(0, |at| at + 1);
    let end = text[caret..].find('\n').map_or(text.len(), |at| caret + at);
    let line = text[start..end].trim_end_matches('\r');
    let replacement = if prefix == "#" {
        let level = line.chars().take_while(|c| *c == '#').count();
        let body = line[level..].trim_start();
        if level >= 6 {
            body.to_string()
        } else {
            format!("{} {body}", "#".repeat(level + 1))
        }
    } else if let Some(body) = line.strip_prefix(prefix) {
        body.to_string()
    } else {
        format!("{prefix}{line}")
    };
    let column = caret - start;
    let at = (column as isize + replacement.len() as isize - line.len() as isize)
        .clamp(0, replacement.len() as isize) as usize;
    Change {
        range: start..start + line.len(),
        text: replacement,
        select: start + at..start + at,
    }
}

pub fn prefix_in_place(
    editor: &gpui::Entity<crate::editor::NoteEditor>,
    prefix: &str,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> bool {
    if !editor.read(cx).caret_in_place(cx) {
        return false;
    }
    let (input, reading) = {
        let e = editor.read(cx);
        (e.input.clone(), e.reading.clone())
    };
    if reading.read(cx).caret_in_property(cx) {
        return true;
    }
    let (text, selection) = read_field(&input, window, cx);
    let change = prefix_line(&text, selection.start, prefix);
    reading.update(cx, |reading, cx| {
        reading.replace_in_place(change.range, &change.text, change.select, window, cx);
        reading.source_if_unseen(cx);
    });
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Paragraph,
    Heading(u8),
    Bullets,
    Numbers,
    Tasks,
    Quote,
    Callout,
    Code,
}

impl Block {
    pub const ALL: [Block; 13] = [
        Block::Paragraph,
        Block::Heading(1),
        Block::Heading(2),
        Block::Heading(3),
        Block::Heading(4),
        Block::Heading(5),
        Block::Heading(6),
        Block::Bullets,
        Block::Numbers,
        Block::Tasks,
        Block::Quote,
        Block::Callout,
        Block::Code,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Line {
    Blank,
    Fence,
    Heading(u8),
    Item(Block),
    Quote { callout: bool },
    Text,
}

fn classify(line: &str) -> Line {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if trimmed.is_empty() {
        return Line::Blank;
    }
    if indent < 4 && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
        return Line::Fence;
    }
    if indent < 4 {
        let level = trimmed.bytes().take_while(|b| *b == b'#').count();
        if (1..=6).contains(&level)
            && trimmed[level..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
        {
            return Line::Heading(level as u8);
        }
        if let Some(rest) = trimmed.strip_prefix('>') {
            return Line::Quote {
                callout: rest.trim_start().starts_with("[!"),
            };
        }
    }
    for marker in ["- ", "* ", "+ "] {
        if let Some(after) = trimmed.strip_prefix(marker) {
            let task = after.len() >= 3 && after.starts_with('[') && after[2..].starts_with(']');
            return Line::Item(if task { Block::Tasks } else { Block::Bullets });
        }
    }
    let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    if (1..10).contains(&digits)
        && trimmed[digits..].starts_with(['.', ')'])
        && trimmed[digits + 1..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
    {
        return Line::Item(Block::Numbers);
    }
    Line::Text
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Unit {
    lines: Range<usize>,
    kind: Block,
}

fn units(lines: &[&str]) -> Vec<Unit> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let start = i;
        let kind = match classify(lines[i]) {
            Line::Blank => {
                i += 1;
                continue;
            }
            Line::Fence => {
                let fence = lines[i].trim_start();
                let mark = &fence[..3];
                i += 1;
                while i < lines.len() && !lines[i].trim_start().starts_with(mark) {
                    i += 1;
                }
                i = (i + 1).min(lines.len());
                Block::Code
            }
            Line::Heading(level) => {
                i += 1;
                Block::Heading(level)
            }
            Line::Quote { callout } => {
                i += 1;
                while i < lines.len() && matches!(classify(lines[i]), Line::Quote { .. }) {
                    i += 1;
                }
                if callout {
                    Block::Callout
                } else {
                    Block::Quote
                }
            }
            Line::Item(kind) => {
                let indent = lines[i].len() - lines[i].trim_start().len();
                i += 1;
                while i < lines.len() {
                    let next = lines[i];
                    let deeper = next.len() - next.trim_start().len() > indent;
                    match classify(next) {
                        Line::Text => i += 1,
                        Line::Fence if deeper => i += 1,
                        _ => break,
                    }
                }
                kind
            }
            Line::Text => {
                i += 1;
                while i < lines.len() && classify(lines[i]) == Line::Text {
                    i += 1;
                }
                Block::Paragraph
            }
        };
        out.push(Unit {
            lines: start..i,
            kind,
        });
    }
    out
}

fn split_lines(text: &str) -> (Vec<&str>, Vec<usize>) {
    let mut lines = Vec::new();
    let mut starts = Vec::new();
    let mut at = 0;
    for raw in text.split('\n') {
        starts.push(at);
        lines.push(raw.trim_end_matches('\r'));
        at += raw.len() + 1;
    }
    (lines, starts)
}

fn line_of(starts: &[usize], byte: usize) -> usize {
    starts.partition_point(|s| *s <= byte).saturating_sub(1)
}

pub fn block_at(text: &str, at: usize) -> Block {
    let (lines, starts) = split_lines(text);
    let line = line_of(&starts, at);
    units(&lines)
        .into_iter()
        .find(|u| u.lines.contains(&line))
        .map_or(Block::Paragraph, |u| u.kind)
}

fn inner(lines: &[&str], unit: &Unit) -> Vec<String> {
    let own = &lines[unit.lines.clone()];
    match unit.kind {
        Block::Code => {
            let mark = own[0].trim_start().get(..3).unwrap_or("```");
            let closed = own.len() > 1 && own[own.len() - 1].trim_start().starts_with(mark);
            let body = if closed {
                &own[1..own.len() - 1]
            } else {
                &own[1..]
            };
            body.iter().map(|l| l.to_string()).collect()
        }
        Block::Quote | Block::Callout => {
            let mut out: Vec<String> = Vec::new();
            for (n, line) in own.iter().enumerate() {
                let rest = line.trim_start().strip_prefix('>').unwrap_or(line);
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                if n == 0 && unit.kind == Block::Callout {
                    let title = rest[rest.find(']').map_or(0, |i| i + 1)..]
                        .trim_start_matches(['+', '-'])
                        .trim();
                    if !title.is_empty() {
                        out.push(title.to_string());
                    }
                    continue;
                }
                out.push(rest.to_string());
            }
            out
        }
        _ => own
            .iter()
            .map(|line| line[prefix(line)..].trim_end().to_string())
            .collect(),
    }
}

fn written(text: &[String], kind: Block) -> String {
    let lines: Vec<&str> = text.iter().map(String::as_str).collect();
    match kind {
        Block::Paragraph => lines.join("\n"),
        Block::Heading(level) => lines
            .iter()
            .map(|l| format!("{} {l}", "#".repeat(level as usize)))
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Bullets => lines
            .iter()
            .map(|l| format!("- {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Numbers => lines
            .iter()
            .enumerate()
            .map(|(n, l)| format!("{}. {l}", n + 1))
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Tasks => lines
            .iter()
            .map(|l| format!("- [ ] {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Quote => lines
            .iter()
            .map(|l| {
                if l.is_empty() {
                    ">".to_string()
                } else {
                    format!("> {l}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Callout => std::iter::once("> [!note]".to_string())
            .chain(lines.iter().map(|l| {
                if l.is_empty() {
                    ">".to_string()
                } else {
                    format!("> {l}")
                }
            }))
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Code => format!("```\n{}\n```", lines.join("\n")),
    }
}

pub fn set_block(text: &str, selection: Range<usize>, kind: Block) -> Option<Change> {
    let (lines, starts) = split_lines(text);
    let first = line_of(&starts, selection.start);
    let last = line_of(&starts, selection.end.max(selection.start + 1) - 1).max(first);
    let touched: Vec<Unit> = units(&lines)
        .into_iter()
        .filter(|u| u.lines.start <= last && first < u.lines.end)
        .collect();
    let (Some(head), Some(tail)) = (touched.first(), touched.last()) else {
        return None;
    };
    if touched.iter().all(|u| u.kind == kind) {
        return None;
    }
    let ending = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let start = starts[head.lines.start];
    let end_line = tail.lines.end - 1;
    let end = starts[end_line] + lines[end_line].len();
    let mut out = String::new();
    let mut at = head.lines.start;
    for unit in &touched {
        for line in &lines[at..unit.lines.start] {
            out.push_str(line);
            out.push_str(ending);
        }
        let block = if unit.kind == kind {
            lines[unit.lines.clone()].join(ending)
        } else {
            written(&inner(&lines, unit), kind).replace('\n', ending)
        };
        out.push_str(&block);
        if unit.lines.end - 1 < end_line {
            out.push_str(ending);
        }
        at = unit.lines.end;
    }
    let shown = prefix(out.lines().next().unwrap_or_default());
    let select = start + shown..start + out.trim_end().len();
    Some(Change {
        range: start..end,
        text: out,
        select,
    })
}

fn read_field(
    field: &gpui::Entity<gpui_component::input::InputState>,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> (String, Range<usize>) {
    use gpui::EntityInputHandler as _;
    use gpui_component::RopeExt as _;
    field.update(cx, |input, cx| {
        let selected = input
            .selected_text_range(true, window, cx)
            .map(|s| s.range)
            .unwrap_or(0..0);
        let text = input.text();
        let range =
            text.offset_utf16_to_offset(selected.start)..text.offset_utf16_to_offset(selected.end);
        (text.to_string(), range)
    })
}

fn write_field(
    field: &gpui::Entity<gpui_component::input::InputState>,
    change: Change,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    use gpui::EntityInputHandler as _;
    use gpui_component::RopeExt as _;
    field.update(cx, |input, cx| {
        let text = input.text();
        let range = text.offset_to_offset_utf16(change.range.start)
            ..text.offset_to_offset_utf16(change.range.end);
        input.replace_text_in_range(Some(range), &change.text, window, cx);
        input.set_selection(change.select.start, change.select.end, cx);
    });
}

pub fn apply_field(
    field: &gpui::Entity<gpui_component::input::InputState>,
    kind: Option<Inline>,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    let (text, selection) = read_field(field, window, cx);
    let change = match kind {
        Some(kind) => toggle(&text, selection, kind),
        None => Some(link(&text, selection)),
    };
    if let Some(change) = change {
        write_field(field, change, window, cx);
    }
}

pub fn apply_in_place(
    editor: &gpui::Entity<crate::editor::NoteEditor>,
    kind: Option<Inline>,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> bool {
    if !editor.read(cx).caret_in_place(cx) {
        return false;
    }
    let (input, reading) = {
        let e = editor.read(cx);
        (e.input.clone(), e.reading.clone())
    };
    if reading.read(cx).caret_in_property(cx) {
        return true;
    }
    let (text, selection) = read_field(&input, window, cx);
    let change = match kind {
        Some(kind) => toggle(&text, selection, kind),
        None => Some(link(&text, selection)),
    };
    if let Some(change) = change {
        reading.update(cx, |reading, cx| {
            reading.replace_in_place(change.range, &change.text, change.select, window, cx);
            reading.source_if_unseen(cx);
        });
    }
    true
}

pub fn set_block_type(
    editor: &gpui::Entity<crate::editor::NoteEditor>,
    block: Block,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    let (input, reading, in_place) = {
        let e = editor.read(cx);
        (e.input.clone(), e.reading.clone(), e.caret_in_place(cx))
    };
    if in_place && reading.read(cx).caret_in_property(cx) {
        return;
    }
    let (text, selection) = read_field(&input, window, cx);
    let Some(change) = set_block(&text, selection, block) else {
        return;
    };
    if in_place {
        reading.update(cx, |reading, cx| {
            reading.replace_in_place(change.range, &change.text, change.select, window, cx);
            reading.source_if_unseen(cx);
        });
    } else {
        write_field(&input, change, window, cx);
    }
}

pub fn in_place_actions(
    element: gpui::Div,
    editor: &gpui::Entity<crate::editor::NoteEditor>,
) -> gpui::Div {
    use crate::actions::*;
    use gpui::InteractiveElement as _;
    let on = |kind: Option<Inline>| {
        let editor = editor.clone();
        move |window: &mut gpui::Window, cx: &mut gpui::App| {
            if !apply_in_place(&editor, kind, window, cx) {
                cx.propagate();
            }
        }
    };
    let bold = on(Some(Inline::Bold));
    let italic = on(Some(Inline::Italic));
    let strike = on(Some(Inline::Strike));
    let highlight = on(Some(Inline::Highlight));
    let code = on(Some(Inline::Code));
    let url = on(None);
    let prefixed = |prefix: &'static str| {
        let editor = editor.clone();
        move |window: &mut gpui::Window, cx: &mut gpui::App| {
            if !prefix_in_place(&editor, prefix, window, cx) {
                cx.propagate();
            }
        }
    };
    let heading = prefixed("#");
    let bullets = prefixed("- ");
    let task = prefixed("- [ ] ");
    element
        .key_context(EDITOR_CONTEXT)
        .on_action(move |_: &FormatBold, window, cx| bold(window, cx))
        .on_action(move |_: &FormatItalic, window, cx| italic(window, cx))
        .on_action(move |_: &FormatStrike, window, cx| strike(window, cx))
        .on_action(move |_: &FormatHighlight, window, cx| highlight(window, cx))
        .on_action(move |_: &FormatCode, window, cx| code(window, cx))
        .on_action(move |_: &FormatLink, window, cx| url(window, cx))
        .on_action(move |_: &FormatHeading, window, cx| heading(window, cx))
        .on_action(move |_: &FormatBullets, window, cx| bullets(window, cx))
        .on_action(move |_: &FormatTask, window, cx| task(window, cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(text: &str, change: &Change) -> String {
        format!(
            "{}{}{}",
            &text[..change.range.start],
            change.text,
            &text[change.range.end..]
        )
    }

    #[test]
    fn a_link_made_twice_gives_the_text_back() {
        let text = "alpha beta gamma";
        let first = link(text, 0..text.len());
        let linked = applied(text, &first);
        assert_eq!(linked, "[alpha beta gamma]()");
        let second = link(&linked, first.select.clone());
        assert_eq!(applied(&linked, &second), text);
        assert_eq!(second.select, 0..text.len());
        let whole = "see [note](note.md) here";
        let unlinked = link(whole, 4..19);
        assert_eq!(applied(whole, &unlinked), "see note here");
        let empty = link("a  b", 2..2);
        assert_eq!(applied("a  b", &empty), "a []() b");
    }

    #[test]
    fn a_line_prefix_is_added_removed_and_heading_levels_cycle() {
        let text = "# T\n\nalpha\nbeta";
        let caret = text.find("alpha").expect("in the text") + 2;
        let bullet = prefix_line(text, caret, "- ");
        let bulleted = applied(text, &bullet);
        assert_eq!(bulleted, "# T\n\n- alpha\nbeta");
        assert_eq!(bullet.select, caret + 2..caret + 2);
        let back = prefix_line(&bulleted, caret + 2, "- ");
        assert_eq!(applied(&bulleted, &back), text);
        let heading = prefix_line(text, caret, "#");
        let headed = applied(text, &heading);
        assert_eq!(headed, "# T\n\n# alpha\nbeta");
        let second = prefix_line(&headed, heading.select.start, "#");
        assert_eq!(applied(&headed, &second), "# T\n\n## alpha\nbeta");
        let crlf = "one\r\ntwo";
        let task = prefix_line(crlf, 1, "- [ ] ");
        assert_eq!(applied(crlf, &task), "- [ ] one\r\ntwo");
    }

    fn run(text: &str, needle: &str, kind: Inline) -> (String, String) {
        let start = text.find(needle).expect("in the text");
        let change = toggle(text, start..start + needle.len(), kind).expect("a change");
        let out = format!(
            "{}{}{}",
            &text[..change.range.start],
            change.text,
            &text[change.range.end..]
        );
        let selected = out[change.select.clone()].to_string();
        (out, selected)
    }

    #[test]
    fn each_format_is_applied_then_removed_on_the_same_selection() {
        let text = "A plain word here.\n";
        for (kind, marked) in [
            (Inline::Bold, "A **plain** word here.\n"),
            (Inline::Italic, "A *plain* word here.\n"),
            (Inline::Strike, "A ~~plain~~ word here.\n"),
            (Inline::Highlight, "A ==plain== word here.\n"),
            (Inline::Code, "A `plain` word here.\n"),
        ] {
            let (out, selected) = run(text, "plain", kind);
            assert_eq!(out, marked, "{kind:?} applied");
            assert_eq!(selected, "plain", "{kind:?}: the text stays selected");
            let start = out.find("plain").expect("kept");
            assert!(active(&out, &(start..start + 5), kind), "{kind:?} active");
            let (back, selected) = run(&out, "plain", kind);
            assert_eq!(back, text, "{kind:?} removed");
            assert_eq!(selected, "plain");
        }
    }

    #[test]
    fn a_selection_of_the_marks_too_is_unwrapped() {
        let (out, selected) = run("A **plain** word.\n", "**plain**", Inline::Bold);
        assert_eq!(out, "A plain word.\n");
        assert_eq!(selected, "plain");
    }

    #[test]
    fn a_part_of_a_span_leaves_it_and_the_span_is_cut_around() {
        let (out, selected) = run("**one two three**\n", "two", Inline::Bold);
        assert_eq!(out, "**one** two **three**\n");
        assert_eq!(selected, "two");
    }

    #[test]
    fn italic_and_bold_are_told_apart() {
        let text = "Some **strong** and *soft* words.\n";
        let at = |s: &str| {
            let start = text.find(s).expect("present");
            start..start + s.len()
        };
        assert!(active(text, &at("strong"), Inline::Bold));
        assert!(!active(text, &at("strong"), Inline::Italic));
        assert!(active(text, &at("soft"), Inline::Italic));
        assert!(!active(text, &at("soft"), Inline::Bold));
        assert!(!active(text, &at("words"), Inline::Bold));
        let both = "A ***mixed*** case.\n";
        let start = both.find("mixed").expect("present");
        assert!(active(both, &(start..start + 5), Inline::Bold));
        assert!(active(both, &(start..start + 5), Inline::Italic));
        assert!(
            !active("snake_case_name\n", &(6..10), Inline::Italic),
            "an underscore inside a word"
        );
    }

    #[test]
    fn marks_inside_code_are_text() {
        let text = "Run `a**b**c` now.\n";
        let start = text.find("b").expect("present");
        assert!(!active(text, &(start..start + 1), Inline::Bold));
        assert!(active(text, &(start..start + 1), Inline::Code));
    }

    #[test]
    fn a_selection_over_lines_formats_each_line_without_its_markup() {
        let text = "- one item\n- two item\n";
        let change = toggle(text, 0..text.len() - 1, Inline::Bold).expect("a change");
        let out = format!(
            "{}{}{}",
            &text[..change.range.start],
            change.text,
            &text[change.range.end..]
        );
        assert_eq!(out, "- **one item**\n- **two item**\n");
        assert!(active(&out, &change.select, Inline::Bold));
        let back = toggle(&out, change.select.clone(), Inline::Bold).expect("removed");
        let out = format!(
            "{}{}{}",
            &out[..back.range.start],
            back.text,
            &out[back.range.end..]
        );
        assert_eq!(out, text);
    }

    #[test]
    fn white_space_at_the_ends_stays_outside_the_marks() {
        let (out, _) = run("one two three\n", "two ", Inline::Italic);
        assert_eq!(out, "one *two* three\n");
    }

    #[test]
    fn an_empty_selection_gets_both_marks_and_the_caret_between() {
        let change = toggle("ab", 1..1, Inline::Strike).expect("a change");
        assert_eq!(change.text, "~~~~");
        assert_eq!(change.select, 3..3);
    }

    #[test]
    fn the_block_type_is_read_from_the_lines() {
        let text = "# Head\n\nPara one\nline two\n\n- [ ] task\n1. first\n\n> [!tip] Hint\n> body\n\n> quoted\n\n```\ncode # not a heading\n```\n";
        let at = |s: &str| text.find(s).expect("present");
        assert_eq!(block_at(text, at("Head")), Block::Heading(1));
        assert_eq!(block_at(text, at("line two")), Block::Paragraph);
        assert_eq!(block_at(text, at("task")), Block::Tasks);
        assert_eq!(block_at(text, at("first")), Block::Numbers);
        assert_eq!(block_at(text, at("body")), Block::Callout);
        assert_eq!(block_at(text, at("quoted")), Block::Quote);
        assert_eq!(block_at(text, at("not a heading")), Block::Code);
    }

    #[test]
    fn a_selection_over_two_blocks_changes_both() {
        let text = "Intro\n\nFirst para\n\n## Second\n\nOutro\n";
        let start = text.find("para").expect("present");
        let end = text.find("cond").expect("present");
        let change = set_block(text, start..end, Block::Bullets).expect("a change");
        let out = format!(
            "{}{}{}",
            &text[..change.range.start],
            change.text,
            &text[change.range.end..]
        );
        assert_eq!(out, "Intro\n\n- First para\n\n- Second\n\nOutro\n");
        assert_eq!(&out[change.select.clone()], "First para\n\n- Second");
        let back = set_block(&out, change.select.clone(), Block::Paragraph).expect("back");
        let back = format!(
            "{}{}{}",
            &out[..back.range.start],
            back.text,
            &out[back.range.end..]
        );
        assert_eq!(back, "Intro\n\nFirst para\n\nSecond\n\nOutro\n");
    }

    #[test]
    fn each_type_writes_its_markup() {
        let text = "Alpha\nBeta\n";
        let apply = |kind: Block| {
            let change = set_block(text, 0..1, kind).expect("a change");
            format!(
                "{}{}{}",
                &text[..change.range.start],
                change.text,
                &text[change.range.end..]
            )
        };
        assert_eq!(apply(Block::Heading(3)), "### Alpha\n### Beta\n");
        assert_eq!(apply(Block::Numbers), "1. Alpha\n2. Beta\n");
        assert_eq!(apply(Block::Tasks), "- [ ] Alpha\n- [ ] Beta\n");
        assert_eq!(apply(Block::Quote), "> Alpha\n> Beta\n");
        assert_eq!(apply(Block::Callout), "> [!note]\n> Alpha\n> Beta\n");
        assert_eq!(apply(Block::Code), "```\nAlpha\nBeta\n```\n");
        assert_eq!(set_block(text, 0..1, Block::Paragraph), None, "already");
        let code = "```\nx = 1\n```\n";
        let change = set_block(code, 5..6, Block::Paragraph).expect("out of the fence");
        assert_eq!(change.text, "x = 1");
    }
}
