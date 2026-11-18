use std::path::{Path, PathBuf};

use gpui::{AppContext as _, Context, Window};

use crate::editor::ViewMode;
use crate::shell::Shell;

fn attachment_folder(note: &Path, folder: &str) -> PathBuf {
    match folder.strip_prefix("./") {
        Some(rest) => note.parent().unwrap_or(Path::new("")).join(rest),
        None => PathBuf::from(folder.trim_start_matches('/')),
    }
}

impl Shell {
    pub fn paste_clipboard(
        &mut self,
        editor: gpui::Entity<crate::editor::NoteEditor>,
        caret: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let image = focus_kit::clipboard::read_image(cx).await;
            let files = match &image {
                Ok(None) => focus_kit::clipboard::read_files(cx).await,
                _ => Ok(None),
            };
            let files = match files {
                Ok(files) => files,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.notify_user(
                            focus_kit::Tone::Danger,
                            crate::errors::clipboard(&error),
                            cx,
                        )
                    });
                    return;
                }
            };
            if let Some(files) = files {
                let _ = this.update_in(cx, |this, window, cx| {
                    if files.refused > 0 {
                        this.notify_user(
                            focus_kit::Tone::Danger,
                            focus_i18n::t_args(
                                "ui-paste-files-refused",
                                &[("count", focus_i18n::Arg::Int(files.refused as i64))],
                            ),
                            cx,
                        );
                    }
                    this.drop_files(&files.paths, window, cx);
                });
                return;
            }
            let text = match &image {
                Ok(None) if caret => Some(focus_kit::clipboard::read_text(cx).await),
                _ => None,
            };
            let _ = this.update_in(cx, |this, window, cx| match (image, text) {
                (Ok(Some(image)), _) => this.attach_bytes(
                    image.bytes,
                    image.extension.to_string(),
                    None,
                    None,
                    None,
                    window,
                    cx,
                ),
                (Ok(None), Some(Ok(Some(text)))) => {
                    editor.update(cx, |editor, cx| editor.insert_at_cursor(&text, window, cx));
                }
                (Ok(None), Some(Ok(None))) => {}
                (Err(error), _) | (Ok(None), Some(Err(error))) => this.notify_user(
                    focus_kit::Tone::Danger,
                    crate::errors::clipboard(&error),
                    cx,
                ),
                (Ok(None), None) => {}
            });
        })
        .detach();
    }

    pub fn drop_files(&mut self, paths: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        for path in paths {
            let path = path.clone();
            self.pending += 1;
            let read = cx.background_spawn({
                let path = path.clone();
                async move { read_dropped(&path) }
            });
            cx.spawn_in(window, async move |this, cx| {
                let result = read.await;
                let _ = this.update_in(cx, |this, window, cx| {
                    this.pending -= 1;
                    let shown = path.display().to_string();
                    match result {
                        Ok(bytes) => {
                            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                            let extension = path
                                .extension()
                                .map(|e| e.to_string_lossy().to_lowercase())
                                .unwrap_or_default();
                            this.attach_bytes(bytes, extension, name, None, None, window, cx);
                        }
                        Err(Dropped::Folder) => this.notify_user(
                            focus_kit::Tone::Danger,
                            focus_i18n::t_args(
                                "ui-attachment-folder",
                                &[("path", shown.as_str().into())],
                            ),
                            cx,
                        ),
                        Err(Dropped::NotAFile) => this.notify_user(
                            focus_kit::Tone::Danger,
                            focus_i18n::t_args(
                                "ui-attachment-not-file",
                                &[("path", shown.as_str().into())],
                            ),
                            cx,
                        ),
                        Err(Dropped::TooLarge) => this.notify_user(
                            focus_kit::Tone::Danger,
                            focus_i18n::t_args(
                                "ui-attachment-too-large",
                                &[
                                    ("path", shown.as_str().into()),
                                    (
                                        "limit",
                                        focus_i18n::format_bytes(MAX_DROPPED).as_str().into(),
                                    ),
                                ],
                            ),
                            cx,
                        ),
                        Err(Dropped::Io(error)) => this.notify_user(
                            focus_kit::Tone::Danger,
                            focus_i18n::t_args(
                                "errors-core-io",
                                &[
                                    ("path", shown.as_str().into()),
                                    ("detail", error.to_string().as_str().into()),
                                ],
                            ),
                            cx,
                        ),
                    }
                });
            })
            .detach();
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn attach_bytes(
        &mut self,
        bytes: Vec<u8>,
        extension: String,
        original_name: Option<String>,
        directory: Option<PathBuf>,
        name_prefix: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let Some(editor) = self.active_editor().cloned() else {
            self.notify_user(focus_kit::Tone::Warn, focus_i18n::t("ui-info-no-note"), cx);
            return;
        };
        let (note, heading, note_path) = {
            let editor = editor.read(cx);
            let line = editor.cursor(cx).line;
            let heading = editor
                .meta
                .headings
                .iter()
                .rev()
                .find(|h| h.line <= line)
                .map(|h| h.text.to_string());
            (editor.id, heading, editor.meta.path.to_path_buf())
        };
        let directory = directory.or_else(|| {
            let configured = library.root().join(".obsidian").join("app.json").is_file();
            let folder = match self.prefs.read(cx).get("editor.attachments") {
                Some(crate::settings::schema::Value::Text(folder))
                    if !configured && !folder.trim().is_empty() =>
                {
                    folder.trim().to_string()
                }
                _ => return None,
            };
            Some(attachment_folder(&note_path, &folder))
        });
        let mut policy = self.store.settings.attachments.clone();
        policy.readable_names = self.prefs.read(cx).bool("editor.paste_names");
        let task = cx.background_spawn(async move {
            let request = focus_core::AttachmentRequest {
                note,
                bytes: &bytes,
                extension: &extension,
                original_name: original_name.as_deref(),
                heading: heading.as_deref(),
                directory: directory.as_deref().map(Path::new),
                name_prefix: name_prefix.as_deref(),
                policy: &policy,
            };
            library.vault().attach(&request)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(saved) => {
                    editor.update(cx, |editor, cx| {
                        if editor.view == ViewMode::Reading
                            || (editor.view == ViewMode::Live && editor.live.is_none())
                        {
                            editor.append(&format!("\n\n{}\n", saved.link), window, cx);
                        } else {
                            editor.insert_block(&saved.link, window, cx);
                        }
                    });
                    let key = if saved.reused {
                        "ui-attachment-reused"
                    } else {
                        "ui-attachment-saved"
                    };
                    this.notify_user(
                        focus_kit::Tone::Ok,
                        focus_i18n::t_args(
                            key,
                            &[("path", saved.path.display().to_string().as_str().into())],
                        ),
                        cx,
                    );
                }
                Err(error) => {
                    this.notify_user(focus_kit::Tone::Danger, crate::errors::vault(&error), cx)
                }
            });
        })
        .detach();
    }
}

