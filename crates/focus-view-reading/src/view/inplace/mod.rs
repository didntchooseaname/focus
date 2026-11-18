#![expect(
    dead_code,
    reason = "the element families fill `key` and the drawing hooks"
)]

use std::ops::Range;
use std::sync::Arc;

use gpui::{
    Bounds, Context, Entity, EntityInputHandler, Hsla, Pixels, Point, Task, UTF16Selection, Window,
    point, px, size,
};
use gpui_component::input::InputState;
use gpui_component::{Rope, RopeExt as _};

use super::{LiveBlock, LiveKind, ReadingView};
use crate::model::{Item, Node, Rich};
use crate::srcmap::{self, SourceMap};
use crate::text::TextKey;

pub(super) mod above;
mod caret;
pub(super) mod code;
mod keys;
pub(super) mod list;
pub(super) mod props;
pub(super) mod quote;
mod rebuild;
pub(super) mod table;

pub(crate) use keys::bindings;
pub(super) use keys::on_actions;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edit {
    pub range: Range<usize>,
    pub text: String,
    pub caret: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Key {
    Enter,
    ShiftEnter,
    Tab,
    ShiftTab,
    Backspace,
    Delete,
    Text(String),
}

pub(crate) struct At<'a> {
    pub text: &'a Rope,
    pub item: Option<&'a Item>,
    pub path: Vec<&'a Node>,
    pub rich: Option<&'a Rich>,
    pub prop: Option<(&'a str, focus_core::PropertyKind)>,
    pub map: Option<Arc<SourceMap>>,
    pub start: usize,
    pub caret: usize,
    pub selection: Range<usize>,
}

pub(super) struct Editing {
    goal_x: Option<Pixels>,
    shown: bool,
    blink: Option<Task<()>>,
    activation: Option<gpui::Subscription>,
    reveal: bool,
    pending: Option<caret::Vertical>,
    press: Option<usize>,
    found: Option<Range<usize>>,
    reveal_to: Option<usize>,
    field: Option<String>,
    blank: Option<(String, usize)>,
}

impl Editing {
    fn new() -> Self {
        Editing {
            goal_x: None,
            shown: true,
            blink: None,
            activation: None,
            reveal: true,
            pending: None,
            press: None,
            found: None,
            reveal_to: None,
            field: None,
            blank: None,
        }
    }
}

pub(super) struct Frame {
    pub text: Rope,
    pub anchor: usize,
    pub head: usize,
    pub marked: Option<Range<usize>>,
    pub caret_shown: bool,
    pub found: Option<(std::rc::Rc<Vec<Range<usize>>>, usize)>,
}

impl Frame {
    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}

pub(super) struct Decor {
    pub rich: Option<Rich>,
    pub map: Option<Arc<SourceMap>>,
    pub caret: Option<(usize, Hsla)>,
    pub selection: Option<Range<usize>>,
    pub marked: Option<Range<usize>>,
    pub found: Vec<(Range<usize>, Hsla)>,
}

pub(super) fn texts<'a>(node: &'a Node, out: &mut Vec<&'a Rich>) {
    match node {
        Node::Heading { rich, .. } | Node::Paragraph(rich) | Node::Math(rich) | Node::Ref(rich) => {
            out.push(rich)
        }
        Node::List { entries, .. } => {
            for entry in entries {
                for child in &entry.children {
                    texts(child, out);
                }
            }
        }
        Node::Code { code, .. } => out.push(code),
        Node::Quote(children) => children.iter().for_each(|c| texts(c, out)),
        Node::Callout {
            title, children, ..
        } => {
            out.push(title);
            children.iter().for_each(|c| texts(c, out));
        }
        Node::Details {
            summary, children, ..
        } => {
            out.push(summary);
            children.iter().for_each(|c| texts(c, out));
        }
        Node::Tabs { tabs, .. } => {
            for tab in tabs {
                for item in &tab.items {
                    texts(&item.node, out);
                }
            }
        }
        Node::Table { head, rows, .. } => {
            out.extend(head.iter());
            out.extend(rows.iter().flatten());
        }
        Node::Mermaid { .. }
        | Node::Query(_)
        | Node::Base { .. }
        | Node::Directive(_)
        | Node::Embed { .. }
        | Node::Image(_)
        | Node::Rule
        | Node::Html(_)
        | Node::Footnotes(_) => {}
    }
}

