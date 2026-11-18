use focus_ai::prompts;
use focus_ai::prompts::Output;
use focus_i18n::{Arg, format_date, format_duration, format_number, t, t_args};
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Icon, IconButton, MenuRow, Tokens, Tooltip, Type, icon, menu,
    menu_separator, size,
};
use gpui::{
    AnyElement, App, Context, Div, Focusable, InteractiveElement, IntoElement, ListOffset,
    MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window,
    div, list, prelude::FluentBuilder, px,
};
use gpui_component::input::Input;

use crate::context::{Chip, ChipKind, format_tokens};
use crate::diff::{self, Mark};
use crate::dir::Dir;
use crate::events::AiEvent;
use crate::model::{
    Assistant, Decision, Entry, FailureRow, Part, ToolLine, ToolState, UserTurn, WriteCard,
};
use crate::panel::{
    AiPanel, Dismiss, MentionAccept, MentionNext, MentionPrev, Newline, Overlay, Submit,
    action_label,
};

const WRENCH: Icon = Icon("\u{E1B1}");
const SQUARE: Icon = Icon("\u{E167}");
const ARROW_UP: Icon = Icon("\u{E04A}");
const ROTATE_CW: Icon = Icon("\u{E149}");
const SHIELD_CHECK: Icon = Icon("\u{E1FF}");
const LOADER: Icon = Icon("\u{E109}");

fn id(parts: &[&dyn std::fmt::Display]) -> SharedString {
    SharedString::from(
        parts
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join("-"),
    )
}

fn tip(label: String) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    move |_, cx| Tooltip::view(label.clone().into(), None, cx)
}

pub(crate) fn summarize_arguments(arguments: &str) -> String {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(arguments)
    else {
        return arguments.trim().to_string();
    };
    let pairs: Vec<String> = map
        .iter()
        .map(|(key, value)| {
            let text = match value {
                serde_json::Value::String(s) => s.replace('\n', " "),
                other => other.to_string(),
            };
            let shortened: String = text.chars().take(48).collect();
            format!(
                "{key}: {shortened}{}",
                if text.chars().count() > 48 { "…" } else { "" }
            )
        })
        .collect();
    pairs.join(", ")
}

impl Render for AiPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let dir = Dir::current();
        if self.placeholder_generation != focus_i18n::generation() {
            self.placeholder_generation = focus_i18n::generation();
            self.input.update(cx, |input, cx| {
                input.set_placeholder(t_("ai-input-placeholder"), window, cx)
            });
        }
        let count = self.entries.len();
        if count > self.listed {
            self.list
                .splice(self.listed..self.listed, count - self.listed);
        } else if count < self.listed {
            self.list.reset(count);
        }
        self.listed = count;
        let extent = self.list.max_offset_for_scrollbar().height;
        if !self.follow && extent > self.last_extent + px(1.) {
            self.new_below = true;
        }
        self.last_extent = extent;
        if self.follow && count > 0 {
            let list = self.list.clone();
            window.on_next_frame(move |_, _| {
                list.scroll_to(ListOffset {
                    item_ix: count - 1,
                    offset_in_item: px(1.0e6),
                })
            });
        }

        let body: AnyElement = if self.history_open {
            self.history_view(&t, dir, cx)
        } else if self.live_backend().is_none() {
            self.no_key_view(&t, dir, cx)
        } else if self.entries.is_empty() {
            self.empty_view(&t)
        } else {
            self.thread_view(cx)
        };

        div()
            .id("ai-panel")
            .key_context("AiPanel")
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this, _: &crate::panel::PageUp, _, cx| this.scroll_jump(false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &crate::panel::PageDown, _, cx| this.scroll_jump(true, cx)),
            )
            .on_action(
                cx.listener(|this, _: &crate::panel::FirstItem, _, cx| this.scroll_edge(false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &crate::panel::LastItem, _, cx| this.scroll_edge(true, cx)),
            )
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .bg(t.bg)
            .text_color(t.ink)
            .child(self.header(&t, dir, cx))
            .child(body)
            .child(self.composer(&t, dir, window, cx))
            .children(self.preview.clone().map(|(preview, position)| {
                gpui::deferred(
                    gpui::anchored()
                        .position(position + gpui::point(gpui::px(0.), gpui::px(18.)))
                        .snap_to_window()
                        .child(preview),
                )
            }))
            .when(self.overlay != Overlay::None, |this| {
                this.child(
                    div()
                        .id("ai-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.dismiss(cx);
                            }),
                        ),
                )
                .child(match self.overlay {
                    Overlay::Models => self.models_menu(&t, dir, cx),
                    _ => self.actions_menu(&t, dir, cx),
                })
            })
    }
}

