#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    Same,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiffLine {
    pub mark: Mark,
    pub text: String,
}

const MAX_CELLS: usize = 4_000_000;

pub(crate) fn lines(before: &str, after: &str) -> Vec<DiffLine> {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let mut start = 0;
    while start < a.len() && start < b.len() && a[start] == b[start] {
        start += 1;
    }
    let mut end = 0;
    while end < a.len() - start
        && end < b.len() - start
        && a[a.len() - 1 - end] == b[b.len() - 1 - end]
    {
        end += 1;
    }
    let (ma, mb) = (&a[start..a.len() - end], &b[start..b.len() - end]);
    let mut out: Vec<DiffLine> = a[..start].iter().map(|t| line(Mark::Same, t)).collect();
    if ma.len().saturating_mul(mb.len()) > MAX_CELLS {
        out.extend(ma.iter().map(|t| line(Mark::Removed, t)));
        out.extend(mb.iter().map(|t| line(Mark::Added, t)));
    } else {
        out.extend(middle(ma, mb));
    }
    out.extend(a[a.len() - end..].iter().map(|t| line(Mark::Same, t)));
    out
}

fn line(mark: Mark, text: &str) -> DiffLine {
    DiffLine {
        mark,
        text: text.to_string(),
    }
}

fn middle(a: &[&str], b: &[&str]) -> Vec<DiffLine> {
    let (n, m) = (a.len(), b.len());
    let mut common = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            common[at(i, j)] = if a[i] == b[j] {
                common[at(i + 1, j + 1)] + 1
            } else {
                common[at(i + 1, j)].max(common[at(i, j + 1)])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::with_capacity(n + m);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push(line(Mark::Same, a[i]));
            i += 1;
            j += 1;
        } else if common[at(i + 1, j)] >= common[at(i, j + 1)] {
            out.push(line(Mark::Removed, a[i]));
            i += 1;
        } else {
            out.push(line(Mark::Added, b[j]));
            j += 1;
        }
    }
    out.extend(a[i..].iter().map(|t| line(Mark::Removed, t)));
    out.extend(b[j..].iter().map(|t| line(Mark::Added, t)));
    out
}

pub(crate) fn counts(diff: &[DiffLine]) -> (usize, usize) {
    let added = diff.iter().filter(|l| l.mark == Mark::Added).count();
    let removed = diff.iter().filter(|l| l.mark == Mark::Removed).count();
    (added, removed)
}

pub(crate) fn hunks(diff: &[DiffLine], context: usize) -> (Vec<DiffLine>, usize) {
    let mut keep = vec![false; diff.len()];
    for (index, entry) in diff.iter().enumerate() {
        if entry.mark != Mark::Same {
            let from = index.saturating_sub(context);
            let to = (index + context + 1).min(diff.len());
            keep[from..to].iter_mut().for_each(|k| *k = true);
        }
    }
    let kept: Vec<DiffLine> = diff
        .iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(l, _)| l.clone())
        .collect();
    let hidden = diff.len() - kept.len();
    (kept, hidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts_have_no_change() {
        let diff = lines("a\nb\n", "a\nb\n");
        assert_eq!(counts(&diff), (0, 0));
    }

    #[test]
    fn a_replaced_line_is_one_removal_and_one_addition() {
        let diff = lines("a\nb\nc", "a\nB\nc");
        assert_eq!(counts(&diff), (1, 1));
        assert_eq!(
            diff[1],
            DiffLine {
                mark: Mark::Removed,
                text: "b".into()
            }
        );
        assert_eq!(
            diff[2],
            DiffLine {
                mark: Mark::Added,
                text: "B".into()
            }
        );
    }

    #[test]
    fn new_note_is_all_additions() {
        let diff = lines("", "x\ny");
        assert_eq!(counts(&diff), (2, 0));
    }

    #[test]
    fn hunks_keep_context_and_report_the_rest() {
        let before: String = (0..20).map(|i| format!("l{i}\n")).collect();
        let after = before.replace("l10\n", "changed\n");
        let (kept, hidden) = hunks(&lines(&before, &after), 1);
        assert_eq!(kept.len(), 4);
        assert_eq!(hidden, 17);
    }

    #[test]
    fn moved_block_is_found_by_the_table() {
        let diff = lines("a\nb\nc\nd", "a\nc\nb\nd");
        assert_eq!(counts(&diff), (1, 1));
    }
}
