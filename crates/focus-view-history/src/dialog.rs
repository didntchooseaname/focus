use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use focus_core::{NoteId, Vault, VaultError};
use focus_history::{CopyOrigin, Kind, Version};
use focus_i18n::{Arg, t as tr, t_args};
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Chip, DiffMark, ErrorCard, Flow as _, Icon, IconButton,
    Segment, Segments, Tokens, Tone, Type, diff_marker, diff_row, ellipsis, floating, size,
};
use focus_view_reading::{ReadingView, Typesetting};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, EventEmitter, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Render, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Styled, Task, UniformListScrollHandle, Window, div,
    prelude::FluentBuilder, px, relative, uniform_list,
};

use crate::model::{self, Comparison, ConflictCopy, Error, Mark, Row, Snapshot, Source};

const WIDTH: f32 = 1040.;
const LIST_WIDTH: f32 = 288.;
const VERSION_ROW: f32 = 44.;
const DIFF_ROW: f32 = 20.;
const MEASURE: f32 = 640.;

#[derive(Debug, Clone, PartialEq)]
pub enum VersionsEvent {
    Close,
    Restored { path: PathBuf },
    Notify { tone: Tone, text: String },
    DeleteCopy { path: PathBuf },
}

enum Entry<'a> {
    Copy(&'a ConflictCopy),
    Version(&'a Version),
}

impl Entry<'_> {
    fn source(&self) -> Source {
        match self {
            Entry::Copy(copy) => Source::Copy(copy.note),
            Entry::Version(version) => Source::Version(version.id.clone()),
        }
    }

    fn seconds(&self) -> i64 {
        let at = match self {
            Entry::Copy(copy) => copy.modified,
            Entry::Version(version) => version.at,
        };
        seconds(at)
    }

    fn label(&self) -> String {
        match self {
            Entry::Copy(copy) => copy_label(copy),
            Entry::Version(version) => kind_label(version.kind),
        }
    }

    fn bytes(&self) -> u64 {
        match self {
            Entry::Copy(copy) => copy.bytes,
            Entry::Version(version) => version.bytes,
        }
    }
}

fn seconds(at: SystemTime) -> i64 {
    at.duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Changes,
    Rendered,
}

enum Stage {
    Loading,
    Ready,
    Failed(Error),
}

pub struct VersionsDialog {
    vault: Arc<Vault>,
    note: NoteId,
    stage: Stage,
    snapshot: Option<Snapshot>,
    selected: usize,
    comparison: Option<Comparison>,
    problem: Option<SharedString>,
    refusal: Option<SharedString>,
    tab: Tab,
    reading: Entity<ReadingView>,
    rendered: Option<Source>,
    restoring: bool,
    focus: FocusHandle,
    list_focus: FocusHandle,
    list_scroll: UniformListScrollHandle,
    visible: std::ops::Range<usize>,
    diff_scroll: UniformListScrollHandle,
    snapshot_task: Option<Task<()>>,
    compare_task: Option<Task<()>>,
    restore_task: Option<Task<()>>,
}

impl EventEmitter<VersionsEvent> for VersionsDialog {}

impl VersionsDialog {
    pub fn new(
        vault: Arc<Vault>,
        note: NoteId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let reading = cx.new(|cx| ReadingView::new(Typesetting::compact(MEASURE), cx));
        let mut dialog = VersionsDialog {
            vault,
            note,
            stage: Stage::Loading,
            snapshot: None,
            selected: 0,
            comparison: None,
            problem: None,
            refusal: None,
            tab: Tab::Changes,
            reading,
            rendered: None,
            restoring: false,
            focus: cx.focus_handle(),
            list_focus: cx.focus_handle().tab_stop(true),
            list_scroll: UniformListScrollHandle::new(),
            visible: 0..0,
            diff_scroll: UniformListScrollHandle::new(),
            snapshot_task: None,
            compare_task: None,
            restore_task: None,
        };
        window.focus(&dialog.list_focus);
        dialog.reload(cx);
        dialog
    }

    pub fn note(&self) -> NoteId {
        self.note
    }

