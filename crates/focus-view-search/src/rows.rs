use std::collections::{HashMap, HashSet};
use std::ops::Range;

use focus_core::NoteId;
use focus_search::FileHit;
use gpui::SharedString;

use crate::replace::{Exclusions, Plan};

#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Header {
        note: NoteId,
        title: SharedString,
        path: SharedString,
        title_ranges: Vec<Range<usize>>,
        count: u32,
        collapsed: bool,
        cite: Option<usize>,
        keep: Option<bool>,
    },
    Reason {
        note: NoteId,
        text: SharedString,
    },
    Line {
        note: NoteId,
        line: u32,
        text: SharedString,
        ranges: Vec<Range<usize>>,
    },
    Before {
        note: NoteId,
        line: u32,
        text: SharedString,
        ranges: Vec<Range<usize>>,
        keep: bool,
    },
    After {
        note: NoteId,
        line: u32,
        text: SharedString,
        ranges: Vec<Range<usize>>,
        keep: bool,
    },
}

impl Row {
    pub fn note(&self) -> NoteId {
        match self {
            Row::Header { note, .. }
            | Row::Reason { note, .. }
            | Row::Line { note, .. }
            | Row::Before { note, .. }
            | Row::After { note, .. } => *note,
        }
    }

    pub fn target(&self) -> (NoteId, Option<u32>) {
        match self {
            Row::Line { note, line, .. }
            | Row::Before { note, line, .. }
            | Row::After { note, line, .. } => (*note, Some(*line)),
            other => (other.note(), None),
        }
    }

    fn selectable(&self) -> bool {
        !matches!(self, Row::Reason { .. } | Row::After { .. })
    }
}

pub struct Labels<'a> {
    pub title_and_path: &'a dyn Fn(NoteId) -> Option<(SharedString, SharedString)>,
    pub reasons: &'a HashMap<NoteId, SharedString>,
    pub cites: &'a HashMap<NoteId, usize>,
    pub collapsed: &'a HashSet<NoteId>,
}

pub fn flatten(hits: &[FileHit], labels: &Labels) -> Vec<Row> {
    let mut rows = Vec::with_capacity(hits.iter().map(|h| h.matches.len() + 1).sum());
    for hit in hits {
        let Some((title, path)) = (labels.title_and_path)(hit.note) else {
            continue;
        };
        let collapsed = labels.collapsed.contains(&hit.note);
        rows.push(Row::Header {
            note: hit.note,
            title,
            path,
            title_ranges: hit.title_ranges.clone(),
            count: hit.match_count.max(hit.matches.len() as u32),
            collapsed,
            cite: labels.cites.get(&hit.note).copied(),
            keep: None,
        });
        if let Some(text) = labels.reasons.get(&hit.note) {
            rows.push(Row::Reason {
                note: hit.note,
                text: text.clone(),
            });
        }
        if collapsed {
            continue;
        }
        rows.extend(hit.matches.iter().map(|m| Row::Line {
            note: hit.note,
            line: m.line,
            text: m.text.clone().into(),
            ranges: m.ranges.clone(),
        }));
    }
    rows
}

pub fn flatten_plan(plan: &Plan, exclusions: &Exclusions, collapsed: &HashSet<NoteId>) -> Vec<Row> {
    let mut rows = Vec::with_capacity(plan.line_count() * 2 + plan.notes.len());
    for note in &plan.notes {
        let folded = collapsed.contains(&note.note);
        rows.push(Row::Header {
            note: note.note,
            title: note.title.clone().into(),
            path: note.folder.clone().into(),
            title_ranges: Vec::new(),
            count: note.lines.len() as u32,
            collapsed: folded,
            cite: None,
            keep: Some(!exclusions.notes.contains(&note.note)),
        });
        if folded {
            continue;
        }
        for change in &note.lines {
            let keep = exclusions.keeps(note.note, change.line);
            rows.push(Row::Before {
                note: note.note,
                line: change.line,
                text: change.before.clone().into(),
                ranges: change.before_ranges.clone(),
                keep,
            });
            rows.push(Row::After {
                note: note.note,
                line: change.line,
                text: change.after.clone().into(),
                ranges: change.after_ranges.clone(),
                keep,
            });
        }
    }
    rows
}

pub fn step(rows: &[Row], from: Option<usize>, delta: isize) -> Option<usize> {
    let mut at = match from {
        Some(i) => i as isize,
        None if delta > 0 => -1,
        None => rows.len() as isize,
    };
    loop {
        at += delta.signum();
        if at < 0 || at >= rows.len() as isize {
            return from
                .filter(|&i| i < rows.len())
                .or(rows.iter().position(Row::selectable));
        }
        if rows[at as usize].selectable() {
            return Some(at as usize);
        }
    }
}

pub fn jump(rows: &[Row], from: Option<usize>, count: isize) -> Option<usize> {
    let mut at = from;
    for _ in 0..count.unsigned_abs() {
        match step(rows, at, count.signum()) {
            Some(next) if Some(next) != at => at = Some(next),
            other => return other.or(at),
        }
    }
    at
}

