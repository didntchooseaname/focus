use std::path::{Path, PathBuf};
use std::sync::Arc;

use focus_core::{OutLink, Vault, VaultEvent};
use focus_kit::{ActiveTokens, Button, ButtonKind, Icon, Tokens, TreeRow, Type, size};
use gpui::{
    AnyElement, App, AppContext, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Task,
    UniformListScrollHandle, Window, div, prelude::FluentBuilder, px, uniform_list,
};

use crate::groups::{Grouped, Row, Rows};
use crate::{Message, Tone, VaultViewEvent, error_message};

pub type BrokenIndex = Grouped<OutLink>;

impl BrokenIndex {
    pub fn load(vault: &Vault) -> BrokenIndex {
        Grouped::from_items(vault, vault.all_unresolved())
    }
}

pub struct BrokenLinks {
    vault: Arc<Vault>,
    index: BrokenIndex,
    new_note_folder: String,
    excluded: Vec<PathBuf>,
    rows: Rows,
    canvases: usize,
    loading: Option<(Task<()>, bool)>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    selected: Option<usize>,
}

impl EventEmitter<VaultViewEvent> for BrokenLinks {}

impl Focusable for BrokenLinks {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BrokenLinks {
    pub fn new(vault: Arc<Vault>, new_note_folder: String, cx: &mut Context<Self>) -> Self {
        let mut list = BrokenLinks {
            vault,
            index: BrokenIndex::default(),
            new_note_folder,
            excluded: Vec::new(),
            rows: Rows::default(),
            canvases: 0,
            loading: None,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            selected: None,
        };
        list.load(cx);
        list
    }

    pub fn set_new_note_folder(&mut self, folder: String) {
        self.new_note_folder = folder;
    }

    pub fn set_excluded_folders(&mut self, folders: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.excluded = folders
            .into_iter()
            .filter(|f| !f.as_os_str().is_empty())
            .collect();
        self.rebuild();
        cx.notify();
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        if let Some((_, stale)) = &mut self.loading {
            *stale = true;
            return;
        }
        let vault = self.vault.clone();
        let task = cx.background_spawn(async move { BrokenIndex::load(&vault) });
        let task = cx.spawn(async move |this, cx| {
            let index = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.loading.take().is_some_and(|(_, stale)| stale) {
                    this.load(cx);
                    return;
                }
                this.index = index;
                this.rebuild();
                cx.notify();
            });
        });
        self.loading = Some((task, false));
    }

    pub fn vault_changed(&mut self, event: &VaultEvent, cx: &mut Context<Self>) {
        if self.loading.is_some() {
            self.load(cx);
            return;
        }
        let notes: Vec<_> = match event {
            VaultEvent::NoteChanged(id) => vec![*id],
            VaultEvent::NoteAdded(id) => std::iter::once(*id)
                .chain(self.vault.linked_from(*id))
                .collect(),
            VaultEvent::NoteRemoved { .. }
            | VaultEvent::NoteMoved { .. }
            | VaultEvent::TreeChanged => {
                self.load(cx);
                return;
            }
        };
        let mut changed = Vec::new();
        for note in notes {
            let links = self.vault.unresolved(note);
            changed.extend(self.index.put(&self.vault, note, links));
        }
        self.index
            .refresh(&mut self.rows, &changed, &self.excluded, |_| true);
        self.count_canvases();
        cx.notify();
    }

    fn rebuild(&mut self) {
        self.rows = self.index.rows(&self.excluded, |_| true);
        self.count_canvases();
    }

    fn count_canvases(&mut self) {
        self.canvases = (0..self.rows.groups())
            .filter(|group| self.index[self.rows.path(*group)].canvas)
            .count();
    }

    fn create(&mut self, target: String, cx: &mut Context<Self>) {
        let mut path = PathBuf::from(target.trim());
        if !target.contains('/') && !self.new_note_folder.trim_matches('/').is_empty() {
            path = Path::new(self.new_note_folder.trim_matches('/')).join(path);
        }
        match self.vault.create_note(&path, "") {
            Ok(note) => {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                cx.emit(VaultViewEvent::Notify {
                    message: Message::new("vault-created").text_arg("name", name),
                    tone: Tone::Ok,
                });
                cx.emit(VaultViewEvent::OpenNote {
                    note,
                    line: None,
                    new_tab: false,
                });
            }
            Err(error) => cx.emit(VaultViewEvent::Notify {
                message: error_message(&error),
                tone: Tone::Danger,
            }),
        }
        cx.notify();
    }

    fn opening(&self, group: usize, line: Option<u32>, new_tab: bool) -> VaultViewEvent {
        let path = self.rows.path(group);
        if self.index[path].canvas {
            VaultViewEvent::OpenFile {
                path: path.to_path_buf(),
                new_tab,
            }
        } else {
            VaultViewEvent::OpenNote {
                note: self.index[path].note,
                line,
                new_tab,
            }
        }
    }

    fn render_row(&self, row: usize, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        match self.rows.get(row) {
            Row::Note(g) => {
                let group = &self.index[self.rows.path(g)];
                let icon = if group.canvas {
                    Icon::FILE
                } else {
                    Icon::FILE_TEXT
                };
                TreeRow::new(("broken-note", row), group.title.clone())
                    .icon(icon)
                    .focused(self.selected == Some(row))
                    .count(group.items.len() as u32)
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                        this.selected = Some(row);
                        cx.emit(this.opening(g, None, event.modifiers().control));
                    }))
                    .into_any_element()
            }
            Row::Item(g, i) => {
                let group = &self.index[self.rows.path(g)];
                let link = &group.items[i];
                let line = link.line;
                let target = link.target.clone();
                let shown: SharedString = match &link.anchor {
                    Some(anchor) => format!("{}#{anchor}", link.target).into(),
                    None => link.target.clone().into(),
                };
                focus_kit::Dir::current()
                    .row()
                    .id(("broken", row))
                    .when(self.selected == Some(row), |this| this.bg(t.card_2))
                    .w_full()
                    .items_center()
                    .gap(size::S2)
                    .h(size::list_row())
                    .px(size::S2)
                    .map(|this| focus_kit::Dir::current().pad_start(this, px(30.)))
                    .hover(move |s| s.bg(t.card_2))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                        this.selected = Some(row);
                        cx.emit(this.opening(g, Some(line), event.modifiers().control));
                    }))
                    .child(
                        focus_kit::Dir::current()
                            .flow(div().flex_1().min_w_0().ui().text_color(t.ink_3))
                            .child(focus_kit::ellipsis(shown)),
                    )
                    .when(is_note_target(&target), |this| {
                        this.child(
                            div().flex_none().child(
                                Button::new(("create", row), focus_i18n::t("vault-broken-create"))
                                    .kind(ButtonKind::Ghost)
                                    .icon(Icon::FILE_PLUS)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.create(target.clone(), cx);
                                    })),
                            ),
                        )
                    })
                    .child(
                        focus_kit::Dir::current()
                            .align_end(div().flex_none().w(px(40.)))
                            .mono_sm()
                            .text_color(t.ink_4)
                            .child((line + 1).to_string()),
                    )
                    .into_any_element()
            }
        }
    }

    pub fn script_create(&mut self, nth: usize, cx: &mut Context<Self>) -> bool {
        let target = self
            .rows
            .items()
            .map(|(g, i)| self.index[self.rows.path(g)].items[i].target.clone())
            .filter(|t| is_note_target(t))
            .nth(nth);
        match target {
            Some(target) => {
                self.create(target, cx);
                true
            }
            None => false,
        }
    }
}

