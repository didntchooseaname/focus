use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    Same(String),
    Removed(String),
    Added(String),
}

enum Edit {
    Same(usize, usize),
    Removed(usize),
    Added(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Same(usize, usize),
    Removed(usize),
    Added(usize),
}

pub fn diff(old: &str, new: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new.split_inclusive('\n').collect();
    ops(&old_lines, &new_lines)
        .into_iter()
        .map(|op| match op {
            Op::Same(i, _) => DiffLine::Same(text(old_lines[i])),
            Op::Removed(i) => DiffLine::Removed(text(old_lines[i])),
            Op::Added(j) => DiffLine::Added(text(new_lines[j])),
        })
        .collect()
}

pub(crate) fn ops(old_lines: &[&str], new_lines: &[&str]) -> Vec<Op> {
    let mut ids = HashMap::new();
    let a = intern(&mut ids, old_lines);
    let b = intern(&mut ids, new_lines);

    let mut in_new = vec![false; ids.len()];
    b.iter().for_each(|&id| in_new[id as usize] = true);
    let mut in_old = vec![false; ids.len()];
    a.iter().for_each(|&id| in_old[id as usize] = true);
    let kept_old: Vec<usize> = (0..a.len()).filter(|&i| in_new[a[i] as usize]).collect();
    let kept_new: Vec<usize> = (0..b.len()).filter(|&j| in_old[b[j] as usize]).collect();
    let filtered_old: Vec<u32> = kept_old.iter().map(|&i| a[i]).collect();
    let filtered_new: Vec<u32> = kept_new.iter().map(|&j| b[j]).collect();
    let mut edits = Vec::with_capacity(filtered_old.len().max(filtered_new.len()));
    walk(&filtered_old, &filtered_new, 0, 0, &mut edits);

    let mut out = Assembler {
        next_old: 0,
        next_new: 0,
        ops: Vec::with_capacity(a.len().max(b.len())),
    };
    for edit in edits {
        match edit {
            Edit::Same(i, j) => out.same(kept_old[i], kept_new[j]),
            Edit::Removed(i) => out.removed_up_to(kept_old[i] + 1),
            Edit::Added(j) => out.added_up_to(kept_new[j] + 1),
        }
    }
    out.removed_up_to(old_lines.len());
    out.added_up_to(new_lines.len());
    out.ops
}

struct Assembler {
    next_old: usize,
    next_new: usize,
    ops: Vec<Op>,
}

fn text(line: &str) -> String {
    line.strip_suffix('\n').unwrap_or(line).to_string()
}

impl Assembler {
    fn removed_up_to(&mut self, end: usize) {
        self.ops.extend((self.next_old..end).map(Op::Removed));
        self.next_old = self.next_old.max(end);
    }

    fn added_up_to(&mut self, end: usize) {
        self.ops.extend((self.next_new..end).map(Op::Added));
        self.next_new = self.next_new.max(end);
    }

