use crate::fold::{FoldMode, Folded};
use crate::index::Postings;
use crate::prefilter::{self, Req};
use crate::{
    CancelToken, Field, FileHit, LineMatch, Pattern, QueryNode, SearchOptions, SearchResults,
    SortOrder,
};
use focus_core::{NoteId, NoteMeta, NoteSummary, PropValue, Vault};
use memchr::memmem;
use rayon::prelude::*;
use roaring::RoaringBitmap;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime};

enum Matcher {
    Text {
        folded: String,
        mode: FoldMode,
    },
    Regex(regex::Regex),
    KeyValue {
        key: String,
        value: Option<(String, FoldMode)>,
    },
    Nothing,
}

pub(crate) struct Term {
    field: Field,
    matcher: Matcher,
    req: Req,
    raw: String,
    index: usize,
    whole_word: bool,
    positive: bool,
    quiet: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct TermInfo {
    positive: bool,
    weight: f32,
    quiet: bool,
}

pub(crate) struct Catalog {
    pub serial: u64,
    pub notes: Vec<NoteSummary>,
    recency: Vec<u32>,
    names: String,
    name_ends: Vec<u32>,
    wide: Vec<char>,
    title_wide: Vec<(u32, u32)>,
    stems: Vec<Stem>,
}

#[derive(Clone, Copy)]
enum Stem {
    SameAsTitle,
    Ascii(u32, u32),
    Wide(u32, u32),
}

const ASCII: (u32, u32) = (u32::MAX, 0);

fn push_wide(wide: &mut Vec<char>, text: &str) -> (u32, u32) {
    match nucleo_matcher::Utf32String::from(text) {
        nucleo_matcher::Utf32String::Ascii(_) => ASCII,
        nucleo_matcher::Utf32String::Unicode(chars) => {
            let start = wide.len() as u32;
            wide.extend_from_slice(&chars);
            (start, chars.len() as u32)
        }
    }
}

fn stem_of(n: &NoteSummary) -> Option<std::borrow::Cow<'_, str>> {
    n.path
        .file_stem()
        .map(|s| s.to_string_lossy())
        .filter(|s| **s != *n.title)
}

impl Catalog {
    pub fn new(notes: Vec<NoteSummary>) -> Catalog {
        let mut recency: Vec<u32> = (0..notes.len() as u32).collect();
        recency.par_sort_unstable_by(|&a, &b| {
            let (a, b) = (&notes[a as usize], &notes[b as usize]);
            b.modified
                .cmp(&a.modified)
                .then_with(|| a.path.cmp(&b.path))
        });
        let folded: Vec<String> = notes
            .par_iter()
            .map(|m| FoldMode::FULL.str(&format!("{}\n{}\n", m.title, m.path.to_string_lossy())))
            .collect();
        let mut names = String::with_capacity(folded.iter().map(String::len).sum());
        let mut name_ends = Vec::with_capacity(notes.len());
        for f in folded {
            names.push_str(&f);
            name_ends.push(names.len() as u32);
        }
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut wide = Vec::new();
        let mut title_wide = Vec::with_capacity(notes.len());
        let mut stems = Vec::with_capacity(notes.len());
        for n in &notes {
            title_wide.push(if n.title.is_ascii() {
                ASCII
            } else {
                push_wide(&mut wide, &n.title)
            });
            stems.push(match stem_of(n) {
                None => Stem::SameAsTitle,
                Some(stem) if stem.is_ascii() => {
                    let bytes = n.path.as_os_str().as_encoded_bytes();
                    let start = memmem::rfind(bytes, stem.as_bytes()).unwrap_or(0);
                    Stem::Ascii(start as u32, stem.len() as u32)
                }
                Some(stem) => {
                    let (start, len) = push_wide(&mut wide, &stem);
                    Stem::Wide(start, len)
                }
            });
        }
        wide.shrink_to_fit();
        Catalog {
            serial,
            notes,
            recency,
            names,
            name_ends,
            wide,
            title_wide,
            stems,
        }
    }

    fn hay<'a>(&'a self, text: &'a str, span: (u32, u32)) -> nucleo_matcher::Utf32Str<'a> {
        if span.0 == u32::MAX {
            nucleo_matcher::Utf32Str::Ascii(text.as_bytes())
        } else {
            nucleo_matcher::Utf32Str::Unicode(
                &self.wide[span.0 as usize..(span.0 + span.1) as usize],
            )
        }
    }

