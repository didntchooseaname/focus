use super::edit::{format_target, rewrite_canvas, rewrite_text, stat};
use super::fsx::{encode_link_path, parent_dir, relative_path, slash, stem};
use super::parse;
use super::*;
use std::ops::Range;

fn split(text: &str) -> (Option<&str>, &str) {
    match parse::frontmatter_end(text) {
        Some(end) => (Some(&text[..end]), &text[end..]),
        None => (None, text),
    }
}

fn join_bodies(first: &str, second: &str, eol: &str) -> String {
    let first = first.trim_end_matches(['\n', '\r']);
    let second = second.trim_start_matches(['\n', '\r']);
    match (first.is_empty(), second.is_empty()) {
        (true, _) => second.to_string(),
        (false, true) => format!("{first}{eol}"),
        (false, false) => format!("{first}{eol}{eol}{second}"),
    }
}

fn word_list(key: &str) -> Option<bool> {
    match key.to_ascii_lowercase().as_str() {
        "tags" | "tag" => Some(true),
        "aliases" | "alias" | "cssclasses" => Some(false),
        _ => None,
    }
}

fn items(key: &str, value: &PropValue) -> Option<Vec<String>> {
    let mut out = Vec::new();
    if let Some(split_spaces) = word_list(key) {
        let texts: Vec<String> = match value {
            PropValue::Text(t) => vec![t.clone()],
            PropValue::List(list) => list
                .iter()
                .filter_map(|v| match v {
                    PropValue::Text(t) => Some(t.clone()),
                    PropValue::Number(n) => Some(parse::format_number(*n)),
                    _ => None,
                })
                .collect(),
            PropValue::Null => Vec::new(),
            _ => return None,
        };
        for text in texts {
            for word in text.split(|c: char| c == ',' || (split_spaces && c.is_whitespace())) {
                let word = word.trim();
                let word = match split_spaces {
                    true => parse::tag_name(word).map(str::to_string),
                    false => (!word.is_empty()).then(|| word.to_string()),
                };
                out.extend(word.map(|w| yaml_scalar(&w)));
            }
        }
        return Some(out);
    }
    let PropValue::List(list) = value else {
        return None;
    };
    for item in list {
        out.push(match item {
            PropValue::Text(t) => yaml_scalar(t),
            PropValue::Number(n) => parse::format_number(*n),
            PropValue::Bool(b) => b.to_string(),
            _ => return None,
        });
    }
    Some(out)
}

fn yaml_scalar(text: &str) -> String {
    let plain = !text.is_empty()
        && !text.starts_with(|c: char| "-?:,[]{}#&*!|>'\"%@`".contains(c) || c.is_whitespace())
        && !text.ends_with(char::is_whitespace)
        && !text.contains(": ")
        && !text.contains(" #")
        && parse::scalar(text) == PropValue::Text(text.to_string());
    if plain {
        return text.to_string();
    }
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn item_key(key: &str, item: &str) -> String {
    match word_list(key) {
        Some(true) => item.trim_matches('"').to_lowercase(),
        _ => item.to_string(),
    }
}

fn entries(yaml: &str) -> Vec<(String, Range<usize>)> {
    let mut out: Vec<(String, Range<usize>)> = Vec::new();
    for (at, line) in parse::line_offsets(yaml) {
        let t = line.trim();
        let top = !line.starts_with([' ', '\t']) && !t.is_empty() && !t.starts_with('#');
        let key = line
            .find(':')
            .filter(|_| top && !t.starts_with("- ") && t != "-")
            .map(|c| line[..c].trim().trim_matches(['"', '\'']).to_string());
        match key {
            Some(key) if !key.is_empty() => out.push((key, at..yaml.len())),
            _ => {}
        }
        if let Some(last) = out.len().checked_sub(2) {
            let start = out[last + 1].1.start;
            out[last].1.end = start;
        }
    }
    out
}

fn join_properties(destination: &str, source: &str, eol: &str) -> String {
    let lines = |yaml: &str| -> Vec<String> { yaml.lines().map(str::to_string).collect() };
    let parsed = |yaml: &str| {
        let lines = lines(yaml);
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        parse::parse_yaml(&refs)
    };
    let dest_values = parsed(destination);
    let src_values = parsed(source);
    let value_of = |values: &[(String, PropValue)], key: &str| {
        values
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.clone())
    };
    let dest_entries = entries(destination);
    let mut out = destination.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(eol);
    }
    let mut replaced: Vec<(Range<usize>, String)> = Vec::new();
    for (key, range) in entries(source) {
        let Some((dest_key, dest_range)) = dest_entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&key))
        else {
            let text = &source[range];
            out.push_str(text.trim_end_matches(['\n', '\r']));
            out.push_str(eol);
            continue;
        };
        let (Some(dv), Some(sv)) = (
            value_of(&dest_values, dest_key),
            value_of(&src_values, &key),
        ) else {
            continue;
        };
        let lists = word_list(dest_key).is_some()
            || (matches!(dv, PropValue::List(_)) && matches!(sv, PropValue::List(_)));
        if !lists {
            continue;
        }
        let (Some(mut kept), Some(added)) = (items(dest_key, &dv), items(dest_key, &sv)) else {
            continue;
        };
        let mut seen: Vec<String> = kept.iter().map(|i| item_key(dest_key, i)).collect();
        let before = kept.len();
        for item in added {
            let id = item_key(dest_key, &item);
            if !seen.contains(&id) {
                seen.push(id);
                kept.push(item);
            }
        }
        if kept.len() == before {
            continue;
        }
        let mut entry = format!("{dest_key}:{eol}");
        for item in kept {
            entry.push_str(&format!("  - {item}{eol}"));
        }
        replaced.push((dest_range.clone(), entry));
    }
    replaced.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    for (range, entry) in replaced {
        out.replace_range(range, &entry);
    }
    out
}