    pub fn busy(&self) -> bool {
        self.snapshot_task.is_some() || self.compare_task.is_some() || self.restore_task.is_some()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_rendered(&mut self, rendered: bool, cx: &mut Context<Self>) {
        self.set_tab(
            if rendered {
                Tab::Rendered
            } else {
                Tab::Changes
            },
            cx,
        );
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let wanted = self.selected_entry().map(|e| e.source());
        let vault = self.vault.clone();
        let note = self.note;
        let task = cx.background_spawn(async move { model::snapshot(&vault, note) });
        self.snapshot_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.snapshot_task = None;
                match result {
                    Ok(snapshot) => {
                        this.snapshot = Some(snapshot);
                        let index = wanted
                            .and_then(|wanted| {
                                (0..this.entry_count()).position(|i| {
                                    this.entry(i).is_some_and(|e| e.source() == wanted)
                                })
                            })
                            .unwrap_or(0);
                        let empty = this.entry_count() == 0;
                        this.stage = Stage::Ready;
                        this.comparison = None;
                        if !empty {
                            this.select(index, cx);
                        }
                        this.problem = this.refusal.take();
                    }
                    Err(error) => this.stage = Stage::Failed(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn versions(&self) -> &[Version] {
        self.snapshot.as_ref().map_or(&[], |s| &s.versions)
    }

    fn copies(&self) -> &[ConflictCopy] {
        self.snapshot.as_ref().map_or(&[], |s| &s.copies)
    }

    fn entry_count(&self) -> usize {
        self.copies().len() + self.versions().len()
    }

    fn entry(&self, index: usize) -> Option<Entry<'_>> {
        let copies = self.copies();
        match copies.get(index) {
            Some(copy) => Some(Entry::Copy(copy)),
            None => self
                .versions()
                .get(index - copies.len())
                .map(Entry::Version),
        }
    }

    fn selected_entry(&self) -> Option<Entry<'_>> {
        self.entry(self.selected)
    }

    pub fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(source) = self.entry(index).map(|e| e.source()) else {
            return;
        };
        let Some(current) = self.snapshot.as_ref().map(|s| s.current.clone()) else {
            return;
        };
        self.selected = index;
        self.comparison = None;
        self.problem = None;
        self.diff_scroll.scroll_to_item(0, ScrollStrategy::Top);
        let vault = self.vault.clone();
        let note = self.note;
        let wanted = source.clone();
        let task = cx.background_spawn(async move {
            match &source {
                Source::Version(id) => model::compare(&vault, note, id, &current),
                Source::Copy(copy) => model::compare_copy(&vault, *copy, &current),
            }
        });
        self.compare_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.compare_task = None;
                if this.selected_entry().map(|e| e.source()) != Some(wanted) {
                    return;
                }
                match result {
                    Ok(comparison) => {
                        this.comparison = Some(comparison);
                        this.show_rendered(cx);
                    }
                    Err(error) => this.problem = Some(error_text(&error).into()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn move_selection(&mut self, by: isize, cx: &mut Context<Self>) {
        let count = self.entry_count();
        if count == 0 {
            return;
        }
        let next = (self.selected as isize + by).clamp(0, count as isize - 1) as usize;
        if next != self.selected {
            if next < self.visible.start {
                self.list_scroll.scroll_to_item(next, ScrollStrategy::Top);
            } else if next + 1 >= self.visible.end {
                self.list_scroll
                    .scroll_to_item(next, ScrollStrategy::Bottom);
            }
            self.select(next, cx);
        }
    }

    fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.show_rendered(cx);
        cx.notify();
    }

    fn show_rendered(&mut self, cx: &mut Context<Self>) {
        let Some(comparison) = self
            .comparison
            .as_ref()
            .filter(|_| self.tab == Tab::Rendered)
        else {
            return;
        };
        if self.rendered.as_ref() == Some(&comparison.source) {
            return;
        }
        self.rendered = Some(comparison.source.clone());
        let text = comparison.text.clone();
        let meta = self.vault.note(self.note);
        let resolver: Arc<dyn focus_view_reading::Resolver> = self.vault.clone();
        self.reading
            .update(cx, |view, cx| view.set_markdown(text, meta, resolver, cx));
    }

    pub fn copy(&mut self, cx: &mut Context<Self>) {
        let Some(comparison) = &self.comparison else {
            return;
        };
        focus_kit::clipboard::copy_text(cx, comparison.text.clone());
        let text = match comparison.source {
            Source::Version(_) => tr("history-copied"),
            Source::Copy(_) => tr("history-copy-text-copied"),
        };
        cx.emit(VersionsEvent::Notify {
            tone: Tone::Ok,
            text,
        });
    }

    pub fn delete_copy(&mut self, cx: &mut Context<Self>) {
        let Some(Entry::Copy(copy)) = self.selected_entry() else {
            return;
        };
        if self.restoring {
            return;
        }
        let path = copy.path.to_path_buf();
        cx.emit(VersionsEvent::DeleteCopy { path });
    }

    pub fn restore(&mut self, cx: &mut Context<Self>) {
        let (Some(source), Some(snapshot)) =
            (self.selected_entry().map(|e| e.source()), &self.snapshot)
        else {
            return;
        };
        if self.restoring || self.comparison.as_ref().is_none_or(Comparison::same) {
            return;
        }
        let expected = snapshot.modified;
        let vault = self.vault.clone();
        let note = self.note;
        self.restoring = true;
        self.problem = None;
        let done = match source {
            Source::Version(_) => tr("history-restored"),
            Source::Copy(_) => tr("history-copy-restored"),
        };
        let task = cx.background_spawn(async move {
            match &source {
                Source::Version(id) => model::restore(&vault, note, id, expected),
                Source::Copy(copy) => model::restore_copy(&vault, note, *copy, expected),
            }
        });
        self.restore_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.restoring = false;
                this.restore_task = None;
                match result {
                    Ok(_) => {
                        if let Some(meta) = this.vault.note(this.note) {
                            cx.emit(VersionsEvent::Restored {
                                path: meta.path.to_path_buf(),
                            });
                        }
                        cx.emit(VersionsEvent::Notify {
                            tone: Tone::Ok,
                            text: done,
                        });
                    }
                    Err(error) => this.refusal = Some(error_text(&error).into()),
                }
                this.reload(cx);
            });
        }));
        cx.notify();
    }

    fn on_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.modified() {
            return;
        }
        match keystroke.key.as_str() {
            "escape" => cx.emit(VersionsEvent::Close),
            "enter" => self.restore(cx),
            "up" => self.move_selection(-1, cx),
            "down" => self.move_selection(1, cx),
            "home" => self.move_selection(isize::MIN / 2, cx),
            "end" => self.move_selection(isize::MAX / 2, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn version_row(&self, index: usize, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(entry) = self.entry(index) else {
            return div().into_any_element();
        };
        let selected = index == self.selected;
        let is_copy = matches!(entry, Entry::Copy(_));
        div()
            .id(("history-version", index))
            .relative()
            .flex()
            .flex_col()
            .justify_center()
            .w_full()
            .h(px(VERSION_ROW))
            .px(size::S3)
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(t.card_3).child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .bg(t.ink)
                        .start_at(px(0.)),
                )
            })
            .when(!selected, |this| {
                this.hover(|s| s.bg(t.card_2)).active(|s| s.bg(t.card_3))
            })
            .on_click(cx.listener(move |this, _, _, cx| this.select(index, cx)))
            .child(
                div()
                    .flex()
                    .ui()
                    .text_color(if selected { t.ink } else { t.ink_2 })
                    .child(ellipsis(focus_i18n::format_datetime(entry.seconds()))),
            )
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .gap(size::S2)
                    .when(is_copy, |this| {
                        this.child(div().flex_none().size(px(6.)).rounded_full().bg(t.warn))
                    })
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .ui_xs()
                            .text_color(t.ink_3)
                            .child(ellipsis(entry.label())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .mono_sm()
                            .text_color(t.ink_3)
                            .child(focus_i18n::format_bytes(entry.bytes())),
                    ),
            )
            .into_any_element()
    }

    fn diff_row(&self, comparison: &Comparison, index: usize, t: &Tokens) -> AnyElement {
        let row = div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(DIFF_ROW))
            .w_full();
        match &comparison.rows[index] {
            Row::Fold(count) => row
                .flow_row()
                .px(size::S3)
                .ui_xs()
                .text_color(t.ink_3)
                .bg(t.card_2)
                .child(t_args(
                    "history-folded",
                    &[("count", Arg::Int(*count as i64))],
                ))
                .into_any_element(),
            Row::Line { mark, number, text } => {
                let mark = match mark {
                    Mark::Added => DiffMark::Added,
                    Mark::Removed => DiffMark::Removed,
                    Mark::Same => DiffMark::Same,
                };
                row.relative()
                    .child(diff_row(t, mark).absolute().inset_0())
                    .mono_sm()
                    .child(
                        div()
                            .flex_none()
                            .w(px(48.))
                            .pr(size::S2)
                            .text_right()
                            .text_color(t.ink_3)
                            .child(number.to_string()),
                    )
                    .child(diff_marker(t, mark).w(px(12.)))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .pr(size::S3)
                            .text_color(t.ink_body)
                            .child(ellipsis(text.clone())),
                    )
                    .into_any_element()
            }
        }
    }

    fn changes(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(comparison) = &self.comparison else {
            return centered(t.ink_3, tr("history-loading")).into_any_element();
        };
        if comparison.same() {
            let same = match comparison.source {
                Source::Version(_) => "history-same",
                Source::Copy(_) => "history-copy-same",
            };
            return centered(t.ink_3, tr(same)).into_any_element();
        }
        let count = comparison.rows.len();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .bg(t.bg_2)
            .child(
                uniform_list(
                    "history-diff",
                    count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _, _| {
                        let Some(comparison) = &this.comparison else {
                            return Vec::new();
                        };
                        range.map(|i| this.diff_row(comparison, i, &t)).collect()
                    }),
                )
                .size_full()
                .py(size::S1)
                .track_scroll(self.diff_scroll.clone()),
            )
            .child(focus_kit::scrollbar(
                "history-diff-scrollbar",
                self.diff_scroll.clone(),
            ))
            .into_any_element()
    }

    fn rendered(&self, t: Tokens) -> AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .bg(t.bg)
            .px(size::S4)
            .child(self.reading.clone())
            .into_any_element()
    }

    fn preview(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(entry) = self.selected_entry() else {
            return div().into_any_element();
        };
        let detail = match &entry {
            Entry::Copy(copy) => div()
                .flex()
                .mono_sm()
                .text_color(t.ink_3)
                .child(ellipsis(copy.path.display().to_string())),
            Entry::Version(version) => div()
                .flex()
                .ui_xs()
                .text_color(t.ink_3)
                .child(ellipsis(kind_label(version.kind))),
        };
        let view = cx.entity();
        let tabs = Segments::new(
            "history-tabs",
            vec![
                Segment::new(tr("history-tab-changes")),
                Segment::new(tr("history-tab-rendered")),
            ],
            usize::from(self.tab == Tab::Rendered),
        )
        .on_select(move |index, _, cx| {
            view.update(cx, |this, cx| {
                this.set_tab(
                    if index == 0 {
                        Tab::Changes
                    } else {
                        Tab::Rendered
                    },
                    cx,
                )
            })
        });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .flow_row()
                    .flex_none()
                    .items_center()
                    .gap(size::S3)
                    .h(px(56.))
                    .px(size::S4)
                    .border_b_1()
                    .border_color(t.line)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .ui_strong()
                                    .text_color(t.ink)
                                    .child(ellipsis(focus_i18n::format_datetime(entry.seconds()))),
                            )
                            .child(detail),
                    )
                    .child(tabs),
            )
            .when_some(self.problem.clone(), |this, problem| {
                this.child(
                    div()
                        .flex_none()
                        .p(size::S3)
                        .border_b_1()
                        .border_color(t.line)
                        .child(ErrorCard::new(problem)),
                )
            })
            .child(match self.tab {
                Tab::Changes => self.changes(t, cx),
                Tab::Rendered => self.rendered(t),
            })
            .into_any_element()
    }

    fn footer(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let comparison = self.comparison.as_ref();
        let can_act = comparison.is_some() && !self.restoring;
        let can_restore = can_act && comparison.is_some_and(|c| !c.same());
        let on_copy = matches!(self.selected_entry(), Some(Entry::Copy(_)));
        div()
            .flow_row()
            .flex_none()
            .items_center()
            .gap(size::S2)
            .h(px(56.))
            .px(size::S4)
            .border_t_1()
            .border_color(t.line)
            .child(div().flow_row().flex_1().min_w_0().gap(size::S2).when_some(
                comparison.filter(|c| !c.same()),
                |this, c| {
                    this.child(
                        Chip::new()
                            .value(focus_i18n::format_number(c.added as i64))
                            .key(tr("history-count-added")),
                    )
                    .child(
                        Chip::new()
                            .value(focus_i18n::format_number(c.removed as i64))
                            .key(tr("history-count-removed")),
                    )
                },
            ))
            .child(
                Button::new("history-copy", tr("history-copy"))
                    .icon(Icon::COPY)
                    .disabled(!can_act)
                    .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
            )
            .when(on_copy, |this| {
                this.child(
                    Button::new("history-delete-copy", tr("history-delete-copy"))
                        .kind(ButtonKind::Danger)
                        .icon(Icon::TRASH)
                        .disabled(self.restoring)
                        .on_click(cx.listener(|this, _, _, cx| this.delete_copy(cx))),
                )
            })
            .child(
                Button::new("history-restore", tr("history-restore"))
                    .kind(ButtonKind::Primary)
                    .icon(Icon::RESET)
                    .disabled(!can_restore)
                    .on_click(cx.listener(|this, _, _, cx| this.restore(cx))),
            )
            .into_any_element()
    }

    fn list(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        focus_kit::keyboard::name_focus(&self.list_focus, "history-versions", cx);
        div()
            .track_focus(&self.list_focus)
            .relative()
            .flex_none()
            .w(px(LIST_WIDTH))
            .h_full()
            .border_end_1()
            .border_color(t.line)
            .child(
                uniform_list(
                    "history-versions",
                    self.entry_count(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        this.visible = range.clone();
                        range.map(|i| this.version_row(i, &t, cx)).collect()
                    }),
                )
                .size_full()
                .track_scroll(self.list_scroll.clone()),
            )
            .child(focus_kit::scrollbar(
                "history-versions-scrollbar",
                self.list_scroll.clone(),
            ))
            .into_any_element()
    }

    fn body(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        match &self.stage {
            Stage::Loading => centered(t.ink_3, tr("history-loading")).into_any_element(),
            Stage::Failed(error) => div()
                .flex_1()
                .p(size::S4)
                .child(ErrorCard::new(error_text(error)))
                .into_any_element(),
            Stage::Ready if self.entry_count() == 0 => {
                let keeping = self.snapshot.as_ref().is_none_or(|s| s.keeping);
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap(size::S2)
                    .p(size::S6)
                    .child(focus_kit::icon(Icon::HISTORY, size::ICON).text_color(t.ink_3))
                    .child(div().ui().text_color(t.ink_2).child(tr("history-empty")))
                    .child(
                        div()
                            .ui_sm()
                            .text_color(t.ink_3)
                            .max_w(px(480.))
                            .text_center()
                            .child(tr(if keeping {
                                "history-empty-hint"
                            } else {
                                "history-off-hint"
                            })),
                    )
                    .into_any_element()
            }
            Stage::Ready => div()
                .flow_row()
                .flex_1()
                .min_h_0()
                .child(self.list(t, cx))
                .child(self.preview(t, cx))
                .into_any_element(),
        }
    }
}

