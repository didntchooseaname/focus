use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use focus_core::NoteId;
use focus_i18n::{Arg, t_args};
use focus_kit::{Dir, Tokens, Type, ellipsis, size};
use gpui::AppContext as _;
use gpui::{
    AnyElement, Context, FontWeight, HighlightStyle, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, UnderlineStyle, div, px,
};

use super::ReadingView;
use crate::query::{QueryBlock, QueryKey, QueryOutcome, QueryResult, QueryState, query_search};

#[derive(Clone, Debug, PartialEq)]
pub struct QueryHitClicked {
    pub note: NoteId,
    pub line: Option<u32>,
    pub new_tab: bool,
}

impl gpui::EventEmitter<QueryHitClicked> for ReadingView {}

#[derive(Default)]
pub(super) struct QueryRefresh {
    fresh: RefCell<HashMap<QueryKey, Arc<QueryOutcome>>>,
    running: RefCell<Option<gpui::Task<()>>>,
}

impl ReadingView {
    fn query_state(&self, block: &QueryBlock, cx: &mut Context<Self>) -> QueryState {
        let key = (block.source.clone(), block.note);
        let fresh = self.queries.fresh.borrow().get(&key).cloned();
        let built = match &block.state {
            QueryState::Ready(outcome) => Some(outcome.clone()),
            QueryState::Pending | QueryState::Unavailable => None,
        };
        let newest = match (built, fresh) {
            (Some(a), Some(b)) => Some(if b.generation >= a.generation { b } else { a }),
            (a, b) => a.or(b),
        };
        let Some(search) = query_search() else {
            return newest.map_or(QueryState::Unavailable, QueryState::Ready);
        };
        let current = search.generation();
        if newest.as_ref().is_none_or(|o| o.generation != current)
            && self.queries.running.borrow().is_none()
        {
            let task = cx.background_spawn(async move {
                let outcome = search.results(&key.0, key.1);
                (key, outcome)
            });
            let running = cx.spawn(async move |this, cx| {
                let (key, outcome) = task.await;
                let _ = this.update(cx, |this, cx| {
                    this.queries.running.borrow_mut().take();
                    if let Some(outcome) = outcome {
                        this.queries.fresh.borrow_mut().insert(key, outcome);
                        cx.notify();
                    }
                });
            });
            *self.queries.running.borrow_mut() = Some(running);
        }
        newest.map_or(QueryState::Pending, QueryState::Ready)
    }

    pub(super) fn render_query(
        &self,
        block: &QueryBlock,
        item: usize,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.query_state(block, cx);
        let dir = Dir::current().of_text(&block.source);
        let count = match &state {
            QueryState::Ready(outcome) => match &outcome.result {
                QueryResult::Found {
                    total_notes,
                    total_matches,
                    ..
                } if *total_notes > 0 => Some(t_args(
                    "search-results-count",
                    &[
                        ("count", Arg::Int(*total_matches as i64)),
                        ("files", Arg::Int(*total_notes as i64)),
                    ],
                )),
                _ => None,
            },
            _ => None,
        };
        let strip = dir
            .row()
            .items_center()
            .gap(size::S2)
            .h(px(28.))
            .px(size::S3)
            .border_b_1()
            .border_color(t.line)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .mono_sm()
                    .text_color(t.ink_3)
                    .child(ellipsis(block.source.clone())),
            )
            .children(
                count.map(|count| div().flex_none().ui_sm().text_color(t.ink_3).child(count)),
            );
        let line = |text: String| {
            div()
                .px(size::S3)
                .py(size::S2)
                .ui_sm()
                .text_color(t.ink_3)
                .child(text)
                .into_any_element()
        };
        let body: Vec<AnyElement> =
            match &state {
                QueryState::Unavailable => vec![line(focus_i18n::t("reading-query-unavailable"))],
                QueryState::Pending => vec![line(focus_i18n::t("search-searching"))],
                QueryState::Ready(outcome) => match &outcome.result {
                    QueryResult::Invalid(reason) => vec![line(t_args(
                        "reading-query-invalid",
                        &[("reason", Arg::Str(reason))],
                    ))],
                    QueryResult::Found { notes, .. } if notes.is_empty() => {
                        vec![line(focus_i18n::t("search-no-results"))]
                    }
                    QueryResult::Found {
                        notes, total_notes, ..
                    } => {
                        let mut rows = Vec::new();
                        let mark = HighlightStyle {
                            color: Some(t.ink),
                            font_weight: Some(FontWeight::SEMIBOLD),
                            underline: Some(UnderlineStyle {
                                thickness: px(1.),
                                color: Some(t.ink_3),
                                wavy: false,
                            }),
                            ..Default::default()
                        };
                        for note in notes {
                            let id = (item << 12) | rows.len();
                            rows.push(
                                self.query_row(dir, id, note.note, None, t)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_shrink()
                                            .min_w_0()
                                            .ui_strong()
                                            .text_color(t.ink)
                                            .child(ellipsis(note.title.clone())),
                                    )
                                    .child(
                                        dir.row()
                                            .flex_1()
                                            .min_w_0()
                                            .justify_end()
                                            .mono_sm()
                                            .text_color(t.ink_3)
                                            .child(ellipsis(note.folder.clone())),
                                    )
                                    .child(
                                        div().flex_none().mono_sm().text_color(t.ink_3).child(
                                            focus_i18n::format_number(i64::from(note.count)),
                                        ),
                                    )
                                    .into_any_element(),
                            );
                            for found in &note.lines {
                                let id = (item << 12) | rows.len();
                                rows.push(
                                    dir.pad_start(
                                        self.query_row(dir, id, note.note, Some(found.line), t),
                                        size::S3 + size::INDENT,
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(32.))
                                            .flex()
                                            .justify_end()
                                            .mono_sm()
                                            .text_color(t.ink_3)
                                            .child((found.line + 1).to_string()),
                                    )
                                    .child(
                                        dir.row()
                                            .flex_1()
                                            .min_w_0()
                                            .ui_sm()
                                            .text_color(t.ink_2)
                                            .child(ellipsis(found.text.clone()).highlights(
                                                found.ranges.iter().map(|r| (r.clone(), mark)),
                                            )),
                                    )
                                    .into_any_element(),
                                );
                            }
                        }
                        if *total_notes > notes.len() {
                            rows.push(line(t_args(
                                "reading-query-more",
                                &[("count", Arg::Int((total_notes - notes.len()) as i64))],
                            )));
                        }
                        rows
                    }
                },
            };
        div()
            .flex()
            .flex_col()
            .border_1()
            .border_color(t.line)
            .bg(t.bg)
            .child(strip)
            .children(body)
            .into_any_element()
    }

    fn query_row(
        &self,
        dir: Dir,
        id: usize,
        note: NoteId,
        line: Option<u32>,
        t: &Tokens,
    ) -> gpui::Stateful<gpui::Div> {
        let (hover, active) = (t.card_2, t.card_3);
        let row = dir
            .row()
            .id(("query-row", id))
            .relative()
            .w_full()
            .items_center()
            .gap(size::S2)
            .h(size::ROW)
            .px(size::S3);
        if self.live {
            return row;
        }
        self.zone(
            row.cursor_pointer()
                .hover(move |s| s.bg(hover))
                .active(move |s| s.bg(active)),
            super::Zone::QueryHit { note, line },
        )
    }
}
