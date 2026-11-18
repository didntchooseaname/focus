use crate::{Heading, LinkKind, OutLink, ParsedMeta, PropValue};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TaskLine {
    pub line: u32,
    pub text: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LinkSpan {
    pub kind: LinkKind,
    pub plain: bool,
    pub line: u32,
    pub full: Range<usize>,
    pub target_range: Range<usize>,
    pub target: String,
    pub anchor: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Parsed {
    pub meta: ParsedMeta,
    pub tasks: Vec<TaskLine>,
}

pub(crate) fn parse(source: &str) -> Parsed {
    let mut out = Parsed::default();
    let mut spans = Vec::new();
    let mut tags = Vec::new();
    scan(source, Some(&mut out), &mut spans, Some(&mut tags));
    for range in tags {
        push_tag(&mut out.meta.tags, &source[range]);
    }
    out.meta.links = spans
        .into_iter()
        .map(|s| OutLink {
            kind: s.kind,
            target: s.target,
            anchor: s.anchor,
            line: s.line,
        })
        .collect();
    out
}

pub(crate) fn link_spans(source: &str) -> Vec<LinkSpan> {
    let mut spans = Vec::new();
    scan(source, None, &mut spans, None);
    spans
}

pub(crate) fn body_tag_spans(source: &str) -> Vec<Range<usize>> {
    let mut tags = Vec::new();
    scan(source, None, &mut Vec::new(), Some(&mut tags));
    tags
}

pub(crate) fn frontmatter_end(source: &str) -> Option<usize> {
    let mut it = lines(source);
    let (_, first) = it.next()?;
    if !is_fence_delim(first) {
        return None;
    }
    for (base, l) in it {
        let t = l.trim_end();
        if t == "---" || t == "..." {
            let after = base + l.len();
            return Some(match source[after..].strip_prefix("\r\n") {
                Some(_) => after + 2,
                None if source[after..].starts_with('\n') => after + 1,
                None if source[after..].starts_with('\r') => after + 1,
                None => after,
            });
        }
    }
    None
}

pub(crate) fn line_offsets(source: &str) -> impl Iterator<Item = (usize, &str)> {
    lines(source)
}

struct Lines<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Iterator for Lines<'a> {
    type Item = (usize, &'a str);
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.src.len() {
            return None;
        }
        let start = self.pos;
        let rest = &self.src.as_bytes()[start..];
        let (end, next) = match memchr_newline(rest) {
            Some(i) => (start + i, start + i + 1),
            None => (self.src.len(), self.src.len()),
        };
        self.pos = next;
        let mut line = &self.src[start..end];
        if let Some(stripped) = line.strip_suffix('\r') {
            line = stripped;
        }
        Some((start, line))
    }
}

fn memchr_newline(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(|&b| b == b'\n')
}

fn lines(src: &str) -> Lines<'_> {
    Lines { src, pos: 0 }
}

fn is_fence_delim(line: &str) -> bool {
    let t = line.trim_end();
    t == "---"
}

fn quote_prefix(line: &str) -> usize {
    let b = line.as_bytes();
    let mut i = 0;
    let mut last = 0;
    loop {
        let mut j = i;
        while j < b.len() && j - i < 4 && b[j] == b' ' {
            j += 1;
        }
        if j < b.len() && b[j] == b'>' {
            j += 1;
            if j < b.len() && b[j] == b' ' {
                j += 1;
            }
            i = j;
            last = j;
        } else {
            return last;
        }
    }
}

struct Fence {
    ch: u8,
    len: usize,
    mermaid: bool,
    tabs: bool,
}

fn fence_open(content: &str) -> Option<Fence> {
    let t = content.trim_start_matches([' ', '\t']);
    let b = t.as_bytes();
    let ch = *b.first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = b.iter().take_while(|&&c| c == ch).count();
    if len < 3 {
        return None;
    }
    if ch == b'`' && t[len..].contains('`') {
        return None;
    }
    let info = t[len..].split_whitespace().next().unwrap_or("");
    Some(Fence {
        ch,
        len,
        mermaid: info.eq_ignore_ascii_case("mermaid"),
        tabs: info.eq_ignore_ascii_case("tabs"),
    })
}

fn fence_close(content: &str, fence: &Fence) -> bool {
    let t = content.trim_start_matches([' ', '\t']);
    let b = t.as_bytes();
    let len = b.iter().take_while(|&&c| c == fence.ch).count();
    len >= fence.len && t[len..].trim().is_empty()
}

fn after_list_marker(content: &str) -> Option<usize> {
    let indent = content.len() - content.trim_start_matches([' ', '\t']).len();
    let b = content.as_bytes();
    let mut i = indent;
    if matches!(b.get(i), Some(b'-' | b'*' | b'+')) {
        i += 1;
    } else {
        let digits = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 || digits > 9 || !matches!(b.get(i + digits), Some(b'.' | b')')) {
            return None;
        }
        i += digits + 1;
    }
    let spaces = b[i..]
        .iter()
        .take_while(|&&c| c == b' ' || c == b'\t')
        .count();
    (spaces > 0).then_some(i + spaces)
}

#[derive(Clone, Copy)]
enum Html {
    Until(&'static str),
    UntilBlank,
}

const HTML_BLOCK_TAGS: &[&str] = &[
    "address",
    "article",
    "aside",
    "base",
    "basefont",
    "blockquote",
    "body",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
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
    "iframe",
    "legend",
    "li",
    "link",
    "main",
    "menu",
    "menuitem",
    "nav",
    "noframes",
    "ol",
    "optgroup",
    "option",
    "p",
    "param",
    "search",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
    "ul",
];

fn html_open(content: &str, after_blank: bool) -> Option<Html> {
    let indent = content.bytes().take_while(|&b| b == b' ').count();
    if indent > 3 {
        return None;
    }
    let t = &content[indent..];
    if !t.starts_with('<') {
        return None;
    }
    if t.starts_with("<!--") {
        return Some(Html::Until("-->"));
    }
    let closing = t.starts_with("</");
    let name_start = if closing { 2 } else { 1 };
    let name_len = t[name_start..]
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'-')
        .count();
    if name_len == 0 || !t.as_bytes()[name_start].is_ascii_alphabetic() {
        return None;
    }
    let name = t[name_start..name_start + name_len].to_ascii_lowercase();
    let after = &t[name_start + name_len..];
    let ends_name =
        after.is_empty() || after.starts_with([' ', '\t', '>']) || after.starts_with("/>");
    if !ends_name {
        return None;
    }
    if !closing {
        for (raw, end) in [
            ("pre", "</pre>"),
            ("script", "</script>"),
            ("style", "</style>"),
            ("textarea", "</textarea>"),
        ] {
            if name == raw {
                return Some(Html::Until(end));
            }
        }
    }
    if HTML_BLOCK_TAGS.contains(&name.as_str()) {
        return Some(Html::UntilBlank);
    }
    let alone = t
        .find('>')
        .is_some_and(|end| !t[1..end].contains('<') && t[end + 1..].trim().is_empty());
    (after_blank && alone).then_some(Html::UntilBlank)
}

