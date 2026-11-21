use std::path::PathBuf;
use std::sync::Arc;

use focus_core::{NoteId, TaskItem, Vault, VaultEvent};
use focus_kit::segments::{Segment, Segments};
use focus_kit::{ActiveTokens, Checkbox, Icon, Tokens, TreeRow, Type, size};
use gpui::{
    AnyElement, App, AppContext, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Task,
    UniformListScrollHandle, Window, div, prelude::FluentBuilder, px, uniform_list,
};

use std::ops::Range;

use focus_md::{BlockKind, Inline, InlineKind, ParseOptions};
use gpui::{FontStyle, FontWeight, HighlightStyle, StrikethroughStyle, UnderlineStyle};

use crate::groups::{Grouped, Row, Rows};
use crate::{Tone, VaultViewEvent, error_message};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaskFilter {
    #[default]
    Open,
    Done,
    All,
}

impl TaskFilter {
    pub fn keeps(self, task: &TaskItem) -> bool {
        match self {
            TaskFilter::Open => !task.done,
            TaskFilter::Done => task.done,
            TaskFilter::All => true,
        }
    }
}

pub type TaskIndex = Grouped<TaskItem>;

impl TaskIndex {
    pub fn load(vault: &Vault) -> TaskIndex {
        Grouped::from_items(
            vault,
            vault.tasks(true).into_iter().map(|task| (task.note, task)),
        )
    }
}

pub struct TaskList {
    vault: Arc<Vault>,
    index: TaskIndex,
    filter: TaskFilter,
    excluded: Vec<PathBuf>,
    rows: Rows,
    loading: Option<(Task<()>, bool)>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    selected: Option<usize>,
}

impl EventEmitter<VaultViewEvent> for TaskList {}

impl Focusable for TaskList {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TaskList {
    pub fn new(vault: Arc<Vault>, cx: &mut Context<Self>) -> Self {
        let mut list = TaskList {
            vault,
            index: TaskIndex::default(),
            filter: TaskFilter::default(),
            excluded: Vec::new(),
            rows: Rows::default(),
            loading: None,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            selected: None,
        };
        list.load(cx);
        list
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let vault = self.vault.clone();
        let task = cx.background_spawn(async move { TaskIndex::load(&vault) });
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

    pub fn set_filter(&mut self, filter: TaskFilter, cx: &mut Context<Self>) {
        self.filter = filter;
        self.rebuild();
        cx.notify();
    }

    pub fn set_excluded_folders(&mut self, folders: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.excluded = folders
            .into_iter()
            .filter(|f| !f.as_os_str().is_empty())
            .collect();
        self.rebuild();
        cx.notify();
    }

    pub fn vault_changed(&mut self, event: &VaultEvent, cx: &mut Context<Self>) {
        if let Some((_, stale)) = &mut self.loading {
            *stale = true;
            return;
        }
        let changed = match event {
            VaultEvent::NoteAdded(id)
            | VaultEvent::NoteChanged(id)
            | VaultEvent::NoteMoved { id, .. } => {
                let tasks = self.vault.tasks_of(*id);
                self.index.put(&self.vault, *id, tasks)
            }
            VaultEvent::NoteRemoved { id, .. } => self.index.remove(*id).into_iter().collect(),
            VaultEvent::TreeChanged => return,
        };
        self.refresh(&changed);
        cx.notify();
    }

    fn rebuild(&mut self) {
        let filter = self.filter;
        self.rows = self.index.rows(&self.excluded, |task| filter.keeps(task));
    }

    fn refresh(&mut self, changed: &[Arc<std::path::Path>]) {
        let filter = self.filter;
        self.index
            .refresh(&mut self.rows, changed, &self.excluded, |task| {
                filter.keeps(task)
            });
    }

    fn toggle(&mut self, note: NoteId, line: u32, cx: &mut Context<Self>) {
        match self.vault.toggle_task(note, line) {
            Ok(_) => {
                let tasks = self.vault.tasks_of(note);
                let changed = self.index.put(&self.vault, note, tasks);
                self.refresh(&changed);
            }
            Err(error) => cx.emit(VaultViewEvent::Notify {
                message: error_message(&error),
                tone: Tone::Danger,
            }),
        }
        cx.notify();
    }

    pub fn script_toggle(&mut self, nth: usize, cx: &mut Context<Self>) -> bool {
        let found = self.rows.items().nth(nth).map(|(g, i)| {
            let group = &self.index[self.rows.path(g)];
            (group.note, group.items[i].line)
        });
        match found {
            Some((note, line)) => {
                self.toggle(note, line, cx);
                true
            }
            None => false,
        }
    }

    fn render_row(&self, row: usize, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        match self.rows.get(row) {
            Row::Note(g) => {
                let group = &self.index[self.rows.path(g)];
                let note = group.note;
                let count = group.items.iter().filter(|t| self.filter.keeps(t)).count() as u32;
                TreeRow::new(("task-note", row), group.title.clone())
                    .icon(Icon::FILE_TEXT)
                    .focused(self.selected == Some(row))
                    .count(count)
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                        this.selected = Some(row);
                        cx.emit(VaultViewEvent::OpenNote {
                            note,
                            line: None,
                            new_tab: event.modifiers().control,
                        });
                    }))
                    .into_any_element()
            }
            Row::Item(g, i) => {
                let group = &self.index[self.rows.path(g)];
                let task = &group.items[i];
                let (note, line) = (group.note, task.line);
                focus_kit::Dir::current()
                    .row()
                    .id(("task", row))
                    .when(self.selected == Some(row), |this| this.bg(t.card_2))
                    .w_full()
                    .items_center()
                    .gap(size::S2)
                    .h(size::list_row())
                    .px(size::S2)
                    .map(|this| focus_kit::Dir::current().pad_start(this, px(30.)))
                    .hover(move |s| s.bg(t.card_2))
                    .child(
                        Checkbox::new(("task-box", row), task.done).on_click(
                            cx.listener(move |this, _, _, cx| this.toggle(note, line, cx)),
                        ),
                    )
                    .child(
                        div()
                            .id(("task-text", row))
                            .flex_1()
                            .min_w_0()
                            .cursor_pointer()
                            .ui()
                            .map(|this| focus_kit::Dir::current().flow(this))
                            .text_color(if task.done { t.ink_3 } else { t.ink_2 })
                            .when(task.done, |this| this.line_through())
                            .child(task_text(&task.text, t))
                            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                                this.selected = Some(row);
                                cx.emit(VaultViewEvent::OpenNote {
                                    note,
                                    line: Some(line),
                                    new_tab: event.modifiers().control,
                                });
                            })),
                    )
                    .child(
                        div()
                            .flex_none()
                            .mono_sm()
                            .text_color(t.ink_4)
                            .child((line + 1).to_string()),
                    )
                    .into_any_element()
            }
        }
    }
}

