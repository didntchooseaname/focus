use std::ops::Range;

use gpui_component::RopeExt as _;

use super::{At, Edit, Key};

pub(crate) fn key(at: &At, key: &Key) -> Option<Edit> {
    if !matches!(key, Key::Enter | Key::Tab | Key::ShiftTab | Key::Text(_)) {
        return None;
    }
    let rows = at.item?.lines.clone();
    let rows = rows.start as usize..rows.end as usize;
    let base = at.text.line_start_offset(rows.start);
    let source = at.text.slice_lines(rows).to_string();
    let caret = at.caret.checked_sub(base)?;
    let selection = at.selection.start.checked_sub(base)?..at.selection.end.checked_sub(base)?;
    if selection.end.max(caret) > source.len() {
        return None;
    }
    let edit = rule(&source, caret, selection, key)?;
    Some(Edit {
        range: edit.range.start + base..edit.range.end + base,
        text: edit.text,
        caret: edit.caret + base,
    })
}

struct Block<'s> {
    source: &'s str,
    code: String,
    content: Range<usize>,
    bytes: Range<usize>,
}

fn block(source: &str, caret: usize) -> Option<Block<'_>> {
    fn walk(
        blocks: &[focus_md::Block],
        caret: usize,
        found: &mut Option<(String, Range<usize>, Range<usize>)>,
    ) {
        for block in blocks {
            match &block.kind {
                focus_md::BlockKind::CodeBlock { code, content, .. }
                    if content.start <= caret && caret <= content.end =>
                {
                    *found = Some((code.clone(), content.clone(), block.bytes.clone()));
                }
                focus_md::BlockKind::Quote(inner)
                | focus_md::BlockKind::Callout { blocks: inner, .. }
                | focus_md::BlockKind::Details { blocks: inner, .. } => walk(inner, caret, found),
                focus_md::BlockKind::List { items, .. } => {
                    items.iter().for_each(|i| walk(&i.blocks, caret, found))
                }
                focus_md::BlockKind::Tabs { tabs, .. } => {
                    tabs.iter().for_each(|t| walk(&t.blocks, caret, found))
                }
                _ => {}
            }
        }
    }
    let document = focus_md::parse(source, &focus_md::ParseOptions::default());
    let mut found = None;
    walk(&document.blocks, caret, &mut found);
    let (code, content, bytes) = found?;
    Some(Block {
        source,
        code,
        content,
        bytes,
    })
}