fn path_to<'a>(node: &'a Node, target: &Rich, path: &mut Vec<&'a Node>) -> bool {
    path.push(node);
    let mut own = Vec::new();
    match node {
        Node::Heading { rich, .. } | Node::Paragraph(rich) | Node::Math(rich) | Node::Ref(rich) => {
            own.push(rich)
        }
        Node::Code { code, .. } => own.push(code),
        Node::Callout { title, .. } => own.push(title),
        Node::Details { summary, .. } => own.push(summary),
        Node::Table { head, rows, .. } => {
            own.extend(head.iter());
            own.extend(rows.iter().flatten());
        }
        _ => {}
    }
    if own.iter().any(|r| std::ptr::eq(*r, target)) {
        return true;
    }
    let children: Vec<&Node> = match node {
        Node::List { entries, .. } => entries.iter().flat_map(|e| e.children.iter()).collect(),
        Node::Quote(children) | Node::Callout { children, .. } | Node::Details { children, .. } => {
            children.iter().collect()
        }
        Node::Tabs { tabs, .. } => tabs
            .iter()
            .flat_map(|t| t.items.iter().map(|i| &i.node))
            .collect(),
        _ => Vec::new(),
    };
    for child in children {
        if path_to(child, target, path) {
            return true;
        }
    }
    path.pop();
    false
}

fn element_key(at: &At, key: &Key) -> Option<Edit> {
    if at.item.is_none() {
        return props::key(at, key);
    }
    at.path.iter().rev().find_map(|node| match node {
        Node::List { .. } => list::key(at, key),
        Node::Table { .. } => table::key(at, key),
        Node::Quote(_) | Node::Callout { .. } => quote::key(at, key),
        Node::Code { .. } => code::key(at, key),
        _ => None,
    })
}

impl ReadingView {
    pub fn set_in_place(&mut self, field: Option<Entity<InputState>>, cx: &mut Context<Self>) {
        if field.is_none() {
            self.editing = None;
        }
        self.in_place = field;
        cx.notify();
    }

    pub fn in_place(&self) -> bool {
        self.in_place.is_some()
    }

    pub fn set_note_field(&mut self, field: Entity<InputState>) {
        self.note_field = Some(field);
    }

    pub(super) fn field(&self) -> Option<&Entity<InputState>> {
        if self.live {
            self.in_place.as_ref()
        } else {
            self.note_field.as_ref()
        }
    }

    pub fn caret_shown(&self) -> bool {
        self.editing.is_some() && self.field().is_some()
    }

    pub fn text_changed(&mut self, cx: &mut Context<Self>) {
        if self.caret_shown()
            && let Some(field) = self.field().cloned()
        {
            let text = field.read(cx).text().clone();
            let folds = self.folds_after_move.take();
            if !self.rebuild_edited(&text, cx) {
                self.in_place_text = None;
            } else if let Some(folds) = folds {
                self.apply_folds(&folds);
                self.refold(cx);
            }
        }
        if let Some(editing) = &mut self.editing {
            editing.reveal = true;
        }
        cx.notify();
    }

    pub fn remove_caret(&mut self, cx: &mut Context<Self>) {
        if self.editing.take().is_some() {
            cx.notify();
        }
    }

    pub fn caret_block(&self) -> Option<LiveBlock> {
        if !self.caret_shown() {
            return None;
        }
        let frame = self.in_place_frame.as_ref()?;
        self.live_block_at(frame.text.offset_to_position(frame.head).line)
    }

    pub fn bounds_for_byte(&self, byte: usize) -> Option<Bounds<Pixels>> {
        let text = &self.in_place_frame.as_ref()?.text;
        let hits = self.registry.0.borrow();
        hits.iter().find_map(|hit| {
            let map = hit.map.as_ref()?;
            let origin = self.origin(hit.key.0, text)?;
            let shown = map.to_shown(byte.checked_sub(origin)?)?;
            let at = hit.layout.position_for_index(shown)?;
            Some(Bounds::new(at, size(px(1.), hit.layout.line_height())))
        })
    }

    pub fn place_caret_at(
        &mut self,
        line: u32,
        column: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.in_place.clone() else {
            return;
        };
        field.update(cx, |field, cx| {
            let byte = field
                .text()
                .position_to_offset(&gpui_component::input::Position {
                    line,
                    character: column,
                });
            field.set_selection(byte, byte, cx);
        });
        self.start_editing(window, cx);
    }

    pub(super) fn accepts_in_place(&mut self, built: &crate::Built, cx: &gpui::App) -> bool {
        if !self.caret_shown() {
            return true;
        }
        let (Some(field), Some(source)) = (self.field(), &built.source) else {
            return true;
        };
        let text = field.read(cx).text();
        if text.len() != source.len() || *text != **source {
            return false;
        }
        self.in_place_text = Some(text.clone());
        true
    }

