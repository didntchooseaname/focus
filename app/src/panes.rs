use focus_kit::Flow as _;
use focus_kit::{ActiveTokens, Button, Icon, IconButton, Tokens, TreeRow, Type, size};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, ElementId, InteractiveElement, IntoElement,
    MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};

mod mentions;
mod queries;

pub(crate) use mentions::open_unlinked_mentions;

use crate::editor::ViewMode;
use crate::settings::schema::Section;
use crate::shell::{BottomTab, Edge, LeftTab, RightTab, Shell};

pub fn empty_state(t: &Tokens, text: impl Into<SharedString>) -> Div {
    let text: SharedString = text.into();
    div()
        .px(size::S4)
        .py(size::S3)
        .ui_sm()
        .text_color(t.ink_3)
        .child(text)
}

fn header(t: &Tokens) -> Div {
    div()
        .relative()
        .flow_row()
        .flex_none()
        .items_center()
        .h(size::SIDEBAR_HEADER)
        .child(
            div()
                .absolute()
                .start_at(px(0.))
                .end_at(px(0.))
                .bottom_0()
                .h(px(1.))
                .bg(t.line),
        )
}

fn prop_text(value: &focus_core::PropValue) -> String {
    use focus_core::PropValue::*;
    match value {
        Null => String::new(),
        Bool(b) => b.to_string(),
        Number(n) => n.to_string(),
        Text(t) => t.clone(),
        List(items) => items
            .iter()
            .map(prop_text)
            .collect::<Vec<_>>()
            .join(&focus_i18n::t("fmt-list-separator")),
        Map(entries) => entries
            .iter()
            .map(|(k, v)| format!("{k}: {}", prop_text(v)))
            .collect::<Vec<_>>()
            .join(&focus_i18n::t("fmt-list-separator")),
    }
}

fn active_bar(t: &Tokens) -> Div {
    div()
        .absolute()
        .start_at(px(0.))
        .end_at(px(0.))
        .bottom_0()
        .h(px(1.))
        .bg(t.ink)
}

fn strip_keys(
    bottom: bool,
    cx: &mut Context<Shell>,
) -> impl Fn(&gpui::KeyDownEvent, &mut Window, &mut App) + 'static {
    cx.listener(
        move |this: &mut Shell, event: &gpui::KeyDownEvent, window, cx| {
            let keys = &event.keystroke;
            if keys.modifiers.modified() {
                return;
            }
            let rtl = focus_kit::Dir::current().rtl;
            match keys.key.as_str() {
                "right" => this.step_strip(bottom, !rtl, cx),
                "left" => this.step_strip(bottom, rtl, cx),
                "down" | "enter" => this.enter_strip(bottom, window, cx),
                _ => return,
            }
            cx.stop_propagation();
        },
    )
}

fn focus_ring(t: &Tokens) -> Div {
    div().absolute().inset_0().border_1().border_color(t.ink)
}

fn text_tab(
    t: &Tokens,
    id: impl Into<ElementId>,
    label: String,
    (active, focused): (bool, bool),
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let tip: SharedString = label.clone().into();
    div()
        .id(id)
        .relative()
        .flow_row()
        .items_center()
        .h_full()
        .flex_shrink()
        .min_w_0()
        .px(size::S2)
        .ui()
        .text_color(if active { t.ink } else { t.ink_2 })
        .cursor_pointer()
        .hover(|s| s.text_color(t.ink))
        .active(|s| s.bg(t.card_2))
        .tooltip(move |_, cx| focus_kit::Tooltip::view(tip.clone(), None, cx))
        .child(focus_kit::ellipsis(label))
        .when(active, |this| this.child(active_bar(t)))
        .when(active && focused, |this| this.child(focus_ring(t)))
        .on_click(on_click)
}