    pub fn title_hay(&self, i: usize) -> nucleo_matcher::Utf32Str<'_> {
        self.hay(&self.notes[i].title, self.title_wide[i])
    }

    pub fn stem_hay(&self, i: usize) -> Option<nucleo_matcher::Utf32Str<'_>> {
        match self.stems[i] {
            Stem::SameAsTitle => None,
            Stem::Ascii(start, len) => {
                let bytes = self.notes[i].path.as_os_str().as_encoded_bytes();
                Some(nucleo_matcher::Utf32Str::Ascii(
                    &bytes[start as usize..(start + len) as usize],
                ))
            }
            Stem::Wide(start, len) => Some(nucleo_matcher::Utf32Str::Unicode(
                &self.wide[start as usize..(start + len) as usize],
            )),
        }
    }

    pub fn by_recency(&self) -> impl Iterator<Item = &NoteSummary> {
        self.recency.iter().map(|&i| &self.notes[i as usize])
    }

    fn name(&self, i: usize) -> &str {
        let start = if i == 0 {
            0
        } else {
            self.name_ends[i - 1] as usize
        };
        &self.names[start..self.name_ends[i] as usize]
    }

    fn name_matches(&self, needles: &[String]) -> RoaringBitmap {
        if needles.is_empty() {
            return RoaringBitmap::new();
        }
        let finders: Vec<memmem::Finder> = needles
            .iter()
            .map(|n| memmem::Finder::new(n.as_bytes()))
            .collect();
        let ids: Vec<NoteId> = (0..self.notes.len())
            .into_par_iter()
            .filter(|&i| {
                finders
                    .iter()
                    .any(|f| f.find(self.name(i).as_bytes()).is_some())
            })
            .map(|i| self.notes[i].id)
            .collect();
        ids.into_iter().collect()
    }
}

pub(crate) enum Node {
    And(Vec<Node>),
    Or(Vec<Node>),
    Not(Box<Node>),
    Term(Term),
    Empty,
}

fn regex_has_upper(src: &str) -> bool {
    let mut escaped = false;
    for c in src.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
        } else if c.is_uppercase() {
            return true;
        }
    }
    false
}

pub(crate) fn compile(node: &QueryNode, options: &SearchOptions, next: &mut usize) -> Node {
    compile_in(node, options, next, true)
}

fn compile_in(node: &QueryNode, options: &SearchOptions, next: &mut usize, positive: bool) -> Node {
    match node {
        QueryNode::And(v) => Node::And(
            v.iter()
                .map(|n| compile_in(n, options, next, positive))
                .collect(),
        ),
        QueryNode::Or(v) => Node::Or(
            v.iter()
                .map(|n| compile_in(n, options, next, positive))
                .collect(),
        ),
        QueryNode::Not(n) => Node::Not(Box::new(compile_in(n, options, next, !positive))),
        QueryNode::Empty => Node::Empty,
        QueryNode::Term { field, pattern } => {
            let index = *next;
            *next += 1;
            let (matcher, req, raw) = match pattern {
                Pattern::Text(t) | Pattern::Phrase(t) => {
                    let mode =
                        FoldMode::for_term(t, options.case_sensitive, options.accent_sensitive);
                    (
                        Matcher::Text {
                            folded: mode.str(t),
                            mode,
                        },
                        prefilter::literal(t),
                        t.clone(),
                    )
                }
                Pattern::Regex(r) => {
                    let ci = !(options.case_sensitive || regex_has_upper(r));
                    match regex::RegexBuilder::new(r)
                        .case_insensitive(ci)
                        .multi_line(true)
                        .size_limit(1 << 22)
                        .build()
                    {
                        Ok(re) => (Matcher::Regex(re), prefilter::regex(r, ci), r.clone()),
                        Err(_) => (Matcher::Nothing, Req::All, r.clone()),
                    }
                }
                Pattern::KeyValue(k, v) => {
                    let mode =
                        FoldMode::for_term(v, options.case_sensitive, options.accent_sensitive);
                    (
                        Matcher::KeyValue {
                            key: k.to_lowercase(),
                            value: Some((mode.str(v), mode)),
                        },
                        Req::All,
                        v.clone(),
                    )
                }
            };
            let matcher = match (field, matcher) {
                (Field::Prop, Matcher::Text { .. }) => Matcher::KeyValue {
                    key: raw.to_lowercase(),
                    value: None,
                },
                (_, m) => m,
            };
            let short = matches!(pattern, Pattern::Text(_)) && raw.chars().count() < 3;
            let whole_word = short
                && matches!(
                    field,
                    Field::Any
                        | Field::Content
                        | Field::Heading
                        | Field::Task { .. }
                        | Field::Title
                );
            Node::Term(Term {
                field: *field,
                matcher,
                req,
                raw,
                index,
                whole_word,
                positive,
                quiet: false,
            })
        }
    }
}

struct Doc<'a> {
    vault: Option<&'a Vault>,
    sum: &'a NoteSummary,
    full: Option<Option<Arc<NoteMeta>>>,
    text: Option<Option<String>>,
    folded: HashMap<(bool, bool), Folded>,
    line_starts: Option<Vec<usize>>,
    whole_word: bool,
    hits: Vec<Hit>,
    title_hits: Vec<Hit>,
    file_terms: Vec<usize>,
    tag_terms: Vec<usize>,
}

