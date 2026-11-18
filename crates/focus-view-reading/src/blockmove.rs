use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub units: Vec<Range<u32>>,
    edges: Vec<Edges>,
    pub list: bool,
}

impl Group {
    pub fn holding(&self, line: u32) -> Option<usize> {
        self.units.iter().position(|lines| lines.contains(&line))
    }

    pub fn starting(&self, line: u32) -> Option<usize> {
        self.units.iter().position(|lines| lines.start == line)
    }

    pub fn merge(&mut self, lines: Range<u32>) {
        let inside: Vec<usize> = (0..self.units.len())
            .filter(|&i| lines.start <= self.units[i].start && self.units[i].end <= lines.end)
            .collect();
        let (Some(&first), Some(&last)) = (inside.first(), inside.last()) else {
            return;
        };
        if first == last {
            return;
        }
        self.units[first].end = self.units[last].end;
        self.edges[first] = Edges {
            starts: self.edges[first].starts,
            ..self.edges[last]
        };
        self.units.drain(first + 1..=last);
        self.edges.drain(first + 1..=last);
    }
}

#[derive(Clone, Debug)]
pub struct Outline {
    pub top: Group,
    pub lists: Vec<Group>,
}

impl Outline {
    pub fn holding(&self, line: u32) -> Option<(&Group, usize)> {
        let item = self
            .lists
            .iter()
            .filter_map(|group| Some((group, group.holding(line)?)))
            .min_by_key(|(group, at)| group.units[*at].end - group.units[*at].start);
        item.or_else(|| Some((&self.top, self.top.holding(line)?)))
    }

    pub fn item_starting(&self, line: u32) -> Option<(&Group, usize)> {
        self.lists
            .iter()
            .find_map(|group| Some((group, group.starting(line)?)))
    }
}

struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
        Lines { text, starts }
    }

    fn start(&self, line: u32) -> usize {
        self.starts
            .get(line as usize)
            .copied()
            .unwrap_or(self.text.len())
    }

    fn end(&self, line: u32) -> usize {
        let next = self.start(line + 1);
        let start = self.start(line);
        let raw = &self.text[start..next];
        start + raw.trim_end_matches('\n').trim_end_matches('\r').len()
    }

    fn blank(&self, line: u32) -> bool {
        self.text[self.start(line)..self.end(line)]
            .trim()
            .is_empty()
    }

    fn trimmed(&self, lines: Range<u32>) -> Range<u32> {
        let mut end = lines.end.min(self.starts.len() as u32);
        while end > lines.start + 1 && self.blank(end - 1) {
            end -= 1;
        }
        lines.start..end.max(lines.start + 1)
    }

    fn span(&self, lines: &Range<u32>) -> Range<usize> {
        self.start(lines.start)..self.end(lines.end.saturating_sub(1).max(lines.start))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Edges {
    ends: bool,
    starts: bool,
    raw: bool,
}

fn edges(block: &focus_md::Block, lines: &Lines) -> Edges {
    use focus_md::BlockKind as K;
    let first =
        lines.text[lines.start(block.lines.start)..lines.end(block.lines.start)].trim_start();
    let fence = first.starts_with("```") || first.starts_with("~~~");
    let starts = match &block.kind {
        K::Heading { .. } => first.starts_with('#'),
        K::CodeBlock { .. } | K::Mermaid { .. } | K::Tabs { .. } => fence,
        _ => false,
    };
    Edges {
        ends: matches!(
            block.kind,
            K::Heading { .. } | K::CodeBlock { .. } | K::Mermaid { .. } | K::Math { .. } | K::Rule
        ) || (fence && matches!(block.kind, K::Tabs { .. })),
        starts,
        raw: matches!(block.kind, K::Html(_) | K::Details { .. }),
    }
}

pub fn outline(text: &str) -> Outline {
    let document = focus_md::parse(text, &focus_md::ParseOptions::default());
    let lines = Lines::new(text);
    let top = Group {
        units: document
            .blocks
            .iter()
            .map(|b| lines.trimmed(b.lines.clone()))
            .collect(),
        edges: document.blocks.iter().map(|b| edges(b, &lines)).collect(),
        list: false,
    };
    let mut lists = Vec::new();
    for block in &document.blocks {
        collect_lists(block, &lines, &mut lists);
    }
    Outline { top, lists }
}

fn collect_lists(block: &focus_md::Block, lines: &Lines, out: &mut Vec<Group>) {
    let focus_md::BlockKind::List { items, .. } = &block.kind else {
        return;
    };
    out.push(Group {
        units: items
            .iter()
            .map(|i| lines.trimmed(i.lines.clone()))
            .collect(),
        edges: vec![Edges::default(); items.len()],
        list: true,
    });
    for item in items {
        for inner in &item.blocks {
            collect_lists(inner, lines, out);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub range: Range<usize>,
    pub text: String,
    pub start: usize,
    pub len: usize,
    pub was: usize,
}

fn has_blank_line(gap: &str) -> bool {
    let parts: Vec<&str> = gap.split('\n').collect();
    parts.len() > 2
        && parts[1..parts.len() - 1]
            .iter()
            .any(|line| line.trim().is_empty())
}

pub fn moved(text: &str, group: &Group, from: usize, to: usize) -> Option<Moved> {
    let count = group.units.len();
    if from >= count || to > count || to == from || to == from + 1 {
        return None;
    }
    let lines = Lines::new(text);
    let spans: Vec<Range<usize>> = group.units.iter().map(|u| lines.span(u)).collect();
    let gaps: Vec<&str> = spans
        .windows(2)
        .map(|w| &text[w[0].end..w[1].start])
        .collect();
    let mut order: Vec<usize> = (0..count).filter(|&i| i != from).collect();
    order.insert(if to > from { to - 1 } else { to }, from);
    if !group.list {
        let meet = |upper: usize, lower: Option<usize>| {
            let (over, under) = (group.edges[upper], lower.map(|l| group.edges[l]));
            over.ends || under.is_some_and(|under| under.starts && !over.raw)
        };
        for (place, gap) in gaps.iter().enumerate() {
            let (upper, lower) = (order[place], order[place + 1]);
            let same = upper == place && lower == place + 1;
            if !same && !has_blank_line(gap) && !meet(upper, Some(lower)) {
                return None;
            }
        }
        let last = order[count - 1];
        let under = text[spans[count - 1].end..]
            .split('\n')
            .nth(1)
            .is_some_and(|line| !line.trim().is_empty());
        if last != count - 1 && under && !meet(last, None) {
            return None;
        }
    }
    let region = spans[0].start..spans[count - 1].end;
    let mut new = String::with_capacity(region.len());
    let mut start = 0;
    for (place, &unit) in order.iter().enumerate() {
        if unit == from {
            start = region.start + new.len();
        }
        new.push_str(&text[spans[unit].clone()]);
        if let Some(gap) = gaps.get(place) {
            new.push_str(gap);
        }
    }
    let old = &text[region.clone()];
    let mut head = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(head) || !new.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = old[head..]
        .bytes()
        .rev()
        .zip(new[head..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - tail) || !new.is_char_boundary(new.len() - tail) {
        tail -= 1;
    }
    Some(Moved {
        range: region.start + head..region.end - tail,
        text: new[head..new.len() - tail].to_string(),
        start,
        len: spans[from].len(),
        was: spans[from].start,
    })
}

pub fn step(text: &str, outline: &Outline, line: u32, down: bool) -> Option<Moved> {
    let (group, at) = outline.holding(line)?;
    step_in(text, group, at, down)
}

pub fn step_in(text: &str, group: &Group, at: usize, down: bool) -> Option<Moved> {
    let to = if down { at + 2 } else { at.checked_sub(1)? };
    if to > group.units.len() {
        return None;
    }
    moved(text, group, at, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, moved: &Moved) -> String {
        format!(
            "{}{}{}",
            &text[..moved.range.start],
            moved.text,
            &text[moved.range.end..]
        )
    }

    fn top_move(text: &str, from: usize, to: usize) -> Option<String> {
        let outline = outline(text);
        moved(text, &outline.top, from, to).map(|m| {
            let out = apply(text, &m);
            let unit = &outline.top.units[from];
            let lines = Lines::new(text);
            assert_eq!(
                &out[m.start..m.start + m.len],
                &text[lines.span(unit)],
                "the bytes of the block are the same"
            );
            out
        })
    }

    const NOTE: &str = "# Title\n\nFirst paragraph,\ntwo lines.\n\n> A quote\n\n```rust\nlet a = 1;\n```\n\nLast **one**.\n";

    #[test]
    fn a_block_moves_over_the_blank_lines_kept_in_place() {
        assert_eq!(
            top_move(NOTE, 4, 1).as_deref(),
            Some(
                "# Title\n\nLast **one**.\n\nFirst paragraph,\ntwo lines.\n\n> A quote\n\n```rust\nlet a = 1;\n```\n"
            ),
            "up to the second place"
        );
        assert_eq!(
            top_move(NOTE, 1, 5).as_deref(),
            Some(
                "# Title\n\n> A quote\n\n```rust\nlet a = 1;\n```\n\nLast **one**.\n\nFirst paragraph,\ntwo lines.\n"
            ),
            "down to the last place"
        );
        assert_eq!(
            top_move(NOTE, 3, 0).as_deref(),
            Some(
                "```rust\nlet a = 1;\n```\n\n# Title\n\nFirst paragraph,\ntwo lines.\n\n> A quote\n\nLast **one**.\n"
            ),
            "to the first place"
        );
        assert_eq!(top_move(NOTE, 2, 2), None, "its own place");
        assert_eq!(top_move(NOTE, 2, 3), None, "its own place, after itself");
    }

    #[test]
    fn a_note_in_crlf_keeps_its_endings() {
        let text = "a\r\n\r\nb\r\n\r\nc\r\n";
        assert_eq!(
            top_move(text, 2, 0).as_deref(),
            Some("c\r\n\r\na\r\n\r\nb\r\n")
        );
    }

    #[test]
    fn a_block_never_lands_where_it_would_join_its_neighbour() {
        let text = "Intro.\n\n# Head\nBody.\n";
        assert_eq!(top_move(text, 0, 2), None, "under the heading, over Body");
        assert_eq!(
            top_move(text, 2, 0).as_deref(),
            Some("Body.\n\nIntro.\n# Head\n"),
            "a heading starts under a paragraph"
        );
        let text = "Intro.\n\n<div>\n\n# Head\nBody.\n";
        assert_eq!(
            top_move(text, 1, 3),
            None,
            "an HTML block runs on to the paragraph under it"
        );
    }

    #[test]
    fn a_list_item_moves_with_its_sub_items_among_its_siblings() {
        let text = "- one\n- two\n  - two a\n  - two b\n- three\n\nAfter.\n";
        let outline = outline(text);
        let (group, at) = outline.item_starting(1).expect("the item of `two`");
        assert_eq!(group.units.len(), 3);
        let up = moved(text, group, at, 0).expect("to the first place");
        assert_eq!(
            apply(text, &up),
            "- two\n  - two a\n  - two b\n- one\n- three\n\nAfter.\n"
        );
        let down = moved(text, group, at, 3).expect("to the last place");
        assert_eq!(
            apply(text, &down),
            "- one\n- three\n- two\n  - two a\n  - two b\n\nAfter.\n"
        );
        let (inner, at) = outline.item_starting(3).expect("the sub-item `two b`");
        assert_eq!(inner.units.len(), 2);
        let swap = moved(text, inner, at, 0).expect("among the sub-items");
        assert_eq!(
            apply(text, &swap),
            "- one\n- two\n  - two b\n  - two a\n- three\n\nAfter.\n"
        );
    }

    #[test]
    fn a_loose_list_keeps_its_blank_lines() {
        let text = "- one\n\n- two\n\n- three\n";
        let outline = outline(text);
        let (group, at) = outline.item_starting(4).expect("the last item");
        let up = moved(text, group, at, 0).expect("to the first place");
        assert_eq!(apply(text, &up), "- three\n\n- one\n\n- two\n");
    }

    #[test]
    fn the_caret_line_finds_the_innermost_unit() {
        let text = "Para.\n\n- a\n  - b\n";
        let outline = outline(text);
        let (group, at) = outline.holding(3).expect("the sub-item");
        assert!(group.list && group.units[at] == (3..4));
        let (group, at) = outline.holding(0).expect("the paragraph");
        assert!(!group.list && at == 0);
        let moved = step(text, &outline, 0, true).expect("down");
        assert_eq!(apply(text, &moved), "- a\n  - b\n\nPara.\n");
        assert_eq!(step(text, &outline, 0, false), None, "first already");
    }

    #[test]
    fn a_folded_section_moves_as_one() {
        let text = "# A\n\nA body.\n\n# B\n\nB body.\n";
        let mut group = outline(text).top;
        group.merge(0..3);
        assert_eq!(group.units, vec![0..3, 4..5, 6..7]);
        let moved = moved(text, &group, 0, 3).expect("to the end");
        assert_eq!(apply(text, &moved), "# B\n\nB body.\n\n# A\n\nA body.\n");
    }
}