    fn same(&mut self, i: usize, j: usize) {
        self.removed_up_to(i);
        self.added_up_to(j);
        self.ops.push(Op::Same(i, j));
        self.next_old = i + 1;
        self.next_new = j + 1;
    }
}

fn intern<'a>(ids: &mut HashMap<&'a str, u32>, lines: &[&'a str]) -> Vec<u32> {
    lines
        .iter()
        .map(|&line| {
            let next = ids.len() as u32;
            *ids.entry(line).or_insert(next)
        })
        .collect()
}

fn walk(a: &[u32], b: &[u32], ao: usize, bo: usize, out: &mut Vec<Edit>) {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    for i in 0..prefix {
        out.push(Edit::Same(ao + i, bo + i));
    }
    let (a, b) = (&a[prefix..], &b[prefix..]);
    let (ao, bo) = (ao + prefix, bo + prefix);
    let suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (a, b) = (&a[..a.len() - suffix], &b[..b.len() - suffix]);
    if a.is_empty() {
        out.extend((0..b.len()).map(|j| Edit::Added(bo + j)));
    } else if b.is_empty() {
        out.extend((0..a.len()).map(|i| Edit::Removed(ao + i)));
    } else {
        let (x0, y0, x, y) = middle_snake(a, b);
        walk(&a[..x0], &b[..y0], ao, bo, out);
        out.extend((x0..x).map(|i| Edit::Same(ao + i, bo + y0 + (i - x0))));
        walk(&a[x..], &b[y..], ao + x, bo + y, out);
    }
    for i in 0..suffix {
        out.push(Edit::Same(ao + a.len() + i, bo + b.len() + i));
    }
}

fn middle_snake(a: &[u32], b: &[u32]) -> (usize, usize, usize, usize) {
    let n = a.len() as isize;
    let m = b.len() as isize;
    let delta = n - m;
    let odd = delta & 1 != 0;
    let max = (n + m + 1) / 2;
    let offset = max + 1;
    let at = |k: isize| (k + offset) as usize;
    let mut forward = vec![0isize; (2 * max + 3) as usize];
    let mut backward = vec![0isize; (2 * max + 3) as usize];
    for d in 0..=max {
        for k in (-d..=d).step_by(2) {
            let mut x = if k == -d || (k != d && forward[at(k - 1)] < forward[at(k + 1)]) {
                forward[at(k + 1)]
            } else {
                forward[at(k - 1)] + 1
            };
            let mut y = x - k;
            let (x0, y0) = (x, y);
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            forward[at(k)] = x;
            if odd && (k - delta).abs() < d && x + backward[at(delta - k)] >= n {
                return (x0 as usize, y0 as usize, x as usize, y as usize);
            }
        }
        for k in (-d..=d).step_by(2) {
            let mut x = if k == -d || (k != d && backward[at(k - 1)] < backward[at(k + 1)]) {
                backward[at(k + 1)]
            } else {
                backward[at(k - 1)] + 1
            };
            let mut y = x - k;
            let (x0, y0) = (x, y);
            while x < n && y < m && a[(n - 1 - x) as usize] == b[(m - 1 - y) as usize] {
                x += 1;
                y += 1;
            }
            backward[at(k)] = x;
            if !odd && (k - delta).abs() <= d && x + forward[at(delta - k)] >= n {
                return (
                    (n - x) as usize,
                    (m - y) as usize,
                    (n - x0) as usize,
                    (m - y0) as usize,
                );
            }
        }
    }
    unreachable!("the two searches meet within (n + m + 1) / 2 edits")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcs_len(a: &[&str], b: &[&str]) -> usize {
        let mut table = vec![vec![0usize; b.len() + 1]; a.len() + 1];
        for i in 0..a.len() {
            for j in 0..b.len() {
                table[i + 1][j + 1] = if a[i] == b[j] {
                    table[i][j] + 1
                } else {
                    table[i][j + 1].max(table[i + 1][j])
                };
            }
        }
        table[a.len()][b.len()]
    }

    fn rebuild(lines: &[DiffLine], keep_removed: bool) -> String {
        lines
            .iter()
            .filter_map(|l| match l {
                DiffLine::Same(t) => Some(format!("{t}\n")),
                DiffLine::Removed(t) if keep_removed => Some(format!("{t}\n")),
                DiffLine::Added(t) if !keep_removed => Some(format!("{t}\n")),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn identical_and_empty_inputs() {
        assert!(diff("", "").is_empty());
        assert_eq!(
            diff("a\nb\n", "a\nb\n"),
            vec![DiffLine::Same("a".into()), DiffLine::Same("b".into())]
        );
        assert_eq!(diff("", "x\n"), vec![DiffLine::Added("x".into())]);
        assert_eq!(diff("x\n", ""), vec![DiffLine::Removed("x".into())]);
    }

    #[test]
    fn a_changed_line_is_removed_then_added() {
        assert_eq!(
            diff("a\nb\nc\n", "a\nB\nc\n"),
            vec![
                DiffLine::Same("a".into()),
                DiffLine::Removed("b".into()),
                DiffLine::Added("B".into()),
                DiffLine::Same("c".into()),
            ]
        );
    }

    #[test]
    fn a_missing_final_newline_is_a_change() {
        assert_eq!(
            diff("a\nb", "a\nb\n"),
            vec![
                DiffLine::Same("a".into()),
                DiffLine::Removed("b".into()),
                DiffLine::Added("b".into()),
            ]
        );
    }

    #[test]
    fn random_inputs_give_valid_minimal_scripts() {
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut next = move |bound: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % bound
        };
        for round in 0..400 {
            let alphabet = if round % 2 == 0 {
                2 + next(5)
            } else {
                10 + next(60)
            };
            let len_a = next(40) as usize;
            let len_b = next(40) as usize;
            let a: Vec<String> = (0..len_a).map(|_| format!("l{}", next(alphabet))).collect();
            let b: Vec<String> = (0..len_b).map(|_| format!("l{}", next(alphabet))).collect();
            let old: String = a.iter().map(|l| format!("{l}\n")).collect();
            let new: String = b.iter().map(|l| format!("{l}\n")).collect();
            let script = diff(&old, &new);
            assert_eq!(rebuild(&script, true), old, "round {round}: old side");
            assert_eq!(rebuild(&script, false), new, "round {round}: new side");
            let same = script
                .iter()
                .filter(|l| matches!(l, DiffLine::Same(_)))
                .count();
            let a_refs: Vec<&str> = a.iter().map(String::as_str).collect();
            let b_refs: Vec<&str> = b.iter().map(String::as_str).collect();
            assert_eq!(
                same,
                lcs_len(&a_refs, &b_refs),
                "round {round}: not minimal"
            );
        }
    }

    #[test]
    fn ten_thousand_lines_with_scattered_changes() {
        let old: String = (0..10_000).map(|i| format!("line {i}\n")).collect();
        let new: String = (0..10_000)
            .map(|i| {
                if i % 97 == 0 {
                    format!("changed {i}\n")
                } else {
                    format!("line {i}\n")
                }
            })
            .collect();
        let script = diff(&old, &new);
        let changed = script
            .iter()
            .filter(|l| !matches!(l, DiffLine::Same(_)))
            .count();
        assert_eq!(changed, 2 * 104);
    }
}