#[derive(Debug, Clone)]
struct Hit {
    range: Range<usize>,
    term: usize,
    class: u8,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn word_class(text: &str, r: &Range<usize>) -> u8 {
    let before = text[..r.start]
        .chars()
        .next_back()
        .is_none_or(|c| !is_word_char(c));
    let after = text[r.end..]
        .chars()
        .next()
        .is_none_or(|c| !is_word_char(c));
    match (before, after) {
        (true, true) => 2,
        (true, false) => 1,
        _ => 0,
    }
}

pub(crate) fn at_word_bounds(text: &str, r: &Range<usize>) -> bool {
    word_class(text, r) == 2
}

fn find_text(
    hay: &str,
    folded: &str,
    mode: FoldMode,
    whole_word: bool,
    out: &mut Vec<Range<usize>>,
) {
    if folded.is_empty() {
        return;
    }
    let f = Folded::new(hay, mode);
    for start in memmem::find_iter(f.text.as_bytes(), folded.as_bytes()) {
        let r = f.orig(start)..f.orig(start + folded.len());
        if !whole_word || at_word_bounds(hay, &r) {
            out.push(r);
        }
    }
}

fn task_state(line: &str) -> Option<bool> {
    let t = line.trim_start_matches([' ', '\t', '>']).trim_start();
    let rest = if let Some(r) = t.strip_prefix(['-', '*', '+']) {
        r
    } else {
        let digits = t.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        t[digits..].strip_prefix(['.', ')'])?
    };
    let rest = rest.strip_prefix([' ', '\t'])?.trim_start();
    let b = rest.as_bytes();
    if b.first() != Some(&b'[') {
        return None;
    }
    let mut chars = rest[1..].chars();
    let mark = chars.next()?;
    (chars.next() == Some(']')).then_some(mark != ' ')
}

fn prop_text(v: &PropValue, out: &mut String) {
    match v {
        PropValue::Null => {}
        PropValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        PropValue::Number(n) => out.push_str(&n.to_string()),
        PropValue::Text(t) => out.push_str(t),
        PropValue::List(items) => {
            for i in items {
                prop_text(i, out);
                out.push('\n');
            }
        }
        PropValue::Map(entries) => {
            for (k, v) in entries {
                out.push_str(k);
                out.push_str(": ");
                prop_text(v, out);
                out.push('\n');
            }
        }
    }
}

impl<'a> Doc<'a> {
    fn new(vault: Option<&'a Vault>, sum: &'a NoteSummary, whole_word: bool) -> Doc<'a> {
        Doc {
            vault,
            sum,
            full: None,
            text: None,
            folded: HashMap::new(),
            line_starts: None,
            whole_word,
            hits: Vec::new(),
            title_hits: Vec::new(),
            file_terms: Vec::new(),
            tag_terms: Vec::new(),
        }
    }

    fn text(&mut self) -> Option<&str> {
        if self.text.is_none() {
            self.text = Some(self.vault.and_then(|v| v.read(self.sum.id).ok()));
        }
        self.text.as_ref().and_then(|t| t.as_deref())
    }

    fn full(&mut self) -> Option<&NoteMeta> {
        if self.full.is_none() {
            self.full = Some(self.vault.and_then(|v| v.note(self.sum.id)));
        }
        self.full.as_ref().and_then(|f| f.as_deref())
    }

    fn has_tag(&mut self, raw: &str) -> bool {
        let want = raw.trim_start_matches('#').to_lowercase();
        let prefix = format!("{want}/");
        self.full().is_some_and(|m| {
            m.tags.iter().any(|tag| {
                let l = tag.to_lowercase();
                l == want || l.starts_with(&prefix)
            })
        })
    }

    fn heading_lines(&mut self) -> Vec<u32> {
        self.full()
            .map(|m| m.headings.iter().map(|h| h.line).collect())
            .unwrap_or_default()
    }

    fn line_of(&mut self, pos: usize) -> u32 {
        if self.line_starts.is_none() {
            let text = self.text.as_ref().and_then(|t| t.as_deref()).unwrap_or("");
            let mut starts = vec![0];
            starts.extend(memchr::memchr_iter(b'\n', text.as_bytes()).map(|i| i + 1));
            self.line_starts = Some(starts);
        }
        let starts = self.line_starts.as_deref().unwrap_or(&[0]);
        (starts.partition_point(|&s| s <= pos) - 1) as u32
    }

