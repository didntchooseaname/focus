use std::ops::Range;
use std::sync::Arc;

use gpui::Context;
use gpui_component::{Rope, RopeExt as _};

use crate::model::{self, Node};
use crate::view::ReadingView;

fn common_ends(old: &Rope, new: &Rope) -> (usize, usize) {
    let mut prefix = 0;
    let (mut a, mut b) = (old.chunks(), new.chunks());
    let (mut ca, mut cb): (&[u8], &[u8]) = (&[], &[]);
    loop {
        if ca.is_empty() {
            match a.next() {
                Some(c) => ca = c.as_bytes(),
                None => break,
            }
        }
        if cb.is_empty() {
            match b.next() {
                Some(c) => cb = c.as_bytes(),
                None => break,
            }
        }
        let n = ca.len().min(cb.len());
        let same = ca[..n]
            .iter()
            .zip(&cb[..n])
            .take_while(|(x, y)| x == y)
            .count();
        prefix += same;
        if same < n {
            break;
        }
        ca = &ca[n..];
        cb = &cb[n..];
    }
    while prefix > 0 && !(old.is_char_boundary(prefix) && new.is_char_boundary(prefix)) {
        prefix -= 1;
    }
    let room = old.len().min(new.len()) - prefix;
    let mut suffix = 0;
    while suffix < room {
        let (a, a_start) = old.chunk(old.len() - suffix - 1);
        let (b, b_start) = new.chunk(new.len() - suffix - 1);
        let a = &a.as_bytes()[..old.len() - suffix - a_start];
        let b = &b.as_bytes()[..new.len() - suffix - b_start];
        let n = a.len().min(b.len()).min(room - suffix);
        let same = a
            .iter()
            .rev()
            .zip(b.iter().rev())
            .take(n)
            .take_while(|(x, y)| x == y)
            .count();
        suffix += same;
        if same < n {
            break;
        }
    }
    while suffix > 0
        && !(old.is_char_boundary(old.len() - suffix) && new.is_char_boundary(new.len() - suffix))
    {
        suffix -= 1;
    }
    (prefix, suffix)
}

fn lines_in(text: &Rope, range: Range<usize>) -> i64 {
    text.slice(range).chars().filter(|c| *c == '\n').count() as i64
}

impl ReadingView {
    pub(super) fn rebuild_edited(&mut self, text: &Rope, cx: &mut Context<Self>) -> bool {
        let Some(old) = self.in_place_text.clone() else {
            return false;
        };
        if old.len() == text.len() && old == *text {
            return true;
        }
        let Some((meta, resolver)) = self.last_source.clone() else {
            return false;
        };
        let host_links = self.host_links.clone();
        let (prefix, suffix) = common_ends(&old, text);
        let old_range = prefix..old.len() - suffix;
        let new_range = prefix..text.len() - suffix;
        let delta = lines_in(text, new_range.clone()) - lines_in(&old, old_range.clone());
        let first = old.offset_to_position(old_range.start).line;
        let last = old.offset_to_position(old_range.end).line;
        let Some(built) = self.built.as_mut() else {
            return false;
        };
        let Some(built) = Arc::get_mut(built) else {
            return false;
        };
        let touches = |lines: &Option<Range<u32>>| {
            lines
                .as_ref()
                .is_some_and(|l| l.start <= last + 1 && first <= l.end)
        };
        if touches(&built.frontmatter_lines) || touches(&built.title_lines) {
            return false;
        }
        let count = built
            .items
            .iter()
            .position(|i| matches!(i.node, Node::Footnotes(_)))
            .unwrap_or(built.items.len());
        let window = first.saturating_sub(1)..last + 2;
        let a = built.items[..count]
            .iter()
            .position(|i| i.lines.end.max(i.lines.start + 1) > window.start)
            .unwrap_or(count);
        let b = built.items[..count]
            .iter()
            .position(|i| i.lines.start >= window.end)
            .unwrap_or(count)
            .max(a);
        let start_line = built
            .items
            .get(a)
            .filter(|_| a < b)
            .map_or(first, |i| i.lines.start.min(first));
        let end_line = built.items[..b]
            .last()
            .filter(|_| a < b)
            .map_or(last + 1, |i| i.lines.end.max(last + 1));
        let new_end = (end_line as i64 + delta).max(start_line as i64) as u32;
        let note = text.to_string();
        let items = model::build_items(
            &note,
            start_line..new_end,
            a,
            built,
            meta.as_deref(),
            resolver.as_ref(),
            host_links.as_deref(),
        );
        let added = items.len();
        built.splice_items(a..b, items, delta);
        built.source = Some(note.into());
        self.in_place_text = Some(text.clone());
        self.items_replaced(a..b, added);
        cx.notify();
        true
    }

