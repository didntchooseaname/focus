use crate::diff::{Op, ops};

#[derive(Debug, Clone)]
struct Hunk {
    base: std::ops::Range<usize>,
    new: std::ops::Range<usize>,
}

fn hunks(base: &[&str], side: &[&str]) -> Vec<Hunk> {
    let mut out: Vec<Hunk> = Vec::new();
    let (mut next_base, mut next_new) = (0, 0);
    let mut open: Option<Hunk> = None;
    for op in ops(base, side) {
        match op {
            Op::Same(i, j) => {
                out.extend(open.take());
                next_base = i + 1;
                next_new = j + 1;
            }
            Op::Removed(i) => {
                let hunk = open.get_or_insert(Hunk {
                    base: next_base..next_base,
                    new: next_new..next_new,
                });
                hunk.base.end = i + 1;
                next_base = i + 1;
            }
            Op::Added(j) => {
                let hunk = open.get_or_insert(Hunk {
                    base: next_base..next_base,
                    new: next_new..next_new,
                });
                hunk.new.end = j + 1;
                next_new = j + 1;
            }
        }
    }
    out.extend(open);
    out
}

fn touch(a: &std::ops::Range<usize>, b: &std::ops::Range<usize>) -> bool {
    let overlap = a.start < b.end && b.start < a.end;
    let insertion = (a.is_empty() || b.is_empty()) && a.start <= b.end && b.start <= a.end;
    overlap || insertion
}

fn side_text(
    base: &[&str],
    side: &[&str],
    hunks: &[&Hunk],
    range: &std::ops::Range<usize>,
) -> String {
    let mut out = String::new();
    let mut at = range.start;
    for hunk in hunks {
        base[at..hunk.base.start]
            .iter()
            .for_each(|l| out.push_str(l));
        side[hunk.new.clone()].iter().for_each(|l| out.push_str(l));
        at = hunk.base.end;
    }
    base[at..range.end].iter().for_each(|l| out.push_str(l));
    out
}

