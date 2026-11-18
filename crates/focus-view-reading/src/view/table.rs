use std::rc::Rc;

use focus_kit::Tokens;
use gpui::{FontWeight, Window, px};

use super::ReadingView;
use crate::model::{BaseSection, Node, Rich};
use crate::text::{Base, runs};

pub(super) const CELL_PAD: f32 = 24.;
const SCROLLED_COLUMN: f32 = 280.;

pub(super) struct Columns {
    pub min: Vec<f32>,
    pub max: Vec<f32>,
}

impl Columns {
    pub fn min_total(&self) -> f32 {
        self.min.iter().sum()
    }

    pub fn max_total(&self) -> f32 {
        self.max.iter().sum()
    }

    pub fn widths(&self, room: f32) -> Vec<f32> {
        let (least, most) = (self.min_total(), self.max_total());
        if most <= room {
            return self.max.clone();
        }
        if least <= room {
            let share = (room - least) / (most - least).max(1.);
            let mut done = 0.;
            let mut total = 0.;
            return self
                .min
                .iter()
                .zip(&self.max)
                .map(|(min, max)| {
                    total += min + (max - min) * share;
                    let width = total.floor() - done;
                    done += width;
                    width
                })
                .collect();
        }
        self.min
            .iter()
            .zip(&self.max)
            .map(|(min, max)| max.min(min.max(SCROLLED_COLUMN)))
            .collect()
    }
}

fn measure(rich: &Rich, base: &Base, t: &Tokens, window: &Window) -> (f32, f32) {
    if rich.text.is_empty() {
        return (0., 0.);
    }
    let line = rich.text.replace('\n', " ");
    let styles = runs(rich, base, t, None);
    let text_system = window.text_system();
    let layout = text_system.layout_line(&line, base.size, &styles, None);
    let right_to_left = line.chars().any(|c| matches!(c, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}'));
    let mut widest = 0f32;
    let mut start = None;
    for (i, c) in line.char_indices().chain([(line.len(), ' ')]) {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                let width = if right_to_left {
                    text_system
                        .layout_line(&line[s..i], base.size, &slice_runs(&styles, s..i), None)
                        .width
                } else {
                    layout.x_for_index(i) - layout.x_for_index(s)
                };
                widest = widest.max(f32::from(width));
                start = None;
            }
            _ => {}
        }
    }
    for (range, marks) in &rich.marks {
        if marks.mono && !right_to_left && range.end <= line.len() {
            widest = widest.max(f32::from(
                layout.x_for_index(range.end) - layout.x_for_index(range.start),
            ));
        }
    }
    (widest, f32::from(layout.width))
}

fn slice_runs(runs: &[gpui::TextRun], range: std::ops::Range<usize>) -> Vec<gpui::TextRun> {
    let mut out = Vec::new();
    let mut at = 0;
    for run in runs {
        let (start, end) = (at.max(range.start), (at + run.len).min(range.end));
        if start < end {
            out.push(gpui::TextRun {
                len: end - start,
                ..run.clone()
            });
        }
        at += run.len;
    }
    out
}

pub(super) fn first_ord(head: &[Rich], rows: &[Vec<Rich>]) -> u32 {
    head.first()
        .or_else(|| rows.first().and_then(|r| r.first()))
        .map_or(0, |r| r.ord)
}

impl ReadingView {
    pub(super) fn table_bases(&self, base: &Base, t: &Tokens) -> (Base, Base) {
        (
            Base {
                family: self.typesetting.text_font.clone(),
                size: px(11.5),
                line_height: px(16.),
                weight: FontWeight::MEDIUM,
                color: t.ink_3,
                ..base.clone()
            },
            Base {
                size: px((self.typesetting.base - 1.).max(12.)),
                line_height: px(((self.typesetting.base - 1.) * 1.5).round()),
                ..base.clone()
            },
        )
    }

