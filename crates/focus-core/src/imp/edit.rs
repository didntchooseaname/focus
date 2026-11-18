use super::fsx::{self, encode_link_path, file_name, parent_dir, relative_path, slash, stem};
use super::parse::{self, LinkSpan};
use super::resolve::Via;
use super::*;

pub(crate) fn clean_rel(root: &Path, p: &Path) -> Result<PathBuf> {
    let rel = if p.is_absolute() {
        p.strip_prefix(root)
            .map_err(|_| VaultError::OutsideVault(p.to_path_buf()))?
    } else {
        p
    };
    match fsx::normalize(rel) {
        Some(n) if !n.as_os_str().is_empty() => Ok(n),
        _ => Err(VaultError::OutsideVault(p.to_path_buf())),
    }
}

fn exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

pub(crate) fn stat(abs: &Path) -> Result<(u64, SystemTime)> {
    let m = std::fs::metadata(abs).map_err(|e| io_err(abs, e))?;
    Ok((m.len(), m.modified().map_err(|e| io_err(abs, e))?))
}

#[derive(Default)]
pub(crate) struct Relocation {
    pub fs_moves: Vec<(PathBuf, PathBuf)>,
    pub moved: Vec<(PathBuf, PathBuf)>,
    pub redirect: HashMap<PathBuf, PathBuf>,
    pub trash: Vec<PathBuf>,
}

struct Rewrite {
    id: NoteId,
    from: PathBuf,
    path: PathBuf,
    text: String,
    links: usize,
    canvas: bool,
}

impl Rewrite {
    fn record(&self, report: &mut MoveReport) {
        if !self.canvas {
            report.updated_notes.push(self.id);
        }
        report.updated_links += self.links;
        report.rewritten.push(Rewritten {
            id: self.id,
            path: self.path.clone(),
            links: self.links,
            canvas: self.canvas,
        });
    }
}

pub(crate) fn rewrite_canvas(
    s: &State,
    text: &str,
    redirect: &HashMap<PathBuf, PathBuf>,
) -> Option<(String, usize)> {
    let (out, count) = super::canvas::rewrite(text, |r| {
        let old = match s.resolve_canvas(&r.file) {
            Resolved::Note(t) => s.entry(t)?.path.to_path_buf(),
            Resolved::File(p) => p.to_path_buf(),
            _ => return None,
        };
        redirect.get(&old).map(|new| slash(new))
    })?;
    (count > 0).then_some((out, count))
}

pub(crate) fn format_target(
    s: &State,
    text: &str,
    span: &LinkSpan,
    via: Via,
    target: (Option<NoteId>, &Path),
    src_new: &Path,
    new_target: &Path,
) -> Option<String> {
    let (target_id, old_target) = target;
    let raw = &text[span.target_range.clone()];
    let is_note = target_id.is_some();
    let keep_md = !is_note || span.target.to_lowercase().ends_with(".md");
    let strip_md = |p: String| -> String {
        if !keep_md && let Some(stripped) = p.strip_suffix(".md") {
            return stripped.to_string();
        }
        p
    };
    let angle = !span.plain && text[..span.target_range.start].ends_with('<');
    let encode = |p: String| {
        if span.plain || angle {
            p
        } else {
            encode_link_path(&p)
        }
    };
    let root_form = |prefix: &str| encode(format!("{prefix}{}", strip_md(slash(new_target))));
    Some(match via {
        Via::SelfRef | Via::None | Via::Alias => return None,
        Via::Relative => {
            let mut p = slash(&relative_path(parent_dir(src_new), new_target));
            if raw.starts_with("./") && !p.starts_with("../") {
                p = format!("./{p}");
            }
            encode(strip_md(p))
        }
        Via::Root => root_form(if raw.starts_with('/') { "/" } else { "" }),
        Via::Name => {
            let unique = match target_id {
                Some(id) => s
                    .names
                    .notes_named(&s.notes, &stem(new_target))
                    .iter()
                    .all(|&n| n == id || s.by_path.get(new_target) == Some(&n)),
                None => s
                    .names
                    .files_named(&file_name(new_target))
                    .iter()
                    .all(|p| **p == *old_target),
            };
            if span.target.contains('/') || !unique {
                root_form("")
            } else if is_note {
                encode(strip_md(format!("{}.md", stem(new_target))))
            } else {
                encode(file_name(new_target))
            }
        }
    })
}