    fn content_matches(&mut self, t: &Term) -> Vec<Range<usize>> {
        let m = &t.matcher;
        let whole_word = self.whole_word || t.whole_word;
        let mut out = Vec::new();
        match m {
            Matcher::Text { folded, mode } => {
                let mode = *mode;
                let key = (mode.case, mode.accents);
                if self.text().is_none() {
                    return out;
                }
                let text = self.text.as_ref().and_then(|t| t.as_deref()).unwrap_or("");
                let f = self
                    .folded
                    .entry(key)
                    .or_insert_with(|| Folded::new(text, mode));
                for start in memmem::find_iter(f.text.as_bytes(), folded.as_bytes()) {
                    let r = f.orig(start)..f.orig(start + folded.len());
                    if !whole_word || at_word_bounds(text, &r) {
                        out.push(r);
                    }
                }
            }
            Matcher::Regex(re) => {
                let Some(text) = self.text() else { return out };
                for mat in re.find_iter(text) {
                    let r = mat.range();
                    if r.is_empty() {
                        continue;
                    }
                    if !whole_word || at_word_bounds(text, &r) {
                        out.push(r);
                    }
                }
            }
            Matcher::KeyValue { .. } | Matcher::Nothing => {}
        }
        out
    }

    fn in_string(&self, hay: &str, t: &Term) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        match &t.matcher {
            Matcher::Text { folded, mode } => find_text(
                hay,
                folded,
                *mode,
                self.whole_word || t.whole_word,
                &mut out,
            ),
            Matcher::Regex(re) => out.extend(
                re.find_iter(hay)
                    .map(|m| m.range())
                    .filter(|r| !r.is_empty()),
            ),
            _ => {}
        }
        out
    }

    fn push_hits(&mut self, ranges: Vec<Range<usize>>, t: &Term) {
        let text = self.text.as_ref().and_then(|x| x.as_deref()).unwrap_or("");
        self.hits.extend(ranges.into_iter().map(|range| Hit {
            class: word_class(text, &range),
            range,
            term: t.index,
        }));
    }

    fn push_title(&mut self, ranges: Vec<Range<usize>>, t: &Term) {
        let title = self.sum.title.clone();
        self.title_hits.extend(ranges.into_iter().map(|range| Hit {
            class: word_class(&title, &range),
            range,
            term: t.index,
        }));
    }

    fn term(&mut self, t: &Term, collect: bool) -> bool {
        let collect = collect && t.positive;
        match t.field {
            Field::Content | Field::Any => {
                let hits = self.content_matches(t);
                let mut found = !hits.is_empty();
                if t.field == Field::Any {
                    let title = self.in_string(&self.sum.title.clone(), t);
                    let path = self.sum.path.to_string_lossy().into_owned();
                    let file = self.in_string(&path, t);
                    found |= !title.is_empty() || !file.is_empty();
                    if collect {
                        self.push_title(title, t);
                        let name_start = path.rfind('/').map_or(0, |i| i + 1);
                        if file.iter().any(|r| r.start >= name_start) {
                            self.file_terms.push(t.index);
                        }
                        if self.has_tag(&t.raw) {
                            self.tag_terms.push(t.index);
                        }
                    }
                }
                if collect {
                    self.push_hits(hits, t);
                }
                found
            }
            Field::Heading => {
                let hits = self.content_matches(t);
                let heading_lines = self.heading_lines();
                let mut kept = Vec::new();
                for r in hits {
                    if heading_lines.contains(&self.line_of(r.start)) {
                        kept.push(r);
                    }
                }
                let found = !kept.is_empty();
                if collect {
                    self.push_hits(kept, t);
                }
                found
            }
            Field::Task { done } => {
                let hits = self.content_matches(t);
                let mut kept = Vec::new();
                for r in hits {
                    let line = self.line_of(r.start) as usize;
                    let starts = self.line_starts.as_deref().unwrap_or(&[]);
                    let text = self.text.as_ref().and_then(|t| t.as_deref()).unwrap_or("");
                    let end = starts.get(line + 1).copied().unwrap_or(text.len());
                    let state = task_state(&text[starts[line]..end]);
                    if state.is_some() && (done.is_none() || state == done) {
                        kept.push(r);
                    }
                }
                let found = !kept.is_empty();
                if collect {
                    self.push_hits(kept, t);
                }
                found
            }
            Field::Title => {
                let r = self.in_string(&self.sum.title.clone(), t);
                let found = !r.is_empty();
                if collect {
                    self.push_title(r, t);
                }
                found
            }
            Field::File => {
                let name = self
                    .sum
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let found = !self.in_string(&name, t).is_empty();
                if collect && found {
                    self.file_terms.push(t.index);
                }
                found
            }
            Field::Path => !self
                .in_string(&self.sum.path.to_string_lossy(), t)
                .is_empty(),
            Field::Tag => {
                let found = self.has_tag(&t.raw);
                if collect && found {
                    self.tag_terms.push(t.index);
                }
                found
            }
            Field::Prop => {
                let Matcher::KeyValue { key, value } = &t.matcher else {
                    return false;
                };
                let Some(meta) = self.full() else {
                    return false;
                };
                meta.props.iter().any(|(k, v)| {
                    if k.to_lowercase() != *key {
                        return false;
                    }
                    match value {
                        None => true,
                        Some((folded, mode)) => {
                            let mut s = String::new();
                            prop_text(v, &mut s);
                            mode.str(&s).contains(folded.as_str())
                        }
                    }
                })
            }
        }
    }

    fn eval(&mut self, node: &Node, collect: bool) -> bool {
        match node {
            Node::Empty => false,
            Node::Term(t) if t.quiet => {
                self.term(t, collect);
                true
            }
            Node::Term(t) => self.term(t, collect),
            Node::Not(n) => !self.eval(n, false),
            Node::And(items) => items.iter().all(|n| self.eval(n, collect)),
            Node::Or(items) => {
                let mut any = false;
                for n in items {
                    any |= self.eval(n, collect);
                    if any && !collect {
                        break;
                    }
                }
                any
            }
        }
    }
}

