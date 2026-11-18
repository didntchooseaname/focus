use std::collections::HashSet;
use std::ops::Range;
use std::time::SystemTime;

use focus_core::{NoteId, Vault, VaultError};
use focus_search::{CancelToken, Field, Pattern, QueryNode, SearchOptions};
use regex::{Regex, RegexBuilder};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternError {
    NoTerms,
    InvalidRegex(String),
}

#[derive(Debug, Clone)]
pub struct Matcher {
    regex: Regex,
    expand_groups: bool,
}

fn positive_terms(node: &QueryNode, out: &mut Vec<(String, bool)>) {
    match node {
        QueryNode::And(nodes) => {
            let words: Vec<String> = nodes
                .iter()
                .filter_map(|n| match n {
                    QueryNode::Term {
                        field: Field::Any | Field::Content,
                        pattern: Pattern::Text(text),
                    } => Some(regex::escape(text)),
                    _ => None,
                })
                .collect();
            if !words.is_empty() {
                out.push((words.join(r"\s+"), false));
            }
            for node in nodes {
                if !matches!(
                    node,
                    QueryNode::Term {
                        pattern: Pattern::Text(_),
                        ..
                    }
                ) {
                    positive_terms(node, out);
                }
            }
        }
        QueryNode::Or(nodes) => nodes.iter().for_each(|n| positive_terms(n, out)),
        QueryNode::Term {
            field: Field::Any | Field::Content,
            pattern,
        } => match pattern {
            Pattern::Text(text) | Pattern::Phrase(text) => out.push((regex::escape(text), false)),
            Pattern::Regex(source) => out.push((source.clone(), true)),
            Pattern::KeyValue(..) => {}
        },
        _ => {}
    }
}

impl Matcher {
    pub fn from_query(source: &str, options: &SearchOptions) -> Result<Matcher, PatternError> {
        let (pattern, expand_groups, raw) = if options.regex {
            (source.trim().to_string(), true, source.trim().to_string())
        } else {
            let mut terms = Vec::new();
            positive_terms(&focus_search::parse_query(source).root, &mut terms);
            if terms.is_empty() {
                return Err(PatternError::NoTerms);
            }
            let raw = terms
                .iter()
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>()
                .join("|");
            (
                format!("(?:{raw})"),
                terms.iter().any(|(_, regex)| *regex),
                raw,
            )
        };
        if pattern.is_empty() {
            return Err(PatternError::NoTerms);
        }
        let pattern = if options.whole_word {
            format!(r"\b{pattern}\b")
        } else {
            pattern
        };
        let case_insensitive = !(options.case_sensitive || raw.chars().any(char::is_uppercase));
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(case_insensitive)
            .multi_line(true)
            .size_limit(1 << 22)
            .build()
            .map_err(|e| {
                PatternError::InvalidRegex(
                    e.to_string()
                        .lines()
                        .last()
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                )
            })?;
        Ok(Matcher {
            regex,
            expand_groups,
        })
    }

    pub fn replace_line(&self, line: &str, replacement: &str) -> Option<LineChange> {
        let mut after = String::with_capacity(line.len());
        let (mut before_ranges, mut after_ranges) = (Vec::new(), Vec::new());
        let mut last = 0;
        for captures in self.regex.captures_iter(line) {
            let whole = captures.get(0)?;
            if whole.is_empty() {
                continue;
            }
            after.push_str(&line[last..whole.start()]);
            let start = after.len();
            if self.expand_groups {
                captures.expand(replacement, &mut after);
            } else {
                after.push_str(replacement);
            }
            before_ranges.push(whole.range());
            after_ranges.push(start..after.len());
            last = whole.end();
        }
        if before_ranges.is_empty() {
            return None;
        }
        after.push_str(&line[last..]);
        Some(LineChange {
            line: 0,
            before: line.to_string(),
            after,
            before_ranges,
            after_ranges,
        })
    }

