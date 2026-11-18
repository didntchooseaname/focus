use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use focus_ai::assist::{self, Candidate, Expansion, Ranked, VaultHint};
use focus_ai::redact::{self, Replacement};
use focus_ai::{Cancel, Client, Error, Message, Role, StreamEvent, Usage};
use focus_core::NoteId;
use focus_search::{CancelToken, FileHit, LineMatch, SearchOptions, SearchResults};

const MAX_CANDIDATES: usize = 40;
const MAX_EXCERPTS: usize = 3;
const MAX_REASON_CHARS: usize = 400;

pub trait AssistBackend: Send + Sync {
    fn model(&self) -> String;
    fn expand(
        &self,
        question: &str,
        hint: &VaultHint,
        cancel: &Cancel,
    ) -> Result<(Expansion, Usage), Error>;
    fn rerank(
        &self,
        question: &str,
        candidates: &[Candidate],
        cancel: &Cancel,
    ) -> Result<(Vec<Ranked>, Usage), Error>;
    fn answer(
        &self,
        question: &str,
        candidates: &[Candidate],
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<Message, Error>;
}

pub struct LiveBackend {
    client: Client,
}

impl LiveBackend {
    pub fn new(client: Client) -> Self {
        LiveBackend { client }
    }
}

impl AssistBackend for LiveBackend {
    fn model(&self) -> String {
        self.client.config().model.clone()
    }
    fn expand(
        &self,
        question: &str,
        hint: &VaultHint,
        cancel: &Cancel,
    ) -> Result<(Expansion, Usage), Error> {
        assist::expand(&self.client, question, hint, cancel)
    }
    fn rerank(
        &self,
        question: &str,
        candidates: &[Candidate],
        cancel: &Cancel,
    ) -> Result<(Vec<Ranked>, Usage), Error> {
        assist::rerank(&self.client, question, candidates, cancel)
    }
    fn answer(
        &self,
        question: &str,
        candidates: &[Candidate],
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<Message, Error> {
        assist::answer(&self.client, question, candidates, cancel, on_event)
    }
}

pub struct Script {
    pub keyword: String,
    pub queries: Vec<String>,
    pub intent: String,
    pub reasons: Vec<String>,
    pub answer: String,
}

pub struct ScriptedBackend {
    scripts: Vec<Script>,
    chunk_delay: Duration,
    failure: Option<fn() -> Error>,
}

impl ScriptedBackend {
    pub fn new(scripts: Vec<Script>) -> Self {
        ScriptedBackend {
            scripts,
            chunk_delay: Duration::from_millis(25),
            failure: None,
        }
    }

    pub fn with_chunk_delay(mut self, delay: Duration) -> Self {
        self.chunk_delay = delay;
        self
    }

    pub fn failing(make: fn() -> Error) -> Self {
        ScriptedBackend {
            scripts: Vec::new(),
            chunk_delay: Duration::ZERO,
            failure: Some(make),
        }
    }

    pub fn demo() -> Self {
        ScriptedBackend::new(vec![Script {
            keyword: "kerberoast".into(),
            queries: vec!["kerberoast".into(), "SPN".into(), "GetUserSPNs".into(), "\"service ticket\" hashcat".into(), "tag:#red-team TGS-REP".into()],
            intent: "How a Kerberoasting attack works and how to run it".into(),
            reasons: vec![
                "Explains the request of service tickets for accounts with an SPN".into(),
                "Lists the tools and commands used to extract the hashes".into(),
                "Covers the offline cracking step with hashcat".into(),
                "Mentions detection through event 4769".into(),
            ],
            answer: "Kerberoasting requests service tickets for accounts that have a service principal name, then cracks them offline [1][2]. The hashes come from the TGS-REP and are cracked with hashcat mode 13100 [3]. Defenders watch for many 4769 events with RC4 encryption [4].".into(),
        }])
    }

    fn script_for(&self, question: &str) -> Option<&Script> {
        let lower = question.to_lowercase();
        self.scripts
            .iter()
            .find(|s| lower.contains(&s.keyword))
            .or(self.scripts.first())
    }

