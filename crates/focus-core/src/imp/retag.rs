use super::parse::{self, tag_name};
use super::*;
use std::ops::Range;

pub(crate) fn is_valid(name: &str) -> bool {
    tag_name(name) == Some(name) && !name.starts_with('/') && !name.contains("//")
}

pub(crate) fn renamed(tag: &str, from: &str, to: &str) -> Option<String> {
    let mut rest = tag.char_indices();
    for f in from.chars() {
        let (_, c) = rest.next()?;
        if !c.to_lowercase().eq(f.to_lowercase()) {
            return None;
        }
    }
    let at = rest.next().map_or(tag.len(), |(i, _)| i);
    let suffix = &tag[at..];
    (suffix.is_empty() || suffix.starts_with('/')).then(|| format!("{to}{suffix}"))
}

struct Word {
    name: Range<usize>,
    extent: Range<usize>,
}

struct Group {
    words: Vec<Word>,
    extent: Range<usize>,
    line: Option<Range<usize>>,
}

struct Value {
    groups: Vec<Group>,
    flow: bool,
}

fn words(text: &str, base: usize) -> Vec<Word> {
    let mut out = Vec::new();
    let mut start = None;
    let ends = text.char_indices().map(|(i, c)| (i, Some(c)));
    for (i, c) in ends.chain([(text.len(), None)]) {
        let separator = c.is_none_or(|c| c == ',' || c.is_whitespace());
        match (start, separator) {
            (None, false) => start = Some(i),
            (Some(s), true) => {
                let raw = &text[s..i];
                if let Some(name) = tag_name(raw) {
                    let offset = raw.find(name).unwrap_or(0);
                    out.push(Word {
                        name: base + s + offset..base + s + offset + name.len(),
                        extent: base + s..base + i,
                    });
                }
                start = None;
            }
            _ => {}
        }
    }
    out
}

fn scalar_words(raw: &str, base: usize) -> Option<Vec<Word>> {
    let trimmed = raw.trim();
    let base = base + (raw.len() - raw.trim_start().len());
    let b = trimmed.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        let inner = &trimmed[1..trimmed.len() - 1];
        if inner.contains('\\') || inner.contains("''") {
            return None;
        }
        return Some(words(inner, base + 1));
    }
    let text = match trimmed.find(" #") {
        Some(p) => trimmed[..p].trim_end(),
        None => trimmed,
    };
    matches!(parse::scalar(text), PropValue::Text(_)).then(|| words(text, base))
}

fn flow_items<'a>(raw: &'a str, base: usize) -> Vec<(Range<usize>, &'a str)> {
    let mut out = Vec::new();
    let mut quote: Option<char> = None;
    let mut depth = 0i32;
    let mut start = 0;
    let push = |from: usize, to: usize, out: &mut Vec<(Range<usize>, &'a str)>| {
        let part = &raw[from..to];
        let lead = part.len() - part.trim_start().len();
        let item = part.trim();
        if !item.is_empty() {
            out.push((base + from + lead..base + from + lead + item.len(), item));
        }
    };
    for (i, c) in raw.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '[' | '{') => depth += 1,
            (None, ']' | '}') => depth -= 1,
            (None, ',') if depth == 0 => {
                push(start, i, &mut out);
                start = i + 1;
            }
            _ => {}
        }
    }
    push(start, raw.len(), &mut out);
    out
}

