mod dialogs;
mod menu;
mod picker;

use std::ops::Range;
use std::path::{Path, PathBuf};

use focus_core::{NoteId, Placement, VaultError};
use focus_view_vault::{Message, check_name};
use gpui::{
    AnyView, AppContext as _, Context, Entity, EntityInputHandler as _, EventEmitter, Focusable,
    MouseButton, MouseDownEvent, Subscription, WeakEntity, Window,
};
use gpui_component::RopeExt as _;
use gpui_component::input::InputState;

use crate::editor::{Load, NoteEditor, ViewMode};
use crate::notices::NoticeText;
use crate::shell::{Overlay, Shell};
use dialogs::{DialogEvent, MergeDialog, TagRenameDialog};
use menu::{EditorMenu, MenuEvent};
use picker::{Choice, NotePicker, PickerEvent};

struct Selection {
    editor: WeakEntity<NoteEditor>,
    note: NoteId,
    range: Range<usize>,
    text: String,
}

enum Purpose {
    Merge(NoteId),
    Extract(Selection),
}

fn field_selection(
    field: &Entity<InputState>,
    window: &mut Window,
    cx: &mut gpui::App,
) -> Option<(Range<usize>, String)> {
    field.update(cx, |input, cx| {
        let selected = input.selected_text_range(true, window, cx)?.range;
        let text = input.text().clone();
        let range =
            text.offset_utf16_to_offset(selected.start)..text.offset_utf16_to_offset(selected.end);
        (!range.is_empty()).then(|| (range.clone(), text.slice(range).to_string()))
    })
}

impl NoteEditor {
    fn compose_selection(
        &self,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> Option<(Range<usize>, String)> {
        if !matches!(self.load, Load::Ready) || self.loose.is_some() {
            return None;
        }
        match (self.view, &self.live) {
            (ViewMode::Source, _) => field_selection(&self.input, window, cx),
            (ViewMode::Live, Some(block)) => {
                let (range, text) = field_selection(&block.input, window, cx)?;
                let base = self
                    .input
                    .read(cx)
                    .text()
                    .line_start_offset(block.lines.start as usize);
                Some((base + range.start..base + range.end, text))
            }
            (ViewMode::Live, None) if self.edit_in_place() => {
                field_selection(&self.input, window, cx)
            }
            _ => None,
        }
    }
}

impl Shell {
    fn compose_animate(&self, cx: &gpui::App) -> bool {
        !self.scripted && !self.prefs.read(cx).bool("appearance.reduce_motion")
    }