    pub(super) fn start_editing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.is_none() {
            self.in_place_text = self.field().and_then(|field| {
                let text = field.read(cx).text();
                let source = self.built.as_ref()?.source.as_ref()?;
                (text.len() == source.len() && *text == **source).then(|| text.clone())
            });
        }
        window.focus(&self.focus);
        let editing = self.editing.get_or_insert_with(Editing::new);
        editing.reveal = true;
        self.restart_blink();
        cx.notify();
    }

    pub(super) fn restart_blink(&mut self) {
        if let Some(editing) = &mut self.editing {
            editing.shown = true;
            editing.blink = None;
        }
    }

    pub(super) fn origin(&self, key_item: u32, text: &Rope) -> Option<usize> {
        if key_item == 0 {
            return Some(0);
        }
        let item = self.built.as_ref()?.items.get(key_item as usize - 1)?;
        Some(text.line_start_offset(item.lines.start as usize))
    }

    pub(super) fn key_item_at(&self, byte: usize, text: &Rope) -> u32 {
        let Some(built) = &self.built else { return 0 };
        built
            .items
            .partition_point(|item| text.line_start_offset(item.lines.start as usize) <= byte)
            as u32
    }

    pub(super) fn mapped_texts(&self, key_item: u32) -> Vec<(&Rich, Arc<SourceMap>)> {
        let Some(built) = &self.built else {
            return Vec::new();
        };
        let mut found = Vec::new();
        if key_item == 0 {
            found.push(&built.header.title);
            found.extend(built.header.props.iter().map(|prop| &prop.value));
        } else if let Some(item) = built.items.get(key_item as usize - 1) {
            texts(&item.node, &mut found);
        }
        let mut mapped: Vec<(&Rich, Arc<SourceMap>)> = found
            .into_iter()
            .filter_map(|rich| Some((rich, srcmap::of(rich)?)))
            .collect();
        mapped.sort_by_key(|(rich, _)| rich.ord);
        mapped
    }

    pub(super) fn read_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.in_place_frame = None;
        if !self.caret_shown() {
            return;
        }
        let Some(field) = self.field().cloned() else {
            return;
        };
        let focused = self.focus.is_focused(window);
        let shown = self.editing.as_ref().is_some_and(|e| e.shown);
        let Some((text, anchor, head)) = self.field_state(window, cx) else {
            return;
        };
        let marked = field
            .update(cx, |field, cx| field.marked_text_range(window, cx))
            .map(|m| text.offset_utf16_to_offset(m.start)..text.offset_utf16_to_offset(m.end));
        let found = field.read(cx).find_ranges(cx);
        let current = found
            .as_ref()
            .and_then(|(ranges, current)| ranges.get(*current).cloned());
        if let Some(editing) = &mut self.editing
            && editing.found != current
        {
            editing.reveal_to = current.as_ref().map(|r| r.start);
            editing.reveal |= current.is_some();
            editing.found = current;
        }
        self.in_place_frame = Some(Frame {
            text,
            anchor,
            head,
            marked,
            caret_shown: shown && focused,
            found,
        });
        self.keep_blinking(window, cx);
        if self.editing.as_ref().is_some_and(|e| e.pending.is_some()) {
            let entity = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = entity.update(cx, |this, cx| this.resume_vertical(window, cx));
            });
            window.request_animation_frame();
        }
    }

    pub(super) fn click_map(&self, rich: &Rich) -> Option<Arc<SourceMap>> {
        let editable = if self.live {
            self.in_place.is_some()
        } else {
            self.note_field.is_some()
                && self.built.as_ref().is_some_and(|built| {
                    built
                        .header
                        .props
                        .iter()
                        .any(|prop| std::ptr::eq(&prop.value, rich))
                })
        };
        if editable { srcmap::of(rich) } else { None }
    }

    pub(super) fn decorate(
        &self,
        key: TextKey,
        rich: &Rich,
        t: &focus_kit::Tokens,
    ) -> Option<Decor> {
        let caret_color = t.ink;
        let frame = self.in_place_frame.as_ref()?;
        let mut decor = Decor {
            rich: None,
            map: None,
            caret: None,
            selection: None,
            marked: None,
            found: Vec::new(),
        };
        let Some(map) = srcmap::of(rich) else {
            return Some(decor);
        };
        let Some(origin) = self.origin(key.0, &frame.text) else {
            return Some(decor);
        };
        let relative = |byte: usize| byte.checked_sub(origin);
        let mut map = map;
        if let Some(head) = relative(frame.head)
            && map.holding(head).next().is_some()
            && let Some(span) = map.span()
        {
            let end = (origin + span.end).min(frame.text.len());
            let source = frame.text.slice(origin..end).to_string();
            if let Some((shown, shown_map)) = srcmap::reveal(rich, &map, head, &source) {
                decor.rich = Some(shown);
                map = Arc::new(shown_map);
            }
        }
        let selection = frame.selection();
        if selection.is_empty() {
            decor.caret = relative(frame.head)
                .and_then(|head| map.to_shown(head))
                .filter(|_| frame.caret_shown)
                .map(|at| (at, caret_color));
        } else if let (Some(start), Some(end)) =
            (relative(selection.start), relative(selection.end))
        {
            decor.selection = map.shown_range(&(start..end));
        } else if let Some(end) = relative(selection.end) {
            decor.selection = map.shown_range(&(0..end));
        }
        if let (Some((ranges, current)), Some(span)) = (&frame.found, map.span()) {
            let (from, to) = (origin + span.start, origin + span.end);
            let first = ranges.partition_point(|r| r.end <= from);
            for (index, range) in ranges.iter().enumerate().skip(first) {
                if range.start >= to {
                    break;
                }
                let (Some(start), Some(end)) = (relative(range.start), relative(range.end)) else {
                    continue;
                };
                if let Some(shown) = map.shown_range(&(start..end)) {
                    let color = if index == *current { t.sel } else { t.card_3 };
                    decor.found.push((shown, color));
                }
            }
        }
        decor.marked = frame
            .marked
            .as_ref()
            .and_then(|m| map.shown_range(&(relative(m.start)?..relative(m.end)?)));
        decor.map = Some(map);
        Some(decor)
    }

    fn at<'a>(&'a self, text: &'a Rope, caret: usize, selection: Range<usize>) -> At<'a> {
        let key_item = self.key_item_at(caret, text);
        let start = self.origin(key_item, text).unwrap_or(0);
        let item = (key_item > 0)
            .then(|| self.built.as_ref()?.items.get(key_item as usize - 1))
            .flatten();
        let relative = caret.saturating_sub(start);
        let mut path = Vec::new();
        let mut rich = None;
        let mut map = None;
        let built = self.built.as_deref();
        let found: Vec<&'a Rich> = match (built, item) {
            (Some(built), None) => std::iter::once(&built.header.title)
                .chain(built.header.props.iter().map(|prop| &prop.value))
                .collect(),
            (_, Some(item)) => {
                let mut out = Vec::new();
                texts(&item.node, &mut out);
                out
            }
            (None, None) => Vec::new(),
        };
        for candidate in found {
            if let Some(m) = srcmap::of(candidate)
                && m.stops()
                    .iter()
                    .any(|s| s.start <= relative && relative <= s.end)
            {
                rich = Some(candidate);
                map = Some(m);
                break;
            }
        }
        let prop = match (built, item) {
            (Some(built), None) => props::key_at(text, caret)
                .and_then(|key| built.header.props.iter().find(|p| p.key == key))
                .map(|p| {
                    let kind = props::typing_kind(p.kind, &p.key, self.prop_typing.as_ref());
                    (p.key.as_str(), kind)
                }),
            _ => None,
        };
        match (item, rich) {
            (Some(item), Some(target)) => {
                path_to(&item.node, target, &mut path);
            }
            (Some(item), None) => path.push(&item.node),
            _ => {}
        }
        At {
            text,
            item,
            path,
            rich,
            prop,
            map,
            start,
            caret,
            selection,
        }
    }

    pub(super) fn field_state(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Rope, usize, usize)> {
        let field = self.field().cloned()?;
        Some(field.update(cx, |field, cx| {
            let text = field.text().clone();
            let head = field.cursor();
            let selected = field
                .selected_text_range(true, window, cx)
                .map(|s| s.range)
                .unwrap_or(0..0);
            let range = text.offset_utf16_to_offset(selected.start)
                ..text.offset_utf16_to_offset(selected.end);
            let anchor = if head == range.start {
                range.end
            } else {
                range.start
            };
            (text, anchor, head)
        }))
    }

    pub(super) fn apply(&mut self, edit: Edit, window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.field().cloned() else {
            return;
        };
        let edit = if edit.text.is_empty() && !edit.range.is_empty() {
            let text = field.read(cx).text().clone();
            let blank = self
                .editing
                .as_ref()
                .and_then(|e| e.blank.as_ref())
                .map(|(key, n)| (key.as_str(), *n));
            props::emptied(&text, edit, blank)
        } else {
            edit
        };
        if !self.live {
            let text = field.read(cx).text().clone();
            let key = self.editing.as_ref().and_then(|e| e.field.clone());
            if !key.is_some_and(|key| props::stays_in(&text, &key, &edit)) {
                return;
            }
        }
        self.prop_refused = None;
        field.update(cx, |field, cx| {
            if !(edit.range.is_empty() && edit.text.is_empty()) {
                let text = field.text();
                let range = text.offset_to_offset_utf16(edit.range.start)
                    ..text.offset_to_offset_utf16(edit.range.end);
                field.replace_text_in_range(Some(range), &edit.text, window, cx);
            }
            field.set_selection(edit.caret, edit.caret, cx);
        });
        self.after_key(cx);
    }

    pub(super) fn select(&mut self, anchor: usize, head: usize, cx: &mut Context<Self>) {
        self.prop_refused = None;
        let Some(field) = self.field().cloned() else {
            return self.after_key(cx);
        };
        let (anchor, head) = if self.live {
            (anchor, head)
        } else {
            let text = field.read(cx).text().clone();
            let span = self
                .editing
                .as_ref()
                .and_then(|e| e.field.as_deref())
                .and_then(|key| props::value_span(&text, key));
            let Some(span) = span else {
                return;
            };
            let clamp = |byte: usize| byte.clamp(span.start, span.end);
            (clamp(anchor), clamp(head))
        };
        field.update(cx, |field, cx| field.set_selection(anchor, head, cx));
        self.after_key(cx);
    }

    fn after_key(&mut self, cx: &mut Context<Self>) {
        self.restart_blink();
        if let Some(editing) = &mut self.editing {
            editing.reveal = true;
        }
        cx.notify();
    }

    pub(super) fn press(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.quiet_handles();
        let Some((text, anchor, head)) = self.field_state(window, cx) else {
            return false;
        };
        let selection = anchor.min(head)..anchor.max(head);
        if !self.live && matches!(key, Key::Enter | Key::ShiftEnter) {
            let (edit, ends) = props::read_enter(&text, head);
            if let Some(edit) = edit {
                self.apply(edit, window, cx);
            }
            if ends {
                self.remove_caret(cx);
            }
            return true;
        }
        let (edit, refused, typing) = {
            let at = self.at(&text, head, selection.clone());
            let typing = at.prop.map(|(key, kind)| (key.to_string(), kind));
            match props::refused(&at, &key) {
                Some(why) => (None, at.prop.map(|(key, _)| (key.to_string(), why)), typing),
                None => (
                    element_key(&at, &key).or_else(|| self.common_key(&at, &key)),
                    None,
                    typing,
                ),
            }
        };
        self.prop_typing = typing;
        if let Some(refused) = refused {
            self.prop_refused = Some(refused);
            self.after_key(cx);
            return true;
        }
        let blank = match key {
            Key::Text(_) if selection.is_empty() => props::blanks_of_empty(&text, head),
            _ => None,
        };
        match edit {
            Some(edit) => {
                self.apply(edit, window, cx);
                if let Some(editing) = &mut self.editing {
                    editing.goal_x = None;
                    if blank.is_some() {
                        editing.blank = blank;
                    }
                }
                true
            }
            None => false,
        }
    }

    fn common_key(&self, at: &At, key: &Key) -> Option<Edit> {
        let selection = at.selection.clone();
        match key {
            Key::Text(text) => Some(Edit {
                range: selection.clone(),
                text: text.clone(),
                caret: selection.start + text.len(),
            }),
            Key::Enter | Key::ShiftEnter => Some(Edit {
                range: selection.clone(),
                text: "\n".into(),
                caret: selection.start + 1,
            }),
            Key::Tab | Key::ShiftTab => None,
            Key::Backspace | Key::Delete if !selection.is_empty() => Some(Edit {
                range: selection.clone(),
                text: String::new(),
                caret: selection.start,
            }),
            Key::Backspace => self.join(at, false),
            Key::Delete => self.join(at, true),
        }
    }
}