fn property_values(source: &str) -> Vec<Value> {
    let Some(end) = parse::frontmatter_end(source) else {
        return Vec::new();
    };
    let lines: Vec<(usize, &str)> = parse::line_offsets(&source[..end]).skip(1).collect();
    let lines = &lines[..lines.len().saturating_sub(1)];
    let line_end = |i: usize| lines.get(i + 1).map_or(end, |(at, _)| *at);
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let (at, line) = lines[i];
        i += 1;
        if line.starts_with([' ', '\t']) {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue;
        };
        let key = line[..colon].trim();
        let after = &line[colon + 1..];
        if !(key.eq_ignore_ascii_case("tags") || key.eq_ignore_ascii_case("tag"))
            || !(after.is_empty() || after.starts_with([' ', '\t']))
        {
            continue;
        }
        let rest_at = at + colon + 1;
        let rest = after.trim();
        if rest.is_empty() || rest.starts_with('#') {
            let mut groups = Vec::new();
            let mut indent = None;
            while i < lines.len() {
                let (item_at, item) = lines[i];
                let t = item.trim();
                if t.is_empty() || t.starts_with('#') {
                    i += 1;
                    continue;
                }
                let lead = item.len() - item.trim_start().len();
                let Some(value) = t.strip_prefix('-') else {
                    break;
                };
                if (!value.is_empty() && !value.starts_with([' ', '\t']))
                    || *indent.get_or_insert(lead) != lead
                {
                    break;
                }
                let value_at = item_at + lead + 1;
                let v = value.trim();
                let nested = v.starts_with(['[', '{'])
                    || (!v.starts_with(['"', '\'']) && (v.contains(": ") || v.ends_with(':')));
                if let Some(words) = scalar_words(value, value_at).filter(|_| !nested) {
                    groups.push(Group {
                        words,
                        extent: value_at..value_at + value.len(),
                        line: Some(item_at..line_end(i)),
                    });
                }
                i += 1;
            }
            out.push(Value {
                groups,
                flow: false,
            });
        } else if rest.starts_with('[') && rest.ends_with(']') {
            let open = rest_at + after.find('[').unwrap_or(0);
            let inner = &rest[1..rest.len() - 1];
            let groups = flow_items(inner, open + 1)
                .into_iter()
                .filter_map(|(extent, item)| {
                    let words = scalar_words(item, extent.start)?;
                    Some(Group {
                        words,
                        extent,
                        line: None,
                    })
                })
                .collect();
            out.push(Value { groups, flow: true });
        } else if !rest.starts_with(['|', '>', '{'])
            && let Some(words) = scalar_words(after, rest_at)
        {
            let extent = rest_at..rest_at + after.len();
            out.push(Value {
                groups: vec![Group {
                    words,
                    extent,
                    line: None,
                }],
                flow: false,
            });
        }
    }
    out
}

fn removals(extents: &[Range<usize>], dropped: &[bool]) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < extents.len() {
        if !dropped[i] {
            i += 1;
            continue;
        }
        let first = i;
        while i < extents.len() && dropped[i] {
            i += 1;
        }
        out.push(if i < extents.len() {
            extents[first].start..extents[i].start
        } else if first > 0 {
            extents[first - 1].end..extents[i - 1].end
        } else {
            extents[first].start..extents[i - 1].end
        });
    }
    out
}

pub(crate) fn rewrite(text: &str, from: &str, to: &str) -> Option<(String, usize)> {
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut count = 0;
    for range in parse::body_tag_spans(text) {
        if let Some(new) = renamed(&text[range.clone()], from, to) {
            count += 1;
            if new != text[range.clone()] {
                edits.push((range, new));
            }
        }
    }

    let values = property_values(text);
    let words: Vec<&Word> = values
        .iter()
        .flat_map(|v| &v.groups)
        .flat_map(|g| &g.words)
        .collect();
    let finals: Vec<(String, bool)> = words
        .iter()
        .map(|w| {
            let name = &text[w.name.clone()];
            match renamed(name, from, to) {
                Some(new) => (new.to_lowercase(), true),
                None => (name.to_lowercase(), false),
            }
        })
        .collect();
    let mut dropped = vec![false; words.len()];
    for i in 0..words.len() {
        if !finals[i].1 {
            continue;
        }
        dropped[i] = (0..words.len()).any(|j| {
            j != i && finals[j].0 == finals[i].0 && (!finals[j].1 || (j < i && !dropped[j]))
        });
    }
    let mut index = 0;
    for value in &values {
        let mut group_dropped = Vec::new();
        for group in &value.groups {
            let flags = &dropped[index..index + group.words.len()];
            let renames = finals[index..index + group.words.len()].iter();
            index += group.words.len();
            for ((word, &gone), (_, touched)) in group.words.iter().zip(flags).zip(renames) {
                if *touched {
                    count += 1;
                    if !gone
                        && let Some(new) = renamed(&text[word.name.clone()], from, to)
                        && new != text[word.name.clone()]
                    {
                        edits.push((word.name.clone(), new));
                    }
                }
            }
            let whole = !group.words.is_empty() && flags.iter().all(|&d| d);
            group_dropped.push(whole);
            if !whole {
                let extents: Vec<Range<usize>> =
                    group.words.iter().map(|w| w.extent.clone()).collect();
                for range in removals(&extents, flags) {
                    edits.push((range, String::new()));
                }
            }
        }
        if value.flow {
            let extents: Vec<Range<usize>> =
                value.groups.iter().map(|g| g.extent.clone()).collect();
            for range in removals(&extents, &group_dropped) {
                edits.push((range, String::new()));
            }
        } else {
            for (group, whole) in value.groups.iter().zip(group_dropped) {
                if whole {
                    let range = group.line.clone().unwrap_or(group.extent.clone());
                    edits.push((range, String::new()));
                }
            }
        }
    }
    if count == 0 {
        return None;
    }
    edits.sort_by_key(|(range, _)| range.start);
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (range, new) in edits {
        out.push_str(&text[at..range.start]);
        out.push_str(&new);
        at = range.end;
    }
    out.push_str(&text[at..]);
    Some((out, count))
}

