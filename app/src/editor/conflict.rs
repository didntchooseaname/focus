use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use focus_core::VaultError;
use focus_history::{History, Kind};
use gpui::{AppContext as _, Context, Task, Window};

use super::{EditorEvent, Load, NoteEditor, REPARSE};
use crate::library::Library;

#[derive(Clone)]
pub(crate) struct Theirs {
    text: String,
    modified: Option<SystemTime>,
    merged: Option<String>,
    merged_from: String,
}

enum Found {
    Same {
        text: String,
        modified: Option<SystemTime>,
    },
    Differs {
        theirs: Theirs,
        kept: Result<(), focus_history::Error>,
    },
}

type Target = Option<(Arc<History>, PathBuf)>;

fn keep(target: &Target, kind: Kind, text: &str) -> Result<(), focus_history::Error> {
    let Some((history, rel)) = target else {
        return Ok(());
    };
    history
        .keep(rel, kind, || Ok(text.as_bytes().to_vec()))
        .map(|_| ())
}

fn already_kept(target: &Target, theirs: &str, ours: &str) -> bool {
    let Some((history, rel)) = target else {
        return false;
    };
    let Ok(versions) = history.list(rel) else {
        return false;
    };
    let [first, second, ..] = &versions[..] else {
        return false;
    };
    let text = |kind: Kind| {
        [first, second]
            .into_iter()
            .find(|v| v.kind == kind)
            .and_then(|v| history.read(rel, &v.id).ok())
    };
    text(Kind::External).as_deref() == Some(theirs) && text(Kind::Unsaved).as_deref() == Some(ours)
}

static KEEPING: Mutex<()> = Mutex::new(());

fn compare(
    theirs: String,
    modified: Option<SystemTime>,
    base: &str,
    ours: String,
    target: &Target,
) -> Found {
    if theirs == ours {
        return Found::Same {
            text: theirs,
            modified,
        };
    }
    let kept = {
        let _keeping = KEEPING.lock().unwrap_or_else(PoisonError::into_inner);
        if already_kept(target, &theirs, &ours) {
            Ok(())
        } else {
            keep(target, Kind::External, &theirs).and_then(|()| keep(target, Kind::Unsaved, &ours))
        }
    };
    let merged = focus_history::merge(base, &ours, &theirs);
    Found::Differs {
        theirs: Theirs {
            text: theirs,
            modified,
            merged,
            merged_from: ours,
        },
        kept,
    }
}

pub(crate) fn write_copy(
    library: &Library,
    loose: Option<&Path>,
    note: &Path,
    text: &str,
    label: &str,
) -> Result<PathBuf, VaultError> {
    let stem = note
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = note
        .extension()
        .map_or_else(|| "md".to_string(), |e| e.to_string_lossy().into_owned());
    for n in 1..1000 {
        let name = match n {
            1 => format!("{stem} ({label}).{extension}"),
            n => format!("{stem} ({label} {n}).{extension}"),
        };
        let path = note.with_file_name(name);
        let written = match loose {
            Some(_) => std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .and_then(|mut file| std::io::Write::write_all(&mut file, text.as_bytes()))
                .map_err(|source| match source.kind() {
                    std::io::ErrorKind::AlreadyExists => VaultError::AlreadyExists(path.clone()),
                    _ => VaultError::Io {
                        path: path.clone(),
                        source,
                    },
                }),
            None => library.vault().create_note(&path, text).map(|_| ()),
        };
        match written {
            Ok(()) => return Ok(path),
            Err(VaultError::AlreadyExists(_)) => continue,
            Err(error) => return Err(error),
        }
    }
    Err(VaultError::AlreadyExists(note.to_path_buf()))
}

impl NoteEditor {
    fn history_target(&self) -> Target {
        if self.loose.is_some() {
            return None;
        }
        let history = self.library.vault().history()?;
        Some((history, self.meta.path.to_path_buf()))
    }

