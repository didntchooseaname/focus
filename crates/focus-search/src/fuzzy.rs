use crate::exec::Catalog;
use crate::{FuzzyHit, FuzzyScope};
use focus_core::{NoteId, NoteSummary, Vault};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use rayon::prelude::*;

#[derive(Default)]
pub(crate) struct LastRun {
    pattern: String,
    catalog: u64,
    matched: Vec<u32>,
}

fn plain(p: &str) -> bool {
    !p.contains([' ', '!', '^', '$', '\'', '\\'])
}

#[derive(Clone, Copy)]
enum Which {
    Title,
    Alias(usize),
    Stem,
    Path,
}

fn recent_bonus(recent: &[NoteId], id: NoteId) -> u32 {
    match recent.iter().position(|&r| r == id) {
        Some(i) => 40u32.saturating_sub(i as u32 * 2).max(8),
        None => 0,
    }
}

fn path_text(m: &NoteSummary) -> String {
    m.path.to_string_lossy().into_owned()
}

pub(crate) fn notes(
    catalog: &Catalog,
    pattern: &str,
    recent: &[NoteId],
    limit: usize,
    last: &mut LastRun,
) -> Vec<FuzzyHit> {
    let snapshot = &catalog.notes;
    if pattern.trim().is_empty() {
        let mut order: Vec<&NoteSummary> = snapshot.iter().collect();
        order.sort_by(|a, b| {
            let ra = recent.iter().position(|&r| r == a.id).unwrap_or(usize::MAX);
            let rb = recent.iter().position(|&r| r == b.id).unwrap_or(usize::MAX);
            ra.cmp(&rb).then(b.modified.cmp(&a.modified))
        });
        *last = LastRun::default();
        return order
            .into_iter()
            .take(limit)
            .map(|m| FuzzyHit {
                note: Some(m.id),
                text: m.title.to_string(),
                positions: Vec::new(),
                score: 0,
                line: None,
            })
            .collect();
    }
    let pat = Pattern::parse(pattern, CaseMatching::Smart, Normalization::Smart);
    let with_dir = pattern.contains('/');
    let incremental = last.catalog == catalog.serial
        && !last.pattern.is_empty()
        && pattern.starts_with(&last.pattern)
        && plain(pattern)
        && plain(&last.pattern);
    let all: Vec<u32>;
    let pool: &[u32] = if incremental {
        &last.matched
    } else {
        all = (0..snapshot.len() as u32).collect();
        &all
    };

    let scored: Vec<(u32, u32, Which)> = pool
        .par_chunks(4096)
        .flat_map_iter(|chunk| {
            let mut title_m = Matcher::new(Config::DEFAULT);
            let mut path_m = Matcher::new(Config::DEFAULT.match_paths());
            let mut buf = Vec::new();
            let mut out = Vec::new();
            for &i in chunk {
                let m = &snapshot[i as usize];
                let mut best: Option<(u32, Which)> = None;
                fn consider(best: &mut Option<(u32, Which)>, s: Option<u32>, w: Which, bonus: u32) {
                    if let Some(s) = s.map(|s| s + bonus)
                        && best.is_none_or(|(b, _)| s > b)
                    {
                        *best = Some((s, w));
                    }
                }
                consider(
                    &mut best,
                    pat.score(catalog.title_hay(i as usize), &mut title_m),
                    Which::Title,
                    8,
                );
                for (k, a) in m.aliases.iter().enumerate() {
                    consider(
                        &mut best,
                        pat.score(Utf32Str::new(a, &mut buf), &mut title_m),
                        Which::Alias(k),
                        4,
                    );
                }
                if with_dir {
                    let path = m.path.to_string_lossy();
                    consider(
                        &mut best,
                        pat.score(Utf32Str::new(&path, &mut buf), &mut path_m),
                        Which::Path,
                        0,
                    );
                } else if let Some(stem) = catalog.stem_hay(i as usize) {
                    consider(&mut best, pat.score(stem, &mut title_m), Which::Stem, 2);
                }
                if let Some((s, w)) = best {
                    out.push((s + recent_bonus(recent, m.id), i, w));
                }
            }
            out
        })
        .collect();

    last.pattern = pattern.to_string();
    last.catalog = catalog.serial;
    last.matched = scored.iter().map(|(_, i, _)| *i).collect();
    last.matched.sort_unstable();

    let mut top = scored;
    let cmp = |a: &(u32, u32, Which), b: &(u32, u32, Which)| b.0.cmp(&a.0).then(a.1.cmp(&b.1));
    if top.len() > limit {
        top.select_nth_unstable_by(limit, cmp);
        top.truncate(limit);
    }
    top.sort_by(cmp);

    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut path_m = Matcher::new(Config::DEFAULT.match_paths());
    let mut buf = Vec::new();
    top.into_iter()
        .map(|(score, i, which)| {
            let m = &snapshot[i as usize];
            let text = match which {
                Which::Title => m.title.to_string(),
                Which::Alias(k) => m.aliases[k].clone(),
                Which::Stem => m
                    .path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                Which::Path => path_text(m),
            };
            let mut positions = Vec::new();
            let mm = if matches!(which, Which::Path) {
                &mut path_m
            } else {
                &mut matcher
            };
            pat.indices(Utf32Str::new(&text, &mut buf), mm, &mut positions);
            positions.sort_unstable();
            positions.dedup();
            FuzzyHit {
                note: Some(m.id),
                text,
                positions,
                score,
                line: None,
            }
        })
        .collect()
}

fn small(
    items: Vec<(String, Option<NoteId>, Option<u32>)>,
    pattern: &str,
    limit: usize,
) -> Vec<FuzzyHit> {
    if pattern.trim().is_empty() {
        return items
            .into_iter()
            .take(limit)
            .map(|(text, note, line)| FuzzyHit {
                note,
                text,
                positions: Vec::new(),
                score: 0,
                line,
            })
            .collect();
    }
    let pat = Pattern::parse(pattern, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf = Vec::new();
    let mut hits: Vec<FuzzyHit> = items
        .into_iter()
        .filter_map(|(text, note, line)| {
            let mut positions = Vec::new();
            let score =
                pat.indices(Utf32Str::new(&text, &mut buf), &mut matcher, &mut positions)?;
            positions.sort_unstable();
            positions.dedup();
            Some(FuzzyHit {
                note,
                text,
                positions,
                score,
                line,
            })
        })
        .collect();
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.text.cmp(&b.text)));
    hits.truncate(limit);
    hits
}

pub(crate) fn other(
    vault: &Vault,
    scope: FuzzyScope,
    pattern: &str,
    limit: usize,
) -> Vec<FuzzyHit> {
    match scope {
        FuzzyScope::Tags => small(
            vault
                .tags()
                .into_iter()
                .map(|t| (t.tag, None, None))
                .collect(),
            pattern,
            limit,
        ),
        FuzzyScope::Headings(id) => {
            let items = vault
                .note(id)
                .map(|m| {
                    m.headings
                        .iter()
                        .map(|h| (h.text.clone(), Some(id), Some(h.line)))
                        .collect()
                })
                .unwrap_or_default();
            small(items, pattern, limit)
        }
        FuzzyScope::Notes => Vec::new(),
    }
}
