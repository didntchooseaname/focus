use std::ops::Range;
use std::sync::Arc;

use crate::model::{Marks, Rich};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub shown: Range<usize>,
    pub source: Range<usize>,
    pub exact: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Markup {
    pub element: Range<usize>,
    pub inserts: Vec<(usize, Range<usize>)>,
    pub hides: Option<Range<usize>>,
    pub line: bool,
    pub value: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceMap {
    pub pieces: Vec<Piece>,
    pub markup: Vec<Markup>,
}

pub fn of(rich: &Rich) -> Option<Arc<SourceMap>> {
    rich.map.clone()
}

impl SourceMap {
    pub fn to_source(&self, shown: usize) -> Option<usize> {
        let piece = self
            .pieces
            .iter()
            .find(|p| p.shown.start <= shown && shown < p.shown.end)
            .or_else(|| self.pieces.iter().rev().find(|p| p.shown.end <= shown))
            .or_else(|| self.pieces.first())?;
        let shown = shown.clamp(piece.shown.start, piece.shown.end);
        Some(if piece.exact {
            piece.source.start + (shown - piece.shown.start)
        } else if shown - piece.shown.start <= (piece.shown.end - shown) {
            piece.source.start
        } else {
            piece.source.end
        })
    }

    pub fn to_shown(&self, source: usize) -> Option<usize> {
        self.pieces.iter().find_map(|p| {
            if !(p.source.start <= source && source <= p.source.end) {
                None
            } else if p.exact {
                Some(p.shown.start + (source - p.source.start))
            } else if source == p.source.start {
                Some(p.shown.start)
            } else if source == p.source.end {
                Some(p.shown.end)
            } else {
                None
            }
        })
    }

    pub fn span(&self) -> Option<Range<usize>> {
        let starts = self
            .pieces
            .iter()
            .map(|p| p.source.start)
            .chain(self.markup.iter().map(|m| m.element.start));
        let ends = self
            .pieces
            .iter()
            .map(|p| p.source.end)
            .chain(self.markup.iter().map(|m| m.element.end));
        Some(starts.min()?..ends.max()?)
    }

    pub fn stops(&self) -> Vec<Range<usize>> {
        let mut stops: Vec<Range<usize>> = Vec::new();
        for piece in &self.pieces {
            if piece.exact {
                stops.push(piece.source.clone());
            } else {
                stops.push(piece.source.start..piece.source.start);
                stops.push(piece.source.end..piece.source.end);
            }
        }
        for markup in &self.markup {
            stops.extend(markup.inserts.iter().map(|(_, r)| r.clone()));
        }
        stops.sort_by_key(|r| (r.start, r.end));
        stops
    }

    pub fn shown_range(&self, range: &Range<usize>) -> Option<Range<usize>> {
        let mut start: Option<usize> = None;
        let mut end: Option<usize> = None;
        for p in &self.pieces {
            if p.source.end < range.start || p.source.start > range.end {
                continue;
            }
            let from = if p.exact {
                p.shown.start
                    + range
                        .start
                        .saturating_sub(p.source.start)
                        .min(p.shown.len())
            } else if range.start <= p.source.start {
                p.shown.start
            } else {
                p.shown.end
            };
            let to = if p.exact {
                p.shown.start + range.end.saturating_sub(p.source.start).min(p.shown.len())
            } else if range.end >= p.source.end {
                p.shown.end
            } else {
                p.shown.start
            };
            start = Some(start.map_or(from, |s| s.min(from)));
            end = Some(end.map_or(to, |e| e.max(to)));
        }
        let (start, end) = (start?, end?);
        (start < end).then_some(start..end)
    }

    pub fn holding(&self, source: usize) -> impl Iterator<Item = &Markup> {
        self.markup
            .iter()
            .filter(move |m| m.element.start <= source && source <= m.element.end)
    }
}

pub fn slice(map: &SourceMap, range: Range<usize>) -> SourceMap {
    let shift = |o: usize| o - range.start;
    let pieces = map
        .pieces
        .iter()
        .filter_map(|p| {
            let (a, b) = (p.shown.start.max(range.start), p.shown.end.min(range.end));
            if a > b || (a == b && !p.shown.is_empty()) {
                return None;
            }
            if p.exact {
                Some(Piece {
                    shown: shift(a)..shift(b),
                    source: p.source.start + (a - p.shown.start)
                        ..p.source.start + (b - p.shown.start),
                    exact: true,
                })
            } else {
                (p.shown.start >= range.start && p.shown.end <= range.end).then(|| Piece {
                    shown: shift(a)..shift(b),
                    ..p.clone()
                })
            }
        })
        .collect();
    let markup = map
        .markup
        .iter()
        .filter(|m| {
            m.inserts
                .iter()
                .all(|(at, _)| range.contains(at) || *at == range.end)
        })
        .map(|m| Markup {
            element: m.element.clone(),
            inserts: m
                .inserts
                .iter()
                .map(|(at, r)| (shift(*at), r.clone()))
                .collect(),
            hides: m.hides.as_ref().map(|h| shift(h.start)..shift(h.end)),
            line: m.line,
            value: m.value,
        })
        .collect();
    SourceMap { pieces, markup }
}

pub fn reveal(
    rich: &Rich,
    map: &SourceMap,
    caret: usize,
    source: &str,
) -> Option<(Rich, SourceMap)> {
    let held: Vec<&Markup> = map.holding(caret).collect();
    if held.is_empty() {
        return None;
    }
    let mut edits: Vec<(usize, usize, Range<usize>, bool)> = Vec::new();
    for markup in &held {
        if let Some(hidden) = &markup.hides {
            edits.push((hidden.start, hidden.end, 0..0, markup.value));
        }
        for (at, bytes) in &markup.inserts {
            if bytes.end <= source.len()
                && source.is_char_boundary(bytes.start)
                && source.is_char_boundary(bytes.end)
            {
                edits.push((*at, *at, bytes.clone(), markup.value));
            }
        }
    }
    edits.sort_by_key(|(at, end, bytes, _)| (*at, *end, bytes.start));
    let mut text = String::with_capacity(rich.text.len() + 16);
    let mut marks: Vec<(Range<usize>, Marks)> = Vec::new();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut kept: Vec<(Range<usize>, usize)> = Vec::new();
    let copy = |from: Range<usize>,
                text: &mut String,
                marks: &mut Vec<(Range<usize>, Marks)>,
                pieces: &mut Vec<Piece>,
                kept: &mut Vec<(Range<usize>, usize)>| {
        if from.start >= from.end {
            return;
        }
        let at = text.len();
        text.push_str(&rich.text[from.clone()]);
        kept.push((from.clone(), at));
        let place = |o: usize| at + o - from.start;
        for (r, m) in &rich.marks {
            let (a, b) = (r.start.max(from.start), r.end.min(from.end));
            if a < b {
                marks.push((place(a)..place(b), *m));
            }
        }
        for p in &map.pieces {
            let (a, b) = (p.shown.start.max(from.start), p.shown.end.min(from.end));
            if a > b || (a == b && !p.shown.is_empty()) {
                continue;
            }
            if p.exact {
                pieces.push(Piece {
                    shown: place(a)..place(b),
                    source: p.source.start + (a - p.shown.start)
                        ..p.source.start + (b - p.shown.start),
                    exact: true,
                });
            } else if p.shown.start >= from.start && p.shown.end <= from.end {
                pieces.push(Piece {
                    shown: place(a)..place(b),
                    ..p.clone()
                });
            }
        }
    };
    let mut at = 0;
    for (start, end, bytes, value) in &edits {
        if *start < at {
            continue;
        }
        copy(at..*start, &mut text, &mut marks, &mut pieces, &mut kept);
        if !bytes.is_empty() {
            let from = text.len();
            text.push_str(&source[bytes.clone()]);
            marks.push((
                from..text.len(),
                Marks {
                    muted: !value,
                    mono: *value,
                    ..Marks::default()
                },
            ));
            pieces.push(Piece {
                shown: from..text.len(),
                source: bytes.clone(),
                exact: true,
            });
        }
        at = *end;
    }
    copy(
        at..rich.text.len(),
        &mut text,
        &mut marks,
        &mut pieces,
        &mut kept,
    );
    let start_of = |o: usize| {
        kept.iter()
            .find(|(r, _)| r.start <= o && o < r.end)
            .map(|(r, at)| at + o - r.start)
    };
    let end_of = |o: usize| {
        kept.iter()
            .find(|(r, _)| r.start < o && o <= r.end)
            .map(|(r, at)| at + o - r.start)
    };
    let links = rich
        .links
        .iter()
        .filter_map(|link| {
            Some(crate::model::LinkSpan {
                range: start_of(link.range.start)?..end_of(link.range.end)?,
                goal: link.goal.clone(),
            })
        })
        .collect();
    marks.sort_by_key(|(r, _)| r.start);
    pieces.sort_by_key(|p| (p.shown.start, p.shown.end));
    Some((
        Rich {
            text,
            marks,
            links,
            ..rich.clone()
        },
        SourceMap {
            pieces,
            markup: map.markup.clone(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact(shown: Range<usize>, source: Range<usize>) -> Piece {
        Piece {
            shown,
            source,
            exact: true,
        }
    }

    fn bold() -> (Rich, SourceMap, &'static str) {
        let source = "**bold** and text";
        let rich = Rich {
            text: "bold and text".into(),
            marks: vec![(
                0..4,
                Marks {
                    strong: true,
                    ..Marks::default()
                },
            )],
            ..Rich::default()
        };
        let map = SourceMap {
            pieces: vec![exact(0..4, 2..6), exact(4..13, 8..17)],
            markup: vec![Markup {
                element: 0..8,
                inserts: vec![(0, 0..2), (4, 6..8)],
                hides: None,
                line: false,
                value: false,
            }],
        };
        (rich, map, source)
    }

    #[test]
    fn shown_and_source_offsets_meet_in_exact_pieces() {
        let (_, map, _) = bold();
        assert_eq!(map.to_source(1), Some(3));
        assert_eq!(map.to_source(4), Some(8), "the start of the second piece");
        assert_eq!(map.to_source(13), Some(17));
        assert_eq!(map.to_shown(3), Some(1));
        assert_eq!(map.to_shown(6), Some(4), "the end of the bold run");
        assert_eq!(map.to_shown(1), None, "inside the markup, not revealed");
        assert_eq!(map.span(), Some(0..17));
        assert_eq!(map.shown_range(&(3..10)), Some(1..6));
        assert_eq!(map.shown_range(&(0..2)), None);
    }

    #[test]
    fn an_atom_takes_the_caret_at_its_ends_only() {
        let map = SourceMap {
            pieces: vec![
                exact(0..1, 0..1),
                Piece {
                    shown: 1..4,
                    source: 1..5,
                    exact: false,
                },
            ],
            markup: Vec::new(),
        };
        assert_eq!(map.to_source(2), Some(1), "nearer its start");
        assert_eq!(map.to_source(3), Some(5), "nearer its end");
        assert_eq!(map.to_shown(3), None);
        assert_eq!(map.to_shown(5), Some(4));
        assert_eq!(map.stops(), vec![0..1, 1..1, 5..5]);
    }

    #[test]
    fn the_markup_of_the_element_holding_the_caret_shows() {
        let (rich, map, source) = bold();
        let (shown, shown_map) =
            reveal(&rich, &map, 3, source).expect("the caret is in the bold run");
        assert_eq!(shown.text, "**bold** and text");
        let muted: Vec<_> = shown
            .marks
            .iter()
            .filter(|(_, m)| m.muted)
            .map(|(r, _)| r.clone())
            .collect();
        assert_eq!(muted, vec![0..2, 6..8]);
        assert!(shown.marks.iter().any(|(r, m)| *r == (2..6) && m.strong));
        assert_eq!(shown_map.to_source(1), Some(1));
        assert_eq!(shown_map.to_source(7), Some(7));
        assert_eq!(shown_map.to_shown(3), Some(3));
        assert_eq!(shown_map.to_shown(12), Some(12));
        assert!(
            reveal(&rich, &map, 12, source).is_none(),
            "the caret in plain text"
        );
    }

    #[test]
    fn a_part_of_a_text_keeps_its_bytes() {
        let map = SourceMap {
            pieces: vec![exact(0..5, 10..15)],
            markup: Vec::new(),
        };
        let part = slice(&map, 3..5);
        assert_eq!(part.pieces, vec![exact(0..2, 13..15)]);
        assert_eq!(part.to_source(1), Some(14));
        assert_eq!(part.to_shown(10), None, "before the part");
    }

    #[test]
    fn a_revealed_atom_shows_its_source_in_its_place() {
        let source = "a[^1] b";
        let rich = Rich {
            text: "a[1] b".into(),
            ..Rich::default()
        };
        let map = SourceMap {
            pieces: vec![
                exact(0..1, 0..1),
                Piece {
                    shown: 1..4,
                    source: 1..5,
                    exact: false,
                },
                exact(4..6, 5..7),
            ],
            markup: vec![Markup {
                element: 1..5,
                inserts: vec![(1, 1..5)],
                hides: Some(1..4),
                line: false,
                value: false,
            }],
        };
        let (shown, shown_map) =
            reveal(&rich, &map, 5, source).expect("the caret ends the reference");
        assert_eq!(shown.text, "a[^1] b");
        assert_eq!(shown_map.to_source(3), Some(3));
        assert_eq!(shown_map.to_shown(6), Some(6));
    }
}