impl AiPanel {
    fn header(&self, t: &Tokens, dir: Dir, cx: &mut Context<Self>) -> Div {
        let has_conversation = !self.wire.is_empty();
        dir.row()
            .items_center()
            .flex_none()
            .h(size::SIDEBAR_HEADER)
            .px(size::S2)
            .gap(size::S1)
            .border_b_1()
            .border_color(t.line)
            .child(
                div().flex_1().min_w_0().child(
                    dir.row()
                        .id("ai-model")
                        .items_center()
                        .gap(size::S1)
                        .h(size::ICON_BUTTON)
                        .px(size::S2)
                        .max_w_full()
                        .rounded(size::RADIUS)
                        .cursor_pointer()
                        .text_color(t.ink_2)
                        .hover(move |s| s.bg(t.card_2).text_color(t.ink))
                        .tooltip(tip(t_("ai-model")))
                        .on_click(
                            cx.listener(|this, _, _, cx| this.toggle_overlay(Overlay::Models, cx)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .mono_sm()
                                .child(focus_kit::ellipsis(self.model.clone())),
                        )
                        .child(icon(Icon::CHEVRON_DOWN, size::ICON_SMALL)),
                ),
            )
            .child(
                IconButton::new("ai-new", Icon::PLUS, t_("ai-new-conversation"))
                    .on_click(cx.listener(|this, _, _, cx| this.new_conversation(cx))),
            )
            .child(
                IconButton::new("ai-history", Icon::HISTORY, t_("ai-history"))
                    .active(self.history_open)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_history(cx))),
            )
            .child(
                IconButton::new("ai-save", Icon::FILE_PLUS, t_("ai-save-as-note"))
                    .disabled(!has_conversation)
                    .on_click(cx.listener(|this, _, _, cx| this.save_as_note(cx))),
            )
    }

    fn empty_view(&self, t: &Tokens) -> AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(size::S1)
            .p(size::S4)
            .child(
                div()
                    .ui_strong()
                    .text_color(t.ink)
                    .child(t_("ai-empty-title")),
            )
            .child(div().ui_sm().text_color(t.ink_3).child(t_("ai-empty-hint")))
            .into_any_element()
    }

    fn no_key_view(&self, t: &Tokens, _dir: Dir, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .items_start()
            .gap(size::S3)
            .p(size::S4)
            .child(
                div()
                    .ui_strong()
                    .text_color(t.ink)
                    .child(t_("ai-no-key-title")),
            )
            .child(div().ui_sm().text_color(t.ink_3).child(t_args(
                "ai-no-key-body",
                &[("provider", Arg::from(self.settings.provider.spec().label))],
            )))
            .child(
                Button::new("ai-open-settings", t_("ai-no-key-open"))
                    .icon(Icon::SETTINGS)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(AiEvent::OpenSettings))),
            )
            .into_any_element()
    }

    fn history_view(&self, t: &Tokens, dir: Dir, cx: &mut Context<Self>) -> AnyElement {
        let list = div()
            .id("ai-history-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.history_scroll)
            .flex()
            .flex_col()
            .py(size::S1);
        if self.history.is_empty() {
            return list
                .p(size::S4)
                .child(
                    div()
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(t_("ai-history-empty")),
                )
                .into_any_element();
        }
        list.children(self.history.iter().map(|(key, title, updated)| {
            let (resume_id, delete_id) = (key.clone(), key.clone());
            let shown = if title.is_empty() {
                t_("ai-history-untitled")
            } else {
                title.clone()
            };
            dir.row()
                .id(id(&[&"ai-history", key]))
                .items_center()
                .gap(size::S2)
                .h(size::ROW + size::S2)
                .mx(size::S1)
                .px(size::S2)
                .rounded(size::RADIUS)
                .cursor_pointer()
                .ui()
                .text_color(t.ink_2)
                .hover(move |s| s.bg(t.card_2).text_color(t.ink))
                .on_click(cx.listener(move |this, _, _, cx| this.resume(resume_id.clone(), cx)))
                .child(div().flex_1().min_w_0().child(focus_kit::ellipsis(shown)))
                .child(
                    div()
                        .flex_none()
                        .mono_sm()
                        .text_color(t.ink_3)
                        .child(format_date(*updated as i64)),
                )
                .child(
                    IconButton::new(
                        id(&[&"ai-history-delete", key]),
                        Icon::TRASH,
                        t_("ai-history-delete"),
                    )
                    .dense(true)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.delete_conversation(delete_id.clone(), cx)
                    })),
                )
        }))
        .into_any_element()
    }

    fn thread_view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        let to_bottom = (!self.follow).then(|| {
            let button = IconButton::new(
                "ai-scroll-bottom",
                Icon::CHEVRON_DOWN,
                t_(if self.new_below {
                    "ai-scroll-bottom-new"
                } else {
                    "ai-scroll-bottom"
                }),
            )
            .on_click(cx.listener(|this, _, _, cx| this.scroll_to_bottom(cx)));
            let marked = div().relative().child(button).when(self.new_below, |this| {
                this.child(
                    focus_kit::Dir::current()
                        .end(div().absolute().top(px(2.)), px(2.))
                        .child(focus_kit::dot(t.ink)),
                )
            });
            focus_kit::Dir::current()
                .end(div().absolute().bottom(size::S3), size::S3)
                .child(marked)
        });
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(
                list(
                    self.list.clone(),
                    cx.processor(|this, ix: usize, _window, cx| this.entry_view(ix, cx)),
                )
                .size_full(),
            )
            .children(to_bottom)
            .into_any_element()
    }

    fn entry_view(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        let dir = Dir::current();
        let last_assistant = self
            .entries
            .iter()
            .rposition(|e| matches!(e, Entry::Assistant(_)));
        let Some(entry) = self.entries.get(ix) else {
            return div().into_any_element();
        };
        let body = match entry {
            Entry::User(turn) => self.user_turn(&t, dir, turn),
            Entry::Assistant(answer) => {
                self.assistant_turn(&t, dir, ix, answer, Some(ix) == last_assistant, cx)
            }
            Entry::Failure(row) => self.failure_row(&t, row, cx),
        };
        div()
            .px(size::S3)
            .pt(if ix == 0 { size::S3 } else { size::S4 })
            .pb(if ix + 1 == self.entries.len() {
                size::S3
            } else {
                px(0.)
            })
            .child(body)
            .into_any_element()
    }

    fn user_turn(&self, t: &Tokens, dir: Dir, turn: &UserTurn) -> AnyElement {
        let mut column = div().flex().flex_col().gap(size::S1).min_w_0();
        column = column.child(
            dir.align(
                div()
                    .min_w_0()
                    .px(size::S3)
                    .py(size::S2)
                    .rounded(size::RADIUS)
                    .bg(t.card_2)
                    .ui()
                    .text_color(t.ink),
                &turn.text,
            )
            .child(turn.text.clone()),
        );
        if !turn.chips.is_empty() {
            column = column.child(dir.row().flex_wrap().gap(size::S1).children(
                turn.chips.iter().map(|chip| {
                    dir.row()
                        .items_baseline()
                        .gap(px(4.))
                        .px(px(6.))
                        .py(px(2.))
                        .rounded(size::RADIUS)
                        .bg(t.card_2)
                        .ui_sm()
                        .child(div().text_color(t.ink_2).child(chip.label.clone()))
                        .child(
                            div()
                                .mono_sm()
                                .text_color(t.ink_3)
                                .child(format!("~{}", format_tokens(chip.tokens))),
                        )
                }),
            ));
        }
        if turn.masked > 0 {
            column = column.child(
                dir.row()
                    .items_center()
                    .gap(size::S1)
                    .ui_xs()
                    .text_color(t.ink_3)
                    .child(icon(SHIELD_CHECK, size::ICON_SMALL))
                    .child(t_args(
                        "ai-redaction-notice",
                        &[("count", Arg::from(turn.masked))],
                    )),
            );
        }
        column.into_any_element()
    }

    fn assistant_turn(
        &self,
        t: &Tokens,
        dir: Dir,
        index: usize,
        answer: &Assistant,
        last: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut column = div().flex().flex_col().gap(size::S2).min_w_0();
        let cursor = (answer.live).then_some(self.blink || self.settings.reduce_motion);
        let last_part = answer.parts.len().saturating_sub(1);
        let mut cursor_placed = false;
        for (part_index, part) in answer.parts.iter().enumerate() {
            let at_end = part_index == last_part;
            column = column.child(match part {
                Part::Reasoning {
                    text, open, took, ..
                } => self.reasoning_row(
                    t,
                    dir,
                    index,
                    part_index,
                    text,
                    *open,
                    *took,
                    if at_end && *open { cursor } else { None },
                    &mut cursor_placed,
                    cx,
                ),
                Part::Text { view, .. } => {
                    cursor_placed |= at_end;
                    view.clone().map_or_else(
                        || div().into_any_element(),
                        |v| div().min_w_0().child(v).into_any_element(),
                    )
                }
                Part::Tool(line) => self.tool_row(t, dir, index, part_index, line, cx),
                Part::Write(card) => self.write_card(t, dir, index, Some(part_index), card, cx),
                Part::Notice(key) => div()
                    .ui_xs()
                    .text_color(t.ink_3)
                    .child(t_(key))
                    .into_any_element(),
            });
        }
        let waiting = matches!(
            answer.parts.last(),
            Some(Part::Tool(ToolLine {
                state: ToolState::Running,
                ..
            }))
        ) || matches!(
            answer.parts.last(),
            Some(Part::Write(WriteCard {
                decision: Decision::Pending,
                ..
            }))
        );
        if answer.live && !cursor_placed && !waiting {
            let visible = self.blink || self.settings.reduce_motion;
            column = column.child(
                div()
                    .mono()
                    .text_color(if visible { t.ink } else { t.ink.opacity(0.) })
                    .child("\u{2588}"),
            );
        }
        if let Some(offer) = &answer.offer {
            column = column.child(self.offer_view(t, dir, index, offer, cx));
        }
        if answer.stopped {
            column = column.child(div().ui_xs().text_color(t.ink_3).child(t_("ai-stopped")));
        }
        if !answer.live && !answer.is_empty() {
            column = column.child(self.answer_footer(t, dir, index, answer, last, cx));
        }
        column.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn reasoning_row(
        &self,
        t: &Tokens,
        dir: Dir,
        index: usize,
        part: usize,
        text: &str,
        open: bool,
        took: Option<std::time::Duration>,
        cursor: Option<bool>,
        placed: &mut bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut shown = text.to_string();
        if cursor.is_some() {
            shown.push('\u{2588}');
            *placed = true;
        }
        let ink = t.ink_3;
        div()
            .flex()
            .flex_col()
            .gap(size::S1)
            .min_w_0()
            .child(
                dir.row()
                    .id(id(&[&"ai-reasoning", &index, &part]))
                    .items_center()
                    .gap(size::S1)
                    .h(px(22.))
                    .cursor_pointer()
                    .text_color(ink)
                    .hover(move |s| s.text_color(t.ink_2))
                    .tooltip(tip(t_(if open {
                        "ai-reasoning-hide"
                    } else {
                        "ai-reasoning-show"
                    })))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_part(index, part, cx)))
                    .child(icon(
                        if open {
                            Icon::CHEVRON_DOWN
                        } else {
                            dir.forward()
                        },
                        size::ICON_SMALL,
                    ))
                    .child(div().ui_sm().child(t_("ai-reasoning")))
                    .when_some(took.filter(|d| !d.is_zero()), |this, d| {
                        this.child(div().mono_sm().child(format_duration(d)))
                    }),
            )
            .when(open, |this| {
                let body = dir.border_start(div().min_w_0(), t.line_2);
                this.child(
                    dir.pad_start(body, size::S2)
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(shown),
                )
            })
            .into_any_element()
    }

    fn tool_row(
        &self,
        t: &Tokens,
        dir: Dir,
        index: usize,
        part: usize,
        line: &ToolLine,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status: AnyElement = match &line.state {
            ToolState::Running => icon(LOADER, size::ICON_SMALL)
                .text_color(t.ink_3)
                .into_any_element(),
            ToolState::Refused => div()
                .ui_xs()
                .text_color(t.ink_3)
                .child(t_("ai-tool-refused"))
                .into_any_element(),
            ToolState::Failed(_) => div()
                .ui_xs()
                .text_color(t.danger)
                .child(t_("ai-tool-failed"))
                .into_any_element(),
            ToolState::Done(_) => div().into_any_element(),
        };
        let result = match &line.state {
            ToolState::Done(text) | ToolState::Failed(text) => Some(text.clone()),
            _ => None,
        };
        div()
            .flex()
            .flex_col()
            .gap(size::S1)
            .min_w_0()
            .child(
                dir.row()
                    .id(id(&[&"ai-tool", &index, &part]))
                    .items_center()
                    .gap(size::S2)
                    .h(px(22.))
                    .cursor_pointer()
                    .text_color(t.ink_3)
                    .hover(move |s| s.text_color(t.ink_2))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_part(index, part, cx)))
                    .child(icon(WRENCH, size::ICON_DENSE))
                    .child(
                        div()
                            .flex_none()
                            .mono_sm()
                            .text_color(t.ink_2)
                            .child(line.name.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .mono_sm()
                            .child(focus_kit::ellipsis(summarize_arguments(&line.arguments))),
                    )
                    .child(status)
                    .when_some(line.took, |this, d| {
                        this.child(div().flex_none().mono_sm().child(format_duration(d)))
                    }),
            )
            .when(line.open, |this| {
                let text = result
                    .filter(|r| !r.is_empty())
                    .map(|r| r.chars().take(2000).collect::<String>())
                    .unwrap_or_else(|| t_("ai-tool-no-output"));
                this.child(
                    div()
                        .id(id(&[&"ai-tool-result", &index, &part]))
                        .max_h(px(160.))
                        .overflow_y_scroll()
                        .p(size::S2)
                        .rounded(size::RADIUS)
                        .bg(t.card_2)
                        .mono_sm()
                        .text_color(t.ink_2)
                        .child(text),
                )
            })
            .into_any_element()
    }

    fn write_card(
        &self,
        t: &Tokens,
        dir: Dir,
        entry: usize,
        part: Option<usize>,
        card: &WriteCard,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (added, removed) = diff::counts(&card.diff);
        let (shown, hidden) = diff::hunks(&card.diff, 1);
        const MAX_ROWS: usize = 24;
        let overflow = shown.len().saturating_sub(MAX_ROWS);
        let rows = shown.into_iter().take(MAX_ROWS).map(|line| {
            let (marker, color, tint) = match line.mark {
                Mark::Added => ("+", t.ok, Some(t.ok.opacity(0.12))),
                Mark::Removed => ("-", t.danger, Some(t.danger.opacity(0.12))),
                Mark::Same => (" ", t.ink_3, None),
            };
            dir_ltr_row()
                .gap(size::S1)
                .px(size::S2)
                .mono_sm()
                .when_some(tint, |this, tint| this.bg(tint))
                .child(div().flex_none().w(px(8.)).text_color(color).child(marker))
                .child(div().flex_1().min_w_0().text_color(t.ink_body).child(
                    if line.text.is_empty() {
                        " ".to_string()
                    } else {
                        line.text
                    },
                ))
        });
        let apply_label = t_("ai-confirm-apply");
        div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .min_w_0()
            .p(size::S3)
            .rounded(size::RADIUS)
            .border_1()
            .border_color(t.line_2)
            .bg(t.card)
            .child(div().ui_strong().child(t_(card.title_key)))
            .when_some(card.path.clone(), |this, path| {
                this.child(
                    div()
                        .mono_sm()
                        .text_color(t.ink_2)
                        .child(focus_kit::ellipsis(path.display().to_string())),
                )
            })
            .when_some(card.note.clone(), |this, note| {
                this.child(div().ui().text_color(t.ink_body).child(note))
            })
            .when(!card.diff.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .py(size::S1)
                        .rounded(size::RADIUS)
                        .border_1()
                        .border_color(t.line)
                        .bg(t.bg_2)
                        .children(rows)
                        .when(overflow + hidden > 0, |this| {
                            this.child(div().px(size::S2).mono_sm().text_color(t.ink_3).child(
                                t_args("ai-diff-more", &[("count", Arg::from(overflow + hidden))]),
                            ))
                        }),
                )
                .child(
                    div()
                        .mono_sm()
                        .text_color(t.ink_3)
                        .child(format!("+{added} -{removed}")),
                )
            })
            .child(match (card.decision, part) {
                (Decision::Pending, _) => dir
                    .row()
                    .gap(size::S2)
                    .child(
                        Button::new(id(&[&"ai-apply", &entry, &part.unwrap_or(99)]), apply_label)
                            .kind(ButtonKind::Primary)
                            .on_click(cx.listener(move |this, _, _, cx| match part {
                                Some(p) => this.answer_write(entry, p, true, cx),
                                None => this.answer_offer(entry, true, cx),
                            })),
                    )
                    .child(
                        Button::new(
                            id(&[&"ai-reject", &entry, &part.unwrap_or(99)]),
                            t_("ai-confirm-reject"),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            match part {
                                Some(p) => this.answer_write(entry, p, false, cx),
                                None => this.answer_offer(entry, false, cx),
                            }
                        })),
                    )
                    .into_any_element(),
                (Decision::Applied, _) => div()
                    .ui_sm()
                    .text_color(t.ok)
                    .child(t_("ai-confirm-applied"))
                    .into_any_element(),
                (Decision::Rejected, _) => div()
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(t_("ai-confirm-rejected"))
                    .into_any_element(),
            })
            .into_any_element()
    }

    fn offer_view(
        &self,
        t: &Tokens,
        dir: Dir,
        entry: usize,
        offer: &crate::model::Offer,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if offer.output == Output::Replace {
            return self.write_card(t, dir, entry, None, &offer.card, cx);
        }
        let label = t_(if offer.output == Output::Insert {
            "ai-output-insert"
        } else {
            "ai-output-note"
        });
        match offer.card.decision {
            Decision::Pending => dir
                .row()
                .gap(size::S2)
                .child(
                    Button::new(id(&[&"ai-offer-apply", &entry]), label)
                        .kind(ButtonKind::Primary)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.answer_offer(entry, true, cx)),
                        ),
                )
                .child(
                    Button::new(id(&[&"ai-offer-reject", &entry]), t_("ai-confirm-reject"))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.answer_offer(entry, false, cx)),
                        ),
                )
                .into_any_element(),
            Decision::Applied => div()
                .ui_sm()
                .text_color(t.ok)
                .child(t_("ai-confirm-applied"))
                .into_any_element(),
            Decision::Rejected => div()
                .ui_sm()
                .text_color(t.ink_3)
                .child(t_("ai-confirm-rejected"))
                .into_any_element(),
        }
    }

    fn answer_footer(
        &self,
        t: &Tokens,
        dir: Dir,
        index: usize,
        answer: &Assistant,
        last: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_text = !answer.answer().is_empty();
        let can_insert = has_text && self.snapshot.note.is_some();
        let copied = self.copied.is_some_and(|(entry, _)| entry == index);
        let mut stats = Vec::new();
        if let Some(usage) = &answer.usage {
            stats.push(t_args(
                "ai-usage-input",
                &[(
                    "count",
                    Arg::from(format_number(i64::from(usage.prompt_tokens)).as_str()),
                )],
            ));
            stats.push(t_args(
                "ai-usage-output",
                &[(
                    "count",
                    Arg::from(format_number(i64::from(usage.completion_tokens)).as_str()),
                )],
            ));
            let total = usage.cache_hit_tokens + usage.cache_miss_tokens;
            if total > 0 {
                let percent =
                    (f64::from(usage.cache_hit_tokens) * 100. / f64::from(total)).round() as i64;
                stats.push(t_args("ai-usage-cache", &[("percent", Arg::from(percent))]));
            }
        }
        if let Some(cost) = answer.cost {
            stats.push(format!("${}", focus_i18n::format_decimal(cost, 4)));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                dir.row()
                    .items_center()
                    .child(
                        IconButton::new(
                            id(&[&"ai-copy", &index]),
                            if copied { Icon::CHECK } else { Icon::COPY },
                            t_(if copied { "ai-copied" } else { "ai-copy" }),
                        )
                        .dense(true)
                        .disabled(!has_text)
                        .on_click(cx.listener(move |this, _, _, cx| this.copy_answer(index, cx))),
                    )
                    .child(
                        IconButton::new(
                            id(&[&"ai-insert", &index]),
                            Icon::FILE_INPUT,
                            t_("ai-insert-at-cursor"),
                        )
                        .dense(true)
                        .disabled(!can_insert)
                        .on_click(cx.listener(move |this, _, _, cx| this.insert_answer(index, cx))),
                    )
                    .when(last, |this| {
                        this.child(
                            IconButton::new(id(&[&"ai-retry", &index]), ROTATE_CW, t_("ai-retry"))
                                .dense(true)
                                .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                        )
                    }),
            )
            .when(!stats.is_empty(), |this| {
                this.child(
                    dir.row()
                        .flex_wrap()
                        .gap_x(size::S3)
                        .mono_sm()
                        .text_color(t.ink_3)
                        .children(stats.into_iter().map(|s| div().child(s))),
                )
            })
            .into_any_element()
    }

    fn failure_row(&self, t: &Tokens, row: &FailureRow, cx: &mut Context<Self>) -> AnyElement {
        let waiting = row
            .retry_at
            .is_some_and(|at| at > std::time::Instant::now());
        div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .min_w_0()
            .p(size::S3)
            .rounded(size::RADIUS)
            .border_1()
            .border_color(t.danger)
            .child(
                Dir::current()
                    .row()
                    .items_start()
                    .gap(size::S2)
                    .child(
                        icon(Icon::ALERT_CIRCLE, size::ICON)
                            .text_color(t.danger)
                            .mt(px(2.)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .ui()
                            .text_color(t.ink)
                            .child(row.failure.message.clone()),
                    ),
            )
            .child(
                Dir::current()
                    .row()
                    .gap(size::S2)
                    .child(
                        Button::new("ai-failure-retry", t_("ai-retry"))
                            .icon(ROTATE_CW)
                            .disabled(waiting)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(Entry::Failure(_)) = this.entries.last() {
                                    this.entries.pop();
                                }
                                this.retry(cx)
                            })),
                    )
                    .when(row.failure.settings, |this| {
                        this.child(
                            Button::new("ai-failure-settings", t_("ai-no-key-open"))
                                .icon(Icon::SETTINGS)
                                .on_click(
                                    cx.listener(|_, _, _, cx| cx.emit(AiEvent::OpenSettings)),
                                ),
                        )
                    }),
            )
            .into_any_element()
    }

    fn composer(
        &mut self,
        t: &Tokens,
        dir: Dir,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let chips = self.chips();
        let running = self.run.is_some();
        let no_backend = self.live_backend().is_none();
        let focused = self.input.read(cx).focus_handle(cx).is_focused(window);
        let border = if no_backend {
            t.line
        } else if focused {
            t.ink
        } else {
            t.line_2
        };
        let total: u32 = chips.iter().map(|c| c.tokens).sum::<u32>() + self.input_tokens;
        let empty = self.input_tokens == 0 && self.input.read(cx).value().trim().is_empty();

        let chip_row = (!chips.is_empty()).then(|| {
            dir.row().flex_wrap().gap(size::S1).children(
                chips
                    .iter()
                    .enumerate()
                    .map(|(i, chip)| self.chip_view(t, dir, i, chip, cx)),
            )
        });
        let mention = self.mention_list(t, cx);

        let input_box = div()
            .id("ai-input-box")
            .key_context("AiComposer")
            .flex()
            .flex_col()
            .px(size::S2)
            .py(size::S1)
            .rounded(size::RADIUS)
            .border_1()
            .border_color(border)
            .bg(t.bg_2)
            .when(!no_backend && !focused, |this| {
                this.hover(move |s| s.border_color(t.ink_3))
            })
            .on_action(cx.listener(|this, _: &Submit, window, cx| {
                if this.mention.is_some() {
                    this.mention_accept(None, window, cx);
                } else if this.run.is_none() {
                    this.submit(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Newline, window, cx| {
                this.input
                    .update(cx, |input, cx| input.insert("\n", window, cx))
            }))
            .on_action(cx.listener(|this, _: &MentionNext, _, cx| {
                if !this.mention_move(true, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &MentionPrev, _, cx| {
                if !this.mention_move(false, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &MentionAccept, window, cx| {
                if !this.mention_accept(None, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                if !this.dismiss(cx) {
                    cx.propagate();
                }
            }))
            .child(
                Input::new(&self.input)
                    .appearance(false)
                    .disabled(no_backend)
                    .px_0()
                    .py_0()
                    .text_size(px(13.))
                    .line_height(px(20.)),
            );

        let masking = self.mask.active().then(|| {
            let count = self.masked_last;
            let label = if count > 0 {
                t_args("ai-redaction-notice", &[("count", Arg::from(count))])
            } else {
                t_("ai-masking-active")
            };
            dir.row()
                .id("ai-masking")
                .items_center()
                .gap(px(4.))
                .px(size::S1)
                .text_color(if count > 0 { t.ink } else { t.ink_3 })
                .tooltip(tip(label))
                .child(icon(SHIELD_CHECK, size::ICON_DENSE))
                .when(count > 0, |this| {
                    this.child(div().mono_sm().child(count.to_string()))
                })
        });

        let send = {
            let (glyph, label, disabled) = if running {
                (SQUARE, t_("ai-stop"), false)
            } else {
                (ARROW_UP, t_("ai-send"), empty || no_backend)
            };
            let (btn, ink, hover, press) = (
                if disabled { t.card_3 } else { t.btn },
                if disabled { t.ink_4 } else { t.btn_ink },
                t.btn_hover,
                t.btn_press,
            );
            div()
                .id("ai-send")
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(size::CONTROL)
                .rounded(size::RADIUS)
                .bg(btn)
                .text_color(ink)
                .when(!disabled, |this| {
                    this.cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .active(move |s| s.bg(press))
                        .tooltip(tip(label))
                })
                .child(icon(glyph, size::ICON))
                .when(!disabled, |this| {
                    this.on_click(cx.listener(move |this, _, window, cx| {
                        if this.run.is_some() {
                            this.stop(cx)
                        } else {
                            this.submit(window, cx)
                        }
                    }))
                })
        };

        let toolbar = dir
            .row()
            .items_center()
            .gap(size::S1)
            .child(
                div()
                    .id("ai-actions-wrap")
                    .when_some(disabled_reason(no_backend, running), |this, reason| {
                        this.tooltip(tip(t_(reason)))
                    })
                    .child(
                        IconButton::new("ai-actions", Icon::SPARKLES, t_("ai-actions"))
                            .active(self.overlay == Overlay::Actions)
                            .disabled(no_backend || running)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_overlay(Overlay::Actions, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .id("ai-context-wrap")
                    .when(no_backend, |this| this.tooltip(tip(t_("ai-needs-key"))))
                    .child(
                        IconButton::new("ai-add-context", Icon::AT_SIGN, t_("ai-add-context"))
                            .disabled(no_backend)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.input
                                    .update(cx, |input, cx| input.insert("@", window, cx));
                                this.focus_input(window, cx);
                            })),
                    ),
            )
            .child(div().flex_1())
            .children(masking)
            .children(
                (self.usage_total.prompt_tokens + self.usage_total.completion_tokens > 0).then(
                    || {
                        let tokens = i64::from(
                            self.usage_total.prompt_tokens + self.usage_total.completion_tokens,
                        );
                        let cost = focus_ai::estimate_cost_usd(&self.model, &self.usage_total)
                            .map_or_else(String::new, |c| {
                                format!("${}", focus_i18n::format_decimal(c, 4))
                            });
                        div()
                            .id("ai-conversation-usage")
                            .flex_none()
                            .mono_sm()
                            .text_color(t.ink_3)
                            .tooltip(tip(t_args(
                                "ai-usage-conversation",
                                &[
                                    ("tokens", Arg::from(format_number(tokens).as_str())),
                                    ("cost", Arg::from(cost.as_str())),
                                ],
                            )))
                            .child(cost)
                    },
                ),
            )
            .child(
                div()
                    .id("ai-total")
                    .flex_none()
                    .mono_sm()
                    .text_color(t.ink_3)
                    .tooltip(tip(t_args(
                        "ai-context-tokens",
                        &[("count", Arg::from(total))],
                    )))
                    .child(format!("~{}", format_tokens(total))),
            )
            .child(send);

        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(size::S2)
            .p(size::S2)
            .border_t_1()
            .border_color(t.line)
            .children(chip_row)
            .children(mention)
            .children(self.settings.ops_mode.then(|| {
                let provider = ("provider", Arg::from(self.settings.provider.spec().label));
                let url = ("url", Arg::from(self.settings.base_url.as_str()));
                let (short, detail) = if focus_ai::Provider::is_loopback(&self.settings.base_url) {
                    ("ai-ops-reminder-local", "ai-ops-reminder-local-detail")
                } else {
                    ("ai-ops-reminder", "ai-ops-reminder-detail")
                };
                let args = [provider, url];
                div()
                    .id("ai-ops-reminder")
                    .ui_xs()
                    .text_color(t.ink_3)
                    .tooltip(tip(t_args(detail, &args)))
                    .child(focus_kit::ellipsis(t_args(short, &args)))
            }))
            .child(input_box)
            .child(toolbar)
    }

    fn chip_view(
        &self,
        t: &Tokens,
        dir: Dir,
        index: usize,
        chip: &Chip,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (kind, path) = (chip.kind, chip.path.clone());
        let tooltip = match (chip.kind, &chip.path) {
            (ChipKind::Selection, _) => t_("ai-context-selection"),
            (ChipKind::Note, Some(p)) => format!("{}: {}", t_("ai-context-note"), p.display()),
            (_, Some(p)) => format!("{}: {}", t_("ai-context-added"), p.display()),
            _ => chip.label.clone(),
        };
        dir.row()
            .id(id(&[&"ai-chip", &index]))
            .items_center()
            .gap(size::S1)
            .h(px(22.))
            .max_w_full()
            .min_w_0()
            .pl(size::S2)
            .pr(px(2.))
            .rounded(size::RADIUS)
            .bg(t.card_2)
            .ui_sm()
            .tooltip(tip(tooltip))
            .child(
                div()
                    .min_w_0()
                    .text_color(t.ink)
                    .child(focus_kit::ellipsis(chip.label.clone())),
            )
            .child(
                div()
                    .flex_none()
                    .mono_sm()
                    .text_color(t.ink_3)
                    .child(format!("~{}", format_tokens(chip.tokens))),
            )
            .child(
                div()
                    .id(id(&[&"ai-chip-remove", &index]))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(18.))
                    .rounded(size::RADIUS)
                    .cursor_pointer()
                    .text_color(t.ink_3)
                    .hover(move |s| s.bg(t.card_3).text_color(t.ink))
                    .tooltip(tip(t_("ai-context-remove")))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.remove_chip(kind, path.clone(), cx)),
                    )
                    .child(icon(Icon::CLOSE, size::ICON_SMALL)),
            )
            .into_any_element()
    }

    fn mention_list(&self, t: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let mention = self.mention.as_ref()?;
        let rows: Vec<AnyElement> = if mention.hits.is_empty() {
            vec![
                MenuRow::new("ai-mention-none", t_("ai-mention-empty"))
                    .disabled(true)
                    .into_any_element(),
            ]
        } else {
            mention
                .hits
                .iter()
                .enumerate()
                .map(|(i, hit)| {
                    MenuRow::new(id(&[&"ai-mention", &i]), hit.text.clone())
                        .icon(Icon::FILE_TEXT)
                        .highlighted(i == mention.selected)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.mention_accept(Some(i), window, cx);
                        }))
                        .into_any_element()
                })
                .collect()
        };
        Some(menu(t).min_w_0().children(rows))
    }

    fn models_menu(&self, t: &Tokens, dir: Dir, cx: &mut Context<Self>) -> AnyElement {
        let mut models = if self.models.is_empty() {
            vec![
                focus_ai::MODEL_FLASH.to_string(),
                focus_ai::MODEL_PRO.to_string(),
            ]
        } else {
            self.models.clone()
        };
        if !models.contains(&self.model) {
            models.insert(0, self.model.clone());
        }
        let menu = menu(t).children(models.into_iter().map(|model| {
            let choice = model.clone();
            MenuRow::new(id(&[&"ai-model-row", &model]), model.clone())
                .checked(model == self.model)
                .on_click(cx.listener(move |this, _, _, cx| this.pick_model(choice.clone(), cx)))
        }));
        dir.at_start(div().absolute().top(size::SIDEBAR_HEADER), size::S2)
            .max_w_full()
            .child(menu)
            .into_any_element()
    }

    fn actions_menu(&self, t: &Tokens, dir: Dir, cx: &mut Context<Self>) -> AnyElement {
        let target = if self
            .snapshot
            .selection
            .as_ref()
            .is_some_and(|s| !s.is_empty())
        {
            t_("ai-actions-target-selection")
        } else if self.snapshot.note.is_some() {
            t_("ai-actions-target-note")
        } else {
            t_("ai-actions-no-target")
        };
        let templates: Vec<_> = prompts::merged(&self.settings.actions)
            .into_iter()
            .filter(|a| a.enabled && (!a.ops_only || self.settings.ops_mode))
            .collect();
        let rows = templates.into_iter().map(|template| {
            let available = self.action_target(&template).is_some();
            let missing =
                (!available).then(|| t_(unavailable_reason(self.snapshot.note.is_some())));
            let key = template.id.clone();
            let description = if template.builtin {
                t_(&template.description)
            } else {
                template.description.clone()
            };
            div()
                .id(id(&[&"ai-action-tip", &key]))
                .when(!description.is_empty(), |this| {
                    this.tooltip(tip(description))
                })
                .child(
                    MenuRow::new(id(&[&"ai-action", &key]), action_label(&template))
                        .disabled(!available)
                        .when_some(missing, |row, reason| row.shortcut(reason))
                        .on_click(cx.listener(move |this, _, _, cx| this.run_action(&key, cx))),
                )
        });
        let menu = menu(t)
            .max_h(px(360.))
            .child(
                div()
                    .px(size::S3)
                    .py(size::S1)
                    .ui_xs()
                    .text_color(t.ink_3)
                    .child(target),
            )
            .child(menu_separator(t))
            .child(
                div()
                    .id("ai-actions-list")
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(menu_separator(t))
            .child(
                MenuRow::new("ai-actions-edit", t_("ai-actions-edit"))
                    .icon(Icon::SLIDERS)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.overlay = Overlay::None;
                        cx.emit(AiEvent::OpenSettings);
                        cx.notify();
                    })),
            );
        dir.at_start(div().absolute().bottom(px(48.)), size::S2)
            .max_w_full()
            .child(menu)
            .into_any_element()
    }
}