    fn items_replaced(&mut self, range: Range<usize>, added: usize) {
        let (a, b) = (range.start, range.end);
        let changed = |(item, _): &(usize, u32)| (a..a + added).contains(item);
        self.decorated.borrow_mut().retain(|key, _| !changed(key));
        self.tables.borrow_mut().retain(|key, _| !changed(key));
        if added != b - a {
            self.decorated.borrow_mut().clear();
            self.tables.borrow_mut().clear();
            let moved = |item: usize| -> Option<usize> {
                if item < a {
                    Some(item)
                } else if item >= b {
                    Some(item + added - (b - a))
                } else {
                    None
                }
            };
            self.folded = self.folded.iter().filter_map(|&i| moved(i)).collect();
            self.callouts = std::mem::take(&mut self.callouts)
                .into_iter()
                .filter_map(|((item, key), open)| Some(((moved(item)?, key), open)))
                .collect();
        }
        self.splice_changed_rows(Some(a..a + added), added as i64 - (b - a) as i64);
    }

    pub(in crate::view) fn keep_caret_line(&mut self) {
        let wanted = self.in_place_frame.as_ref().and_then(|frame| {
            if frame.anchor != frame.head {
                return None;
            }
            let row = frame.text.offset_to_point(frame.head).row;
            let line = frame.text.slice_line(row).to_string();
            let bare = line.trim_start_matches(['>', ' ', '\t']);
            bare.trim().is_empty().then(|| {
                let start = frame.text.line_start_offset(row);
                (row as u32, start, frame.head, line.contains('>'))
            })
        });
        let Some(built) = self.built.as_ref() else {
            return;
        };
        let holds = |item: &model::Item, row: u32| {
            item.lines.start <= row && row < item.lines.end.max(item.lines.start + 1)
        };
        let target = wanted.and_then(|(row, start, head, quoted)| {
            let header = [&built.frontmatter_lines, &built.title_lines]
                .into_iter()
                .flatten()
                .any(|lines| lines.contains(&row));
            if header {
                return None;
            }
            let holder = built
                .items
                .iter()
                .position(|item| !is_caret_line(item) && holds(item, row));
            match holder {
                None if !quoted => {
                    let index = built.items.partition_point(|item| item.lines.start <= row);
                    Some((index, false, row, head - start))
                }
                Some(index)
                    if quoted
                        && matches!(
                            built.items[index].node,
                            Node::Quote(_) | Node::Callout { .. }
                        ) =>
                {
                    let origin = self.in_place_frame.as_ref().map(|f| {
                        f.text
                            .line_start_offset(built.items[index].lines.start as usize)
                    })?;
                    Some((index, true, row, head - origin))
                }
                _ => None,
            }
        });
        let existing = built.items.iter().enumerate().find_map(|(index, item)| {
            if is_caret_line(item) {
                Some((index, false))
            } else if last_child(&item.node).is_some_and(is_caret_text) {
                Some((index, true))
            } else {
                None
            }
        });
        let same = match (existing, target) {
            (Some((index, false)), Some((_, false, row, column))) => {
                let item = &built.items[index];
                item.lines.start == row && caret_of(&item.node) == Some(column)
            }
            (Some((index, true)), Some((at, true, _, column))) => {
                index == at
                    && last_child(&built.items[index].node).and_then(caret_of) == Some(column)
            }
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        let Some(built) = self.built.as_mut().and_then(Arc::get_mut) else {
            return;
        };
        let mut target = target;
        match existing {
            Some((index, false)) => {
                built.splice_items(index..index + 1, Vec::new(), 0);
                self.items_replaced(index..index + 1, 0);
                if let Some((at, false, ..)) = &mut target
                    && *at > index
                {
                    *at -= 1;
                }
            }
            Some((index, true)) => {
                if let Some(children) = children_mut(&mut built.items[index].node) {
                    children.pop();
                }
                self.items_replaced(index..index + 1, 1);
            }
            None => {}
        }
        let Some(built) = self.built.as_mut().and_then(Arc::get_mut) else {
            return;
        };
        match target {
            Some((index, false, row, column)) => {
                built.splice_items(
                    index..index,
                    vec![model::Item {
                        node: caret_paragraph(column),
                        lines: row..row + 1,
                    }],
                    0,
                );
                self.items_replaced(index..index, 1);
            }
            Some((index, true, _, column)) => {
                if let Some(children) = children_mut(&mut built.items[index].node) {
                    children.push(caret_paragraph(column));
                }
                self.items_replaced(index..index + 1, 1);
            }
            None => {}
        }
    }
}

const CARET_LINE: u32 = u32::MAX;

fn caret_paragraph(column: usize) -> Node {
    let mut rich = model::Rich {
        ord: CARET_LINE,
        ..model::Rich::default()
    };
    rich.map = Some(Arc::new(crate::srcmap::SourceMap {
        pieces: vec![crate::srcmap::Piece {
            shown: 0..0,
            source: column..column,
            exact: true,
        }],
        markup: Vec::new(),
    }));
    Node::Paragraph(rich)
}

fn is_caret_text(node: &Node) -> bool {
    matches!(node, Node::Paragraph(rich) if rich.ord == CARET_LINE)
}

fn is_caret_line(item: &model::Item) -> bool {
    is_caret_text(&item.node)
}

fn caret_of(node: &Node) -> Option<usize> {
    match node {
        Node::Paragraph(rich) => rich.map.as_ref()?.pieces.first().map(|p| p.source.start),
        _ => None,
    }
}

fn last_child(node: &Node) -> Option<&Node> {
    match node {
        Node::Quote(children) | Node::Callout { children, .. } => children.last(),
        _ => None,
    }
}

fn children_mut(node: &mut Node) -> Option<&mut Vec<Node>> {
    match node {
        Node::Quote(children) | Node::Callout { children, .. } => Some(children),
        _ => None,
    }
}

impl ReadingView {
    pub(in crate::view) fn splice_changed_rows(
        &mut self,
        items: Option<Range<usize>>,
        item_delta: i64,
    ) {
        let old = self.rows.clone();
        self.recompute();
        let new = &self.rows;
        let touched =
            |row: &super::super::Row| items.as_ref().is_some_and(|r| r.contains(&row.item));
        let prefix = old
            .iter()
            .zip(new.iter())
            .take_while(|(a, b)| a == b && !touched(b))
            .count();
        let suffix = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take(old.len().min(new.len()) - prefix)
            .take_while(|(a, b)| {
                !touched(b)
                    && a.part == b.part
                    && a.parts == b.parts
                    && a.editor == b.editor
                    && b.item as i64 == a.item as i64 + item_delta
            })
            .count();
        let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
        if prefix == old_end && prefix == new_end {
            return;
        }
        let top = self.list.logical_scroll_top();
        self.list.splice(prefix + 1..old_end + 1, new_end - prefix);
        if prefix < top.item_ix {
            self.list.scroll_to(top);
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui_component::Rope;

    use super::common_ends;

    fn ends(old: &str, new: &str) -> (usize, usize) {
        common_ends(&Rope::from(old), &Rope::from(new))
    }

    #[test]
    fn the_shared_ends_stop_at_a_character() {
        assert_eq!(ends("abc", "aXc"), (1, 1));
        assert_eq!(ends("aé", "aè"), (1, 0));
        assert_eq!(ends("éa", "èa"), (0, 1));
        assert_eq!(ends("- عنصر\n- ثان\n", "- عنصر\n- @\n- ثان\n"), (13, 7));
        let long = |c: &str| format!("{}{c}{}", "x".repeat(10_000), "y".repeat(10_000));
        assert_eq!(ends(&long("é"), &long("è")), (10_000, 10_000));
        assert_eq!(ends(&long(""), &long("")), (20_000, 0));
    }
}