impl EntityInputHandler for ReadingView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let field = self.field().cloned()?;
        field.update(cx, |field, cx| {
            field.text_for_range(range, adjusted, window, cx)
        })
    }

    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let field = self.field().cloned()?;
        field.update(cx, |field, cx| {
            field.selected_text_range(ignore_disabled_input, window, cx)
        })
    }

    fn marked_text_range(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let field = self.field().cloned()?;
        field.update(cx, |field, cx| field.marked_text_range(window, cx))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(field) = self.field().cloned() {
            field.update(cx, |field, cx| field.unmark_text(window, cx));
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.field().cloned() else {
            return;
        };
        let composing = field.update(cx, |field, cx| {
            field.marked_text_range(window, cx).is_some()
        });
        if range.is_none() && !composing {
            self.press(Key::Text(text.to_string()), window, cx);
            return;
        }
        field.update(cx, |field, cx| {
            field.replace_text_in_range(range, text, window, cx)
        });
        self.after_key(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(field) = self.field().cloned() {
            field.update(cx, |field, cx| {
                field.replace_and_mark_text_in_range(
                    range,
                    new_text,
                    new_selected_range,
                    window,
                    cx,
                )
            });
            self.after_key(cx);
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let field = self.field()?.read(cx);
        let byte = field.text().offset_utf16_to_offset(range_utf16.start);
        self.bounds_for_byte(byte)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let byte = self.byte_at(point)?;
        let field = self.field()?.read(cx);
        Some(field.text().offset_to_offset_utf16(byte))
    }
}

impl ReadingView {
    pub(super) fn byte_at(&self, position: Point<Pixels>) -> Option<usize> {
        let text = &self.in_place_frame.as_ref()?.text;
        let (key, shown, map) = self.registry.locate_caret(position)?;
        Some(self.origin(key.0, text)? + map.to_source(shown)?)
    }

    pub(super) fn input_canvas(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        use gpui::{IntoElement as _, Styled as _};
        if !self.caret_shown() {
            return None;
        }
        let focus = self.focus.clone();
        let entity = cx.entity();
        Some(
            gpui::canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| {
                    window.handle_input(&focus, gpui::ElementInputHandler::new(bounds, entity), cx)
                },
            )
            .absolute()
            .size_full()
            .into_any_element(),
        )
    }

    fn keep_blinking(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = window.is_window_active() && self.focus.is_focused(window);
        let Some(editing) = &mut self.editing else {
            return;
        };
        if editing.activation.is_none() {
            editing.activation = Some(cx.observe_window_activation(window, |_, _, cx| cx.notify()));
        }
        if !active {
            editing.blink = None;
            editing.shown = true;
            return;
        }
        if editing.blink.is_some() {
            return;
        }
        editing.blink = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
                let blinking = this.update(cx, |this, cx| {
                    let Some(editing) = &mut this.editing else {
                        return false;
                    };
                    editing.shown = !editing.shown;
                    cx.notify();
                    true
                });
                if !matches!(blinking, Ok(true)) {
                    break;
                }
            }
        }));
    }

    pub(super) fn keep_caret_visible(&mut self, window: &mut Window) {
        let Some(frame) = &self.in_place_frame else {
            return;
        };
        let Some(editing) = &mut self.editing else {
            return;
        };
        if !std::mem::take(&mut editing.reveal) {
            return;
        }
        let head = editing.reveal_to.take().unwrap_or(frame.head);
        let key_item = self.key_item_at(head, &frame.text);
        let viewport = self.list.viewport_bounds();
        if viewport.size.height <= px(0.) {
            return;
        }
        match self.bounds_for_byte(head) {
            Some(caret) => {
                let margin = px(24.);
                if caret.top() < viewport.top() + margin {
                    self.list.scroll_by(caret.top() - viewport.top() - margin);
                    window.request_animation_frame();
                } else if caret.bottom() > viewport.bottom() - margin {
                    self.list
                        .scroll_by(caret.bottom() - viewport.bottom() + margin);
                    window.request_animation_frame();
                }
            }
            None if key_item > 0 && self.drawn_keys().all(|k| k != key_item) => {
                self.scroll_to_item(key_item as usize - 1);
                window.request_animation_frame();
            }
            None => {}
        }
    }

    fn drawn_keys(&self) -> impl Iterator<Item = u32> + '_ {
        let keys: Vec<u32> = self.registry.0.borrow().iter().map(|h| h.key.0).collect();
        keys.into_iter()
    }

    pub(super) fn editable_at(&self, kind: LiveKind, lines: &Range<u32>) -> bool {
        let Some(built) = &self.built else {
            return false;
        };
        match kind {
            LiveKind::Properties | LiveKind::Title => !self.mapped_texts(0).is_empty(),
            LiveKind::End => false,
            LiveKind::Block => built
                .items
                .iter()
                .position(|item| {
                    item.lines.start <= lines.start
                        && lines.start < item.lines.end.max(item.lines.start + 1)
                })
                .is_some_and(|i| !self.mapped_texts(i as u32 + 1).is_empty()),
        }
    }
}

