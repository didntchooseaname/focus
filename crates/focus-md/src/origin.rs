use std::ops::Range;

#[derive(Debug, Clone)]
pub(crate) struct Origin {
    pieces: Vec<Piece>,
}

#[derive(Debug, Clone, Copy)]
struct Piece {
    from: usize,
    to: usize,
    exact: bool,
}

impl Origin {
    pub fn shift(by: usize) -> Self {
        Origin {
            pieces: vec![Piece {
                from: 0,
                to: by,
                exact: true,
            }],
        }
    }

    pub fn is_identity(&self) -> bool {
        matches!(
            self.pieces.as_slice(),
            [Piece {
                to: 0,
                exact: true,
                ..
            }]
        )
    }

    pub fn push(&mut self, from: usize, to: usize, exact: bool) {
        if let Some(last) = self.pieces.last_mut() {
            if last.from == from {
                *last = Piece { from, to, exact };
                if self.pieces.len() > 1 {
                    let before = self.pieces[self.pieces.len() - 2];
                    if before.exact == exact && before.to + (from - before.from) == to {
                        self.pieces.pop();
                    }
                }
                return;
            }
            if last.exact == exact && last.to + (from - last.from) == to {
                return;
            }
        }
        self.pieces.push(Piece { from, to, exact });
    }

    fn piece(&self, at: usize) -> Piece {
        let index = self.pieces.partition_point(|p| p.from <= at).max(1) - 1;
        self.pieces[index]
    }

    fn at(&self, at: usize) -> usize {
        let piece = self.piece(at);
        piece.to + at.saturating_sub(piece.from)
    }

    pub fn range(&self, range: &Range<usize>) -> Range<usize> {
        let start = self.at(range.start);
        if range.end <= range.start {
            return start..start;
        }
        let last = self.piece(range.end - 1);
        start..last.to + (range.end - last.from)
    }

    pub fn breaks(&self, range: &Range<usize>) -> impl Iterator<Item = usize> + '_ {
        let first = self.pieces.partition_point(|p| p.from <= range.start);
        let end = range.end;
        self.pieces[first..]
            .iter()
            .take_while(move |p| p.from < end)
            .map(|p| p.from)
    }

    pub fn exact(&self, range: &Range<usize>) -> bool {
        let first = self
            .pieces
            .partition_point(|p| p.from <= range.start)
            .max(1)
            - 1;
        self.pieces[first..]
            .iter()
            .take_while(|p| p.from < range.end.max(range.start + 1))
            .all(|p| p.exact)
    }

    pub fn slice(&self, start: usize) -> Origin {
        let first = self.pieces.partition_point(|p| p.from <= start).max(1) - 1;
        let head = self.pieces[first];
        let mut pieces = vec![Piece {
            from: 0,
            to: head.to + start.saturating_sub(head.from),
            exact: head.exact,
        }];
        pieces.extend(self.pieces[first + 1..].iter().map(|p| Piece {
            from: p.from - start,
            ..*p
        }));
        Origin { pieces }
    }

    pub fn of_lines(original: &str, line_starts: &[usize], first: u32, derived: &str) -> Origin {
        let mut origin = Origin { pieces: Vec::new() };
        let mut from = 0;
        for (k, line) in derived.split('\n').enumerate() {
            let (to, exact) = match line_starts.get(first as usize + k) {
                Some(&start) => {
                    let end = original[start..]
                        .find('\n')
                        .map_or(original.len(), |i| start + i);
                    let whole = &original[start..end];
                    [
                        whole,
                        whole.strip_suffix('\r').unwrap_or(whole),
                        whole.trim_end(),
                    ]
                    .iter()
                    .find(|candidate| candidate.ends_with(line))
                    .map_or((start, false), |c| (start + c.len() - line.len(), true))
                }
                None => (original.len(), false),
            };
            origin.push(from, to, exact);
            from += line.len() + 1;
        }
        if origin.pieces.is_empty() {
            origin.push(0, 0, false);
        }
        origin
    }
}

pub(crate) fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        text.bytes()
            .enumerate()
            .filter(|&(_, b)| b == b'\n')
            .map(|(i, _)| i + 1),
    );
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_removed_comment_is_a_jump() {
        let mut origin = Origin::shift(0);
        origin.push(2, 7, true);
        assert_eq!(origin.range(&(0..2)), 0..2);
        assert_eq!(origin.range(&(2..4)), 7..9);
        assert_eq!(origin.range(&(0..4)), 0..9);
        assert_eq!(origin.range(&(2..2)), 7..7);
        assert_eq!(origin.breaks(&(0..4)).collect::<Vec<_>>(), vec![2]);
        assert_eq!(origin.breaks(&(2..4)).count(), 0);
        assert_eq!(origin.slice(1).range(&(1..3)), 7..9);
    }

    #[test]
    fn lines_without_their_markers() {
        let original = "> a\n> > bc  \r\n>\n";
        let derived = "a\n> bc\n";
        let origin = Origin::of_lines(original, &line_starts(original), 0, derived);
        assert_eq!(origin.range(&(0..1)), 2..3);
        assert_eq!(&original[origin.range(&(4..6))], "bc");
        assert!(origin.exact(&(4..6)));
        let empty = origin.range(&(7..7));
        assert_eq!(empty, 15..15);
    }
}