impl Render for TaskList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let selected = match self.filter {
            TaskFilter::Open => 0,
            TaskFilter::Done => 1,
            TaskFilter::All => 2,
        };
        let view = cx.entity().downgrade();
        let filters = Segments::new(
            "task-filter",
            vec![
                Segment::new(focus_i18n::t("vault-tasks-open")),
                Segment::new(focus_i18n::t("vault-tasks-done")),
                Segment::new(focus_i18n::t("vault-tasks-all")),
            ],
            selected,
        )
        .on_select(move |choice, _, cx| {
            let filter = [TaskFilter::Open, TaskFilter::Done, TaskFilter::All][choice.min(2)];
            let _ = view.update(cx, |this, cx| this.set_filter(filter, cx));
        });
        crate::keys::keyed(div().id("task-list"), cx)
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(t.bg)
            .child(
                focus_kit::Dir::current()
                    .row()
                    .flex_none()
                    .items_center()
                    .px(size::S2)
                    .py(size::S1)
                    .child(filters),
            )
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
                                .child(focus_i18n::t("vault-tasks-empty")),
                        )
                    })
                    .child(
                        uniform_list(
                            "task-rows",
                            self.rows.len(),
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                range.map(|row| this.render_row(row, &t, cx)).collect()
                            }),
                        )
                        .size_full()
                        .track_scroll(self.scroll.clone()),
                    )
                    .child(focus_kit::scrollbar(
                        "task-list-scrollbar",
                        self.scroll.clone(),
                    )),
            )
    }
}

impl crate::keys::Keyed for TaskList {
    fn selection(&mut self) -> (usize, &mut Option<usize>, &UniformListScrollHandle) {
        (self.rows.len(), &mut self.selected, &self.scroll)
    }