pub(super) fn caret_point(view: &ReadingView, byte: usize) -> Option<(Point<Pixels>, Pixels)> {
    let bounds = view.bounds_for_byte(byte)?;
    Some((point(bounds.left(), bounds.top()), bounds.size.height))
}

impl ReadingView {
    fn field_text(&self, cx: &gpui::App) -> Option<Rope> {
        Some(self.field()?.read(cx).text().clone())
    }

    fn byte_in(&self, position: Point<Pixels>, text: &Rope) -> Option<usize> {
        let (key, shown, map) = self.registry.locate_caret(position)?;
        Some(self.origin(key.0, text)? + map.to_source(shown)?)
    }

    pub(super) fn press_in_place(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.in_place_press = None;
        if event.modifiers.alt || self.field().is_none() {
            return;
        }
        let Some(text) = self.field_text(cx) else {
            return;
        };
        let byte = if self.live {
            self.byte_in(event.position, &text)
        } else {
            self.value_byte_at(event.position, &text)
        };
        let Some(byte) = byte else {
            if !self.live {
                self.remove_caret(cx);
            }
            return;
        };
        if !self.live {
            let key = props::key_at(&text, byte);
            if key.is_none() {
                return self.remove_caret(cx);
            }
            if self
                .editing
                .as_ref()
                .is_some_and(|e| e.field.as_ref() != key.as_ref())
            {
                self.remove_caret(cx);
            }
            if (event.modifiers.shift || event.click_count > 1)
                && !self.begin_value_edit(byte, &text, window, cx)
            {
                return;
            }
        }
        self.in_place_press = Some(byte);
        if event.modifiers.shift && self.caret_shown() {
            let anchor = self
                .field_state(window, cx)
                .map_or(byte, |(_, anchor, _)| anchor);
            self.in_place_press = Some(anchor);
            self.select(anchor, byte, cx);
            return;
        }
        let range = match event.click_count {
            1 => return,
            2 => text.word_range(byte).unwrap_or(byte..byte),
            _ => match self.stop_at(&text, byte) {
                Some(stop) => {
                    let origin = self.origin(stop.key_item, &text).unwrap_or(0);
                    self.mapped_texts(stop.key_item)
                        .into_iter()
                        .find(|(rich, _)| rich.ord == stop.ord)
                        .and_then(|(_, map)| map.span())
                        .map_or(byte..byte, |span| origin + span.start..origin + span.end)
                }
                None => byte..byte,
            },
        };
        self.start_editing(window, cx);
        self.select(range.start, range.end, cx);
    }

