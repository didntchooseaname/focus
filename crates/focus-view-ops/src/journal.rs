use std::collections::BTreeSet;
use std::ops::Range;

use focus_i18n::{Arg, t_args};
use focus_kit::{ActiveTokens, Button, Field, Icon, MenuRow, Type, icon, menu, size};
use focus_ops::{JournalEntry, journal};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle,
    InteractiveElement, IntoElement, ParentElement, Render, ScrollStrategy,
    StatefulInteractiveElement, Styled, Subscription, UniformListScrollHandle, Window, div, px,
    uniform_list,
};
use gpui_component::input::InputState;

use crate::event::OpsEvent;
use crate::state::OpsState;
use crate::ui::{hatched_empty, t_};

pub(crate) fn filter(entries: &[JournalEntry], text: &str, target: Option<&str>) -> Vec<usize> {
    let needle = text.trim().to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| target.is_none_or(|t| e.target.as_deref() == Some(t)))
        .filter(|(_, e)| {
            needle.is_empty()
                || e.text.to_lowercase().contains(&needle)
                || e.target
                    .as_deref()
                    .is_some_and(|t| t.to_lowercase().contains(&needle))
        })
        .map(|(i, _)| i)
        .collect()
}

pub struct JournalPanel {
    state: Entity<OpsState>,
    query: Entity<InputState>,
    generation: u64,
    target: Option<String>,
    menu: bool,
    menus: crate::ui::MenuFocus,
    copied: crate::ui::CopyConfirm,
    shown: Vec<usize>,
    seen: usize,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    compact: crate::rich::Compact,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<OpsEvent> for JournalPanel {}

impl JournalPanel {
    fn jump(&mut self, jump: crate::ui::Jump, _: &mut Window, cx: &mut Context<Self>) {
        crate::ui::scroll_jump(&self.scroll.0.borrow().base_handle, jump);
        cx.notify();
    }

    pub fn new(
        state: Entity<OpsState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> JournalPanel {
        let query = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-journal-filter"))
        });
        let observe = cx.observe(&state, |this, _, cx| {
            this.refilter(cx);
            cx.notify();
        });
        let typed = cx.subscribe_in(
            &query,
            window,
            |this: &mut JournalPanel, _, _: &gpui_component::input::InputEvent, _, cx| {
                this.refilter(cx);
                cx.notify();
            },
        );
        let mut panel = JournalPanel {
            generation: focus_i18n::generation(),
            state,
            query,
            target: None,
            menu: false,
            menus: crate::ui::MenuFocus::new(cx),
            copied: Default::default(),
            shown: Vec::new(),
            seen: 0,
            scroll: UniformListScrollHandle::new(),
            focus: crate::ui::list_focus(cx),
            compact: crate::rich::Compact::new(),
            _subscriptions: vec![observe, typed],
        };
        panel.refilter(cx);
        panel
    }

    pub fn focus_filter(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.update(cx, |input, cx| input.focus(window, cx));
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let text = self.query.read(cx).value().to_string();
        let state = self.state.read(cx);
        let entries = state
            .active
            .as_ref()
            .map(|a| a.journal.as_slice())
            .unwrap_or_default();
        self.shown = filter(entries, &text, self.target.as_deref());
        if entries.len() > self.seen
            && let Some(last) = self.shown.len().checked_sub(1)
        {
            self.scroll.scroll_to_item(last, ScrollStrategy::Bottom);
        }
        self.seen = entries.len();
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.state.read(cx).active.as_ref() else {
            return;
        };
        let entries: Vec<JournalEntry> = self
            .shown
            .iter()
            .filter_map(|i| active.journal.get(*i).cloned())
            .collect();
        let name = active.engagement.name.replace(['/', '\\'], "-");
        cx.emit(OpsEvent::ExportJournalCsv {
            file_name: format!("{name}-journal.csv"),
            content: journal::to_csv(&entries),
        });
    }
}