fn t_(key: &str) -> String {
    t(key)
}

fn unavailable_reason(has_note: bool) -> &'static str {
    if has_note {
        "ai-action-needs-selection"
    } else {
        "ai-action-needs-note"
    }
}

fn disabled_reason(no_backend: bool, running: bool) -> Option<&'static str> {
    if no_backend {
        Some("ai-needs-key")
    } else if running {
        Some("ai-actions-running")
    } else {
        None
    }
}

fn dir_ltr_row() -> Div {
    div().flex().flex_row()
}

#[cfg(test)]
mod tests {
    use super::summarize_arguments;

    #[test]
    fn json_arguments_become_key_value_pairs() {
        let text = summarize_arguments(r#"{"query":"kerberoasting","limit":5}"#);
        assert!(
            text.contains("query: kerberoasting") && text.contains("limit: 5"),
            "{text}"
        );
    }

    #[test]
    fn a_greyed_row_and_the_greyed_button_say_why() {
        assert_eq!(super::unavailable_reason(false), "ai-action-needs-note");
        assert_eq!(super::unavailable_reason(true), "ai-action-needs-selection");
        assert_eq!(super::disabled_reason(true, true), Some("ai-needs-key"));
        assert_eq!(
            super::disabled_reason(false, true),
            Some("ai-actions-running")
        );
        assert_eq!(super::disabled_reason(false, false), None);
    }

    #[test]
    fn long_values_are_cut() {
        let long = "x".repeat(80);
        let text = summarize_arguments(&format!(r#"{{"path":"{long}"}}"#));
        assert!(text.ends_with('…') && text.len() < 70);
    }

    #[test]
    fn invalid_json_is_shown_as_written() {
        assert_eq!(summarize_arguments(" not json "), "not json");
    }
}