fn candidates(
    node: &Node,
    vault: &Vault,
    postings: &Postings,
    catalog: &Catalog,
    options: &SearchOptions,
) -> Option<RoaringBitmap> {
    match node {
        Node::Empty => Some(RoaringBitmap::new()),
        Node::Not(_) => None,
        Node::And(items) => {
            let mut acc: Option<RoaringBitmap> = None;
            for i in items {
                if let Some(b) = candidates(i, vault, postings, catalog, options) {
                    acc = Some(match acc {
                        None => b,
                        Some(a) => a & b,
                    });
                }
            }
            acc
        }
        Node::Or(items) => {
            let mut acc = RoaringBitmap::new();
            for i in items {
                acc |= candidates(i, vault, postings, catalog, options)?;
            }
            Some(acc)
        }
        Node::Term(t) if t.quiet => None,
        Node::Term(t) => match t.field {
            Field::Content | Field::Heading | Field::Task { .. } => postings.eval(&t.req),
            Field::Any => {
                let mut b = postings.eval(&t.req)?;
                if let Matcher::Text { .. } = t.matcher {
                    b |= catalog.name_matches(&[FoldMode::FULL.str(&t.raw)]);
                } else if let Matcher::Regex(re) = &t.matcher {
                    b.extend(
                        catalog
                            .notes
                            .iter()
                            .filter(|m| {
                                re.is_match(&m.title) || re.is_match(&m.path.to_string_lossy())
                            })
                            .map(|m| m.id),
                    );
                }
                Some(b)
            }
            Field::Title | Field::File | Field::Path => {
                let ids: Vec<NoteId> = catalog
                    .notes
                    .par_iter()
                    .filter(|m| Doc::new(None, m, options.whole_word).term(t, false))
                    .map(|m| m.id)
                    .collect();
                Some(ids.into_iter().collect())
            }
            Field::Tag => Some(
                vault
                    .notes_with_tag(t.raw.trim_start_matches('#'))
                    .into_iter()
                    .collect(),
            ),
            Field::Prop => {
                let ids: Vec<NoteId> = catalog
                    .notes
                    .par_iter()
                    .filter(|m| Doc::new(Some(vault), m, options.whole_word).term(t, false))
                    .map(|m| m.id)
                    .collect();
                Some(ids.into_iter().collect())
            }
        },
    }
}

const SNIPPET: usize = 200;

pub(crate) fn snippet(line: &str, ranges: &[Range<usize>]) -> (String, Vec<Range<usize>>) {
    if line.chars().count() <= SNIPPET {
        return (line.to_string(), ranges.to_vec());
    }
    let first = ranges.first().map_or(0, |r| r.start);
    let lead = 40;
    let start = line[..first]
        .char_indices()
        .rev()
        .nth(lead - 1)
        .map_or(0, |(i, _)| i);
    let end = line[start..]
        .char_indices()
        .nth(SNIPPET)
        .map_or(line.len(), |(i, _)| start + i);
    let kept = ranges
        .iter()
        .filter(|r| r.start >= start && r.start < end)
        .map(|r| r.start - start..r.end.min(end) - start)
        .collect();
    (line[start..end].to_string(), kept)
}

fn days_since(t: SystemTime) -> f32 {
    SystemTime::now()
        .duration_since(t)
        .map(|d| d.as_secs_f32() / 86_400.0)
        .unwrap_or(0.0)
}

struct Verified {
    sum: NoteSummary,
    text: String,
    hits: Vec<Hit>,
    title_ranges: Vec<Range<usize>>,
    score: f32,
    explain: Vec<(&'static str, f32)>,
    match_count: u32,
}

struct RankCtx {
    terms: Vec<TermInfo>,
    order: Vec<usize>,
    total_weight: f32,
    avg_size: f32,
}

impl RankCtx {
    fn ranks(&self, term: usize) -> bool {
        self.terms.get(term).is_some_and(|t| t.positive && !t.quiet)
    }