    fn activate(&mut self, row: usize, cx: &mut Context<Self>) {
        let target = match self.rows.get(row) {
            Row::Note(g) => (self.index[self.rows.path(g)].note, None),
            Row::Item(g, i) => {
                let group = &self.index[self.rows.path(g)];
                (group.note, Some(group.items[i].line))
            }
        };
        cx.emit(VaultViewEvent::OpenNote {
            note: target.0,
            line: target.1,
            new_tab: false,
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Link,
    Code,
    Strong,
    Emphasis,
    Strike,
}

fn task_runs(source: &str) -> (String, Vec<(Range<usize>, Mark)>) {
    let document = focus_md::parse(source, &ParseOptions { hard_breaks: false });
    let Some(inlines) = document.blocks.first().and_then(|block| match &block.kind {
        BlockKind::Paragraph(inlines) => Some(inlines),
        _ => None,
    }) else {
        return (source.to_string(), Vec::new());
    };
    let mut text = String::new();
    let mut marks = Vec::new();
    push_runs(inlines, None, &mut text, &mut marks);
    (text, marks)
}

fn push_runs(
    inlines: &[Inline],
    mark: Option<Mark>,
    text: &mut String,
    marks: &mut Vec<(Range<usize>, Mark)>,
) {
    fn push(
        piece: &str,
        mark: Option<Mark>,
        text: &mut String,
        marks: &mut Vec<(Range<usize>, Mark)>,
    ) {
        let start = text.len();
        text.push_str(piece);
        if let Some(mark) = mark.filter(|_| !piece.is_empty()) {
            marks.push((start..text.len(), mark));
        }
    }
    for inline in inlines {
        match &inline.kind {
            InlineKind::Text(piece) | InlineKind::Math(piece) => push(piece, mark, text, marks),
            InlineKind::Code(piece) => push(piece, Some(Mark::Code), text, marks),
            InlineKind::Tag(tag) => push(&format!("#{tag}"), mark, text, marks),
            InlineKind::Image(image) => push(&image.alt, mark, text, marks),
            InlineKind::SoftBreak | InlineKind::HardBreak => push(" ", mark, text, marks),
            InlineKind::Strong(children) => {
                push_runs(children, Some(mark.unwrap_or(Mark::Strong)), text, marks)
            }
            InlineKind::Emphasis(children) => {
                push_runs(children, Some(mark.unwrap_or(Mark::Emphasis)), text, marks)
            }
            InlineKind::Strike(children) => {
                push_runs(children, Some(mark.unwrap_or(Mark::Strike)), text, marks)
            }
            InlineKind::Highlight(children) => push_runs(children, mark, text, marks),
            InlineKind::Link(link) if link.children.is_empty() => {
                push(&link.target, Some(Mark::Link), text, marks)
            }
            InlineKind::Link(link) => push_runs(&link.children, Some(Mark::Link), text, marks),
            InlineKind::FootnoteRef(_) | InlineKind::BlockId(_) | InlineKind::Html(_) => {}
        }
    }
}

fn task_text(source: &str, t: &Tokens) -> focus_kit::ellipsis::Ellipsis {
    let (text, marks) = task_runs(source);
    focus_kit::ellipsis(text).highlights(marks.into_iter().map(|(range, mark)| {
        let style = match mark {
            Mark::Link => HighlightStyle {
                color: Some(t.ink),
                underline: Some(UnderlineStyle {
                    thickness: px(1.),
                    color: Some(t.line_2),
                    wavy: false,
                }),
                ..Default::default()
            },
            Mark::Code => HighlightStyle {
                background_color: Some(t.card_2),
                ..Default::default()
            },
            Mark::Strong => HighlightStyle {
                font_weight: Some(FontWeight::SEMIBOLD),
                ..Default::default()
            },
            Mark::Emphasis => HighlightStyle {
                font_style: Some(FontStyle::Italic),
                ..Default::default()
            },
            Mark::Strike => HighlightStyle {
                strikethrough: Some(StrikethroughStyle {
                    thickness: px(1.),
                    color: None,
                }),
                ..Default::default()
            },
        };
        (range, style)
    }))
}

#[cfg(test)]
mod tests {
    use super::{Mark, task_runs};

    #[test]
    fn a_task_reads_as_text_with_its_links_marked() {
        let (text, marks) = task_runs("Notes synced, see [[Note-taking for exams]]");
        assert_eq!(text, "Notes synced, see Note-taking for exams");
        assert_eq!(marks, vec![(18..39, Mark::Link)]);
        let (text, marks) = task_runs("Run `nmap` on **every** host");
        assert_eq!(text, "Run nmap on every host");
        assert_eq!(marks, vec![(4..8, Mark::Code), (12..17, Mark::Strong)]);
    }
}
