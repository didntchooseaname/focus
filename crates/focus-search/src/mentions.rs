use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use focus_core::{HistoryKind, NoteId, NoteSummary, Vault, VaultError};
use memchr::{memchr, memmem};
use rayon::prelude::*;
use roaring::RoaringBitmap;

use crate::CancelToken;
use crate::exec::{Catalog, at_word_bounds, snippet};
use crate::fold::{FoldMode, Folded};
use crate::index::Postings;
use crate::prefilter;

#[derive(Debug, Clone)]
pub struct MentionOptions {
    pub max_notes: usize,
    pub max_per_note: usize,
    pub budget_ms: u32,
}

impl Default for MentionOptions {
    fn default() -> Self {
        MentionOptions {
            max_notes: 100,
            max_per_note: 20,
            budget_ms: 200,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mention {
    pub line: u32,
    pub text: String,
    pub range: Range<usize>,
    pub span: Range<usize>,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct MentionHit {
    pub note: NoteId,
    pub mentions: Vec<Mention>,
    pub count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Mentions {
    pub hits: Vec<MentionHit>,
    pub total_notes: usize,
    pub total_mentions: usize,
    pub partial: bool,
    pub cancelled: bool,
    pub elapsed_ms: f32,
}

fn names_of(path: &Path, aliases: &[String]) -> Vec<String> {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    let mut names: Vec<String> = Vec::new();
    for name in stem.into_iter().chain(aliases.iter().cloned()) {
        let name = name.trim().to_string();
        if !name.is_empty()
            && !names
                .iter()
                .any(|n| n.to_lowercase() == name.to_lowercase())
        {
            names.push(name);
        }
    }
    names
}

const MODE: FoldMode = FoldMode {
    case: true,
    accents: false,
};

pub(crate) fn run(
    vault: &Vault,
    postings: &Postings,
    catalog: &Catalog,
    note: NoteId,
    options: &MentionOptions,
    cancel: &CancelToken,
    started: Instant,
) -> Mentions {
    let done = |mut m: Mentions| {
        m.elapsed_ms = started.elapsed().as_secs_f32() * 1000.0;
        m
    };
    let Some(meta) = vault.note(note) else {
        return done(Mentions::default());
    };
    let names = names_of(&meta.path, &meta.aliases);
    let mut candidates = Some(RoaringBitmap::new());
    for name in &names {
        match (
            postings.eval(&prefilter::literal(name)),
            candidates.as_mut(),
        ) {
            (Some(found), Some(all)) => *all |= found,
            _ => candidates = None,
        }
    }
    let needles: Vec<String> = names.iter().map(|n| MODE.str(n)).collect();
    let order: Vec<&NoteSummary> = catalog
        .by_recency()
        .filter(|m| m.id != note && candidates.as_ref().is_none_or(|c| c.contains(m.id)))
        .collect();

    let budget = std::time::Duration::from_millis(options.budget_ms as u64);
    let chunk = 8 * rayon::current_num_threads().max(1);
    let cut = AtomicBool::new(false);
    let mut found: Vec<(&NoteSummary, MentionHit)> = Vec::new();
    let mut partial = false;
    for (k, part) in order.chunks(chunk).enumerate() {
        if cancel.is_cancelled() || (k > 0 && started.elapsed() > budget) {
            partial = true;
            break;
        }
        found.extend(
            part.par_iter()
                .filter_map(|m| {
                    if cancel.is_cancelled() || started.elapsed() > budget {
                        cut.store(true, Ordering::Relaxed);
                        return None;
                    }
                    verify(vault, m, &needles, options.max_per_note).map(|hit| (*m, hit))
                })
                .collect::<Vec<_>>(),
        );
        if cut.load(Ordering::Relaxed) {
            partial = true;
            break;
        }
    }
    found.sort_by(|a, b| a.0.path.cmp(&b.0.path));
    let total_notes = found.len();
    let total_mentions = found.iter().map(|(_, h)| h.count as usize).sum();
    found.truncate(options.max_notes);
    done(Mentions {
        hits: found.into_iter().map(|(_, hit)| hit).collect(),
        total_notes,
        total_mentions,
        partial,
        cancelled: cancel.is_cancelled(),
        elapsed_ms: 0.0,
    })
}

fn verify(vault: &Vault, m: &NoteSummary, needles: &[String], keep: usize) -> Option<MentionHit> {
    let text = vault.read(m.id).ok()?;
    let spans = find_mentions(&text, needles);
    if spans.is_empty() {
        return None;
    }
    let mut starts = vec![0];
    starts.extend(memchr::memchr_iter(b'\n', text.as_bytes()).map(|i| i + 1));
    let mentions = spans
        .iter()
        .take(keep)
        .map(|span| {
            let line = starts.partition_point(|&s| s <= span.start) - 1;
            let from = starts[line];
            let to = starts.get(line + 1).map_or(text.len(), |&s| s);
            let whole = text[from..to].trim_end_matches(['\n', '\r']);
            let local = span.start - from..span.end - from;
            let (text, ranges) = snippet(whole, std::slice::from_ref(&local));
            Mention {
                line: line as u32,
                text,
                range: ranges.first().cloned().unwrap_or(0..0),
                span: span.clone(),
                column: local.start,
            }
        })
        .collect();
    Some(MentionHit {
        note: m.id,
        mentions,
        count: spans.len() as u32,
    })
}

pub(crate) fn find_mentions(text: &str, needles: &[String]) -> Vec<Range<usize>> {
    let skip = excluded(text);
    let folded = Folded::new(text, MODE);
    let mut found = Vec::new();
    for needle in needles.iter().filter(|n| !n.is_empty()) {
        for start in memmem::find_iter(folded.text.as_bytes(), needle.as_bytes()) {
            let r = folded.orig(start)..folded.orig(start + needle.len());
            let next = skip.partition_point(|s| s.end <= r.start);
            let inside = skip.get(next).is_some_and(|s| s.start < r.end);
            if !inside && at_word_bounds(text, &r) {
                found.push(r);
            }
        }
    }
    found.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    let mut out: Vec<Range<usize>> = Vec::with_capacity(found.len());
    for r in found {
        if out.last().is_none_or(|last| r.start >= last.end) {
            out.push(r);
        }
    }
    out
}

fn frontmatter_end(text: &str) -> usize {
    let first = text.split_inclusive('\n').next().unwrap_or("");
    if first.trim_end() != "---" || !first.ends_with('\n') {
        return 0;
    }
    let mut at = first.len();
    for line in text[at..].split_inclusive('\n') {
        at += line.len();
        if matches!(line.trim_end(), "---" | "...") {
            return at;
        }
    }
    0
}

fn fence(line: &str) -> Option<(u8, usize)> {
    let b = line.trim_start_matches([' ', '\t', '>']).as_bytes();
    let marker = *b.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let n = b.iter().take_while(|&&c| c == marker).count();
    (n >= 3).then_some((marker, n))
}

fn excluded(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut at = frontmatter_end(text);
    if at > 0 {
        out.push(0..at);
    }
    let mut open: Option<(u8, usize, usize)> = None;
    for line in text[at..].split_inclusive('\n') {
        let start = at;
        at += line.len();
        let body = line.trim_end_matches(['\n', '\r']);
        match (open, fence(body)) {
            (Some((marker, len, from)), Some((m, n))) if m == marker && n >= len => {
                let rest = body.trim_start_matches([' ', '\t', '>']);
                if rest[n..].trim().is_empty() {
                    out.push(from..at);
                    open = None;
                }
            }
            (Some(_), _) => {}
            (None, Some((m, n))) => open = Some((m, n, start)),
            (None, None) => inline(body, start, &mut out),
        }
    }
    if let Some((_, _, from)) = open {
        out.push(from..text.len());
    }
    out
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/')
}

fn inline(line: &str, from: usize, out: &mut Vec<Range<usize>>) {
    let b = line.as_bytes();
    let find = |at: usize, what: &[u8]| memmem::find(&b[at..], what).map(|i| at + i);
    let mut i = 0;
    while i < b.len() {
        let end = match b[i] {
            b'`' => {
                let n = b[i..].iter().take_while(|&&c| c == b'`').count();
                let mut at = i + n;
                let mut close = None;
                while let Some(j) = memchr(b'`', &b[at..]).map(|j| at + j) {
                    let run = b[j..].iter().take_while(|&&c| c == b'`').count();
                    if run == n {
                        close = Some(j + n);
                        break;
                    }
                    at = j + run;
                }
                match close {
                    Some(close) => Some(close),
                    None => {
                        i += n;
                        continue;
                    }
                }
            }
            b'[' if b[i..].starts_with(b"[[") => find(i + 2, b"]]").map(|j| j + 2),
            b'[' => memchr(b']', &b[i + 1..])
                .map(|j| i + 1 + j)
                .filter(|&j| b.get(j + 1) == Some(&b'('))
                .and_then(|j| memchr(b')', &b[j + 2..]).map(|k| j + 2 + k + 1)),
            b'<' if b[i..].starts_with(b"<!--") => {
                Some(find(i + 4, b"-->").map_or(b.len(), |j| j + 3))
            }
            b'<' => memchr(b'>', &b[i + 1..])
                .map(|j| i + 1 + j)
                .filter(|&j| {
                    let inside = &b[i + 1..j];
                    !inside.contains(&b' ') && (inside.contains(&b':') || inside.contains(&b'@'))
                })
                .map(|j| j + 1),
            b'%' if b[i..].starts_with(b"%%") => {
                Some(find(i + 2, b"%%").map_or(b.len(), |j| j + 2))
            }
            b'#' if (i == 0 || b[i - 1].is_ascii_whitespace() || b[i - 1] == b'(')
                && line[i + 1..].chars().next().is_some_and(is_tag_char) =>
            {
                Some(
                    i + 1
                        + line[i + 1..]
                            .char_indices()
                            .find(|(_, c)| !is_tag_char(*c))
                            .map_or(line.len() - i - 1, |(k, _)| k),
                )
            }
            b':' if b[i..].starts_with(b"://") => {
                let scheme = b[..i]
                    .iter()
                    .rev()
                    .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
                    .count();
                let start = (i - scheme).max(out.last().map_or(0, |r| r.end.saturating_sub(from)));
                let stop = b[i..]
                    .iter()
                    .position(|c| c.is_ascii_whitespace() || matches!(c, b'>' | b')'))
                    .map_or(b.len(), |k| i + k);
                out.push(from + start..from + stop);
                i = stop;
                continue;
            }
            _ => None,
        };
        match end {
            Some(end) => {
                out.push(from + i..from + end);
                i = end;
            }
            None => i += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkPath {
    Relative,
    Root,
    Shortest,
}

pub fn mention_link(
    words: &str,
    from: &Path,
    to: &Path,
    markdown: bool,
    form: LinkPath,
    unique: bool,
) -> String {
    let to = to.with_extension("");
    let root = to.to_string_lossy().replace('\\', "/");
    let target = match form {
        LinkPath::Root => root,
        LinkPath::Shortest if unique => to
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or(root),
        LinkPath::Shortest => root,
        LinkPath::Relative => {
            let base: Vec<_> = from
                .parent()
                .map(|p| p.components().collect())
                .unwrap_or_default();
            let target: Vec<_> = to.components().collect();
            let shared = base.iter().zip(&target).take_while(|(a, b)| a == b).count();
            let mut parts: Vec<String> =
                std::iter::repeat_n("..".to_string(), base.len() - shared).collect();
            parts.extend(
                target[shared..]
                    .iter()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned()),
            );
            parts.join("/")
        }
    };
    if markdown {
        format!("[{words}]({}.md)", target.replace(' ', "%20"))
    } else if target == words {
        format!("[[{words}]]")
    } else {
        format!("[[{target}|{words}]]")
    }
}

pub fn link_mention(source: &str, span: Range<usize>, words: &str, link: &str) -> Option<String> {
    if source.get(span.clone())? != words {
        return None;
    }
    let mut out = String::with_capacity(source.len() + link.len());
    out.push_str(&source[..span.start]);
    out.push_str(link);
    out.push_str(&source[span.end..]);
    Some(out)
}

#[derive(Debug)]
pub enum LinkMentionError {
    Changed,
    Vault(VaultError),
}

impl std::fmt::Display for LinkMentionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkMentionError::Changed => write!(f, "the note changed since the mention was found"),
            LinkMentionError::Vault(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for LinkMentionError {}

pub fn write_mention_link(
    vault: &Vault,
    note: NoteId,
    span: Range<usize>,
    words: &str,
    link: &str,
) -> Result<(), LinkMentionError> {
    let modified = vault.note(note).ok_or(LinkMentionError::Changed)?.modified;
    let source = vault.read(note).map_err(LinkMentionError::Vault)?;
    let content = link_mention(&source, span, words, link).ok_or(LinkMentionError::Changed)?;
    match vault.write_with(note, &content, Some(modified), HistoryKind::Bulk) {
        Ok(_) => Ok(()),
        Err(VaultError::Conflict(_)) => Err(LinkMentionError::Changed),
        Err(e) => Err(LinkMentionError::Vault(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans<'a>(text: &'a str, names: &[&str]) -> Vec<&'a str> {
        let needles: Vec<String> = names.iter().map(|n| MODE.str(n)).collect();
        find_mentions(text, &needles)
            .into_iter()
            .map(|r| &text[r])
            .collect()
    }

    #[test]
    fn whole_words_in_any_case() {
        assert_eq!(
            spans("Kerberos, KERBEROS and kerberoasting.", &["Kerberos"]),
            ["Kerberos", "KERBEROS"]
        );
        assert_eq!(
            spans("Ticket granting ticket", &["ticket granting"]),
            ["Ticket granting"]
        );
        assert_eq!(spans("Café cafe", &["Café"]), ["Café"], "accents are kept");
    }

    #[test]
    fn not_in_code_links_addresses_tags_or_frontmatter() {
        let text = "---\naliases: [Kerberos]\n---\n\
            `Kerberos` and [[Kerberos]] or [[Other|Kerberos]] ![[Kerberos.png]]\n\
            [Kerberos](x.md) https://ex.com/Kerberos <https://Kerberos.org> #Kerberos\n\
            %% Kerberos %% <!-- Kerberos -->\n\
            ```\nKerberos\n```\n\
            ~~~~\n```\nKerberos\n~~~~\n\
            > ```\n> Kerberos\n> ```\n\
            Kerberos (#tag) ``a ` Kerberos`` here\n# Kerberos\n";
        assert_eq!(spans(text, &["Kerberos"]), ["Kerberos", "Kerberos"]);
        let last = find_mentions(text, &[MODE.str("Kerberos")]);
        assert!(
            text[last[1].start..].starts_with("Kerberos\n"),
            "a heading counts"
        );
    }

    #[test]
    fn the_longer_name_wins_an_overlap() {
        assert_eq!(
            spans(
                "Active Directory and AD",
                &["AD", "Active Directory", "Directory"]
            ),
            ["Active Directory", "AD"]
        );
    }

    #[test]
    fn unclosed_constructs_hide_nothing_after_them() {
        assert_eq!(spans("a ` b Kerberos", &["Kerberos"]), ["Kerberos"]);
        assert_eq!(spans("[[open Kerberos", &["Kerberos"]), ["Kerberos"]);
        assert_eq!(spans("---\nKerberos", &["Kerberos"]), ["Kerberos"]);
    }

    #[test]
    fn names_are_the_file_name_and_the_aliases() {
        assert_eq!(
            names_of(
                Path::new("Red/Kerberos.md"),
                &[" krb ".into(), "kerberos".into(), String::new()]
            ),
            ["Kerberos", "krb"]
        );
    }

    #[test]
    fn links_keep_the_words_and_follow_the_setting() {
        let (from, to) = (Path::new("A/B/note.md"), Path::new("A/C/Kerberos.md"));
        assert_eq!(
            mention_link("kerberos", from, to, false, LinkPath::Shortest, true),
            "[[Kerberos|kerberos]]"
        );
        assert_eq!(
            mention_link("Kerberos", from, to, false, LinkPath::Shortest, true),
            "[[Kerberos]]"
        );
        assert_eq!(
            mention_link("Kerberos", from, to, false, LinkPath::Shortest, false),
            "[[A/C/Kerberos|Kerberos]]"
        );
        assert_eq!(
            mention_link("Kerberos", from, to, false, LinkPath::Relative, true),
            "[[../C/Kerberos|Kerberos]]"
        );
        assert_eq!(
            mention_link(
                "the ticket",
                from,
                Path::new("Golden ticket.md"),
                true,
                LinkPath::Root,
                true
            ),
            "[the ticket](Golden%20ticket.md)"
        );
    }

    #[test]
    fn only_the_bytes_of_the_mention_change() {
        let source = "a\r\nKerberos here\r\nKerberos\r\n";
        assert_eq!(
            link_mention(source, 3..11, "Kerberos", "[[Kerberos]]").as_deref(),
            Some("a\r\n[[Kerberos]] here\r\nKerberos\r\n")
        );
        assert_eq!(
            link_mention(source, 4..12, "Kerberos", "[[Kerberos]]"),
            None
        );
        assert_eq!(
            link_mention(source, 3..90, "Kerberos", "[[Kerberos]]"),
            None
        );
    }
}
