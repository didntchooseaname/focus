use std::ops::Range;

use gpui_component::RopeExt as _;

use super::{At, Edit, Key};

pub(crate) fn key(at: &At, key: &Key) -> Option<Edit> {
    if !matches!(
        key,
        Key::Enter | Key::ShiftEnter | Key::Tab | Key::ShiftTab | Key::Backspace
    ) {
        return None;
    }
    let rows = at.item?.lines.clone();
    let rows = rows.start as usize..rows.end as usize;
    let base = at.text.line_start_offset(rows.start);
    let source = at.text.slice_lines(rows).to_string();
    rule(&source, base, at.caret, at.selection.clone(), key)
}

fn rule(
    source: &str,
    base: usize,
    caret: usize,
    selection: Range<usize>,
    key: &Key,
) -> Option<Edit> {
    let lines = read(source, base);
    let row = lines
        .iter()
        .rposition(|l| l.start <= caret && caret <= l.start + l.text.len())?;
    let item = owner(&lines, row)?;
    match key {
        Key::Enter => Some(enter(&lines, item, &selection)),
        Key::ShiftEnter => {
            let line = &lines[item];
            let column = line.marker.as_ref()?.content;
            let text = format!("{}{}{}", line.eol(), line.quote(), " ".repeat(column));
            Some(Edit {
                caret: selection.start + text.len(),
                range: selection,
                text,
            })
        }
        Key::Tab => Some(match sibling_before(&lines, item) {
            Some(above) => {
                let column = lines[above].marker.as_ref()?.content;
                shift(
                    &lines,
                    item,
                    column as isize - lines[item].columns as isize,
                    caret,
                )
            }
            None => stay(caret),
        }),
        Key::ShiftTab => Some(outdent(&lines, item, caret).unwrap_or_else(|| stay(caret))),
        Key::Backspace => {
            let line = &lines[item];
            let start = line.start + line.marker.as_ref()?.text;
            if !selection.is_empty() || caret != start {
                return None;
            }
            outdent(&lines, item, caret).or_else(|| Some(join_above(&lines, item, caret)))
        }
        Key::Delete | Key::Text(_) => None,
    }
}

struct Line<'a> {
    start: usize,
    text: &'a str,
    crlf: bool,
    quote: usize,
    depth: usize,
    indent: usize,
    columns: usize,
    blank: bool,
    marker: Option<Marker>,
}

impl Line<'_> {
    fn quote(&self) -> &str {
        &self.text[..self.quote]
    }

    fn eol(&self) -> &'static str {
        if self.crlf { "\r\n" } else { "\n" }
    }
}

struct Marker {
    sign: usize,
    spaces: usize,
    task: Option<Range<usize>>,
    text: usize,
    number: Option<(u64, usize)>,
    content: usize,
}

fn read(source: &str, base: usize) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for piece in source.split('\n') {
        let crlf = piece.ends_with('\r');
        let text = piece.strip_suffix('\r').unwrap_or(piece);
        let (quote, depth) = quote_markers(text);
        let rest = &text[quote..];
        let indent = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let columns = width(&rest[..indent], 0);
        let blank = rest.trim().is_empty();
        let marker = (!blank)
            .then(|| marker(text, quote + indent, columns))
            .flatten();
        lines.push(Line {
            start: base + start,
            text,
            crlf,
            quote,
            depth,
            indent,
            columns,
            blank,
            marker,
        });
        start += piece.len() + 1;
    }
    lines
}

fn quote_markers(text: &str) -> (usize, usize) {
    let bytes = text.as_bytes();
    let (mut at, mut depth) = (0, 0);
    loop {
        let lead = bytes[at..]
            .iter()
            .take(3)
            .take_while(|&&b| b == b' ')
            .count();
        if bytes.get(at + lead) != Some(&b'>') {
            return (at, depth);
        }
        at += lead + 1;
        if bytes.get(at) == Some(&b' ') {
            at += 1;
        }
        depth += 1;
    }
}

