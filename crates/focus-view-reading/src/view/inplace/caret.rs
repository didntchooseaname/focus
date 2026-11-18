use gpui::{Context, Pixels, Window, point, px};
use gpui_component::{Rope, RopeExt as _};

use super::{At, Edit};
use crate::model::Node;
use crate::view::ReadingView;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Vertical {
    pub down: bool,
    pub select: bool,
    pub tries: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Move {
    Left,
    Right,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    NoteStart,
    NoteEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Stop {
    pub byte: usize,
    pub key_item: u32,
    pub ord: u32,
}

fn next_boundary(text: &Rope, byte: usize) -> usize {
    (byte + text.char_at(byte).map_or(1, char::len_utf8)).min(text.len())
}

fn previous_boundary(text: &Rope, byte: usize) -> usize {
    let mut at = byte.saturating_sub(1);
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn order(a: Pixels, b: Pixels) -> std::cmp::Ordering {
    f32::from(a).total_cmp(&f32::from(b))
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn word_edge(text: &Rope, byte: usize, forward: bool) -> usize {
    let mut at = byte;
    let char_at = |at: usize, forward: bool| {
        if forward {
            text.char_at(at)
        } else {
            (at > 0)
                .then(|| text.char_at(previous_boundary(text, at)))
                .flatten()
        }
    };
    let step = |at: usize, forward: bool| {
        if forward {
            next_boundary(text, at)
        } else {
            previous_boundary(text, at)
        }
    };
    while let Some(c) = char_at(at, forward)
        && !word_char(c)
        && c != '\n'
    {
        at = step(at, forward);
    }
    while let Some(c) = char_at(at, forward)
        && word_char(c)
    {
        at = step(at, forward);
    }
    if at == byte && char_at(at, forward).is_some() {
        at = step(at, forward);
    }
    at
}

fn joins(node: &Node) -> bool {
    matches!(node, Node::Paragraph(_) | Node::Heading { .. })
}

impl ReadingView {
    pub(super) fn stop_from(&self, text: &Rope, byte: usize, forward: bool) -> Option<Stop> {
        let count = self.built.as_ref()?.items.len() as u32;
        let mut key_item = self.key_item_at(byte, text);
        loop {
            let origin = self.origin(key_item, text)?;
            let mut best: Option<Stop> = None;
            for (rich, map) in self.mapped_texts(key_item) {
                for range in map.stops() {
                    let (start, end) = (origin + range.start, origin + range.end);
                    let candidate = if forward {
                        if start > byte {
                            Some(start)
                        } else if byte < end {
                            Some(next_boundary(text, byte).min(end))
                        } else {
                            None
                        }
                    } else if end < byte {
                        Some(end)
                    } else if start < byte {
                        Some(previous_boundary(text, byte).max(start))
                    } else {
                        None
                    };
                    let Some(candidate) = candidate else { continue };
                    let better = best.is_none_or(|b| {
                        if forward {
                            candidate < b.byte
                        } else {
                            candidate > b.byte
                        }
                    });
                    if better {
                        best = Some(Stop {
                            byte: candidate,
                            key_item,
                            ord: rich.ord,
                        });
                    }
                }
            }
            if best.is_some() {
                return best;
            }
            if forward {
                key_item += 1;
                if key_item > count {
                    return None;
                }
            } else {
                key_item = key_item.checked_sub(1)?;
            }
        }
    }

    pub(super) fn stop_at(&self, text: &Rope, byte: usize) -> Option<Stop> {
        let key_item = self.key_item_at(byte, text);
        let origin = self.origin(key_item, text)?;
        let relative = byte.checked_sub(origin)?;
        self.mapped_texts(key_item)
            .into_iter()
            .find_map(|(rich, map)| {
                map.stops()
                    .iter()
                    .any(|r| r.start <= relative && relative <= r.end)
                    .then_some(Stop {
                        byte,
                        key_item,
                        ord: rich.ord,
                    })
            })
    }

    pub(super) fn join(&self, at: &At, forward: bool) -> Option<Edit> {
        let here = self.stop_at(at.text, at.caret)?;
        let (from, other) = self.past_markup(at, at.caret, forward)?;
        let range = from.min(other.byte)..from.max(other.byte);
        let delete = Edit {
            range: range.clone(),
            text: String::new(),
            caret: range.start,
        };
        let only_move = Edit {
            range: at.caret..at.caret,
            text: String::new(),
            caret: other.byte,
        };
        if other.key_item == here.key_item {
            return Some(if other.ord == here.ord {
                delete
            } else {
                only_move
            });
        }
        let built = self.built.as_ref()?;
        let node = |key: u32| {
            key.checked_sub(1)
                .and_then(|i| built.items.get(i as usize))
                .map(|item| &item.node)
        };
        let both_text =
            node(here.key_item).is_some_and(joins) && node(other.key_item).is_some_and(joins);
        Some(if both_text { delete } else { only_move })
    }

    fn past_markup(&self, at: &At, caret: usize, forward: bool) -> Option<(usize, Stop)> {
        let markup: Vec<std::ops::Range<usize>> = at
            .map
            .iter()
            .flat_map(|map| map.markup.iter())
            .filter(|m| !m.line)
            .flat_map(|m| m.inserts.iter())
            .filter(|(_, r)| !r.is_empty())
            .map(|(_, r)| at.start + r.start..at.start + r.end)
            .collect();
        let mut from = caret;
        let mut other = self.stop_from(at.text, from, forward)?;
        for _ in 0..8 {
            let edge = markup.iter().find_map(|r| {
                if forward {
                    (r.start == from && other.byte > r.start && other.byte <= r.end)
                        .then_some(r.end)
                } else {
                    (r.end == from && other.byte < r.end && other.byte >= r.start)
                        .then_some(r.start)
                }
            });
            let Some(edge) = edge else { break };
            from = edge;
            other = self.stop_from(at.text, from, forward)?;
        }
        Some((from, other))
    }

    pub(super) fn moved(
        &self,
        text: &Rope,
        anchor: usize,
        head: usize,
        mv: Move,
        select: bool,
    ) -> Option<usize> {
        let collapsed = anchor == head || select;
        match mv {
            Move::Left if !collapsed => Some(anchor.min(head)),
            Move::Right if !collapsed => Some(anchor.max(head)),
            Move::Left => self.stop_from(text, head, false).map(|s| s.byte),
            Move::Right => self.stop_from(text, head, true).map(|s| s.byte),
            Move::WordLeft | Move::WordRight => {
                let forward = mv == Move::WordRight;
                let target = word_edge(text, head, forward);
                if self.stop_at(text, target).is_some() {
                    Some(target)
                } else {
                    let back = if forward {
                        previous_boundary(text, target)
                    } else {
                        next_boundary(text, target)
                    };
                    self.stop_from(text, back, forward).map(|s| s.byte)
                }
            }
            Move::LineStart | Move::LineEnd => self.line_edge(head, mv == Move::LineEnd),
            Move::NoteStart => self.stop_from(text, 0, true).map(|s| s.byte).or(Some(0)),
            Move::NoteEnd => self.stop_from(text, text.len(), false).map(|s| s.byte),
        }
    }

    fn line_edge(&self, head: usize, end: bool) -> Option<usize> {
        let text = &self.in_place_frame.as_ref()?.text;
        let (at, line) = super::caret_point(self, head)?;
        let hits = self.registry.0.borrow();
        let hit = hits.iter().find(|h| {
            h.map.is_some()
                && h.bounds.top() <= at.y + px(0.5)
                && at.y + line <= h.bounds.bottom() + px(0.5)
                && h.bounds.left() <= at.x + px(0.5)
                && at.x <= h.bounds.right() + px(0.5)
        })?;
        let map = hit.map.as_ref()?;
        let y = at.y + line / 2.;
        let left = hit
            .layout
            .closest_index_for_position(point(hit.bounds.left() - px(1.), y));
        let right = hit
            .layout
            .closest_index_for_position(point(hit.bounds.right() + px(1.), y));
        let shown = if end {
            left.max(right)
        } else {
            left.min(right)
        };
        Some(self.origin(hit.key.0, text)? + map.to_source(shown)?)
    }

    pub(super) fn vertical_target(&self, head: usize, down: bool, goal_x: Pixels) -> Option<usize> {
        let text = &self.in_place_frame.as_ref()?.text;
        let (at, line) = super::caret_point(self, head)?;
        let hits = self.registry.0.borrow();
        let mut lines = Vec::new();
        for (n, hit) in hits.iter().enumerate() {
            if hit.map.is_none() {
                continue;
            }
            let height = hit.layout.line_height();
            let count = (hit.bounds.size.height / height).round().max(1.) as usize;
            for k in 0..count {
                let top = hit.bounds.top() + height * k as f32;
                lines.push((top, top + height, n));
            }
        }
        let candidates = lines.iter().filter(|(top, bottom, _)| {
            if down {
                *top >= at.y + line - px(1.)
            } else {
                *bottom <= at.y + px(1.)
            }
        });
        let nearest = if down {
            candidates
                .clone()
                .map(|(top, _, _)| *top)
                .min_by(|a, b| order(*a, *b))
        } else {
            candidates
                .clone()
                .map(|(_, bottom, _)| *bottom)
                .max_by(|a, b| order(*a, *b))
        }?;
        let (top, bottom, n) = candidates
            .filter(|(top, bottom, _)| {
                if down {
                    (*top - nearest).abs() < px(2.)
                } else {
                    (*bottom - nearest).abs() < px(2.)
                }
            })
            .min_by(|a, b| {
                let distance = |n: usize| {
                    let bounds = hits[n].bounds;
                    if goal_x < bounds.left() {
                        bounds.left() - goal_x
                    } else if goal_x > bounds.right() {
                        goal_x - bounds.right()
                    } else {
                        px(0.)
                    }
                };
                order(distance(a.2), distance(b.2))
            })
            .copied()?;
        let hit = &hits[n];
        let x = goal_x.max(hit.bounds.left()).min(hit.bounds.right());
        let shown = hit
            .layout
            .closest_index_for_position(point(x, (top + bottom) / 2.));
        Some(self.origin(hit.key.0, text)? + hit.map.as_ref()?.to_source(shown)?)
    }

    pub(super) fn vertical(&mut self, mv: Vertical, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, anchor, head)) = self.field_state(window, cx) else {
            return;
        };
        let Some((at, line)) = super::caret_point(self, head) else {
            if let Some(editing) = &mut self.editing {
                editing.reveal = true;
                editing.pending = (mv.tries < 4).then_some(Vertical {
                    tries: mv.tries + 1,
                    ..mv
                });
            }
            cx.notify();
            return;
        };
        let goal_x = self.editing.as_ref().and_then(|e| e.goal_x).unwrap_or(at.x);
        match self.vertical_target(head, mv.down, goal_x) {
            Some(target) => {
                self.select(if mv.select { anchor } else { target }, target, cx);
                if let Some(editing) = &mut self.editing {
                    editing.goal_x = Some(goal_x);
                }
            }
            None if mv.tries < 2 => {
                self.list.scroll_by(if mv.down { line } else { -line });
                if let Some(editing) = &mut self.editing {
                    editing.goal_x = Some(goal_x);
                    editing.pending = Some(Vertical {
                        tries: mv.tries + 1,
                        ..mv
                    });
                }
                cx.notify();
            }
            None => {
                let edge = self.line_edge(head, mv.down).unwrap_or(head);
                self.select(if mv.select { anchor } else { edge }, edge, cx);
            }
        }
    }

    pub(super) fn resume_vertical(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pending = self.editing.as_mut().and_then(|e| e.pending.take());
        if let Some(mv) = pending {
            self.vertical(mv, window, cx);
        }
    }
}