fn merged_text(destination: &str, source: &str, placement: Placement) -> String {
    let eol = if destination.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let (dest_fm, dest_body) = split(destination);
    let (src_fm, src_body) = split(source);
    let frontmatter = match (dest_fm, src_fm) {
        (Some(d), Some(s)) => {
            let yaml = |fm: &str| {
                let start = fm.find('\n').map_or(fm.len(), |i| i + 1);
                let close = fm.trim_end_matches(['\n', '\r']);
                let end = close.rfind('\n').map_or(start, |i| i + 1).max(start);
                (start, end)
            };
            let (ds, de) = yaml(d);
            let (ss, se) = yaml(s);
            format!(
                "{}{}{}",
                &d[..ds],
                join_properties(&d[ds..de], &s[ss..se], eol),
                &d[de..]
            )
        }
        (Some(fm), None) | (None, Some(fm)) => fm.to_string(),
        (None, None) => String::new(),
    };
    let body = match placement {
        Placement::End => join_bodies(dest_body, src_body, eol),
        Placement::Start => join_bodies(src_body, dest_body, eol),
    };
    let body = if body.is_empty() || body.ends_with('\n') {
        body
    } else {
        format!("{body}{eol}")
    };
    format!("{frontmatter}{body}")
}

fn inserted(note: &str, text: &str, placement: Placement) -> String {
    let eol = if note.contains("\r\n") { "\r\n" } else { "\n" };
    let (frontmatter, body) = split(note);
    let body = match placement {
        Placement::End => join_bodies(body, text, eol),
        Placement::Start => join_bodies(text, body, eol),
    };
    let body = if body.is_empty() || body.ends_with('\n') {
        body
    } else {
        format!("{body}{eol}")
    };
    format!("{}{body}", frontmatter.unwrap_or(""))
}

fn strict_read(abs: &Path, rel: &Path) -> Result<String> {
    let bytes = std::fs::read(abs).map_err(|e| io_err(abs, e))?;
    String::from_utf8(bytes).map_err(|_| VaultError::Other(format!("not UTF-8: {}", rel.display())))
}

struct MergePlan {
    source: (NoteId, Arc<Path>),
    destination: (NoteId, Arc<Path>),
    source_text: String,
    text: String,
    others: Vec<(NoteId, Arc<Path>, String, usize, bool)>,
    links: usize,
}

impl Inner {
    fn plan_merge(
        &self,
        source: NoteId,
        destination: NoteId,
        placement: Placement,
    ) -> Result<MergePlan> {
        if source == destination {
            return Err(VaultError::Other(
                "a note cannot be merged into itself".into(),
            ));
        }
        let s = self.state.read();
        let src = s
            .entry(source)
            .ok_or(VaultError::UnknownNote(source))?
            .path
            .clone();
        let dst = s
            .entry(destination)
            .ok_or(VaultError::UnknownNote(destination))?
            .path
            .clone();
        let source_text = strict_read(&self.root.join(&src), &src)?;
        let dest_text = strict_read(&self.root.join(&dst), &dst)?;
        let redirect: HashMap<PathBuf, PathBuf> = [(src.to_path_buf(), dst.to_path_buf())]
            .into_iter()
            .collect();
        let mut links = 0;
        let (_, own) = rewrite_text(&s, source, &src, &src, &source_text, &redirect);
        links += own;
        let (moved_text, _) = rewrite_text(&s, source, &src, &dst, &source_text, &redirect);
        let (dest_new, to_source) =
            rewrite_text(&s, destination, &dst, &dst, &dest_text, &redirect);
        links += to_source;
        let mut sources: Vec<NoteId> = s.incoming.get(&source).cloned().unwrap_or_default();
        sources.sort_unstable();
        sources.dedup();
        let mut others = Vec::new();
        for id in sources {
            if id == source || id == destination {
                continue;
            }
            let Some(e) = s.source(id) else { continue };
            let path = e.path.clone();
            let abs = self.root.join(&path);
            let canvas = walk::is_canvas(&path);
            let rewritten = if canvas {
                let Ok(text) = strict_read(&abs, &path) else {
                    continue;
                };
                rewrite_canvas(&s, &text, &redirect)
            } else {
                let text = strict_read(&abs, &path)?;
                Some(rewrite_text(&s, id, &path, &path, &text, &redirect)).filter(|(_, n)| *n > 0)
            };
            if let Some((text, n)) = rewritten {
                links += n;
                others.push((id, path, text, n, canvas));
            }
        }
        Ok(MergePlan {
            source: (source, src),
            destination: (destination, dst),
            text: merged_text(&dest_new, &moved_text, placement),
            source_text,
            others,
            links,
        })
    }

