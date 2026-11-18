use std::sync::Arc;

use focus_core::VaultEvent;
use focus_kit::{Button, ButtonKind, Flow as _, Tokens, Type, ellipsis, size};
use focus_search::{CancelToken, SearchOptions};
use gpui::{AppContext as _, Context, Div, ParentElement, Styled, Window, div, px};

use super::{Arg, SearchEvent, SearchPanel, t_args, tr};
use crate::replace::{
    self, Exclusions, Failure as ReplaceFailure, Matcher, PatternError, Plan, Report,
};
use crate::rows::Row;

#[derive(Default)]
pub(super) struct ReplaceState {
    pub open: bool,
    pub plan: Option<Arc<Plan>>,
    pub pattern_error: Option<PatternError>,
    pub exclusions: Exclusions,
    pub building: bool,
    pub generation: u64,
    pub cancel: Option<CancelToken>,
    pub report: Option<Report>,
    pub applying: bool,
}

impl SearchPanel {
    pub fn set_replace_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.replace.open = open;
        self.selected = None;
        if open {
            self.replacement
                .update(cx, |input, cx| input.focus(window, cx));
            self.run_plan(window, cx);
        } else {
            if let Some(token) = self.replace.cancel.take() {
                token.cancel();
            }
            self.replace = ReplaceState::default();
            self.rebuild_rows(cx);
            self.focus_field(window, cx);
        }
        cx.notify();
    }

    pub fn set_replacement(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.replacement.update(cx, |input, cx| {
            input.set_value(text.to_string(), window, cx)
        });
    }

    pub fn replace_selection(&self) -> (usize, usize) {
        match &self.replace.plan {
            Some(plan) => (
                self.replace.exclusions.kept_lines(plan),
                self.replace.exclusions.kept_notes(plan),
            ),
            None => (0, 0),
        }
    }

    pub(super) fn run_plan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(previous) = self.replace.cancel.take() {
            previous.cancel();
        }
        self.replace.generation += 1;
        let generation = self.replace.generation;
        self.replace.plan = None;
        self.replace.pattern_error = None;
        self.replace.building = false;
        let text = self.input.read(cx).value().to_string();
        let replacement = self.replacement.read(cx).value().to_string();
        let services = self.services.clone();
        let matcher = if text.trim().is_empty() {
            None
        } else {
            Matcher::from_query(&text, &self.options)
                .map_err(|e| self.replace.pattern_error = Some(e))
                .ok()
        };
        let (Some(matcher), Some(services)) = (matcher, services) else {
            self.rebuild_rows(cx);
            cx.notify();
            return;
        };
        let token = CancelToken::new();
        self.replace.cancel = Some(token.clone());
        self.replace.building = true;
        let query = focus_search::parse_query(&text);
        let options = SearchOptions {
            max_files: 100_000,
            max_matches_per_file: 1,
            ..self.options.clone()
        };
        cx.spawn_in(window, async move |this, cx| {
            let plan = cx
                .background_spawn(async move {
                    let found = services.index.search(&query, &options, &token);
                    let notes: Vec<_> = found.hits.iter().map(|h| h.note).collect();
                    replace::build(&services.vault, &notes, &matcher, &replacement, &token)
                })
                .await;
            this.update_in(cx, |this, _, cx| {
                if generation != this.replace.generation || plan.cancelled {
                    return;
                }
                this.replace.building = false;
                this.replace.plan = Some(Arc::new(plan));
                this.selected = None;
                this.rebuild_rows(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        self.rebuild_rows(cx);
        cx.notify();
    }

    pub(super) fn toggle_keep(&mut self, row: usize, cx: &mut Context<Self>) {
        let ex = &mut self.replace.exclusions;
        match self.rows.get(row) {
            Some(Row::Header {
                note,
                keep: Some(_),
                ..
            }) => {
                if !ex.notes.remove(note) {
                    ex.notes.insert(*note);
                }
            }
            Some(Row::Before { note, line, .. } | Row::After { note, line, .. }) => {
                if !ex.lines.remove(&(*note, *line)) {
                    ex.lines.insert((*note, *line));
                }
            }
            _ => return,
        }
        self.rebuild_rows(cx);
        cx.notify();
    }

    pub fn apply_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(plan), Some(services)) = (self.replace.plan.clone(), self.services.clone())
        else {
            return;
        };
        if self.replace.applying || self.replace_selection().0 == 0 {
            return;
        }
        self.replace.applying = true;
        let exclusions = self.replace.exclusions.clone();
        cx.spawn_in(window, async move |this, cx| {
            let vault = services.vault.clone();
            let report = cx
                .background_spawn(async move { replace::apply(&vault, &plan, &exclusions) })
                .await;
            this.update_in(cx, |this, window, cx| {
                let written: Vec<_> = report.written.iter().map(|(note, _)| *note).collect();
                if let Some(services) = &this.services {
                    written
                        .iter()
                        .for_each(|&note| services.index.apply(&VaultEvent::NoteChanged(note)));
                }
                this.replace.applying = false;
                this.replace.exclusions = Exclusions::default();
                this.replace.report = Some(report);
                cx.emit(SearchEvent::NotesWritten(written));
                this.run_search(window, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn replace_summary(&self) -> Option<String> {
        let plan = self.replace.plan.as_ref()?;
        Some(t_args(
            "search-replace-summary",
            &[
                ("lines", Arg::Int(plan.line_count() as i64)),
                ("files", Arg::Int(plan.notes.len() as i64)),
            ],
        ))
    }

    pub(super) fn replace_error(&self) -> Option<String> {
        matches!(self.replace.pattern_error, Some(PatternError::NoTerms))
            .then(|| tr("search-replace-no-terms"))
    }

    pub(super) fn replace_footer(&self, t: Tokens, cx: &mut Context<Self>) -> Option<Div> {
        if !self.replace.open {
            return None;
        }
        let (lines, files) = self.replace_selection();
        let text = if self.replace.plan.is_some() {
            t_args(
                "search-replace-kept",
                &[
                    ("lines", Arg::Int(lines as i64)),
                    ("files", Arg::Int(files as i64)),
                ],
            )
        } else {
            String::new()
        };
        let disabled = lines == 0 || self.replace.applying;
        Some(
            div()
                .flow_row()
                .flex_none()
                .items_center()
                .gap(size::S2)
                .h(px(44.))
                .px(size::S3)
                .border_t_1()
                .border_color(t.line)
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .ui_sm()
                        .text_color(t.ink_2)
                        .child(ellipsis(text)),
                )
                .child(
                    Button::new("replace-apply", tr("search-replace-apply"))
                        .kind(ButtonKind::Primary)
                        .disabled(disabled)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.apply_replace(window, cx)),
                        ),
                ),
        )
    }

    pub(super) fn replace_card(&self, t: Tokens) -> Option<Div> {
        let line = |text: String| div().ui_sm().text_color(t.ink_2).child(text);
        let mut card = div()
            .flex()
            .flex_col()
            .gap(size::S1)
            .p(size::S3)
            .border_1()
            .border_color(t.line_2);
        if let Some(report) = &self.replace.report {
            card = card.child(div().ui_strong().text_color(t.ink).child(t_args(
                "search-replace-done",
                &[
                    ("count", Arg::Int(report.occurrences as i64)),
                    ("files", Arg::Int(report.written.len() as i64)),
                ],
            )));
            if !report.failed.is_empty() {
                card = card.child(line(t_args(
                    "search-replace-failed",
                    &[("count", Arg::Int(report.failed.len() as i64))],
                )));
            }
            for (_, title, failure) in &report.failed {
                let reason = match failure {
                    ReplaceFailure::Changed => tr("search-replace-changed"),
                    ReplaceFailure::Write(detail) => t_args(
                        "search-replace-write-error",
                        &[("detail", Arg::Str(detail))],
                    ),
                };
                card = card.child(
                    div()
                        .flow_row()
                        .relative()
                        .items_start()
                        .gap(size::S2)
                        .pad_start(size::S3)
                        .ui_sm()
                        .text_color(t.ink_2)
                        .child(
                            (div().absolute().top_0().bottom_0().w(px(2.)).bg(t.danger))
                                .start_at(px(0.)),
                        )
                        .child(div().flex_1().min_w_0().child(format!("{title}: {reason}"))),
                );
            }
            return Some(card);
        }
        let plan = self
            .replace
            .plan
            .as_ref()
            .filter(|p| !p.unreadable.is_empty())?;
        Some(card.child(line(t_args(
            "search-replace-unreadable",
            &[("count", Arg::Int(plan.unreadable.len() as i64))],
        ))))
    }
}