    fn disk_reader(
        &self,
    ) -> impl FnOnce() -> (Result<String, VaultError>, Option<SystemTime>) + Send + 'static {
        let library = self.library.clone();
        let id = self.id;
        let loose = self.loose.clone();
        move || match loose {
            Some(path) => crate::loose::read(&path),
            None => {
                let modified = library.note(id).map(|m| m.modified);
                (library.read(id), modified)
            }
        }
    }

    pub fn disk_changed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<()>> {
        if self.dirty || self.live.is_some() {
            self.check_conflict(cx);
            None
        } else {
            self.reload_from_disk(window, cx)
        }
    }

    fn reload_from_disk(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<()>> {
        let target = if self.follower || !matches!(self.load, Load::Ready) {
            None
        } else {
            self.history_target()
        };
        if target.is_none() {
            return (!self.parked).then(|| self.load(window, cx));
        }
        let shown = self.loaded.clone();
        let read = self.disk_reader();
        let work = cx.background_spawn(async move {
            let (text, modified) = read();
            if let Ok(text) = &text
                && *text != shown
                && let Err(error) = keep(&target, Kind::Reload, &shown)
            {
                eprintln!("focus: keeping the text shown before a reload: {error}");
            }
            (text, modified)
        });
        Some(cx.spawn_in(window, async move |this, cx| {
            let (result, modified) = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.parked {
                    return;
                }
                if this.dirty || this.live.is_some() {
                    this.check_conflict(cx);
                    return;
                }
                match result {
                    Ok(text) if text == this.loaded => {
                        this.disk_modified = modified;
                        if let Some(meta) = this.library.note(this.id) {
                            this.meta = meta;
                        }
                        cx.emit(EditorEvent::Changed);
                    }
                    result => this.fill(result, modified, window, cx),
                }
            });
        }))
    }

    pub(crate) fn check_conflict(&mut self, cx: &mut Context<Self>) {
        if self.follower {
            self.conflict = true;
            cx.emit(EditorEvent::Changed);
            cx.notify();
            return;
        }
        let ours = self.on_disk(&self.source(cx));
        let base = self.loaded.clone();
        let read = self.disk_reader();
        let target = self.history_target();
        let work = cx.background_spawn(async move {
            let (theirs, modified) = read();
            Ok::<_, VaultError>(compare(theirs?, modified, &base, ours, &target))
        });
        self.conflict_task = Some(cx.spawn(async move |this, cx| {
            let found: Result<Found, VaultError> = work.await;
            let _ = this.update(cx, |this, cx| {
                this.conflict_task = None;
                this.conflict_found(found, cx);
            });
        }));
    }

    fn conflict_found(&mut self, found: Result<Found, VaultError>, cx: &mut Context<Self>) {
        match found {
            Ok(Found::Same { text, modified }) => {
                self.loaded = text;
                self.disk_modified = modified;
                self.conflict = false;
                self.theirs = None;
                self.dirty = self.on_disk(&self.source(cx)) != self.loaded;
                if self.dirty {
                    self.schedule_save(cx);
                }
            }
            Ok(Found::Differs { theirs, kept }) => {
                if let Err(error) = kept {
                    eprintln!("focus: keeping both texts of a conflict on disk: {error}");
                    cx.emit(EditorEvent::Failed(focus_i18n::t(
                        "ui-conflict-versions-failed",
                    )));
                }
                self.conflict = true;
                self.theirs = Some(Arc::new(theirs));
            }
            Err(error) => {
                eprintln!("focus: reading a note changed on disk: {error}");
                self.conflict = true;
                self.theirs = None;
            }
        }
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    pub fn can_merge(&self) -> bool {
        self.theirs.as_ref().is_some_and(|t| t.merged.is_some())
    }

    pub fn merge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(theirs) = self.theirs.clone() else {
            return;
        };
        let ours = self.on_disk(&self.source(cx));
        if ours == theirs.merged_from {
            self.apply_merge(theirs.merged.clone(), window, cx);
            return;
        }
        let (base, from) = (self.loaded.clone(), ours.clone());
        let work =
            cx.background_spawn(async move { focus_history::merge(&base, &from, &theirs.text) });
        self.conflict_task = Some(cx.spawn_in(window, async move |this, cx| {
            let merged = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.conflict_task = None;
                if this.on_disk(&this.source(cx)) != ours {
                    this.merge(window, cx);
                } else {
                    this.apply_merge(merged, window, cx);
                }
            });
        }));
    }

    fn apply_merge(&mut self, merged: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(theirs) = self.theirs.take() else {
            return;
        };
        let Some(merged) = merged else {
            self.theirs = Some(Arc::new(Theirs {
                merged: None,
                ..Theirs::clone(&theirs)
            }));
            cx.emit(EditorEvent::Failed(focus_i18n::t(
                "ui-conflict-merge-overlap",
            )));
            cx.emit(EditorEvent::Changed);
            cx.notify();
            return;
        };
        self.close_block(false, window, cx);
        self.loaded = theirs.text.clone();
        self.disk_modified = theirs.modified;
        self.crlf = self.loaded.contains("\r\n");
        self.conflict = false;
        let buffer = if self.crlf {
            merged.replace("\r\n", "\n")
        } else {
            merged
        };
        self.input.update(cx, |input, cx| {
            super::replace_changed_span(input, &buffer, window, cx)
        });
        self.dirty = self.on_disk(&self.source(cx)) != self.loaded;
        self.schedule_parse(REPARSE, cx);
        if self.dirty {
            self.schedule_save(cx);
        }
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    pub fn keep_both(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.conflict_task.is_some() {
            return;
        }
        let text = self.on_disk(&self.source(cx));
        let library = self.library.clone();
        let loose = self.loose.clone();
        let note = match &loose {
            Some(path) => path.clone(),
            None => self.meta.path.to_path_buf(),
        };
        let label = focus_i18n::t("ui-conflict-copy");
        let work = cx.background_spawn(async move {
            write_copy(&library, loose.as_deref(), &note, &text, &label)
        });
        self.conflict_task = Some(cx.spawn_in(window, async move |this, cx| {
            let written = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.conflict_task = None;
                match written {
                    Ok(path) => {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        cx.emit(EditorEvent::Notice(focus_i18n::t_args(
                            "ui-conflict-both-kept",
                            &[("name", name.as_str().into())],
                        )));
                        this.dirty = false;
                        this.load(window, cx).detach();
                    }
                    Err(error) => cx.emit(EditorEvent::Failed(focus_i18n::t_args(
                        "reading-save-failed",
                        &[("error", crate::errors::vault(&error).as_str().into())],
                    ))),
                }
            });
        }));
    }

    pub fn scan_copies(&mut self, cx: &mut Context<Self>) {
        if self.loose.is_some() {
            return;
        }
        let vault = self.library.vault().clone();
        let id = self.id;
        let work = cx.background_spawn(async move {
            focus_view_history::model::conflict_copies(&vault, id).len()
        });
        self.copies_task = Some(cx.spawn(async move |this, cx| {
            let count = work.await;
            let _ = this.update(cx, |this, cx| {
                this.copies_task = None;
                if this.copies != count {
                    this.copies = count;
                    cx.notify();
                }
            });
        }));
    }

    pub fn in_folder(&self, folder: &Path) -> bool {
        self.loose.is_none() && self.meta.path.parent().unwrap_or(Path::new("")) == folder
    }
}

#[cfg(test)]
mod tests;
