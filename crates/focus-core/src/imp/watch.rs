use super::fsx::{is_markdown, parent_dir};
use super::*;
use notify::{RecursiveMode, Watcher as _};
use std::sync::Weak;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

const CONFIG_DIR: &str = ".obsidian";
const QUIET: Duration = Duration::from_millis(100);
const MAX_BATCH: Duration = Duration::from_secs(2);

enum Msg {
    Fs(notify::Result<notify::Event>),
    Stop,
}

pub(crate) struct Watcher {
    stop: Sender<Msg>,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "only the tests wait for the watches")
    )]
    ready: Arc<(parking_lot::Mutex<bool>, parking_lot::Condvar)>,
}

impl Watcher {
    #[cfg(test)]
    pub(crate) fn wait_ready(&self, timeout: std::time::Duration) -> bool {
        let (done, signal) = &*self.ready;
        let mut done = done.lock();
        if !*done {
            signal.wait_for(&mut done, timeout);
        }
        *done
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.stop.send(Msg::Stop);
    }
}

pub(crate) fn start(vault: &Arc<Vault>) -> Result<Watcher> {
    let (tx, rx) = channel::<Msg>();
    let fs_tx = tx.clone();
    let mut watcher = notify::recommended_watcher(move |ev| {
        let _ = fs_tx.send(Msg::Fs(ev));
    })
    .map_err(|e| VaultError::Other(format!("disk watcher: {e}")))?;
    let weak = Arc::downgrade(vault);
    let root = vault.inner.root.clone();
    let folders: Vec<Arc<Path>> = vault.inner.state.read().folders.iter().cloned().collect();
    let ready = Arc::new((parking_lot::Mutex::new(false), parking_lot::Condvar::new()));
    let ready_set = ready.clone();
    std::thread::Builder::new()
        .name("focus-watch".into())
        .spawn(move || {
            let _ = watcher.watch(&root, RecursiveMode::NonRecursive);
            let _ = watcher.watch(&root.join(CONFIG_DIR), RecursiveMode::NonRecursive);
            for f in folders {
                let _ = watcher.watch(&root.join(f), RecursiveMode::NonRecursive);
            }
            if let Some(vault) = weak.upgrade() {
                vault.inner.reload_property_types();
                let (added, removed) = vault.inner.apply_batch(HashSet::new(), true);
                drop(vault);
                for d in removed {
                    let _ = watcher.unwatch(&root.join(d));
                }
                for d in added {
                    let _ = watcher.watch(&root.join(d), RecursiveMode::NonRecursive);
                }
            }
            *ready_set.0.lock() = true;
            ready_set.1.notify_all();
            run(weak, rx, watcher, root);
        })
        .map_err(|e| VaultError::Other(format!("watcher thread: {e}")))?;
    Ok(Watcher { stop: tx, ready })
}