    fn share(&self, set: &[usize]) -> f32 {
        if self.total_weight <= 0.0 {
            return 0.0;
        }
        let mut seen: Vec<usize> = set.iter().copied().filter(|&t| self.ranks(t)).collect();
        seen.sort_unstable();
        seen.dedup();
        seen.iter().map(|&t| self.terms[t].weight).sum::<f32>() / self.total_weight
    }
}

fn adjacent(text: &str, a: &Range<usize>, b: &Range<usize>) -> bool {
    b.start >= a.end
        && b.start - a.end <= 3
        && text[a.end..b.start]
            .chars()
            .all(|c| !is_word_char(c) && c != '\n')
}

fn score(mut d: Doc, ctx: &RankCtx) -> Verified {
    let text = d.text.take().flatten().unwrap_or_default();
    d.hits.sort_by_key(|h| (h.range.start, h.range.end));
    d.hits.dedup_by(|a, b| a.range == b.range);
    let mut starts = vec![0];
    starts.extend(memchr::memchr_iter(b'\n', text.as_bytes()).map(|i| i + 1));
    let line_of = |pos: usize| starts.partition_point(|&s| s <= pos) - 1;

    let title_terms: Vec<usize> = d.title_hits.iter().map(|h| h.term).collect();
    let mut title_ranges: Vec<Range<usize>> = d
        .title_hits
        .iter()
        .filter(|h| ctx.ranks(h.term))
        .map(|h| h.range.clone())
        .collect();
    title_ranges.sort_by_key(|r| (r.start, r.end));
    title_ranges.dedup();
    let title_chars = d.sum.title.chars().count().max(1) as f32;
    let mut covered = 0usize;
    let mut end = 0usize;
    for r in &title_ranges {
        let from = r.start.max(end);
        if r.end > from {
            covered += d.sum.title[from..r.end].chars().count();
            end = r.end;
        }
    }
    let title = 10.0 * ctx.share(&title_terms) + 10.0 * (covered as f32 / title_chars).min(1.0);
    let file = 6.0
        * ctx.share(
            &d.file_terms
                .iter()
                .copied()
                .filter(|t| !title_terms.contains(t))
                .collect::<Vec<_>>(),
        );
    let heading_lines = d.heading_lines();
    let heading_terms: Vec<usize> = d
        .hits
        .iter()
        .filter(|h| heading_lines.contains(&(line_of(h.range.start) as u32)))
        .map(|h| h.term)
        .collect();
    let headings = 5.0 * ctx.share(&heading_terms);
    let tags = 4.0 * ctx.share(&d.tag_terms);

    let class_weight = [0.25f32, 0.6, 1.0];
    let (k1, b) = (1.2f32, 0.75f32);
    let norm = 1.0 - b + b * (d.sum.size as f32 / ctx.avg_size.max(1.0));
    let mut body = 0.0;
    for (t, info) in ctx.terms.iter().enumerate() {
        if !ctx.ranks(t) {
            continue;
        }
        let tf: f32 = d
            .hits
            .iter()
            .filter(|h| h.term == t)
            .map(|h| class_weight[h.class as usize])
            .sum();
        body += info.weight * tf * (k1 + 1.0) / (tf + k1 * norm) / (k1 + 1.0);
    }
    if ctx.total_weight > 0.0 {
        body = 6.0 * body / ctx.total_weight;
    }

    let mut proximity = 0.0;
    if ctx.order.len() > 1 {
        let ranking: Vec<usize> = ctx
            .order
            .iter()
            .copied()
            .filter(|&t| ctx.ranks(t))
            .collect();
        let mut line_terms: HashMap<usize, Vec<usize>> = HashMap::new();
        for h in &d.hits {
            line_terms
                .entry(line_of(h.range.start))
                .or_default()
                .push(h.term);
        }
        if ranking.len() > 1
            && line_terms
                .values()
                .any(|ts| ranking.iter().all(|t| ts.contains(t)))
        {
            proximity += 3.0;
        }
        let pairs = ctx.order.windows(2).filter(|w| {
            let (a, b) = (w[0], w[1]);
            d.hits.iter().filter(|h| h.term == a).any(|ha| {
                d.hits
                    .iter()
                    .filter(|h| h.term == b)
                    .any(|hb| adjacent(&text, &ha.range, &hb.range))
            })
        });
        proximity += 8.0 * pairs.count() as f32 / (ctx.order.len() - 1) as f32;
    }
    let recency = (-days_since(d.sum.modified) / 60.0).exp();

    d.hits.retain(|h| ctx.ranks(h.term));
    let explain = vec![
        ("title", title),
        ("file", file),
        ("headings", headings),
        ("tags", tags),
        ("body", body),
        ("proximity", proximity),
        ("recency", recency),
    ];
    let score = explain.iter().map(|(_, v)| v).sum();
    Verified {
        sum: d.sum.clone(),
        text,
        match_count: d.hits.len() as u32,
        hits: d.hits,
        title_ranges,
        score,
        explain,
    }
}

fn into_hit(v: Verified, options: &SearchOptions) -> FileHit {
    let text = &v.text;
    let mut starts = vec![0];
    starts.extend(memchr::memchr_iter(b'\n', text.as_bytes()).map(|i| i + 1));
    let line_of = |pos: usize| starts.partition_point(|&s| s <= pos) - 1;
    let mut matches: Vec<LineMatch> = Vec::new();
    let mut i = 0;
    while i < v.hits.len() && matches.len() < options.max_matches_per_file {
        let line = line_of(v.hits[i].range.start);
        let line_start = starts[line];
        let line_end = starts.get(line + 1).map_or(text.len(), |&e| e - 1);
        let line_text = text[line_start..line_end].trim_end_matches('\r');
        let mut ranges = Vec::new();
        while i < v.hits.len() && line_of(v.hits[i].range.start) == line {
            let r = &v.hits[i].range;
            if r.start - line_start < line_text.len() {
                ranges.push(r.start - line_start..(r.end - line_start).min(line_text.len()));
            }
            i += 1;
        }
        let (snip, rs) = snippet(line_text, &ranges);
        matches.push(LineMatch {
            line: line as u32,
            text: snip,
            ranges: rs,
        });
    }
    FileHit {
        note: v.sum.id,
        score: v.score,
        title_ranges: v.title_ranges,
        matches,
        match_count: v.match_count,
        explain: v.explain,
    }
}

fn name_needles(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::Term(Term {
            field: Field::Any | Field::Title | Field::File,
            raw,
            matcher: Matcher::Text { .. },
            positive: true,
            ..
        }) => out.push(FoldMode::FULL.str(raw)),
        Node::And(v) | Node::Or(v) => v.iter().for_each(|n| name_needles(n, out)),
        _ => {}
    }
}

