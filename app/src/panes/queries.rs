use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::ThreadId;

use focus_search::{CancelToken, DiagnosticKind, SearchOptions, parse_query};
use focus_view_reading::query::{
    QUERY_LINES, QUERY_NOTES, QueryKey, QueryLine, QueryNote, QueryOutcome, QueryResult,
    QuerySearch, set_query_search,
};

use focus_core::{NoteId, Vault};
use focus_search::SearchIndex;

use crate::library::Library;

const KEPT: usize = 64;

struct IndexQueries {
    vault: Arc<Vault>,
    index: Arc<SearchIndex>,
    ui: ThreadId,
    kept: Mutex<HashMap<QueryKey, Arc<QueryOutcome>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl IndexQueries {
    fn run(&self, source: &str, holder: Option<NoteId>) -> QueryResult {
        let query = parse_query(source);
        if let Some(problem) = query
            .diagnostics
            .iter()
            .find(|d| !matches!(d.kind, DiagnosticKind::UnknownOperator { .. }))
        {
            return QueryResult::Invalid(focus_view_search::diagnostic_message(&problem.kind));
        }
        let options = SearchOptions {
            max_files: QUERY_NOTES + 1,
            max_matches_per_file: QUERY_LINES,
            ..Default::default()
        };
        let found = self.index.search(&query, &options, &CancelToken::new());
        let held = found.hits.iter().find(|hit| Some(hit.note) == holder);
        let total_notes = found.total_files - usize::from(held.is_some());
        let total_matches = found.total_matches
            - held.map_or(0, |hit| {
                hit.match_count.max(hit.matches.len() as u32) as usize
            });
        let notes = found
            .hits
            .iter()
            .filter(|hit| Some(hit.note) != holder)
            .take(QUERY_NOTES)
            .filter_map(|hit| {
                let meta = self.vault.note(hit.note)?;
                Some(QueryNote {
                    note: hit.note,
                    title: meta.title.to_string(),
                    folder: meta
                        .path
                        .parent()
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_default(),
                    count: hit.match_count.max(hit.matches.len() as u32),
                    lines: hit
                        .matches
                        .iter()
                        .map(|m| QueryLine {
                            line: m.line,
                            text: m.text.clone(),
                            ranges: m.ranges.clone(),
                        })
                        .collect(),
                })
            })
            .collect();
        QueryResult::Found {
            notes,
            total_notes,
            total_matches,
        }
    }
}

impl QuerySearch for IndexQueries {
    fn generation(&self) -> u64 {
        self.index.generation() + focus_i18n::generation()
    }

    fn results(&self, source: &str, note: Option<NoteId>) -> Option<Arc<QueryOutcome>> {
        let generation = self.generation();
        let key = (source.to_string(), note);
        let kept = lock(&self.kept).get(&key).cloned();
        if kept.as_ref().is_some_and(|k| k.generation == generation)
            || std::thread::current().id() == self.ui
        {
            return kept;
        }
        let outcome = Arc::new(QueryOutcome {
            generation,
            result: self.run(source, note),
        });
        let mut all = lock(&self.kept);
        if all.len() >= KEPT {
            all.clear();
        }
        all.insert(key, outcome.clone());
        Some(outcome)
    }
}

thread_local! {
    static REGISTERED: Cell<usize> = const { Cell::new(0) };
}

pub fn register(library: Option<&Arc<Library>>) {
    let address = library.map_or(0, |l| Arc::as_ptr(l.index()) as usize);
    if REGISTERED.with(|r| r.replace(address)) == address {
        return;
    }
    set_query_search(library.map(|library| {
        Arc::new(IndexQueries {
            vault: library.vault().clone(),
            index: library.index().clone(),
            ui: std::thread::current().id(),
            kept: Mutex::new(HashMap::new()),
        }) as Arc<dyn QuerySearch>
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use focus_core::OpenOptions;
    use std::path::PathBuf;

    fn queries(name: &str, notes: &[(&str, &str)], ui: ThreadId) -> IndexQueries {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/test-vaults")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("vault");
        for (path, text) in notes {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        let vault = Vault::open(
            &root,
            OpenOptions {
                cache_dir: dir.join("cache"),
                ignore: Vec::new(),
                watch: false,
            },
        )
        .unwrap();
        let index = SearchIndex::open(vault.clone(), &dir.join("cache"), &|_| {}).unwrap();
        IndexQueries {
            vault,
            index,
            ui,
            kept: Mutex::new(HashMap::new()),
        }
    }

    fn elsewhere() -> ThreadId {
        std::thread::spawn(|| std::thread::current().id())
            .join()
            .unwrap()
    }

    #[test]
    fn a_query_finds_notes_and_their_lines() {
        let q = queries(
            "query-found",
            &[
                (
                    "Red/kerberos.md",
                    "# Kerberos\n\nTickets and kerberos again.\n",
                ),
                ("other.md", "Nothing here.\n"),
            ],
            elsewhere(),
        );
        let outcome = q.results("kerberos", None).unwrap();
        let QueryResult::Found {
            notes,
            total_notes,
            total_matches,
        } = &outcome.result
        else {
            panic!("{outcome:?}");
        };
        assert_eq!((*total_notes, *total_matches), (1, 2));
        assert_eq!(notes[0].folder, "Red");
        assert_eq!(notes[0].count, 2);
        let lines: Vec<u32> = notes[0].lines.iter().map(|l| l.line).collect();
        assert_eq!(lines, [0, 2]);
        assert!(
            Arc::ptr_eq(&outcome, &q.results("kerberos", None).unwrap()),
            "kept"
        );
        let holder = q
            .vault
            .note_by_path(std::path::Path::new("Red/kerberos.md"));
        assert_eq!(
            q.results("kerberos", holder).unwrap().result,
            QueryResult::Found {
                notes: Vec::new(),
                total_notes: 0,
                total_matches: 0
            },
            "the note that holds the block is left out"
        );
    }

    #[test]
    fn no_result_and_an_invalid_query() {
        let q = queries("query-none", &[("a.md", "alpha\n")], elsewhere());
        assert_eq!(
            q.results("zulu", None).unwrap().result,
            QueryResult::Found {
                notes: Vec::new(),
                total_notes: 0,
                total_matches: 0
            }
        );
        assert!(matches!(
            q.results("\"alpha", None).unwrap().result,
            QueryResult::Invalid(_)
        ));
        assert!(matches!(
            q.results("unknown:alpha", None).unwrap().result,
            QueryResult::Found { .. }
        ));
    }

    #[test]
    fn notes_past_the_bound_are_counted() {
        let notes: Vec<(String, String)> = (0..QUERY_NOTES + 5)
            .map(|i| (format!("n{i}.md"), "kerberos\n".repeat(QUERY_LINES + 2)))
            .collect();
        let notes: Vec<(&str, &str)> = notes
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect();
        let q = queries("query-bound", &notes, elsewhere());
        let outcome = q.results("kerberos", None).unwrap();
        let QueryResult::Found {
            notes, total_notes, ..
        } = &outcome.result
        else {
            panic!("{outcome:?}");
        };
        assert_eq!((notes.len(), *total_notes), (QUERY_NOTES, QUERY_NOTES + 5));
        assert!(notes.iter().all(|n| n.lines.len() == QUERY_LINES));
        assert!(notes.iter().all(|n| n.count as usize == QUERY_LINES + 2));
    }

    #[test]
    fn the_interface_thread_never_searches() {
        let q = queries(
            "query-ui",
            &[("a.md", "alpha\n")],
            std::thread::current().id(),
        );
        assert_eq!(q.results("alpha", None), None);
    }
}