    pub(super) fn share_columns(
        &self,
        item: usize,
        sections: &[BaseSection],
        view: Option<u32>,
        base: &Base,
        t: &Tokens,
        window: &Window,
    ) {
        let built = self.built.clone();
        let tables: Vec<(usize, &BaseSection)> = match (view, &built) {
            (Some(rank), Some(built)) => built
                .items
                .iter()
                .enumerate()
                .filter_map(|(at, entry)| match &entry.node {
                    Node::Base {
                        sections,
                        view: Some(of),
                        ..
                    } if *of == rank => Some(sections.iter().map(move |section| (at, section))),
                    _ => None,
                })
                .flatten()
                .collect(),
            _ => sections.iter().map(|section| (item, section)).collect(),
        };
        if tables.len() < 2 {
            return;
        }
        let (head_base, cell_base) = self.table_bases(base, t);
        let measured: Vec<_> = tables
            .into_iter()
            .filter_map(|(item, section)| match &section.table {
                Node::Table { head, rows, .. } => {
                    let key = (item, first_ord(head, rows));
                    Some((
                        key,
                        self.columns(key, head, rows, &head_base, &cell_base, t, window),
                    ))
                }
                _ => None,
            })
            .collect();
        let Some((_, first)) = measured.first() else {
            return;
        };
        if measured
            .iter()
            .all(|(_, columns)| Rc::ptr_eq(columns, first))
        {
            return;
        }
        let widest = |of: fn(&Columns) -> &Vec<f32>| {
            let mut widths = of(first).clone();
            for (_, columns) in &measured {
                for (width, other) in widths.iter_mut().zip(of(columns)) {
                    *width = width.max(*other);
                }
            }
            widths
        };
        let shared = Rc::new(Columns {
            min: widest(|columns| &columns.min),
            max: widest(|columns| &columns.max),
        });
        let mut tables = self.tables.borrow_mut();
        for (key, _) in measured {
            tables.insert(key, shared.clone());
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn columns(
        &self,
        key: (usize, u32),
        head: &[Rich],
        rows: &[Vec<Rich>],
        head_base: &Base,
        cell_base: &Base,
        t: &Tokens,
        window: &Window,
    ) -> Rc<Columns> {
        if let Some(found) = self.tables.borrow().get(&key) {
            return found.clone();
        }
        let count = head.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
        let mut columns = Columns {
            min: vec![CELL_PAD; count],
            max: vec![CELL_PAD; count],
        };
        let cells = head
            .iter()
            .enumerate()
            .map(|(i, r)| (i, r, head_base))
            .chain(
                rows.iter()
                    .flat_map(|row| row.iter().enumerate().map(|(i, r)| (i, r, cell_base))),
            );
        for (column, rich, base) in cells {
            let (word, line) = measure(rich, base, t, window);
            columns.min[column] = columns.min[column].max(word.ceil() + CELL_PAD + 1.);
            columns.max[column] = columns.max[column].max(line.ceil() + CELL_PAD + 1.);
        }
        let columns = Rc::new(columns);
        self.tables.borrow_mut().insert(key, columns.clone());
        columns
    }
}

#[cfg(test)]
mod tests {
    use super::Columns;

    #[test]
    fn a_shared_table_ends_on_the_edge_of_its_room() {
        let columns = Columns {
            min: vec![61., 87., 75., 113.],
            max: vec![83., 413., 97., 151.],
        };
        let (least, most) = (columns.min_total() as u32, columns.max_total() as u32);
        for room in least..most {
            let widths = columns.widths(room as f32 + 0.6);
            assert_eq!(widths.iter().sum::<f32>(), room as f32, "room {room}");
            for ((width, min), max) in widths.iter().zip(&columns.min).zip(&columns.max) {
                assert!(width.fract() == 0. && min <= width && width <= max);
            }
        }
        assert_eq!(columns.widths(1000.), columns.max);
        assert_eq!(columns.widths(100.), vec![83., 280., 97., 151.]);
    }
}