impl Shell {
    pub fn pick_attachment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| this.drop_files(&paths, window, cx));
        })
        .detach();
    }
}

const MAX_DROPPED: u64 = 100 * 1024 * 1024;

#[derive(Debug)]
enum Dropped {
    Folder,
    NotAFile,
    TooLarge,
    Io(std::io::Error),
}

fn read_dropped(path: &Path) -> Result<Vec<u8>, Dropped> {
    let meta = std::fs::metadata(path).map_err(Dropped::Io)?;
    if meta.is_dir() {
        return Err(Dropped::Folder);
    }
    if !meta.is_file() {
        return Err(Dropped::NotAFile);
    }
    if meta.len() > MAX_DROPPED {
        return Err(Dropped::TooLarge);
    }
    std::fs::read(path).map_err(Dropped::Io)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    #[test]
    fn a_dropped_file_is_checked_before_it_is_read() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-tmp/dropped");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let small = root.join("shot.png");
        std::fs::write(&small, [1u8, 2, 3]).unwrap();
        assert_eq!(super::read_dropped(&small).unwrap(), vec![1, 2, 3]);
        assert!(matches!(
            super::read_dropped(&root),
            Err(super::Dropped::Folder)
        ));
        let large = root.join("large.bin");
        std::fs::File::create(&large)
            .unwrap()
            .set_len(super::MAX_DROPPED + 1)
            .unwrap();
        assert!(
            matches!(super::read_dropped(&large), Err(super::Dropped::TooLarge)),
            "refused from its size, without reading it"
        );
    }

    #[test]
    fn the_attachments_setting_is_relative_to_the_note_or_the_root() {
        let note = Path::new("Red Team/Kerberoasting.md");
        assert_eq!(
            super::attachment_folder(note, "./assets"),
            PathBuf::from("Red Team/assets")
        );
        assert_eq!(
            super::attachment_folder(note, "Attachments"),
            PathBuf::from("Attachments")
        );
        assert_eq!(
            super::attachment_folder(note, "/Media"),
            PathBuf::from("Media")
        );
    }
}
