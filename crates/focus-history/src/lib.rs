mod copies;
mod diff;
mod merge;
mod owner_only;

pub use copies::{ConflictName, CopyOrigin, conflict_copy};
pub use diff::{DiffLine, diff};
pub use merge::merge;

use owner_only::{DirBuilderExt as _, OpenOptionsExt as _};
use parking_lot::RwLock;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const EDIT_INTERVAL: Duration = Duration::from_secs(5 * 60);
const AHEAD_OF_CLOCK: Duration = Duration::from_secs(1);

const STALE_TEMPORARY: Duration = Duration::from_secs(3600);

const DAY_MS: u64 = 24 * 3600 * 1000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("no such version: {0}")]
    UnknownVersion(String),
    #[error("content of the version no longer matches its hash: {0}")]
    Damaged(PathBuf),
}

pub type Result<T> = std::result::Result<T, Error>;

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    pub enabled: bool,
    pub days: u32,
    pub max_bytes: u64,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        HistoryConfig {
            enabled: true,
            days: 14,
            max_bytes: 200 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Edit,
    Bulk,
    Restore,
    External,
    Unsaved,
    Reload,
}

impl Kind {
    fn letter(self) -> char {
        match self {
            Kind::Edit => 'e',
            Kind::Bulk => 'b',
            Kind::Restore => 'r',
            Kind::External => 'x',
            Kind::Unsaved => 'u',
            Kind::Reload => 'l',
        }
    }

    fn from_letter(c: &str) -> Option<Kind> {
        match c {
            "e" => Some(Kind::Edit),
            "b" => Some(Kind::Bulk),
            "r" => Some(Kind::Restore),
            "x" => Some(Kind::External),
            "u" => Some(Kind::Unsaved),
            "l" => Some(Kind::Reload),
            _ => None,
        }
    }