fn centered(color: gpui::Hsla, text: impl Into<SharedString>) -> gpui::Div {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .ui()
        .text_color(color)
        .child(text.into())
}

fn kind_label(kind: Kind) -> String {
    tr(match kind {
        Kind::Edit => "history-kind-edit",
        Kind::Bulk => "history-kind-bulk",
        Kind::Restore => "history-kind-restore",
        Kind::External => "history-kind-external",
        Kind::Unsaved => "history-kind-unsaved",
        Kind::Reload => "history-kind-reload",
    })
}

fn copy_label(copy: &ConflictCopy) -> String {
    let (key, device) = match (copy.origin, &copy.device) {
        (CopyOrigin::Syncthing, Some(device)) => ("history-copy-syncthing-device", device),
        (CopyOrigin::ObsidianSync, Some(device)) => ("history-copy-obsidian-device", device),
        (CopyOrigin::Syncthing, None) => return tr("history-copy-syncthing"),
        (CopyOrigin::ObsidianSync, None) => return tr("history-copy-obsidian"),
    };
    t_args(key, &[("device", Arg::Str(device))])
}

pub fn error_text(error: &Error) -> String {
    match error {
        Error::Unavailable => tr("history-error-unavailable"),
        Error::UnknownNote => tr("history-error-unknown-note"),
        Error::History(focus_history::Error::UnknownVersion(_)) => tr("history-error-gone"),
        Error::History(focus_history::Error::Damaged(_)) => tr("history-error-damaged"),
        Error::History(focus_history::Error::Io { path, .. })
        | Error::Vault(VaultError::Io { path, .. })
        | Error::Io { path, .. } => t_args(
            "history-error-io",
            &[("path", Arg::Str(&path.display().to_string()))],
        ),
        Error::Vault(VaultError::Conflict(_)) => tr("history-error-conflict"),
        Error::Vault(other) => t_args(
            "history-error-other",
            &[("detail", Arg::Str(&other.to_string()))],
        ),
    }
}