    #[cfg(test)]
    fn plain_replace(&self, line: &str, replacement: &str) -> String {
        use regex::NoExpand;
        if self.expand_groups {
            self.regex.replace_all(line, replacement).into_owned()
        } else {
            self.regex
                .replace_all(line, NoExpand(replacement))
                .into_owned()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineChange {
    pub line: u32,
    pub before: String,
    pub after: String,
    pub before_ranges: Vec<Range<usize>>,
    pub after_ranges: Vec<Range<usize>>,
}

#[derive(Debug, Clone)]
pub struct NoteChange {
    pub note: NoteId,
    pub title: String,
    pub folder: String,
    source: String,
    modified: SystemTime,
    pub lines: Vec<LineChange>,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub notes: Vec<NoteChange>,
    pub unreadable: Vec<String>,
    pub cancelled: bool,
}

impl Plan {
    pub fn line_count(&self) -> usize {
        self.notes.iter().map(|n| n.lines.len()).sum()
    }
}

pub fn build(
    vault: &Vault,
    candidates: &[NoteId],
    matcher: &Matcher,
    replacement: &str,
    cancel: &CancelToken,
) -> Plan {
    let mut plan = Plan::default();
    for &id in candidates {
        if cancel.is_cancelled() {
            plan.cancelled = true;
            break;
        }
        let Some(meta) = vault.note(id) else { continue };
        let path = vault.absolute(&meta.path);
        let read = std::fs::read(&path).and_then(|bytes| {
            let modified = std::fs::metadata(&path)?.modified()?;
            String::from_utf8(bytes)
                .map(|text| (text, modified))
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        });
        let Ok((source, modified)) = read else {
            plan.unreadable.push(meta.path.display().to_string());
            continue;
        };
        let mut lines = Vec::new();
        for (index, raw) in source.split('\n').enumerate() {
            let text = raw.strip_suffix('\r').unwrap_or(raw);
            if let Some(mut change) = matcher.replace_line(text, replacement) {
                change.line = index as u32;
                lines.push(change);
            }
        }
        if lines.is_empty() {
            continue;
        }
        let folder = meta
            .path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        plan.notes.push(NoteChange {
            note: id,
            title: meta.title.to_string(),
            folder,
            source,
            modified,
            lines,
        });
    }
    plan
}

#[derive(Debug, Clone, Default)]
pub struct Exclusions {
    pub notes: HashSet<NoteId>,
    pub lines: HashSet<(NoteId, u32)>,
}

impl Exclusions {
    pub fn keeps(&self, note: NoteId, line: u32) -> bool {
        !self.notes.contains(&note) && !self.lines.contains(&(note, line))
    }

    pub fn kept_lines(&self, plan: &Plan) -> usize {
        plan.notes
            .iter()
            .flat_map(|n| n.lines.iter().map(move |l| (n.note, l.line)))
            .filter(|&(n, l)| self.keeps(n, l))
            .count()
    }

    pub fn kept_notes(&self, plan: &Plan) -> usize {
        plan.notes
            .iter()
            .filter(|n| n.lines.iter().any(|l| self.keeps(n.note, l.line)))
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Changed,
    Write(String),
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub written: Vec<(NoteId, usize)>,
    pub failed: Vec<(NoteId, String, Failure)>,
    pub occurrences: usize,
}

pub fn apply(vault: &Vault, plan: &Plan, exclusions: &Exclusions) -> Report {
    let mut report = Report::default();
    for note in &plan.notes {
        let kept: Vec<&LineChange> = note
            .lines
            .iter()
            .filter(|l| exclusions.keeps(note.note, l.line))
            .collect();
        if kept.is_empty() {
            continue;
        }
        let mut content = String::with_capacity(note.source.len());
        let mut next = kept.iter().peekable();
        for (index, raw) in note.source.split_inclusive('\n').enumerate() {
            match next.next_if(|l| l.line as usize == index) {
                Some(change) => {
                    let ending = &raw[raw.trim_end_matches(['\n', '\r']).len()..];
                    content.push_str(&change.after);
                    content.push_str(ending);
                }
                None => content.push_str(raw),
            }
        }
        match vault.write_with(
            note.note,
            &content,
            Some(note.modified),
            focus_core::HistoryKind::Bulk,
        ) {
            Ok(_) => {
                report.written.push((note.note, kept.len()));
                report.occurrences += kept.iter().map(|l| l.after_ranges.len()).sum::<usize>();
            }
            Err(VaultError::Conflict(_)) => {
                report
                    .failed
                    .push((note.note, note.title.clone(), Failure::Changed))
            }
            Err(error) => report.failed.push((
                note.note,
                note.title.clone(),
                Failure::Write(error.to_string()),
            )),
        }
    }
    report
}

#[cfg(test)]
pub(crate) fn scratch_vault(
    name: &str,
    notes: &[(&str, &str)],
) -> (std::sync::Arc<Vault>, std::path::PathBuf) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-vaults")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch vault folder");
    for (path, text) in notes {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("note folder")).expect("note folder");
        std::fs::write(file, text).expect("scratch note");
    }
    let cache = root.join("..").join(format!("{name}-cache"));
    let vault = Vault::open(
        &root,
        focus_core::OpenOptions {
            cache_dir: cache,
            ignore: Vec::new(),
            watch: false,
        },
    )
    .expect("scratch vault");
    (vault, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(vault: &Vault) -> Vec<NoteId> {
        let mut ids: Vec<NoteId> = vault.summaries().iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids
    }

    fn options() -> SearchOptions {
        SearchOptions::default()
    }

    #[test]
    fn pattern_takes_positive_text_terms_only() {
        let m = Matcher::from_query("golden ticket -silver tag:#red", &options()).unwrap();
        assert_eq!(
            m.replace_line("a golden  ticket here", "x").unwrap().after,
            "a x here"
        );
        assert!(m.replace_line("a golden one", "x").is_none());
        assert!(m.replace_line("a silver one", "x").is_none());
        let m = Matcher::from_query("cat OR \"red dog\"", &options()).unwrap();
        assert_eq!(
            m.replace_line("cat and red dog", "x").unwrap().after,
            "x and x"
        );
        assert_eq!(
            Matcher::from_query("tag:#red", &options()).unwrap_err(),
            PatternError::NoTerms
        );
        assert!(matches!(
            Matcher::from_query(
                "(",
                &SearchOptions {
                    regex: true,
                    ..options()
                }
            ),
            Err(PatternError::InvalidRegex(_))
        ));
    }

    #[test]
    fn smart_case_and_whole_word() {
        let m = Matcher::from_query("kerb", &options()).unwrap();
        assert_eq!(m.plain_replace("Kerb KERB kerb", "x"), "x x x");
        let m = Matcher::from_query("Kerb", &options()).unwrap();
        assert_eq!(m.plain_replace("Kerb kerb", "x"), "x kerb");
        let m = Matcher::from_query(
            "cat",
            &SearchOptions {
                whole_word: true,
                ..options()
            },
        )
        .unwrap();
        assert_eq!(m.plain_replace("cat concat cat.", "x"), "x concat x.");
    }

    #[test]
    fn ranges_cover_matches_and_inserted_text() {
        let m = Matcher::from_query("ab", &options()).unwrap();
        let change = m.replace_line("xx ab yy ab", "LONGER").unwrap();
        assert_eq!(change.after, "xx LONGER yy LONGER");
        assert_eq!(change.before_ranges, [3..5, 9..11]);
        assert_eq!(change.after_ranges, [3..9, 13..19]);
    }

    #[test]
    fn regex_mode_expands_groups_and_text_mode_does_not() {
        let m = Matcher::from_query(
            r"(\w+)@host",
            &SearchOptions {
                regex: true,
                ..options()
            },
        )
        .unwrap();
        assert_eq!(
            m.replace_line("bob@host", "$1 at host").unwrap().after,
            "bob at host"
        );
        let m = Matcher::from_query("cost", &options()).unwrap();
        assert_eq!(m.replace_line("cost", "$1").unwrap().after, "$1");
    }

    #[test]
    fn plan_lists_changed_lines_and_apply_writes_only_kept_ones() {
        let (vault, root) = scratch_vault(
            "replace-apply",
            &[
                ("a.md", "one foo\ntwo\nfoo foo three\r\nfour foo"),
                ("dir/b.md", "no match\n"),
                ("c.md", "foo"),
            ],
        );
        let m = Matcher::from_query("foo", &options()).unwrap();
        let plan = build(&vault, &ids(&vault), &m, "bar", &CancelToken::new());
        assert_eq!(plan.notes.len(), 2);
        assert_eq!(plan.line_count(), 4);
        let a = plan.notes.iter().find(|n| n.title == "a").unwrap();
        assert_eq!(
            a.lines.iter().map(|l| l.line).collect::<Vec<_>>(),
            [0, 2, 3]
        );
        assert_eq!(
            a.lines[1].before, "foo foo three",
            "the carriage return stays out of the line"
        );
        let mut exclusions = Exclusions::default();
        exclusions.lines.insert((a.note, 3));
        assert_eq!(exclusions.kept_lines(&plan), 3);
        let report = apply(&vault, &plan, &exclusions);
        assert_eq!(report.failed, []);
        assert_eq!(report.occurrences, 4);
        assert_eq!(
            std::fs::read_to_string(root.join("a.md")).unwrap(),
            "one bar\ntwo\nbar bar three\r\nfour foo"
        );
        assert_eq!(std::fs::read_to_string(root.join("c.md")).unwrap(), "bar");
    }

    #[test]
    fn excluded_note_is_not_written_and_nothing_is_written_by_planning() {
        let (vault, root) =
            scratch_vault("replace-exclude", &[("a.md", "foo\n"), ("b.md", "foo\n")]);
        let m = Matcher::from_query("foo", &options()).unwrap();
        let plan = build(&vault, &ids(&vault), &m, "bar", &CancelToken::new());
        assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), "foo\n");
        let mut exclusions = Exclusions::default();
        exclusions.notes.insert(plan.notes[0].note);
        assert_eq!(exclusions.kept_notes(&plan), 1);
        let report = apply(&vault, &plan, &exclusions);
        assert_eq!(report.written.len(), 1);
        let texts: Vec<String> = ["a.md", "b.md"]
            .iter()
            .map(|f| std::fs::read_to_string(root.join(f)).unwrap())
            .collect();
        assert_eq!(texts.iter().filter(|t| t.as_str() == "foo\n").count(), 1);
    }