    pub(super) fn caret_on_click(
        &mut self,
        open: &super::LiveOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(byte) = self.in_place_press.take() else {
            return false;
        };
        if !self.editable_at(open.kind, &open.lines) {
            return false;
        }
        self.start_editing(window, cx);
        if let Some(editing) = &mut self.editing {
            editing.goal_x = None;
        }
        self.select(byte, byte, cx);
        true
    }

    fn value_byte_at(&self, position: Point<Pixels>, text: &Rope) -> Option<usize> {
        let Some(super::Zone::PropField(index)) = self.zone_at(position) else {
            return None;
        };
        let prop = self.built.as_ref()?.header.props.get(index)?;
        if let Some(((key_item, ord), shown, map)) = self.registry.locate_caret(position)
            && key_item == 0
            && ord == prop.value.ord
        {
            return Some(self.origin(0, text)? + map.to_source(shown)?);
        }
        Some(prop.value.map.as_ref()?.span()?.end)
    }

    fn begin_value_edit(
        &mut self,
        byte: usize,
        text: &Rope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(key) = props::key_at(text, byte) else {
            return false;
        };
        self.start_editing(window, cx);
        if let Some(editing) = &mut self.editing {
            editing.field = Some(key);
        }
        true
    }

    pub(super) fn caret_on_value_click(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(byte) = self.in_place_press.take() else {
            return;
        };
        let Some(text) = self.field_text(cx) else {
            return;
        };
        if !self.begin_value_edit(byte, &text, window, cx) {
            return;
        }
        if let Some(editing) = &mut self.editing {
            editing.goal_x = None;
        }
        self.select(byte, byte, cx);
    }