pub fn end(rows: &[Row], last: bool) -> Option<usize> {
    if last {
        rows.iter().rposition(Row::selectable)
    } else {
        rows.iter().position(Row::selectable)
    }
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use focus_search::LineMatch;

    use super::*;

    fn hit(note: NoteId, lines: &[u32], total: u32) -> FileHit {
        FileHit {
            note,
            score: 1.0,
            title_ranges: vec![],
            matches: lines
                .iter()
                .map(|&line| LineMatch {
                    line,
                    text: format!("text {line}"),
                    ranges: vec![0..4],
                })
                .collect(),
            match_count: total,
            explain: Vec::new(),
        }
    }

    fn rows(hits: &[FileHit], collapsed: &[NoteId], reasons: &[(NoteId, &str)]) -> Vec<Row> {
        let reasons: HashMap<NoteId, SharedString> = reasons
            .iter()
            .map(|(n, r)| (*n, (*r).to_string().into()))
            .collect();
        let cites = HashMap::from([(2, 1)]);
        let collapsed: HashSet<NoteId> = collapsed.iter().copied().collect();
        let lookup = |id: NoteId| {
            (id != 9).then(|| {
                (
                    SharedString::from(format!("Note {id}")),
                    SharedString::from(format!("dir/{id}.md")),
                )
            })
        };
        flatten(
            hits,
            &Labels {
                title_and_path: &lookup,
                reasons: &reasons,
                cites: &cites,
                collapsed: &collapsed,
            },
        )
    }

    #[test]
    fn groups_lines_under_their_note() {
        let flat = rows(&[hit(1, &[3, 7], 2), hit(2, &[0], 5)], &[], &[]);
        assert_eq!(flat.len(), 5);
        assert!(matches!(
            &flat[0],
            Row::Header {
                note: 1,
                count: 2,
                cite: None,
                collapsed: false,
                ..
            }
        ));
        assert!(matches!(
            &flat[3],
            Row::Header {
                note: 2,
                count: 5,
                cite: Some(1),
                ..
            }
        ));
        assert_eq!(flat[1].target(), (1, Some(3)));
        assert_eq!(flat[0].target(), (1, None));
    }

    #[test]
    fn collapsed_notes_keep_their_header_only() {
        let flat = rows(&[hit(1, &[3, 7], 2), hit(2, &[0], 1)], &[1], &[]);
        assert_eq!(flat.len(), 3);
        assert!(matches!(
            &flat[0],
            Row::Header {
                collapsed: true,
                ..
            }
        ));
    }

    #[test]
    fn reasons_follow_their_header_and_unknown_notes_are_skipped() {
        let flat = rows(&[hit(9, &[1], 1), hit(2, &[4], 1)], &[], &[(2, "why")]);
        assert_eq!(flat.len(), 3);
        assert!(matches!(&flat[1], Row::Reason { note: 2, .. }));
    }

    #[test]
    fn stepping_skips_reasons_and_stops_at_the_ends() {
        let flat = rows(&[hit(1, &[3], 1), hit(2, &[4], 1)], &[], &[(2, "why")]);
        assert_eq!(step(&flat, None, 1), Some(0));
        assert_eq!(step(&flat, Some(1), 1), Some(2));
        assert_eq!(step(&flat, Some(2), 1), Some(4));
        assert_eq!(step(&flat, Some(4), 1), Some(4));
        assert_eq!(step(&flat, Some(0), -1), Some(0));
        assert_eq!(step(&flat, None, -1), Some(4));
        assert_eq!(step(&[], None, 1), None);
    }

    #[test]
    fn a_page_moves_many_rows_and_the_ends_are_one_key_away() {
        let flat = rows(&[hit(1, &[3, 4, 5], 3), hit(2, &[6, 7], 2)], &[], &[]);
        assert_eq!(flat.len(), 7);
        assert_eq!(
            jump(&flat, None, 3),
            Some(2),
            "from nothing a page counts from before the first row"
        );
        assert_eq!(jump(&flat, Some(1), 3), Some(4));
        assert_eq!(jump(&flat, Some(1), 50), Some(6), "stops on the last row");
        assert_eq!(jump(&flat, Some(5), -3), Some(2));
        assert_eq!(jump(&flat, Some(5), -50), Some(0), "stops on the first row");
        assert_eq!(jump(&flat, None, -2), Some(5));
        assert_eq!(jump(&[], None, 4), None);
        assert_eq!((end(&flat, false), end(&flat, true)), (Some(0), Some(6)));
        let reasoned = rows(&[hit(1, &[3], 1), hit(2, &[4], 1)], &[], &[(2, "why")]);
        assert_eq!(
            end(&reasoned, true),
            Some(4),
            "a reason row is never selected"
        );
        assert_eq!(end(&[], true), None);
    }
}