pub(crate) fn rewrite_text(
    s: &State,
    id: NoteId,
    src_old: &Path,
    src_new: &Path,
    text: &str,
    redirect: &HashMap<PathBuf, PathBuf>,
) -> (String, usize) {
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for span in parse::link_spans(text) {
        if span.target.is_empty() || parse::is_external(&span.target) {
            continue;
        }
        let (res, via) = s.resolve(Some(id), src_old, &span.target);
        let (old_target, target_id): (PathBuf, Option<NoteId>) = match &res {
            Resolved::Note(t) => match s.entry(*t) {
                Some(e) => (e.path.to_path_buf(), Some(*t)),
                None => continue,
            },
            Resolved::File(p) => (p.to_path_buf(), None),
            _ => continue,
        };
        let new_target = redirect
            .get(&old_target)
            .cloned()
            .unwrap_or_else(|| old_target.clone());
        if new_target == old_target && src_new == src_old {
            continue;
        }
        let Some(new_raw) = format_target(
            s,
            text,
            &span,
            via,
            (target_id, &old_target),
            src_new,
            &new_target,
        ) else {
            continue;
        };
        if new_raw != text[span.target_range.clone()] {
            edits.push((span.target_range.clone(), new_raw));
        }
    }
    edits.sort_by_key(|(range, _)| range.start);
    let count = edits.len();
    let mut out = text.to_string();
    for (range, new) in edits.into_iter().rev() {
        out.replace_range(range, &new);
    }
    (out, count)
}

pub(crate) fn trash_destination(root: &Path, rel: &Path) -> PathBuf {
    let dir = root.join(".trash");
    let name = file_name(rel);
    let mut dest = dir.join(&name);
    let (base, ext) = match rel.extension() {
        Some(e) if !rel.is_dir() => (stem(rel), format!(".{}", e.to_string_lossy())),
        _ => (name.clone(), String::new()),
    };
    let mut n = 1;
    while exists(&dest) {
        dest = dir.join(format!("{base} {n}{ext}"));
        n += 1;
    }
    dest
}

impl Inner {
    pub(crate) fn write(
        &self,
        id: NoteId,
        content: &str,
        expected: Option<SystemTime>,
        kind: focus_history::Kind,
    ) -> Result<SystemTime> {
        let _ops = self.ops.lock();
        let path = self.note_path(id)?;
        let abs = self.root.join(&path);
        if let Some(expected) = expected {
            match std::fs::metadata(&abs).and_then(|m| m.modified()) {
                Ok(current) if current != expected => {
                    return Err(VaultError::Conflict(path.to_path_buf()));
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err(VaultError::Conflict(path.to_path_buf()));
                }
                Err(e) => return Err(io_err(&abs, e)),
            }
        }
        self.keep_version(&path, kind)?;
        fsx::write_atomic(&abs, content.as_bytes()).map_err(|e| io_err(&abs, e))?;
        let (size, modified) = stat(&abs)?;
        let entry = entry_from_parsed(id, path, parse::parse(content), size, modified);
        {
            let mut s = self.state.write();
            if s.replace_content(entry) {
                s.rebuild();
            }
        }
        self.emit(vec![VaultEvent::NoteChanged(id)]);
        self.maybe_save_cache();
        Ok(modified)
    }

