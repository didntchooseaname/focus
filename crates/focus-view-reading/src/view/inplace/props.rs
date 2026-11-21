use std::ops::Range;

use focus_kit::{Flow as _, Icon, Tokens, Type as _, icon, size};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::{Rope, RopeExt as _};

use super::super::ReadingView;
use super::super::live::{LiveKind, LiveOpen};
use super::super::properties::FieldError;
use super::{At, Edit, Key};

pub(crate) fn key(at: &At, key: &Key) -> Option<Edit> {
    let key = match key {
        Key::ShiftEnter => &Key::Enter,
        key => key,
    };
    if !matches!(key, Key::Enter | Key::Backspace | Key::Text(_)) {
        return None;
    }
    let source = frontmatter(at.text)?;
    if at.caret.max(at.selection.end) > source.len() {
        return None;
    }
    if let (Key::Text(typed), Some((_, kind))) = (key, at.prop) {
        if let Some(edit) = first_key(&source, at.caret, &at.selection, typed, kind.kind) {
            return Some(edit);
        }
        if super::super::properties::has_form(kind.kind) {
            return None;
        }
    }
    rule(&source, at.caret, at.selection.clone(), key)
}

pub(super) fn typing_kind(
    model: focus_core::PropertyKind,
    key: &str,
    before: Option<&(String, focus_core::PropertyKind)>,
) -> focus_core::PropertyKind {
    use focus_core::TypeOrigin::Value;
    match before {
        Some((name, kind)) if name == key && model.origin == Value && kind.origin == Value => *kind,
        _ => model,
    }
}

pub(super) fn key_at(text: &gpui_component::Rope, caret: usize) -> Option<String> {
    let source = frontmatter(text)?;
    if caret > source.len() {
        return None;
    }
    let at = value_at(&source, caret)?;
    let mut line = at.line;
    loop {
        let row = &source[line.clone()];
        if !row.starts_with([' ', '\t', '-', '#']) {
            return row.split_once(':').map(|(key, _)| key.trim().to_string());
        }
        if !at.element || line.start == 0 {
            return None;
        }
        let end = line.start - 1;
        line = source[..end].rfind('\n').map_or(0, |i| i + 1)..end;
    }
}

fn first_key(
    source: &str,
    caret: usize,
    selection: &Range<usize>,
    typed: &str,
    kind: focus_core::PropertyType,
) -> Option<Edit> {
    let at = value_at(source, caret)?;
    if at.element || !at.value.is_empty() || !selection.is_empty() {
        return None;
    }
    let space = if source[..at.value.start].ends_with(':') {
        " "
    } else {
        ""
    };
    let quoted = |text: &str| {
        if needs_quotes(text) {
            let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
            (format!("\"{escaped}"), "\"")
        } else {
            (text.to_string(), "")
        }
    };
    let (head, tail) = if kind.is_list() {
        let eol = if source[at.line.end..].starts_with('\r') {
            "\r\n"
        } else {
            "\n"
        };
        let (item, close) = quoted(typed);
        (format!("{eol}  - {item}"), close)
    } else if super::super::properties::has_form(kind) {
        if space.is_empty() {
            return None;
        }
        (format!("{space}{typed}"), "")
    } else {
        let (value, close) = quoted(typed);
        if space.is_empty() && close.is_empty() {
            return None;
        }
        (format!("{space}{value}"), close)
    };
    Some(Edit {
        range: caret..caret,
        caret: caret + head.len(),
        text: format!("{head}{tail}"),
    })
}

fn tag_refuses(c: char) -> Option<FieldError> {
    if c.is_whitespace() {
        Some(FieldError::TagSpace)
    } else if c == '#' {
        Some(FieldError::TagHash)
    } else if c.is_control() || (c.is_ascii_punctuation() && !matches!(c, '_' | '-' | '/')) {
        Some(FieldError::TagChar(c))
    } else {
        None
    }
}

pub(crate) fn refused(at: &At, key: &Key) -> Option<FieldError> {
    let Key::Text(typed) = key else {
        return None;
    };
    let (_, kind) = at.prop?;
    let kind = kind.kind;
    if kind == focus_core::PropertyType::Tags {
        return typed.chars().find_map(tag_refuses);
    }
    if !super::super::properties::has_form(kind) {
        return None;
    }
    let source = frontmatter(at.text)?;
    let value = value_at(&source, at.caret)?;
    let written = &source[value.value.clone()];
    let inner = match written.chars().next() {
        Some(q @ ('"' | '\'')) if written.len() >= 2 && written.ends_with(q) => {
            value.value.start + 1..value.value.end - 1
        }
        _ => value.value.clone(),
    };
    let selection = at.selection.clone();
    if selection.start < inner.start || selection.end > inner.end {
        return None;
    }
    let after = format!(
        "{}{typed}{}",
        &source[inner.start..selection.start],
        &source[selection.end..inner.end]
    );
    if kind.accepts_typing(&after) {
        return None;
    }
    Some(FieldError::Value(match kind {
        focus_core::PropertyType::Date => focus_core::ValueError::DateForm,
        focus_core::PropertyType::Datetime => focus_core::ValueError::DatetimeForm,
        _ => focus_core::ValueError::NotNumber,
    }))
}

pub(crate) fn tag_error(text: &Rope, caret: usize) -> Option<FieldError> {
    let source = frontmatter(text)?;
    let value = value_at(&source, caret)?;
    let written = unquoted(&source, value.value);
    let tag = &source[written];
    (!tag.is_empty() && tag.chars().all(|c| c.is_ascii_digit())).then_some(FieldError::TagNumber)
}

fn unquoted(source: &str, range: Range<usize>) -> Range<usize> {
    let value = &source[range.clone()];
    match value.chars().next() {
        Some(q @ ('"' | '\'')) if value.len() >= 2 && value.ends_with(q) => {
            range.start + 1..range.end - 1
        }
        _ => range,
    }
}

fn key_line(source: &str, key: &str) -> Option<(usize, usize)> {
    let mut at = 0;
    for raw in source.split_inclusive('\n') {
        let start = at;
        at += raw.len();
        if start == 0 || raw.starts_with([' ', '\t', '-', '#']) {
            continue;
        }
        let line = raw.trim_end_matches(['\n', '\r']);
        if let Some((name, after)) = line.split_once(':')
            && name.trim() == key
            && (after.is_empty() || after.starts_with([' ', '\t']))
        {
            return Some((start, start + name.len() + 1));
        }
    }
    None
}