    pub fn waits(self) -> bool {
        matches!(self, Kind::Edit | Kind::Reload)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VersionId(String);

impl VersionId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub id: VersionId,
    pub at: SystemTime,
    pub bytes: u64,
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneReport {
    pub versions: usize,
    pub bytes: u64,
}

struct Entry {
    at_ms: u64,
    kind: Kind,
    bytes: u64,
    hash: String,
    name: String,
}

impl Entry {
    fn parse(name: &str) -> Option<Entry> {
        let mut parts = name.splitn(4, '-');
        let at_ms = u64::from_str_radix(parts.next()?, 16).ok()?;
        let kind = Kind::from_letter(parts.next()?)?;
        let bytes = u64::from_str_radix(parts.next()?, 16).ok()?;
        let hash = parts.next()?;
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(Entry {
            at_ms,
            kind,
            bytes,
            hash: hash.to_string(),
            name: name.to_string(),
        })
    }

    fn name_for(at_ms: u64, kind: Kind, bytes: u64, hash: &str) -> String {
        format!("{at_ms:016x}-{}-{bytes:x}-{hash}", kind.letter())
    }
}

type Clock = Arc<dyn Fn() -> SystemTime + Send + Sync>;

pub struct History {
    dir: PathBuf,
    config: RwLock<HistoryConfig>,
    clock: Clock,
    temporaries: AtomicU64,
}

impl History {
    pub fn open(dir: &Path, config: HistoryConfig) -> Result<History> {
        History::open_with_clock(dir, config, Arc::new(SystemTime::now))
    }

    pub fn open_with_clock(dir: &Path, config: HistoryConfig, clock: Clock) -> Result<History> {
        for folder in [dir.to_path_buf(), dir.join("blobs"), dir.join("notes")] {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&folder)
                .map_err(io(&folder))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))
                    .map_err(io(&folder))?;
            }
        }
        Ok(History {
            dir: dir.to_path_buf(),
            config: RwLock::new(config),
            clock,
            temporaries: AtomicU64::new(0),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn config(&self) -> HistoryConfig {
        *self.config.read()
    }

    pub fn set_config(&self, config: HistoryConfig) {
        *self.config.write() = config;
    }

    fn now_ms(&self) -> u64 {
        (self.clock)()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64)
    }

    fn lock(&self) -> Result<File> {
        let path = self.dir.join("lock");
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(&path)
            .map_err(io(&path))?;
        file.lock().map_err(io(&path))?;
        Ok(file)
    }

    fn note_dir(&self, rel: &Path) -> PathBuf {
        self.dir.join("notes").join(note_key(rel))
    }

    fn blob_path(&self, hash: &str) -> PathBuf {
        self.dir.join("blobs").join(&hash[..2]).join(hash)
    }

    fn write_private(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let n = self.temporaries.fetch_add(1, Ordering::Relaxed);
        let temporary = path.with_file_name(format!(".tmp-{}-{n}", std::process::id()));
        let written = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(bytes)?;
            fs::rename(&temporary, path)
        })();
        written.map_err(|source| {
            let _ = fs::remove_file(&temporary);
            Error::Io {
                path: path.to_path_buf(),
                source,
            }
        })
    }

    pub fn keep(
        &self,
        rel: &Path,
        kind: Kind,
        previous: impl FnOnce() -> std::io::Result<Vec<u8>>,
    ) -> Result<Option<VersionId>> {
        if !self.config().enabled {
            return Ok(None);
        }
        let note = self.note_dir(rel);
        let edit_is_early = |latest: &Option<Entry>, now: u64| {
            kind.waits()
                && latest.as_ref().is_some_and(|l| {
                    l.at_ms <= now + AHEAD_OF_CLOCK.as_millis() as u64
                        && now.saturating_sub(l.at_ms) <= EDIT_INTERVAL.as_millis() as u64
                })
        };
        if edit_is_early(&last_entry(&note)?, self.now_ms()) {
            return Ok(None);
        }
        let _lock = self.lock()?;
        let latest = last_entry(&note)?;
        let now = self.now_ms();
        if edit_is_early(&latest, now) {
            return Ok(None);
        }
        let bytes = match previous() {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(Error::Io {
                    path: rel.to_path_buf(),
                    source,
                });
            }
        };
        let hash = hex(&Sha256::digest(&bytes));
        if latest.as_ref().is_some_and(|l| l.hash == hash) {
            return Ok(None);
        }
        self.store_blob(&hash, &bytes)?;
        ensure_dir(&note)?;
        let path_file = note.join("path");
        if fs::read(&path_file).ok().as_deref() != Some(rel_bytes(rel).as_slice()) {
            self.write_private(&path_file, &rel_bytes(rel))?;
        }
        let at = latest.as_ref().map_or(now, |l| now.max(l.at_ms + 1));
        let name = Entry::name_for(at, kind, bytes.len() as u64, &hash);
        let version = note.join(&name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&version)
        {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(io(&version)(e)),
        }
        Ok(Some(VersionId(name)))
    }

    fn store_blob(&self, hash: &str, bytes: &[u8]) -> Result<()> {
        let path = self.blob_path(hash);
        if fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64) {
            return Ok(());
        }
        if let Some(shard) = path.parent() {
            ensure_dir(shard)?;
        }
        self.write_private(&path, bytes)
    }

    pub fn list(&self, rel: &Path) -> Result<Vec<Version>> {
        let note = self.note_dir(rel);
        let mut versions: Vec<(u64, Version)> = entries(&note)?
            .into_iter()
            .filter(|e| fs::metadata(self.blob_path(&e.hash)).is_ok_and(|m| m.len() == e.bytes))
            .map(|e| {
                let at = UNIX_EPOCH + Duration::from_millis(e.at_ms);
                (
                    e.at_ms,
                    Version {
                        id: VersionId(e.name),
                        at,
                        bytes: e.bytes,
                        kind: e.kind,
                    },
                )
            })
            .collect();
        versions.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.id.0.cmp(&a.1.id.0)));
        Ok(versions.into_iter().map(|(_, v)| v).collect())
    }

    pub fn read(&self, rel: &Path, id: &VersionId) -> Result<String> {
        let entry = Entry::parse(&id.0).ok_or_else(|| Error::UnknownVersion(id.0.clone()))?;
        let marker = self.note_dir(rel).join(&entry.name);
        if !marker.exists() {
            return Err(Error::UnknownVersion(id.0.clone()));
        }
        let path = self.blob_path(&entry.hash);
        let bytes = fs::read(&path).map_err(io(&path))?;
        if hex(&Sha256::digest(&bytes)) != entry.hash {
            return Err(Error::Damaged(path));
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    pub fn move_note(&self, from: &Path, to: &Path) -> Result<()> {
        if from == to {
            return Ok(());
        }
        let _lock = self.lock()?;
        self.relocate(from, to)
    }

    pub fn move_folder(&self, from: &Path, to: &Path) -> Result<()> {
        if from == to {
            return Ok(());
        }
        let _lock = self.lock()?;
        let notes = self.dir.join("notes");
        let mut moves = Vec::new();
        for item in fs::read_dir(&notes).map_err(io(&notes))? {
            let item = item.map_err(io(&notes))?;
            let Ok(raw) = fs::read(item.path().join("path")) else {
                continue;
            };
            #[cfg(unix)]
            let path = {
                use std::os::unix::ffi::OsStrExt as _;
                PathBuf::from(std::ffi::OsStr::from_bytes(&raw))
            };
            #[cfg(not(unix))]
            let Ok(path) = String::from_utf8(raw).map(PathBuf::from) else {
                continue;
            };
            if let Ok(rest) = path.strip_prefix(from) {
                moves.push((path.clone(), to.join(rest)));
            }
        }
        for (old, new) in moves {
            self.relocate(&old, &new)?;
        }
        Ok(())
    }

    fn relocate(&self, from: &Path, to: &Path) -> Result<()> {
        let (old_dir, new_dir) = (self.note_dir(from), self.note_dir(to));
        if old_dir == new_dir || !old_dir.is_dir() {
            return Ok(());
        }
        if !new_dir.exists() {
            fs::rename(&old_dir, &new_dir).map_err(io(&old_dir))?;
        } else {
            for item in fs::read_dir(&old_dir).map_err(io(&old_dir))? {
                let item = item.map_err(io(&old_dir))?;
                let target = new_dir.join(item.file_name());
                if item.file_name() == "path" || target.exists() {
                    fs::remove_file(item.path()).map_err(io(&item.path()))?;
                } else {
                    fs::rename(item.path(), &target).map_err(io(&target))?;
                }
            }
            fs::remove_dir(&old_dir).map_err(io(&old_dir))?;
        }
        self.write_private(&new_dir.join("path"), &rel_bytes(to))
    }

    pub fn prune(&self) -> Result<PruneReport> {
        self.prune_with(self.config())
    }

    pub fn clear(&self) -> Result<PruneReport> {
        self.prune_with(HistoryConfig {
            days: 0,
            max_bytes: 0,
            ..self.config()
        })
    }

    fn prune_with(&self, config: HistoryConfig) -> Result<PruneReport> {
        let _lock = self.lock()?;
        let now = self.now_ms();
        let horizon = now.saturating_sub(config.days as u64 * DAY_MS);
        let notes = self.dir.join("notes");

        struct Held {
            folder: PathBuf,
            entry: Entry,
        }
        let mut held = Vec::new();
        let mut report = PruneReport::default();
        for item in fs::read_dir(&notes).map_err(io(&notes))? {
            let folder = item.map_err(io(&notes))?.path();
            for entry in entries(&folder)? {
                let intact =
                    fs::metadata(self.blob_path(&entry.hash)).is_ok_and(|m| m.len() == entry.bytes);
                if entry.at_ms < horizon || !intact {
                    remove_file(&folder.join(&entry.name))?;
                    report.versions += 1;
                } else {
                    held.push(Held {
                        folder: folder.clone(),
                        entry,
                    });
                }
            }
        }

        held.sort_by_key(|h| h.entry.at_ms);
        let mut references: HashMap<&str, (u32, u64)> = HashMap::new();
        for h in &held {
            references
                .entry(h.entry.hash.as_str())
                .or_insert((0, h.entry.bytes))
                .0 += 1;
        }
        let mut total: u64 = references.values().map(|&(_, bytes)| bytes).sum();
        let mut dropped = HashSet::new();
        for (i, h) in held.iter().enumerate() {
            if total <= config.max_bytes {
                break;
            }
            remove_file(&h.folder.join(&h.entry.name))?;
            report.versions += 1;
            dropped.insert(i);
            let (count, bytes) = references
                .get_mut(h.entry.hash.as_str())
                .expect("every held version was counted");
            *count -= 1;
            if *count == 0 {
                total -= *bytes;
            }
        }
        let live: HashSet<String> = held
            .iter()
            .enumerate()
            .filter(|(i, _)| !dropped.contains(i))
            .map(|(_, h)| h.entry.hash.clone())
            .collect();

        report.bytes = self.collect_blobs(&live, now)?;
        self.remove_empty_notes(&notes)?;
        Ok(report)
    }

    fn collect_blobs(&self, live: &HashSet<String>, now: u64) -> Result<u64> {
        let blobs = self.dir.join("blobs");
        let mut freed = 0;
        for shard in fs::read_dir(&blobs).map_err(io(&blobs))? {
            let shard = shard.map_err(io(&blobs))?.path();
            let Ok(files) = fs::read_dir(&shard) else {
                continue;
            };
            for file in files {
                let file = file.map_err(io(&shard))?;
                let name = file.file_name().to_string_lossy().into_owned();
                let Ok(meta) = file.metadata() else { continue };
                if name.starts_with(".tmp-") {
                    let age = meta
                        .modified()
                        .ok()
                        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_millis() as u64);
                    if now.saturating_sub(age) > STALE_TEMPORARY.as_millis() as u64 {
                        remove_file(&file.path())?;
                    }
                } else if !live.contains(&name) {
                    freed += meta.len();
                    remove_file(&file.path())?;
                }
            }
            let _ = fs::remove_dir(&shard);
        }
        Ok(freed)
    }

    fn remove_empty_notes(&self, notes: &Path) -> Result<()> {
        for item in fs::read_dir(notes).map_err(io(notes))? {
            let folder = item.map_err(io(notes))?.path();
            if entries(&folder)?.is_empty() {
                let _ = fs::remove_file(folder.join("path"));
                let _ = fs::remove_dir(&folder);
            }
        }
        Ok(())
    }

    pub fn size(&self) -> Result<u64> {
        let blobs = self.dir.join("blobs");
        let mut total = 0;
        for shard in fs::read_dir(&blobs).map_err(io(&blobs))? {
            let Ok(files) = fs::read_dir(shard.map_err(io(&blobs))?.path()) else {
                continue;
            };
            total += files
                .filter_map(|f| f.ok()?.metadata().ok())
                .map(|m| m.len())
                .sum::<u64>();
        }
        Ok(total)
    }
}

fn entries(note: &Path) -> Result<Vec<Entry>> {
    let items = match fs::read_dir(note) {
        Ok(items) => items,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io(note)(e)),
    };
    let mut out = Vec::new();
    for item in items {
        let item = match item {
            Ok(item) => item,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(io(note)(e)),
        };
        if let Some(entry) = item.file_name().to_str().and_then(Entry::parse) {
            out.push(entry);
        }
    }
    Ok(out)
}

fn last_entry(note: &Path) -> Result<Option<Entry>> {
    Ok(entries(note)?.into_iter().max_by_key(|e| e.at_ms))
}

fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(path)(e)),
        _ => Ok(()),
    }
}

fn ensure_dir(path: &Path) -> Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => Err(io(path)(e)),
        _ => Ok(()),
    }
}

fn rel_bytes(rel: &Path) -> Vec<u8> {
    let mut out = Vec::new();
    for component in rel.components() {
        if let std::path::Component::Normal(name) = component {
            if !out.is_empty() {
                out.push(b'/');
            }
            out.extend_from_slice(name.as_encoded_bytes());
        }
    }
    out
}

fn note_key(rel: &Path) -> String {
    hex(&Sha256::digest(rel_bytes(rel))[..16])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
#[cfg(unix)]
mod tests;