    fn fail(&self) -> Result<(), Error> {
        match self.failure {
            Some(make) => Err(make()),
            None => Ok(()),
        }
    }
}

impl AssistBackend for ScriptedBackend {
    fn model(&self) -> String {
        focus_ai::MODEL_FLASH.to_string()
    }

    fn expand(
        &self,
        question: &str,
        _: &VaultHint,
        _: &Cancel,
    ) -> Result<(Expansion, Usage), Error> {
        self.fail()?;
        let expansion = self
            .script_for(question)
            .map(|s| Expansion {
                queries: s.queries.clone(),
                intent: s.intent.clone(),
            })
            .unwrap_or_default();
        Ok((
            expansion,
            Usage {
                prompt_tokens: 420,
                completion_tokens: 96,
                cache_miss_tokens: 420,
                ..Usage::default()
            },
        ))
    }

    fn rerank(
        &self,
        question: &str,
        candidates: &[Candidate],
        _: &Cancel,
    ) -> Result<(Vec<Ranked>, Usage), Error> {
        self.fail()?;
        let usage = Usage {
            prompt_tokens: 1800,
            completion_tokens: 140,
            cache_miss_tokens: 1800,
            ..Usage::default()
        };
        let Some(script) = self.script_for(question) else {
            return Ok((Vec::new(), usage));
        };
        let ranked = candidates
            .iter()
            .zip(&script.reasons)
            .enumerate()
            .map(|(i, (candidate, reason))| Ranked {
                index: candidate.index,
                relevance: 1.0 - i as f32 * 0.15,
                reason: reason.clone(),
            })
            .collect();
        Ok((ranked, usage))
    }