    pub(crate) fn preview_merge(&self, source: NoteId, destination: NoteId) -> Result<MergeReport> {
        let _ops = self.ops.lock();
        let plan = self.plan_merge(source, destination, Placement::End)?;
        Ok(MergeReport {
            links: plan.links,
            rewritten: plan.others.iter().map(rewritten).collect(),
            trashed: None,
        })
    }

    fn keep_text(&self, rel: &Path, text: &str) -> Result<()> {
        let Some(history) = self.history() else {
            return Ok(());
        };
        history
            .keep(rel, focus_history::Kind::Bulk, || {
                Ok(text.as_bytes().to_vec())
            })
            .map(|_| ())
            .map_err(super::history::from_history)
    }

    pub(crate) fn merge(
        &self,
        source: NoteId,
        destination: NoteId,
        placement: Placement,
        trash: TrashKind,
    ) -> Result<MergeReport> {
        let _ops = self.ops.lock();
        let plan = self.plan_merge(source, destination, placement)?;
        let (src_id, src) = plan.source.clone();
        let (dst_id, dst) = plan.destination.clone();
        self.keep_version(&dst, focus_history::Kind::Bulk)?;
        self.keep_text(&dst, &plan.source_text)?;
        for (_, path, _, _, _) in &plan.others {
            self.keep_version(path, focus_history::Kind::Bulk)?;
        }
        let abs = self.root.join(&dst);
        fsx::write_atomic(&abs, plan.text.as_bytes()).map_err(|e| io_err(&abs, e))?;
        for (_, path, text, _, _) in &plan.others {
            let abs = self.root.join(path);
            fsx::write_atomic(&abs, text.as_bytes()).map_err(|e| io_err(&abs, e))?;
        }
        let trashed = self.move_to_trash(&src, trash);

        let mut events = Vec::new();
        {
            let mut s = self.state.write();
            let (size, modified) = stat(&abs)?;
            s.insert_note(entry_from_parsed(
                dst_id,
                dst.clone(),
                parse::parse(&plan.text),
                size,
                modified,
            ));
            events.push(VaultEvent::NoteChanged(dst_id));
            for (id, path, text, _, canvas) in &plan.others {
                let (size, modified) = stat(&self.root.join(path))?;
                if *canvas {
                    if let Some(f) = s.files.get_mut(path) {
                        (f.size, f.modified) = (size, modified);
                    }
                    let parsed = super::canvas::parse(text);
                    s.insert_canvas(entry_from_parsed(*id, path.clone(), parsed, size, modified));
                    continue;
                }
                s.insert_note(entry_from_parsed(
                    *id,
                    path.clone(),
                    parse::parse(text),
                    size,
                    modified,
                ));
                events.push(VaultEvent::NoteChanged(*id));
            }
            if trashed.is_ok() {
                s.remove_note(src_id);
                events.push(VaultEvent::NoteRemoved {
                    id: src_id,
                    path: src.to_path_buf(),
                });
            }
            s.rebuild();
        }
        events.push(VaultEvent::TreeChanged);
        self.emit(events);
        self.maybe_save_cache();
        let trashed = trashed?;
        Ok(MergeReport {
            links: plan.links,
            rewritten: plan.others.iter().map(rewritten).collect(),
            trashed,
        })
    }

    pub(crate) fn insert_text(
        &self,
        id: NoteId,
        text: &str,
        placement: Placement,
    ) -> Result<SystemTime> {
        let path = self.state.read().entry(id).map(|e| e.path.clone());
        let path = path.ok_or(VaultError::UnknownNote(id))?;
        let abs = self.root.join(&path);
        let modified = std::fs::metadata(&abs)
            .and_then(|m| m.modified())
            .map_err(|e| io_err(&abs, e))?;
        let current = strict_read(&abs, &path)?;
        self.write(
            id,
            &inserted(&current, text, placement),
            Some(modified),
            focus_history::Kind::Bulk,
        )
    }

