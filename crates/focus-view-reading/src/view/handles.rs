use focus_kit::{Icon, Tokens, icon, size};
use gpui::{
    AnyElement, Bounds, Context, CursorStyle, InteractiveElement, IntoElement, MouseButton,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Styled, Window, div,
    prelude::FluentBuilder, px,
};

use super::inplace::Edit;
use super::{ReadingView, Row};
use crate::blockmove::{self, Group};
use crate::model::Node;

const HIT: f32 = 28.;
const SQUARE: f32 = 26.;
const STRIP: f32 = 28.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HandleAt {
    pub item: usize,
    pub entry: Option<u32>,
    pub inset: Option<Bounds<Pixels>>,
}

pub(super) struct HandleDrag {
    at: HandleAt,
    group: Group,
    from: usize,
    text: String,
    slot: Option<usize>,
    tried: Vec<(usize, bool)>,
}

pub(super) type EntryBounds =
    std::rc::Rc<std::cell::RefCell<std::collections::HashMap<usize, Vec<(u32, Bounds<Pixels>)>>>>;

impl ReadingView {
    pub fn set_block_handles(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.handles != on {
            self.handles = on;
            self.handle_hover = None;
            self.handle_drag = None;
            cx.notify();
        }
    }

    fn handles_shown(&self) -> bool {
        self.handles
            && self.live
            && self.in_place.is_some()
            && !self.compact
            && self.open.is_none()
            && !self.handle_quiet
            && !self
                .in_place_frame
                .as_ref()
                .is_some_and(|frame| frame.anchor != frame.head)
            && !(self.anchor.is_some() && self.anchor != self.head)
    }

    pub(super) fn quiet_handles(&mut self) {
        self.handle_quiet = true;
        self.handle_hover = None;
    }

    pub(super) fn hover_handle(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.handle_drag.is_some() {
            return;
        }
        if self.handle_pointer != Some(position) {
            self.handle_pointer = Some(position);
            self.handle_pointed = false;
            if std::mem::take(&mut self.handle_quiet) {
                cx.notify();
            }
        }
        let at = self.handle_target(position);
        if at != self.handle_hover {
            self.handle_hover = at;
            cx.notify();
        }
    }

