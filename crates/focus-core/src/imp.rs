mod attach;
mod cache;
mod canvas;
mod compose;
mod config;
mod edit;
mod fsx;
mod history;
pub(crate) mod import;
mod parse;
pub(crate) mod props;
mod resolve;
pub(crate) mod retag;
mod tree;
mod walk;
mod watch;
#[cfg(windows)]
mod windows_trash;
#[cfg(unix)]
mod xdg_trash;

use super::*;
use parking_lot::{Mutex, RwLock};
use rayon::prelude::*;
use resolve::NameIndex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Resolved {
    Note(NoteId),
    File(Arc<Path>),
    External,
    Unresolved,
}

pub(crate) struct NoteEntry {
    pub note: cache::CachedNote,
    pub path: Arc<Path>,
    pub resolved: Box<[Resolved]>,
}

impl NoteEntry {
    pub fn id(&self) -> NoteId {
        self.note.id
    }

    pub fn parsed(&self) -> parse::Parsed {
        cache::decode_parsed(&self.note.blob).expect("metadata blob validated")
    }

    pub fn links(&self) -> Vec<OutLink> {
        cache::decode_links(&self.note.blob).expect("metadata blob validated")
    }

    pub fn meta(&self) -> NoteMeta {
        let p = self.parsed().meta;
        NoteMeta {
            id: self.note.id,
            path: self.path.clone(),
            title: self.note.title.clone(),
            aliases: p.aliases,
            tags: p.tags,
            headings: p.headings,
            blocks: p.blocks,
            props: p.props,
            links: p.links,
            words: p.words,
            tasks_open: p.tasks_open,
            tasks_done: p.tasks_done,
            size: self.note.size,
            modified: self.note.modified,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FileEntry {
    pub size: u64,
    pub modified: SystemTime,
    pub kind: NodeKind,
}

pub(crate) struct State {
    pub notes: Vec<Option<NoteEntry>>,
    pub by_path: HashMap<Arc<Path>, NoteId>,
    pub next_id: NoteId,
    pub files: HashMap<Arc<Path>, FileEntry>,
    pub folders: HashSet<Arc<Path>>,
    pub canvases: HashMap<NoteId, NoteEntry>,
    pub canvas_by_path: HashMap<Arc<Path>, NoteId>,
    pub names: NameIndex,
    pub incoming: HashMap<NoteId, Vec<NoteId>>,
    pub file_incoming: HashMap<Arc<Path>, Vec<NoteId>>,
    pub tree: Arc<TreeSnapshot>,
    pub generation: u64,
    pub dirty: bool,
    pub open_ms: f32,
    pub reparsed: usize,
    pub tags: Option<Arc<TagIndex>>,
    pub outside: HashSet<Arc<Path>>,
    pub not_followed: HashSet<Arc<Path>>,
}

#[derive(Default)]
pub(crate) struct TagIndex {
    tags: BTreeMap<String, (String, Vec<NoteId>)>,
    counts: Vec<TagCount>,
}

pub(crate) struct Inner {
    root: PathBuf,
    name: String,
    options: OpenOptions,
    external: ExternalLinks,
    ignore: walk::Ignore,
    obsidian: RwLock<ObsidianConfig>,
    policy: RwLock<AttachmentPolicy>,
    state: RwLock<State>,
    subscribers: Mutex<Vec<Sender<VaultEvent>>>,
    chosen_types: RwLock<BTreeMap<String, PropertyType>>,
    config_subscribers: Mutex<Vec<Sender<ConfigEvent>>>,
    ops: Mutex<()>,
    watcher: Mutex<Option<watch::Watcher>>,
    cache_writer: Mutex<Option<JoinHandle<()>>>,
    last_cache_write: Mutex<Instant>,
    history: history::Slot,
}

fn io_err(path: &Path, source: std::io::Error) -> VaultError {
    VaultError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn is_broken(link: &OutLink, resolved: &Resolved) -> bool {
    *resolved == Resolved::Unresolved && link.kind != LinkKind::Include
}

pub(crate) fn fold_char(c: char) -> char {
    const TABLE: &str = "AAAAAA\u{c6}CEEEEIIII\u{d0}NOOOOO\u{d7}OUUUUY\u{de}\u{df}aaaaaa\u{e6}ceeeeiiii\u{f0}nooooo\u{f7}ouuuuy\u{fe}yAaAaAaCcCcCcCcDdDdEeEeEeEeEeGgGgGgGgHhHhIiIiIiIiIi\u{132}\u{133}JjKk\u{138}LlLlLlLlLlNnNnNn\u{149}\u{14a}\u{14b}OoOoOo\u{152}\u{153}RrRrRrSsSsSsSsTtTtTtUuUuUuUuUuUuWwYyYZzZzZz\u{17f}";
    let cp = c as u32;
    if (0xC0..0x180).contains(&cp) {
        TABLE.chars().nth((cp - 0xC0) as usize).unwrap_or(c)
    } else {
        c
    }
}

fn title_for(path: &Path, h1: Option<&str>, props: &[(String, PropValue)]) -> (Arc<str>, bool) {
    if let Some(t) = h1.filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case(&fsx::file_name(path)))
    {
        return (Arc::from(t), false);
    }
    for (k, v) in props {
        if k.eq_ignore_ascii_case("title")
            && let PropValue::Text(t) = v
            && !t.trim().is_empty()
        {
            return (Arc::from(t.trim()), false);
        }
    }
    (Arc::from(fsx::stem(path)), true)
}

pub(crate) fn moved_entry(
    entry: &NoteEntry,
    new_path: Arc<Path>,
    size: u64,
    modified: SystemTime,
) -> NoteEntry {
    let mut note = entry.note.clone();
    if note.title_from_name {
        note.title = Arc::from(fsx::stem(&new_path));
    }
    note.path = new_path.clone();
    note.size = size;
    note.modified = modified;
    NoteEntry {
        note,
        path: new_path,
        resolved: Box::new([]),
    }
}

pub(crate) fn entry_from_parsed(
    id: NoteId,
    path: Arc<Path>,
    parsed: parse::Parsed,
    size: u64,
    modified: SystemTime,
) -> NoteEntry {
    let (title, title_from_name) =
        title_for(&path, parsed.meta.title.as_deref(), &parsed.meta.props);
    let note = cache::CachedNote {
        path: path.clone(),
        id,
        size,
        modified,
        title,
        title_from_name,
        aliases: parsed.meta.aliases.clone().into_boxed_slice(),
        blob: Arc::from(cache::encode_parsed(&parsed)),
    };
    NoteEntry {
        note,
        path,
        resolved: Box::new([]),
    }
}

pub(crate) fn read_lossy(abs: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(abs)?;
    Ok(match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    })
}

impl State {
    fn empty() -> State {
        State {
            notes: Vec::new(),
            by_path: HashMap::new(),
            next_id: 0,
            files: HashMap::new(),
            folders: HashSet::new(),
            canvases: HashMap::new(),
            canvas_by_path: HashMap::new(),
            names: NameIndex::default(),
            incoming: HashMap::new(),
            file_incoming: HashMap::new(),
            tree: Arc::new(TreeSnapshot::default()),
            generation: 0,
            dirty: false,
            open_ms: 0.0,
            reparsed: 0,
            tags: None,
            outside: HashSet::new(),
            not_followed: HashSet::new(),
        }
    }

    pub fn entry(&self, id: NoteId) -> Option<&NoteEntry> {
        self.notes.get(id as usize)?.as_ref()
    }

    pub fn insert_note(&mut self, entry: NoteEntry) {
        let id = entry.id() as usize;
        if self.notes.len() <= id {
            self.notes.resize_with(id + 1, || None);
        }
        self.by_path.insert(entry.path.clone(), entry.id());
        self.notes[id] = Some(entry);
    }

    pub fn remove_note(&mut self, id: NoteId) -> Option<NoteEntry> {
        let e = self.notes.get_mut(id as usize)?.take()?;
        self.by_path.remove(&e.path);
        Some(e)
    }

    pub fn source(&self, id: NoteId) -> Option<&NoteEntry> {
        self.entry(id).or_else(|| self.canvases.get(&id))
    }

    pub fn insert_canvas(&mut self, entry: NoteEntry) {
        self.canvas_by_path.insert(entry.path.clone(), entry.id());
        self.canvases.insert(entry.id(), entry);
    }

    pub fn remove_canvas(&mut self, id: NoteId) -> Option<NoteEntry> {
        let e = self.canvases.remove(&id)?;
        self.canvas_by_path.remove(&e.path);
        Some(e)
    }

    pub fn alloc_id(&mut self) -> NoteId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add_folder_chain(&mut self, dir: &Path) {
        let mut p = dir;
        while !p.as_os_str().is_empty() {
            if !self.folders.insert(Arc::from(p)) {
                break;
            }
            p = fsx::parent_dir(p);
        }
    }

    pub fn resolve(
        &self,
        from_id: Option<NoteId>,
        from: &Path,
        target: &str,
    ) -> (Resolved, resolve::Via) {
        self.names.resolve(&self.notes, from_id, from, target)
    }

    pub fn resolve_canvas(&self, target: &str) -> Resolved {
        match self.resolve(None, Path::new(""), target) {
            (r, resolve::Via::Relative | resolve::Via::Root) => r,
            (Resolved::External, _) => Resolved::External,
            _ => Resolved::Unresolved,
        }
    }

    fn resolve_entry(&self, e: &NoteEntry) -> Box<[Resolved]> {
        e.links()
            .iter()
            .map(|l| self.resolve(Some(e.id()), &e.path, &l.target).0)
            .collect()
    }

    fn resolve_canvas_entry(&self, e: &NoteEntry) -> Box<[Resolved]> {
        e.links()
            .iter()
            .map(|l| self.resolve_canvas(&l.target))
            .collect()
    }

    pub fn rebuild(&mut self) {
        self.names = NameIndex::build(&self.notes, &self.files);
        let resolved: Vec<Option<Box<[Resolved]>>> = self
            .notes
            .par_iter()
            .map(|e| e.as_ref().map(|e| self.resolve_entry(e)))
            .collect();
        for (slot, r) in self.notes.iter_mut().zip(resolved) {
            if let (Some(e), Some(r)) = (slot.as_mut(), r) {
                e.resolved = r;
            }
        }
        let gone: Vec<NoteId> = self
            .canvases
            .values()
            .filter(|e| !self.files.contains_key(&e.path))
            .map(NoteEntry::id)
            .collect();
        for id in gone {
            self.remove_canvas(id);
        }
        let resolved: Vec<(NoteId, Box<[Resolved]>)> = self
            .canvases
            .values()
            .map(|e| (e.id(), self.resolve_canvas_entry(e)))
            .collect();
        for (id, r) in resolved {
            if let Some(e) = self.canvases.get_mut(&id) {
                e.resolved = r;
            }
        }
        self.rebuild_incoming();
        self.rebuild_tree();
        self.tags = None;
        self.dirty = true;
    }

    fn rebuild_incoming(&mut self) {
        self.incoming.clear();
        self.file_incoming.clear();
        for e in self.notes.iter().flatten().chain(self.canvases.values()) {
            add_incoming(&mut self.incoming, &mut self.file_incoming, e);
        }
    }

    pub fn set_outside(&mut self, path: &Path, outside: bool) {
        if outside {
            self.outside.insert(fsx::stored(path));
        } else {
            self.outside.remove(path);
        }
    }

    pub fn rebuild_tree(&mut self) {
        self.generation += 1;
        let (by_path, files) = (&self.by_path, &self.files);
        self.outside
            .retain(|p| by_path.contains_key(p) || files.contains_key(p));
        let outside = &self.outside;
        let followed = |p: &Arc<Path>| outside.contains(p).then_some(LinkOut::Followed);
        let notes = self.notes.iter().flatten().map(|e| tree::Item {
            path: e.path.clone(),
            kind: NodeKind::Note,
            note: Some(e.id()),
            link_out: followed(&e.path),
        });
        let files = self.files.iter().map(|(p, f)| tree::Item {
            path: p.clone(),
            kind: f.kind,
            note: None,
            link_out: followed(p),
        });
        let folders = self.folders.iter().map(|p| tree::Item {
            path: p.clone(),
            kind: NodeKind::Folder,
            note: None,
            link_out: None,
        });
        let skipped = self.not_followed.iter().map(|p| tree::Item {
            path: p.clone(),
            kind: NodeKind::Other,
            note: None,
            link_out: Some(LinkOut::NotFollowed),
        });
        self.tree = Arc::new(tree::build(
            folders.chain(notes).chain(files).chain(skipped),
            self.generation,
        ));
    }

    pub fn replace_content(&mut self, mut entry: NoteEntry) -> bool {
        let id = entry.id();
        let old = self.remove_note(id);
        let aliases_changed = old
            .as_ref()
            .is_none_or(|o| o.note.aliases != entry.note.aliases);
        if let Some(o) = &old {
            remove_incoming(&mut self.incoming, &mut self.file_incoming, o);
        }
        entry.resolved = self.resolve_entry(&entry);
        add_incoming(&mut self.incoming, &mut self.file_incoming, &entry);
        self.insert_note(entry);
        self.tags = None;
        self.dirty = true;
        aliases_changed
    }
}

fn add_incoming(
    incoming: &mut HashMap<NoteId, Vec<NoteId>>,
    file_incoming: &mut HashMap<Arc<Path>, Vec<NoteId>>,
    e: &NoteEntry,
) {
    let src = e.id();
    for r in e.resolved.iter() {
        match r {
            Resolved::Note(t) if *t != src => {
                let v = incoming.entry(*t).or_default();
                if v.last() != Some(&src) && !v.contains(&src) {
                    v.push(src);
                }
            }
            Resolved::File(p) => {
                let v = file_incoming.entry(p.clone()).or_default();
                if !v.contains(&src) {
                    v.push(src);
                }
            }
            _ => {}
        }
    }
}

fn remove_incoming(
    incoming: &mut HashMap<NoteId, Vec<NoteId>>,
    file_incoming: &mut HashMap<Arc<Path>, Vec<NoteId>>,
    e: &NoteEntry,
) {
    let src = e.id();
    for r in e.resolved.iter() {
        match r {
            Resolved::Note(t) => {
                if let Some(v) = incoming.get_mut(t) {
                    v.retain(|&s| s != src);
                    if v.is_empty() {
                        incoming.remove(t);
                    }
                }
            }
            Resolved::File(p) => {
                if let Some(v) = file_incoming.get_mut(p) {
                    v.retain(|&s| s != src);
                    if v.is_empty() {
                        file_incoming.remove(p);
                    }
                }
            }
            _ => {}
        }
    }
}

impl TagIndex {
    fn build(notes: &[Option<NoteEntry>]) -> TagIndex {
        let per_note: Vec<(NoteId, Vec<String>)> = notes
            .par_iter()
            .flatten()
            .map(|e| (e.id(), e.parsed().meta.tags))
            .collect();
        let mut tags: BTreeMap<String, (String, Vec<NoteId>)> = BTreeMap::new();
        for (id, note_tags) in per_note {
            for t in note_tags {
                tags.entry(t.to_lowercase())
                    .or_insert_with(|| (t, Vec::new()))
                    .1
                    .push(id);
            }
        }
        let mut per_tag: BTreeMap<String, (String, HashSet<NoteId>)> = BTreeMap::new();
        for (key, (display, ids)) in &tags {
            let mut end = key.len();
            loop {
                let k = &key[..end];
                let disp_end = display
                    .char_indices()
                    .map(|(i, _)| i)
                    .chain([display.len()])
                    .find(|&i| display[..i].to_lowercase().len() >= end)
                    .unwrap_or(display.len());
                let entry = per_tag
                    .entry(k.to_string())
                    .or_insert_with(|| (display[..disp_end].to_string(), HashSet::new()));
                entry.1.extend(ids.iter().copied());
                match k.rfind('/') {
                    Some(i) => end = i,
                    None => break,
                }
            }
        }
        let counts = per_tag
            .into_values()
            .map(|(tag, ids)| TagCount {
                tag,
                count: ids.len() as u32,
            })
            .collect();
        TagIndex { tags, counts }
    }
}

pub(crate) fn open(
    root: &Path,
    options: OpenOptions,
    external: ExternalLinks,
) -> Result<Arc<Vault>> {
    let start = Instant::now();
    let root = dunce::canonicalize(root).map_err(|e| io_err(root, e))?;
    if !root.is_dir() {
        return Err(VaultError::Other(format!(
            "not a directory: {}",
            root.display()
        )));
    }
    let ignore = walk::Ignore::new(&options.ignore)
        .map_err(|e| VaultError::Other(format!("invalid ignore pattern: {e}")))?;
    let obsidian = config::read_obsidian(&root);
    let cache_path = options.cache_dir.join(cache::FILE_NAME);

    let (cached, walked) = rayon::join(
        || {
            std::fs::read(&cache_path)
                .ok()
                .and_then(|bytes| cache::decode(&root, &bytes))
        },
        || {
            walk::walk(
                &root,
                Path::new(""),
                &ignore,
                external == ExternalLinks::Follow,
            )
        },
    );
    let walked = walked.map_err(|e| io_err(&root, e))?;

    let mut state = State::empty();
    let had_cache = cached.is_some();
    let mut cache_by_path: HashMap<Arc<Path>, cache::CachedNote> = HashMap::new();
    if let Some(c) = cached {
        state.next_id = c.next_id;
        cache_by_path = c.notes.into_iter().map(|n| (n.path.clone(), n)).collect();
    }

    let mut note_files: Vec<(walk::FileRec, bool)> = Vec::new();
    for f in walked.files {
        state.set_outside(&f.rel, f.outside);
        match walk::kind_of(&f.rel) {
            NodeKind::Note => note_files.push((f, false)),
            kind => {
                if walk::is_canvas(&f.rel) {
                    note_files.push((f.clone(), true));
                }
                state.files.insert(
                    fsx::stored(&f.rel),
                    FileEntry {
                        size: f.size,
                        modified: f.modified,
                        kind,
                    },
                );
            }
        }
    }
    for d in walked.folders {
        state.folders.insert(fsx::stored(&d));
    }
    state.not_followed = walked.skipped.iter().map(|p| fsx::stored(p)).collect();
    struct Slot {
        rec: walk::FileRec,
        canvas: bool,
        id: NoteId,
        cached: Option<cache::CachedNote>,
    }
    let mut slots = Vec::with_capacity(note_files.len());
    let mut unknown = Vec::new();
    for (rec, canvas) in note_files {
        match cache_by_path.remove(rec.rel.as_path()) {
            Some(c) => {
                let id = c.id;
                let fresh = (c.size == rec.size && c.modified == rec.modified).then_some(c);
                slots.push(Slot {
                    rec,
                    canvas,
                    id,
                    cached: fresh,
                });
            }
            None => unknown.push((rec, canvas)),
        }
    }
    unknown.par_sort_unstable_by(|(a, _), (b, _)| {
        a.rel
            .as_os_str()
            .as_encoded_bytes()
            .cmp(b.rel.as_os_str().as_encoded_bytes())
    });
    for (rec, canvas) in unknown {
        let id = state.alloc_id();
        slots.push(Slot {
            rec,
            canvas,
            id,
            cached: None,
        });
    }
    let removed_from_cache = !cache_by_path.is_empty();
    drop(cache_by_path);

    let reparsed = std::sync::atomic::AtomicUsize::new(0);
    let entries: Vec<(bool, NoteEntry)> = slots
        .into_par_iter()
        .filter_map(|slot| {
            let path: Arc<Path> = fsx::stored(&slot.rec.rel);
            let canvas = slot.canvas;
            let parsed = match slot.cached {
                Some(note) => {
                    let entry = NoteEntry {
                        path: note.path.clone(),
                        note,
                        resolved: Box::new([]),
                    };
                    return Some((canvas, entry));
                }
                None => {
                    if !canvas {
                        reparsed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    match read_lossy(&root.join(&slot.rec.rel)) {
                        Ok(text) if canvas => canvas::parse(&text),
                        Ok(text) => parse::parse(&text),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
                        Err(_) => parse::Parsed::default(),
                    }
                }
            };
            let entry = entry_from_parsed(slot.id, path, parsed, slot.rec.size, slot.rec.modified);
            Some((canvas, entry))
        })
        .collect();
    for (canvas, e) in entries {
        if canvas {
            state.insert_canvas(e);
        } else {
            state.insert_note(e);
        }
    }
    state.reparsed = reparsed.into_inner();
    state.rebuild();
    state.dirty = !had_cache || state.reparsed > 0 || removed_from_cache;
    state.open_ms = start.elapsed().as_secs_f32() * 1000.0;

    let name = fsx::file_name(&root);
    let watch = options.watch;
    let vault = Arc::new(Vault {
        inner: Inner {
            root,
            name,
            options,
            external,
            ignore,
            obsidian: RwLock::new(obsidian),
            policy: RwLock::new(AttachmentPolicy::default()),
            state: RwLock::new(state),
            subscribers: Mutex::new(Vec::new()),
            chosen_types: RwLock::new(BTreeMap::new()),
            config_subscribers: Mutex::new(Vec::new()),
            ops: Mutex::new(()),
            watcher: Mutex::new(None),
            cache_writer: Mutex::new(None),
            last_cache_write: Mutex::new(Instant::now()),
            history: history::Slot::default(),
        },
    });
    vault.inner.save_cache_in_background();
    if !had_cache {
        release_free_memory();
    }
    if watch {
        let w = watch::start(&vault)?;
        *vault.inner.watcher.lock() = Some(w);
    }
    Ok(vault)
}

pub(crate) fn release_free_memory() {
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

pub(crate) fn parse_meta(source: &str) -> ParsedMeta {
    parse::parse(source).meta
}
pub(crate) fn config_dir() -> PathBuf {
    config::config_dir()
}
pub(crate) fn home_dir() -> Option<PathBuf> {
    config::home_dir()
}
pub(crate) fn data_dir() -> PathBuf {
    config::data_dir()
}
pub(crate) fn history_dir_for(vault_root: &Path) -> PathBuf {
    config::history_dir_for(vault_root)
}
pub(crate) fn cache_dir_for(vault_root: &Path) -> PathBuf {
    config::cache_dir_for(vault_root)
}
pub(crate) fn load_settings() -> anyhow::Result<settings::Settings> {
    config::load_settings()
}
pub(crate) fn save_settings(settings: &settings::Settings) -> anyhow::Result<()> {
    config::save_settings(settings)
}

fn backlink_context(line: &str, link: std::ops::Range<usize>) -> (String, std::ops::Range<usize>) {
    const MAX: usize = 160;
    let first = (line.len() - line.trim_start().len()).min(link.start);
    let last = line.trim_end().len().max(link.end);
    let (mut start, mut end) = (first, last);
    if line[first..last].chars().count() > MAX {
        let budget = MAX.saturating_sub(line[link.clone()].chars().count());
        start = line[first..link.start]
            .char_indices()
            .rev()
            .take(budget / 3)
            .last()
            .map_or(link.start, |(i, _)| first + i);
        let used = line[start..link.end].chars().count();
        let after = MAX.saturating_sub(used);
        end = line[link.end..last]
            .char_indices()
            .nth(after)
            .map_or(last, |(i, _)| link.end + i);
    }
    (
        line[start..end].to_string(),
        link.start - start..link.end - start,
    )
}

impl Inner {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn name(&self) -> String {
        self.name.clone()
    }
    pub(crate) fn stats(&self) -> VaultStats {
        let s = self.state.read();
        let notes = s.by_path.len();
        let bytes = s.notes.iter().flatten().map(|e| e.note.size).sum::<u64>()
            + s.files.values().map(|f| f.size).sum::<u64>();
        VaultStats {
            notes,
            folders: s.folders.len(),
            attachments: s
                .files
                .values()
                .filter(|f| f.kind == NodeKind::Attachment)
                .count(),
            bytes,
            open_ms: s.open_ms,
            reparsed: s.reparsed,
        }
    }
    pub(crate) fn obsidian(&self) -> ObsidianConfig {
        self.obsidian.read().clone()
    }
    pub(crate) fn property_type(&self, name: &str, value: &PropValue) -> PropertyKind {
        let chosen = props::lookup(&self.chosen_types.read(), name);
        let obsidian = props::lookup(&self.obsidian.read().property_types, name);
        props::resolve(name, value, chosen, obsidian)
    }
    pub(crate) fn set_chosen_property_types(&self, types: BTreeMap<String, PropertyType>) {
        let changed = {
            let mut chosen = self.chosen_types.write();
            let changed = *chosen != types;
            *chosen = types;
            changed
        };
        if changed {
            self.emit_config(ConfigEvent::PropertyTypes);
        }
    }
    pub(crate) fn chosen_property_types(&self) -> BTreeMap<String, PropertyType> {
        self.chosen_types.read().clone()
    }
    pub(crate) fn reload_property_types(&self) {
        let types = config::read_property_types(&self.root);
        let changed = {
            let mut config = self.obsidian.write();
            let changed = config.property_types != types;
            config.property_types = types;
            changed
        };
        if changed {
            self.emit_config(ConfigEvent::PropertyTypes);
        }
    }
    pub(crate) fn subscribe_config(&self) -> Receiver<ConfigEvent> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.config_subscribers.lock().push(tx);
        rx
    }
    fn emit_config(&self, event: ConfigEvent) {
        self.config_subscribers
            .lock()
            .retain(|tx| tx.send(event).is_ok());
    }
    pub(crate) fn set_attachment_policy(&self, policy: AttachmentPolicy) {
        *self.policy.write() = policy;
    }
    pub(crate) fn tree(&self) -> Arc<TreeSnapshot> {
        self.state.read().tree.clone()
    }
    pub(crate) fn note(&self, id: NoteId) -> Option<Arc<NoteMeta>> {
        let entry = {
            let s = self.state.read();
            let e = s.entry(id)?;
            NoteEntry {
                note: e.note.clone(),
                path: e.path.clone(),
                resolved: Box::new([]),
            }
        };
        Some(Arc::new(entry.meta()))
    }
    pub(crate) fn note_by_path(&self, relative: &Path) -> Option<NoteId> {
        self.state.read().by_path.get(relative).copied()
    }
    pub(crate) fn notes(&self) -> Vec<Arc<NoteMeta>> {
        let entries: Vec<NoteEntry> = self
            .state
            .read()
            .notes
            .iter()
            .flatten()
            .map(|e| NoteEntry {
                note: e.note.clone(),
                path: e.path.clone(),
                resolved: Box::new([]),
            })
            .collect();
        entries.par_iter().map(|e| Arc::new(e.meta())).collect()
    }
    pub(crate) fn summaries(&self) -> Vec<NoteSummary> {
        self.state
            .read()
            .notes
            .iter()
            .flatten()
            .map(|e| NoteSummary {
                id: e.id(),
                path: e.path.clone(),
                title: e.note.title.clone(),
                aliases: e.note.aliases.to_vec(),
                size: e.note.size,
                modified: e.note.modified,
            })
            .collect()
    }
    pub(crate) fn absolute(&self, relative: &Path) -> PathBuf {
        self.root.join(relative)
    }
    fn note_path(&self, id: NoteId) -> Result<Arc<Path>> {
        self.state
            .read()
            .entry(id)
            .map(|e| e.path.clone())
            .ok_or(VaultError::UnknownNote(id))
    }
    pub(crate) fn read(&self, id: NoteId) -> Result<String> {
        let path = self.note_path(id)?;
        let abs = self.root.join(&path);
        read_lossy(&abs).map_err(|e| io_err(&abs, e))
    }

    pub(crate) fn resolve_link(&self, from: Option<NoteId>, target: &str) -> LinkTarget {
        let s = self.state.read();
        let note_path: &Path = match from {
            Some(id) => match s.entry(id) {
                Some(e) => &e.path,
                None => return LinkTarget::Unresolved,
            },
            None => Path::new(""),
        };
        let raw = target.trim();
        let raw = raw
            .split_once('|')
            .map_or(raw, |(t, _)| t.trim_end_matches('\\'));
        if parse::is_external(raw) {
            return LinkTarget::External(raw.to_string());
        }
        let (path_part, anchor) = match raw.split_once('#') {
            Some((p, a)) => (p, Some(a.to_string()).filter(|a| !a.is_empty())),
            None => (raw, None),
        };
        let decoded = parse::percent_decode(path_part);
        match s.resolve(from, note_path, &decoded).0 {
            Resolved::Note(id) => LinkTarget::Note { id, anchor },
            Resolved::File(p) => LinkTarget::Attachment(p.to_path_buf()),
            Resolved::External => LinkTarget::External(raw.to_string()),
            Resolved::Unresolved => LinkTarget::Unresolved,
        }
    }

    pub(crate) fn source_path(&self, id: NoteId) -> Option<Arc<Path>> {
        self.state.read().source(id).map(|e| e.path.clone())
    }
    pub(crate) fn canvas_by_path(&self, relative: &Path) -> Option<NoteId> {
        self.state.read().canvas_by_path.get(relative).copied()
    }
    pub(crate) fn canvases(&self) -> Vec<(NoteId, Arc<Path>)> {
        self.state
            .read()
            .canvases
            .values()
            .map(|e| (e.id(), e.path.clone()))
            .collect()
    }

    pub(crate) fn links_of(&self, id: NoteId) -> Vec<(OutLink, LinkTarget)> {
        let s = self.state.read();
        let Some(e) = s.source(id) else {
            return Vec::new();
        };
        e.links()
            .into_iter()
            .zip(e.resolved.iter())
            .map(|(l, r)| {
                let target = match r {
                    Resolved::Note(t) => LinkTarget::Note {
                        id: *t,
                        anchor: l.anchor.clone(),
                    },
                    Resolved::File(p) => LinkTarget::Attachment(p.to_path_buf()),
                    Resolved::External => LinkTarget::External(l.target.clone()),
                    Resolved::Unresolved => LinkTarget::Unresolved,
                };
                (l, target)
            })
            .collect()
    }

    pub(crate) fn linked_from(&self, id: NoteId) -> Vec<NoteId> {
        self.state
            .read()
            .incoming
            .get(&id)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn backlinks(&self, id: NoteId) -> Vec<Backlink> {
        let sources: Vec<(NoteId, Arc<Path>, Vec<u32>)> = {
            let s = self.state.read();
            let Some(ids) = s.incoming.get(&id) else {
                return Vec::new();
            };
            ids.iter()
                .filter_map(|&src| {
                    let e = s.source(src)?;
                    let mut lines: Vec<u32> = e
                        .links()
                        .iter()
                        .zip(e.resolved.iter())
                        .filter(|(_, r)| **r == Resolved::Note(id))
                        .map(|(l, _)| l.line)
                        .collect();
                    lines.dedup();
                    Some((src, e.path.clone(), lines))
                })
                .collect()
        };
        let state = &self.state;
        let mut out: Vec<Backlink> = sources
            .par_iter()
            .flat_map_iter(|(src, path, lines)| {
                let mut found = Vec::new();
                let Ok(text) = read_lossy(&self.root.join(path)) else {
                    return found;
                };
                if walk::is_canvas(path) {
                    let s = state.read();
                    for r in canvas::file_refs(&text).unwrap_or_default() {
                        if lines.contains(&r.line)
                            && s.resolve_canvas(&r.file) == Resolved::Note(id)
                        {
                            let context = r.written();
                            found.push(Backlink {
                                source: *src,
                                line: r.line,
                                range: 0..r.file.len(),
                                context,
                            });
                        }
                    }
                    return found;
                }
                let spans = parse::link_spans(&text);
                let s = state.read();
                for span in spans.iter().filter(|sp| lines.contains(&sp.line)) {
                    if s.resolve(Some(*src), path, &span.target).0 != Resolved::Note(id) {
                        continue;
                    }
                    let line_start = text[..span.full.start].rfind('\n').map_or(0, |i| i + 1);
                    let line_end = text[span.full.end..]
                        .find('\n')
                        .map_or(text.len(), |i| span.full.end + i);
                    let line = text[line_start..line_end].trim_end_matches('\r');
                    let local = span.full.start - line_start..span.full.end - line_start;
                    let (context, range) = backlink_context(line, local);
                    found.push(Backlink {
                        source: *src,
                        line: span.line,
                        context,
                        range,
                    });
                }
                found
            })
            .collect();
        out.sort_by(|a, b| a.source.cmp(&b.source).then(a.line.cmp(&b.line)));
        out
    }

    pub(crate) fn unresolved(&self, id: NoteId) -> Vec<OutLink> {
        let s = self.state.read();
        let Some(e) = s.source(id) else {
            return Vec::new();
        };
        e.links()
            .into_iter()
            .zip(e.resolved.iter())
            .filter(|(l, r)| is_broken(l, r))
            .map(|(l, _)| l)
            .collect()
    }
    pub(crate) fn all_unresolved(&self) -> Vec<(NoteId, OutLink)> {
        let s = self.state.read();
        s.notes
            .iter()
            .flatten()
            .chain(s.canvases.values())
            .flat_map(|e| {
                e.links()
                    .into_iter()
                    .zip(e.resolved.iter())
                    .filter(|(l, r)| is_broken(l, r))
                    .map(|(l, _)| (e.id(), l))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn tag_index(&self) -> Arc<TagIndex> {
        if let Some(t) = self.state.read().tags.clone() {
            return t;
        }
        let mut s = self.state.write();
        if let Some(t) = s.tags.clone() {
            return t;
        }
        let t = Arc::new(TagIndex::build(&s.notes));
        s.tags = Some(t.clone());
        t
    }
    pub(crate) fn is_outside(&self, relative: &Path) -> bool {
        self.state.read().outside.contains(relative)
    }
    pub(crate) fn tags(&self) -> Vec<TagCount> {
        self.tag_index().counts.clone()
    }
    pub(crate) fn notes_with_tag(&self, tag: &str) -> Vec<NoteId> {
        let ix = self.tag_index();
        let key = tag.trim_start_matches('#').to_lowercase();
        let prefix = format!("{key}/");
        let mut out: Vec<NoteId> = ix
            .tags
            .range(key.clone()..)
            .take_while(|(k, _)| {
                **k == key || k.starts_with(&prefix) || k.as_str() < prefix.as_str()
            })
            .filter(|(k, _)| **k == key || k.starts_with(&prefix))
            .flat_map(|(_, (_, ids))| ids.iter().copied())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
    pub(crate) fn tasks(&self, include_done: bool) -> Vec<TaskItem> {
        let s = self.state.read();
        let mut out = Vec::new();
        for e in s.notes.iter().flatten() {
            for t in e
                .parsed()
                .tasks
                .into_iter()
                .filter(|t| include_done || !t.done)
            {
                out.push(TaskItem {
                    note: e.id(),
                    line: t.line,
                    text: t.text,
                    done: t.done,
                });
            }
        }
        out
    }

    pub(crate) fn tasks_of(&self, id: NoteId) -> Vec<TaskItem> {
        let s = self.state.read();
        let Some(e) = s.entry(id) else {
            return Vec::new();
        };
        e.parsed()
            .tasks
            .into_iter()
            .map(|t| TaskItem {
                note: id,
                line: t.line,
                text: t.text,
                done: t.done,
            })
            .collect()
    }

    pub(crate) fn subscribe(&self) -> Receiver<VaultEvent> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.subscribers.lock().push(tx);
        rx
    }

    pub(crate) fn emit(&self, events: Vec<VaultEvent>) {
        if events.is_empty() {
            return;
        }
        let mut subs = self.subscribers.lock();
        subs.retain(|tx| events.iter().all(|e| tx.send(e.clone()).is_ok()));
    }

    fn cache_snapshot(&self) -> Option<(NoteId, Vec<cache::CachedNote>)> {
        let mut s = self.state.write();
        if !s.dirty {
            return None;
        }
        s.dirty = false;
        let notes = s
            .notes
            .iter()
            .flatten()
            .chain(s.canvases.values())
            .map(|e| e.note.clone())
            .collect();
        Some((s.next_id, notes))
    }

    fn write_cache(root: &Path, cache_dir: &Path, next_id: NoteId, notes: Vec<cache::CachedNote>) {
        let bytes = cache::encode_records(root, next_id, &notes);
        let path = cache_dir.join(cache::FILE_NAME);
        if let Err(e) = fsx::write_atomic(&path, &bytes) {
            eprintln!("focus-core: cache not written ({}): {e}", path.display());
        }
    }

    pub(crate) fn save_cache_in_background(&self) {
        let Some((next_id, notes)) = self.cache_snapshot() else {
            return;
        };
        let root = self.root.clone();
        let dir = self.options.cache_dir.clone();
        let mut writer = self.cache_writer.lock();
        if let Some(h) = writer.take() {
            let _ = h.join();
        }
        *writer = Some(std::thread::spawn(move || {
            Self::write_cache(&root, &dir, next_id, notes)
        }));
        *self.last_cache_write.lock() = Instant::now();
    }

    pub(crate) fn maybe_save_cache(&self) {
        if self.last_cache_write.lock().elapsed().as_secs() >= 30 {
            self.save_cache_in_background();
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.watcher.lock().take();
        if let Some(h) = self.cache_writer.lock().take() {
            let _ = h.join();
        }
        if let Some((next_id, notes)) = self.cache_snapshot() {
            Self::write_cache(&self.root, &self.options.cache_dir, next_id, notes);
        }
    }
}

#[cfg(test)]
pub(crate) fn test_dir(name: &str) -> PathBuf {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let dir = std::fs::canonicalize(&target)
        .unwrap_or(target)
        .join("focus-core-tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test directory under target/");
    dir
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_canvas;
#[cfg(test)]
mod tests_compose;