fn terms_of<'n>(node: &'n Node, in_and: bool, out: &mut Vec<(&'n Term, bool)>) {
    match node {
        Node::Term(t) => out.push((t, in_and)),
        Node::And(v) => v.iter().for_each(|n| terms_of(n, v.len() > 1, out)),
        Node::Or(v) => v.iter().for_each(|n| terms_of(n, false, out)),
        Node::Not(n) => terms_of(n, false, out),
        Node::Empty => {}
    }
}

fn mark_quiet(node: &mut Node, infos: &[TermInfo]) {
    match node {
        Node::Term(t) => t.quiet = infos.get(t.index).is_some_and(|i| i.quiet),
        Node::And(v) | Node::Or(v) => v.iter_mut().for_each(|n| mark_quiet(n, infos)),
        Node::Not(n) => mark_quiet(n, infos),
        Node::Empty => {}
    }
}

fn rank_context(root: &Node, postings: &Postings, catalog: &Catalog) -> RankCtx {
    let mut terms = Vec::new();
    terms_of(root, false, &mut terms);
    let n = catalog.notes.len().max(1) as f32;
    let mut infos =
        vec![TermInfo::default(); terms.iter().map(|(t, _)| t.index + 1).max().unwrap_or(0)];
    for &(t, in_and) in &terms {
        let df = match (&t.matcher, postings.eval(&t.req)) {
            (Matcher::Text { .. }, Some(b)) => {
                let mut b = b;
                if t.field == Field::Any {
                    b |= catalog.name_matches(&[FoldMode::FULL.str(&t.raw)]);
                }
                b.len() as f32
            }
            (Matcher::Text { .. }, None) => n,
            (_, Some(b)) => b.len() as f32,
            (_, None) => n / 4.0,
        };
        let weight = (1.0 + (n - df + 0.5) / (df + 0.5)).ln().max(0.05);
        let quiet = in_and && t.positive && t.whole_word && df / n > 0.25;
        infos[t.index] = TermInfo {
            positive: t.positive,
            weight,
            quiet,
        };
    }
    if infos.iter().filter(|i| i.positive).all(|i| i.quiet) {
        for i in &mut infos {
            i.quiet = false;
        }
    }
    let order: Vec<usize> = terms
        .iter()
        .filter(|(t, _)| t.positive)
        .map(|(t, _)| t.index)
        .collect();
    let total_weight = infos
        .iter()
        .filter(|i| i.positive && !i.quiet)
        .map(|i| i.weight)
        .sum();
    let avg_size = catalog.notes.iter().map(|m| m.size as f32).sum::<f32>() / n;
    RankCtx {
        terms: infos,
        order,
        total_weight,
        avg_size,
    }
}