fn width(white: &str, from: usize) -> usize {
    white.bytes().fold(from, |column, b| {
        if b == b'\t' {
            column + 4 - column % 4
        } else {
            column + 1
        }
    }) - from
}

fn marker(text: &str, at: usize, column: usize) -> Option<Marker> {
    let rest = &text[at..];
    let bytes = rest.as_bytes();
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    let (sign, number) = match bytes.first()? {
        b'-' | b'*' | b'+' => (1, None),
        _ if (1..=9).contains(&digits) && matches!(bytes.get(digits), Some(b'.' | b')')) => {
            (digits + 1, Some((rest[..digits].parse().ok()?, digits)))
        }
        _ => return None,
    };
    let after = &rest[sign..];
    if !(after.is_empty() || after.starts_with([' ', '\t'])) {
        return None;
    }
    let first = bytes[0];
    if number.is_none()
        && rest.bytes().filter(|&b| b == first).count() >= 3
        && rest.bytes().all(|b| b == first || b == b' ' || b == b'\t')
    {
        return None;
    }
    let white = after.len() - after.trim_start_matches([' ', '\t']).len();
    let spaced = width(&after[..white], column + sign);
    let content = column
        + sign
        + if after[white..].is_empty() || spaced > 4 {
            1
        } else {
            spaced
        };
    let mut text_at = sign + white;
    let mut task = None;
    if after[white..].starts_with('[')
        && let Some(state) = after[white + 1..].chars().next()
        && after[white + 1 + state.len_utf8()..].starts_with(']')
        && state != ']'
    {
        let end = white + 1 + state.len_utf8() + 1;
        let tail = &after[end..];
        if tail.is_empty() || tail.starts_with([' ', '\t']) {
            task = Some(at + sign + white..at + sign + end);
            text_at = sign + end + (tail.len() - tail.trim_start_matches([' ', '\t']).len());
        }
    }
    Some(Marker {
        sign: at + sign,
        spaces: at + sign + white,
        task,
        text: at + text_at,
        number,
        content,
    })
}

fn owner(lines: &[Line], row: usize) -> Option<usize> {
    let target = &lines[row];
    if target.marker.is_some() {
        return Some(row);
    }
    let mut blank = false;
    for r in (0..row).rev() {
        let line = &lines[r];
        if line.depth < target.depth {
            return None;
        }
        if line.blank {
            blank = true;
            continue;
        }
        if let Some(marker) = &line.marker
            && (target.columns >= marker.content || !blank)
        {
            return Some(r);
        }
    }
    None
}

fn parent(lines: &[Line], item: usize) -> Option<usize> {
    let columns = lines[item].columns;
    (0..item).rev().find(|&r| {
        lines[r]
            .marker
            .as_ref()
            .is_some_and(|m| columns >= m.content && lines[r].depth == lines[item].depth)
    })
}

fn sibling_before(lines: &[Line], item: usize) -> Option<usize> {
    let columns = lines[item].columns;
    for r in (0..item).rev() {
        let Some(marker) = &lines[r].marker else {
            continue;
        };
        if lines[r].depth != lines[item].depth {
            return None;
        }
        if columns >= marker.content {
            return None;
        }
        if lines[r].columns <= columns {
            return Some(r);
        }
    }
    None
}

fn last_row(lines: &[Line], item: usize) -> usize {
    let content = lines[item].marker.as_ref().map_or(0, |m| m.content);
    let mut last = item;
    for r in item + 1..lines.len() {
        let line = &lines[r];
        if line.depth != lines[item].depth {
            break;
        }
        if line.blank {
            continue;
        }
        let lazy = line.marker.is_none() && r == last + 1;
        if line.columns >= content || lazy {
            last = r;
        } else {
            break;
        }
    }
    last
}

