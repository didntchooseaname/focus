mod exec;
mod fold;
mod fuzzy;
mod index;
mod mentions;
mod prefilter;
mod query;

use focus_core::{NoteId, Vault, VaultEvent};
pub use mentions::{
    LinkMentionError, LinkPath, Mention, MentionHit, MentionOptions, Mentions, link_mention,
    mention_link, write_mention_link,
};
use parking_lot::{Mutex, RwLock};
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Default)]
pub struct IndexStats {
    pub notes: usize,
    pub trigrams: usize,
    pub bytes_on_disk: u64,
    pub bytes_in_memory: u64,
    pub build_ms: f32,
    pub reindexed: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct IndexProgress {
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone)]
pub struct Query {
    pub source: String,
    pub root: QueryNode,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticKind {
    UnclosedQuote,
    InvalidRegex { message: String },
    UnknownOperator { name: String },
    MissingValue { operator: String },
    UnclosedParenthesis,
    UnmatchedParenthesis,
}

#[derive(Debug, Clone)]
pub enum QueryNode {
    And(Vec<QueryNode>),
    Or(Vec<QueryNode>),
    Not(Box<QueryNode>),
    Term { field: Field, pattern: Pattern },
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Any,
    Content,
    Title,
    Heading,
    Tag,
    Path,
    File,
    Task { done: Option<bool> },
    Prop,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Text(String),
    Phrase(String),
    Regex(String),
    KeyValue(String, String),
}

pub fn parse_query(input: &str) -> Query {
    query::parse(input)
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub accent_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub max_files: usize,
    pub max_matches_per_file: usize,
    pub sort: SortOrder,
    pub budget_ms: u32,
}

impl Default for SearchOptions {
    fn default() -> Self {
        SearchOptions {
            case_sensitive: false,
            accent_sensitive: false,
            whole_word: false,
            regex: false,
            max_files: 500,
            max_matches_per_file: 20,
            sort: SortOrder::Relevance,
            budget_ms: 40,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Relevance,
    Path,
    ModifiedNewest,
    ModifiedOldest,
}

#[derive(Debug, Clone)]
pub struct LineMatch {
    pub line: u32,
    pub text: String,
    pub ranges: Vec<Range<usize>>,
}

#[derive(Debug, Clone)]
pub struct FileHit {
    pub note: NoteId,
    pub score: f32,
    pub title_ranges: Vec<Range<usize>>,
    pub matches: Vec<LineMatch>,
    pub match_count: u32,
    pub explain: Vec<(&'static str, f32)>,
}

#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    pub hits: Vec<FileHit>,
    pub total_files: usize,
    pub total_matches: usize,
    pub elapsed_ms: f32,
    pub truncated: bool,
    pub cancelled: bool,
    pub totals_are_minimums: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuzzyScope {
    Notes,
    Tags,
    Headings(NoteId),
}

#[derive(Debug, Clone)]
pub struct FuzzyHit {
    pub note: Option<NoteId>,
    pub text: String,
    pub positions: Vec<u32>,
    pub score: u32,
    pub line: Option<u32>,
}

pub struct SearchIndex {
    vault: Arc<Vault>,
    cache_path: PathBuf,
    postings: RwLock<index::Postings>,
    pending: Mutex<HashSet<NoteId>>,
    dirty: AtomicBool,
    generation: AtomicU64,
    catalog: Arc<Mutex<Option<Versioned>>>,
    refreshing: Arc<AtomicBool>,
    last_fuzzy: Mutex<fuzzy::LastRun>,
    build_ms: f32,
    reindexed: usize,
}

type Versioned = (u64, Arc<exec::Catalog>);

fn release_free_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: `malloc_trim` only returns free arena pages to the system.
        let spawned = std::thread::Builder::new()
            .name("focus-trim".into())
            .spawn(|| unsafe {
                libc::malloc_trim(0);
            });
        drop(spawned);
    }
}

fn stamp(path: &Path, size: u64, modified: std::time::SystemTime) -> index::DocStamp {
    index::DocStamp {
        path_hash: index::path_hash(path),
        size,
        modified,
    }
}

impl SearchIndex {
    pub fn open(
        vault: Arc<Vault>,
        cache_dir: &Path,
        progress: &(dyn Fn(IndexProgress) + Sync),
    ) -> anyhow::Result<Arc<SearchIndex>> {
        let started = Instant::now();
        let cache_path = cache_dir.join(index::FILE_NAME);
        let cached = std::fs::File::open(&cache_path)
            .ok()
            .and_then(|f| index::Postings::open_file(vault.root(), &f));
        let had_cache = cached.is_some();
        let mut postings = cached.unwrap_or_default();
        let summaries = vault.summaries();
        let current: std::collections::HashMap<NoteId, index::DocStamp> = summaries
            .iter()
            .map(|m| (m.id, stamp(&m.path, m.size, m.modified)))
            .collect();
        let stale: roaring::RoaringBitmap = postings
            .docs
            .iter()
            .filter(|(id, d)| current.get(id) != Some(*d))
            .map(|(id, _)| *id)
            .collect();
        postings.remove(&stale);
        let todo: Vec<(NoteId, index::DocStamp)> = current
            .into_iter()
            .filter(|(id, _)| !postings.docs.contains_key(id))
            .collect();
        let reindexed = todo.len();
        postings.add(&vault, todo, progress);
        let mut dirty = had_cache && (reindexed > 0 || !stale.is_empty());
        if !had_cache {
            dirty = postings.write_and_remap(vault.root(), &cache_path).is_err();
            release_free_memory();
        }
        let catalog_notes = summaries;
        Ok(Arc::new(SearchIndex {
            vault,
            cache_path,
            postings: RwLock::new(postings),
            pending: Mutex::new(HashSet::new()),
            dirty: AtomicBool::new(dirty),
            generation: AtomicU64::new(0),
            catalog: Arc::new(Mutex::new(Some((
                0,
                Arc::new(exec::Catalog::new(catalog_notes)),
            )))),
            refreshing: Arc::new(AtomicBool::new(false)),
            last_fuzzy: Mutex::new(fuzzy::LastRun::default()),
            build_ms: started.elapsed().as_secs_f32() * 1000.0,
            reindexed,
        }))
    }