fn lines_under(source: &str, key: &str) -> Vec<Range<usize>> {
    let Some((line, _)) = key_line(source, key) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    let mut at = line
        + source[line..]
            .find('\n')
            .map_or(source.len() - line, |i| i + 1);
    for raw in source[at..].split_inclusive('\n') {
        let start = at;
        at += raw.len();
        let row = raw.trim_end_matches(['\n', '\r']);
        if !row.starts_with([' ', '\t', '-']) || matches!(row.trim_end(), "---" | "...") {
            break;
        }
        lines.push(start..start + row.len());
    }
    lines
}

pub(crate) fn value_span(text: &Rope, key: &str) -> Option<Range<usize>> {
    let source = frontmatter(text)?;
    let (line, _) = key_line(&source, key)?;
    let own = line_value(&source, line)?;
    if !own.value.is_empty() {
        return Some(unquoted(&source, own.value));
    }
    let elements: Vec<Range<usize>> = lines_under(&source, key)
        .into_iter()
        .filter_map(|l| line_value(&source, l.start).filter(|v| v.element))
        .map(|v| unquoted(&source, v.value))
        .collect();
    match (elements.first(), elements.last()) {
        (Some(first), Some(last)) => Some(first.start..last.end),
        _ => Some(own.value),
    }
}

fn property_block(text: &Rope, key: &str) -> Option<Range<usize>> {
    let source = frontmatter(text)?;
    let (line, colon) = key_line(&source, key)?;
    let own = line_value(&source, line)?.line.end;
    let end = lines_under(&source, key).last().map_or(own, |l| l.end);
    Some(colon..end)
}

pub(crate) fn stays_in(text: &Rope, key: &str, edit: &Edit) -> bool {
    let Some(block) = property_block(text, key) else {
        return false;
    };
    let end = (block.end + edit.text.len()).saturating_sub(edit.range.len());
    block.start <= edit.range.start
        && edit.range.end <= block.end
        && block.start <= edit.caret
        && edit.caret <= end
}

pub(crate) fn blanks_of_empty(text: &Rope, caret: usize) -> Option<(String, usize)> {
    let source = frontmatter(text)?;
    let value = value_at(&source, caret)?;
    if value.element || !value.value.is_empty() {
        return None;
    }
    let line = &source[value.line.clone()];
    let colon = line.find(':')?;
    Some((
        line[..colon].trim().to_string(),
        value.value.start - (value.line.start + colon + 1),
    ))
}

pub(crate) fn emptied(text: &Rope, edit: Edit, blank: Option<(&str, usize)>) -> Edit {
    if !edit.text.is_empty() || edit.range.is_empty() {
        return edit;
    }
    let Some(source) = frontmatter(text) else {
        return edit;
    };
    if edit.range.end > source.len() {
        return edit;
    }
    let Some(at) = value_at(&source, edit.range.start) else {
        return edit;
    };
    if at.value.is_empty() || edit.range.end > at.value.end {
        return edit;
    }
    let left = format!(
        "{}{}",
        &source[at.value.start..edit.range.start],
        &source[edit.range.end..at.value.end]
    );
    if !matches!(left.as_str(), "" | "\"\"" | "''") {
        return edit;
    }
    if at.element {
        let Some(key) = key_at(text, edit.range.start) else {
            return edit;
        };
        let elements = lines_under(&source, &key)
            .into_iter()
            .filter(|l| source[l.clone()].trim_start().starts_with('-'))
            .count();
        let eol = if source[..at.line.start].ends_with("\r\n") {
            2
        } else {
            1
        };
        if elements != 1 || at.line.start < eol {
            return edit;
        }
        let start = at.line.start - eol;
        return Edit {
            range: start..at.line.end,
            text: String::new(),
            caret: start,
        };
    }
    let line = &source[at.line.clone()];
    let Some(colon) = line.find(':') else {
        return edit;
    };
    let after = at.line.start + colon + 1;
    let keep = blank
        .filter(|(key, _)| *key == line[..colon].trim())
        .map_or(0, |(_, n)| n)
        .min(at.value.start - after);
    Edit {
        range: after + keep..at.value.end,
        text: String::new(),
        caret: after + keep,
    }
}

pub(crate) fn read_enter(text: &Rope, caret: usize) -> (Option<Edit>, bool) {
    let Some(source) = frontmatter(text) else {
        return (None, true);
    };
    match value_at(&source, caret) {
        Some(at) if at.element && !at.value.is_empty() => {
            (rule(&source, caret, caret..caret, &Key::Enter), false)
        }
        Some(at) if at.element => (rule(&source, caret, caret..caret, &Key::Backspace), true),
        _ => (None, true),
    }
}

fn frontmatter(text: &gpui_component::Rope) -> Option<String> {
    let first = text.slice_line(0).to_string();
    if first.trim_start_matches('\u{feff}').trim_end() != "---" {
        return None;
    }
    let rows = text.lines_len();
    let close =
        (1..rows).find(|&r| matches!(text.slice_line(r).to_string().trim_end(), "---" | "..."))?;
    Some(text.slice_lines(0..close + 1).to_string())
}

struct Value {
    line: Range<usize>,
    value: Range<usize>,
    element: bool,
}

fn value_at(source: &str, caret: usize) -> Option<Value> {
    let start = source.get(..caret)?.rfind('\n').map_or(0, |i| i + 1);
    line_value(source, start).filter(|v| v.value.start <= caret && caret <= v.value.end)
}

fn line_value(source: &str, start: usize) -> Option<Value> {
    let end = source[start..]
        .find('\n')
        .map_or(source.len(), |i| start + i);
    let text = source[start..end].trim_end_matches('\r');
    let trimmed = text.trim_start();
    let indent = text.len() - trimmed.len();
    let (from, element) = if let Some(rest) = trimmed.strip_prefix('-')
        && (rest.is_empty() || rest.starts_with(' '))
    {
        (indent + 1 + usize::from(!rest.is_empty()), true)
    } else if indent == 0 && !trimmed.starts_with('#') {
        let colon = text.find(':')?;
        let after = &text[colon + 1..];
        if !(after.is_empty() || after.starts_with(' ')) {
            return None;
        }
        (colon + 1 + (after.len() - after.trim_start().len()), false)
    } else {
        return None;
    };
    Some(Value {
        line: start..start + text.len(),
        value: start + from..start + text.trim_end().len().max(from),
        element,
    })
}

