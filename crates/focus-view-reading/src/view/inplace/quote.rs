use std::ops::Range;

use gpui::{Context, Window};
use gpui_component::RopeExt as _;

use super::super::ReadingView;
use super::super::picker::PickerKind;
use super::{At, Edit, Key};
use crate::model::Node;

pub(crate) fn key(at: &At, key: &Key) -> Option<Edit> {
    if !matches!(key, Key::Enter | Key::ShiftEnter) {
        return None;
    }
    let rows = at.item?.lines.clone();
    let rows = rows.start as usize..rows.end as usize;
    let base = at.text.line_start_offset(rows.start);
    let source = at.text.slice_lines(rows).to_string();
    enter(&source, base, at.caret, at.selection.clone())
}

fn enter(source: &str, base: usize, caret: usize, selection: Range<usize>) -> Option<Edit> {
    let lines: Vec<(usize, &str)> = source
        .split('\n')
        .scan(base, |start, piece| {
            let line = (*start, piece);
            *start += piece.len() + 1;
            Some(line)
        })
        .collect();
    let row = lines
        .iter()
        .rposition(|(start, text)| *start <= caret && caret <= start + text.len())?;
    let (start, raw) = lines[row];
    let crlf = raw.ends_with('\r');
    let text = raw.strip_suffix('\r').unwrap_or(raw);
    let eol = if crlf { "\r\n" } else { "\n" };
    let (prefix, depth) = markers(text);
    if depth == 0 {
        return None;
    }
    let deeper = |r: usize| {
        lines
            .get(r)
            .is_some_and(|(_, next)| markers(next.trim_end_matches('\r')).1 >= depth)
    };
    let empty = text[prefix.len()..].trim().is_empty();
    if empty && selection.is_empty() && !deeper(row + 1) {
        let outer = outer(&prefix);
        let written = format!("{}{eol}{outer}", outer.trim_end());
        return Some(Edit {
            range: start..start + text.len(),
            caret: start + written.len(),
            text: written,
        });
    }
    let written = format!("{eol}{prefix}");
    Some(Edit {
        caret: selection.start + written.len(),
        range: selection,
        text: written,
    })
}

fn markers(text: &str) -> (String, usize) {
    let bytes = text.as_bytes();
    let (mut at, mut depth, mut end) = (0, 0, 0);
    while at < bytes.len() {
        match bytes[at] {
            b' ' | b'\t' => at += 1,
            b'>' => {
                at += 1;
                if bytes.get(at) == Some(&b' ') {
                    at += 1;
                }
                depth += 1;
                end = at;
            }
            b'-' | b'*' | b'+' if bytes.get(at + 1) == Some(&b' ') => at += 2,
            b'0'..=b'9' => {
                let digits = bytes[at..]
                    .iter()
                    .take_while(|b| b.is_ascii_digit())
                    .count();
                match bytes.get(at + digits..at + digits + 2) {
                    Some([b'.' | b')', b' ']) => at += digits + 2,
                    _ => break,
                }
            }
            _ => break,
        }
    }
    let prefix = text[..end]
        .chars()
        .map(|c| {
            if matches!(c, ' ' | '\t' | '>') {
                c
            } else {
                ' '
            }
        })
        .collect();
    (prefix, depth)
}

fn outer(prefix: &str) -> String {
    match prefix.rfind('>') {
        Some(at) => prefix[..at].to_string(),
        None => String::new(),
    }
}

pub(in crate::view) fn icon_clicked(
    view: &mut ReadingView,
    item: usize,
    callout: u32,
    window: &mut Window,
    cx: &mut Context<ReadingView>,
) -> bool {
    let Some(field) = view.in_place.clone() else {
        return false;
    };
    let Some(lines) = view
        .built
        .as_ref()
        .and_then(|b| b.items.get(item))
        .map(|i| {
            let mut keys = Vec::new();
            callouts(&i.node, &mut keys);
            (i.lines.clone(), keys)
        })
    else {
        return false;
    };
    let (rows, keys) = lines;
    let Some(rank) = keys.iter().position(|&k| k == callout) else {
        return false;
    };
    let text = field.read(cx).text();
    let base = text.line_start_offset(rows.start as usize);
    let source = text
        .slice_lines(rows.start as usize..rows.end as usize)
        .to_string();
    let Some(kind) = type_bytes(&source, rank) else {
        return false;
    };
    let line = text.offset_to_point(base + kind.start).row as u32;
    let kind = PickerKind::CalloutType {
        start: base + kind.start,
        end: base + kind.end,
    };
    view.open_picker_of(kind, line, window.mouse_position(), window, cx);
    true
}