pub(crate) fn run(
    vault: &Vault,
    postings: &Postings,
    catalog: &Catalog,
    mut root: Node,
    options: &SearchOptions,
    cancel: &CancelToken,
    started: Instant,
) -> SearchResults {
    let ctx = rank_context(&root, postings, catalog);
    mark_quiet(&mut root, &ctx.terms);
    let root = &root;
    let cand = candidates(root, vault, postings, catalog, options);
    if cancel.is_cancelled() {
        return SearchResults {
            cancelled: true,
            elapsed_ms: started.elapsed().as_secs_f32() * 1000.0,
            ..Default::default()
        };
    }
    let mut needles = Vec::new();
    name_needles(root, &mut needles);
    let preferred = catalog.name_matches(&needles);
    let admitted = |id: NoteId| cand.as_ref().is_none_or(|c| c.contains(id));
    let mut ids: Vec<&NoteSummary> = Vec::new();
    for pass in [true, false] {
        ids.extend(
            catalog
                .recency
                .iter()
                .map(|&i| &catalog.notes[i as usize])
                .filter(|m| preferred.contains(m.id) == pass && admitted(m.id)),
        );
    }

    let budget = std::time::Duration::from_millis(options.budget_ms as u64);
    let mut verified: Vec<Verified> = Vec::new();
    let mut partial = false;
    let mut cancelled = false;
    let chunk = 8 * rayon::current_num_threads().max(1);
    let any_found = AtomicBool::new(false);
    let cut = AtomicBool::new(false);
    for (k, part) in ids.chunks(chunk).enumerate() {
        if cancel.is_cancelled() {
            cancelled = true;
            break;
        }
        if k > 0 && started.elapsed() > budget {
            partial = true;
            break;
        }
        let found: Vec<Verified> = part
            .par_iter()
            .filter_map(|m| {
                if cancel.is_cancelled() {
                    return None;
                }
                if (k > 0 || any_found.load(Ordering::Relaxed)) && started.elapsed() > budget {
                    cut.store(true, Ordering::Relaxed);
                    return None;
                }
                let mut d = Doc::new(Some(vault), m, options.whole_word);
                let hit = d.eval(root, true).then(|| score(d, &ctx));
                if hit.is_some() {
                    any_found.store(true, Ordering::Relaxed);
                }
                hit
            })
            .collect();
        verified.extend(found);
        if cut.load(Ordering::Relaxed) {
            partial = true;
            break;
        }
    }
    if cancel.is_cancelled() {
        cancelled = true;
    }
    let total_files = verified.len();
    let total_matches = verified.iter().map(|v| v.match_count as usize).sum();
    match options.sort {
        SortOrder::Relevance => verified.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.sum.path.cmp(&b.sum.path))
        }),
        SortOrder::Path => verified.sort_by(|a, b| a.sum.path.cmp(&b.sum.path)),
        SortOrder::ModifiedNewest => verified.sort_by_key(|v| std::cmp::Reverse(v.sum.modified)),
        SortOrder::ModifiedOldest => verified.sort_by_key(|v| v.sum.modified),
    }
    let truncated = verified.len() > options.max_files;
    verified.truncate(options.max_files);
    let hits: Vec<FileHit> = verified
        .into_par_iter()
        .map(|v| into_hit(v, options))
        .collect();
    SearchResults {
        hits,
        total_files,
        total_matches,
        elapsed_ms: started.elapsed().as_secs_f32() * 1000.0,
        truncated,
        cancelled,
        totals_are_minimums: partial || cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_lines() {
        assert_eq!(task_state("- [ ] a"), Some(false));
        assert_eq!(task_state("  * [x] a"), Some(true));
        assert_eq!(task_state("> 2. [/] a"), Some(true));
        assert_eq!(task_state("- a [ ]"), None);
        assert_eq!(task_state("[ ] a"), None);
    }

    #[test]
    fn snippets_cut_on_char_boundaries() {
        let line = "é".repeat(300) + "kerberos" + &"à".repeat(300);
        let start = line.find("kerberos").unwrap();
        let (s, r) = snippet(&line, std::slice::from_ref(&(start..start + 8)));
        assert_eq!(s.chars().count(), 200);
        assert_eq!(&s[r[0].clone()], "kerberos");
        let one = 0..2;
        let (s, r) = snippet("court", std::slice::from_ref(&one));
        assert_eq!(
            (s.as_str(), r.as_slice()),
            ("court", std::slice::from_ref(&one))
        );
    }

    #[test]
    fn upper_in_regex() {
        assert!(!regex_has_upper(r"\S+\W"));
        assert!(regex_has_upper(r"Kerb\S"));
    }
}