    pub(crate) fn create_note(&self, relative: &Path, content: &str) -> Result<NoteId> {
        let _ops = self.ops.lock();
        let mut rel = clean_rel(&self.root, relative)?;
        if !fsx::is_markdown(&rel) {
            rel = PathBuf::from(format!("{}.md", rel.display()));
        }
        if self.ignore.excluded(&rel) {
            return Err(VaultError::Other(format!(
                "path excluded from the vault: {}",
                rel.display()
            )));
        }
        let abs = self.root.join(&rel);
        if exists(&abs) {
            return Err(VaultError::AlreadyExists(rel));
        }
        let parent = parent_dir(&rel).to_path_buf();
        let new_folder =
            !parent.as_os_str().is_empty() && !self.state.read().folders.contains(parent.as_path());
        std::fs::create_dir_all(abs.parent().unwrap_or(&self.root)).map_err(|e| io_err(&abs, e))?;
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&abs)
                .map_err(|e| match e.kind() {
                    std::io::ErrorKind::AlreadyExists => VaultError::AlreadyExists(rel.clone()),
                    _ => io_err(&abs, e),
                })?;
            f.write_all(content.as_bytes())
                .map_err(|e| io_err(&abs, e))?;
        }
        let (size, modified) = stat(&abs)?;
        let id = {
            let mut s = self.state.write();
            let id = s.alloc_id();
            s.insert_note(entry_from_parsed(
                id,
                fsx::stored(&rel),
                parse::parse(content),
                size,
                modified,
            ));
            s.add_folder_chain(&parent);
            s.rebuild();
            id
        };
        let mut events = vec![VaultEvent::NoteAdded(id)];
        if new_folder {
            events.push(VaultEvent::TreeChanged);
        }
        self.emit(events);
        Ok(id)
    }

    pub(crate) fn create_folder(&self, relative: &Path) -> Result<()> {
        let _ops = self.ops.lock();
        let rel = clean_rel(&self.root, relative)?;
        let abs = self.root.join(&rel);
        if exists(&abs) {
            return Err(VaultError::AlreadyExists(rel));
        }
        std::fs::create_dir_all(&abs).map_err(|e| io_err(&abs, e))?;
        {
            let mut s = self.state.write();
            s.add_folder_chain(&rel);
            s.rebuild_tree();
        }
        self.emit(vec![VaultEvent::TreeChanged]);
        Ok(())
    }

    pub(crate) fn rename(&self, from: &Path, to: &Path, update_links: bool) -> Result<MoveReport> {
        let _ops = self.ops.lock();
        match self.plan_rename(from, to)? {
            Some(plan) => self.relocate(plan, update_links),
            None => Ok(MoveReport::default()),
        }
    }

    pub(crate) fn preview_rename(&self, from: &Path, to: &Path) -> Result<MoveReport> {
        let _ops = self.ops.lock();
        let Some(plan) = self.plan_rename(from, to)? else {
            return Ok(MoveReport::default());
        };
        let mut report = MoveReport::default();
        for r in self.rewrites(&plan)? {
            r.record(&mut report);
        }
        Ok(report)
    }

    fn plan_rename(&self, from: &Path, to: &Path) -> Result<Option<Relocation>> {
        let from = clean_rel(&self.root, from)?;
        let mut to = clean_rel(&self.root, to)?;
        if from == to {
            return Ok(None);
        }
        let abs_from = self.root.join(&from);
        let meta = std::fs::symlink_metadata(&abs_from).map_err(|e| io_err(&abs_from, e))?;
        if fsx::is_markdown(&from) && !meta.is_dir() && to.extension().is_none() {
            to = PathBuf::from(format!("{}.md", to.display()));
        }
        if exists(&self.root.join(&to)) {
            return Err(VaultError::AlreadyExists(to));
        }
        if to.starts_with(&from) {
            return Err(VaultError::Other(format!(
                "a folder cannot move into itself: {}",
                to.display()
            )));
        }
        if self.ignore.excluded(&to) {
            return Err(VaultError::Other(format!(
                "path excluded from the vault: {}",
                to.display()
            )));
        }
        let mut plan = Relocation::default();
        plan.fs_moves.push((from.clone(), to.clone()));
        {
            let s = self.state.read();
            let under = |dir: &Path, new_dir: &Path, out: &mut Vec<(PathBuf, PathBuf)>| {
                for p in s.by_path.keys().chain(s.files.keys()) {
                    if let Ok(rest) = p.strip_prefix(dir) {
                        out.push((p.to_path_buf(), new_dir.join(rest)));
                    }
                }
            };
            if meta.is_dir() {
                under(&from, &to, &mut plan.moved);
            } else {
                plan.moved.push((from.clone(), to.clone()));
                let policy = self.policy.read().clone();
                if fsx::is_markdown(&from) && policy.follow_note && policy.per_note {
                    let folder = self.obsidian.read().attachment_folder.clone();
                    let old_dir = attach::note_attachment_dir(&folder, &from, true);
                    let new_dir = attach::note_attachment_dir(&folder, &to, true);
                    let has_notes = s.by_path.keys().any(|p| p.starts_with(&old_dir));
                    if old_dir != new_dir
                        && s.folders.contains(old_dir.as_path())
                        && !has_notes
                        && !exists(&self.root.join(&new_dir))
                    {
                        plan.fs_moves.push((old_dir.clone(), new_dir.clone()));
                        under(&old_dir, &new_dir, &mut plan.moved);
                    }
                }
            }
        }
        plan.redirect = plan.moved.iter().cloned().collect();
        Ok(Some(plan))
    }

    fn rewrites(&self, plan: &Relocation) -> Result<Vec<Rewrite>> {
        let moved_paths: HashMap<&Path, &Path> = plan
            .moved
            .iter()
            .map(|(a, b)| (a.as_path(), b.as_path()))
            .collect();
        let s = self.state.read();
        let mut sources: Vec<NoteId> = Vec::new();
        for old in plan.redirect.keys() {
            if let Some(&id) = s.by_path.get(old.as_path()) {
                sources.push(id);
                sources.extend(s.incoming.get(&id).into_iter().flatten().copied());
            } else if let Some(v) = s.file_incoming.get(old.as_path()) {
                sources.extend(v.iter().copied());
            }
        }
        sources.sort_unstable();
        sources.dedup();
        let mut out = Vec::new();
        for id in sources {
            let Some(e) = s.source(id) else { continue };
            let old = e.path.to_path_buf();
            let new = moved_paths
                .get(old.as_path())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| old.clone());
            let abs = self.root.join(&old);
            let canvas = walk::is_canvas(&old);
            let (new_text, links) = if canvas {
                let bytes = std::fs::read(&abs).map_err(|err| io_err(&abs, err))?;
                let Ok(text) = String::from_utf8(bytes) else {
                    continue;
                };
                match rewrite_canvas(&s, &text, &plan.redirect) {
                    Some(done) => done,
                    None => continue,
                }
            } else {
                let text = read_lossy(&abs).map_err(|err| io_err(&abs, err))?;
                rewrite_text(&s, id, &old, &new, &text, &plan.redirect)
            };
            if links > 0 {
                out.push(Rewrite {
                    id,
                    from: old,
                    path: new,
                    text: new_text,
                    links,
                    canvas,
                });
            }
        }
        Ok(out)
    }

    pub(crate) fn relocate(&self, plan: Relocation, update_links: bool) -> Result<MoveReport> {
        let mut report = MoveReport::default();
        let rewrites = if update_links {
            self.rewrites(&plan)?
        } else {
            Vec::new()
        };

        for r in &rewrites {
            self.keep_version(&r.from, focus_history::Kind::Bulk)?;
        }
        for (from, to) in &plan.fs_moves {
            let (a, b) = (self.root.join(from), self.root.join(to));
            if let Some(parent) = b.parent() {
                std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
            }
            std::fs::rename(&a, &b).map_err(|e| io_err(&a, e))?;
            self.follow_move(from, to);
        }
        for r in &rewrites {
            let abs = self.root.join(&r.path);
            fsx::write_atomic(&abs, r.text.as_bytes()).map_err(|e| io_err(&abs, e))?;
            r.record(&mut report);
        }
        let mut trashed = Vec::new();
        for rel in &plan.trash {
            let abs = self.root.join(rel);
            let dest = trash_destination(&self.root, rel);
            std::fs::create_dir_all(self.root.join(".trash")).map_err(|e| io_err(&dest, e))?;
            std::fs::rename(&abs, &dest).map_err(|e| io_err(&abs, e))?;
            if let Ok(inside) = dest.strip_prefix(&self.root) {
                self.follow_move(rel, inside);
            }
            trashed.push(rel.clone());
        }

        let mut events = Vec::new();
        {
            let mut s = self.state.write();
            let mut moved_notes = Vec::new();
            for (old, new) in &plan.moved {
                if s.outside.remove(old.as_path()) {
                    s.outside.insert(fsx::stored(new));
                }
                if let Some(&id) = s.by_path.get(old.as_path()) {
                    let entry = s.remove_note(id).expect("id taken from by_path");
                    moved_notes.push((id, entry, old.clone(), new.clone()));
                } else if let Some(f) = s.files.remove(old.as_path()) {
                    if let Some(&id) = s.canvas_by_path.get(old.as_path()) {
                        let entry = s.remove_canvas(id).expect("id taken from canvas_by_path");
                        s.insert_canvas(moved_entry(&entry, fsx::stored(new), f.size, f.modified));
                    }
                    s.files.insert(fsx::stored(new), f);
                }
            }
            for (id, entry, old, new) in moved_notes {
                let (size, modified) =
                    stat(&self.root.join(&new)).unwrap_or((entry.note.size, entry.note.modified));
                s.insert_note(moved_entry(&entry, fsx::stored(&new), size, modified));
                events.push(VaultEvent::NoteMoved {
                    id,
                    from: old,
                    to: new,
                });
            }
            for (from, to) in &plan.fs_moves {
                if s.folders.contains(from.as_path()) {
                    let inside: Vec<Arc<Path>> = s
                        .folders
                        .iter()
                        .filter(|f| f.starts_with(from))
                        .cloned()
                        .collect();
                    for f in inside {
                        s.folders.remove(&f);
                        let rest = f.strip_prefix(from).unwrap_or(Path::new(""));
                        s.folders.insert(fsx::stored(&to.join(rest)));
                    }
                }
                s.add_folder_chain(parent_dir(to));
            }
            for r in rewrites {
                let abs = self.root.join(&r.path);
                let (size, modified) = stat(&abs)?;
                let path = fsx::stored(&r.path);
                if r.canvas {
                    if let Some(f) = s.files.get_mut(&path) {
                        (f.size, f.modified) = (size, modified);
                    }
                    let parsed = super::canvas::parse(&r.text);
                    s.insert_canvas(entry_from_parsed(r.id, path, parsed, size, modified));
                    continue;
                }
                s.insert_note(entry_from_parsed(
                    r.id,
                    path,
                    parse::parse(&r.text),
                    size,
                    modified,
                ));
                events.push(VaultEvent::NoteChanged(r.id));
            }
            for rel in trashed {
                if let Some(&id) = s.by_path.get(rel.as_path()) {
                    s.remove_note(id);
                    events.push(VaultEvent::NoteRemoved { id, path: rel });
                } else {
                    s.files.remove(rel.as_path());
                }
            }
            s.rebuild();
        }
        events.push(VaultEvent::TreeChanged);
        self.emit(events);
        Ok(report)
    }

    pub(crate) fn move_to_trash(&self, rel: &Path, kind: TrashKind) -> Result<Option<PathBuf>> {
        let abs = self.root.join(rel);
        std::fs::symlink_metadata(&abs).map_err(|e| io_err(&abs, e))?;
        match kind {
            TrashKind::Vault => {
                let dest = trash_destination(&self.root, rel);
                std::fs::create_dir_all(self.root.join(".trash")).map_err(|e| io_err(&dest, e))?;
                std::fs::rename(&abs, &dest).map_err(|e| io_err(&abs, e))?;
                let inside = dest.strip_prefix(&self.root).map(Path::to_path_buf).ok();
                if let Some(inside) = &inside {
                    self.follow_move(rel, inside);
                }
                Ok(inside)
            }
            #[cfg(unix)]
            TrashKind::System => {
                let data_home = super::xdg_trash::data_home().map_err(|e| io_err(&abs, e))?;
                super::xdg_trash::trash(&abs, &data_home).map_err(|e| io_err(&abs, e))?;
                Ok(None)
            }
            #[cfg(windows)]
            TrashKind::System => {
                super::windows_trash::trash(&abs).map_err(|e| io_err(&abs, e))?;
                Ok(None)
            }
            #[cfg(not(any(unix, windows)))]
            TrashKind::System => Err(VaultError::Other(
                "the system trash is only implemented for the FreeDesktop specification".into(),
            )),
        }
    }

    pub(crate) fn trash(&self, relative: &Path, kind: TrashKind) -> Result<()> {
        let _ops = self.ops.lock();
        let rel = clean_rel(&self.root, relative)?;
        self.move_to_trash(&rel, kind)?;
        let mut events = Vec::new();
        {
            let mut s = self.state.write();
            let ids: Vec<(NoteId, Arc<Path>)> = s
                .by_path
                .iter()
                .filter(|(p, _)| p.starts_with(&rel))
                .map(|(p, &id)| (id, p.clone()))
                .collect();
            for (id, path) in ids {
                s.remove_note(id);
                events.push(VaultEvent::NoteRemoved {
                    id,
                    path: path.to_path_buf(),
                });
            }
            s.files.retain(|p, _| !p.starts_with(&rel));
            s.folders.retain(|p| !p.starts_with(&rel));
            s.not_followed.retain(|p| !p.starts_with(&rel));
            s.rebuild();
        }
        events.push(VaultEvent::TreeChanged);
        self.emit(events);
        Ok(())
    }

    pub(crate) fn toggle_task(&self, note: NoteId, line: u32) -> Result<bool> {
        let (path, expected) = {
            let s = self.state.read();
            let e = s.entry(note).ok_or(VaultError::UnknownNote(note))?;
            (e.path.clone(), e.note.modified)
        };
        let text = self.read(note)?;
        let mut start = 0;
        for (n, l) in text.split_inclusive('\n').enumerate() {
            if n as u32 == line {
                let content = l.trim_end_matches(['\n', '\r']);
                let Some((mark, done)) = parse::task_mark(content) else {
                    return Err(VaultError::Other(format!(
                        "no checkbox on line {} of {}",
                        line + 1,
                        path.display()
                    )));
                };
                let mark_len = content[mark..].chars().next().map_or(1, char::len_utf8);
                let mut new = String::with_capacity(text.len());
                new.push_str(&text[..start + mark]);
                new.push(if done { ' ' } else { 'x' });
                new.push_str(&text[start + mark + mark_len..]);
                self.write(note, &new, Some(expected), focus_history::Kind::Edit)?;
                return Ok(!done);
            }
            start += l.len();
        }
        Err(VaultError::Other(format!(
            "line {} not in {}",
            line + 1,
            path.display()
        )))
    }
}