fn stay(caret: usize) -> Edit {
    Edit {
        range: caret..caret,
        text: String::new(),
        caret,
    }
}

fn enter(lines: &[Line], item: usize, selection: &Range<usize>) -> Edit {
    let line = &lines[item];
    let Some(marker) = &line.marker else {
        return stay(selection.start);
    };
    let empty = line.text[marker.text..].trim().is_empty() && last_row(lines, item) == item;
    if empty && selection.is_empty() {
        if let Some(edit) = outdent(lines, item, selection.start) {
            return edit;
        }
        let text = format!("{}{}{}", line.quote().trim_end(), line.eol(), line.quote());
        return Edit {
            range: line.start..line.start + line.text.len(),
            caret: line.start + text.len(),
            text,
        };
    }
    let sign = match marker.number {
        Some((number, digits)) => {
            let delimiter = &line.text[marker.sign - 1..marker.sign];
            let padded = if line.text.as_bytes()[marker.sign - 1 - digits] == b'0' {
                digits
            } else {
                0
            };
            format!("{:0padded$}{delimiter}", number + 1)
        }
        None => line.text[marker.sign - 1..marker.sign].to_string(),
    };
    let spaces = match &line.text[marker.sign..marker.spaces] {
        "" => " ",
        white => white,
    };
    let task = match &marker.task {
        Some(task) => match &line.text[task.end..marker.text] {
            "" => "[ ] ".to_string(),
            white => format!("[ ]{white}"),
        },
        None => String::new(),
    };
    let text = format!(
        "{}{}{}{sign}{spaces}{task}",
        line.eol(),
        line.quote(),
        &line.text[line.quote..line.quote + line.indent],
    );
    Edit {
        range: selection.clone(),
        caret: selection.start + text.len(),
        text,
    }
}

fn outdent(lines: &[Line], item: usize, caret: usize) -> Option<Edit> {
    let up = parent(lines, item)?;
    Some(shift(
        lines,
        item,
        lines[up].columns as isize - lines[item].columns as isize,
        caret,
    ))
}

fn shift(lines: &[Line], item: usize, by: isize, caret: usize) -> Edit {
    let last = last_row(lines, item);
    let range = lines[item].start..lines[last].start + lines[last].text.len();
    let mut text = String::new();
    let mut moved = caret;
    for line in &lines[item..=last] {
        if line.blank {
            text.push_str(line.text);
        } else {
            let columns = (line.columns as isize + by).max(0) as usize;
            let indent = " ".repeat(columns);
            let rest = &line.text[line.quote + line.indent..];
            if (line.start + line.quote + line.indent..=line.start + line.text.len())
                .contains(&caret)
            {
                moved = range.start + text.len() + line.quote + indent.len() + caret
                    - (line.start + line.quote + line.indent);
            }
            text.push_str(line.quote());
            text.push_str(&indent);
            text.push_str(rest);
        }
        if line.start != lines[last].start {
            text.push_str(line.eol());
        }
    }
    Edit {
        range,
        text,
        caret: moved,
    }
}