    fn answer(
        &self,
        question: &str,
        candidates: &[Candidate],
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<Message, Error> {
        self.fail()?;
        let text = self
            .script_for(question)
            .map(|s| strip_missing_citations(&s.answer, candidates.len()))
            .unwrap_or_default();
        let mut sent = String::new();
        for chunk in text.split_inclusive(' ') {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            std::thread::sleep(self.chunk_delay);
            sent.push_str(chunk);
            on_event(StreamEvent::Content(chunk.to_string()));
        }
        on_event(StreamEvent::Usage(Usage {
            prompt_tokens: 2400,
            completion_tokens: 90,
            cache_miss_tokens: 2400,
            ..Usage::default()
        }));
        on_event(StreamEvent::Done {
            finish_reason: "stop".into(),
        });
        Ok(Message {
            role: Role::Assistant,
            content: sent,
            reasoning: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        })
    }
}

fn strip_missing_citations(text: &str, available: usize) -> String {
    let mut out = text.to_string();
    for (range, number) in assist::citations(text).into_iter().rev() {
        if number as usize > available {
            out.replace_range(range, "");
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    MissingKey,
    InvalidKey,
    Balance,
    RateLimited { retry_after_secs: u32 },
    Network,
    Other,
}

pub fn classify(error: &Error) -> Failure {
    match error {
        Error::MissingKey => Failure::MissingKey,
        Error::InvalidKey => Failure::InvalidKey,
        Error::InsufficientBalance => Failure::Balance,
        Error::RateLimited { retry_after_secs } => Failure::RateLimited {
            retry_after_secs: *retry_after_secs,
        },
        Error::Network(_) | Error::Server { .. } => Failure::Network,
        _ => Failure::Other,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Expanding,
    Searching,
    Ranking,
    Answering,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub note: NoteId,
    pub line: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum Progress {
    Stage(Stage),
    Expanded {
        intent: String,
        queries: Vec<String>,
    },
    Merged(Vec<FileHit>),
    Ranked(Vec<(NoteId, String)>),
    Sources(Vec<Source>),
    Replacements(Vec<Replacement>),
    Chunk(String),
    Failed(Failure),
    Done {
        cost_usd: Option<f64>,
    },
}

pub trait Searcher: Send + Sync {
    fn search(&self, query: &str, options: &SearchOptions, cancel: &CancelToken) -> SearchResults;
}

pub struct IndexSearcher(pub Arc<focus_search::SearchIndex>);

impl Searcher for IndexSearcher {
    fn search(&self, query: &str, options: &SearchOptions, cancel: &CancelToken) -> SearchResults {
        self.0
            .search(&focus_search::parse_query(query), options, cancel)
    }
}

pub type NoteLookup = dyn Fn(NoteId) -> Option<(String, String)> + Send + Sync;

pub struct Pipeline {
    pub backend: Arc<dyn AssistBackend>,
    pub searcher: Arc<dyn Searcher>,
    pub lookup: Arc<NoteLookup>,
    pub options: SearchOptions,
    pub redaction: redact::Options,
    pub hint: VaultHint,
}

pub struct Run {
    pub question: String,
    pub local: Vec<FileHit>,
    pub queries: Option<Vec<String>>,
}

impl Pipeline {
    pub fn run(&self, run: Run, cancel: &Cancel, progress: &mut dyn FnMut(Progress)) {
        match self.execute(run, cancel, progress) {
            Ok(()) | Err(Error::Cancelled) => {}
            Err(error) => progress(Progress::Failed(classify(&error))),
        }
    }

    fn execute(
        &self,
        run: Run,
        cancel: &Cancel,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<(), Error> {
        let mut known: Vec<Replacement> = Vec::new();
        let question = mask(&run.question, &self.redaction, &mut known);
        let mut used = Usage::default();

        let queries = match run.queries {
            Some(queries) => queries,
            None => {
                progress(Progress::Stage(Stage::Expanding));
                let (expansion, usage) = self.backend.expand(&question, &self.hint, cancel)?;
                add_usage(&mut used, &usage);
                let queries: Vec<String> = expansion
                    .queries
                    .iter()
                    .map(|q| redact::restore(q, &known))
                    .collect();
                progress(Progress::Expanded {
                    intent: redact::restore(&expansion.intent, &known),
                    queries: queries.clone(),
                });
                queries
            }
        };
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }

        progress(Progress::Stage(Stage::Searching));
        let token = CancelToken::new();
        let mut merged = run.local;
        for query in &queries {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let results = self.searcher.search(query, &self.options, &token);
            merged = merge(
                merged,
                results.hits,
                self.options.max_matches_per_file,
                self.options.max_files,
            );
        }
        progress(Progress::Merged(merged.clone()));
        if merged.is_empty() {
            progress(Progress::Done {
                cost_usd: self.cost(&used),
            });
            return Ok(());
        }

        progress(Progress::Stage(Stage::Ranking));
        let candidates = self.candidates(&merged[..merged.len().min(MAX_CANDIDATES)], &mut known);
        let (ranked, usage) = self.backend.rerank(&question, &candidates, cancel)?;
        add_usage(&mut used, &usage);
        let order: Vec<NoteId> = ranked
            .iter()
            .filter_map(|r| candidates.iter().find(|c| c.index == r.index))
            .filter_map(|c| merged.get(c.index as usize - 1))
            .map(|h| h.note)
            .collect();
        let reasons: Vec<(NoteId, String)> = ranked
            .iter()
            .filter_map(|r| {
                let hit = merged.get(r.index as usize - 1)?;
                let reason: String = redact::restore(&r.reason, &known)
                    .chars()
                    .take(MAX_REASON_CHARS)
                    .collect();
                Some((hit.note, reason))
            })
            .collect();
        progress(Progress::Ranked(reasons));

        let cited: Vec<&FileHit> = if order.is_empty() {
            merged.iter().take(8).collect()
        } else {
            order
                .iter()
                .filter_map(|id| merged.iter().find(|h| h.note == *id))
                .collect()
        };
        let cited: Vec<FileHit> = cited.into_iter().cloned().collect();
        let answer_candidates = self.candidates(&cited, &mut known);
        progress(Progress::Sources(
            cited
                .iter()
                .map(|h| Source {
                    note: h.note,
                    line: h.matches.first().map(|m| m.line),
                })
                .collect(),
        ));
        progress(Progress::Replacements(known.clone()));
        progress(Progress::Stage(Stage::Answering));
        self.backend.answer(
            &question,
            &answer_candidates,
            cancel,
            &mut |event| match event {
                StreamEvent::Content(text) => progress(Progress::Chunk(text)),
                StreamEvent::Usage(u) => add_usage(&mut used, &u),
                _ => {}
            },
        )?;
        progress(Progress::Done {
            cost_usd: self.cost(&used),
        });
        Ok(())
    }

    fn cost(&self, usage: &Usage) -> Option<f64> {
        focus_ai::estimate_cost_usd(&self.backend.model(), usage)
    }

    fn candidates(&self, hits: &[FileHit], known: &mut Vec<Replacement>) -> Vec<Candidate> {
        hits.iter()
            .enumerate()
            .filter_map(|(i, hit)| {
                let (title, path) = (self.lookup)(hit.note)?;
                Some(Candidate {
                    index: i as u32 + 1,
                    title: mask(&title, &self.redaction, known),
                    path: mask(&path, &self.redaction, known),
                    excerpts: hit
                        .matches
                        .iter()
                        .take(MAX_EXCERPTS)
                        .map(|m| mask(&m.text, &self.redaction, known))
                        .collect(),
                })
            })
            .collect()
    }
}

fn mask(text: &str, options: &redact::Options, known: &mut Vec<Replacement>) -> String {
    let redacted = redact::redact(text, options, known);
    for replacement in redacted.replacements {
        if !known.iter().any(|k| k.token == replacement.token) {
            known.push(replacement);
        }
    }
    redacted.text
}

fn add_usage(total: &mut Usage, other: &Usage) {
    total.prompt_tokens += other.prompt_tokens;
    total.completion_tokens += other.completion_tokens;
    total.reasoning_tokens += other.reasoning_tokens;
    total.cache_hit_tokens += other.cache_hit_tokens;
    total.cache_miss_tokens += other.cache_miss_tokens;
}

pub fn merge(
    base: Vec<FileHit>,
    extra: Vec<FileHit>,
    max_matches: usize,
    max_files: usize,
) -> Vec<FileHit> {
    let mut by_note: HashMap<NoteId, FileHit> = HashMap::new();
    let mut order: Vec<NoteId> = Vec::new();
    for hit in base.into_iter().chain(extra) {
        match by_note.get_mut(&hit.note) {
            Some(existing) => {
                existing.score = existing.score.max(hit.score) + 0.05;
                existing.match_count += hit.match_count;
                for m in hit.matches {
                    if !existing.matches.iter().any(|e| e.line == m.line) {
                        existing.matches.push(m);
                    }
                }
                if existing.title_ranges.is_empty() {
                    existing.title_ranges = hit.title_ranges;
                }
            }
            None => {
                order.push(hit.note);
                by_note.insert(hit.note, hit);
            }
        }
    }
    let mut hits: Vec<FileHit> = order
        .into_iter()
        .filter_map(|id| by_note.remove(&id))
        .collect();
    for hit in &mut hits {
        hit.matches.sort_by_key(|m: &LineMatch| m.line);
        hit.matches.truncate(max_matches);
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits.truncate(max_files);
    hits
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn hit(note: NoteId, score: f32, lines: &[u32]) -> FileHit {
        FileHit {
            note,
            score,
            title_ranges: Vec::new(),
            matches: lines
                .iter()
                .map(|&line| LineMatch {
                    line,
                    text: format!("line {line} of note {note} password=hunter2xyz"),
                    ranges: vec![0..4],
                })
                .collect(),
            match_count: lines.len() as u32,
            explain: Vec::new(),
        }
    }

    struct Fixed(Vec<FileHit>);
    impl Searcher for Fixed {
        fn search(&self, _: &str, _: &SearchOptions, _: &CancelToken) -> SearchResults {
            SearchResults {
                hits: self.0.clone(),
                total_files: self.0.len(),
                ..SearchResults::default()
            }
        }
    }

    fn pipeline(backend: ScriptedBackend, extra: Vec<FileHit>) -> Pipeline {
        Pipeline {
            backend: Arc::new(backend),
            searcher: Arc::new(Fixed(extra)),
            lookup: Arc::new(|id| Some((format!("Note {id}"), format!("folder/note{id}.md")))),
            options: SearchOptions::default(),
            redaction: redact::Options {
                secrets: true,
                ip_addresses: false,
                hostnames: false,
            },
            hint: VaultHint::default(),
        }
    }

    fn collect(pipeline: &Pipeline, run: Run) -> Vec<Progress> {
        let events = Mutex::new(Vec::new());
        pipeline.run(run, &Cancel::new(), &mut |p| events.lock().unwrap().push(p));
        events.into_inner().unwrap()
    }

    #[test]
    fn merge_unites_notes_and_lines() {
        let merged = merge(
            vec![hit(1, 1.0, &[3, 9]), hit(2, 0.5, &[1])],
            vec![hit(1, 0.4, &[9, 12]), hit(3, 2.0, &[7])],
            3,
            10,
        );
        assert_eq!(merged.iter().map(|h| h.note).collect::<Vec<_>>(), [3, 1, 2]);
        let first = merged.iter().find(|h| h.note == 1).unwrap();
        assert_eq!(
            first.matches.iter().map(|m| m.line).collect::<Vec<_>>(),
            [3, 9, 12]
        );
        assert_eq!(first.match_count, 4);
        assert_eq!(
            merge(vec![hit(1, 1.0, &[1, 2, 3, 4])], vec![], 2, 10)[0]
                .matches
                .len(),
            2
        );
        assert_eq!(
            merge(vec![hit(1, 1.0, &[1]), hit(2, 0.9, &[1])], vec![], 5, 1).len(),
            1
        );
    }

    #[test]
    fn full_run_expands_searches_ranks_and_answers() {
        let events = collect(
            &pipeline(
                ScriptedBackend::demo().with_chunk_delay(Duration::ZERO),
                vec![hit(5, 1.0, &[2]), hit(6, 0.8, &[4])],
            ),
            Run {
                question: "how does kerberoasting work".into(),
                local: vec![hit(7, 0.7, &[1])],
                queries: None,
            },
        );
        let stages: Vec<Stage> = events
            .iter()
            .filter_map(|e| {
                if let Progress::Stage(s) = e {
                    Some(*s)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            stages,
            [
                Stage::Expanding,
                Stage::Searching,
                Stage::Ranking,
                Stage::Answering
            ]
        );
        let Some(Progress::Expanded { queries, intent }) = events
            .iter()
            .find(|e| matches!(e, Progress::Expanded { .. }))
        else {
            panic!()
        };
        assert_eq!(queries.len(), 5);
        assert!(intent.contains("Kerberoasting"));
        let Some(Progress::Merged(merged)) =
            events.iter().find(|e| matches!(e, Progress::Merged(_)))
        else {
            panic!()
        };
        assert_eq!(merged.len(), 3);
        let Some(Progress::Ranked(reasons)) =
            events.iter().find(|e| matches!(e, Progress::Ranked(_)))
        else {
            panic!()
        };
        assert_eq!(reasons.len(), 3);
        let Some(Progress::Sources(sources)) =
            events.iter().find(|e| matches!(e, Progress::Sources(_)))
        else {
            panic!()
        };
        assert_eq!(sources.len(), 3);
        let answer: String = events
            .iter()
            .filter_map(|e| {
                if let Progress::Chunk(c) = e {
                    Some(c.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            answer.contains("[1][2]") && answer.contains("[3]") && !answer.contains("[4]"),
            "{answer}"
        );
        let Some(Progress::Done { cost_usd }) = events.last() else {
            panic!("{:?}", events.last())
        };
        let expected = Usage {
            prompt_tokens: 420 + 1800 + 2400,
            completion_tokens: 96 + 140 + 90,
            cache_miss_tokens: 420 + 1800 + 2400,
            ..Usage::default()
        };
        assert_eq!(
            *cost_usd,
            focus_ai::estimate_cost_usd(focus_ai::MODEL_FLASH, &expected),
            "cost is the sum of the three calls"
        );
    }

    #[test]
    fn edited_queries_skip_the_expansion() {
        let events = collect(
            &pipeline(
                ScriptedBackend::demo().with_chunk_delay(Duration::ZERO),
                vec![hit(5, 1.0, &[2])],
            ),
            Run {
                question: "kerberoast".into(),
                local: vec![],
                queries: Some(vec!["spn".into()]),
            },
        );
        assert!(!events.iter().any(|e| matches!(
            e,
            Progress::Expanded { .. } | Progress::Stage(Stage::Expanding)
        )));
        assert!(matches!(events.last(), Some(Progress::Done { .. })));
    }

    #[test]
    fn secrets_are_masked_in_candidates_and_restored_in_the_answer() {
        struct Echo(Mutex<Vec<String>>);
        impl AssistBackend for Echo {
            fn model(&self) -> String {
                focus_ai::MODEL_FLASH.into()
            }
            fn expand(
                &self,
                _: &str,
                _: &VaultHint,
                _: &Cancel,
            ) -> Result<(Expansion, Usage), Error> {
                Ok((Expansion::default(), Usage::default()))
            }
            fn rerank(
                &self,
                _: &str,
                c: &[Candidate],
                _: &Cancel,
            ) -> Result<(Vec<Ranked>, Usage), Error> {
                self.0
                    .lock()
                    .unwrap()
                    .extend(c.iter().flat_map(|c| c.excerpts.clone()));
                Ok((Vec::new(), Usage::default()))
            }
            fn answer(
                &self,
                _: &str,
                c: &[Candidate],
                _: &Cancel,
                on: &mut dyn FnMut(StreamEvent),
            ) -> Result<Message, Error> {
                on(StreamEvent::Content(c[0].excerpts[0].clone()));
                Ok(Message {
                    role: Role::Assistant,
                    content: String::new(),
                    reasoning: None,
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                })
            }
        }
        let echo = Arc::new(Echo(Mutex::new(Vec::new())));
        let mut p = pipeline(ScriptedBackend::new(Vec::new()), vec![]);
        p.backend = echo.clone();
        let events = collect(
            &p,
            Run {
                question: "q".into(),
                local: vec![hit(1, 1.0, &[1])],
                queries: Some(vec![]),
            },
        );
        let sent = echo.0.lock().unwrap().join(" ");
        assert!(!sent.contains("hunter2xyz"), "{sent}");
        let Some(Progress::Replacements(replacements)) = events
            .iter()
            .find(|e| matches!(e, Progress::Replacements(_)))
        else {
            panic!()
        };
        assert!(
            replacements
                .iter()
                .any(|r| r.original.contains("hunter2xyz"))
        );
    }

    #[test]
    fn errors_keep_the_local_results_and_are_classified() {
        let p = pipeline(ScriptedBackend::failing(|| Error::InvalidKey), vec![]);
        let events = collect(
            &p,
            Run {
                question: "q".into(),
                local: vec![hit(1, 1.0, &[1])],
                queries: None,
            },
        );
        assert!(matches!(
            events.last(),
            Some(Progress::Failed(Failure::InvalidKey))
        ));
        assert!(
            !events.iter().any(|e| matches!(e, Progress::Merged(_))),
            "nothing replaces the local list"
        );
        assert_eq!(
            classify(&Error::RateLimited {
                retry_after_secs: 7
            }),
            Failure::RateLimited {
                retry_after_secs: 7
            }
        );
        assert_eq!(classify(&Error::Network("x".into())), Failure::Network);
        assert_eq!(classify(&Error::Protocol("x".into())), Failure::Other);
    }

    #[test]
    fn cancellation_is_silent() {
        let cancel = Cancel::new();
        cancel.cancel();
        let p = pipeline(ScriptedBackend::demo(), vec![]);
        let mut events = Vec::new();
        p.run(
            Run {
                question: "kerberoast".into(),
                local: vec![],
                queries: Some(vec!["a".into()]),
            },
            &cancel,
            &mut |e| events.push(e),
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Progress::Failed(_) | Progress::Done { .. }))
        );
    }
}