    pub(super) fn drag_in_place(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(anchor) = self.in_place_press else {
            return;
        };
        let Some(text) = self.field_text(cx) else {
            return;
        };
        let Some(head) = self.byte_in(position, &text) else {
            return;
        };
        if self.editing.is_none() {
            let mut editing = Editing::new();
            if !self.live {
                editing.field = props::key_at(&text, anchor);
            }
            self.editing = Some(editing);
        }
        self.select(anchor, head, cx);
    }

    pub fn show_caret(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.in_place.is_some() && self.live {
            self.start_editing(window, cx);
            return;
        }
        if self.live {
            return;
        }
        let Some(field) = self.note_field.clone() else {
            return;
        };
        let (text, head) = {
            let field = field.read(cx);
            (field.text().clone(), field.cursor())
        };
        match props::key_at(&text, head) {
            Some(key) => {
                self.start_editing(window, cx);
                if let Some(editing) = &mut self.editing {
                    editing.field = Some(key);
                    editing.goal_x = None;
                }
            }
            None => self.remove_caret(cx),
        }
    }

    pub fn caret_in_property(&self, cx: &gpui::App) -> bool {
        if !self.caret_shown() {
            return false;
        }
        let Some(field) = self.field() else {
            return false;
        };
        let field = field.read(cx);
        props::key_at(field.text(), field.cursor()).is_some()
    }

