use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use focus_core::{LinkTarget, NoteId, NoteMeta, Vault, VaultError};
use focus_view_reading::Resolver;
use gpui::{Context, Window};

use crate::editor::NoteEditor;
use crate::shell::Shell;

static IDS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

static WRITTEN: Mutex<Option<HashMap<PathBuf, SystemTime>>> = Mutex::new(None);

pub fn id_for(path: &Path) -> NoteId {
    let mut ids = IDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let index = match ids.iter().position(|known| known == path) {
        Some(index) => index,
        None => {
            ids.push(path.to_path_buf());
            ids.len() - 1
        }
    };
    NoteId::MAX - index as NoteId
}

pub fn path_for(id: NoteId) -> Option<PathBuf> {
    let index = usize::try_from(NoteId::MAX - id).ok()?;
    IDS.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(index)
        .cloned()
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub fn read(path: &Path) -> (Result<String, VaultError>, Option<SystemTime>) {
    let io = |source| VaultError::Io {
        path: path.to_path_buf(),
        source,
    };
    (std::fs::read_to_string(path).map_err(io), modified(path))
}

pub fn write(
    path: &Path,
    text: &str,
    expected: Option<SystemTime>,
) -> Result<SystemTime, VaultError> {
    let io = |source| VaultError::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(expected) = expected
        && std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map_err(io)?
            != expected
    {
        return Err(VaultError::Conflict(path.to_path_buf()));
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.focus-write"));
    std::fs::write(&temporary, text).map_err(io)?;
    let written = std::fs::metadata(&temporary)
        .and_then(|m| m.modified())
        .map_err(io)?;
    WRITTEN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(path.to_path_buf(), written);
    std::fs::rename(&temporary, path).map_err(io)?;
    Ok(written)
}

fn ours(path: &Path) -> bool {
    let written = WRITTEN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .and_then(|written| written.get(path).copied());
    written.is_some() && written == modified(path)
}

pub struct LooseResolver {
    folder: PathBuf,
    vault: Arc<Vault>,
}

impl LooseResolver {
    pub fn new(file: &Path, vault: Arc<Vault>) -> Self {
        LooseResolver {
            folder: file.parent().map(Path::to_path_buf).unwrap_or_default(),
            vault,
        }
    }

    fn beside(&self, target: &str) -> Option<LinkTarget> {
        let target = target.split('|').next().unwrap_or(target);
        let (target, anchor) = match target.split_once('#') {
            Some((target, anchor)) => (target, Some(anchor.to_string())),
            None => (target, None),
        };
        let target = percent_decoded(target.trim());
        if target.is_empty() || Path::new(&target).is_absolute() {
            return None;
        }
        let found = [
            self.folder.join(&target),
            self.folder.join(format!("{target}.md")),
        ]
        .into_iter()
        .find(|candidate| candidate.is_file())?;
        let markdown = found
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"));
        if let Ok(relative) = found.strip_prefix(self.vault.root()) {
            return Some(match self.vault.note_by_path(relative) {
                Some(id) => LinkTarget::Note { id, anchor },
                None => LinkTarget::Attachment(relative.to_path_buf()),
            });
        }
        Some(if markdown {
            LinkTarget::Note {
                id: id_for(&found),
                anchor,
            }
        } else {
            LinkTarget::Attachment(found)
        })
    }
}

fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let byte = (bytes[i] == b'%')
            .then(|| bytes.get(i + 1..i + 3))
            .flatten()
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match byte {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Resolver for LooseResolver {
    fn resolve_link(&self, _from: NoteId, target: &str) -> LinkTarget {
        self.beside(target)
            .unwrap_or_else(|| self.vault.resolve_link_from_root(target))
    }
    fn absolute(&self, relative: &Path) -> PathBuf {
        self.vault.absolute(relative)
    }
    fn resolve_from_root(&self, target: &str) -> LinkTarget {
        self.vault.resolve_link_from_root(target)
    }
    fn note_title(&self, note: NoteId) -> Option<String> {
        match path_for(note) {
            Some(path) => Some(title_of(&path)),
            None => Resolver::note_title(self.vault.as_ref(), note),
        }
    }
    fn note_text(&self, note: NoteId) -> Option<Result<(String, Arc<NoteMeta>), String>> {
        let Some(path) = path_for(note) else {
            return Resolver::note_text(self.vault.as_ref(), note);
        };
        Some(match std::fs::read_to_string(&path) {
            Ok(text) => Ok((text, Arc::new(meta(&path, note)))),
            Err(_) => Err(focus_i18n::t_args(
                "reading-embed-unreadable",
                &[("note", path.to_string_lossy().as_ref().into())],
            )),
        })
    }
}

impl NoteEditor {
    pub(crate) fn resolver(&self) -> Arc<dyn Resolver> {
        match &self.loose {
            Some(path) => Arc::new(LooseResolver::new(path, self.library.vault().clone())),
            None => self.library.vault().clone(),
        }
    }

    pub(crate) fn sheet_meta(&self) -> Arc<NoteMeta> {
        let Some(path) = &self.loose else {
            return self
                .library
                .note(self.id)
                .unwrap_or_else(|| self.meta.clone());
        };
        let mut meta = (*self.meta).clone();
        let mut crumb = PathBuf::from(focus_i18n::t("ui-loose-crumb"));
        crumb.extend(crumbs(path));
        crumb.push(path.file_name().unwrap_or_default());
        meta.path = Arc::from(crumb.as_path());
        Arc::new(meta)
    }
}

pub fn crumbs(path: &Path) -> Vec<String> {
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    let home = focus_core::settings::home_dir();
    match home
        .as_deref()
        .and_then(|home| parent.strip_prefix(home).ok())
    {
        Some(below) => std::iter::once("~".to_string())
            .chain(below.iter().map(|part| part.to_string_lossy().into_owned()))
            .collect(),
        None => parent
            .iter()
            .filter(|part| *part != std::ffi::OsStr::new(std::path::MAIN_SEPARATOR_STR))
            .map(|part| part.to_string_lossy().into_owned())
            .collect(),
    }
}

fn title_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn meta(path: &Path, id: NoteId) -> NoteMeta {
    let title = title_of(path);
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    NoteMeta {
        id,
        path: Arc::from(path),
        title: Arc::from(title.as_str()),
        aliases: Vec::new(),
        tags: Vec::new(),
        headings: Vec::new(),
        blocks: Vec::new(),
        props: Vec::new(),
        links: Vec::new(),
        words: 0,
        tasks_open: 0,
        tasks_done: 0,
        size,
        modified: SystemTime::UNIX_EPOCH,
    }
}

pub fn route(
    given: Option<PathBuf>,
    paths: Vec<PathBuf>,
    known: &[PathBuf],
) -> (Option<PathBuf>, Vec<PathBuf>) {
    let (folders, files): (Vec<PathBuf>, Vec<PathBuf>) =
        paths.into_iter().partition(|p| p.is_dir());
    let vault_of = |file: &std::path::Path| {
        file.ancestors()
            .skip(1)
            .find(|dir| dir.join(".obsidian").is_dir())
            .map(std::path::Path::to_path_buf)
            .or_else(|| known.iter().find(|vault| file.starts_with(vault)).cloned())
    };
    let vault = folders
        .into_iter()
        .next()
        .or_else(|| files.iter().find_map(|f| vault_of(f)))
        .or(given);
    (vault, files)
}

impl Shell {
    pub fn open_given_paths(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        for path in std::mem::take(&mut self.open_paths) {
            let inside = path
                .strip_prefix(library.root())
                .ok()
                .and_then(|relative| library.note_by_path(relative));
            match inside {
                Some(id) => self.open_note(id, true, window, cx),
                None if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("md")) =>
                {
                    self.open_loose(path, window, cx)
                }
                None if crate::media::opens(&path) => self.open_media(&path, None, window, cx),
                None => self.open_file(&path, cx),
            }
        }
    }

    pub fn open_loose(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let id = id_for(&path);
        if let Some(index) = self.tabs.iter().position(|tab| tab.read(cx).id == id) {
            self.activate(index, window, cx);
            return;
        }
        self.watch_loose(&path, window, cx);
        self.open_meta(Arc::new(meta(&path, id)), Some(path), true, window, cx);
    }

    fn watch_loose(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let Some(folder) = path.parent() else { return };
        if self.loose_watch.is_none() {
            match watch::Watch::start() {
                Ok((watch, changes)) => {
                    self.loose_watch = Some(watch);
                    cx.spawn_in(window, async move |this, cx| {
                        while let Ok(changed) = changes.recv().await {
                            let alive = this.update_in(cx, |this, window, cx| {
                                this.loose_changed(&changed, window, cx)
                            });
                            if alive.is_err() {
                                break;
                            }
                        }
                    })
                    .detach();
                }
                Err(error) => {
                    eprintln!("focus: watching loose notes: {error}");
                    return;
                }
            }
        }
        if let Some(watch) = &self.loose_watch
            && let Err(error) = watch.add(folder)
        {
            eprintln!("focus: watching {}: {error}", folder.display());
        }
    }

    fn loose_changed(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if !path.is_file() || ours(path) {
            return;
        }
        for tab in self.all_tabs() {
            if tab.read(cx).loose.as_deref() == Some(path) {
                tab.update(cx, |editor, cx| {
                    if let Some(task) = editor.external_change(window, cx) {
                        task.detach();
                    }
                });
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) mod watch {
    use std::collections::HashMap;
    use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    pub struct Watch {
        fd: Arc<OwnedFd>,
        folders: Arc<Mutex<HashMap<i32, PathBuf>>>,
    }

    const MASK: u32 = libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_ONLYDIR;

    impl Watch {
        pub fn start() -> std::io::Result<(Watch, smol::channel::Receiver<PathBuf>)> {
            // SAFETY: no pointer is passed; the result is checked.
            let raw = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
            if raw < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // SAFETY: `raw` is a descriptor this function just opened and owns alone.
            let fd = Arc::new(unsafe { OwnedFd::from_raw_fd(raw) });
            let folders: Arc<Mutex<HashMap<i32, PathBuf>>> = Arc::default();
            let (sender, changes) = smol::channel::unbounded();
            let (reader, names) = (fd.clone(), folders.clone());
            std::thread::Builder::new()
                .name("focus-loose-watch".into())
                .spawn(move || read_events(&reader, &names, &sender))?;
            Ok((Watch { fd, folders }, changes))
        }

        pub fn add(&self, folder: &Path) -> std::io::Result<()> {
            use std::os::unix::ffi::OsStrExt as _;
            let mut folders = self
                .folders
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if folders.values().any(|known| known == folder) {
                return Ok(());
            }
            let name = std::ffi::CString::new(folder.as_os_str().as_bytes())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
            // SAFETY: `name` is a valid NUL terminated string that outlives the call.
            let wd = unsafe { libc::inotify_add_watch(self.fd.as_raw_fd(), name.as_ptr(), MASK) };
            if wd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            folders.insert(wd, folder.to_path_buf());
            Ok(())
        }
    }

    fn read_events(
        fd: &OwnedFd,
        folders: &Mutex<HashMap<i32, PathBuf>>,
        sender: &smol::channel::Sender<PathBuf>,
    ) {
        use std::os::unix::ffi::OsStrExt as _;
        let header = std::mem::size_of::<libc::inotify_event>();
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            // SAFETY: the buffer is valid for its whole length during the call.
            let read =
                unsafe { libc::read(fd.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
            if read < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                eprintln!(
                    "focus: watching loose notes: {}",
                    std::io::Error::last_os_error()
                );
                return;
            }
            let mut at = 0;
            while at + header <= read as usize {
                // SAFETY: the kernel wrote a whole event header at `at`; `read_unaligned`
                // accepts any alignment.
                let event: libc::inotify_event =
                    unsafe { std::ptr::read_unaligned(buffer.as_ptr().add(at).cast()) };
                let start = at + header;
                let end = (start + event.len as usize).min(read as usize);
                at = start + event.len as usize;
                let name = &buffer[start..end];
                let name = &name[..name.iter().position(|b| *b == 0).unwrap_or(name.len())];
                let folder = folders
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .get(&event.wd)
                    .cloned();
                if let Some(folder) = folder
                    && !name.is_empty()
                    && sender
                        .send_blocking(folder.join(std::ffi::OsStr::from_bytes(name)))
                        .is_err()
                {
                    return;
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) mod watch {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use notify::event::{EventKind, ModifyKind};
    use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};

    pub struct Watch {
        watcher: Mutex<RecommendedWatcher>,
        folders: Mutex<Vec<PathBuf>>,
    }

    impl Watch {
        pub fn start() -> std::io::Result<(Watch, smol::channel::Receiver<PathBuf>)> {
            let (sender, changes) = smol::channel::unbounded();
            let watcher =
                notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    let Ok(event) = event else { return };
                    if matches!(
                        event.kind,
                        EventKind::Create(_)
                            | EventKind::Modify(
                                ModifyKind::Data(_) | ModifyKind::Name(_) | ModifyKind::Any
                            )
                    ) {
                        for path in event.paths {
                            if sender.send_blocking(path).is_err() {
                                return;
                            }
                        }
                    }
                })
                .map_err(std::io::Error::other)?;
            Ok((
                Watch {
                    watcher: Mutex::new(watcher),
                    folders: Mutex::default(),
                },
                changes,
            ))
        }

        pub fn add(&self, folder: &Path) -> std::io::Result<()> {
            let mut folders = self
                .folders
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if folders.iter().any(|known| known == folder) {
                return Ok(());
            }
            self.watcher
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .watch(folder, RecursiveMode::NonRecursive)
                .map_err(std::io::Error::other)?;
            folders.push(folder.to_path_buf());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_opens_its_folder_its_vault_or_the_given_one() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-tmp/route");
        let _ = std::fs::remove_dir_all(&dir);
        let (marked, known, given) = (dir.join("marked"), dir.join("known"), dir.join("given"));
        std::fs::create_dir_all(marked.join(".obsidian")).unwrap();
        std::fs::create_dir_all(marked.join("sub")).unwrap();
        std::fs::create_dir_all(&known).unwrap();
        let (a, b, c) = (
            marked.join("sub/A.md"),
            known.join("B.md"),
            dir.join("C.md"),
        );
        assert_eq!(
            route(None, vec![known.clone()], &[]),
            (Some(known.clone()), vec![])
        );
        assert_eq!(
            route(Some(given.clone()), vec![a.clone()], &[]),
            (Some(marked.clone()), vec![a])
        );
        assert_eq!(
            route(None, vec![b.clone()], std::slice::from_ref(&known)),
            (Some(known), vec![b])
        );
        assert_eq!(
            route(Some(given.clone()), vec![c.clone()], &[]),
            (Some(given), vec![c.clone()])
        );
        assert_eq!(route(None, vec![c.clone()], &[]), (None, vec![c]));
    }

    #[test]
    fn links_of_a_loose_note_resolve_from_its_folder_then_the_vault() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-tmp/loose-links");
        let _ = std::fs::remove_dir_all(&dir);
        let (vault_root, outside) = (dir.join("vault"), dir.join("outside"));
        std::fs::create_dir_all(vault_root.join(".obsidian")).unwrap();
        std::fs::create_dir_all(outside.join("img")).unwrap();
        std::fs::write(vault_root.join("Inside.md"), "# Inside\n").unwrap();
        std::fs::write(outside.join("Loose.md"), "# Loose\n").unwrap();
        std::fs::write(outside.join("Sibling.md"), "# Sibling\n").unwrap();
        std::fs::write(outside.join("img/a b.png"), b"png").unwrap();
        let vault = Vault::open(
            &vault_root,
            focus_core::OpenOptions {
                cache_dir: dir.join("cache"),
                ignore: Vec::new(),
                watch: false,
            },
        )
        .unwrap();
        let resolver = LooseResolver::new(&outside.join("Loose.md"), vault.clone());
        let from = id_for(&outside.join("Loose.md"));
        assert!(matches!(
            resolver.resolve_link(from, "img/a%20b.png"),
            LinkTarget::Attachment(path) if path == outside.join("img/a b.png")
        ));
        let sibling = match resolver.resolve_link(from, "Sibling#Part") {
            LinkTarget::Note { id, anchor } => {
                assert_eq!(anchor.as_deref(), Some("Part"));
                id
            }
            other => panic!("expected a note, got {other:?}"),
        };
        assert_eq!(path_for(sibling), Some(outside.join("Sibling.md")));
        assert!(matches!(
            resolver.note_text(sibling),
            Some(Ok((text, _))) if text == "# Sibling\n"
        ));
        let inside = vault.note_by_path(Path::new("Inside.md")).unwrap();
        assert!(matches!(
            resolver.resolve_link(from, "Inside"),
            LinkTarget::Note { id, .. } if id == inside
        ));
        assert!(matches!(
            resolver.resolve_link(from, "/etc/hostname"),
            LinkTarget::Unresolved | LinkTarget::Attachment(_)
        ));
        assert!(!matches!(
            resolver.resolve_link(from, "/etc/hostname"),
            LinkTarget::Attachment(path) if path == Path::new("/etc/hostname")
        ));
    }

    #[test]
    fn a_loose_note_is_written_in_place_unless_it_changed_since() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-tmp/loose");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("Loose.md");
        std::fs::write(&note, "# Loose\n").unwrap();
        let (text, modified) = read(&note);
        assert_eq!(text.unwrap(), "# Loose\n");
        let written = write(&note, "# Loose\n\nEdited.\n", modified).unwrap();
        assert_eq!(
            std::fs::read_to_string(&note).unwrap(),
            "# Loose\n\nEdited.\n"
        );
        assert!(
            !dir.join(".Loose.md.focus-write").exists(),
            "no temporary file left"
        );
        assert!(ours(&note), "the watch knows this change for Focus's own");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&note, "changed elsewhere").unwrap();
        assert!(matches!(
            write(&note, "mine", Some(written)),
            Err(VaultError::Conflict(_))
        ));
    }
}