fn callouts(node: &Node, out: &mut Vec<u32>) {
    let children: &[Node] = match node {
        Node::Callout { key, children, .. } => {
            out.push(*key);
            children
        }
        Node::Quote(children) | Node::Details { children, .. } => children,
        Node::List { entries, .. } => {
            for entry in entries {
                entry.children.iter().for_each(|c| callouts(c, out));
            }
            return;
        }
        Node::Tabs { tabs, .. } => {
            for tab in tabs {
                tab.items.iter().for_each(|i| callouts(&i.node, out));
            }
            return;
        }
        _ => return,
    };
    children.iter().for_each(|c| callouts(c, out));
}

fn type_bytes(source: &str, rank: usize) -> Option<Range<usize>> {
    fn walk(blocks: &[focus_md::Block], out: &mut Vec<Range<usize>>) {
        for block in blocks {
            match &block.kind {
                focus_md::BlockKind::Callout { marker, blocks, .. } => {
                    out.push(marker.clone());
                    walk(blocks, out);
                }
                focus_md::BlockKind::Quote(blocks)
                | focus_md::BlockKind::Details { blocks, .. } => walk(blocks, out),
                focus_md::BlockKind::List { items, .. } => {
                    items.iter().for_each(|i| walk(&i.blocks, out))
                }
                focus_md::BlockKind::Tabs { tabs, .. } => {
                    tabs.iter().for_each(|t| walk(&t.blocks, out))
                }
                _ => {}
            }
        }
    }
    let document = focus_md::parse(source, &focus_md::ParseOptions::default());
    let mut markers = Vec::new();
    walk(&document.blocks, &mut markers);
    let marker = markers.get(rank)?;
    let close = source[marker.clone()].find(']')?;
    Some(marker.start + 2..marker.start + close)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn after(note: &str) -> Option<String> {
        let caret = note.find('|').expect("a caret");
        let source = note.replacen('|', "", 1);
        let edit = enter(&source, 0, caret, caret..caret)?;
        let mut out = source;
        out.replace_range(edit.range.clone(), &edit.text);
        out.insert(edit.caret, '|');
        Some(out)
    }

    #[test]
    fn enter_writes_the_markers_of_its_level() {
        assert_eq!(after("> a|\n").as_deref(), Some("> a\n> |\n"));
        assert_eq!(
            after("> [!note]- Ti|tle\n> body").as_deref(),
            Some("> [!note]- Ti\n> |tle\n> body")
        );
        assert_eq!(after("> > deep|\n").as_deref(), Some("> > deep\n> > |\n"));
        assert_eq!(after(">no space|").as_deref(), Some(">no space\n>|"));
        assert_eq!(
            after("- > in a list|").as_deref(),
            Some("- > in a list\n  > |")
        );
        assert_eq!(
            after("> a|\r\n> b\r\n").as_deref(),
            Some("> a\r\n> |\r\n> b\r\n")
        );
        assert_eq!(after("plain|"), None);
    }

    #[test]
    fn enter_on_an_empty_last_line_leaves_the_quote() {
        assert_eq!(
            after("> [!tip] T\n> body\n> |").as_deref(),
            Some("> [!tip] T\n> body\n\n|")
        );
        assert_eq!(
            after("> a\n> |\n\nafter\n").as_deref(),
            Some("> a\n\n|\n\nafter\n")
        );
        assert_eq!(after("> a\n> |\n> b").as_deref(), Some("> a\n> \n> |\n> b"));
        assert_eq!(after("> > a\n> > |").as_deref(), Some("> > a\n>\n> |"));
    }

    #[test]
    fn the_type_of_each_callout_is_found_in_order() {
        let source = "> [!warning]- Out\n> > [!tip] In\n> body\n";
        let first = type_bytes(source, 0).expect("first");
        let second = type_bytes(source, 1).expect("second");
        assert_eq!((&source[first], &source[second]), ("warning", "tip"));
        assert_eq!(type_bytes(source, 2), None);
        let in_list = "- item\n  > [!note] In a list\n";
        assert_eq!(&in_list[type_bytes(in_list, 0).expect("in a list")], "note");
    }
}