fn run(
    weak: Weak<Vault>,
    rx: std::sync::mpsc::Receiver<Msg>,
    mut watcher: notify::RecommendedWatcher,
    root: PathBuf,
) {
    loop {
        let mut batch = Batch::default();
        match rx.recv() {
            Ok(Msg::Fs(ev)) => batch.add(&root, ev),
            Ok(Msg::Stop) | Err(_) => return,
        }
        let started = Instant::now();
        loop {
            let left = MAX_BATCH.saturating_sub(started.elapsed());
            if left.is_zero() {
                break;
            }
            match rx.recv_timeout(QUIET.min(left)) {
                Ok(Msg::Fs(ev)) => batch.add(&root, ev),
                Ok(Msg::Stop) => return,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        let Some(vault) = weak.upgrade() else { return };
        let dir = Path::new(CONFIG_DIR);
        if batch.rescan || batch.paths.contains(dir) {
            let _ = watcher.watch(&root.join(dir), RecursiveMode::NonRecursive);
        }
        if batch.rescan
            || batch.paths.contains(dir)
            || batch.paths.contains(&dir.join(config::TYPES_FILE))
        {
            vault.inner.reload_property_types();
        }
        let (added, removed) = vault.inner.apply_batch(batch.paths, batch.rescan);
        for d in removed {
            let _ = watcher.unwatch(&root.join(d));
        }
        for d in added {
            let _ = watcher.watch(&root.join(d), RecursiveMode::NonRecursive);
        }
    }
}

#[derive(Default)]
struct Batch {
    paths: HashSet<PathBuf>,
    rescan: bool,
}

impl Batch {
    fn add(&mut self, root: &Path, ev: notify::Result<notify::Event>) {
        match ev {
            Ok(ev) => {
                if ev.need_rescan() {
                    self.rescan = true;
                }
                for p in ev.paths {
                    if let Ok(rel) = p.strip_prefix(root) {
                        self.paths.insert(rel.to_path_buf());
                    }
                }
            }
            Err(_) => self.rescan = true,
        }
    }
}

#[derive(Default)]
struct Observed {
    notes: Vec<walk::FileRec>,
    files: Vec<walk::FileRec>,
    folders: Vec<PathBuf>,
    gone: Vec<PathBuf>,
    scanned: Vec<PathBuf>,
    skipped: Vec<PathBuf>,
}

impl Inner {
    fn observe(&self, paths: HashSet<PathBuf>, rescan: bool) -> Observed {
        let mut obs = Observed::default();
        let paths: Vec<PathBuf> = if rescan {
            vec![PathBuf::new()]
        } else {
            paths.into_iter().collect()
        };
        let known_folder = |p: &Path| self.state.read().folders.contains(p);
        for rel in paths {
            if !rel.as_os_str().is_empty() && self.ignore.excluded(&rel) {
                continue;
            }
            let abs = self.root.join(&rel);
            match std::fs::metadata(&abs) {
                Ok(m) if m.is_dir() => {
                    if rel.as_os_str().is_empty() && !rescan
                        || (!rel.as_os_str().is_empty() && known_folder(&rel))
                    {
                        continue;
                    }
                    if let Ok(w) = walk::walk(
                        &self.root,
                        &rel,
                        &self.ignore,
                        self.external == ExternalLinks::Follow,
                    ) {
                        obs.skipped.extend(w.skipped);
                        if !rel.as_os_str().is_empty() {
                            obs.folders.push(rel.clone());
                        }
                        obs.folders.extend(w.folders);
                        for f in w.files {
                            if is_markdown(&f.rel) {
                                obs.notes.push(f)
                            } else {
                                obs.files.push(f)
                            }
                        }
                        obs.scanned.push(rel);
                    }
                }
                Ok(m) if m.is_file() => {
                    let outside = walk::resolves_outside(&abs, &self.root);
                    if outside && self.external == ExternalLinks::Skip {
                        obs.skipped.push(rel);
                        continue;
                    }
                    let rec = walk::FileRec {
                        rel: rel.clone(),
                        size: m.len(),
                        modified: m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        outside,
                    };
                    if is_markdown(&rel) {
                        obs.notes.push(rec)
                    } else {
                        obs.files.push(rec)
                    }
                }
                Ok(_) => {}
                Err(_) => obs.gone.push(rel),
            }
        }
        obs
    }

    pub(crate) fn apply_batch(
        &self,
        paths: HashSet<PathBuf>,
        rescan: bool,
    ) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let _ops = self.ops.lock();
        let mut obs = self.observe(paths, rescan);

        struct Change {
            id: Option<NoteId>,
            rec: walk::FileRec,
        }
        let mut changed_or_added: Vec<Change> = Vec::new();
        let mut removed: Vec<(NoteId, PathBuf, u64, SystemTime)> = Vec::new();
        let mut file_upserts = Vec::new();
        let mut canvas_changes: Vec<(Option<NoteId>, walk::FileRec)> = Vec::new();
        let mut file_removals: Vec<PathBuf> = Vec::new();
        let mut folder_adds: Vec<PathBuf> = Vec::new();
        let mut folder_removals: Vec<PathBuf> = Vec::new();
        let mut link_removals: Vec<PathBuf> = Vec::new();
        let link_adds = std::mem::take(&mut obs.skipped);
        {
            let s = self.state.read();
            let mut seen: HashSet<&Path> = HashSet::new();
            for rec in &obs.notes {
                seen.insert(&rec.rel);
            }
            for rec in &obs.files {
                seen.insert(&rec.rel);
            }
            for f in &obs.folders {
                seen.insert(f);
            }
            let mut gone_prefixes: Vec<&Path> = obs.gone.iter().map(PathBuf::as_path).collect();
            gone_prefixes.extend(obs.scanned.iter().map(PathBuf::as_path));
            let under =
                |p: &Path| gone_prefixes.iter().any(|g| p.starts_with(g)) && !seen.contains(p);
            for (p, &id) in &s.by_path {
                if under(p) {
                    let e = s.entry(id).expect("by_path points to a live note");
                    removed.push((id, p.to_path_buf(), e.note.size, e.note.modified));
                }
            }
            for p in s.files.keys() {
                if under(p) {
                    file_removals.push(p.to_path_buf());
                }
            }
            for p in &s.folders {
                if under(p) {
                    folder_removals.push(p.to_path_buf());
                }
            }
            for p in &s.not_followed {
                if under(p) && !link_adds.iter().any(|a| a == &**p) {
                    link_removals.push(p.to_path_buf());
                }
            }
            for rec in obs.notes {
                match s.by_path.get(rec.rel.as_path()) {
                    Some(&id) => {
                        let e = s.entry(id).expect("by_path points to a live note");
                        if e.note.size != rec.size || e.note.modified != rec.modified {
                            changed_or_added.push(Change { id: Some(id), rec });
                        }
                    }
                    None => changed_or_added.push(Change { id: None, rec }),
                }
            }
            for rec in obs.files {
                if walk::is_canvas(&rec.rel) {
                    let known = s.canvas_by_path.get(rec.rel.as_path()).copied();
                    let fresh = known.and_then(|id| s.canvases.get(&id)).is_some_and(|e| {
                        e.note.size == rec.size && e.note.modified == rec.modified
                    });
                    if !fresh {
                        canvas_changes.push((known, rec.clone()));
                    }
                }
                let same = s
                    .files
                    .get(rec.rel.as_path())
                    .is_some_and(|f| f.size == rec.size && f.modified == rec.modified);
                if !same {
                    file_upserts.push(rec);
                }
            }
            for f in obs.folders {
                if !s.folders.contains(f.as_path()) {
                    folder_adds.push(f);
                }
            }
        }

        let mut moves: Vec<(NoteId, PathBuf, walk::FileRec)> = Vec::new();
        let mut by_stamp: HashMap<(u64, SystemTime), Vec<usize>> = HashMap::new();
        for (i, r) in removed.iter().enumerate() {
            by_stamp.entry((r.2, r.3)).or_default().push(i);
        }
        let mut taken = vec![false; removed.len()];
        let mut fresh = Vec::new();
        for c in changed_or_added {
            if c.id.is_none()
                && let Some(i) = by_stamp
                    .get_mut(&(c.rec.size, c.rec.modified))
                    .and_then(|v| v.pop())
            {
                taken[i] = true;
                moves.push((removed[i].0, removed[i].1.clone(), c.rec));
                continue;
            }
            fresh.push(c);
        }
        let removed: Vec<(NoteId, PathBuf)> = removed
            .into_iter()
            .zip(taken)
            .filter(|(_, t)| !t)
            .map(|(r, _)| (r.0, r.1))
            .collect();

        let parsed: Vec<(Option<NoteId>, walk::FileRec, Option<parse::Parsed>)> = fresh
            .into_par_iter()
            .map(|c| {
                let parsed = match read_lossy(&self.root.join(&c.rec.rel)) {
                    Ok(text) => Some(parse::parse(&text)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(_) => Some(parse::Parsed::default()),
                };
                (c.id, c.rec, parsed)
            })
            .collect();

        let canvas_parsed: Vec<(Option<NoteId>, walk::FileRec, parse::Parsed)> = canvas_changes
            .into_par_iter()
            .filter_map(|(id, rec)| {
                let parsed = match read_lossy(&self.root.join(&rec.rel)) {
                    Ok(text) => canvas::parse(&text),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
                    Err(_) => parse::Parsed::default(),
                };
                Some((id, rec, parsed))
            })
            .collect();

        let structural = !removed.is_empty()
            || !canvas_parsed.is_empty()
            || !moves.is_empty()
            || parsed.iter().any(|(id, _, p)| id.is_none() && p.is_some())
            || !file_upserts.is_empty()
            || !file_removals.is_empty()
            || !folder_adds.is_empty()
            || !folder_removals.is_empty()
            || !link_removals.is_empty()
            || link_adds
                .iter()
                .any(|p| !self.state.read().not_followed.contains(p.as_path()));
        if !structural && parsed.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut events = Vec::new();
        let mut history_moves = Vec::new();
        let tree_changed = structural;
        {
            let mut s = self.state.write();
            for (id, path) in &removed {
                s.remove_note(*id);
                events.push(VaultEvent::NoteRemoved {
                    id: *id,
                    path: path.clone(),
                });
            }
            let mut moved_entries = Vec::new();
            for (id, from, rec) in moves {
                if let Some(e) = s.remove_note(id) {
                    moved_entries.push((id, e, from, rec));
                }
            }
            for (id, e, from, rec) in moved_entries {
                s.set_outside(&rec.rel, rec.outside);
                s.insert_note(moved_entry(
                    &e,
                    super::fsx::stored(&rec.rel),
                    rec.size,
                    rec.modified,
                ));
                history_moves.push((from.clone(), rec.rel.clone()));
                events.push(VaultEvent::NoteMoved {
                    id,
                    from,
                    to: rec.rel,
                });
            }
            let mut realias = false;
            for (id, rec, p) in parsed {
                let Some(p) = p else { continue };
                s.set_outside(&rec.rel, rec.outside);
                match id {
                    Some(id) => {
                        let entry = entry_from_parsed(
                            id,
                            super::fsx::stored(&rec.rel),
                            p,
                            rec.size,
                            rec.modified,
                        );
                        realias |= s.replace_content(entry);
                        events.push(VaultEvent::NoteChanged(id));
                    }
                    None => {
                        let id = s.alloc_id();
                        s.insert_note(entry_from_parsed(
                            id,
                            super::fsx::stored(&rec.rel),
                            p,
                            rec.size,
                            rec.modified,
                        ));
                        s.add_folder_chain(parent_dir(&rec.rel));
                        events.push(VaultEvent::NoteAdded(id));
                    }
                }
            }
            for p in &file_removals {
                s.files.remove(p.as_path());
            }
            for rec in file_upserts {
                s.set_outside(&rec.rel, rec.outside);
                s.add_folder_chain(parent_dir(&rec.rel));
                let kind = walk::kind_of(&rec.rel);
                s.files.insert(
                    super::fsx::stored(&rec.rel),
                    FileEntry {
                        size: rec.size,
                        modified: rec.modified,
                        kind,
                    },
                );
            }
            for (id, rec, parsed) in canvas_parsed {
                let id = id.unwrap_or_else(|| s.alloc_id());
                let path = super::fsx::stored(&rec.rel);
                s.insert_canvas(entry_from_parsed(id, path, parsed, rec.size, rec.modified));
            }
            for f in &folder_removals {
                s.folders.remove(f.as_path());
            }
            for f in &folder_adds {
                s.add_folder_chain(f);
            }
            for p in &link_removals {
                s.not_followed.remove(p.as_path());
            }
            for p in link_adds {
                s.add_folder_chain(parent_dir(&p));
                s.not_followed.insert(super::fsx::stored(&p));
            }
            if structural || realias {
                s.rebuild();
            }
        }
        for (from, to) in &history_moves {
            self.follow_move(from, to);
        }
        if tree_changed {
            events.push(VaultEvent::TreeChanged);
        }
        self.emit(events);
        self.maybe_save_cache();
        (folder_adds, folder_removals)
    }
}