    #[test]
    fn every_note_replaced_keeps_a_version_even_after_a_recent_one() {
        let (vault, root) = scratch_vault(
            "replace-history",
            &[("a.md", "foo\n"), ("b.md", "foo and more\n")],
        );
        let store = root.join("../replace-history-versions");
        let _ = std::fs::remove_dir_all(&store);
        vault
            .set_history(&store, focus_core::HistoryConfig::default())
            .unwrap();
        let a = vault.note_by_path(std::path::Path::new("a.md")).unwrap();
        vault.write(a, "foo, typed just now\n", None).unwrap();
        let m = Matcher::from_query("foo", &options()).unwrap();
        let plan = build(&vault, &ids(&vault), &m, "bar", &CancelToken::new());
        let report = apply(&vault, &plan, &Exclusions::default());
        assert_eq!(report.written.len(), 2);
        let history = vault.history().unwrap();
        let read = |rel: &str| -> Vec<String> {
            let rel = std::path::Path::new(rel);
            history
                .list(rel)
                .unwrap()
                .iter()
                .map(|v| history.read(rel, &v.id).unwrap())
                .collect()
        };
        assert_eq!(read("a.md"), ["foo, typed just now\n", "foo\n"]);
        assert_eq!(read("b.md"), ["foo and more\n"]);
    }