fn line_start(source: &str, at: usize) -> usize {
    source[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn rule(source: &str, caret: usize, selection: Range<usize>, key: &Key) -> Option<Edit> {
    if let Key::Text(_) = key {
        let start = line_start(source, caret);
        let bare = source[start..].trim_start_matches([' ', '\t', '>']);
        if !(bare.starts_with("```") || bare.starts_with("~~~")) {
            return None;
        }
    }
    let block = block(source, caret)?;
    if source[block.bytes.clone()].starts_with('<') {
        return None;
    }
    if block.code.is_empty() {
        return block.first_line(key, &selection);
    }
    let start = line_start(source, caret);
    let end = source[caret..]
        .find('\n')
        .map_or(source.len(), |i| caret + i);
    let crlf = source[start..end].ends_with('\r');
    let line = &source[start..end - usize::from(crlf)];
    let eol = if crlf { "\r\n" } else { "\n" };
    let (container, own) = block.split(start, line);
    let column = caret - start - container.len();
    if matches!(key, Key::Enter)
        && selection.is_empty()
        && let Some(edit) = block.leave(start, end, own, eol)
    {
        return Some(edit);
    }
    match key {
        Key::Enter => {
            let indent = own.len() - own.trim_start_matches([' ', '\t']).len();
            let text = format!("{eol}{container}{}", &own[..indent.min(column)]);
            Some(Edit {
                caret: selection.start + text.len(),
                range: selection,
                text,
            })
        }
        Key::Tab => {
            let text = block.unit();
            Some(Edit {
                caret: selection.start + text.len(),
                range: selection,
                text,
            })
        }
        Key::ShiftTab => {
            let unit = block.unit();
            let removed = if own.starts_with('\t') {
                1
            } else {
                own.bytes()
                    .take(unit.len())
                    .take_while(|&b| b == b' ')
                    .count()
            };
            let at = start + container.len();
            Some(Edit {
                range: at..at + removed,
                text: String::new(),
                caret: caret.saturating_sub(removed).max(at),
            })
        }
        _ => None,
    }
}

impl Block<'_> {
    fn leave(&self, start: usize, end: usize, own: &str, eol: &str) -> Option<Edit> {
        let fence_start = end + 1;
        if !own.trim().is_empty()
            || start <= line_start(self.source, self.content.start)
            || fence_start < self.content.end
            || fence_start >= self.bytes.end
        {
            return None;
        }
        let fence_end = self.source[fence_start..]
            .find('\n')
            .map_or(self.source.len(), |i| fence_start + i);
        let fence = self.source[fence_start..fence_end].trim_end_matches('\r');
        let bare = fence.trim_start_matches([' ', '\t', '>']);
        if !(bare.starts_with("```") || bare.starts_with("~~~")) {
            return None;
        }
        let text = format!("{fence}{eol}{}", self.container());
        Some(Edit {
            range: start..fence_start + fence.len(),
            caret: start + text.len(),
            text,
        })
    }

    fn container(&self) -> &str {
        &self.source[line_start(self.source, self.bytes.start)..self.bytes.start]
    }

    fn split<'l>(&self, start: usize, line: &'l str) -> (&'l str, &'l str) {
        let first = line_start(self.source, self.content.start);
        if start >= first {
            let row = self.source[first..start].matches('\n').count();
            if let Some(own) = self.code.split('\n').nth(row)
                && let Some(container) = line.strip_suffix(own)
            {
                return (container, &line[container.len()..]);
            }
        }
        let length = self.container().len();
        match line.get(..length) {
            Some(container) => (container, &line[length..]),
            None => ("", line),
        }
    }

    fn first_line(&self, key: &Key, selection: &Range<usize>) -> Option<Edit> {
        let container = self.container();
        let at = self.content.start;
        let crlf = self.source[..at].ends_with("\r\n");
        let eol = if crlf { "\r\n" } else { "\n" };
        let typed = match key {
            Key::Text(text) => text.clone(),
            Key::Tab => self.unit(),
            Key::Enter => format!("{eol}{container}"),
            _ => return None,
        };
        let line_start = at == 0 || self.source.as_bytes()[at - 1] == b'\n';
        let (text, caret) = if line_start {
            let text = format!("{container}{typed}{eol}");
            let caret = at + text.len() - eol.len();
            (text, caret)
        } else {
            let text = format!("{eol}{container}{typed}");
            let caret = at + text.len();
            (text, caret)
        };
        let range = if selection.is_empty() {
            at..at
        } else {
            selection.clone()
        };
        Some(Edit {
            caret: range.start + (caret - at),
            range,
            text,
        })
    }

    fn unit(&self) -> String {
        if self.code.lines().any(|l| l.starts_with('\t')) {
            return "\t".into();
        }
        let smallest = self
            .code
            .lines()
            .map(|l| l.len() - l.trim_start_matches(' ').len())
            .filter(|&n| n > 0 && n <= 8)
            .min()
            .unwrap_or(4);
        " ".repeat(smallest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(note: &str, key: Key) -> Option<String> {
        let caret = note.find('|').expect("a caret");
        let source = note.replacen('|', "", 1);
        let edit = rule(&source, caret, caret..caret, &key)?;
        let mut out = source;
        out.replace_range(edit.range.clone(), &edit.text);
        out.insert(edit.caret, '|');
        Some(out)
    }

    fn after(note: &str, key: Key) -> String {
        press(note, key).expect("a code rule")
    }

    #[test]
    fn enter_keeps_the_indent_of_its_line() {
        assert_eq!(
            after("```\nfn x() {|\n```", Key::Enter),
            "```\nfn x() {\n|\n```"
        );
        assert_eq!(
            after("```rust\n    let a = 1;|\n```", Key::Enter),
            "```rust\n    let a = 1;\n    |\n```"
        );
        assert_eq!(
            after("```\n\tif x|\n```", Key::Enter),
            "```\n\tif x\n\t|\n```"
        );
        assert_eq!(after("```\n  | x\n```", Key::Enter), "```\n  \n  | x\n```");
        assert_eq!(
            after("```\r\na|\r\n```\r\n", Key::Enter),
            "```\r\na\r\n|\r\n```\r\n"
        );
        assert_eq!(
            after("    indented|\n", Key::Enter),
            "    indented\n    |\n"
        );
    }

    #[test]
    fn enter_on_an_empty_last_line_leaves_the_block() {
        assert_eq!(
            press("```\nfn x() {}\n|\n```\nAfter.\n", Key::Enter).as_deref(),
            Some("```\nfn x() {}\n```\n|\nAfter.\n")
        );
        assert_eq!(
            press("~~~py\r\na = 1\r\n    |\r\n~~~", Key::Enter).as_deref(),
            Some("~~~py\r\na = 1\r\n~~~\r\n|")
        );
        assert_eq!(
            press("> ```\n> a\n> |\n> ```\n", Key::Enter).as_deref(),
            Some("> ```\n> a\n> ```\n> |\n")
        );
        assert_eq!(
            press("```\na\n|\nb\n```", Key::Enter).as_deref(),
            Some("```\na\n\n|\nb\n```")
        );
        assert_eq!(
            press("```\n|\n\n```", Key::Enter).as_deref(),
            Some("```\n\n|\n\n```")
        );
        assert_eq!(press("```\na\n|", Key::Enter), None);
    }

    #[test]
    fn code_in_containers_keeps_their_markers() {
        assert_eq!(
            after("> ```\n>   x|\n> ```", Key::Enter),
            "> ```\n>   x\n>   |\n> ```"
        );
        assert_eq!(
            after("- item\n\n  ```\n    y|\n  ```", Key::Enter),
            "- item\n\n  ```\n    y\n    |\n  ```"
        );
    }

    #[test]
    fn an_empty_block_gets_its_first_line() {
        assert_eq!(after("```\n|```", Key::Text("x".into())), "```\nx|\n```");
        assert_eq!(
            after("> ```\n|> ```", Key::Text("x".into())),
            "> ```\n> x|\n> ```"
        );
        assert_eq!(after("> ```\n|> ```", Key::Enter), "> ```\n> \n> |\n> ```");
        assert_eq!(after("```|", Key::Text("x".into())), "```\nx|");
        assert_eq!(press("```\na|\n```", Key::Text("x".into())), None);
    }

    #[test]
    fn tab_inserts_the_indent_of_the_block() {
        assert_eq!(after("```\na|\n```", Key::Tab), "```\na    |\n```");
        assert_eq!(
            after("```\nif x:\n  y|\n```", Key::Tab),
            "```\nif x:\n  y  |\n```"
        );
        assert_eq!(after("```\n\tx|\n```", Key::Tab), "```\n\tx\t|\n```");
        assert_eq!(
            after("```\nif x:\n    y|\n```", Key::ShiftTab),
            "```\nif x:\ny|\n```"
        );
        assert_eq!(after("```\n\t\tz|\n```", Key::ShiftTab), "```\n\tz|\n```");
        assert_eq!(after("```\nz|\n```", Key::ShiftTab), "```\nz|\n```");
    }

    #[test]
    fn html_pre_and_other_keys_keep_the_common_rules() {
        assert_eq!(press("<pre><code>a|</code></pre>\n", Key::Enter), None);
        assert_eq!(press("```\na|\n```", Key::Backspace), None);
        assert_eq!(press("text|\n", Key::Tab), None);
    }
}
