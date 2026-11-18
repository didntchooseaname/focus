use std::ops::Range;
use std::sync::{Arc, RwLock};

use focus_core::NoteId;

pub const QUERY_NOTES: usize = 20;
pub const QUERY_LINES: usize = 5;

#[derive(Clone, Debug, PartialEq)]
pub struct QueryLine {
    pub line: u32,
    pub text: String,
    pub ranges: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryNote {
    pub note: NoteId,
    pub title: String,
    pub folder: String,
    pub count: u32,
    pub lines: Vec<QueryLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QueryResult {
    Found {
        notes: Vec<QueryNote>,
        total_notes: usize,
        total_matches: usize,
    },
    Invalid(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryOutcome {
    pub generation: u64,
    pub result: QueryResult,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QueryState {
    Unavailable,
    Pending,
    Ready(Arc<QueryOutcome>),
}

pub type QueryKey = (String, Option<NoteId>);

pub trait QuerySearch: Send + Sync {
    fn generation(&self) -> u64;
    fn results(&self, source: &str, note: Option<NoteId>) -> Option<Arc<QueryOutcome>>;
}

static SEARCH: RwLock<Option<Arc<dyn QuerySearch>>> = RwLock::new(None);

pub fn set_query_search(search: Option<Arc<dyn QuerySearch>>) {
    *SEARCH
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = search;
}

pub fn query_search() -> Option<Arc<dyn QuerySearch>> {
    SEARCH
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

pub fn search_state(source: &str, note: Option<NoteId>) -> QueryState {
    match query_search() {
        None => QueryState::Unavailable,
        Some(search) => match search.results(source, note) {
            Some(outcome) => QueryState::Ready(outcome),
            None => QueryState::Pending,
        },
    }
}

pub fn is_query(lang: &str) -> bool {
    lang.trim().eq_ignore_ascii_case("query")
}

#[derive(Clone, Debug)]
pub struct QueryBlock {
    pub source: String,
    pub line: u32,
    pub note: Option<NoteId>,
    pub state: QueryState,
}

impl QueryBlock {
    pub fn new(
        code: &str,
        line: u32,
        note: Option<NoteId>,
        library: &dyn crate::Resolver,
    ) -> QueryBlock {
        let source = code.trim().to_string();
        let state = library.query_results(&source, note);
        QueryBlock {
            source,
            line,
            note,
            state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::Resolver;
    use crate::model::Node;
    use focus_core::LinkTarget;
    use std::path::{Path, PathBuf};

    struct Plain;

    impl Resolver for Plain {
        fn resolve_link(&self, _: NoteId, _: &str) -> LinkTarget {
            LinkTarget::Unresolved
        }
        fn absolute(&self, relative: &Path) -> PathBuf {
            relative.to_path_buf()
        }
    }

    struct Fixed;

    impl Resolver for Fixed {
        fn resolve_link(&self, _: NoteId, _: &str) -> LinkTarget {
            LinkTarget::Unresolved
        }
        fn absolute(&self, relative: &Path) -> PathBuf {
            relative.to_path_buf()
        }
        fn query_results(&self, source: &str, _: Option<NoteId>) -> QueryState {
            match source {
                "tag:#red" => QueryState::Ready(Arc::new(QueryOutcome {
                    generation: 3,
                    result: QueryResult::Invalid("why".into()),
                })),
                _ => QueryState::Pending,
            }
        }
    }

    fn blocks(text: &str, library: &dyn Resolver) -> Vec<(String, u32, QueryState)> {
        crate::model::build_text(text, None, library, None)
            .items
            .iter()
            .filter_map(|item| match &item.node {
                Node::Query(block) => Some((block.source.clone(), block.line, block.state.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_query_block_holds_what_the_resolver_gives() {
        let text = "Text\n\n```query\n  tag:#red \n```\n\n~~~ Query\n~~~\n\n```rust\nquery\n```\n";
        let found = blocks(text, &Fixed);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!((found[0].0.as_str(), found[0].1), ("tag:#red", 2));
        assert!(matches!(&found[0].2, QueryState::Ready(o) if o.generation == 3));
        assert_eq!(
            (found[1].0.as_str(), &found[1].2),
            ("", &QueryState::Pending)
        );
        assert_eq!(blocks(text, &Plain)[0].2, QueryState::Unavailable);
    }

    #[test]
    fn the_language_is_query_in_any_case() {
        assert!(is_query("query") && is_query(" Query "));
        assert!(!is_query("queryx") && !is_query(""));
    }
}
