use std::ops::Range;

use focus_kit::{Dir, Icon, IconButton, Tokens, Type, size};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use super::ReadingView;
use super::live::{InnerSlot, LiveKind, LiveOpen};
use crate::model::{Node, TabNode};
use crate::text::Base;

impl ReadingView {
    pub(super) fn selected_tab(&self, item: usize, key: u32, tabs: &[TabNode]) -> usize {
        self.tab_titles
            .iter()
            .find_map(|title| {
                tabs.iter()
                    .position(|tab| !tab.title.is_empty() && tab.title == *title)
            })
            .or_else(|| self.tab_index.get(&(item, key)).copied())
            .unwrap_or(0)
            .min(tabs.len().saturating_sub(1))
    }

    pub fn select_tab(&mut self, title: &str, cx: &mut Context<Self>) {
        self.tab_titles.retain(|t| t != title);
        self.tab_titles.insert(0, title.to_string());
        self.relayout_below(None, cx);
        cx.emit(super::FoldsChanged);
    }

    fn choose_tab(
        &mut self,
        item: usize,
        key: u32,
        index: usize,
        title: &str,
        cx: &mut Context<Self>,
    ) {
        self.tab_index.insert((item, key), index);
        if !title.is_empty() {
            self.tab_titles.retain(|t| t != title);
            self.tab_titles.insert(0, title.to_string());
        }
        self.relayout_below(None, cx);
        cx.emit(super::FoldsChanged);
    }

    pub(super) fn reveal_tab(&mut self, item: usize, line: u32) {
        let Some(built) = self.built.clone() else {
            return;
        };
        let Some(entry) = built.items.get(item) else {
            return;
        };
        let mut node = &entry.node;
        while let Node::Tabs { key, tabs } = node {
            let Some(index) = tabs.iter().position(|tab| tab.lines.contains(&line)) else {
                return;
            };
            if self.selected_tab(item, *key, tabs) != index {
                self.tab_index.insert((item, *key), index);
                let title = &tabs[index].title;
                if !title.is_empty() {
                    self.tab_titles.retain(|t| t != title);
                    self.tab_titles.insert(0, title.clone());
                }
                self.recompute();
                self.list.reset(self.rows.len() + 1);
            }
            match tabs[index].items.iter().find(|i| i.lines.contains(&line)) {
                Some(inner) => node = &inner.node,
                None => return,
            }
        }
    }

    pub(super) fn inner_block(
        &self,
        item: usize,
        lines: Range<u32>,
        body: AnyElement,
        t: &Tokens,
    ) -> AnyElement {
        let bounds = self.inner_bounds.clone();
        let group: SharedString = format!("live-inner-{item}-{}", lines.start).into();
        div()
            .id(group.clone())
            .group(group.clone())
            .relative()
            .child(body)
            .when(self.live && !lines.is_empty(), |this| {
                this.child(self.hover_rule(group, 0., 0., 0., t.line_2))
            })
            .child(
                gpui::canvas(
                    |_, _, _| {},
                    move |b, _, _, _| {
                        bounds
                            .borrow_mut()
                            .entry(item)
                            .or_default()
                            .push((lines.clone(), b))
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_tabs(
        &self,
        item: usize,
        key_item: u32,
        key: u32,
        tabs: &[TabNode],
        lines: Option<Range<u32>>,
        base: &Base,
        t: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.selected_tab(item, key, tabs);
        let dir = Dir::current();
        let id = (item << 16) | key as usize;
        let focus = self
            .tab_focus
            .borrow_mut()
            .entry((item, key))
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let titles: Vec<String> = tabs.iter().map(|tab| tab.title.clone()).collect();
        let group: SharedString = format!("tabs-{id}").into();
        let strip = dir
            .flow(div())
            .id(("tabs-strip", id))
            .track_focus(&focus)
            .flex_none()
            .h(px(32.))
            .border_b_1()
            .border_color(t.line)
            .overflow_x_scroll()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener({
                let titles = titles.clone();
                move |this, event: &KeyDownEvent, _, cx| {
                    let forward = match event.keystroke.key.as_str() {
                        "right" => !Dir::current().rtl,
                        "left" => Dir::current().rtl,
                        _ => return,
                    };
                    let count = titles.len();
                    let next = if forward {
                        (selected + 1) % count
                    } else {
                        (selected + count - 1) % count
                    };
                    this.choose_tab(item, key, next, &titles[next], cx);
                    cx.stop_propagation();
                }
            }))
            .children(tabs.iter().enumerate().map(|(i, tab)| {
                let on = i == selected;
                let title = if tab.title.is_empty() {
                    focus_i18n::t_args(
                        "reading-tab-untitled",
                        &[("number", ((i + 1) as i64).into())],
                    )
                } else {
                    tab.title.clone()
                };
                let chosen = tab.title.clone();
                let focus = focus.clone();
                div()
                    .id(("tab-title", (id << 8) | i))
                    .relative()
                    .flex_none()
                    .flex()
                    .items_center()
                    .h_full()
                    .px(size::S3)
                    .ui_sm()
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .text_color(if on { t.ink } else { t.ink_3 })
                    .when(!on, |this| this.hover(move |s| s.text_color(t.ink_2)))
                    .child(title)
                    .when(on, |this| {
                        this.child(Dir::across(div().absolute()).bottom_0().h(px(1.)).bg(t.ink))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.focus(&focus);
                        this.choose_tab(item, key, i, &chosen, cx);
                    }))
            }))
            .child(div().flex_1())
            .when_some(lines.filter(|_| self.live), |this, lines| {
                this.child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .px(size::S2)
                        .invisible()
                        .group_hover(group.clone(), |s| s.visible())
                        .child(
                            IconButton::new(
                                ("tabs-source", id),
                                Icon::CODE,
                                focus_i18n::t("reading-tabs-edit-source"),
                            )
                            .tab_stop(false)
                            .dense(true)
                            .on_click(cx.listener(
                                move |_, _, _, cx| {
                                    cx.emit(LiveOpen {
                                        kind: LiveKind::Block,
                                        lines: lines.clone(),
                                        line: lines.start,
                                        column: 0,
                                    });
                                },
                            )),
                        ),
                )
            });
        let mut editor_drawn = false;
        let content = tabs.get(selected).map(|tab| {
            tab.items
                .iter()
                .filter_map(|inner| {
                    match self.inner_slot(
                        &inner.lines,
                        super::live::is_container(&inner.node),
                        &mut editor_drawn,
                    ) {
                        InnerSlot::Editor => Some(self.open_editor(t)),
                        InnerSlot::Hidden => None,
                        InnerSlot::Block => {
                            let body = match &inner.node {
                                Node::Tabs { key, tabs } => self.render_tabs(
                                    item,
                                    key_item,
                                    *key,
                                    tabs,
                                    Some(inner.lines.clone()),
                                    base,
                                    t,
                                    window,
                                    cx,
                                ),
                                node => self.render_node(node, key_item, item, base, t, window, cx),
                            };
                            Some(self.inner_block(item, inner.lines.clone(), body, t))
                        }
                    }
                })
                .collect::<Vec<_>>()
        });
        div()
            .id(("tabs", id))
            .group(group)
            .flex()
            .flex_col()
            .border_1()
            .border_color(t.line)
            .bg(t.bg)
            .child(strip)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px((self.typesetting.base * 0.8).round()))
                    .p(size::S4)
                    .children(content.unwrap_or_default()),
            )
            .into_any_element()
    }
}