struct Retag {
    id: NoteId,
    path: Arc<Path>,
    text: String,
    count: usize,
    modified: SystemTime,
}

impl Inner {
    fn retags(&self, from: &str, to: &str, report: &mut TagRenameReport) -> Vec<Retag> {
        let ids = self.notes_with_tag(from);
        let found: Vec<(NoteId, Arc<Path>)> = {
            let s = self.state.read();
            ids.iter()
                .filter_map(|&id| s.entry(id).map(|e| (id, e.path.clone())))
                .collect()
        };
        let read: Vec<std::result::Result<Option<Retag>, (PathBuf, VaultError)>> = found
            .into_par_iter()
            .map(|(id, path)| {
                let abs = self.root.join(&path);
                let fail = |e: VaultError| (path.to_path_buf(), e);
                let modified = std::fs::metadata(&abs)
                    .and_then(|m| m.modified())
                    .map_err(|e| fail(io_err(&abs, e)))?;
                let bytes = std::fs::read(&abs).map_err(|e| fail(io_err(&abs, e)))?;
                let text = String::from_utf8(bytes).map_err(|_| {
                    fail(VaultError::Other(format!("not UTF-8: {}", path.display())))
                })?;
                Ok(rewrite(&text, from, to).map(|(text, count)| Retag {
                    id,
                    path: path.clone(),
                    text,
                    count,
                    modified,
                }))
            })
            .collect();
        let mut out = Vec::new();
        for r in read {
            match r {
                Ok(Some(retag)) => out.push(retag),
                Ok(None) => {}
                Err(failure) => report.failed.push(failure),
            }
        }
        out.sort_by_key(|r| r.id);
        out
    }

    pub(crate) fn preview_rename_tag(&self, from: &str, to: &str) -> Result<TagRenameReport> {
        let (from, to) = check_names(from, to)?;
        let _ops = self.ops.lock();
        let mut report = TagRenameReport::default();
        for r in self.retags(from, to, &mut report) {
            report.notes.push(r.id);
            report.occurrences += r.count;
        }
        Ok(report)
    }

    pub(crate) fn rename_tag(&self, from: &str, to: &str) -> Result<TagRenameReport> {
        let (from, to) = check_names(from, to)?;
        let _ops = self.ops.lock();
        let mut report = TagRenameReport::default();
        let mut written = Vec::new();
        let outcomes: Vec<(Retag, Result<(u64, SystemTime)>)> = self
            .retags(from, to, &mut report)
            .into_par_iter()
            .map(|r| {
                let abs = self.root.join(&r.path);
                let outcome = (|| {
                    let now = std::fs::metadata(&abs)
                        .and_then(|m| m.modified())
                        .map_err(|e| io_err(&abs, e))?;
                    if now != r.modified {
                        return Err(VaultError::Conflict(r.path.to_path_buf()));
                    }
                    self.keep_version(&r.path, focus_history::Kind::Bulk)?;
                    fsx::write_atomic(&abs, r.text.as_bytes()).map_err(|e| io_err(&abs, e))?;
                    stat_of(&abs)
                })();
                (r, outcome)
            })
            .collect();
        for (r, outcome) in outcomes {
            match outcome {
                Ok((size, modified)) => {
                    report.notes.push(r.id);
                    report.occurrences += r.count;
                    written.push((r, size, modified));
                }
                Err(e) => report.failed.push((r.path.to_path_buf(), e)),
            }
        }
        if written.is_empty() {
            return Ok(report);
        }
        let mut events = Vec::new();
        {
            let mut s = self.state.write();
            let mut aliases = false;
            for (r, size, modified) in written {
                let entry = entry_from_parsed(r.id, r.path, parse::parse(&r.text), size, modified);
                aliases |= s.replace_content(entry);
                events.push(VaultEvent::NoteChanged(r.id));
            }
            if aliases {
                s.rebuild();
            }
        }
        self.emit(events);
        self.maybe_save_cache();
        Ok(report)
    }
}