pub fn merge(base: &str, ours: &str, theirs: &str) -> Option<String> {
    let base_lines: Vec<&str> = base.split_inclusive('\n').collect();
    let ours_lines: Vec<&str> = ours.split_inclusive('\n').collect();
    let theirs_lines: Vec<&str> = theirs.split_inclusive('\n').collect();
    let ours_hunks = hunks(&base_lines, &ours_lines);
    let theirs_hunks = hunks(&base_lines, &theirs_lines);

    let mut all: Vec<(bool, &Hunk)> = ours_hunks
        .iter()
        .map(|h| (false, h))
        .chain(theirs_hunks.iter().map(|h| (true, h)))
        .collect();
    all.sort_by_key(|(_, h)| (h.base.start, h.base.end));

    let mut out = String::with_capacity(ours.len().max(theirs.len()));
    let mut copied = 0;
    let mut index = 0;
    while index < all.len() {
        let mut range = all[index].1.base.clone();
        let mut end = index + 1;
        while end < all.len() && touch(&range, &all[end].1.base) {
            range.start = range.start.min(all[end].1.base.start);
            range.end = range.end.max(all[end].1.base.end);
            end += 1;
        }
        let cluster = &all[index..end];
        let of = |theirs: bool| -> Vec<&Hunk> {
            cluster
                .iter()
                .filter(|(side, _)| *side == theirs)
                .map(|(_, h)| *h)
                .collect()
        };
        let (mine, other) = (of(false), of(true));
        let text = match (mine.is_empty(), other.is_empty()) {
            (false, true) => side_text(&base_lines, &ours_lines, &mine, &range),
            (true, false) => side_text(&base_lines, &theirs_lines, &other, &range),
            _ => {
                let a = side_text(&base_lines, &ours_lines, &mine, &range);
                let b = side_text(&base_lines, &theirs_lines, &other, &range);
                if a != b {
                    return None;
                }
                a
            }
        };
        base_lines[copied..range.start]
            .iter()
            .for_each(|l| out.push_str(l));
        out.push_str(&text);
        copied = range.end;
        index = end;
    }
    base_lines[copied..].iter().for_each(|l| out.push_str(l));
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::merge;

    #[test]
    fn changes_far_apart_merge() {
        let base = "a\nb\nc\nd\ne\n";
        assert_eq!(
            merge(base, "A\nb\nc\nd\ne\n", "a\nb\nc\nd\nE\n").as_deref(),
            Some("A\nb\nc\nd\nE\n")
        );
    }

    #[test]
    fn changes_on_neighbouring_lines_merge() {
        assert_eq!(
            merge("a\nb\nc\n", "a\nB\nc\n", "a\nb\nC\n").as_deref(),
            Some("a\nB\nC\n")
        );
    }

    #[test]
    fn an_insertion_and_a_removal_elsewhere_merge() {
        let base = "one\ntwo\nthree\nfour\n";
        assert_eq!(
            merge(base, "one\nnew\ntwo\nthree\nfour\n", "one\ntwo\nthree\n").as_deref(),
            Some("one\nnew\ntwo\nthree\n")
        );
    }

    #[test]
    fn overlapping_changes_do_not_merge() {
        assert_eq!(merge("a\nb\nc\n", "a\nX\nc\n", "a\nY\nc\n"), None);
        assert_eq!(merge("a\nb\nc\nd\n", "a\nB\nC\nd\n", "a\nd\n"), None);
    }

    #[test]
    fn insertions_at_the_same_place_do_not_merge() {
        assert_eq!(merge("a\nb\n", "a\nX\nb\n", "a\nY\nb\n"), None);
        assert_eq!(merge("a\nb\n", "a\nX\nb\n", "a\nB\n"), None);
    }

    #[test]
    fn the_same_change_on_both_sides_is_taken_once() {
        assert_eq!(
            merge("a\nb\nc\n", "a\nB\nc\n", "a\nB\nc\n").as_deref(),
            Some("a\nB\nc\n")
        );
        assert_eq!(
            merge("a\nb\nc\n", "a\nB\nc\nend\n", "start\na\nB\nc\n").as_deref(),
            Some("start\na\nB\nc\nend\n")
        );
    }

    #[test]
    fn changes_at_the_first_and_the_last_line() {
        let base = "first\nmiddle 1\nmiddle 2\nlast\n";
        assert_eq!(
            merge(
                base,
                "FIRST\nmiddle 1\nmiddle 2\nlast\n",
                "first\nmiddle 1\nmiddle 2\nLAST\n"
            )
            .as_deref(),
            Some("FIRST\nmiddle 1\nmiddle 2\nLAST\n")
        );
        assert_eq!(
            merge("x\ny\n", "top\nx\ny\n", "x\ny\nbottom\n").as_deref(),
            Some("top\nx\ny\nbottom\n")
        );
        assert_eq!(
            merge("a\nb\nc", "A\nb\nc", "a\nb\nc\nd").as_deref(),
            Some("A\nb\nc\nd")
        );
    }

    #[test]
    fn crlf_line_ends_are_kept() {
        let base = "a\r\nb\r\nc\r\nd\r\n";
        assert_eq!(
            merge(base, "A\r\nb\r\nc\r\nd\r\n", "a\r\nb\r\nc\r\nD\r\n").as_deref(),
            Some("A\r\nb\r\nc\r\nD\r\n")
        );
        assert_eq!(merge(base, "A\r\nb\r\nc\r\nd\r\n", "a\nb\nc\nd\n"), None);
    }

    #[test]
    fn an_empty_ancestor() {
        assert_eq!(merge("", "", "text\n").as_deref(), Some("text\n"));
        assert_eq!(merge("", "mine\n", "").as_deref(), Some("mine\n"));
        assert_eq!(merge("", "same\n", "same\n").as_deref(), Some("same\n"));
        assert_eq!(merge("", "mine\n", "theirs\n"), None);
    }

    #[test]
    fn an_unchanged_side_gives_the_other() {
        let base = "a\nb\n";
        assert_eq!(merge(base, base, "z\n").as_deref(), Some("z\n"));
        assert_eq!(merge(base, "", base).as_deref(), Some(""));
        assert_eq!(merge(base, base, base).as_deref(), Some(base));
    }

    #[test]
    fn random_merges_keep_both_sides_when_they_succeed() {
        let mut seed = 0x9E3779B97F4A7C15u64;
        let mut next = move |bound: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % bound
        };
        let mut merged = 0;
        for round in 0..300 {
            let base: Vec<String> = (0..20).map(|i| format!("line {i}\n")).collect();
            let mut ours = base.clone();
            let mut theirs = base.clone();
            let a = next(8) as usize;
            ours[a] = format!("ours {round}\n");
            let b = 12 + next(8) as usize;
            theirs[b] = format!("theirs {round}\n");
            if next(2) == 0 {
                theirs.remove(b.min(theirs.len() - 1).max(11));
            }
            let base: String = base.concat();
            let result = merge(&base, &ours.concat(), &theirs.concat())
                .unwrap_or_else(|| panic!("round {round}: no merge"));
            assert!(result.contains(&format!("ours {round}\n")), "round {round}");
            merged += 1;
        }
        assert_eq!(merged, 300);
    }
}