    pub(crate) fn move_text(&self, from: NoteId, to: &Path, text: &str) -> String {
        let s = self.state.read();
        let Some(path) = s.entry(from).map(|e| e.path.clone()) else {
            return text.to_string();
        };
        let mut edits = Vec::new();
        for span in parse::link_spans(text) {
            if span.target.is_empty() || parse::is_external(&span.target) {
                continue;
            }
            let (here, via) = s.resolve(Some(from), &path, &span.target);
            let (there, _) = s.resolve(Some(from), to, &span.target);
            if here == there {
                continue;
            }
            let target = match &here {
                Resolved::Note(t) => s.entry(*t).map(|e| (Some(*t), e.path.to_path_buf())),
                Resolved::File(p) => Some((None, p.to_path_buf())),
                _ => None,
            };
            let Some((id, old)) = target else { continue };
            if let Some(new) = format_target(&s, text, &span, via, (id, &old), to, &old) {
                edits.push((span.target_range.clone(), new));
            }
        }
        let mut out = text.to_string();
        edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        for (range, new) in edits {
            out.replace_range(range, &new);
        }
        out
    }

    pub(crate) fn link_to(&self, from: NoteId, to: NoteId) -> Option<String> {
        let s = self.state.read();
        let from = s.entry(from)?.path.clone();
        let target = s.entry(to)?.path.clone();
        let cfg = self.obsidian.read().clone();
        let name = stem(&target);
        let unique = s.names.notes_named(&s.notes, &name).len() <= 1;
        let path = match cfg.new_link_format.as_str() {
            "absolute" => slash(&target),
            "relative" => slash(&relative_path(parent_dir(&from), &target)),
            _ if unique => format!("{name}.md"),
            _ => slash(&target),
        };
        Some(if cfg.use_markdown_links {
            format!("[{name}]({})", encode_link_path(&path))
        } else {
            let path = path.strip_suffix(".md").unwrap_or(&path);
            format!("[[{path}]]")
        })
    }
}

fn rewritten(other: &(NoteId, Arc<Path>, String, usize, bool)) -> Rewritten {
    let (id, path, _, links, canvas) = other;
    Rewritten {
        id: *id,
        path: path.to_path_buf(),
        links: *links,
        canvas: *canvas,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_join_at_the_end_or_the_start() {
        assert_eq!(
            merged_text("# D\n\ntext\n", "# S\nmore\n", Placement::End),
            "# D\n\ntext\n\n# S\nmore\n"
        );
        assert_eq!(
            merged_text("---\na: 1\n---\nd\n", "s", Placement::Start),
            "---\na: 1\n---\ns\n\nd\n"
        );
        assert_eq!(
            merged_text("", "---\nb: 2\n---\ns\n", Placement::End),
            "---\nb: 2\n---\ns\n"
        );
        assert_eq!(
            merged_text("d\r\n", "s\r\n", Placement::End),
            "d\r\n\r\ns\r\n"
        );
    }

    #[test]
    fn properties_join_lists_and_keep_the_destination_values() {
        let d = "---\ntitle: D\ntags: [a, b]\naliases:\n  - Dee\nrelated:\n  - \"[[X]]\"\n---\nd\n";
        let s = "---\ntitle: S\ntags: B c\naliases: [Ess]\nstatus: draft\nrelated:\n  - \"[[Y]]\"\n---\ns\n";
        assert_eq!(
            merged_text(d, s, Placement::End),
            "---\ntitle: D\ntags:\n  - a\n  - b\n  - c\naliases:\n  - Dee\n  - Ess\nrelated:\n  - \"[[X]]\"\n  - \"[[Y]]\"\nstatus: draft\n---\nd\n\ns\n"
        );
    }

    #[test]
    fn scalars_read_back_as_written() {
        for (text, out) in [
            ("plain", "plain"),
            ("[[Note]]", "\"[[Note]]\""),
            ("true", "\"true\""),
            ("12", "\"12\""),
            ("a: b", "\"a: b\""),
            ("say \"hi\"", "say \"hi\""),
            ("#x", "\"#x\""),
        ] {
            assert_eq!(yaml_scalar(text), out);
        }
    }

    #[test]
    fn text_goes_after_the_frontmatter_or_at_the_end() {
        assert_eq!(
            inserted("---\na: 1\n---\nbody\n", "new", Placement::Start),
            "---\na: 1\n---\nnew\n\nbody\n"
        );
        assert_eq!(inserted("body", "new\n", Placement::End), "body\n\nnew\n");
        assert_eq!(inserted("", "new", Placement::End), "new\n");
    }
}