fn stat_of(abs: &Path) -> Result<(u64, SystemTime)> {
    let m = std::fs::metadata(abs).map_err(|e| io_err(abs, e))?;
    Ok((m.len(), m.modified().map_err(|e| io_err(abs, e))?))
}

fn check_names<'a>(from: &'a str, to: &'a str) -> Result<(&'a str, &'a str)> {
    for name in [from, to] {
        if !is_valid(name) {
            return Err(VaultError::Other(format!("not a valid tag: {name:?}")));
        }
    }
    Ok((from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, from: &str, to: &str) -> String {
        rewrite(text, from, to).map_or(text.to_string(), |(t, _)| t)
    }

    #[test]
    fn a_nested_tag_follows_its_parent_and_a_longer_name_does_not() {
        assert_eq!(renamed("a/b", "a", "x").as_deref(), Some("x/b"));
        assert_eq!(renamed("A/b/C", "a/B", "z").as_deref(), Some("z/C"));
        assert_eq!(renamed("ab", "a", "x"), None);
        assert_eq!(renamed("a", "a/b", "x"), None);
        assert_eq!(
            run("#a #a/b #a/b/c #a/bc #ab", "a/b", "q"),
            "#a #q #q/c #a/bc #ab"
        );
    }

    #[test]
    fn valid_names() {
        for ok in ["a", "a/b", "y1984", "élévation", "x_y-z"] {
            assert!(is_valid(ok), "{ok}");
        }
        for bad in ["", "#a", "1984", "a b", "a/", "/a", "a//b", "a,b", "a.b"] {
            assert!(!is_valid(bad), "{bad}");
        }
    }

    #[test]
    fn property_forms_keep_their_shape() {
        assert_eq!(
            run("---\ntags: [a, \"b\", 'a/x']\n---\n", "a", "n"),
            "---\ntags: [n, \"b\", 'n/x']\n---\n"
        );
        assert_eq!(
            run("---\ntags: a, b a/x # c\n---\n", "a", "n"),
            "---\ntags: n, b n/x # c\n---\n"
        );
        assert_eq!(
            run("---\ntags:\n  - a\n  - \"#a/x\"\ntag: a/y\n---\n", "a", "n"),
            "---\ntags:\n  - n\n  - \"#n/x\"\ntag: n/y\n---\n"
        );
        assert_eq!(
            run("---\ntags: 2024\n---\n#a", "a", "n"),
            "---\ntags: 2024\n---\n#n"
        );
    }

    #[test]
    fn a_repeated_property_tag_is_kept_once() {
        assert_eq!(
            run("---\ntags: [a, b, c]\n---\n", "a", "b"),
            "---\ntags: [b, c]\n---\n"
        );
        assert_eq!(
            run("---\ntags: [c, b, a]\n---\n", "a", "B"),
            "---\ntags: [c, b]\n---\n"
        );
        assert_eq!(
            run("---\ntags:\n  - a\n  - b\n---\nbody #a\n", "a", "b"),
            "---\ntags:\n  - b\n---\nbody #b\n"
        );
        assert_eq!(
            run("---\ntags: a A b\n---\n", "a", "b"),
            "---\ntags: b\n---\n"
        );
        assert_eq!(
            run("---\ntags: x, a\n---\n", "a", "x"),
            "---\ntags: x\n---\n"
        );
        let (text, count) = rewrite("---\ntags: [x, X]\n---\n", "x", "y").unwrap();
        assert_eq!((text.as_str(), count), ("---\ntags: [y]\n---\n", 2));
    }

    #[test]
    fn tags_in_code_links_urls_and_comments_stay() {
        let text = "`#a` [#a](x.md) [[n|#a]] https://h.org/#a %% #a %%\n```\n#a\n```\n<div>\n#a\n</div>\n\n#a";
        let out = run(text, "a", "b");
        assert_eq!(out, text.replace("\n\n#a", "\n\n#b"));
    }
}