    fn handle_target(&self, position: Point<Pixels>) -> Option<HandleAt> {
        if !self.handles_shown() {
            return None;
        }
        let built = self.built.as_ref()?;
        let viewport = self.list.viewport_bounds();
        if !viewport.contains(&position) {
            return None;
        }
        let first = self.list.logical_scroll_top().item_ix.max(1);
        for row in first..=self.rows.len() {
            let Some(bounds) = self.list.bounds_for_item(row) else {
                continue;
            };
            if bounds.bottom() <= position.y {
                continue;
            }
            if bounds.top() > position.y {
                return None;
            }
            let r = self.rows.get(row - 1)?;
            let item = built.items.get(r.item)?;
            if r.editor || item.lines.is_empty() || matches!(item.node, Node::Footnotes(_)) {
                return None;
            }
            if !matches!(item.node, Node::List { .. }) {
                return Some(HandleAt {
                    item: r.item,
                    entry: None,
                    inset: None,
                });
            }
            let entries = self.entry_bounds.borrow();
            let drawn = entries.get(&r.item)?;
            let distance = |b: &Bounds<Pixels>| {
                if b.top() <= position.y && position.y < b.bottom() {
                    0.
                } else {
                    f32::from(
                        (b.top() - position.y)
                            .abs()
                            .min((b.bottom() - position.y).abs()),
                    )
                }
            };
            let (line, b) = drawn.iter().min_by(|a, b| {
                (distance(&a.1), f32::from(a.1.size.height))
                    .partial_cmp(&(distance(&b.1), f32::from(b.1.size.height)))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;
            return Some(HandleAt {
                item: r.item,
                entry: Some(*line),
                inset: Some(Bounds::new(
                    gpui::point(b.left() - bounds.left(), b.top() - bounds.top()),
                    b.size,
                )),
            });
        }
        None
    }

    fn handle_start(&self) -> f32 {
        let gutter = self.gutter();
        let pad = f32::from(size::SHEET_PAD);
        if gutter >= pad {
            gutter - pad + 1.
        } else {
            (gutter - HIT).max(0.)
        }
    }

    fn first_line(&self, node: &Node, t: &Tokens) -> f32 {
        match node {
            Node::Heading { level, .. } => f32::from(self.heading_base(*level, t).line_height),
            Node::Paragraph(_) | Node::List { .. } | Node::Quote(_) | Node::Ref(_) => {
                (self.typesetting.base * self.typesetting.leading).round()
            }
            _ => STRIP,
        }
    }

    pub(super) fn row_handle(
        &self,
        row: &Row,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> (Option<AnyElement>, Option<AnyElement>) {
        let Some(built) = &self.built else {
            return (None, None);
        };
        let Some(item) = built.items.get(row.item) else {
            return (None, None);
        };
        if row.part != 0 {
            return (None, None);
        }
        let dragged = self
            .handle_drag
            .as_ref()
            .filter(|d| d.at.item == row.item)
            .map(|d| d.at);
        let background = dragged.map(|at| {
            let (_, bottom) = self.block_insets(&item.node, row);
            let gutter = self.gutter();
            match at.inset {
                Some(inset) => div()
                    .absolute()
                    .left(inset.left())
                    .top(inset.top())
                    .w(inset.size.width)
                    .h(inset.size.height)
                    .bg(t.card_2),
                None => div()
                    .absolute()
                    .left(px(gutter))
                    .right(px(gutter))
                    .top_0()
                    .bottom(px(bottom))
                    .bg(t.card_2),
            }
            .into_any_element()
        });
        let (card, ink) = (t.card_2, t.ink);
        let shown = match (dragged, self.handle_hover) {
            (Some(at), _) => Some(at),
            (None, Some(at)) if at.item == row.item && self.handles_shown() => Some(at),
            _ => None,
        };
        let handle = shown.map(|at| {
            let (top, line) = match at.inset {
                Some(inset) => (
                    f32::from(inset.top()),
                    (self.typesetting.base * self.typesetting.leading).round(),
                ),
                None => (
                    self.block_insets(&item.node, row).0,
                    self.first_line(&item.node, t),
                ),
            };
            let held = dragged.is_some();
            let pointed = held || self.handle_pointed;
            let place = super::blocks::dir_of(std::slice::from_ref(&item.node))
                .start(div().absolute(), px(self.handle_start()));
            place
                .id(("block-handle", row.item))
                .group("block-handle")
                .top(px(top + (line - HIT) / 2.))
                .size(px(HIT))
                .flex()
                .items_center()
                .justify_center()
                .cursor(if held {
                    CursorStyle::ClosedHand
                } else {
                    CursorStyle::OpenHand
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.press_handle(at, window, cx);
                    }),
                )
                .child(
                    div()
                        .size(px(SQUARE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(size::RADIUS)
                        .text_color(if pointed { t.ink } else { t.ink_3 })
                        .when(pointed, |this| this.bg(t.card_2))
                        .group_hover("block-handle", move |s| s.bg(card).text_color(ink))
                        .child(icon(Icon::GRIP_VERTICAL, size::ICON_DENSE)),
                )
                .into_any_element()
        });
        (background, handle)
    }

    fn press_handle(&mut self, at: HandleAt, window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.in_place.clone() else {
            return;
        };
        let Some(built) = self.built.clone() else {
            return;
        };
        let text = field.read(cx).text().to_string();
        let outline = blockmove::outline(&text);
        let found = match at.entry {
            Some(line) => outline
                .item_starting(line)
                .map(|(group, from)| (group.clone(), from)),
            None => {
                let mut group = outline.top.clone();
                for heading in built.headings.iter().filter(|h| !h.inner) {
                    if !self.folded.contains(&heading.item) {
                        continue;
                    }
                    let index = built.headings.iter().position(|h| std::ptr::eq(h, heading));
                    let end = index.map_or(heading.item + 1, |i| built.section_end[i]);
                    let last = built.items[heading.item..end.min(built.items.len())]
                        .iter()
                        .rev()
                        .find(|i| !i.lines.is_empty());
                    if let (Some(first), Some(last)) = (built.items.get(heading.item), last) {
                        group.merge(first.lines.start..last.lines.end);
                    }
                }
                let start = built.items.get(at.item).map(|i| i.lines.start);
                start
                    .and_then(|line| group.starting(line))
                    .map(|from| (group, from))
            }
        };
        let Some((group, from)) = found else {
            return;
        };
        window.focus(&self.focus);
        self.handle_hover = Some(at);
        self.handle_drag = Some(HandleDrag {
            at,
            group,
            from,
            text,
            slot: None,
            tried: Vec::new(),
        });
        cx.notify();
    }

    fn places(&self, drag: &HandleDrag) -> Vec<(usize, Pixels)> {
        let Some(built) = &self.built else {
            return Vec::new();
        };
        let units = &drag.group.units;
        let mut out = Vec::new();
        if drag.group.list {
            let entries = self.entry_bounds.borrow();
            let Some(drawn) = entries.get(&drag.at.item) else {
                return out;
            };
            let bounds_of = |line: u32| drawn.iter().find(|(l, _)| *l == line).map(|(_, b)| *b);
            for (slot, unit) in units.iter().enumerate() {
                if let Some(b) = bounds_of(unit.start) {
                    out.push((slot, b.top() - px(1.)));
                }
            }
            if let Some(b) = units.last().and_then(|u| bounds_of(u.start)) {
                out.push((units.len(), b.bottom() + px(1.)));
            }
            return out;
        }
        let gap = px((self.typesetting.base * 0.8).round());
        let row_of_line = |line: u32| -> Option<Bounds<Pixels>> {
            let item = built.items.iter().position(|i| i.lines.start == line)?;
            let row = self.rows.iter().position(|r| r.item == item && !r.editor)?;
            self.list.bounds_for_item(row + 1)
        };
        for (slot, unit) in units.iter().enumerate() {
            if let Some(b) = row_of_line(unit.start) {
                out.push((slot, b.top() - gap / 2.));
            }
        }
        let last_row = units.last().and_then(|u| {
            self.rows.iter().rposition(|r| {
                !r.editor
                    && built
                        .items
                        .get(r.item)
                        .is_some_and(|i| u.start <= i.lines.start && i.lines.start < u.end)
            })
        });
        if let Some(row) = last_row
            && let Some(b) = self.list.bounds_for_item(row + 1)
        {
            out.push((units.len(), b.bottom() - gap / 2.));
        }
        out
    }

    pub(super) fn drag_handle(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.handle_drag.as_ref() else {
            return;
        };
        let nearest = self
            .places(drag)
            .into_iter()
            .min_by(|a, b| {
                f32::from((a.1 - position.y).abs()).total_cmp(&f32::from((b.1 - position.y).abs()))
            })
            .map(|(slot, _)| slot);
        let Some(drag) = self.handle_drag.as_mut() else {
            return;
        };
        let slot = nearest.filter(|&slot| {
            if slot == drag.from || slot == drag.from + 1 {
                return false;
            }
            if let Some((_, ok)) = drag.tried.iter().find(|(s, _)| *s == slot) {
                return *ok;
            }
            let ok = blockmove::moved(&drag.text, &drag.group, drag.from, slot).is_some();
            drag.tried.push((slot, ok));
            ok
        });
        if slot != drag.slot {
            drag.slot = slot;
            cx.notify();
        }
    }

    pub(super) fn drop_handle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.handle_drag.take() else {
            return;
        };
        self.handle_hover = None;
        cx.notify();
        let (Some(slot), Some(field)) = (drag.slot, self.in_place.clone()) else {
            return;
        };
        let text = field.read(cx).text().to_string();
        if text != drag.text {
            return;
        }
        let Some(moved) = blockmove::moved(&text, &drag.group, drag.from, slot) else {
            return;
        };
        self.folds_after_move = Some(self.fold_state());
        self.pending_folds = self.folds_after_move.clone();
        self.start_editing(window, cx);
        self.apply(
            Edit {
                range: moved.range,
                text: moved.text,
                caret: moved.start + moved.len,
            },
            window,
            cx,
        );
    }

    pub(super) fn cancel_handle(&mut self, cx: &mut Context<Self>) -> bool {
        if self.handle_drag.take().is_some() {
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn landing_line(&self, t: &Tokens) -> Option<AnyElement> {
        let drag = self.handle_drag.as_ref()?;
        let slot = drag.slot?;
        let (_, y) = self.places(drag).into_iter().find(|(s, _)| *s == slot)?;
        let viewport = self.list.viewport_bounds();
        let gutter = px(self.gutter());
        let (left, right) = match (focus_kit::Dir::current().rtl, drag.at.inset) {
            (false, Some(inset)) => (inset.left(), gutter),
            (true, Some(inset)) => (gutter, viewport.size.width - inset.right()),
            (_, None) => (gutter, gutter),
        };
        Some(
            div()
                .absolute()
                .left(left)
                .right(right)
                .top(y - viewport.top() - px(1.))
                .h(px(2.))
                .bg(t.ink)
                .into_any_element(),
        )
    }

    pub(super) fn handle_listeners(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dragging = self.handle_drag.is_some();
        if !dragging && self.handle_hover.is_none() {
            return None;
        }
        let entity = cx.entity().downgrade();
        let area = self.list.viewport_bounds();
        Some(
            gpui::canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    let moved = entity.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase != gpui::DispatchPhase::Bubble {
                            return;
                        }
                        let _ = moved.update(cx, |this, cx| {
                            if dragging {
                                this.drag_handle(event.position, cx);
                            } else if !area.contains(&event.position) && this.handle_hover.is_some()
                            {
                                this.handle_hover = None;
                                cx.notify();
                            }
                        });
                    });
                    if dragging {
                        let released = entity.clone();
                        window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                            if phase == gpui::DispatchPhase::Bubble {
                                let _ =
                                    released.update(cx, |this, cx| this.drop_handle(window, cx));
                            }
                        });
                    }
                },
            )
            .absolute()
            .size_0()
            .into_any_element(),
        )
    }

    pub(super) fn handle_dragged(&self) -> bool {
        self.handle_drag.is_some()
    }

    pub fn script_press_handle(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.hover_handle(at, cx);
        let target = self.handle_hover.ok_or(
            "no block handle shows there (setting off, nothing editable in place, or a selection)",
        )?;
        self.press_handle(target, window, cx);
        if self.handle_drag.is_none() {
            return Err("the block of that handle cannot move".into());
        }
        Ok(())
    }

    pub fn script_point_handle(
        &mut self,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.hover_handle(at, cx);
        if self.handle_hover.is_none() {
            return Err("no block handle shows there".into());
        }
        self.handle_pointed = true;
        cx.notify();
        Ok(())
    }

    pub fn script_drag_handle(
        &mut self,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.handle_drag.is_none() {
            return Err("no block handle is held".into());
        }
        self.drag_handle(at, cx);
        Ok(())
    }

    pub fn script_drop_handle(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.handle_drag.is_none() {
            return Err("no block handle is held".into());
        }
        self.drop_handle(window, cx);
        Ok(())
    }

    pub fn move_caret_block(
        &mut self,
        down: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.caret_shown() || !self.live {
            return false;
        }
        let Some(field) = self.in_place.clone() else {
            return false;
        };
        let Some(built) = self.built.clone() else {
            return false;
        };
        let (text, caret) = {
            let field = field.read(cx);
            (field.text().to_string(), field.cursor())
        };
        let line = text[..caret].matches('\n').count() as u32;
        let outline = blockmove::outline(&text);
        let in_list = outline
            .lists
            .iter()
            .any(|group| group.holding(line).is_some());
        let moved = if in_list {
            blockmove::step(&text, &outline, line, down)
        } else {
            let mut group = outline.top.clone();
            for (index, heading) in built.headings.iter().enumerate() {
                if heading.inner || !self.folded.contains(&heading.item) {
                    continue;
                }
                let end = built.section_end[index].min(built.items.len());
                let last = built.items[heading.item..end]
                    .iter()
                    .rev()
                    .find(|i| !i.lines.is_empty());
                if let (Some(first), Some(last)) = (built.items.get(heading.item), last) {
                    group.merge(first.lines.start..last.lines.end);
                }
            }
            group
                .holding(line)
                .and_then(|at| blockmove::step_in(&text, &group, at, down))
        };
        let Some(moved) = moved else {
            return false;
        };
        let offset = caret.saturating_sub(moved.was);
        self.folds_after_move = Some(self.fold_state());
        self.pending_folds = self.folds_after_move.clone();
        self.apply(
            Edit {
                range: moved.range.clone(),
                text: moved.text.clone(),
                caret: (moved.start + offset).min(moved.start + moved.len),
            },
            window,
            cx,
        );
        true
    }
}