fn contains_ignoring_case(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

fn atx_heading(line: &str) -> Option<(u8, &str)> {
    let indent = line.bytes().take_while(|&b| b == b' ').count();
    if indent > 3 {
        return None;
    }
    let t = &line[indent..];
    let level = t.bytes().take_while(|&b| b == b'#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &t[level..];
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let mut text = rest.trim();
    let trimmed = text.trim_end_matches('#');
    if trimmed.len() != text.len() && (trimmed.is_empty() || trimmed.ends_with([' ', '\t'])) {
        text = trimmed.trim_end();
    }
    Some((level as u8, text))
}

fn task(content: &str) -> Option<(bool, &str)> {
    let t = content.trim_start_matches([' ', '\t']);
    let b = t.as_bytes();
    let mut i = 0;
    if matches!(b.first(), Some(b'-' | b'*' | b'+')) {
        i = 1;
    } else {
        while i < b.len() && b[i].is_ascii_digit() && i < 9 {
            i += 1;
        }
        if i == 0 || !matches!(b.get(i), Some(b'.' | b')')) {
            return None;
        }
        i += 1;
    }
    if !matches!(b.get(i), Some(b' ' | b'\t')) {
        return None;
    }
    while matches!(b.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    if b.get(i) != Some(&b'[') || b.get(i + 2) != Some(&b']') {
        return None;
    }
    let mark = t[i + 1..].chars().next()?;
    if mark.len_utf8() != 1 {
        return None;
    }
    let after = &t[i + 3..];
    if !after.is_empty() && !after.starts_with([' ', '\t']) {
        return None;
    }
    Some((mark != ' ', after.trim()))
}

pub(crate) fn task_mark(line: &str) -> Option<(usize, bool)> {
    let q = quote_prefix(line);
    let content = &line[q..];
    let (done, _) = task(content)?;
    let open = content.find('[')?;
    Some((q + open + 1, done))
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '/'
}

fn tag_at(s: &str, i: usize) -> Option<usize> {
    let mut end = i;
    let mut has_non_digit = false;
    for (off, c) in s[i..].char_indices() {
        if !is_tag_char(c) {
            break;
        }
        if !c.is_ascii_digit() && c != '/' {
            has_non_digit = true;
        }
        end = i + off + c.len_utf8();
    }
    while end > i && s.as_bytes()[end - 1] == b'/' {
        end -= 1;
    }
    (end > i && has_non_digit).then_some(end)
}

pub(crate) fn percent_decode(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

pub(crate) fn is_external(target: &str) -> bool {
    let Some(colon) = target.find(':') else {
        return false;
    };
    colon > 1
        && target[..colon].bytes().enumerate().all(|(i, c)| {
            c.is_ascii_alphabetic()
                || (i > 0 && (c.is_ascii_digit() || matches!(c, b'+' | b'.' | b'-')))
        })
}

fn wiki_at(
    line: &str,
    open: usize,
    base: usize,
    lineno: u32,
    embed: bool,
    spans: &mut Vec<LinkSpan>,
) -> Option<usize> {
    let inner_start = open + 2;
    let close_rel = line[inner_start..].find("]]")?;
    let inner = &line[inner_start..inner_start + close_rel];
    if inner.contains("[[") || inner.is_empty() {
        return None;
    }
    let target_end = inner
        .find(['|', '#'])
        .map(|i| {
            if i > 0 && inner.as_bytes()[i - 1] == b'\\' {
                i - 1
            } else {
                i
            }
        })
        .unwrap_or(inner.len());
    let raw_target = &inner[..target_end];
    let anchor = inner.find('#').map(|h| {
        let rest = &inner[h + 1..];
        let end = rest.find('|').unwrap_or(rest.len());
        let a = &rest[..end];
        a.strip_suffix('\\').unwrap_or(a).trim().to_string()
    });
    let full_start = if embed { open - 1 } else { open };
    let end = inner_start + close_rel + 2;
    spans.push(LinkSpan {
        kind: if embed {
            LinkKind::Embed
        } else {
            LinkKind::Wiki
        },
        plain: true,
        line: lineno,
        full: base + full_start..base + end,
        target_range: base + inner_start..base + inner_start + raw_target.len(),
        target: raw_target.trim().to_string(),
        anchor: anchor.filter(|a| !a.is_empty()),
    });
    Some(end)
}

fn md_link_at(
    line: &str,
    open: usize,
    base: usize,
    lineno: u32,
    embed: bool,
    spans: &mut Vec<LinkSpan>,
) -> Option<usize> {
    let b = line.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    let close = loop {
        match b.get(i)? {
            b'\\' => i += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break i;
                }
            }
            _ => {}
        }
        i += 1;
    };
    if b.get(close + 1) != Some(&b'(') {
        return None;
    }
    let mut j = close + 2;
    while b.get(j) == Some(&b' ') {
        j += 1;
    }
    let (dest_range, mut k) = if b.get(j) == Some(&b'<') {
        let end = j + 1 + line[j + 1..].find('>')?;
        (j + 1..end, end + 1)
    } else {
        let start = j;
        let mut parens = 0usize;
        while let Some(&c) = b.get(j) {
            match c {
                b'\\' => j += 1,
                b'(' => parens += 1,
                b')' if parens == 0 => break,
                b')' => parens -= 1,
                b' ' | b'\t' => break,
                _ => {}
            }
            j += 1;
        }
        (start..j.min(b.len()), j)
    };
    while b.get(k) == Some(&b' ') {
        k += 1;
    }
    if let Some(&q) = b.get(k)
        && (q == b'"' || q == b'\'')
    {
        k = k + 1 + line[k + 1..].find(q as char)? + 1;
        while b.get(k) == Some(&b' ') {
            k += 1;
        }
    }
    if b.get(k) != Some(&b')') {
        return None;
    }
    let end = k + 1;
    let dest = &line[dest_range.clone()];
    if dest.is_empty() || dest.starts_with(REF_OPEN) {
        return Some(end);
    }
    let (raw_target, anchor) = if is_external(dest) {
        (dest, None)
    } else {
        match dest.find('#') {
            Some(h) => (&dest[..h], Some(percent_decode(&dest[h + 1..]))),
            None => (dest, None),
        }
    };
    let full_start = if embed { open - 1 } else { open };
    spans.push(LinkSpan {
        kind: if embed {
            LinkKind::Embed
        } else {
            LinkKind::Markdown
        },
        plain: false,
        line: lineno,
        full: base + full_start..base + end,
        target_range: base + dest_range.start..base + dest_range.start + raw_target.len(),
        target: if is_external(raw_target) {
            raw_target.to_string()
        } else {
            percent_decode(raw_target)
        },
        anchor: anchor.filter(|a| !a.is_empty()),
    });
    Some(end)
}

type RefBlock = (u32, Vec<(Range<usize>, String)>);

fn directive_alone(trimmed: &str, name: &str) -> bool {
    trimmed
        .strip_prefix("{{#")
        .and_then(|t| t.strip_prefix(name))
        .and_then(|t| t.trim_start().strip_prefix("}}"))
        .is_some_and(str::is_empty)
}

fn ref_link(first: u32, parts: &[(Range<usize>, String)], spans: &mut Vec<LinkSpan>) {
    let (Some((head, _)), Some((tail, _))) = (parts.first(), parts.last()) else {
        return;
    };
    let joined = parts
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if is_external(&joined) {
        return;
    }
    let (target, anchor) = match joined.find('#') {
        Some(h) => (joined[..h].to_string(), Some(joined[h + 1..].to_string())),
        None => (joined.clone(), None),
    };
    let end = if parts.len() == 1 {
        head.start + target.len()
    } else {
        tail.end
    };
    spans.push(LinkSpan {
        kind: LinkKind::Markdown,
        plain: true,
        line: first + 1,
        full: head.start..tail.end,
        target_range: head.start..end,
        target,
        anchor: anchor.filter(|a| !a.is_empty()),
    });
}

const REF_OPEN: &str = "{{#ref";

fn directive_at(text: &str, from: usize, name: &str) -> Option<(usize, usize)> {
    let opening = format!("{{{{#{name}");
    let mut search = from;
    loop {
        let at = search + text[search..].find(&opening)?;
        let rest = &text[at + opening.len()..];
        let spaces = rest.len() - rest.trim_start_matches(' ').len();
        if rest[spaces..].starts_with("}}") {
            return Some((at, at + opening.len() + spaces + 2));
        }
        search = at + opening.len();
    }
}
const INCLUDE: &str = "{{#include";

fn scan_directives(
    line: &str,
    base: usize,
    lineno: u32,
    seg: Range<usize>,
    spans: &mut Vec<LinkSpan>,
) {
    let text = &line[seg.clone()];
    let mut from = 0;
    while let Some((at, start)) = directive_at(text, from, "ref") {
        let Some((len, end)) = directive_at(text, start, "endref") else {
            break;
        };
        directive_link(
            line,
            base,
            lineno,
            LinkKind::Markdown,
            seg.start + at..seg.start + end,
            seg.start + start..seg.start + len,
            spans,
        );
        from = end;
    }
    from = 0;
    while let Some(at) = text[from..].find(INCLUDE) {
        let mut i = from + at + INCLUDE.len();
        i += text[i..].len() - text[i..].trim_start().len();
        let end = text[i..]
            .find(|c: char| c.is_whitespace() || c == '}')
            .map_or(text.len(), |e| i + e);
        let raw = &text[i..end];
        let path = match raw.find(':') {
            Some(colon) if colon > 0 => &raw[..colon],
            _ => raw,
        };
        let close = text[end..].find("}}").map_or(end, |e| end + e + 2);
        if !path.is_empty() {
            directive_link(
                line,
                base,
                lineno,
                LinkKind::Include,
                seg.start + from + at..seg.start + close,
                seg.start + i..seg.start + i + path.len(),
                spans,
            );
        }
        from = close.max(i);
    }
}

fn directive_link(
    line: &str,
    base: usize,
    lineno: u32,
    kind: LinkKind,
    full: Range<usize>,
    path: Range<usize>,
    spans: &mut Vec<LinkSpan>,
) {
    let raw = &line[path.clone()];
    let lead = raw.len() - raw.trim_start().len();
    let raw = raw.trim();
    if raw.is_empty() || is_external(raw) {
        return;
    }
    let (target, anchor) = match raw.find('#') {
        Some(h) => (&raw[..h], Some(raw[h + 1..].to_string())),
        None => (raw, None),
    };
    let start = path.start + lead;
    spans.push(LinkSpan {
        kind,
        plain: true,
        line: lineno,
        full: base + full.start..base + full.end,
        target_range: base + start..base + start + target.len(),
        target: target.to_string(),
        anchor: anchor.filter(|a| !a.is_empty()),
    });
}

fn visible_segments(line: &str, in_comment: &mut bool, out: &mut Vec<Range<usize>>) {
    out.clear();
    let b = line.as_bytes();
    let mut seg_start = 0;
    let mut i = 0;
    while i < b.len() {
        if *in_comment {
            match line[i..].find("%%") {
                Some(p) => {
                    *in_comment = false;
                    i += p + 2;
                    seg_start = i;
                    continue;
                }
                None => return,
            }
        }
        match b[i] {
            b'`' => {
                let n = b[i..].iter().take_while(|&&c| c == b'`').count();
                let mut j = i + n;
                let mut closed = None;
                while j < b.len() {
                    if b[j] == b'`' {
                        let m = b[j..].iter().take_while(|&&c| c == b'`').count();
                        if m == n {
                            closed = Some(j + m);
                            break;
                        }
                        j += m;
                    } else {
                        j += 1;
                    }
                }
                match closed {
                    Some(end) => {
                        if i > seg_start {
                            out.push(seg_start..i);
                        }
                        i = end;
                        seg_start = i;
                    }
                    None => i += n,
                }
            }
            b'%' if b.get(i + 1) == Some(&b'%') => {
                if i > seg_start {
                    out.push(seg_start..i);
                }
                *in_comment = true;
                i += 2;
            }
            b'\\' => i += 2,
            _ => i += 1,
        }
    }
    if seg_start < b.len() {
        out.push(seg_start..b.len());
    }
}

fn scan_wikis(line: &str, base: usize, lineno: u32, kind: LinkKind, spans: &mut Vec<LinkSpan>) {
    let b = line.as_bytes();
    let mut i = 0;
    while let Some(rel) = line[i..].find("[[") {
        let open = i + rel;
        let shape = kind == LinkKind::Diagram
            && open > 0
            && (b[open - 1].is_ascii_alphanumeric() || b[open - 1] == b'_');
        let embed = open > 0 && b[open - 1] == b'!';
        let found = if shape {
            None
        } else {
            wiki_at(line, open, base, lineno, embed, spans)
        };
        match found {
            Some(end) => {
                if let Some(s) = spans.last_mut() {
                    s.kind = kind;
                }
                i = end;
            }
            None => i = open + 2,
        }
    }
}

fn mermaid_click(line: &str, base: usize, lineno: u32, spans: &mut Vec<LinkSpan>) {
    let t = line.trim_start();
    let Some((keyword, rest)) = ["click", "link"]
        .iter()
        .find_map(|k| t.strip_prefix(k).map(|r| (*k, r)))
    else {
        return;
    };
    if !rest.starts_with([' ', '\t']) {
        return;
    }
    let offset = line.len() - t.len() + keyword.len();
    let Some(q) = rest.find('"') else { return };
    let start = offset + q + 1;
    let Some(len) = line[start..].find('"') else {
        return;
    };
    let value = &line[start..start + len];
    if value.is_empty() || value.starts_with("[[") || is_external(value) {
        return;
    }
    let (target, anchor) = match value.split_once('#') {
        Some((t, a)) => (t, Some(a.trim().to_string()).filter(|a| !a.is_empty())),
        None => (value, None),
    };
    spans.push(LinkSpan {
        kind: LinkKind::Diagram,
        plain: true,
        line: lineno,
        full: base + start..base + start + len,
        target_range: base + start..base + start + target.len(),
        target: target.trim().to_string(),
        anchor,
    });
}

fn block_id(line: &str) -> Option<&str> {
    let t = line.trim_end();
    let caret = t.rfind('^')?;
    let id = &t[caret + 1..];
    let before = &t[..caret];
    let valid = !id.is_empty() && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-');
    (valid && (before.is_empty() || before.ends_with([' ', '\t']))).then_some(id)
}

fn scan_inline(
    line: &str,
    base: usize,
    lineno: u32,
    seg: Range<usize>,
    tags: Option<&mut Vec<Range<usize>>>,
    spans: &mut Vec<LinkSpan>,
) {
    let b = line.as_bytes();
    let mut tags = tags;
    let mut i = seg.start;
    let end = seg.end;
    let text = &line[..end];
    while i < end {
        match b[i] {
            b'\\' => i += 2,
            b'!' if b.get(i + 1) == Some(&b'[') => {
                let next = if b.get(i + 2) == Some(&b'[') {
                    wiki_at(text, i + 1, base, lineno, true, spans)
                } else {
                    md_link_at(text, i + 1, base, lineno, true, spans)
                };
                i = next.unwrap_or(i + 1);
            }
            b'[' => {
                let next = if b.get(i + 1) == Some(&b'[') {
                    wiki_at(text, i, base, lineno, false, spans)
                } else {
                    md_link_at(text, i, base, lineno, false, spans)
                };
                i = next.unwrap_or(i + 1);
            }
            b'#' => {
                let boundary = i == 0 || line[..i].ends_with(|c: char| c.is_whitespace());
                match (boundary, tag_at(text, i + 1)) {
                    (true, Some(tag_end)) => {
                        if let Some(tags) = tags.as_deref_mut() {
                            tags.push(base + i + 1..base + tag_end);
                        }
                        i = tag_end;
                    }
                    _ => i += 1,
                }
            }
            _ => i += 1,
        }
    }
}

fn count_words(s: &str) -> u32 {
    let mut n = 0;
    let mut in_word = false;
    for c in s.chars() {
        let w = c.is_alphanumeric();
        if w && !in_word {
            n += 1;
        }
        in_word = w;
    }
    n
}

fn scan(
    source: &str,
    mut out: Option<&mut Parsed>,
    spans: &mut Vec<LinkSpan>,
    mut tags: Option<&mut Vec<Range<usize>>>,
) {
    let mut it = lines(source).enumerate().peekable();
    let mut lineno_after_fm = 0u32;

    if let Some((_, (_, first))) = it.peek()
        && is_fence_delim(first)
    {
        let mut fm_lines = Vec::new();
        let mut closed = None;
        for (n, (base, l)) in lines(source).enumerate().skip(1) {
            let t = l.trim_end();
            if t == "---" || t == "..." {
                closed = Some(n);
                break;
            }
            fm_lines.push((n as u32, base, l));
        }
        if let Some(close) = closed {
            for &(n, base, l) in &fm_lines {
                if !l.trim_start().starts_with('#') {
                    scan_wikis(l, base, n, LinkKind::Property, spans);
                }
            }
            let fm_lines: Vec<&str> = fm_lines.iter().map(|&(_, _, l)| l).collect();
            if let Some(out) = out.as_deref_mut() {
                let props = parse_yaml(&fm_lines);
                apply_props(&mut out.meta, props);
            }
            for _ in 0..=close {
                it.next();
            }
            lineno_after_fm = close as u32 + 1;
        }
    }

    let mut fence: Option<Fence> = None;
    let mut tabs: Vec<Fence> = Vec::new();
    let mut ref_block: Option<RefBlock> = None;
    let mut html: Option<Html> = None;
    let mut after_blank = true;
    let mut in_comment = false;
    let mut segs = Vec::new();
    let mut words = 0u32;
    let mut run_start: Option<u32> = None;
    let mut previous_run: Option<u32> = None;
    for (n, (base, line)) in it {
        let lineno = n as u32;
        if line.trim().is_empty() {
            previous_run = run_start.take().or(previous_run);
        } else if run_start.is_none() {
            run_start = Some(lineno);
        }
        let q = quote_prefix(line);
        let content = &line[q..];
        let blank = line.trim().is_empty();
        let previous_blank = std::mem::replace(&mut after_blank, blank);
        if let Some(f) = &fence {
            if fence_close(content, f) {
                fence = None;
            } else if f.mermaid {
                scan_wikis(content, base + q, lineno, LinkKind::Diagram, spans);
                mermaid_click(content, base + q, lineno, spans);
            }
            if out.is_some() {
                words += count_words(line);
            }
            continue;
        }
        if let Some(Html::Until(end)) = html {
            if contains_ignoring_case(content, end) {
                html = None;
            }
            if out.is_some() {
                words += count_words(line);
            }
            continue;
        }
        if matches!(html, Some(Html::UntilBlank)) && blank {
            html = None;
        }
        if !in_comment && html.is_none() {
            if tabs.last().is_some_and(|t| fence_close(content, t)) {
                tabs.pop();
                continue;
            }
            if !tabs.is_empty()
                && (line.starts_with("tab:")
                    || line
                        .strip_prefix("== ")
                        .is_some_and(|t| !t.trim().is_empty()))
            {
                continue;
            }
            let trimmed = content.trim();
            if directive_alone(trimmed, "ref") {
                ref_block = Some((lineno, Vec::new()));
                continue;
            }
            if let Some((first, parts)) = ref_block.take() {
                if directive_alone(trimmed, "endref") {
                    ref_link(first, &parts, spans);
                    continue;
                }
                if !trimmed.is_empty() {
                    let start = base + q + (content.len() - content.trim_start().len());
                    let mut parts = parts;
                    parts.push((start..start + trimmed.len(), trimmed.to_string()));
                    ref_block = Some((first, parts));
                    continue;
                }
            }
            if directive_alone(trimmed, "endref") {
                continue;
            }
        }
        let fence_here = fence_open(content)
            .or_else(|| after_list_marker(content).and_then(|at| fence_open(&content[at..])));
        if !in_comment && let Some(f) = fence_here {
            if f.tabs {
                tabs.push(f);
                continue;
            }
            fence = Some(f);
            if out.is_some() {
                words += count_words(line);
            }
            continue;
        }
        if !in_comment && html.is_none() {
            match html_open(content, previous_blank) {
                Some(Html::Until(end)) => {
                    let opened = content.trim_start();
                    let rest = match opened.strip_prefix("<!--") {
                        Some(comment) => comment,
                        None => opened.find('>').map_or("", |at| &opened[at + 1..]),
                    };
                    if !contains_ignoring_case(rest, end) {
                        html = Some(Html::Until(end));
                    }
                    if out.is_some() {
                        words += count_words(line);
                    }
                    continue;
                }
                opened => html = opened,
            }
        }
        let raw_html = html.is_some();
        let was_comment = in_comment;
        visible_segments(line, &mut in_comment, &mut segs);
        if let Some(out) = out.as_deref_mut() {
            words += count_words(line);
            if !was_comment && !raw_html {
                if q == 0
                    && let Some((level, text)) = atx_heading(line)
                {
                    if level == 1 && out.meta.title.is_none() {
                        out.meta.title = Some(text.to_string());
                    }
                    out.meta.headings.push(Heading {
                        level,
                        text: text.to_string(),
                        line: lineno,
                    });
                }
                if let Some(id) = block_id(line) {
                    let alone = line.trim_start().starts_with('^');
                    let target = match (alone, run_start) {
                        (true, Some(start)) if start == lineno => previous_run.unwrap_or(lineno),
                        (true, Some(start)) => start,
                        _ => lineno,
                    };
                    out.meta.blocks.push((id.to_string(), target));
                }
                if let Some((done, text)) = task(content) {
                    if done {
                        out.meta.tasks_done += 1;
                    } else {
                        out.meta.tasks_open += 1;
                    }
                    out.tasks.push(TaskLine {
                        line: lineno,
                        text: text.to_string(),
                        done,
                    });
                }
            }
        }
        for seg in segs.iter() {
            let tags = if raw_html { None } else { tags.as_deref_mut() };
            scan_inline(line, base, lineno, seg.clone(), tags, spans);
            if !raw_html {
                scan_directives(line, base, lineno, seg.clone(), spans);
            }
        }
    }

    if let Some(out) = out {
        out.meta.words = words;
        out.meta.body_start_line = lineno_after_fm;
    }
}

pub(crate) fn tag_name(raw: &str) -> Option<&str> {
    let t = raw.trim().trim_start_matches('#').trim_end_matches('/');
    let valid = !t.is_empty()
        && !t.bytes().all(|c| c.is_ascii_digit() || c == b'/')
        && t.chars().all(is_tag_char);
    valid.then_some(t)
}

fn push_tag(tags: &mut Vec<String>, raw: &str) {
    let Some(t) = tag_name(raw) else {
        return;
    };
    if !tags
        .iter()
        .any(|e| e.eq_ignore_ascii_case(t) || e.to_lowercase() == t.to_lowercase())
    {
        tags.push(t.to_string());
    }
}

fn apply_props(meta: &mut ParsedMeta, props: Vec<(String, PropValue)>) {
    for (key, value) in &props {
        match key.to_ascii_lowercase().as_str() {
            "tags" | "tag" => for_each_word(value, |w| push_tag(&mut meta.tags, w), true),
            "aliases" | "alias" => for_each_word(
                value,
                |w| {
                    if !w.is_empty() && !meta.aliases.iter().any(|a| a == w) {
                        meta.aliases.push(w.to_string());
                    }
                },
                false,
            ),
            _ => {}
        }
    }
    meta.props = props;
}

fn for_each_word(value: &PropValue, mut f: impl FnMut(&str), split_spaces: bool) {
    fn walk(value: &PropValue, f: &mut dyn FnMut(&str), split_spaces: bool, top: bool) {
        match value {
            PropValue::Text(s) if top || split_spaces => {
                for part in s.split(|c: char| c == ',' || (split_spaces && c.is_whitespace())) {
                    let p = part.trim();
                    if !p.is_empty() {
                        f(p);
                    }
                }
            }
            PropValue::Text(s) => f(s.trim()),
            PropValue::Number(n) if !split_spaces => f(&format_number(*n)),
            PropValue::List(items) => {
                for item in items {
                    walk(item, f, split_spaces, false);
                }
            }
            _ => {}
        }
    }
    walk(value, &mut f, split_spaces, true);
}

pub(crate) fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

fn indent_of(line: &str) -> usize {
    line.bytes()
        .take_while(|&b| b == b' ' || b == b'\t')
        .count()
}

fn is_blank_or_comment(line: &str) -> bool {
    let t = line.trim();
    t.is_empty() || t.starts_with('#')
}

pub(crate) fn parse_yaml(lines: &[&str]) -> Vec<(String, PropValue)> {
    let mut i = 0;
    let mut out = Vec::new();
    while i < lines.len() {
        if is_blank_or_comment(lines[i]) {
            i += 1;
            continue;
        }
        let indent = indent_of(lines[i]);
        match parse_map(lines, &mut i, indent) {
            PropValue::Map(entries) if !entries.is_empty() => out.extend(entries),
            _ => i += 1,
        }
    }
    out
}

fn split_key(t: &str) -> Option<(String, &str)> {
    let (key, rest) = if t.starts_with('"') || t.starts_with('\'') {
        let q = t.as_bytes()[0] as char;
        let end = t[1..].find(q)? + 1;
        let after = &t[end + 1..];
        let after = after.strip_prefix(':')?;
        (t[1..end].to_string(), after)
    } else {
        let mut idx = None;
        for (i, c) in t.char_indices() {
            if c == ':' {
                let next = t[i + 1..].chars().next();
                if next.is_none() || next == Some(' ') || next == Some('\t') {
                    idx = Some(i);
                    break;
                }
            }
        }
        let i = idx?;
        (t[..i].trim().to_string(), &t[i + 1..])
    };
    if key.is_empty() {
        return None;
    }
    Some((key, rest.trim()))
}

fn parse_map(lines: &[&str], i: &mut usize, indent: usize) -> PropValue {
    let mut entries = Vec::new();
    while *i < lines.len() {
        let line = lines[*i];
        if is_blank_or_comment(line) {
            *i += 1;
            continue;
        }
        let ind = indent_of(line);
        if ind != indent {
            break;
        }
        let t = line.trim();
        if t.starts_with("- ") || t == "-" {
            break;
        }
        let Some((key, rest)) = split_key(t) else {
            break;
        };
        *i += 1;
        let value = parse_value_after_key(lines, i, indent, rest);
        entries.push((key, value));
    }
    PropValue::Map(entries)
}

fn next_content(lines: &[&str], mut i: usize) -> Option<usize> {
    while i < lines.len() {
        if !is_blank_or_comment(lines[i]) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn parse_value_after_key(lines: &[&str], i: &mut usize, indent: usize, rest: &str) -> PropValue {
    if rest.is_empty() || rest.starts_with('#') {
        let Some(n) = next_content(lines, *i) else {
            return PropValue::Null;
        };
        let nind = indent_of(lines[n]);
        let nt = lines[n].trim();
        if (nt.starts_with("- ") || nt == "-") && nind >= indent {
            *i = n;
            return parse_list(lines, i, nind);
        }
        if nind > indent {
            *i = n;
            return parse_map(lines, i, nind);
        }
        return PropValue::Null;
    }
    if rest.starts_with('|') || rest.starts_with('>') {
        let folded = rest.starts_with('>');
        let mut parts = Vec::new();
        let mut block_indent = None;
        while *i < lines.len() {
            let l = lines[*i];
            if l.trim().is_empty() {
                parts.push(String::new());
                *i += 1;
                continue;
            }
            let ind = indent_of(l);
            if ind <= indent {
                break;
            }
            let bi = *block_indent.get_or_insert(ind);
            parts.push(l.get(bi.min(ind)..).unwrap_or("").to_string());
            *i += 1;
        }
        while parts.last().is_some_and(|p| p.is_empty()) {
            parts.pop();
        }
        let text = if folded {
            parts.join(" ")
        } else {
            parts.join("\n")
        };
        return PropValue::Text(text);
    }
    scalar(rest)
}

fn parse_list(lines: &[&str], i: &mut usize, indent: usize) -> PropValue {
    let mut items = Vec::new();
    while *i < lines.len() {
        let line = lines[*i];
        if is_blank_or_comment(line) {
            *i += 1;
            continue;
        }
        if indent_of(line) != indent {
            break;
        }
        let t = line.trim();
        let Some(rest) = t.strip_prefix('-') else {
            break;
        };
        if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
            break;
        }
        let rest = rest.trim();
        *i += 1;
        if rest.is_empty() {
            match next_content(lines, *i) {
                Some(n) if indent_of(lines[n]) > indent => {
                    *i = n;
                    let nind = indent_of(lines[n]);
                    let nt = lines[n].trim();
                    if nt.starts_with("- ") {
                        items.push(parse_list(lines, i, nind));
                    } else {
                        items.push(parse_map(lines, i, nind));
                    }
                }
                _ => items.push(PropValue::Null),
            }
        } else if let Some((key, value_text)) =
            split_key(rest).filter(|_| !rest.starts_with(['"', '\'', '[', '{']))
        {
            let item_indent = indent + 2;
            let first = parse_value_after_key(lines, i, item_indent, value_text);
            let mut entries = vec![(key, first)];
            if let PropValue::Map(more) = parse_map(lines, i, item_indent) {
                entries.extend(more);
            }
            items.push(PropValue::Map(entries));
        } else {
            items.push(scalar(rest));
        }
    }
    PropValue::List(items)
}

fn strip_comment(s: &str) -> &str {
    match s.find(" #") {
        Some(p) => s[..p].trim_end(),
        None => s,
    }
}

fn unquote(s: &str) -> Option<String> {
    let b = s.as_bytes();
    if b.len() >= 2 && b[0] == b'"' && b[b.len() - 1] == b'"' {
        let inner = &s[1..s.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(o) => out.push(o),
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        return Some(out);
    }
    if b.len() >= 2 && b[0] == b'\'' && b[b.len() - 1] == b'\'' {
        return Some(s[1..s.len() - 1].replace("''", "'"));
    }
    None
}

fn split_flow(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut start = 0;
    for (i, c) in inner.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '[' | '{') => depth += 1,
            (None, ']' | '}') => depth -= 1,
            (None, ',') if depth == 0 => {
                parts.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    parts
        .into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

pub(crate) fn scalar(raw: &str) -> PropValue {
    let s = raw.trim();
    if let Some(q) = unquote(s) {
        return PropValue::Text(q);
    }
    if s.starts_with('[') && s.ends_with(']') {
        return PropValue::List(
            split_flow(&s[1..s.len() - 1])
                .into_iter()
                .map(scalar)
                .collect(),
        );
    }
    if s.starts_with('{') && s.ends_with('}') {
        let entries = split_flow(&s[1..s.len() - 1])
            .into_iter()
            .filter_map(|p| split_key(p).map(|(k, v)| (k, scalar(v))))
            .collect();
        return PropValue::Map(entries);
    }
    let s = strip_comment(s);
    match s {
        "" | "~" | "null" | "Null" | "NULL" => return PropValue::Null,
        "true" | "True" | "TRUE" => return PropValue::Bool(true),
        "false" | "False" | "FALSE" => return PropValue::Bool(false),
        _ => {}
    }
    let b = s.as_bytes();
    let numeric = b.iter().any(|c| c.is_ascii_digit())
        && b.iter().enumerate().all(|(i, &c)| {
            c.is_ascii_digit()
                || c == b'.'
                || c == b'e'
                || c == b'E'
                || ((c == b'-' || c == b'+') && (i == 0 || matches!(b[i - 1], b'e' | b'E')))
        });
    if numeric && let Ok(n) = s.parse::<f64>() {
        return PropValue::Number(n);
    }
    PropValue::Text(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(s: &str) -> ParsedMeta {
        parse(s).meta
    }

    #[test]
    fn frontmatter_scalars_lists_and_order() {
        let m = meta(
            "---\ntitle: \"Index Red Team\"\ntype: index\ncount: 3\ndraft: false\nempty:\nlast_verified: \"2026-08-24\"\ndate: 2024-01-02\ntags: [red-team, index]\n---\n# Corps\n",
        );
        let keys: Vec<_> = m.props.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            [
                "title",
                "type",
                "count",
                "draft",
                "empty",
                "last_verified",
                "date",
                "tags"
            ]
        );
        assert_eq!(m.props[0].1, PropValue::Text("Index Red Team".into()));
        assert_eq!(m.props[2].1, PropValue::Number(3.0));
        assert_eq!(m.props[3].1, PropValue::Bool(false));
        assert_eq!(m.props[4].1, PropValue::Null);
        assert_eq!(m.props[6].1, PropValue::Text("2024-01-02".into()));
        assert_eq!(m.tags, ["red-team", "index"]);
        assert_eq!(m.title.as_deref(), Some("Corps"));
        assert_eq!(m.body_start_line, 10);
    }

    #[test]
    fn frontmatter_block_lists_maps_and_scalars() {
        let src = "---\naliases:\n  - Kerberoast\n  - \"SPN roasting\"\ntags:\n- attack/T1558\n- '#ad'\nnested:\n  a: 1\n  b:\n    - x\nitems:\n  - name: one\n    v: 2\n  - name: two\ndesc: |\n  ligne 1\n  ligne 2\n---\n";
        let m = meta(src);
        assert_eq!(m.aliases, ["Kerberoast", "SPN roasting"]);
        assert_eq!(m.tags, ["attack/T1558", "ad"]);
        let nested = &m.props.iter().find(|(k, _)| k == "nested").unwrap().1;
        assert_eq!(
            nested,
            &PropValue::Map(vec![
                ("a".into(), PropValue::Number(1.0)),
                (
                    "b".into(),
                    PropValue::List(vec![PropValue::Text("x".into())])
                )
            ])
        );
        let items = &m.props.iter().find(|(k, _)| k == "items").unwrap().1;
        assert_eq!(
            items,
            &PropValue::List(vec![
                PropValue::Map(vec![
                    ("name".into(), PropValue::Text("one".into())),
                    ("v".into(), PropValue::Number(2.0))
                ]),
                PropValue::Map(vec![("name".into(), PropValue::Text("two".into()))]),
            ])
        );
        let desc = &m.props.iter().find(|(k, _)| k == "desc").unwrap().1;
        assert_eq!(desc, &PropValue::Text("ligne 1\nligne 2".into()));
    }

    #[test]
    fn tags_as_strings() {
        assert_eq!(meta("---\ntags: a, b/c\n---\n").tags, ["a", "b/c"]);
        assert_eq!(
            meta("---\ntags: a b c # commentaire\n---\n").tags,
            ["a", "b", "c"]
        );
        assert_eq!(meta("---\ntag: solo\n---\n").tags, ["solo"]);
        assert_eq!(
            meta("---\naliases: Un, Deux\n---\n").aliases,
            ["Un", "Deux"]
        );
        assert_eq!(meta("---\ntags: []\n---\n").tags, Vec::<String>::new());
    }

    #[test]
    fn unclosed_frontmatter_is_body() {
        let m = meta("---\ntitle: x\n# Vrai titre\n");
        assert!(m.props.is_empty());
        assert_eq!(m.body_start_line, 0);
        assert_eq!(m.title.as_deref(), Some("Vrai titre"));
    }

    #[test]
    fn headings_outside_code() {
        let m = meta(
            "# Un\ntext\n```bash\n# commentaire\n```\n## Deux ##\n   ### Trois\n    # pas un titre\n#pas-titre\n####### sept\n> ## citation\n~~~\n# x\n~~~\n## Quatre",
        );
        let hs: Vec<_> = m
            .headings
            .iter()
            .map(|h| (h.level, h.text.as_str(), h.line))
            .collect();
        assert_eq!(
            hs,
            [
                (1, "Un", 0),
                (2, "Deux", 5),
                (3, "Trois", 6),
                (2, "Quatre", 14)
            ]
        );
        assert_eq!(m.tags, ["pas-titre"]);
    }

    #[test]
    fn title_prefers_h1_then_frontmatter() {
        let m = meta("---\ntitle: Depuis fm\n---\n## Pas h1\n# Le H1\n");
        assert_eq!(m.title.as_deref(), Some("Le H1"));
        assert_eq!(meta("---\ntitle: Fm\n---\ntexte").title, None);
    }

    #[test]
    fn body_tags() {
        let m = meta(
            "Un #tag et #a/b/c, pas#ici, ni #123 ni `#code`.\nhttps://x.com/#frag [lien](#ancre) #Tag #élévation #2024/q1 #x_y-z/ #/\n```\n#dans-code\n```\n%% #commentaire %% #après",
        );
        assert_eq!(
            m.tags,
            ["tag", "a/b/c", "élévation", "2024/q1", "x_y-z", "après"]
        );
    }

    fn links_of(m: &ParsedMeta) -> Vec<(LinkKind, &str)> {
        m.links
            .iter()
            .map(|l| (l.kind, l.target.as_str()))
            .collect()
    }

    #[test]
    fn a_tabs_fence_is_a_container_and_its_code_stays_code() {
        let m = meta(
            "````tabs\ntab: Linux\n## Depuis Linux\n- [ ] tester #linux [[Kerberoasting]]\n```bash\n#pas-un-tag [[pas un lien]]\ntab: pas un onglet\n```\nfin ^bloc-a\ntab: Windows\n> [!TIP]\n> voir [Rubeus](Rubeus.md)\n````\napres #apres\n",
        );
        assert_eq!(m.tags, ["linux", "apres"]);
        assert_eq!(
            links_of(&m),
            [
                (LinkKind::Wiki, "Kerberoasting"),
                (LinkKind::Markdown, "Rubeus.md")
            ]
        );
        assert_eq!(
            m.headings
                .iter()
                .map(|h| h.text.as_str())
                .collect::<Vec<_>>(),
            ["Depuis Linux"]
        );
        assert_eq!((m.tasks_open, m.blocks.len()), (1, 1));
    }

    #[test]
    fn a_short_tabs_fence_closes_on_a_bare_fence_and_an_open_one_loses_nothing() {
        let closed = meta("```tabs\ntab: A\n#un\n```\n#deux\n");
        assert_eq!(closed.tags, ["un", "deux"]);
        let open = meta("````tabs\ntab: A\ntexte #un [[Note]]\n");
        assert_eq!(open.tags, ["un"]);
        assert_eq!(links_of(&open), [(LinkKind::Wiki, "Note")]);
    }

    #[test]
    fn mdbook_tabs_are_markdown_and_their_directives_no_tags() {
        let m = meta(
            "{{#tabs}}\n{{#tab name=\"Rust\"}}\n#rust [[Overflow]]\n{{#endtab }}\n{{#tab name=\"C\"}}\n- [x] fait\n{{#endtab}}\n{{#endtabs}}\n",
        );
        assert_eq!(m.tags, ["rust"]);
        assert_eq!(links_of(&m), [(LinkKind::Wiki, "Overflow")]);
        assert_eq!(m.tasks_done, 1);
    }

    #[test]
    fn tab_titles_and_refs_read_as_focus_md_reads_them() {
        let m = meta(
            "````tabs\n== Linux\n#linux\n== \n````\n{{#ref }}\n../a\nb.md\n{{#endref }}\n\n{{#ref}}\n\nc.md\n{{#endref}}\n\nvoir {{#ref }}e.md{{#endref }} ici\n\n{{#ref}}\nd.md\n",
        );
        assert_eq!(m.tags, ["linux"]);
        assert_eq!(
            links_of(&m),
            [
                (LinkKind::Markdown, "../a b.md"),
                (LinkKind::Markdown, "e.md")
            ]
        );
    }

    #[test]
    fn mdbook_ref_is_a_link_and_include_an_embed() {
        let m = meta(
            "{{#include ../../banners/hacktricks-training.md}}\n\n  {{#ref}}\n  ../ntlm/README.md\n  {{#endref}}\n\nvoir [fast bin]({{#ref}}fast-bin-attack.md{{#endref}}) et {{#include code.rs:main }}\n```\n{{#include dans-du-code.md}}\n```\n",
        );
        assert_eq!(
            links_of(&m),
            [
                (LinkKind::Include, "../../banners/hacktricks-training.md"),
                (LinkKind::Markdown, "../ntlm/README.md"),
                (LinkKind::Markdown, "fast-bin-attack.md"),
                (LinkKind::Include, "code.rs")
            ]
        );
        let text = "x {{#ref}}a b.md{{#endref}}\n";
        let spans = link_spans(text);
        assert_eq!(&text[spans[0].target_range.clone()], "a b.md");
        assert!(spans[0].plain);
    }

    #[test]
    fn no_tag_in_a_fence_opened_on_a_list_item() {
        let m = meta(
            "- Flags:\n\n  - ```c\n    #define BIT (2U)\n    ```\n* ```c\n  #include <stdio.h>\n  ```\n1. ```\n   #pragma once\n   ```\n- after #kept",
        );
        assert_eq!(m.tags, ["kept"]);
    }

    #[test]
    fn no_tag_heading_or_link_in_a_pre_block_across_blank_lines() {
        let m = meta(
            "<pre class=\"language-bash\"><code># Execute\nrun # Take a shot\n\n# Screenshots\nadd sp, #0x20 [[Not a link]]\n</code></pre>\n\n# Real #kept",
        );
        assert_eq!(m.tags, ["kept"]);
        assert_eq!(
            m.headings
                .iter()
                .map(|h| h.text.as_str())
                .collect::<Vec<_>>(),
            ["Real #kept"]
        );
        assert!(m.links.is_empty());
    }

    #[test]
    fn no_tag_in_style_script_or_comment() {
        let m = meta(
            "<style>\n  .b { color: #cc0000; }\n\n  .c { color: #ff4d4d; }\n</style>\n<script setup>\nlet a = 1 // #Any\n</script>\n<!--\n#hidden\n-->\n<!-- one line #gone -->\n#kept",
        );
        assert_eq!(m.tags, ["kept"]);
    }

    #[test]
    fn no_tag_in_an_html_block_up_to_the_blank_line() {
        let m = meta(
            "<table><tr><td>(lldb) disas #Disas</td></tr>\n<tr><td>#Breakpoint</td></tr></table>\n\n<div>\n#inside\n</div>\n\n#kept",
        );
        assert_eq!(m.tags, ["kept"]);
    }

    #[test]
    fn inline_html_and_plain_code_like_text_keep_their_tags() {
        let m = meta(
            "<b>bold</b> #first\n\n  - Syntax: add Xn1 | #imm\n\nmN = copy() #Independent copy\\\n\nhttps://x.com/#frag [a](#anchor) #10 #0x10",
        );
        assert_eq!(m.tags, ["first", "imm", "Independent", "0x10"]);
    }

    #[test]
    fn wiki_links() {
        let m = meta(
            "[[Note]] [[dir/Note#Titre|alias]] ![[image.png|300]] [[#Local]] [[a\\|b]] [[Note#^bloc]]\n`[[pas un lien]]`",
        );
        let ls: Vec<_> = m
            .links
            .iter()
            .map(|l| (l.kind, l.target.as_str(), l.anchor.as_deref()))
            .collect();
        assert_eq!(
            ls,
            [
                (LinkKind::Wiki, "Note", None),
                (LinkKind::Wiki, "dir/Note", Some("Titre")),
                (LinkKind::Embed, "image.png", None),
                (LinkKind::Wiki, "", Some("Local")),
                (LinkKind::Wiki, "a", None),
                (LinkKind::Wiki, "Note", Some("^bloc")),
            ]
        );
    }

    #[test]
    fn markdown_links() {
        let src = "[A](Red_Team/Windows%20notes.md) ![](./assets/Pasted%20image.png) [B](<dir/with space.md#Une section> \"titre\") [C](https://x.org/a#b) [D](#ancre%20x) [E](f(1).md)";
        let m = meta(src);
        let ls: Vec<_> = m
            .links
            .iter()
            .map(|l| (l.kind, l.target.as_str(), l.anchor.as_deref()))
            .collect();
        assert_eq!(
            ls,
            [
                (LinkKind::Markdown, "Red_Team/Windows notes.md", None),
                (LinkKind::Embed, "./assets/Pasted image.png", None),
                (LinkKind::Markdown, "dir/with space.md", Some("Une section")),
                (LinkKind::Markdown, "https://x.org/a#b", None),
                (LinkKind::Markdown, "", Some("ancre x")),
                (LinkKind::Markdown, "f(1).md", None),
            ]
        );
    }

    #[test]
    fn link_spans_point_at_raw_target() {
        let src = "x [[dir/Note#T|a]] y [b](Mon%20fichier.md#s)\n![[img.png]]";
        let spans = link_spans(src);
        assert_eq!(&src[spans[0].target_range.clone()], "dir/Note");
        assert_eq!(&src[spans[0].full.clone()], "[[dir/Note#T|a]]");
        assert_eq!(&src[spans[1].target_range.clone()], "Mon%20fichier.md");
        assert_eq!(&src[spans[1].full.clone()], "[b](Mon%20fichier.md#s)");
        assert_eq!(&src[spans[2].full.clone()], "![[img.png]]");
        assert_eq!(spans[2].line, 1);
    }

    #[test]
    fn links_in_callouts_and_code() {
        let m = meta("> [!tip] Navigation\n> voir [[Cible]]\n> ```\n> [[Code]]\n> ```\n");
        assert_eq!(m.links.len(), 1);
        assert_eq!(m.links[0].target, "Cible");
    }

    #[test]
    fn tasks_and_words() {
        let p = parse(
            "---\na: b c d\n---\n- [ ] faire ça\n- [x] fait\n* [/] en cours\n1. [ ] numéro\n- [] pas une tâche\n-[ ] non plus\n> - [ ] dans une citation\nl'élévation de privilèges",
        );
        assert_eq!(p.meta.tasks_open, 3);
        assert_eq!(p.meta.tasks_done, 2);
        assert_eq!(
            p.tasks[0],
            TaskLine {
                line: 3,
                text: "faire ça".into(),
                done: false
            }
        );
        assert!(p.tasks[2].done);
        assert_eq!(p.meta.words, 20);
    }

    #[test]
    fn task_marks() {
        assert_eq!(task_mark("- [ ] a"), Some((3, false)));
        assert_eq!(task_mark("> 1. [x] a"), Some((6, true)));
        assert_eq!(task_mark("- a [ ] b"), None);
    }

    #[test]
    fn crlf_and_empty() {
        let m = meta("---\r\ntags: [a]\r\n---\r\n# T\r\n[[L]]\r\n");
        assert_eq!(m.tags, ["a"]);
        assert_eq!(m.title.as_deref(), Some("T"));
        assert_eq!(m.links[0].line, 4);
        let e = meta("");
        assert!(e.links.is_empty() && e.headings.is_empty());
    }

    #[test]
    fn external_detection() {
        assert!(is_external("https://a"));
        assert!(is_external("mailto:x@y"));
        assert!(!is_external("C:\\x"));
        assert!(!is_external("note.md"));
        assert!(!is_external("dir/a:b.md"));
    }

    #[test]
    fn block_ids_end_a_line() {
        assert_eq!(block_id("Paragraph ^abc-1"), Some("abc-1"));
        assert_eq!(block_id("^table-id  "), Some("table-id"));
        assert_eq!(block_id("x^2"), None);
        assert_eq!(block_id("text ^"), None);
        assert_eq!(block_id("text ^a_b"), None);
        let m = meta("# T\n```\ncode ^in-code\n```\n%% hidden ^in-comment %%\nline ^kept\n");
        assert_eq!(m.blocks, [("kept".to_string(), 5)]);
        let m = meta("intro\n\n- a\n- b\n\n^list-id\n\n| x |\n| - |\n| 1 |\n^table-id\n");
        assert_eq!(
            m.blocks,
            [("list-id".to_string(), 2), ("table-id".to_string(), 7)]
        );
    }
}