    pub fn in_place_selection(&self) -> Option<Range<usize>> {
        if !self.live || self.in_place.is_none() || !self.caret_shown() {
            return None;
        }
        let selection = self.in_place_frame.as_ref()?.selection();
        (!selection.is_empty()).then_some(selection)
    }

    pub fn pointer_held(&self) -> bool {
        self.dragging
    }

    pub fn viewport(&self) -> Bounds<Pixels> {
        self.list.viewport_bounds()
    }

    pub fn selection_lines(
        &self,
        range: &Range<usize>,
    ) -> (Option<Bounds<Pixels>>, Option<Bounds<Pixels>>) {
        let Some(frame) = self.in_place_frame.as_ref() else {
            return (None, None);
        };
        let text = &frame.text;
        let hits = self.registry.0.borrow();
        let drawn = |byte: usize| {
            hits.iter().enumerate().find_map(|(at, hit)| {
                let map = hit.map.as_ref()?;
                let origin = self.origin(hit.key.0, text)?;
                let shown = map.to_shown(byte.checked_sub(origin)?)?;
                let point = hit.layout.position_for_index(shown)?;
                Some((at, point, hit.layout.line_height()))
            })
        };
        let boundaries = |from: usize, forward: bool| {
            std::iter::successors(Some(from), move |&b| {
                let mut next = if forward { b + 1 } else { b.checked_sub(1)? };
                while next > 0 && next < text.len() && !text.is_char_boundary(next) {
                    next = if forward { next + 1 } else { next - 1 };
                }
                Some(next)
            })
            .take_while(move |b| range.start <= *b && *b <= range.end)
            .take(256)
        };
        let first = boundaries(range.start, true).find_map(drawn);
        let last = boundaries(range.end, false).find_map(drawn);
        let line_box = |(_, point, height): (usize, Point<Pixels>, Pixels)| {
            Bounds::new(point, size(px(1.), height))
        };
        let first_box = first.map(|(index, point, height)| {
            let hit = &hits[index];
            let rtl = focus_kit::Dir::current().of_text(&hit.rich.text).rtl;
            let other = match last {
                Some((at, end, _)) if at == index && (end.y - point.y).abs() < px(1.) => end.x,
                _ if rtl => hit.bounds.left(),
                _ => hit.bounds.right(),
            };
            Bounds::from_corners(
                gpui::point(point.x.min(other), point.y),
                gpui::point(point.x.max(other), point.y + height),
            )
        });
        (first_box, last.map(line_box))
    }

    pub fn replace_in_place(
        &mut self,
        range: Range<usize>,
        text: &str,
        select: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.field().cloned() else {
            return;
        };
        self.prop_refused = None;
        field.update(cx, |field, cx| {
            let rope = field.text();
            let range16 =
                rope.offset_to_offset_utf16(range.start)..rope.offset_to_offset_utf16(range.end);
            field.replace_text_in_range(Some(range16), text, window, cx);
            field.set_selection(select.start, select.end, cx);
        });
        self.quiet_handles();
        self.after_key(cx);
    }

    pub fn insert_in_place(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.press(Key::Text(text.to_string()), window, cx);
    }
}

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl ReadingView {
    pub fn script_dump(&self) -> String {
        let hits = self.registry.0.borrow();
        let texts: Vec<String> = hits
            .iter()
            .map(|hit| {
                let optional = |o: Option<usize>| o.map_or("null".to_string(), |v| v.to_string());
                format!(
                    "{{\"key\":[{},{}],\"text\":{},\"mapped\":{},\"caret\":{},\"selection\":{}}}",
                    hit.key.0,
                    hit.key.1,
                    json_string(&hit.rich.text),
                    hit.map.is_some(),
                    optional(hit.caret),
                    hit.selection
                        .as_ref()
                        .map_or("null".to_string(), |r| format!("[{},{}]", r.start, r.end)),
                )
            })
            .collect();
        format!(
            "{{\"caret_shown\":{},\"texts\":[\n{}\n]}}\n",
            self.caret_shown(),
            texts.join(",\n")
        )
    }
}