fn needs_quotes(value: &str) -> bool {
    let Some(first) = value.chars().next() else {
        return false;
    };
    value.trim() != value
        || value.contains(": ")
        || value.ends_with(':')
        || value.contains(" #")
        || matches!(
            first,
            '[' | ']'
                | '{'
                | '}'
                | ','
                | '#'
                | '&'
                | '*'
                | '!'
                | '|'
                | '>'
                | '\''
                | '"'
                | '%'
                | '@'
                | '`'
        )
        || ((first == '-' || first == '?' || first == ':')
            && value[1..].chars().next().is_none_or(char::is_whitespace))
}

fn rule(source: &str, caret: usize, selection: Range<usize>, key: &Key) -> Option<Edit> {
    let at = value_at(source, caret)?;
    let line = &source[at.line.clone()];
    let crlf = source[at.line.end..].starts_with('\r');
    let eol = if crlf { "\r\n" } else { "\n" };
    match key {
        Key::Enter if at.element => {
            let marker = &line[..at.value.start - at.line.start];
            let text = format!("{eol}{marker}");
            Some(Edit {
                caret: at.value.end + text.len(),
                range: at.value.end..at.value.end,
                text,
            })
        }
        Key::Enter => Some(Edit {
            range: caret..caret,
            text: String::new(),
            caret,
        }),
        Key::Backspace if at.element && at.value.is_empty() && selection.is_empty() => {
            let start = at.line.start.checked_sub(eol.len())?;
            Some(Edit {
                range: start..at.line.end,
                text: String::new(),
                caret: start,
            })
        }
        Key::Text(typed) => {
            if selection.start < at.value.start || selection.end > at.value.end {
                return None;
            }
            let value = &source[at.value.clone()];
            let quote = value.chars().next().filter(|c| matches!(c, '"' | '\''));
            let closed = quote.is_some_and(|q| value.len() >= 2 && value.ends_with(q));
            match quote {
                Some('"') if closed => {
                    let escaped = typed.replace('\\', "\\\\").replace('"', "\\\"");
                    let inner = at.value.start + 1..at.value.end - 1;
                    let written = &source[inner.clone()];
                    let after = format!(
                        "{}{escaped}{}",
                        &source[inner.start..selection.start.max(inner.start)],
                        &source[selection.end.min(inner.end)..inner.end]
                    );
                    let blank_only = written.ends_with([' ', '\t'])
                        && !needs_quotes(written.trim_end())
                        && !written.contains('\\');
                    if blank_only
                        && inner.start <= selection.start
                        && selection.end <= inner.end
                        && !after.contains('\\')
                        && !needs_quotes(&after)
                    {
                        return Some(Edit {
                            caret: selection.start - 1 + escaped.len(),
                            range: at.value.clone(),
                            text: after,
                        });
                    }
                    Some(Edit {
                        caret: selection.start + escaped.len(),
                        range: selection,
                        text: escaped,
                    })
                }
                Some('\'') if closed => {
                    let escaped = typed.replace('\'', "''");
                    Some(Edit {
                        caret: selection.start + escaped.len(),
                        range: selection,
                        text: escaped,
                    })
                }
                _ => {
                    let before = &source[at.value.start..selection.start];
                    let after = &source[selection.end..at.value.end];
                    let new = format!("{before}{typed}{after}");
                    if !needs_quotes(&new) {
                        return None;
                    }
                    let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
                    let head = format!("\"{}{}", escape(before), escape(typed));
                    Some(Edit {
                        range: at.value.clone(),
                        caret: at.value.start + head.len(),
                        text: format!("{head}{}\"", escape(after)),
                    })
                }
            }
        }
        _ => None,
    }
}

pub(in crate::view) fn add_chip(
    view: &ReadingView,
    t: &Tokens,
    cx: &mut Context<ReadingView>,
) -> Option<AnyElement> {
    if !view.live || view.in_place.is_none() {
        return None;
    }
    let (hover, pressed) = (t.card_2, t.card_3);
    let ink = t.ink;
    Some(
        div()
            .id("reading-props-add")
            .flow_row()
            .flex_none()
            .items_center()
            .gap(size::S1)
            .h(px(22.))
            .px(size::S2)
            .rounded(size::RADIUS)
            .ui_sm()
            .text_color(t.ink_3)
            .cursor_pointer()
            .hover(move |s| s.bg(hover).text_color(ink))
            .active(move |s| s.bg(pressed))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| {
                let Some(field) = this.in_place.clone() else {
                    return;
                };
                let text = field.read(cx).text().clone();
                let (edit, open) = new_property(&text);
                this.apply(edit, window, cx);
                cx.emit(open);
            }))
            .child(icon(Icon::PLUS, size::ICON_DENSE))
            .child(focus_i18n::t("reading-props-add"))
            .into_any_element(),
    )
}