    pub fn apply(&self, event: &VaultEvent) {
        let id = match event {
            VaultEvent::NoteAdded(id) | VaultEvent::NoteChanged(id) => *id,
            VaultEvent::NoteRemoved { id, .. } | VaultEvent::NoteMoved { id, .. } => *id,
            VaultEvent::TreeChanged => return,
        };
        self.pending.lock().insert(id);
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    fn flush(&self) {
        let ids: Vec<NoteId> = {
            let mut p = self.pending.lock();
            if p.is_empty() {
                return;
            }
            p.drain().collect()
        };
        let mut postings = self.postings.write();
        let stale: roaring::RoaringBitmap = ids
            .iter()
            .copied()
            .filter(|id| postings.docs.contains_key(id))
            .collect();
        postings.remove(&stale);
        let todo = ids
            .iter()
            .filter_map(|&id| {
                self.vault
                    .note(id)
                    .map(|m| (id, stamp(&m.path, m.size, m.modified)))
            })
            .collect();
        postings.add(&self.vault, todo, &|_| {});
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub fn persist(&self) -> anyhow::Result<()> {
        self.flush();
        let mut postings = self.postings.write();
        if postings.is_corrupt() {
            let mut fresh = index::Postings::default();
            let all = self
                .vault
                .summaries()
                .iter()
                .map(|m| (m.id, stamp(&m.path, m.size, m.modified)))
                .collect();
            fresh.add(&self.vault, all, &|_| {});
            *postings = fresh;
            self.dirty.store(true, Ordering::Relaxed);
        }
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return Ok(());
        }
        postings
            .write_and_remap(self.vault.root(), &self.cache_path)
            .map_err(|e| {
                self.dirty.store(true, Ordering::Relaxed);
                anyhow::anyhow!("writing index {}: {e}", self.cache_path.display())
            })
    }

    fn catalog(&self) -> Arc<exec::Catalog> {
        let generation = self.generation.load(Ordering::Relaxed);
        if let Some((g, c)) = self.catalog.lock().as_ref()
            && *g == generation
        {
            return c.clone();
        }
        let c = Arc::new(exec::Catalog::new(self.vault.summaries()));
        let mut slot = self.catalog.lock();
        if slot.as_ref().is_none_or(|(g, _)| *g <= generation) {
            *slot = Some((generation, c.clone()));
        }
        c
    }

    fn catalog_for_palette(&self) -> Arc<exec::Catalog> {
        let generation = self.generation.load(Ordering::Relaxed);
        let current = self.catalog.lock().clone();
        match current {
            Some((g, c)) if g == generation => c,
            Some((_, c)) => {
                if !self.refreshing.swap(true, Ordering::AcqRel) {
                    let (vault, slot, flag) = (
                        self.vault.clone(),
                        self.catalog.clone(),
                        self.refreshing.clone(),
                    );
                    let spawned = std::thread::Builder::new()
                        .name("focus-catalog".into())
                        .spawn(move || {
                            let c = Arc::new(exec::Catalog::new(vault.summaries()));
                            let mut s = slot.lock();
                            if s.as_ref().is_none_or(|(g, _)| *g < generation) {
                                *s = Some((generation, c));
                            }
                            flag.store(false, Ordering::Release);
                        });
                    if spawned.is_err() {
                        self.refreshing.store(false, Ordering::Release);
                        return self.catalog();
                    }
                }
                c
            }
            None => self.catalog(),
        }
    }

    pub fn search(
        &self,
        query: &Query,
        options: &SearchOptions,
        cancel: &CancelToken,
    ) -> SearchResults {
        let started = Instant::now();
        self.flush();
        let root = if options.regex {
            QueryNode::Term {
                field: Field::Any,
                pattern: Pattern::Regex(query.source.clone()),
            }
        } else {
            query.root.clone()
        };
        let compiled = exec::compile(&root, options, &mut 0);
        if matches!(compiled, exec::Node::Empty) {
            return SearchResults {
                elapsed_ms: started.elapsed().as_secs_f32() * 1000.0,
                ..Default::default()
            };
        }
        let catalog = self.catalog();
        let postings = self.postings.read();
        exec::run(
            &self.vault,
            &postings,
            &catalog,
            compiled,
            options,
            cancel,
            started,
        )
    }

    pub fn unlinked_mentions(
        &self,
        note: NoteId,
        options: &MentionOptions,
        cancel: &CancelToken,
    ) -> Mentions {
        let started = Instant::now();
        self.flush();
        let catalog = self.catalog();
        let postings = self.postings.read();
        mentions::run(
            &self.vault,
            &postings,
            &catalog,
            note,
            options,
            cancel,
            started,
        )
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    pub fn fuzzy(
        &self,
        pattern: &str,
        scope: FuzzyScope,
        recent: &[NoteId],
        limit: usize,
    ) -> Vec<FuzzyHit> {
        match scope {
            FuzzyScope::Notes => {
                let catalog = self.catalog_for_palette();
                let mut last = self.last_fuzzy.lock();
                fuzzy::notes(&catalog, pattern, recent, limit, &mut last)
            }
            other => fuzzy::other(&self.vault, other, pattern, limit),
        }
    }

    pub fn stats(&self) -> IndexStats {
        let postings = self.postings.read();
        IndexStats {
            notes: postings.docs.len(),
            trigrams: postings.trigram_count(),
            bytes_on_disk: std::fs::metadata(&self.cache_path)
                .map(|m| m.len())
                .unwrap_or(0),
            bytes_in_memory: postings.memory_bytes(),
            build_ms: self.build_ms,
            reindexed: self.reindexed,
        }
    }
}

#[cfg(test)]
mod tests;