impl Render for JournalPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.menus.sync(self.menu, window, cx);
        let t = *cx.tokens();
        if self.generation != focus_i18n::generation() {
            self.generation = focus_i18n::generation();
            self.query.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-journal-filter"), window, cx)
            });
        }

        let Some(active) = self.state.read(cx).active.clone() else {
            return div()
                .id("ops-journal")
                .size_full()
                .bg(t.bg)
                .flex()
                .child(hatched_empty(&t, t_("ops-no-engagement"), None));
        };
        let targets: BTreeSet<String> = active
            .journal
            .iter()
            .filter_map(|e| e.target.clone())
            .collect();
        let total = self.shown.len();
        let target_label = self
            .target
            .clone()
            .unwrap_or_else(|| t_("ops-journal-all-targets"));

        let target_button = div()
            .id("ops-journal-target")
            .flex()
            .items_center()
            .justify_between()
            .gap(size::S2)
            .flex_none()
            .w(px(168.))
            .h(size::CONTROL)
            .px(size::S2)
            .rounded(size::RADIUS)
            .border_1()
            .border_color(t.line_2)
            .bg(t.bg_2)
            .cursor_pointer()
            .text_color(t.ink)
            .on_click(cx.listener(|this, _, _, cx| {
                this.menu = this.menus.click_opens(this.menu);
                cx.notify();
            }))
            .child(
                div()
                    .min_w_0()
                    .mono_sm()
                    .child(focus_kit::ellipsis(target_label)),
            )
            .child(icon(Icon::CHEVRON_DOWN, size::ICON_SMALL).text_color(t.ink_3));
        let menu_el = self.menu.then(|| {
            let mut rows: Vec<(Option<String>, String)> =
                vec![(None, t_("ops-journal-all-targets"))];
            rows.extend(targets.iter().map(|x| (Some(x.clone()), x.clone())));
            let entity = cx.entity();
            let dismiss = move |_: &mut Window, cx: &mut gpui::App| {
                entity.update(cx, |this, cx| {
                    this.menu = false;
                    this.menus.dismissed();
                    cx.notify();
                })
            };
            self.menus
                .attach(div().absolute().bottom(px(32.)).left_0(), dismiss)
                .child(menu(&t).min_w(px(168.)).max_h(px(200.)).children(
                    rows.into_iter().enumerate().map(|(i, (value, label))| {
                        MenuRow::new(("ops-journal-target-row", i), label)
                            .checked(value == self.target)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.target = value.clone();
                                this.menu = false;
                                this.refilter(cx);
                                cx.notify();
                            }))
                    }),
                ))
        });

        let header = crate::ui::row()
            .items_center()
            .gap(size::S3)
            .flex_none()
            .h(size::SIDEBAR_HEADER)
            .px(size::S3)
            .border_b_1()
            .border_color(t.line)
            .child(
                div()
                    .flex_none()
                    .ui_strong()
                    .text_color(t.ink)
                    .child(t_("ops-section-journal")),
            )
            .child(
                div()
                    .flex_none()
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(t_args("ops-journal-count", &[("count", Arg::from(total))])),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .max_w(px(360.))
                    .child(Field::new(&self.query)),
            )
            .child(div().relative().child(target_button).children(menu_el))
            .child(div().flex_1())
            .child(
                Button::new("ops-journal-export", t_("ops-journal-export-csv"))
                    .icon(Icon::FILE_TEXT)
                    .disabled(total == 0)
                    .on_click(cx.listener(|this, _, _, cx| this.export(cx))),
            );

        let body: AnyElement = if total == 0 {
            hatched_empty(
                &t,
                t_(if active.journal.is_empty() {
                    "ops-journal-empty"
                } else {
                    "ops-journal-no-match"
                }),
                None,
            )
            .into_any_element()
        } else {
            let entity = cx.entity();
            uniform_list(
                "ops-journal-list",
                total,
                move |range: Range<usize>, _window: &mut Window, cx: &mut App| {
                    let entity_for_links = entity.clone();
                    entity.update(cx, |this, cx| {
                        let Some(active) = this.state.read(cx).active.clone() else {
                            return Vec::new();
                        };
                        range
                            .filter_map(|row| {
                                this.shown
                                    .get(row)
                                    .and_then(|i| active.journal.get(*i))
                                    .map(|e| (row, e.clone()))
                            })
                            .map(|(row, e)| {
                                let text = this.compact.element(
                                    format!("journal-{row}"),
                                    &e.text,
                                    &active.hosts,
                                    cx,
                                );
                                div()
                                    .id(("ops-journal-row", row))
                                    .flex()
                                    .items_center()
                                    .gap(size::S3)
                                    .h(px(22.))
                                    .px(size::S3)
                                    .child(
                                        div()
                                            .flex_none()
                                            .mono_sm()
                                            .text_color(t.ink_3)
                                            .child(e.timestamp),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(120.))
                                            .mono_sm()
                                            .text_color(t.ink_2)
                                            .child(focus_kit::ellipsis(
                                                e.target.unwrap_or_else(|| "-".into()),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .h(px(20.))
                                            .overflow_hidden()
                                            .child(text),
                                    )
                                    .children(e.id.clone().map(|id| {
                                        let entity = entity_for_links.clone();
                                        let which = format!("link-{row}");
                                        let (face, label) = this.copied.face(
                                            &which,
                                            focus_kit::Icon::LINK,
                                            "ops-journal-copy-link",
                                        );
                                        focus_kit::IconButton::new(
                                            ("ops-journal-link", row),
                                            face,
                                            label,
                                        )
                                        .dense(true)
                                        .on_click(
                                            move |_, _, cx| {
                                                focus_kit::clipboard::copy_text(
                                                    cx,
                                                    format!("[[Journal#^{id}]]"),
                                                );
                                                entity.update(cx, |this, cx| {
                                                    this.copied.mark(
                                                        which.clone(),
                                                        |view: &mut JournalPanel| &mut view.copied,
                                                        cx,
                                                    )
                                                });
                                            },
                                        )
                                    }))
                            })
                            .collect()
                    })
                },
            )
            .track_scroll(self.scroll.clone())
            .flex_1()
            .into_any_element()
        };

        crate::ui::pageable(div().id("ops-journal"), &self.focus, cx, Self::jump)
            .flex()
            .flex_col()
            .size_full()
            .bg(t.bg)
            .child(header)
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(target: Option<&str>, text: &str) -> JournalEntry {
        JournalEntry {
            timestamp: "2026-11-12 10:00:00".into(),
            target: target.map(Into::into),
            text: text.into(),
            id: None,
        }
    }

    #[test]
    fn filter_matches_text_and_target_case_insensitively() {
        let entries = vec![
            entry(Some("10.0.0.5"), "Found SMB signing off"),
            entry(None, "Break"),
            entry(Some("10.0.0.6"), "smb relay"),
        ];
        assert_eq!(filter(&entries, "smb", None), [0, 2]);
        assert_eq!(filter(&entries, "", Some("10.0.0.6")), [2]);
        assert_eq!(filter(&entries, "0.0.5", None), [0]);
        assert_eq!(filter(&entries, "nothing", None), Vec::<usize>::new());
    }
}
