use std::ops::Range;

use focus_kit::{ActiveTokens, Tokens};
use gpui::{
    AnyElement, AnyView, Bounds, Context, CursorStyle, Div, Hsla, InteractiveElement, IntoElement,
    ParentElement, Pixels, Point, SharedString, Styled, Window, div, prelude::FluentBuilder, px,
};

use super::{Built, ReadingView, Row};
use crate::model::{Item, Node};

const RULE_OFFSET: f32 = 12.;
const REVEAL_MARGIN: f32 = 24.;

pub(super) enum InnerSlot {
    Block,
    Editor,
    Hidden,
}

fn overlap(lines: &Range<u32>, open: &Range<u32>) -> bool {
    lines.start < open.end.max(open.start + 1) && open.start < lines.end
}

pub(super) fn is_container(node: &Node) -> bool {
    matches!(
        node,
        Node::Tabs { .. } | Node::Details { .. } | Node::Footnotes(_)
    )
}

fn holds(item: &Item, open: &Range<u32>) -> bool {
    match &item.node {
        Node::Footnotes(notes) => notes.iter().any(|n| overlap(&n.lines, open)),
        _ => overlap(&item.lines, open),
    }
}

pub(super) fn source_caret(
    block: &str,
    text: &str,
    offset: usize,
    near: u32,
) -> Option<(u32, u32)> {
    const CONTEXT: usize = 48;
    const WINDOW: usize = 512;
    let shown = |c: &char| !matches!(c, '\u{2066}'..='\u{2069}');
    let before = text[..offset.min(text.len())].chars().filter(shown).count();
    let rendered: Vec<char> = text.chars().filter(shown).collect();
    if rendered.is_empty() {
        return None;
    }
    let from = before.saturating_sub(CONTEXT);
    let context = &rendered[from..(before + CONTEXT).min(rendered.len())];
    let at = before - from;
    let source: Vec<char> = block.chars().collect();
    let same = |a: char, b: char| a == b || (a.is_whitespace() && b.is_whitespace());
    let line_of = |index: usize| source[..index].iter().filter(|c| **c == '\n').count() as u32;
    let mut best: Option<(usize, usize, u32, usize)> = None;
    for start in (0..source.len())
        .filter(|&i| same(source[i], context[0]))
        .take(4096)
    {
        let mut next = start;
        let mut found: Vec<Option<usize>> = Vec::with_capacity(context.len());
        for &c in context {
            let at = (next..(next + WINDOW).min(source.len())).find(|&i| same(source[i], c));
            if let Some(i) = at {
                next = i + 1;
            }
            found.push(at);
        }
        let matched = found.iter().flatten().count();
        if matched * 2 < context.len() {
            continue;
        }
        let after = found
            .iter()
            .skip(at + 1)
            .flatten()
            .next()
            .copied()
            .unwrap_or(source.len());
        let caret = match found.get(at).copied().flatten() {
            Some(first) => (first..after)
                .rev()
                .find(|&i| same(source[i], context[at]))
                .unwrap_or(first),
            None => found[..at].iter().flatten().last().map_or(start, |i| i + 1),
        };
        let last = found.iter().flatten().last().copied().unwrap_or(start);
        let distance = line_of(caret).abs_diff(near);
        let key = (usize::MAX - matched, last - start, distance, caret);
        if best.is_none_or(|b| (key.0, key.1, key.2) < (b.0, b.1, b.2)) {
            best = Some(key);
        }
    }
    let caret = best?.3;
    let line = line_of(caret);
    let column = source[..caret]
        .iter()
        .rev()
        .take_while(|c| **c != '\n')
        .count() as u32;
    Some((line, column))
}

fn line_span(text: &str, lines: &Range<u32>) -> Range<usize> {
    let mut starts = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1));
    let start = starts
        .by_ref()
        .nth(lines.start as usize)
        .unwrap_or(text.len());
    let end = if lines.end > lines.start {
        starts
            .nth((lines.end - lines.start - 1) as usize)
            .unwrap_or(text.len())
    } else {
        start
    };
    start..end.max(start)
}