fn is_note_target(target: &str) -> bool {
    Path::new(target)
        .extension()
        .is_none_or(|e| e.eq_ignore_ascii_case("md"))
}

impl Render for BrokenLinks {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let links = self.rows.items().count();
        let total = if self.canvases == 0 {
            Message::new("vault-broken-total")
                .count_arg("links", links)
                .count_arg("notes", self.rows.groups())
        } else {
            Message::new("vault-broken-total-files")
                .count_arg("links", links)
                .count_arg("files", self.rows.groups())
        };
        crate::keys::keyed(div().id("broken-links"), cx)
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(t.bg)
            .when(links > 0, |this| {
                this.child(
                    focus_kit::Dir::current()
                        .row()
                        .flex_none()
                        .items_center()
                        .h(px(32.))
                        .px(size::S3)
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(total.text()),
                )
            })
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .when(self.rows.is_empty() && self.loading.is_none(), |this| {
                        this.child(
                            div()
                                .p(size::S4)
                                .ui_sm()
                                .text_color(t.ink_3)
                                .child(focus_i18n::t("vault-broken-empty")),
                        )
                    })
                    .child(
                        uniform_list(
                            "broken-rows",
                            self.rows.len(),
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                range.map(|row| this.render_row(row, &t, cx)).collect()
                            }),
                        )
                        .size_full()
                        .track_scroll(self.scroll.clone()),
                    )
                    .child(focus_kit::scrollbar(
                        "broken-scrollbar",
                        self.scroll.clone(),
                    )),
            )
    }
}

impl crate::keys::Keyed for BrokenLinks {
    fn selection(&mut self) -> (usize, &mut Option<usize>, &UniformListScrollHandle) {
        (self.rows.len(), &mut self.selected, &self.scroll)
    }

    fn activate(&mut self, row: usize, cx: &mut Context<Self>) {
        let (group, line) = match self.rows.get(row) {
            Row::Note(g) => (g, None),
            Row::Item(g, i) => (g, Some(self.index[self.rows.path(g)].items[i].line)),
        };
        cx.emit(self.opening(group, line, false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focus_core::OpenOptions;

    #[test]
    fn a_canvas_with_a_card_of_a_missing_file_is_listed() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/focus-view-vault-tests/broken");
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("vault");
        std::fs::create_dir_all(root.join("Boards")).unwrap();
        std::fs::write(root.join("Here.md"), "# Here\n\n[[Nowhere]]\n").unwrap();
        std::fs::write(
            root.join("Boards/Plan.canvas"),
            r#"{"nodes":[
{"id":"a","type":"file","file":"Here.md","x":0,"y":0,"width":200,"height":100},
{"id":"b","type":"file","file":"Gone.md","x":0,"y":200,"width":200,"height":100}
],"edges":[]}"#,
        )
        .unwrap();
        let vault = Vault::open(
            &root,
            OpenOptions {
                cache_dir: dir.join("cache"),
                ignore: Vec::new(),
                watch: false,
            },
        )
        .unwrap();
        let index = BrokenIndex::load(&vault);
        let rows = index.rows(&[], |_| true);
        let listed: Vec<(String, Vec<String>)> = (0..rows.groups())
            .map(|g| {
                let group = &index[rows.path(g)];
                let targets = group.items.iter().map(|l| l.target.clone()).collect();
                (group.title.clone(), targets)
            })
            .collect();
        assert_eq!(
            listed,
            [
                ("Plan.canvas".to_string(), vec!["Gone.md".to_string()]),
                ("Here".to_string(), vec!["Nowhere".to_string()]),
            ]
        );
        let canvas = vault
            .canvas_by_path(Path::new("Boards/Plan.canvas"))
            .unwrap();
        assert_eq!(index[rows.path(0)].note, canvas);
        assert!(index[rows.path(0)].canvas && !index[rows.path(1)].canvas);
        assert!(vault.note(canvas).is_none());
    }
}