    #[test]
    fn a_note_edited_since_the_plan_is_reported_and_left_alone() {
        let (vault, root) = scratch_vault("replace-conflict", &[("a.md", "foo\n")]);
        let m = Matcher::from_query("foo", &options()).unwrap();
        let plan = build(&vault, &ids(&vault), &m, "bar", &CancelToken::new());
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(root.join("a.md"), "foo, edited elsewhere\n").unwrap();
        let report = apply(&vault, &plan, &Exclusions::default());
        assert!(report.written.is_empty());
        assert!(matches!(
            report.failed.as_slice(),
            [(_, _, Failure::Changed)]
        ));
        assert_eq!(
            std::fs::read_to_string(root.join("a.md")).unwrap(),
            "foo, edited elsewhere\n"
        );
    }

    #[test]
    fn invalid_utf8_notes_are_listed_not_planned() {
        let (vault, root) = scratch_vault("replace-utf8", &[("a.md", "foo\n")]);
        std::fs::write(root.join("a.md"), b"foo \xff\xfe\n").unwrap();
        let m = Matcher::from_query("foo", &options()).unwrap();
        let plan = build(&vault, &ids(&vault), &m, "bar", &CancelToken::new());
        assert!(plan.notes.is_empty());
        assert_eq!(plan.unreadable, ["a.md"]);
    }
}