impl Render for VersionsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let title = self
            .vault
            .note(self.note)
            .map(|meta| meta.title.to_string())
            .unwrap_or_default();
        let ready = matches!(self.stage, Stage::Ready) && self.entry_count() > 0;
        let card = floating(&t)
            .flex()
            .flex_col()
            .w(px(WIDTH))
            .max_w(relative(0.94))
            .h(relative(0.82))
            .child(
                div()
                    .flow_row()
                    .flex_none()
                    .items_center()
                    .gap(size::S2)
                    .h(px(48.))
                    .px(size::S4)
                    .border_b_1()
                    .border_color(t.line)
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .display_sm()
                            .text_color(t.ink)
                            .child(ellipsis(t_args(
                                "history-title",
                                &[("note", Arg::Str(&title))],
                            ))),
                    )
                    .child(
                        IconButton::new("history-close", Icon::CLOSE, tr("history-close"))
                            .shortcut(focus_i18n::keys_text("escape"))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(VersionsEvent::Close))),
                    ),
            )
            .child(self.body(t, cx))
            .when(ready, |this| this.child(self.footer(t, cx)));
        focus_kit::fade_in(
            "fade-history",
            div()
                .id("history-dialog")
                .track_focus(&self.focus)
                .on_key_down(
                    cx.listener(|this, event: &KeyDownEvent, _, cx| this.on_key(event, cx)),
                )
                .absolute()
                .inset_0()
                .flex()
                .items_start()
                .justify_center()
                .pt(relative(0.06))
                .bg(t.veil)
                .child(card),
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_the_dialog_cites_exists_in_the_four_languages() {
        let source = include_str!("dialog.rs");
        let mut cited = 0;
        for piece in source.split('"') {
            let key = piece.starts_with("history-")
                && !piece.ends_with('-')
                && piece
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !key || piece.matches('-').count() < 1 {
                continue;
            }
            let element = matches!(
                piece,
                "history-dialog"
                    | "history-version"
                    | "history-diff"
                    | "history-diff-scrollbar"
                    | "history-versions"
                    | "history-versions-scrollbar"
                    | "history-tabs"
            );
            if element {
                continue;
            }
            cited += 1;
            for code in ["en", "fr", "ar", "zh-CN"] {
                assert!(
                    focus_i18n::audit::embedded(code)
                        .messages
                        .contains_key(piece),
                    "{code} lacks {piece}"
                );
            }
        }
        assert!(cited >= 20, "only {cited} keys found");
    }

    #[test]
    fn every_failure_has_a_translated_sentence() {
        focus_i18n::init(None);
        focus_i18n::set_language("en").unwrap();
        let path = PathBuf::from("a/b.md");
        let io = || std::io::Error::other("x");
        let errors = [
            Error::Unavailable,
            Error::UnknownNote,
            Error::History(focus_history::Error::UnknownVersion("v".into())),
            Error::History(focus_history::Error::Damaged(path.clone())),
            Error::History(focus_history::Error::Io {
                path: path.clone(),
                source: io(),
            }),
            Error::Vault(VaultError::Conflict(path.clone())),
            Error::Vault(VaultError::Other("no".into())),
            Error::Io {
                path: path.clone(),
                source: io(),
            },
        ];
        for error in errors {
            let text = error_text(&error);
            assert!(!text.starts_with("history-"), "{error}: {text}");
        }
    }
}