    fn show_compose<V, E>(
        &mut self,
        view: Entity<V>,
        on_event: impl Fn(&mut Shell, &E, &mut Window, &mut Context<Shell>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) where
        V: EventEmitter<E> + Focusable + gpui::Render,
        E: 'static,
    {
        view.read(cx).focus_handle(cx).focus(window);
        let subscription: Subscription =
            cx.subscribe_in(&view, window, move |this, _, event: &E, window, cx| {
                on_event(this, event, window, cx)
            });
        let view: AnyView = view.into();
        self.overlay = Overlay::Transfer {
            view,
            _events: subscription,
        };
        cx.notify();
    }

    fn run_compose<T: Send + 'static>(
        &mut self,
        work: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(&mut Shell, T, &mut Window, &mut Context<Shell>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let saves: Vec<_> = self
            .all_tabs()
            .into_iter()
            .filter_map(|tab| {
                tab.update(cx, |editor, cx| {
                    let ready = editor.dirty && matches!(editor.load, Load::Ready);
                    (ready && !editor.follower && !editor.conflict && editor.loose.is_none())
                        .then(|| editor.save(false, cx))
                })
            })
            .collect();
        self.pending += 1;
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            for save in saves {
                save.await;
            }
            let result = background.spawn(async move { work() }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.pending -= 1;
                done(this, result, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn compose_notice(&mut self, tone: focus_kit::Tone, message: Message, cx: &mut Context<Self>) {
        self.notify(tone, NoticeText::Vault(message), cx);
    }

    fn compose_failed(&mut self, error: &VaultError, cx: &mut Context<Self>) {
        let text = crate::errors::vault(error);
        self.notify_user(focus_kit::Tone::Danger, text, cx);
    }

    pub fn open_tag_rename(&mut self, tag: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let animate = self.compose_animate(cx);
        let vault = library.vault().clone();
        let tag = tag.to_string();
        let dialog = cx.new(|cx| TagRenameDialog::new(vault, tag, animate, window, cx));
        window.on_next_frame(|window, cx| {
            if let Ok(select_all) = gpui::Keystroke::parse("ctrl-a") {
                window.dispatch_keystroke(select_all, cx);
            }
        });
        self.show_compose(
            dialog,
            |this, event: &DialogEvent, window, cx| match event {
                DialogEvent::Close => this.close_overlay(window, cx),
                DialogEvent::RenameTag { from, to } => {
                    let (from, to) = (from.clone(), to.clone());
                    this.close_overlay(window, cx);
                    this.rename_tag(from, to, window, cx);
                }
                DialogEvent::Merge => {}
            },
            window,
            cx,
        );
    }

    fn rename_tag(
        &mut self,
        from: String,
        to: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let vault = library.vault().clone();
        let (old, new) = (from.clone(), to.clone());
        self.run_compose(
            move || vault.rename_tag(&old, &new),
            move |this, result, _, cx| match result {
                Ok(report) if report.failed.is_empty() => this.compose_notice(
                    focus_kit::Tone::Ok,
                    Message::new("ui-tag-renamed")
                        .text_arg("from", from)
                        .text_arg("to", to)
                        .count_arg("notes", report.notes.len()),
                    cx,
                ),
                Ok(report) => {
                    for (path, error) in &report.failed {
                        eprintln!("focus: renaming a tag in {}: {error}", path.display());
                    }
                    let first = report.failed[0].0.display().to_string();
                    this.compose_notice(
                        focus_kit::Tone::Danger,
                        Message::new("ui-tag-renamed-partly")
                            .text_arg("to", to)
                            .count_arg("notes", report.notes.len())
                            .count_arg("failed", report.failed.len())
                            .text_arg("path", first),
                        cx,
                    );
                }
                Err(error) => this.compose_failed(&error, cx),
            },
            window,
            cx,
        );
    }

    fn open_picker(&mut self, purpose: Purpose, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let source = match &purpose {
            Purpose::Merge(note) => *note,
            Purpose::Extract(selection) => selection.note,
        };
        let Some(meta) = library.note(source) else {
            return;
        };
        let name = meta
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let placeholder = match &purpose {
            Purpose::Merge(_) => {
                focus_i18n::t_args("ui-merge-placeholder", &[("note", name.as_str().into())])
            }
            Purpose::Extract(_) => focus_i18n::t("ui-extract-placeholder"),
        };
        let recent: Vec<NoteId> = self.history.iter().rev().copied().collect();
        let animate = self.compose_animate(cx);
        let picker =
            cx.new(|cx| NotePicker::new(library, source, recent, placeholder, animate, window, cx));
        let purpose = std::cell::RefCell::new(Some(purpose));
        self.show_compose(
            picker,
            move |this, event: &PickerEvent, window, cx| match event {
                PickerEvent::Close => this.close_overlay(window, cx),
                PickerEvent::Chosen(choice, placement) => {
                    let Some(purpose) = purpose.borrow_mut().take() else {
                        return;
                    };
                    this.close_overlay(window, cx);
                    match purpose {
                        Purpose::Merge(source) => {
                            this.confirm_merge(source, choice.clone(), *placement, window, cx)
                        }
                        Purpose::Extract(selection) => {
                            this.extract(selection, choice.clone(), *placement, window, cx)
                        }
                    }
                }
            },
            window,
            cx,
        );
    }

    fn new_note_path(
        &mut self,
        typed: &str,
        beside: &Path,
        cx: &mut Context<Self>,
    ) -> Option<PathBuf> {
        match check_name(typed, true) {
            Ok(file) => Some(
                beside
                    .parent()
                    .map(|p| p.join(&file))
                    .unwrap_or_else(|| PathBuf::from(&file)),
            ),
            Err(reason) => {
                self.compose_notice(focus_kit::Tone::Danger, reason, cx);
                None
            }
        }
    }

    pub fn open_merge(
        &mut self,
        source: Option<NoteId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = source.or_else(|| self.last_note(cx)) else {
            self.notify_user(
                focus_kit::Tone::Neutral,
                focus_i18n::t("ui-merge-no-note"),
                cx,
            );
            return;
        };
        self.open_picker(Purpose::Merge(source), window, cx);
    }

    fn confirm_merge(
        &mut self,
        source: NoteId,
        choice: Choice,
        placement: Placement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let Some(source_path) = library.note(source).map(|m| m.path.to_path_buf()) else {
            return;
        };
        let vault = library.vault().clone();
        let (destination, new_note, count) = match choice {
            Choice::Note(dest) => {
                let Some(path) = library.note(dest).map(|m| m.path.to_path_buf()) else {
                    return;
                };
                let count = cx.background_spawn(async move {
                    vault
                        .preview_merge(source, dest)
                        .map(|r| r.links)
                        .map_err(|e| crate::errors::vault(&e))
                });
                (path, false, count)
            }
            Choice::New(typed) => {
                let Some(path) = self.new_note_path(&typed, &source_path, cx) else {
                    return;
                };
                let (from, to) = (source_path.clone(), path.clone());
                let count = cx.background_spawn(async move {
                    vault
                        .preview_rename(&from, &to)
                        .map(|r| r.updated_links)
                        .map_err(|e| crate::errors::vault(&e))
                });
                (path, true, count)
            }
        };
        let animate = self.compose_animate(cx);
        let trash = self.tree_settings(cx).trash;
        let dialog = cx.new(|cx| {
            MergeDialog::new(
                source_path,
                destination,
                placement,
                new_note,
                trash,
                count,
                animate,
                cx,
            )
        });
        let read = dialog.downgrade();
        self.show_compose(
            dialog,
            move |this, event: &DialogEvent, window, cx| match event {
                DialogEvent::Close => this.close_overlay(window, cx),
                DialogEvent::Merge => {
                    let Some(dialog) = read.upgrade() else { return };
                    let dialog = dialog.read(cx);
                    let (destination, placement, new_note, trash) = (
                        dialog.destination.clone(),
                        dialog.placement,
                        dialog.new_note,
                        dialog.trash,
                    );
                    this.close_overlay(window, cx);
                    this.merge(source, destination, placement, new_note, trash, window, cx);
                }
                DialogEvent::RenameTag { .. } => {}
            },
            window,
            cx,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn merge(
        &mut self,
        source: NoteId,
        destination: PathBuf,
        placement: Placement,
        new_note: bool,
        trash: focus_core::TrashKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let vault = library.vault().clone();
        let to = destination.clone();
        self.run_compose(
            move || -> Result<(Option<NoteId>, usize), VaultError> {
                if new_note {
                    let from = vault
                        .source_path(source)
                        .ok_or(VaultError::UnknownNote(source))?;
                    let report = vault.rename(&from, &to, true)?;
                    return Ok((vault.note_by_path(&to), report.updated_links));
                }
                let dest = vault
                    .note_by_path(&to)
                    .ok_or_else(|| VaultError::Other(format!("unknown note: {}", to.display())))?;
                let report = vault.merge(source, dest, placement, trash)?;
                Ok((Some(dest), report.links))
            },
            move |this, result, window, cx| match result {
                Ok((dest, links)) => {
                    let name = destination
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    this.compose_notice(
                        focus_kit::Tone::Ok,
                        Message::new("ui-note-merged")
                            .text_arg("destination", name)
                            .count_arg("links", links),
                        cx,
                    );
                    if let Some(dest) = dest {
                        this.open_note(dest, false, window, cx);
                    }
                }
                Err(error) => this.compose_failed(&error, cx),
            },
            window,
            cx,
        );
    }

    pub fn open_extract(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor().cloned() else {
            self.notify_user(
                focus_kit::Tone::Neutral,
                focus_i18n::t("ui-extract-no-selection"),
                cx,
            );
            return;
        };
        let selected = editor.update(cx, |editor, cx| editor.compose_selection(window, cx));
        let Some((range, text)) = selected else {
            self.notify_user(
                focus_kit::Tone::Neutral,
                focus_i18n::t("ui-extract-no-selection"),
                cx,
            );
            return;
        };
        let note = editor.read(cx).id;
        let selection = Selection {
            editor: editor.downgrade(),
            note,
            range,
            text,
        };
        self.open_picker(Purpose::Extract(selection), window, cx);
    }

    fn extract(
        &mut self,
        selection: Selection,
        choice: Choice,
        placement: Placement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let Some(source_path) = library.note(selection.note).map(|m| m.path.to_path_buf()) else {
            return;
        };
        let target = match choice {
            Choice::Note(id) => match library.note(id) {
                Some(meta) => Ok((id, meta.path.to_path_buf())),
                None => return,
            },
            Choice::New(typed) => match self.new_note_path(&typed, &source_path, cx) {
                Some(path) => Err(path),
                None => return,
            },
        };
        let vault = library.vault().clone();
        let (note, text) = (selection.note, selection.text.clone());
        self.run_compose(
            move || -> Result<(NoteId, String, PathBuf), VaultError> {
                let (id, path) = match target {
                    Ok((id, path)) => {
                        let moved = vault.move_text(note, &path, &text);
                        vault.insert_text(id, &moved, placement)?;
                        (id, path)
                    }
                    Err(path) => {
                        let moved = vault.move_text(note, &path, &text);
                        (vault.create_note(&path, &moved)?, path)
                    }
                };
                let link = vault.link_to(note, id).ok_or(VaultError::UnknownNote(id))?;
                Ok((id, link, path))
            },
            move |this, result, window, cx| match result {
                Ok((_, link, path)) => {
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let replaced = selection.editor.upgrade().is_some_and(|editor| {
                        editor.update(cx, |editor, cx| {
                            replace_selection(
                                editor,
                                &selection.range,
                                &selection.text,
                                &link,
                                window,
                                cx,
                            )
                        })
                    });
                    if replaced {
                        this.compose_notice(
                            focus_kit::Tone::Ok,
                            Message::new("ui-selection-extracted").text_arg("destination", name),
                            cx,
                        );
                    } else {
                        this.compose_notice(
                            focus_kit::Tone::Danger,
                            Message::new("ui-extract-source-changed").text_arg("destination", name),
                            cx,
                        );
                    }
                }
                Err(error) => this.compose_failed(&error, cx),
            },
            window,
            cx,
        );
    }

    pub fn editor_menu_trigger(
        &self,
        editor: &Entity<NoteEditor>,
        cx: &Context<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static {
        let editor = editor.clone();
        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
            this.field_menu(&editor, event, window, cx)
        })
    }

    pub fn in_place_menu_trigger(
        &self,
        editor: &Entity<NoteEditor>,
        cx: &Context<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static {
        let editor = editor.clone();
        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
            this.in_place_menu(&editor, event, window, cx)
        })
    }

    pub fn field_menu(
        &mut self,
        editor: &Entity<NoteEditor>,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Right || !matches!(self.overlay, Overlay::None) {
            return;
        }
        let (view, block, note) = {
            let e = editor.read(cx);
            let block = e.live.as_ref().map(|b| b.input.clone());
            (e.view, block, e.input.clone())
        };
        let field = match (view, block) {
            (ViewMode::Source, _) => note,
            (ViewMode::Live, Some(block)) => block,
            _ => return,
        };
        let pressed = field.update(cx, |input, cx| {
            let at = input.character_index_for_point(event.position, window, cx)?;
            let selected = input.selected_text_range(true, window, cx)?.range;
            if !(selected.start <= at && at < selected.end) {
                let byte = input.text().offset_utf16_to_offset(at);
                input.set_selection(byte, byte, cx);
            }
            Some(())
        });
        if pressed.is_none() && view == ViewMode::Live {
            return;
        }
        cx.stop_propagation();
        let selected = editor
            .update(cx, |editor, cx| editor.compose_selection(window, cx))
            .is_some();
        self.show_editor_menu(event.position, Some(field), selected, window, cx);
    }

    pub fn in_place_menu(
        &mut self,
        editor: &Entity<NoteEditor>,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Right || !matches!(self.overlay, Overlay::None) {
            return;
        }
        let in_place = {
            let e = editor.read(cx);
            e.view == ViewMode::Live && e.live.is_none() && e.edit_in_place()
        };
        if !in_place
            || editor
                .update(cx, |editor, cx| editor.compose_selection(window, cx))
                .is_none()
        {
            return;
        }
        cx.stop_propagation();
        self.show_editor_menu(event.position, None, true, window, cx);
    }

    fn show_editor_menu(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        field: Option<Entity<InputState>>,
        selected: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drawn = field.is_some();
        let ai = self.prefs.read(cx).bool("ai.enabled");
        let menu = cx.new(|cx| EditorMenu::new(at, drawn, selected, ai, cx));
        self.show_compose(
            menu,
            move |this, event: &MenuEvent, window, cx| match event {
                MenuEvent::Close => this.close_overlay(window, cx),
                MenuEvent::Edit(action) => {
                    let action = action.boxed_clone();
                    this.overlay = Overlay::None;
                    if let Some(field) = &field {
                        field.update(cx, |input, cx| input.focus(window, cx));
                        window.dispatch_action(action, cx);
                    }
                    cx.notify();
                }
                MenuEvent::Extract => {
                    this.overlay = Overlay::None;
                    this.open_extract(window, cx);
                }
                MenuEvent::AskAi => {
                    this.overlay = Overlay::None;
                    window.dispatch_action(Box::new(crate::actions::AskAi), cx);
                    cx.notify();
                }
            },
            window,
            cx,
        );
    }

    pub fn compose_press(
        &mut self,
        target: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let views = self.vault_views.as_ref().ok_or("no vault open")?;
        let (tags, tree) = (views.tags.clone(), views.tree.clone());
        let done = match target {
            "menu:rename-tag" => tags.update(cx, |tags, cx| tags.script_rename(cx)),
            "menu:merge" => tree.update(cx, |tree, cx| tree.script_merge(cx)),
            "menu:extract" => {
                if !matches!(self.overlay, Overlay::Transfer { .. }) {
                    return Err("no editor menu open".into());
                }
                self.overlay = Overlay::None;
                self.open_extract(window, cx);
                true
            }
            "menu:ask-ai" => {
                if !matches!(self.overlay, Overlay::Transfer { .. }) {
                    return Err("no editor menu open".into());
                }
                if !self.prefs.read(cx).bool("ai.enabled") {
                    return Err("the AI functions are off: the menu has no Ask the AI".into());
                }
                self.overlay = Overlay::None;
                window.dispatch_action(Box::new(crate::actions::AskAi), cx);
                true
            }
            "editor" => {
                let editor = self.active_editor().cloned().ok_or("no active note")?;
                let in_place =
                    editor.read(cx).live.is_none() && editor.read(cx).view == ViewMode::Live;
                let at = if in_place {
                    let (range, _) = editor
                        .update(cx, |editor, cx| editor.compose_selection(window, cx))
                        .ok_or("no selection in place")?;
                    let reading = editor.read(cx).reading.clone();
                    let bounds = reading
                        .read(cx)
                        .bounds_for_byte(range.end.saturating_sub(1));
                    bounds
                        .map(|b| b.center())
                        .ok_or("the selection is not drawn")?
                } else {
                    let field = editor.read(cx).typing_input();
                    let bounds = field.read(cx).cursor_bounds();
                    bounds
                        .map(|b| b.center() - gpui::point(gpui::px(4.), gpui::px(0.)))
                        .ok_or("the caret is not drawn")?
                };
                let event = MouseDownEvent {
                    button: MouseButton::Right,
                    position: at,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                };
                if in_place {
                    self.in_place_menu(&editor, &event, window, cx);
                } else {
                    self.field_menu(&editor, &event, window, cx);
                }
                matches!(self.overlay, Overlay::Transfer { .. })
            }
            tag => match tag.strip_prefix("tag:") {
                Some(tag) => tags.update(cx, |tags, cx| tags.script_menu(tag, window, cx)),
                None => return Err(format!("unknown target: {target}")),
            },
        };
        done.then_some(())
            .ok_or_else(|| format!("{target}: not drawn, or disabled"))
    }
}

fn replace_selection(
    editor: &mut NoteEditor,
    range: &Range<usize>,
    text: &str,
    link: &str,
    window: &mut Window,
    cx: &mut Context<NoteEditor>,
) -> bool {
    editor.close_block(true, window, cx);
    editor.input.update(cx, |input, cx| {
        let rope = input.text().clone();
        let current = rope.to_string();
        let range = if current.get(range.clone()) == Some(text) {
            range.clone()
        } else {
            let mut found = current
                .match_indices(text)
                .map(|(at, _)| at..at + text.len());
            match (found.next(), found.next()) {
                (Some(only), None) => only,
                _ => return false,
            }
        };
        let utf16 =
            rope.offset_to_offset_utf16(range.start)..rope.offset_to_offset_utf16(range.end);
        input.replace_text_in_range(Some(utf16), link, window, cx);
        true
    })
}