pub(crate) fn handle(
    t: &Tokens,
    id: &'static str,
    vertical: bool,
    dragging: bool,
) -> gpui::Stateful<Div> {
    let line = div()
        .when(vertical, |this| this.w(px(1.)).h_full())
        .when(!vertical, |this| this.h(px(1.)).w_full())
        .when(dragging, |this| this.bg(t.ink_3))
        .group_hover(id, |s| s.bg(t.ink_3));
    div()
        .id(id)
        .group(id)
        .absolute()
        .flow_row()
        .items_center()
        .justify_center()
        .when(vertical, |this| {
            this.w(size::HANDLE).top_0().bottom_0().cursor_col_resize()
        })
        .when(!vertical, |this| {
            this.h(size::HANDLE)
                .start_at(px(0.))
                .end_at(px(0.))
                .cursor_row_resize()
        })
        .child(line)
}

impl Shell {
    pub fn render_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        queries::register(self.library.as_ref());
        let t = *cx.tokens();
        let layout = &self.layout;
        let (left_open, right_open) = (layout.left_open, layout.right_open);
        let (left_w, right_w) = (layout.left_w, layout.right_w);
        let dragging = self.drag.as_ref().map(|d| d.edge);

        let mut center = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(self.render_center(window, cx));
        if self.layout.bottom_open {
            let bottom_h = self.layout.bottom_h;
            center = center.child(self.render_bottom(window, cx)).child(
                handle(&t, "handle-bottom", false, dragging == Some(Edge::Bottom))
                    .bottom(bottom_h - size::HANDLE / 2.)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.start_drag(Edge::Bottom, event.position.y, cx);
                        }),
                    ),
            );
        }

        div()
            .relative()
            .flow_row()
            .flex_1()
            .min_h_0()
            .when(left_open, |this| {
                this.child(self.render_left(left_w, window, cx))
            })
            .child(center)
            .when(right_open, |this| {
                this.child(self.render_right(right_w, window, cx))
            })
            .when(left_open, |this| {
                this.child(
                    handle(&t, "handle-left", true, dragging == Some(Edge::Left))
                        .start_at(left_w - size::HANDLE / 2. - px(0.5))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                this.start_drag(Edge::Left, event.position.x, cx);
                            }),
                        ),
                )
            })
            .when(right_open, |this| {
                this.child(
                    handle(&t, "handle-right", true, dragging == Some(Edge::Right))
                        .end_at(right_w - size::HANDLE / 2. - px(0.5))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                this.start_drag(Edge::Right, event.position.x, cx);
                            }),
                        ),
                )
            })
    }

    fn render_left(
        &mut self,
        width: gpui::Pixels,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = *cx.tokens();
        let tab = self.layout.left_tab;
        let tabs = [
            (
                LeftTab::Files,
                Icon::FOLDER,
                focus_i18n::t("ui-left-files"),
                "ui-cmd-files",
            ),
            (
                LeftTab::Search,
                Icon::SEARCH,
                focus_i18n::t("ui-left-search"),
                "ui-cmd-search",
            ),
            (LeftTab::Tags, Icon::HASH, focus_i18n::t("ui-left-tags"), ""),
            (
                LeftTab::Bookmarks,
                Icon::BOOKMARK,
                focus_i18n::t("ui-left-bookmarks"),
                "",
            ),
        ];
        let content: AnyElement = match tab {
            LeftTab::Files => self.render_files(cx),
            LeftTab::Search => div()
                .size_full()
                .child(self.search.clone())
                .into_any_element(),
            LeftTab::Tags => match &self.vault_views {
                Some(views) => views.tags.clone().into_any_element(),
                None => empty_state(&t, focus_i18n::t("ui-vault-none")).into_any_element(),
            },
            LeftTab::Bookmarks => self.render_bookmarks(cx),
        };
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(width)
            .h_full()
            .border_end_1()
            .border_color(t.line)
            .child(header(&t).px(size::S1).children(tabs.into_iter().map(
                |(which, glyph, label, keys)| {
                    let mut button =
                        IconButton::new(SharedString::from(format!("left-{label}")), glyph, label)
                            .active(which == tab)
                            .tab_stop(false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.layout.left_tab = which;
                                cx.notify();
                            }));
                    if !keys.is_empty() {
                        button = button.shortcut(crate::actions::shortcut(keys));
                    }
                    div()
                        .relative()
                        .flow_row()
                        .items_center()
                        .h_full()
                        .px(px(4.))
                        .child(button)
                        .when(which == tab, |this| this.child(active_bar(&t)))
                },
            )))
            .child(div().flex_1().min_h_0().child(content))
    }

    fn render_files(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        if let Some(error) = &self.vault_error {
            return div()
                .flex()
                .flex_col()
                .gap(size::S3)
                .p(size::S4)
                .child(div().ui_sm().text_color(t.danger).child(error.clone()))
                .child(
                    Button::new("open-vault", focus_i18n::t("ui-vault-open-folder"))
                        .on_click(cx.listener(|this, _, window, cx| this.prompt_vault(window, cx))),
                )
                .into_any_element();
        }
        if self.library.is_none() {
            if self.vault_root.is_some() {
                return empty_state(&t, focus_i18n::t("ui-vault-loading")).into_any_element();
            }
            return div()
                .flex()
                .flex_col()
                .gap(size::S3)
                .p(size::S4)
                .child(
                    div()
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(focus_i18n::t("ui-vault-none")),
                )
                .child(
                    Button::new("open-vault", focus_i18n::t("ui-vault-open-folder"))
                        .on_click(cx.listener(|this, _, window, cx| this.prompt_vault(window, cx))),
                )
                .into_any_element();
        }
        match &self.vault_views {
            Some(views) => views.tree.clone().into_any_element(),
            None => empty_state(&t, focus_i18n::t("ui-vault-loading")).into_any_element(),
        }
    }

    fn render_bookmarks(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        match &self.vault_views {
            Some(views) => views.bookmarks.clone().into_any_element(),
            None => empty_state(&t, focus_i18n::t("ui-vault-none")).into_any_element(),
        }
    }

    fn render_right(
        &mut self,
        width: gpui::Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = *cx.tokens();
        let ops = self.prefs.read(cx).bool("ops.enabled");
        let tab = match self.layout.right_tab {
            RightTab::Ai if !self.prefs.read(cx).bool("ai.enabled") => RightTab::Outline,
            tab => tab,
        };
        let mut tabs = vec![
            (RightTab::Outline, focus_i18n::t("ui-right-outline")),
            (RightTab::Links, focus_i18n::t("ui-right-links")),
            (RightTab::Info, focus_i18n::t("ui-right-info")),
        ];
        if self.prefs.read(cx).bool("ai.enabled") {
            tabs.push((RightTab::Ai, focus_i18n::t("ui-right-ai")));
        }
        if ops {
            tabs.push((RightTab::Ops, focus_i18n::t("ui-right-ops")));
        }
        let whole: Option<AnyElement> = match tab {
            RightTab::Ai => self.ai.clone().map(|panel| panel.into_any_element()),
            RightTab::Ops => self
                .ops
                .as_ref()
                .map(|ops| ops.sidebar.clone().into_any_element()),
            RightTab::Links => Some(self.render_links(cx)),
            RightTab::Outline => Some(self.render_outline(cx)),
            _ => None,
        };
        let content: AnyElement = match tab {
            RightTab::Outline => gpui::Empty.into_any_element(),
            RightTab::Links => {
                empty_state(&t, focus_i18n::t("ui-links-unavailable")).into_any_element()
            }
            RightTab::Info => self.render_info(cx),
            RightTab::Ai => div()
                .flex()
                .flex_col()
                .gap(size::S3)
                .child(empty_state(&t, focus_i18n::t("ui-ai-no-key")))
                .child(
                    div().px(size::S4).child(
                        Button::new("ai-settings", focus_i18n::t("ui-ai-open-settings"))
                            .icon(Icon::SETTINGS)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_settings(Section::Ai, window, cx)
                            })),
                    ),
                )
                .into_any_element(),
            RightTab::Ops => empty_state(&t, focus_i18n::t("ui-ops-empty")).into_any_element(),
        };
        let strip_focused = self.right_strip.is_focused(window);
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(width)
            .h_full()
            .border_start_1()
            .border_color(t.line)
            .child(
                header(&t)
                    .id("right-tabs")
                    .track_focus(&self.right_strip)
                    .on_key_down(strip_keys(false, cx))
                    .min_w_0()
                    .px(size::S1)
                    .children(tabs.into_iter().map(|(which, label)| {
                        text_tab(
                            &t,
                            SharedString::from(format!("right-{label}")),
                            label,
                            (which == tab, strip_focused),
                            cx.listener(move |this, _, _, cx| {
                                this.layout.right_tab = which;
                                cx.notify();
                            }),
                        )
                    })),
            )
            .child(match whole {
                Some(view) => div().flex_1().min_h_0().child(view).into_any_element(),
                None => div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("right-content")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.right_scroll)
                            .child(content),
                    )
                    .child(focus_kit::scrollbar(
                        "right-scrollbar",
                        self.right_scroll.clone(),
                    ))
                    .into_any_element(),
            })
    }

    fn render_links(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        let (Some(library), Some(note)) = (self.library.clone(), self.last_note(cx)) else {
            return empty_state(&t, focus_i18n::t("ui-info-no-note")).into_any_element();
        };
        let backlinks = library.vault().backlinks(note);
        let outgoing = library.vault().links_of(note);
        let section = |title: String, count: usize| {
            div()
                .flow_row()
                .items_center()
                .gap(size::S2)
                .px(size::S4)
                .pt(size::S4)
                .pb(size::S1)
                .ui_xs()
                .text_color(t.ink_3)
                .child(title)
                .child(
                    div()
                        .mono_sm()
                        .child(focus_i18n::format_number(count as i64)),
                )
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        rows.push(section(focus_i18n::t("ui-links-backlinks"), backlinks.len()).into_any_element());
        for (i, link) in backlinks.iter().enumerate() {
            let (source, line) = (link.source, link.line);
            let row = match library.note(source) {
                Some(meta) => TreeRow::new(("backlink", i), meta.title.to_string())
                    .icon(Icon::FILE_TEXT)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_note(source, false, window, cx);
                        this.pending_line = Some((source, line));
                    })),
                None => {
                    let Some(path) = library.vault().source_path(source) else {
                        continue;
                    };
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let absolute = library.vault().absolute(&path);
                    TreeRow::new(("backlink", i), name)
                        .icon(Icon::FILE)
                        .on_click(cx.listener(move |this, _, _, cx| this.open_file(&absolute, cx)))
                }
            };
            rows.push(row.into_any_element());
            rows.push(
                div()
                    .pad_start(px(42.))
                    .pad_end(size::S3)
                    .pb(size::S1)
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(focus_kit::ellipsis(link.context.clone()))
                    .into_any_element(),
            );
        }
        rows.extend(self.unlinked_mention_rows(&library, note, &t, cx));
        rows.push(section(focus_i18n::t("ui-links-outgoing"), outgoing.len()).into_any_element());
        for (i, (link, target)) in outgoing.iter().enumerate() {
            let (label, id) = match target {
                focus_core::LinkTarget::Note { id, .. } => (
                    library
                        .note(*id)
                        .map(|m| m.title.to_string())
                        .unwrap_or_else(|| link.target.clone()),
                    Some(*id),
                ),
                _ => (link.target.clone(), None),
            };
            let row = TreeRow::new(("outgoing", i), label).icon(if id.is_some() {
                Icon::FILE_TEXT
            } else {
                Icon::UNLINK
            });
            rows.push(match id {
                Some(id) => row
                    .on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.open_note(id, false, window, cx)
                        }),
                    )
                    .into_any_element(),
                None => row.into_any_element(),
            });
        }
        let local = self.local_graph.clone().map(|graph| {
            div()
                .flex_none()
                .h(px(240.))
                .border_b_1()
                .border_color(t.line)
                .child(graph)
                .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .size_full()
            .children(local)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("links-content")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.right_scroll)
                            .pb(size::S4)
                            .children(rows),
                    )
                    .child(focus_kit::scrollbar(
                        "links-scrollbar",
                        self.right_scroll.clone(),
                    )),
            )
            .into_any_element()
    }

    fn render_outline(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(outline) = self.render_file_outline(cx) {
            return outline;
        }
        let t = *cx.tokens();
        let (editor, reading, view) = match (self.active_editor().cloned(), self.active_base_view())
        {
            (Some(editor), _) => {
                let note = editor.read(cx);
                let (reading, view) = (note.reading.clone(), note.view);
                (Some(editor), reading, view)
            }
            (None, Some(page)) => (None, page, ViewMode::Reading),
            (None, None) => {
                return empty_state(&t, focus_i18n::t("ui-info-no-note")).into_any_element();
            }
        };
        let Some(built) = reading.read(cx).built().cloned() else {
            return empty_state(&t, focus_i18n::t("ui-note-loading")).into_any_element();
        };
        if built.headings.is_empty() {
            return empty_state(&t, focus_i18n::t("ui-outline-empty")).into_any_element();
        }
        let current = match (&editor, view) {
            (Some(editor), ViewMode::Source) => {
                let line = editor.read(cx).cursor(cx).line;
                built.headings.iter().rposition(|h| h.line <= line)
            }
            _ => reading
                .read(cx)
                .top_item()
                .and_then(|top| built.headings.iter().rposition(|h| h.item <= top)),
        };
        let top = built.headings.iter().map(|h| h.level).min().unwrap_or(1);
        let rows = built.headings.len();
        let list = gpui::uniform_list("outline", rows, move |range, _, _| {
            range
                .map(|i| {
                    let heading = &built.headings[i];
                    let (line, item) = (heading.line, heading.item);
                    let editor = editor.clone();
                    let reading = reading.clone();
                    TreeRow::new(("heading", i), heading.text.clone())
                        .depth((heading.level - top) as usize)
                        .active(current == Some(i))
                        .on_click(move |_, window, cx| {
                            let Some(editor) = &editor else {
                                reading.update(cx, |reading, cx| {
                                    reading.scroll_to_item(item);
                                    cx.notify();
                                });
                                return;
                            };
                            reading.update(cx, |reading, cx| {
                                reading.scroll_to_line(line);
                                cx.notify();
                            });
                            if view != ViewMode::Reading {
                                editor.update(cx, |editor, cx| editor.go_to_line(line, window, cx));
                            }
                        })
                })
                .collect::<Vec<_>>()
        })
        .size_full()
        .py(size::S1)
        .track_scroll(self.outline_scroll.clone());
        div()
            .relative()
            .size_full()
            .child(list)
            .child(focus_kit::scrollbar(
                "outline-scrollbar",
                self.outline_scroll.clone(),
            ))
            .into_any_element()
    }

    fn render_info(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        let Some(editor) = self.active_editor() else {
            return empty_state(&t, focus_i18n::t("ui-info-no-note")).into_any_element();
        };
        let meta = editor.read(cx).meta.clone();
        let modified = meta
            .modified
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| focus_i18n::format_datetime(d.as_secs() as i64))
            .unwrap_or_default();
        let row = |label: String, value: String, mono: bool| {
            div()
                .flow_row()
                .items_start()
                .gap(size::S3)
                .px(size::S4)
                .py(size::S1)
                .child(
                    div()
                        .w(px(72.))
                        .flex_none()
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(focus_kit::truncated(
                            ElementId::Name(format!("info-key-{label}").into()),
                            label,
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .ui_sm()
                        .when(mono, |d| d.mono_sm().line_height(px(18.)))
                        .text_color(t.ink_2)
                        .child(value),
                )
        };
        div()
            .flex()
            .flex_col()
            .py(size::S2)
            .child(row(
                focus_i18n::t("ui-info-path"),
                meta.path.display().to_string(),
                true,
            ))
            .child(row(
                focus_i18n::t("ui-info-words"),
                focus_i18n::format_number(i64::from(meta.words)),
                true,
            ))
            .child(row(
                focus_i18n::t("ui-info-size"),
                focus_i18n::format_bytes(meta.size),
                true,
            ))
            .child(row(focus_i18n::t("ui-info-modified"), modified, true))
            .child(row(
                focus_i18n::t("ui-info-headings"),
                focus_i18n::format_number(meta.headings.len() as i64),
                true,
            ))
            .when(!meta.props.is_empty(), |this| {
                this.child(
                    div()
                        .px(size::S4)
                        .pt(size::S4)
                        .pb(size::S2)
                        .ui_xs()
                        .text_color(t.ink_3)
                        .child(focus_i18n::t("ui-info-properties")),
                )
                .children(
                    meta.props
                        .iter()
                        .map(|(key, value)| row(key.clone(), prop_text(value), false)),
                )
            })
            .into_any_element()
    }

    fn render_bottom(&mut self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let strip_focused = self.bottom_strip.is_focused(window);
        let tab = self.layout.bottom_tab;
        let tabs = [
            (BottomTab::Journal, focus_i18n::t("ui-bottom-journal")),
            (BottomTab::Tasks, focus_i18n::t("ui-bottom-tasks")),
            (BottomTab::Broken, focus_i18n::t("ui-bottom-broken")),
        ];
        let text = match tab {
            BottomTab::Journal => focus_i18n::t("ui-journal-empty"),
            BottomTab::Tasks => focus_i18n::t("ui-tasks-unavailable"),
            BottomTab::Broken => focus_i18n::t("ui-broken-unavailable"),
        };
        div()
            .flex()
            .flex_col()
            .flex_none()
            .h(self.layout.bottom_h)
            .border_t_1()
            .border_color(t.line)
            .child(
                header(&t)
                    .id("bottom-tabs")
                    .track_focus(&self.bottom_strip)
                    .on_key_down(strip_keys(true, cx))
                    .px(size::S1)
                    .children(tabs.into_iter().map(|(which, label)| {
                        text_tab(
                            &t,
                            SharedString::from(format!("bottom-{label}")),
                            label,
                            (which == tab, strip_focused),
                            cx.listener(move |this, _, _, cx| {
                                this.layout.bottom_tab = which;
                                cx.notify();
                            }),
                        )
                    }))
                    .child(div().flex_1())
                    .child(
                        IconButton::new(
                            "bottom-close",
                            Icon::CLOSE,
                            focus_i18n::t("ui-panel-close"),
                        )
                        .shortcut(crate::actions::shortcut("ui-cmd-toggle-bottom"))
                        .dense(true)
                        .tab_stop(false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.layout.bottom_open = false;
                            cx.notify();
                        })),
                    ),
            )
            .child(match (tab, &self.ops, &self.vault_views) {
                (BottomTab::Journal, Some(ops), _) => div()
                    .flex_1()
                    .min_h_0()
                    .child(ops.journal.clone())
                    .into_any_element(),
                (BottomTab::Tasks, _, Some(views)) => div()
                    .flex_1()
                    .min_h_0()
                    .child(views.tasks.clone())
                    .into_any_element(),
                (BottomTab::Broken, _, Some(views)) => div()
                    .flex_1()
                    .min_h_0()
                    .child(views.broken.clone())
                    .into_any_element(),
                _ => empty_state(&t, text).into_any_element(),
            })
    }
}