fn new_property(text: &gpui_component::Rope) -> (Edit, LiveOpen) {
    let bom = if text.slice_line(0).to_string().starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    match frontmatter(text) {
        Some(source) => {
            let close_row = source.trim_end_matches(['\r', '\n']).matches('\n').count();
            let at = text.line_start_offset(close_row);
            let eol = if source.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            let edit = Edit {
                range: at..at,
                text: eol.to_string(),
                caret: at,
            };
            let open = LiveOpen {
                kind: LiveKind::Properties,
                lines: 0..close_row as u32 + 2,
                line: close_row as u32,
                column: 0,
            };
            (edit, open)
        }
        None => {
            let edit = Edit {
                range: bom..bom,
                text: "---\n\n---\n".into(),
                caret: bom + 4,
            };
            let open = LiveOpen {
                kind: LiveKind::Properties,
                lines: 0..3,
                line: 1,
                column: 0,
            };
            (edit, open)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::properties::property_checkbox_edit;

    fn press(note: &str, key: Key) -> Option<String> {
        let caret = note.find('|').expect("a caret");
        let source = note.replacen('|', "", 1);
        let edit = rule(&source, caret, caret..caret, &key)?;
        let mut out = source;
        out.replace_range(edit.range.clone(), &edit.text);
        out.insert(edit.caret, '|');
        Some(out)
    }

    fn typed(note: &str, text: &str) -> Option<String> {
        press(note, Key::Text(text.into()))
    }

    #[test]
    fn typing_quotes_a_value_that_needs_it() {
        assert_eq!(typed("---\ntitle: a|\n---\n", "b"), None);
        assert_eq!(
            typed("---\nrelated: |\n---\n", "[").as_deref(),
            Some("---\nrelated: \"[|\"\n---\n")
        );
        assert_eq!(
            typed("---\ntitle: a|\n---\n", ":").as_deref(),
            Some("---\ntitle: \"a:|\"\n---\n")
        );
        assert_eq!(
            typed("---\ntitle: a| b\n---\n", " #").as_deref(),
            Some("---\ntitle: \"a #| b\"\n---\n")
        );
        assert_eq!(
            typed("---\ntitle: \"a|\"\n---\n", "\"").as_deref(),
            Some("---\ntitle: \"a\\\"|\"\n---\n")
        );
        assert_eq!(
            typed("---\ntitle: 'it|'\n---\n", "'").as_deref(),
            Some("---\ntitle: 'it''|'\n---\n")
        );
        assert_eq!(
            typed("---\ntitle: \"A |\"\n---\n", "N").as_deref(),
            Some("---\ntitle: A N|\n---\n")
        );
        assert_eq!(
            typed("---\ntitle: \"Hel|\"\n---\n", "l").as_deref(),
            Some("---\ntitle: \"Hell|\"\n---\n")
        );
        assert_eq!(
            typed("---\ntags:\n  - |\n---\n", "#").as_deref(),
            Some("---\ntags:\n  - \"#|\"\n---\n")
        );
    }

    #[test]
    fn enter_adds_a_list_element_and_backspace_removes_an_empty_one() {
        assert_eq!(
            press("---\ntags:\n  - one|\n---\n", Key::Enter).as_deref(),
            Some("---\ntags:\n  - one\n  - |\n---\n")
        );
        assert_eq!(
            press("---\ntags:\n  - o|ne\n---\n", Key::Enter).as_deref(),
            Some("---\ntags:\n  - one\n  - |\n---\n")
        );
        assert_eq!(
            press("---\ntitle: a|b\n---\n", Key::Enter).as_deref(),
            Some("---\ntitle: a|b\n---\n")
        );
        assert_eq!(
            press("---\ntags:\n  - one\n  - |\n---\n", Key::Backspace).as_deref(),
            Some("---\ntags:\n  - one|\n---\n")
        );
        assert_eq!(press("---\ntags:\n  - |one\n---\n", Key::Backspace), None);
        assert_eq!(
            press("---\r\ntags:\r\n  - a|\r\n---\r\n", Key::Enter).as_deref(),
            Some("---\r\ntags:\r\n  - a\r\n  - |\r\n---\r\n")
        );
    }

    #[test]
    fn only_values_have_rules() {
        assert_eq!(press("---\nti|tle: a\n---\n", Key::Text("x".into())), None);
        assert_eq!(press("---\n# com|ment\n---\n", Key::Enter), None);
        assert!(!needs_quotes("plain words"));
        assert!(!needs_quotes("-1"));
        assert!(!needs_quotes("a:b"));
        assert!(needs_quotes("- a"));
        assert!(needs_quotes("[[Note]]"));
        assert!(needs_quotes(" lead"));
    }

    fn typed_prop(kind: focus_core::PropertyType) -> crate::model::Prop {
        crate::model::Prop {
            key: "key".into(),
            value: crate::model::Rich::default(),
            kind: focus_core::PropertyKind {
                kind,
                origin: focus_core::TypeOrigin::Focus,
                obsidian: None,
            },
            fits: true,
            checked: None,
        }
    }

    fn type_in(
        note: &str,
        typed: &str,
        kind: focus_core::PropertyType,
    ) -> (String, Vec<FieldError>) {
        type_in_kind(note, typed, typed_prop(kind).kind)
    }

    fn type_in_kind(
        note: &str,
        typed: &str,
        kind: focus_core::PropertyKind,
    ) -> (String, Vec<FieldError>) {
        let mut caret = note.find('|').expect("a caret");
        let mut text = note.replacen('|', "", 1);
        let mut refusals = Vec::new();
        for c in typed.chars() {
            let rope = gpui_component::Rope::from(text.as_str());
            let at = At {
                text: &rope,
                item: None,
                path: Vec::new(),
                rich: None,
                prop: Some(("key", kind)),
                map: None,
                start: 0,
                caret,
                selection: caret..caret,
            };
            let typed = Key::Text(c.to_string());
            if let Some(why) = refused(&at, &typed) {
                refusals.push(why);
                continue;
            }
            match key(&at, &typed) {
                Some(edit) => {
                    text.replace_range(edit.range.clone(), &edit.text);
                    caret = edit.caret;
                }
                None => {
                    text.insert(caret, c);
                    caret += c.len_utf8();
                }
            }
        }
        (text, refusals)
    }

    fn read_back(note: &str) -> focus_core::PropValue {
        focus_core::parse_meta(note)
            .props
            .into_iter()
            .find(|(k, _)| k == "key")
            .map(|(_, v)| v)
            .expect("the property")
    }

    #[test]
    fn typed_values_are_written_in_their_documented_form() {
        use focus_core::{PropValue, PropertyType, ValueError};
        let (note, refused) = type_in("---\nkey: |\n---\n", "2020-08-21", PropertyType::Date);
        assert_eq!(note, "---\nkey: 2020-08-21\n---\n");
        assert!(refused.is_empty());
        assert!(PropertyType::Date.fits(&read_back(&note)));

        let (note, _) = type_in(
            "---\nkey: |\n---\n",
            "2020-08-21T10:30:00",
            PropertyType::Datetime,
        );
        assert_eq!(note, "---\nkey: 2020-08-21T10:30:00\n---\n");
        assert!(PropertyType::Datetime.fits(&read_back(&note)));

        let (note, refused) = type_in("---\nkey: |\n---\n", "-1x.5", PropertyType::Number);
        assert_eq!(note, "---\nkey: -1.5\n---\n");
        assert_eq!(refused, [FieldError::Value(ValueError::NotNumber)]);
        assert_eq!(read_back(&note), PropValue::Number(-1.5));

        let (note, refused) = type_in("---\nkey: 2020-|\n---\n", "a/08", PropertyType::Date);
        assert_eq!(note, "---\nkey: 2020-08\n---\n");
        assert_eq!(refused, [FieldError::Value(ValueError::DateForm); 2]);

        let (note, _) = type_in("---\nkey: |\n---\n", "a: b", PropertyType::Text);
        assert_eq!(note, "---\nkey: \"a: b\"\n---\n");
    }

    #[test]
    fn an_empty_value_is_filled_in_its_documented_form() {
        use focus_core::{PropValue, PropertyType};
        let note = |value: &str| format!("---\nfirst: x\nkey:{value}\nlast: y\n---\nBody.\n");
        let filled = |typed: &str, kind: PropertyType| {
            let at = note("|");
            let (out, refused) = type_in(&at, typed, kind);
            assert!(refused.is_empty(), "{typed}: {refused:?}");
            assert!(kind.fits(&read_back(&out)), "{out}");
            out
        };
        assert_eq!(
            filled("2020-08-21", PropertyType::Date),
            note(" 2020-08-21")
        );
        assert_eq!(
            filled("2020-08-21T10:30", PropertyType::Datetime),
            note(" 2020-08-21T10:30")
        );
        assert_eq!(filled("-1977", PropertyType::Number), note(" -1977"));
        assert_eq!(read_back(&note(" -1977")), PropValue::Number(-1977.0));
        assert_eq!(filled("plain", PropertyType::Text), note(" plain"));
        assert_eq!(filled("a: b", PropertyType::Text), note(" \"a: b\""));
        assert_eq!(filled("Leia", PropertyType::Multitext), note("\n  - Leia"));
        assert_eq!(
            filled("[[Star Wars]]", PropertyType::Aliases),
            note("\n  - \"[[Star Wars]]\"")
        );
        assert_eq!(
            read_back(&note("\n  - Leia")),
            PropValue::List(vec![PropValue::Text("Leia".into())])
        );
        let (out, _) = type_in("---\nkey: |\n---\n", "7", PropertyType::Number);
        assert_eq!(out, "---\nkey: 7\n---\n");
        let (out, _) = type_in("---\r\nkey:|\r\n---\r\n", "a", PropertyType::Multitext);
        assert_eq!(out, "---\r\nkey:\r\n  - a\r\n---\r\n");
    }

    #[test]
    fn a_type_read_from_the_value_stays_while_it_is_typed() {
        use focus_core::{PropertyType, TypeOrigin};
        let read = |kind| focus_core::PropertyKind {
            kind,
            origin: TypeOrigin::Value,
            obsidian: None,
        };
        let text = read(PropertyType::Text);
        let before = ("key".to_string(), text);
        assert_eq!(
            typing_kind(read(PropertyType::Number), "key", Some(&before)),
            text
        );
        let (out, refused) = type_in_kind("---\nkey:|\n---\n", "2020-08-21", text);
        assert_eq!(
            (out.as_str(), refused.len()),
            ("---\nkey: 2020-08-21\n---\n", 0)
        );
        let number = read(PropertyType::Number);
        assert_eq!(typing_kind(number, "other", Some(&before)), number);
        let chosen = typed_prop(PropertyType::Number).kind;
        assert_eq!(typing_kind(chosen, "key", Some(&before)), chosen);
        assert_eq!(typing_kind(number, "key", None), number);
    }

    #[test]
    fn the_property_at_the_caret_is_the_key_of_its_line() {
        let text = gpui_component::Rope::from(
            "---\nwhen: 2021-0\ncount:\ncast:\n  - a\n- b\nlast: x\n---\n# Title\n",
        );
        let at = |needle: &str, shift: usize| {
            key_at(&text, text.to_string().find(needle).unwrap() + shift)
        };
        assert_eq!(at("2021-0", 6).as_deref(), Some("when"));
        assert_eq!(at("count:", 6).as_deref(), Some("count"));
        assert_eq!(at("  - a", 4).as_deref(), Some("cast"));
        assert_eq!(at("- b", 3).as_deref(), Some("cast"));
        assert_eq!(at("last", 1), None);
        assert_eq!(at("# Title", 3), None);
    }

    #[test]
    fn an_empty_value_refuses_what_its_form_never_holds() {
        use focus_core::{PropertyType, ValueError};
        let note = "---\nkey:|\n---\n";
        let unchanged = note.replace('|', "");
        for (kind, typed, why) in [
            (PropertyType::Date, "x", ValueError::DateForm),
            (PropertyType::Date, "/", ValueError::DateForm),
            (PropertyType::Datetime, "T", ValueError::DatetimeForm),
            (PropertyType::Number, "a", ValueError::NotNumber),
            (PropertyType::Number, ",", ValueError::NotNumber),
        ] {
            let (out, refused) = type_in(note, typed, kind);
            assert_eq!(out, unchanged, "{typed}");
            assert_eq!(refused, [FieldError::Value(why)], "{typed}");
        }
    }

    #[test]
    fn an_empty_value_left_empty_changes_no_byte() {
        use focus_core::PropertyType;
        for kind in [
            PropertyType::Date,
            PropertyType::Datetime,
            PropertyType::Number,
            PropertyType::Text,
            PropertyType::Multitext,
        ] {
            let (out, refused) = type_in("---\nkey:|\n---\n", "", kind);
            assert_eq!((out.as_str(), refused.len()), ("---\nkey:\n---\n", 0));
        }
        assert_eq!(
            press("---\nkey:|\n---\n", Key::Enter).as_deref(),
            Some("---\nkey:|\n---\n")
        );
    }

    #[test]
    fn a_checkbox_writes_true_or_false_in_place() {
        use focus_core::PropValue;
        let note = "---\ndone: false # reviewed\nkey: x\n---\n";
        let (range, word) = property_checkbox_edit(note, "done", true).expect("the line");
        let mut out = note.to_string();
        out.replace_range(range, &word);
        assert_eq!(out, "---\ndone: true # reviewed\nkey: x\n---\n");
        let read = focus_core::parse_meta(&out).props;
        assert_eq!(read[0].1, PropValue::Bool(true));

        let note = "---\r\ndone:\r\n---\r\n";
        let (range, word) = property_checkbox_edit(note, "done", true).expect("the line");
        let mut out = note.to_string();
        out.replace_range(range, &word);
        assert_eq!(out, "---\r\ndone: true\r\n---\r\n");
        assert_eq!(property_checkbox_edit(note, "other", true), None);
        assert_eq!(property_checkbox_edit("done: true\n", "done", true), None);
    }

    #[test]
    fn a_value_out_of_its_type_is_left_byte_for_byte() {
        let note = "---\ndue: 21/08/2020 # as typed\nn: '12'\ndone: false\n---\nBody.\n";
        let (range, word) = property_checkbox_edit(note, "done", true).expect("the line");
        let mut out = note.to_string();
        out.replace_range(range.clone(), &word);
        let changed = note.find("done:").unwrap();
        assert!(range.start > changed);
        assert_eq!(out[..changed], note[..changed]);
        assert_eq!(&out[changed..], "done: true\n---\nBody.\n");
        let (typed, _) = type_in(
            "---\ndue: 21/08/2020 # as typed\nn: '12'\nkey: |\n---\n",
            "7",
            focus_core::PropertyType::Number,
        );
        assert_eq!(
            typed,
            "---\ndue: 21/08/2020 # as typed\nn: '12'\nkey: 7\n---\n"
        );
    }

    #[test]
    fn a_new_property_line_goes_before_the_closing_fence() {
        let text = gpui_component::Rope::from("---\ntitle: a\n---\nbody\n");
        let (edit, open) = new_property(&text);
        assert_eq!(
            (edit.range, edit.text.as_str(), edit.caret),
            (13..13, "\n", 13)
        );
        assert_eq!((open.lines, open.line), (0..4, 2));
        let bare = gpui_component::Rope::from("body\n");
        let (edit, open) = new_property(&bare);
        assert_eq!(
            (edit.range, edit.text.as_str(), edit.caret),
            (0..0, "---\n\n---\n", 4)
        );
        assert_eq!((open.lines, open.line), (0..3, 1));
    }

    struct Field {
        text: String,
        caret: usize,
        kind: focus_core::PropertyKind,
        read: Option<String>,
        blank: Option<(String, usize)>,
        refused: Vec<FieldError>,
        ended: bool,
    }

    impl Field {
        fn new(note: &str, kind: focus_core::PropertyType, read: bool) -> Field {
            let caret = note.find('|').expect("a caret");
            let text = note.replacen('|', "", 1);
            let key = key_at(&Rope::from(text.as_str()), caret).expect("a value at the caret");
            Field {
                text,
                caret,
                kind: typed_prop(kind).kind,
                read: read.then_some(key),
                blank: None,
                refused: Vec::new(),
                ended: false,
            }
        }

        fn apply(&mut self, edit: Edit) {
            let rope = Rope::from(self.text.as_str());
            let blank = self.blank.as_ref().map(|(k, n)| (k.as_str(), *n));
            let edit = emptied(&rope, edit, blank);
            if let Some(key) = &self.read
                && !stays_in(&rope, key, &edit)
            {
                return;
            }
            self.text.replace_range(edit.range.clone(), &edit.text);
            self.caret = edit.caret;
        }

        fn key(&mut self, pressed: Key) {
            let rope = Rope::from(self.text.as_str());
            let caret = self.caret;
            if self.read.is_some() && matches!(pressed, Key::Enter | Key::ShiftEnter) {
                let (edit, ends) = read_enter(&rope, caret);
                if let Some(edit) = edit {
                    self.apply(edit);
                }
                self.ended |= ends;
                return;
            }
            let name = key_at(&rope, caret).unwrap_or_default();
            let at = At {
                text: &rope,
                item: None,
                path: Vec::new(),
                rich: None,
                prop: Some((name.as_str(), self.kind)),
                map: None,
                start: 0,
                caret,
                selection: caret..caret,
            };
            if let Some(why) = refused(&at, &pressed) {
                self.refused.push(why);
                return;
            }
            let blank = match pressed {
                Key::Text(_) => blanks_of_empty(&rope, caret),
                _ => None,
            };
            let edit = key(&at, &pressed).or_else(|| match &pressed {
                Key::Text(typed) => Some(Edit {
                    range: caret..caret,
                    text: typed.clone(),
                    caret: caret + typed.len(),
                }),
                Key::Backspace => {
                    let source = frontmatter(&rope)?;
                    let value = value_at(&source, caret)?;
                    let from = source[..caret].char_indices().last()?.0;
                    Some(if caret > value.value.start {
                        Edit {
                            range: from..caret,
                            text: String::new(),
                            caret: from,
                        }
                    } else {
                        Edit {
                            range: caret..caret,
                            text: String::new(),
                            caret: from,
                        }
                    })
                }
                _ => None,
            });
            drop(rope);
            if let Some(edit) = edit {
                self.apply(edit);
                if blank.is_some() {
                    self.blank = blank;
                }
            }
        }

        fn typed(&mut self, text: &str) -> &mut Field {
            for c in text.chars() {
                self.key(Key::Text(c.to_string()));
            }
            self
        }

        fn erase(&mut self, count: usize) -> &mut Field {
            for _ in 0..count {
                self.key(Key::Backspace);
            }
            self
        }

        fn shown(&self) -> String {
            let mut out = self.text.clone();
            out.insert(self.caret, '|');
            out
        }
    }

    fn value_of(note: &str, key: &str) -> focus_core::PropValue {
        focus_core::parse_meta(note)
            .props
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .unwrap_or_else(|| panic!("no {key} in {note}"))
    }

    const BODY: &str = "\n---\n# Title\n\nBody: text.\n- item\n";

    #[test]
    fn each_type_is_filled_and_changed_in_read_in_its_documented_form() {
        use focus_core::{PropValue, PropertyType};
        let note = |line: &str| format!("---\nfirst: x\n{line}\nlast: y{BODY}");
        let filled = |kind: PropertyType, typed: &str, written: &str| {
            let mut field = Field::new(&note("key:|"), kind, true);
            field.typed(typed);
            assert!(field.refused.is_empty(), "{typed}: {:?}", field.refused);
            assert_eq!(field.text, note(written), "{typed}");
            assert!(kind.fits(&value_of(&field.text, "key")), "{}", field.text);
            field
        };
        filled(PropertyType::Text, "A New Hope", "key: A New Hope");
        filled(
            PropertyType::Text,
            "[[Episode IV]]",
            "key: \"[[Episode IV]]\"",
        );
        filled(PropertyType::Number, "1977", "key: 1977");
        filled(PropertyType::Number, "3.14", "key: 3.14");
        filled(PropertyType::Date, "2020-08-21", "key: 2020-08-21");
        filled(
            PropertyType::Datetime,
            "2020-08-21T10:30:00",
            "key: 2020-08-21T10:30:00",
        );
        filled(PropertyType::Multitext, "Leia", "key:\n  - Leia");
        filled(PropertyType::Aliases, "Hope", "key:\n  - Hope");
        assert_eq!(
            value_of(&note("key: 1977"), "key"),
            PropValue::Number(1977.0)
        );
        for (kind, before, typed, after) in [
            (PropertyType::Text, "key: plain", "other", "key: other"),
            (PropertyType::Number, "key: 1977", "-2.5", "key: -2.5"),
            (
                PropertyType::Date,
                "key: 2020-08-21",
                "2021-03-04",
                "key: 2021-03-04",
            ),
            (
                PropertyType::Datetime,
                "key: 2020-08-21T10:30",
                "2021-03-04T08:15",
                "key: 2021-03-04T08:15",
            ),
            (PropertyType::Multitext, "key:\n  - a", "b", "key:\n  - b"),
        ] {
            let at = note(&format!("{before}|"));
            let mut field = Field::new(&at, kind, true);
            let erase = before.len() - before.rfind(' ').expect("a value") - 1;
            field.erase(erase).typed(typed);
            assert_eq!(field.text, note(after), "{before}");
            assert!(field.refused.is_empty());
            assert!(kind.fits(&value_of(&field.text, "key")));
        }
        let mut field = Field::new(&note("key:\n  - a|"), PropertyType::Multitext, true);
        field.key(Key::Enter);
        assert!(!field.ended);
        field.typed("b");
        assert_eq!(field.text, note("key:\n  - a\n  - b"));
        field.key(Key::Enter);
        field.key(Key::Enter);
        assert!(field.ended);
        assert_eq!(field.text, note("key:\n  - a\n  - b"));
        let text = note("key: false");
        let (range, word) = property_checkbox_edit(&text, "key", true).expect("the line");
        let mut out = text.clone();
        out.replace_range(range, &word);
        assert_eq!(out, note("key: true"));
        assert_eq!(value_of(&out, "key"), PropValue::Bool(true));
    }

    #[test]
    fn an_invalid_entry_is_refused_in_read() {
        use focus_core::{PropertyType, ValueError};
        let note = format!("---\nkey: 2020-|{BODY}");
        let mut field = Field::new(&note, PropertyType::Date, true);
        field.typed("x/08");
        assert_eq!(field.text, format!("---\nkey: 2020-08{BODY}"));
        assert_eq!(field.refused, [FieldError::Value(ValueError::DateForm); 2]);
        field.key(Key::Enter);
        assert!(field.ended);
        assert_eq!(field.text, format!("---\nkey: 2020-08{BODY}"));
        let mut field = Field::new(&format!("---\nkey: |{BODY}"), PropertyType::Number, true);
        field.typed("1e");
        field.typed("x");
        assert_eq!(field.refused, [FieldError::Value(ValueError::NotNumber)]);
    }

    #[test]
    fn keys_typed_in_read_never_change_the_body_or_another_property() {
        use focus_core::PropertyType;
        let note = format!("---\nother: o\nkey: a|\nnext: n{BODY}");
        let mut field = Field::new(&note, PropertyType::Text, true);
        let body = |text: &str| text[text.find("\n---\n").unwrap()..].to_string();
        let before = body(&field.text);
        field.erase(4);
        field.key(Key::Delete);
        field.typed("b");
        field.key(Key::Enter);
        field.key(Key::ShiftEnter);
        field.key(Key::Tab);
        assert_eq!(body(&field.text), before);
        assert_eq!(field.text, format!("---\nother: o\nkey: b\nnext: n{BODY}"));
        let rope = Rope::from(field.text.as_str());
        let body_at = field.text.find("Body").unwrap();
        assert_eq!(blanks_of_empty(&rope, body_at), None);
        assert_eq!(tag_error(&rope, body_at), None);
        assert_eq!(read_enter(&rope, body_at), (None, true));
        assert_eq!(key_at(&rope, body_at), None);
        for edit in [
            Edit {
                range: body_at..body_at,
                text: "x".into(),
                caret: body_at + 1,
            },
            Edit {
                range: 4..5,
                text: String::new(),
                caret: 4,
            },
            Edit {
                range: 13..18,
                text: String::new(),
                caret: 13,
            },
            Edit {
                range: 17..17,
                text: String::new(),
                caret: 10,
            },
        ] {
            assert!(!stays_in(&rope, "key", &edit), "{edit:?}");
        }
        assert!(stays_in(
            &rope,
            "key",
            &Edit {
                range: 18..18,
                text: "c".into(),
                caret: 19,
            }
        ));
    }

    #[test]
    fn read_keeps_the_caret_inside_the_value() {
        let text = Rope::from("---\nkey: \"a: b\"\ncast:\n  - a\n  - bc\nempty:\nblank: \n---\n");
        let at = |needle: &str| text.to_string().find(needle).unwrap();
        assert_eq!(value_span(&text, "key"), Some(at("a: b")..at("a: b") + 4));
        assert_eq!(value_span(&text, "cast"), Some(at("- a") + 2..at("bc") + 2));
        let empty = at("empty:") + "empty:".len();
        assert_eq!(value_span(&text, "empty"), Some(empty..empty));
        let blank = at("blank: ") + "blank: ".len();
        assert_eq!(value_span(&text, "blank"), Some(blank..blank));
        assert_eq!(value_span(&text, "none"), None);
    }

    #[test]
    fn an_empty_tags_is_filled_as_a_list_of_tags() {
        use focus_core::{PropValue, PropertyType};
        let mut field = Field::new(&format!("---\ntags:|{BODY}"), PropertyType::Tags, true);
        field.typed("recipe");
        assert_eq!(field.text, format!("---\ntags:\n  - recipe{BODY}"));
        field.key(Key::Enter);
        field.typed("inbox/to-read");
        assert_eq!(
            field.text,
            format!("---\ntags:\n  - recipe\n  - inbox/to-read{BODY}")
        );
        assert_eq!(
            value_of(&field.text, "tags"),
            PropValue::List(vec![
                PropValue::Text("recipe".into()),
                PropValue::Text("inbox/to-read".into())
            ])
        );
        assert!(field.refused.is_empty());
        let mut field = Field::new(&format!("---\ntags: |{BODY}"), PropertyType::Tags, false);
        field.typed("cooking");
        assert_eq!(field.text, format!("---\ntags: \n  - cooking{BODY}"));
    }

    #[test]
    fn a_key_no_tag_holds_is_refused_with_its_reason() {
        use focus_core::PropertyType;
        let note = format!("---\ntags:\n  - a|{BODY}");
        for (typed, why) in [
            (" ", FieldError::TagSpace),
            ("\t", FieldError::TagSpace),
            ("#", FieldError::TagHash),
            ("!", FieldError::TagChar('!')),
            (".", FieldError::TagChar('.')),
            (",", FieldError::TagChar(',')),
            ("[", FieldError::TagChar('[')),
            ("b c", FieldError::TagSpace),
        ] {
            let mut field = Field::new(&note, PropertyType::Tags, true);
            field.key(Key::Text(typed.into()));
            assert_eq!(field.text, note.replace('|', ""), "{typed:?}");
            assert_eq!(field.refused.first(), Some(&why), "{typed:?}");
        }
        let mut field = Field::new(&note, PropertyType::Tags, true);
        field.typed("_b-c/d1é\u{1f600}");
        assert!(field.refused.is_empty());
        assert!(field.text.contains("  - a_b-c/d1é\u{1f600}\n"));
        let text = Rope::from(format!("---\ntags:\n  - 1984{BODY}").as_str());
        let caret = text.to_string().find("1984").unwrap() + 4;
        assert_eq!(tag_error(&text, caret), Some(FieldError::TagNumber));
        let text = Rope::from(format!("---\ntags:\n  - y1984{BODY}").as_str());
        assert_eq!(tag_error(&text, caret + 1), None);
    }

    #[test]
    fn a_value_typed_then_erased_gives_back_the_bare_key() {
        use focus_core::PropertyType;
        for read in [true, false] {
            for (kind, typed) in [
                (PropertyType::Text, "plain"),
                (PropertyType::Text, "a: b"),
                (PropertyType::Number, "12"),
                (PropertyType::Date, "2020-08-21"),
            ] {
                let note = format!("---\nkey:|\nnext: n{BODY}");
                let mut field = Field::new(&note, kind, read);
                field.typed(typed);
                assert_ne!(field.shown(), note);
                for _ in 0..typed.len() {
                    if field.shown() != note {
                        field.erase(1);
                    }
                }
                assert_eq!(field.shown(), note, "{typed}");
            }
        }
        let mut field = Field::new(
            &format!("---\nkey: 1977|\nnext: n{BODY}"),
            PropertyType::Number,
            true,
        );
        field.erase(4);
        assert_eq!(field.shown(), format!("---\nkey:|\nnext: n{BODY}"));
        let mut field = Field::new(&format!("---\nkey: \"a|\"{BODY}"), PropertyType::Text, true);
        field.erase(1);
        assert_eq!(field.shown(), format!("---\nkey:|{BODY}"));
    }

    #[test]
    fn a_list_that_loses_its_last_element_goes_back_to_its_key() {
        use focus_core::PropertyType;
        let mut field = Field::new(
            &format!("---\ncast:|\nnext: n{BODY}"),
            PropertyType::Multitext,
            true,
        );
        field.typed("ab");
        assert_eq!(field.text, format!("---\ncast:\n  - ab\nnext: n{BODY}"));
        field.erase(2);
        assert_eq!(field.shown(), format!("---\ncast:|\nnext: n{BODY}"));
        field.typed("c");
        assert_eq!(field.text, format!("---\ncast:\n  - c\nnext: n{BODY}"));
        let mut field = Field::new(
            "---\r\ncast:\r\n  - a|\r\n---\r\n",
            PropertyType::Multitext,
            false,
        );
        field.erase(1);
        assert_eq!(field.shown(), "---\r\ncast:|\r\n---\r\n");
        let mut field = Field::new(
            &format!("---\ncast:\n  - a\n  - b|{BODY}"),
            PropertyType::Multitext,
            true,
        );
        field.erase(1);
        assert_eq!(field.shown(), format!("---\ncast:\n  - a\n  - |{BODY}"));
        field.erase(1);
        assert_eq!(field.shown(), format!("---\ncast:\n  - a|{BODY}"));
        field.erase(1);
        assert_eq!(field.shown(), format!("---\ncast:|{BODY}"));
    }

    #[test]
    fn a_blank_of_the_file_is_left_alone() {
        use focus_core::PropertyType;
        let note = format!("---\nkey: |\nstamp: \ntags: \n  - a{BODY}");
        let mut field = Field::new(&note, PropertyType::Number, true);
        field.typed("42").erase(2);
        assert_eq!(field.shown(), note);
        let note = format!("---\ntags: \n  - a|\nstamp: {BODY}");
        let mut field = Field::new(&note, PropertyType::Tags, true);
        field.erase(1);
        assert_eq!(field.shown(), format!("---\ntags: |\nstamp: {BODY}"));
        let note = format!("---\nkey: |\nstamp: {BODY}");
        let mut field = Field::new(&note, PropertyType::Date, true);
        field.erase(1);
        field.key(Key::Enter);
        assert!(field.ended);
        assert_eq!(field.text, note.replace('|', ""));
    }
}