fn join_above(lines: &[Line], item: usize, caret: usize) -> Edit {
    let line = &lines[item];
    let first = !(0..item).any(|r| lines[r].marker.is_some() && lines[r].depth == line.depth);
    let above = (0..item).rev().find(|&r| !lines[r].blank);
    let start = match above {
        Some(r) if !first => lines[r].start + lines[r].text.len(),
        _ => line.start + line.quote,
    };
    Edit {
        range: start..caret,
        text: String::new(),
        caret: start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(note: &str, key: Key) -> Option<String> {
        let caret = note.find('|').expect("a caret");
        let source = note.replacen('|', "", 1);
        let edit = rule(&source, 0, caret, caret..caret, &key)?;
        let mut out = source.clone();
        out.replace_range(edit.range.clone(), &edit.text);
        out.insert(edit.caret, '|');
        Some(out)
    }

    fn after(note: &str, key: Key) -> String {
        press(note, key).expect("a list rule")
    }

    fn markers(note: &str) -> Vec<String> {
        fn walk(blocks: &[focus_md::Block], source: &str, out: &mut Vec<String>) {
            for block in blocks {
                match &block.kind {
                    focus_md::BlockKind::List { items, .. } => {
                        for item in items {
                            out.push(source[item.marker.clone()].to_string());
                            walk(&item.blocks, source, out);
                        }
                    }
                    focus_md::BlockKind::Quote(inner)
                    | focus_md::BlockKind::Callout { blocks: inner, .. } => {
                        walk(inner, source, out)
                    }
                    _ => {}
                }
            }
        }
        let source = note.replace('|', "");
        let document = focus_md::parse(&source, &focus_md::ParseOptions::default());
        let mut out = Vec::new();
        walk(&document.blocks, &source, &mut out);
        out
    }

    #[test]
    fn enter_starts_an_item_of_the_same_kind() {
        assert_eq!(after("- one|\n- two\n", Key::Enter), "- one\n- |\n- two\n");
        assert_eq!(after("* a|\n", Key::Enter), "* a\n* |\n");
        assert_eq!(after("+   wide|", Key::Enter), "+   wide\n+   |");
        assert_eq!(after("- sp|lit", Key::Enter), "- sp\n- |lit");
        assert_eq!(after("- [x] done|\n", Key::Enter), "- [x] done\n- [ ] |\n");
        assert_eq!(after("- [/] half|\n", Key::Enter), "- [/] half\n- [ ] |\n");
        assert_eq!(after("  - nested|\n", Key::Enter), "  - nested\n  - |\n");
        assert_eq!(
            after("- a\r\n- b|\r\n", Key::Enter),
            "- a\r\n- b\r\n- |\r\n"
        );
    }

    #[test]
    fn numbers_follow_and_written_numbers_stay() {
        assert_eq!(
            after("1. one|\n2. two\n", Key::Enter),
            "1. one\n2. |\n2. two\n"
        );
        assert_eq!(after("9) nine|", Key::Enter), "9) nine\n10) |");
        assert_eq!(after("07. seven|", Key::Enter), "07. seven\n08. |");
        assert_eq!(after("3. [ ] task|", Key::Enter), "3. [ ] task\n4. [ ] |");
    }

    #[test]
    fn enter_on_an_empty_item_ends_the_list() {
        assert_eq!(after("- a\n- |", Key::Enter), "- a\n\n|");
        assert_eq!(
            after("- a\n- [ ] |\n\nafter\n", Key::Enter),
            "- a\n\n|\n\nafter\n"
        );
        assert_eq!(after("1. a\n2. |", Key::Enter), "1. a\n\n|");
        assert_eq!(markers("- a\n\nafter\n"), ["- "]);
        assert_eq!(after("- a\n  - |", Key::Enter), "- a\n- |");
    }

    #[test]
    fn tab_and_shift_tab_change_the_level() {
        assert_eq!(after("- a\n- b|\n", Key::Tab), "- a\n  - b|\n");
        assert_eq!(after("1. a\n2. b|\n", Key::Tab), "1. a\n   2. b|\n");
        assert_eq!(
            after("- [ ] a\n- [ ] b|\n", Key::Tab),
            "- [ ] a\n  - [ ] b|\n"
        );
        assert_eq!(
            after("- a\n- b|\n  - c\n  more\n- d\n", Key::Tab),
            "- a\n  - b|\n    - c\n    more\n- d\n"
        );
        assert_eq!(after("- a\n  - b|\n", Key::ShiftTab), "- a\n- b|\n");
        assert_eq!(after("- a\n\t- b|\n", Key::ShiftTab), "- a\n- b|\n");
        assert_eq!(after("- a|\n- b\n", Key::Tab), "- a|\n- b\n");
        assert_eq!(after("- a|\n", Key::ShiftTab), "- a|\n");
        assert_eq!(after("- a\n- b|c\n", Key::Tab), "- a\n  - b|c\n");
        assert_eq!(markers("- a\n  - b\n    - c\n"), ["- ", "- ", "- "]);
    }

    #[test]
    fn backspace_at_the_start_lowers_then_joins() {
        assert_eq!(after("- a\n  - |b\n", Key::Backspace), "- a\n- |b\n");
        assert_eq!(after("- a\n- |b\n", Key::Backspace), "- a|b\n");
        assert_eq!(after("- [ ] a\n- [ ] |b\n", Key::Backspace), "- [ ] a|b\n");
        assert_eq!(
            after("- a\n  - c\n- |b\n", Key::Backspace),
            "- a\n  - c|b\n"
        );
        assert_eq!(after("- a\n\n- |b\n", Key::Backspace), "- a|b\n");
        assert_eq!(after("- |a\n- b\n", Key::Backspace), "|a\n- b\n");
        assert_eq!(press("- a|b\n", Key::Backspace), None);
    }

    #[test]
    fn lists_in_callouts_and_quotes_keep_their_markers() {
        let note = "> [!note] Title\n> - a|\n> - b\n";
        assert_eq!(
            after(note, Key::Enter),
            "> [!note] Title\n> - a\n> - |\n> - b\n"
        );
        assert_eq!(after("> - a\n> - b|\n", Key::Tab), "> - a\n>   - b|\n");
        assert_eq!(after("> > 1. a|", Key::Enter), "> > 1. a\n> > 2. |");
        assert_eq!(after("> - a\n> - |", Key::Enter), "> - a\n>\n> |");
        assert_eq!(after("> - a\n> - |b", Key::Backspace), "> - a|b");
        assert_eq!(markers("> - a\n>   - b\n"), ["- ", "- "]);
    }

    #[test]
    fn shift_enter_continues_the_item() {
        assert_eq!(after("- a|b\n", Key::ShiftEnter), "- a\n  |b\n");
        assert_eq!(after("> 1. a|", Key::ShiftEnter), "> 1. a\n>    |");
    }

    #[test]
    fn continuation_lines_belong_to_their_item() {
        assert_eq!(
            after("- a\n  - b\n  c|\n", Key::Enter),
            "- a\n  - b\n  c\n  - |\n"
        );
        assert_eq!(
            after("- a\n  - b\n\n  c|\n", Key::Enter),
            "- a\n  - b\n\n  c\n- |\n"
        );
        assert_eq!(press("- - -\n|", Key::Enter), None);
        assert_eq!(press("plain|\n", Key::Enter), None);
        assert_eq!(press("- a|\n", Key::Text("x".into())), None);
    }

    #[test]
    fn right_to_left_text_is_bytes_like_any() {
        assert_eq!(after("- عربي|\n", Key::Enter), "- عربي\n- |\n");
        assert_eq!(after("- أ\n- ب|\n", Key::Tab), "- أ\n  - ب|\n");
    }

    #[test]
    fn a_long_list_takes_under_a_millisecond_per_key() {
        let mut note: String = (0..300)
            .map(|i| format!("- [ ] item {i} with words\n"))
            .collect();
        note.insert(note.find("item 150").expect("item 150") + 8, '|');
        let source = note.replacen('|', "", 1);
        let caret = note.find('|').expect("caret");
        let mut times = Vec::new();
        for key in [Key::Enter, Key::Tab, Key::ShiftTab, Key::Backspace] {
            for _ in 0..20 {
                let start = std::time::Instant::now();
                std::hint::black_box(rule(&source, 0, caret, caret..caret, &key));
                times.push(start.elapsed());
            }
        }
        times.sort();
        let median = times[times.len() / 2];
        assert!(median < std::time::Duration::from_millis(1), "{median:?}");
    }
}