pub type LiveBlock = (LiveKind, Range<u32>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveKind {
    Block,
    Properties,
    Title,
    End,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveOpen {
    pub kind: LiveKind,
    pub lines: Range<u32>,
    pub line: u32,
    pub column: u32,
}

pub(super) struct OpenBlock {
    pub lines: Range<u32>,
    pub editor: AnyView,
}

impl ReadingView {
    pub fn set_live(&mut self, live: bool, cx: &mut Context<Self>) {
        if self.live == live {
            return;
        }
        self.live = live;
        self.editing = None;
        if !live {
            self.open = None;
            self.pending_open_lines = None;
        }
        self.relayout_below(None, cx);
    }

    pub fn live(&self) -> bool {
        self.live
    }

    pub fn open_block(&mut self, lines: Range<u32>, editor: AnyView, cx: &mut Context<Self>) {
        let from = self
            .open
            .as_ref()
            .map(|o| o.lines.start)
            .into_iter()
            .chain([lines.start])
            .min();
        let height = self.drawn_height(&lines);
        self.editor_height.set(height);
        self.open_floor = height;
        self.open = Some(OpenBlock { lines, editor });
        self.pending_open_lines = None;
        self.relayout_below(from, cx);
    }

    fn drawn_height(&self, lines: &Range<u32>) -> f32 {
        let Some(built) = &self.built else { return 0. };
        let inner = self
            .inner_bounds
            .borrow()
            .values()
            .flatten()
            .filter(|(l, _)| l == lines)
            .map(|(_, b)| f32::from(b.size.height))
            .next();
        if let Some(height) = inner {
            return height;
        }
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                !r.editor
                    && built
                        .items
                        .get(r.item)
                        .is_some_and(|i| overlap(&i.lines, lines))
            })
            .filter_map(|(at, r)| {
                let height = f32::from(self.list.bounds_for_item(at + 1)?.size.height);
                let (_, bottom) = self.block_insets(&built.items[r.item].node, r);
                Some(height - bottom)
            })
            .sum()
    }

    pub fn set_open_lines(&mut self, lines: Range<u32>, cx: &mut Context<Self>) {
        if self.busy() {
            self.pending_open_lines = Some(lines);
            return;
        }
        if let Some(open) = &mut self.open {
            if open.lines == lines {
                return;
            }
            let from = open.lines.start.min(lines.start);
            open.lines = lines;
            self.relayout_below(Some(from), cx);
        }
    }

    pub fn close_block(&mut self, cx: &mut Context<Self>) {
        self.editor_bounds.set(None);
        if let Some(open) = self.open.take() {
            self.pending_open_lines = None;
            self.relayout_below(Some(open.lines.start), cx);
        }
    }

    pub fn open_lines(&self) -> Option<Range<u32>> {
        self.open.as_ref().map(|o| o.lines.clone())
    }

    pub fn live_blocks(&self) -> Vec<LiveBlock> {
        let Some(built) = &self.built else {
            return Vec::new();
        };
        let mut out = Vec::new();
        out.extend(
            built
                .frontmatter_lines
                .clone()
                .map(|l| (LiveKind::Properties, l)),
        );
        out.extend(built.title_lines.clone().map(|l| (LiveKind::Title, l)));
        for (index, item) in built.items.iter().enumerate() {
            self.blocks_of(index, &item.node, &item.lines, &mut out);
        }
        out.sort_by_key(|(_, lines)| lines.start);
        out
    }

    fn blocks_of(&self, item: usize, node: &Node, lines: &Range<u32>, out: &mut Vec<LiveBlock>) {
        match node {
            Node::Tabs { key, tabs } if !tabs.is_empty() => {
                let tab = &tabs[self.selected_tab(item, *key, tabs)];
                for inner in &tab.items {
                    self.blocks_of(item, &inner.node, &inner.lines, out);
                }
            }
            Node::Details {
                key,
                open,
                child_lines,
                ..
            } if self.callouts.get(&(item, *key)).copied().unwrap_or(*open)
                && !child_lines.is_empty() =>
            {
                out.extend(
                    child_lines
                        .iter()
                        .filter(|l| !l.is_empty())
                        .map(|l| (LiveKind::Block, l.clone())),
                );
            }
            Node::Footnotes(notes) => out.extend(
                notes
                    .iter()
                    .filter(|n| !n.lines.is_empty())
                    .map(|n| (LiveKind::Block, n.lines.clone())),
            ),
            _ if lines.is_empty() => {}
            _ => out.push((LiveKind::Block, lines.clone())),
        }
    }

    pub fn live_neighbors(&self, lines: &Range<u32>) -> (Option<LiveBlock>, Option<LiveBlock>) {
        let blocks = self.live_blocks();
        let before = blocks
            .iter()
            .rev()
            .find(|(_, l)| l.end <= lines.start)
            .cloned();
        let after = blocks
            .iter()
            .find(|(_, l)| l.start >= lines.end.max(lines.start + 1))
            .cloned();
        (before, after)
    }

    pub fn live_block_at(&self, line: u32) -> Option<LiveBlock> {
        self.live_blocks()
            .into_iter()
            .find(|(_, l)| l.contains(&line))
    }

    pub(super) fn place_editor(&mut self) {
        let Some(open) = &self.open else { return };
        let Some(built) = &self.built else { return };
        if self.open_in_header(built).is_some() {
            return;
        }
        let overlaps = |item: usize| overlap(&built.items[item].lines, &open.lines);
        if self.inner_container(built).is_some() {
            return;
        }
        let at = self
            .rows
            .iter()
            .position(|r| overlaps(r.item))
            .unwrap_or_else(|| {
                self.rows
                    .iter()
                    .position(|r| built.items[r.item].lines.start >= open.lines.start)
                    .unwrap_or(self.rows.len())
            });
        self.rows.retain(|r| !overlaps(r.item));
        let item = self.rows.get(at).map_or(built.items.len(), |r| r.item);
        self.rows.insert(
            at.min(self.rows.len()),
            Row {
                item,
                part: 0,
                parts: 1,
                editor: true,
            },
        );
    }

    pub(super) fn relayout_below(&mut self, from: Option<u32>, cx: &mut Context<Self>) {
        let old = self.rows.len();
        let keep = match (from, &self.built) {
            (Some(line), Some(built)) => self
                .rows
                .iter()
                .position(|r| {
                    r.editor || built.items.get(r.item).is_none_or(|i| i.lines.end > line)
                })
                .unwrap_or(old),
            _ => 0,
        };
        self.recompute();
        let header_changed = from.is_none_or(|line| {
            self.built.as_ref().is_some_and(|b| {
                b.frontmatter_lines.as_ref().is_some_and(|l| l.end > line)
                    || b.title_lines.as_ref().is_some_and(|l| l.end > line)
            })
        });
        let start = if header_changed { 0 } else { keep + 1 };
        let top = self.list.logical_scroll_top();
        self.list
            .splice(start..old + 1, self.rows.len() + 1 - start);
        if start <= top.item_ix {
            self.list.scroll_to(top);
        }
        cx.notify();
    }

    fn inner_container(&self, built: &Built) -> Option<usize> {
        let open = &self.open.as_ref()?.lines;
        built.items.iter().position(|item| {
            is_container(&item.node)
                && holds(item, open)
                && match &item.node {
                    Node::Footnotes(_) => true,
                    _ => open.start > item.lines.start,
                }
        })
    }

    pub(super) fn live_target(&self, position: Point<Pixels>) -> Option<LiveOpen> {
        let mut open = self.live_block_under(position)?;
        if open.kind != LiveKind::End {
            self.place_caret(position, &mut open);
        }
        Some(open)
    }

    fn place_caret(&self, position: Point<Pixels>, open: &mut LiveOpen) {
        let Some(built) = &self.built else { return };
        let Some(source) = built.source.as_deref() else {
            return;
        };
        let Some((key, offset, rich)) = self.registry.locate(position) else {
            return;
        };
        let item = match open.kind {
            LiveKind::Properties | LiveKind::Title => None,
            _ => built.items.iter().position(|i| holds(i, &open.lines)),
        };
        if key.0 != item.map_or(0, |i| i as u32 + 1) {
            return;
        }
        if let Some(Item {
            node: Node::Code {
                line: fence, code, ..
            },
            lines,
        }) = item.and_then(|i| built.items.get(i))
            && *lines == open.lines
        {
            let part = if key.1 == code.ord && rich.text.len() == code.text.len() {
                0
            } else {
                key.1
            };
            let before = &rich.text[..offset.min(rich.text.len())];
            let row = before.matches('\n').count() as u32;
            let line = fence + 1 + part * super::CODE_PART as u32 + row;
            if !open.lines.contains(&line) {
                return;
            }
            let shown_line = rich.text.split('\n').nth(row as usize).unwrap_or_default();
            let source_line =
                source[line_span(source, &(line..line + 1))].trim_end_matches(['\n', '\r']);
            let indent = if source_line.ends_with(shown_line) {
                source_line.chars().count() - shown_line.chars().count()
            } else {
                0
            };
            open.line = line;
            open.column = (indent
                + before
                    .rsplit('\n')
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .count()) as u32;
            return;
        }
        let block = &source[line_span(source, &open.lines)];
        if let Some((line, column)) = source_caret(
            block,
            &rich.text,
            offset,
            open.line.saturating_sub(open.lines.start),
        ) {
            open.line = open.lines.start + line;
            open.column = column;
        }
    }

    fn live_block_under(&self, position: Point<Pixels>) -> Option<LiveOpen> {
        let built = self.built.as_ref()?;
        if self.open.is_some()
            && self
                .editor_bounds
                .get()
                .is_some_and(|b| b.contains(&position))
        {
            return None;
        }
        let viewport = self.list.viewport_bounds();
        if !viewport.contains(&position) {
            return None;
        }
        let top = self.list.logical_scroll_top().item_ix;
        let mut last_bottom = None;
        for row in top..=self.rows.len() {
            let Some(bounds) = self.list.bounds_for_item(row) else {
                continue;
            };
            last_bottom = Some(bounds.bottom());
            if bounds.bottom() < position.y {
                continue;
            }
            if bounds.top() > position.y {
                break;
            }
            let fraction =
                ((position.y - bounds.top()) / bounds.size.height.max(px(1.))).clamp(0., 0.999);
            if row == 0 {
                let (kind, lines) = self.header_target(position)?;
                return Some(LiveOpen {
                    line: lines.start,
                    kind,
                    lines,
                    column: 0,
                });
            }
            let r = self.rows.get(row - 1)?;
            if r.editor {
                return None;
            }
            let inner = self.inner_bounds.borrow().get(&r.item).and_then(|found| {
                found
                    .iter()
                    .filter(|(_, b)| b.contains(&position))
                    .min_by(|a, b| {
                        f32::from(a.1.size.height).total_cmp(&f32::from(b.1.size.height))
                    })
                    .cloned()
            });
            if let Some((lines, b)) = inner.filter(|(lines, _)| !lines.is_empty()) {
                let fraction =
                    ((position.y - b.top()) / b.size.height.max(px(1.))).clamp(0., 0.999);
                let line = (lines.start + (fraction * (lines.end - lines.start) as f32) as u32)
                    .min(lines.end - 1);
                return Some(LiveOpen {
                    kind: LiveKind::Block,
                    lines,
                    line,
                    column: 0,
                });
            }
            let lines = built.items.get(r.item)?.lines.clone();
            if lines.is_empty() {
                return None;
            }
            let (start, count) = if r.parts > 1 {
                let first = lines.start + 1 + r.part * super::CODE_PART as u32;
                (
                    first,
                    (super::CODE_PART as u32)
                        .min(lines.end.saturating_sub(first))
                        .max(1),
                )
            } else {
                (lines.start, lines.end - lines.start)
            };
            let line = (start + (fraction * count as f32) as u32).min(lines.end - 1);
            return Some(LiveOpen {
                kind: LiveKind::Block,
                lines,
                line,
                column: 0,
            });
        }
        let below = last_bottom.is_some_and(|b| position.y > b);
        below.then(|| LiveOpen {
            kind: LiveKind::End,
            lines: built.line_count..built.line_count,
            line: built.line_count,
            column: 0,
        })
    }

    fn open_row(&self) -> Option<usize> {
        let open = self.open.as_ref()?;
        let built = self.built.as_ref()?;
        if self.open_in_header(built).is_some() {
            return Some(0);
        }
        if let Some(at) = self.rows.iter().position(|r| r.editor) {
            return Some(at + 1);
        }
        self.rows
            .iter()
            .position(|r| {
                built
                    .items
                    .get(r.item)
                    .is_some_and(|i| holds(i, &open.lines))
            })
            .map(|at| at + 1)
    }

    pub fn reveal_open_block(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    pub(super) fn keep_open_block_visible(&mut self, window: &mut Window) {
        let last = self.editor_bounds.take();
        let Some(row) = self.open_row() else {
            self.parked = false;
            return;
        };
        let viewport = self.list.viewport_bounds();
        let row_bounds = self.list.bounds_for_item(row);
        self.parked = !match row_bounds {
            Some(b) => b.intersects(&viewport),
            None => self.unmeasured_row_shown(row, viewport),
        };
        if viewport.size.height <= px(0.) {
            return;
        }
        if row_bounds.is_none() && !self.parked {
            window.request_animation_frame();
            return;
        }
        let bounds = last.filter(|_| !self.parked).or(row_bounds);
        let inside = bounds.is_some_and(|b| {
            b.top() >= viewport.top() - px(0.5)
                && (b.bottom() <= viewport.bottom() + px(0.5)
                    || b.size.height >= viewport.size.height)
        });
        if inside && !self.parked {
            return;
        }
        match bounds.filter(|_| row_bounds.is_some_and(|b| b.intersects(&viewport))) {
            Some(b) if b.top() < viewport.top() || b.size.height >= viewport.size.height => self
                .list
                .scroll_by(b.top() - viewport.top() - px(REVEAL_MARGIN)),
            Some(b) => self
                .list
                .scroll_by(b.bottom() - viewport.bottom() + px(REVEAL_MARGIN)),
            None => self.list.scroll_to_reveal_item(row),
        }
        window.request_animation_frame();
    }

    fn unmeasured_row_shown(&self, row: usize, viewport: Bounds<Pixels>) -> bool {
        row == self.list.logical_scroll_top().item_ix
            || row
                .checked_sub(1)
                .and_then(|above| self.list.bounds_for_item(above))
                .is_some_and(|above| above.bottom() < viewport.bottom())
    }

    pub(super) fn parked_editor(&self) -> Option<AnyElement> {
        let open = self.open.as_ref().filter(|_| self.parked)?;
        let height = self.editor_height.clone();
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_0()
                .overflow_hidden()
                .child(
                    div()
                        .relative()
                        .w(px(self.room()))
                        .child(open.editor.clone())
                        .child(
                            gpui::canvas(
                                move |b, _, _| height.set(f32::from(b.size.height)),
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(super) fn open_in_header(&self, built: &Built) -> Option<LiveKind> {
        let open = &self.open.as_ref()?.lines;
        if built
            .frontmatter_lines
            .as_ref()
            .is_some_and(|l| l.start == open.start)
        {
            Some(LiveKind::Properties)
        } else if built
            .title_lines
            .as_ref()
            .is_some_and(|l| l.start == open.start)
        {
            Some(LiveKind::Title)
        } else {
            None
        }
    }

    fn rule_offset(&self) -> f32 {
        self.gutter().min(RULE_OFFSET)
    }

    pub(super) fn open_editor(&self, t: &Tokens) -> AnyElement {
        let Some(open) = &self.open else {
            return div().into_any_element();
        };
        let dir = focus_kit::Dir::current();
        let rule = dir
            .start(div().absolute(), px(-self.rule_offset()))
            .top_0()
            .bottom_0()
            .w(px(1.))
            .bg(t.ink);
        if self.parked {
            return div()
                .relative()
                .w_full()
                .h(px(self.editor_height.get().max(1.)))
                .bg(t.bg_2)
                .child(rule)
                .into_any_element();
        }
        let (bounds, height) = (self.editor_bounds.clone(), self.editor_height.clone());
        let editor = div()
            .relative()
            .w_full()
            .min_h(px(self.open_floor))
            .bg(t.bg_2)
            .child(open.editor.clone())
            .child(rule)
            .child(
                gpui::canvas(
                    move |b, _, _| {
                        bounds.set(Some(b));
                        height.set(f32::from(b.size.height));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        div().w_full().child(editor).into_any_element()
    }

    pub(super) fn inner_slot(
        &self,
        lines: &Range<u32>,
        container: bool,
        editor_drawn: &mut bool,
    ) -> InnerSlot {
        let Some(open) = &self.open else {
            return InnerSlot::Block;
        };
        if lines.is_empty()
            || !overlap(lines, &open.lines)
            || (container && open.lines.start > lines.start)
        {
            return InnerSlot::Block;
        }
        if std::mem::replace(editor_drawn, true) {
            InnerSlot::Hidden
        } else {
            InnerSlot::Editor
        }
    }

    pub(super) fn live_hover(
        &self,
        row: usize,
        body: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.live {
            return body;
        }
        let Some(built) = &self.built else {
            return body;
        };
        let rule = match row {
            0 => None,
            _ => self.rows.get(row - 1).filter(|r| !r.editor).and_then(|r| {
                let item = built.items.get(r.item)?;
                let shows_inner = match &item.node {
                    Node::Tabs { .. } | Node::Footnotes(_) => true,
                    Node::Details { key, open, .. } => {
                        self.callouts.get(&(r.item, *key)).copied().unwrap_or(*open)
                    }
                    _ => false,
                };
                (!item.lines.is_empty() && !shows_inner).then(|| self.block_insets(&item.node, r))
            }),
        };
        let t = *cx.tokens();
        let (background, handle) = match row.checked_sub(1).and_then(|r| self.rows.get(r)) {
            Some(r) if !r.editor => self.row_handle(&r.clone(), &t, cx),
            _ => (None, None),
        };
        let group: SharedString = format!("live-{row}").into();
        div()
            .id(("live-row", row))
            .group(group.clone())
            .relative()
            .w_full()
            .cursor(CursorStyle::IBeam)
            .children(background)
            .child(body)
            .when_some(rule, |this, (top, bottom)| {
                this.child(self.hover_rule(group, self.gutter(), top, bottom, t.line_2))
            })
            .children(handle)
            .into_any_element()
    }

    pub(super) fn block_insets(&self, node: &Node, row: &Row) -> (f32, f32) {
        let gap = (self.typesetting.base * 0.8).round();
        match node {
            Node::Heading { level, .. } => self.heading_space(*level),
            Node::Code { .. } if row.part + 1 < row.parts => (0., 0.),
            _ => (0., gap),
        }
    }

    pub(super) fn hover_rule(
        &self,
        group: SharedString,
        start: f32,
        top: f32,
        bottom: f32,
        color: Hsla,
    ) -> Div {
        focus_kit::Dir::current()
            .start(div().absolute(), px(start - self.rule_offset()))
            .top(px(top))
            .bottom(px(bottom))
            .w(px(1.))
            .invisible()
            .group_hover(group, |s| s.visible())
            .bg(color)
    }

    fn header_target(&self, position: Point<Pixels>) -> Option<(LiveKind, Range<u32>)> {
        let built = self.built.as_ref()?;
        let props = self.props_bounds.get();
        let on_props = props.is_some_and(|b| position.y >= b.top());
        match (&built.frontmatter_lines, &built.title_lines) {
            (Some(lines), _) if on_props => Some((LiveKind::Properties, lines.clone())),
            (_, Some(lines)) => Some((LiveKind::Title, lines.clone())),
            (Some(lines), None) => Some((LiveKind::Properties, lines.clone())),
            (None, None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{line_span, source_caret};

    #[test]
    fn the_caret_lands_on_the_character_clicked_markup_skipped() {
        let block = "Paragraphe avec **gras**, ~~barré~~ et [[Accueil|l'accueil]].\nDeuxième ligne, un mot composé.\n";
        let rendered = "Paragraphe avec gras, barré et l'accueil. Deuxième ligne, un mot composé.";
        let at = |word: &str| rendered.find(word).unwrap_or_default();
        assert_eq!(source_caret(block, rendered, at("barré"), 0), Some((0, 28)));
        assert_eq!(
            source_caret(block, rendered, at("l'accueil"), 0),
            Some((0, 49))
        );
        assert_eq!(source_caret(block, rendered, at("mot"), 1), Some((1, 19)));
        assert_eq!(
            source_caret(block, rendered, rendered.len(), 1),
            Some((1, 31))
        );
        assert_eq!(
            source_caret("- [ ] Tâche ouverte", "Tâche ouverte", "Tâche ".len(), 0),
            Some((0, 12))
        );
        assert_eq!(source_caret("```\ncode\n```", "", 0, 0), None);
    }

    #[test]
    fn among_equal_texts_the_line_nearest_the_click_wins() {
        let block = "| oui | non |\n| --- | --- |\n| oui | oui |\n";
        assert_eq!(source_caret(block, "oui", 1, 2), Some((2, 3)));
        assert_eq!(source_caret(block, "oui", 1, 0), Some((0, 3)));
    }

    #[test]
    fn a_line_span_covers_whole_lines() {
        let text = "a\nbb\nccc\n";
        assert_eq!(&text[line_span(text, &(1..2))], "bb\n");
        assert_eq!(&text[line_span(text, &(1..3))], "bb\nccc\n");
        assert_eq!(&text[line_span(text, &(3..4))], "");
    }
}
