use std::rc::Rc;

use focus_kit::Flow as _;
use focus_kit::segments::{Segment, Segments};
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Checkbox, Chip, Dir, Icon, Tokens, Type, icon, size,
};
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window, div, img,
    prelude::FluentBuilder, px,
};

use super::{ReadingEvent, ReadingView};
use crate::media::Slot;
use crate::model::{
    Built, CalloutIcon, EmbedBody, Family, HtmlPart, ImageRef, ImageSource, LinkGoal, Node, Rich,
    decorated,
};
use crate::text::Base;

fn dir_of(nodes: &[Node]) -> Dir {
    fn first(node: &Node) -> Option<&str> {
        match node {
            Node::Paragraph(rich) | Node::Heading { rich, .. } => Some(&rich.text),
            Node::List { entries, .. } => entries
                .iter()
                .flat_map(|e| e.children.iter())
                .find_map(first),
            Node::Quote(children) => children.iter().find_map(first),
            Node::Callout {
                title, children, ..
            } => children.iter().find_map(first).or(Some(&title.text)),
            _ => None,
        }
    }
    match nodes.iter().find_map(first) {
        Some(text) => Dir::current().of_text(text),
        None => Dir::current(),
    }
}

impl ReadingView {
    pub(super) fn render_row(
        &mut self,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = *cx.tokens();
        let Some(built) = self.built.clone() else {
            return div().into_any_element();
        };
        if row == 0 {
            if self.compact {
                return div().into_any_element();
            }
            return div()
                .w_full()
                .px(px(self.gutter()))
                .child(self.render_header(&built, &t, cx))
                .into_any_element();
        }
        let Some(&super::Row {
            item,
            part,
            parts,
            editor,
        }) = self.rows.get(row - 1)
        else {
            return div().into_any_element();
        };
        if editor {
            let gap = px((self.typesetting.base * 0.8).round());
            return div()
                .w_full()
                .px(px(self.gutter()))
                .pb(gap)
                .child(self.open_editor(&t))
                .into_any_element();
        }
        let entry = &built.items[item];
        self.inner_bounds.borrow_mut().remove(&item);
        self.entry_bounds.borrow_mut().remove(&item);
        let key_item = item as u32 + 1;
        let base = self.base(&t);
        if let Node::Heading { level, rich } = &entry.node {
            return div()
                .id(("heading-row", item))
                .group(SharedString::from(format!("heading-{item}")))
                .w_full()
                .px(px(self.gutter()))
                .child(self.render_heading(item, *level, rich, &built, &t))
                .into_any_element();
        }
        let gap = px((self.typesetting.base * 0.8).round());
        let gutter = if matches!(entry.node, Node::Mermaid { .. }) {
            px(0.)
        } else {
            px(self.gutter())
        };
        if parts > 1
            && let Node::Code {
                label,
                lang,
                detected,
                line,
                code,
            } = &entry.node
        {
            let body = self.render_code(
                key_item,
                item,
                label,
                lang,
                *detected,
                *line,
                code,
                Some((part, parts)),
                &t,
                cx,
            );
            return div()
                .w_full()
                .px(gutter)
                .when(part + 1 == parts, |this| this.pb(gap))
                .child(body)
                .into_any_element();
        }
        let body = match &entry.node {
            Node::Tabs { key, tabs } => self.render_tabs(
                item,
                key_item,
                *key,
                tabs,
                Some(entry.lines.clone()),
                &base,
                &t,
                window,
                cx,
            ),
            node => self.render_node(node, key_item, item, &base, &t, window, cx),
        };
        let above = super::inplace::above::render(self, item, &t, window, cx);
        div()
            .w_full()
            .px(gutter)
            .pb(gap)
            .children(above)
            .child(body)
            .into_any_element()
    }

    fn heading_base(&self, level: u8, t: &Tokens) -> Base {
        if self.compact {
            return Base {
                weight: FontWeight::SEMIBOLD,
                color: if level <= 4 { t.ink } else { t.ink_2 },
                ..self.base(t)
            };
        }
        let display: SharedString = focus_kit::typography::DISPLAY.into();
        let text = self.typesetting.text_font.clone();
        let (family, size_px, line, color) = match level {
            1 => (display, 26., 32., t.ink),
            2 => (display, 20., 28., t.ink),
            3 => (text, 16.5, 24., t.ink),
            4 => (text, 14.5, 22., t.ink),
            _ => (text, 14.5, 22., t.ink_2),
        };
        Base {
            family,
            size: px(size_px),
            line_height: px(line),
            color,
            weight: FontWeight::SEMIBOLD,
            ..self.base(t)
        }
    }

    pub(super) fn render_heading(
        &self,
        item: usize,
        level: u8,
        rich: &Rich,
        built: &Built,
        t: &Tokens,
    ) -> AnyElement {
        let base = self.heading_base(level, t);
        if self.compact {
            return div()
                .w_full()
                .pt(size::S3)
                .pb(size::S1)
                .child(self.text(item as u32 + 1, rich, &base, t))
                .into_any_element();
        }
        let line = f32::from(base.line_height);
        let (top, bottom) = heading_space(level);
        let folded = self.folded.contains(&item);
        let hidden_lines = built
            .headings
            .iter()
            .position(|h| !h.inner && h.item == item)
            .and_then(|h| {
                let end = built.section_end[h];
                let start = built.items.get(item + 1)?.lines.start;
                let stop = built
                    .items
                    .get(end)
                    .map_or(built.items.last()?.lines.end, |i| i.lines.start);
                Some(stop.saturating_sub(start))
            })
            .unwrap_or(0);
        let group: SharedString = format!("heading-{item}").into();
        let dir = Dir::current().of_text(&rich.text);
        let outside = self.gutter() >= FOLD_ROOM;
        let toggle = div()
            .absolute()
            .top(px(top))
            .w(px(FOLD_ROOM))
            .h(px(line))
            .flow_row()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .child(
                div()
                    .id(("fold", item))
                    .size(px(18.))
                    .flow_row()
                    .items_center()
                    .justify_center()
                    .rounded(size::RADIUS)
                    .text_color(t.ink_3)
                    .when(!folded, |this| {
                        this.invisible().group_hover(group.clone(), |s| s.visible())
                    })
                    .hover(move |s| s.bg(t.card_2))
                    .child(icon(
                        if folded {
                            dir.forward()
                        } else {
                            Icon::CHEVRON_DOWN
                        },
                        size::ICON_DENSE,
                    )),
            );
        let toggle = if outside {
            dir.start(toggle, px(-FOLD_ROOM))
        } else {
            dir.end(toggle, px(-FOLD_ROOM))
        };
        div()
            .id(("heading", item))
            .relative()
            .when(!outside, |this| dir.pad_end(this, px(FOLD_ROOM)))
            .w_full()
            .pt(px(top))
            .pb(px(bottom))
            .when(level == 2, |this| {
                this.child(
                    div()
                        .absolute()
                        .start_at(px(0.))
                        .end_at(px(0.))
                        .top(px(top - 12. - 1.))
                        .h(px(1.))
                        .bg(t.line),
                )
            })
            .child(
                dir.flow(div())
                    .items_start()
                    .gap(size::S2)
                    .child(div().flex_1().min_w_0().child(self.text(
                        item as u32 + 1,
                        rich,
                        &base,
                        t,
                    )))
                    .when(folded, |this| {
                        this.child(Chip::new().key(focus_i18n::t_args(
                            "reading-lines-hidden",
                            &[("count", (hidden_lines as i64).into())],
                        )))
                    }),
            )
            .child(self.zone(toggle, super::Zone::Fold(item)))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_node(
        &self,
        node: &Node,
        key_item: u32,
        item: usize,
        base: &Base,
        t: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node {
            Node::Tabs { key, tabs } => {
                self.render_tabs(item, key_item, *key, tabs, None, base, t, window, cx)
            }
            Node::Ref(rich) => div()
                .flow_row()
                .items_center()
                .gap(size::S2)
                .child(
                    div()
                        .flex_none()
                        .text_color(t.ink_3)
                        .child(icon(Icon::LINK, px(14.))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(self.text(key_item, rich, base, t)),
                )
                .into_any_element(),
            Node::Directive(text) => div()
                .mono_sm()
                .text_color(t.ink_4)
                .child(text.clone())
                .into_any_element(),
            Node::Base {
                caption,
                sections,
                view,
            } => {
                self.share_columns(item, sections, *view, base, t, window);
                div()
                    .flex()
                    .flex_col()
                    .gap(size::S2)
                    .children(sections.iter().enumerate().map(|(at, section)| {
                        let (top, bottom) = heading_space(3);
                        div()
                            .flex()
                            .flex_col()
                            .when_some(section.title.as_ref(), |this, title| {
                                this.child(
                                    div()
                                        .when(at > 0, |this| this.pt(px(top - 8.)))
                                        .pb(px(bottom))
                                        .child(self.text(
                                            key_item,
                                            title,
                                            &self.heading_base(3, t),
                                            t,
                                        )),
                                )
                            })
                            .child(self.render_node(
                                &section.table,
                                key_item,
                                item,
                                base,
                                t,
                                window,
                                cx,
                            ))
                    }))
                    .when(!caption.text.is_empty(), |this| {
                        this.child(self.text(
                            key_item,
                            caption,
                            &Base {
                                family: self.typesetting.text_font.clone(),
                                size: px(12.5),
                                line_height: px(18.),
                                color: t.ink_3,
                                ..base.clone()
                            },
                            t,
                        ))
                    })
                    .into_any_element()
            }
            Node::Heading { rich, .. } => self.text(
                key_item,
                rich,
                &Base {
                    weight: FontWeight::SEMIBOLD,
                    color: t.ink,
                    ..base.clone()
                },
                t,
            ),
            Node::Paragraph(rich) => self.text(key_item, rich, base, t),
            Node::List {
                tight,
                entries,
                ordered,
            } => {
                let spacing = if *tight { px(2.) } else { px(8.) };
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing)
                    .children(entries.iter().map(|entry| {
                        let done = matches!(entry.task, Some('x' | 'X'));
                        let cancelled = entry.task == Some('-');
                        let line = entry.line;
                        let dir = match entry.children.first() {
                            Some(Node::Paragraph(rich)) => {
                                focus_kit::Dir::current().of_text(&rich.text)
                            }
                            _ => focus_kit::Dir::current(),
                        };
                        let marker: AnyElement = match entry.task {
                            Some(state) => dir
                                .flow(div())
                                .h(base.line_height)
                                .items_center()
                                .child(
                                    self.zone(
                                        div().flex_none().cursor_pointer().child(
                                            Checkbox::new(("task", line as usize), done)
                                                .forced(focus_kit::Forced::Live),
                                        ),
                                        super::Zone::Task(line),
                                    ),
                                )
                                .when(state == '/', |this| {
                                    this.child(div().absolute().size(px(6.)).bg(t.ink_2))
                                })
                                .into_any_element(),
                            None => dir
                                .flow(div())
                                .h(base.line_height)
                                .items_center()
                                .justify_end()
                                .w(if *ordered { px(22.) } else { px(14.) })
                                .text_size(base.size)
                                .text_color(t.ink_3)
                                .child(entry.marker.clone())
                                .into_any_element(),
                        };
                        let child_base = if done || cancelled {
                            Base {
                                color: t.ink_3,
                                ..base.clone()
                            }
                        } else {
                            base.clone()
                        };
                        let drawn = self.entry_bounds.clone();
                        let record = gpui::canvas(
                            move |b, _, _| {
                                drawn.borrow_mut().entry(item).or_default().push((line, b))
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full();
                        dir.flow(div())
                            .relative()
                            .gap(px(10.))
                            .child(record)
                            .child(div().flex_none().relative().child(marker))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(spacing)
                                    .when(done || cancelled, |this| this.line_through())
                                    .children(entry.children.iter().map(|n| {
                                        self.render_node(
                                            n,
                                            key_item,
                                            item,
                                            &child_base,
                                            t,
                                            window,
                                            cx,
                                        )
                                    })),
                            )
                    }))
                    .into_any_element()
            }
            Node::Code {
                label,
                lang,
                detected,
                line,
                code,
            } => self.render_code(
                key_item, item, label, lang, *detected, *line, code, None, t, cx,
            ),
            Node::Mermaid { source } => self.render_mermaid(source, item, t, window, cx),
            Node::Query(block) => self.render_query(block, item, t, cx),
            Node::Math(rich) => div()
                .px(size::S4)
                .py(size::S3)
                .bg(t.bg_2)
                .border_1()
                .border_color(t.line)
                .child(self.text(
                    key_item,
                    rich,
                    &Base {
                        family: base.mono.clone(),
                        size: px(13.),
                        line_height: px(21.),
                        ..base.clone()
                    },
                    t,
                ))
                .into_any_element(),
            Node::Quote(children) => {
                let dir = dir_of(children);
                dir.bar_start(
                    dir.pad_start(div().flex().flex_col().gap(size::S2), size::S4),
                    t.line_2,
                )
                .children(children.iter().map(|n| {
                    self.render_node(
                        n,
                        key_item,
                        item,
                        &Base {
                            color: t.ink_2,
                            ..base.clone()
                        },
                        t,
                        window,
                        cx,
                    )
                }))
                .into_any_element()
            }
            Node::Callout {
                key,
                family,
                icon: glyph,
                title,
                fold,
                children,
            } => {
                let dir = dir_of(std::slice::from_ref(node));
                let tone = match family {
                    Family::Neutral => t.ink_2,
                    Family::Ok => t.ok,
                    Family::Warn => t.warn,
                    Family::Danger => t.danger,
                };
                let state_key = (item, *key);
                let open = self
                    .callouts
                    .get(&state_key)
                    .copied()
                    .unwrap_or(fold.unwrap_or(true));
                let foldable = fold.is_some();
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .bg(t.bg_2)
                    .border_1()
                    .border_color(t.line)
                    .child(
                        dir.start(div().absolute(), px(-1.))
                            .top(px(-1.))
                            .bottom(px(-1.))
                            .w(px(2.))
                            .bg(tone),
                    )
                    .child(
                        dir.flow(div().id(("callout", (item << 16) | *key as usize)))
                            .items_center()
                            .gap(size::S2)
                            .min_h(px(32.))
                            .px(size::S4)
                            .when(foldable, |this| {
                                this.cursor_pointer()
                                    .hover(move |s| s.bg(t.card_2))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.callouts.insert(state_key, !open);
                                        let row = this.row_of(item).unwrap_or(0);
                                        this.list.splice(row..row + 1, 1);
                                        cx.emit(super::FoldsChanged);
                                        cx.notify();
                                    }))
                            })
                            .child(self.zone(
                                div().flex().flex_none().child(
                                    icon(callout_icon(*glyph), size::ICON_DENSE).text_color(tone),
                                ),
                                super::Zone::CalloutIcon {
                                    item,
                                    callout: *key,
                                },
                            ))
                            .child(div().flex_1().min_w_0().child(self.text(
                                key_item,
                                title,
                                &Base {
                                    family: self.typesetting.text_font.clone(),
                                    size: px(13.),
                                    line_height: px(20.),
                                    weight: FontWeight::SEMIBOLD,
                                    color: t.ink,
                                    ..base.clone()
                                },
                                t,
                            )))
                            .when(foldable, |this| {
                                this.child(
                                    icon(
                                        if open {
                                            Icon::CHEVRON_DOWN
                                        } else {
                                            dir.forward()
                                        },
                                        size::ICON_DENSE,
                                    )
                                    .text_color(t.ink_3),
                                )
                            }),
                    )
                    .when(open && !children.is_empty(), |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(size::S3)
                                .px(size::S4)
                                .pb(size::S3)
                                .children(children.iter().map(|n| {
                                    self.render_node(n, key_item, item, base, t, window, cx)
                                })),
                        )
                    })
                    .into_any_element()
            }
            Node::Details {
                key,
                summary,
                open,
                children,
                child_lines,
            } => {
                let state_key = (item, *key);
                let open = self.callouts.get(&state_key).copied().unwrap_or(*open);
                div()
                    .flex()
                    .flex_col()
                    .bg(t.bg_2)
                    .border_1()
                    .border_color(t.line)
                    .child(
                        div()
                            .id(("details", (item << 16) | *key as usize))
                            .flow_row()
                            .items_center()
                            .gap(size::S2)
                            .min_h(px(32.))
                            .px(size::S4)
                            .cursor_pointer()
                            .hover(move |s| s.bg(t.card_2))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.callouts.insert(state_key, !open);
                                let row = this.row_of(item).unwrap_or(0);
                                this.list.splice(row..row + 1, 1);
                                cx.emit(super::FoldsChanged);
                                cx.notify();
                            }))
                            .child(
                                icon(
                                    if open {
                                        Icon::CHEVRON_DOWN
                                    } else {
                                        focus_kit::Dir::current().forward()
                                    },
                                    size::ICON_DENSE,
                                )
                                .text_color(t.ink_3),
                            )
                            .child(div().flex_1().min_w_0().child(self.text(
                                key_item,
                                summary,
                                &Base {
                                    family: self.typesetting.text_font.clone(),
                                    size: px(13.),
                                    line_height: px(20.),
                                    weight: FontWeight::SEMIBOLD,
                                    color: t.ink,
                                    ..base.clone()
                                },
                                t,
                            ))),
                    )
                    .when(open && !children.is_empty(), |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(size::S3)
                                .px(size::S4)
                                .pb(size::S3)
                                .children({
                                    let mut editor_drawn = false;
                                    children
                                        .iter()
                                        .enumerate()
                                        .filter_map(|(i, n)| {
                                            let Some(lines) = child_lines.get(i) else {
                                                return Some(self.render_node(
                                                    n, key_item, item, base, t, window, cx,
                                                ));
                                            };
                                            match self.inner_slot(
                                                lines,
                                                super::live::is_container(n),
                                                &mut editor_drawn,
                                            ) {
                                                super::live::InnerSlot::Editor => {
                                                    Some(self.open_editor(t))
                                                }
                                                super::live::InnerSlot::Hidden => None,
                                                super::live::InnerSlot::Block => {
                                                    let body = self.render_node(
                                                        n, key_item, item, base, t, window, cx,
                                                    );
                                                    Some(self.inner_block(
                                                        item,
                                                        lines.clone(),
                                                        body,
                                                        t,
                                                    ))
                                                }
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                }),
                        )
                    })
                    .into_any_element()
            }
            Node::Table {
                align, head, rows, ..
            } => {
                let (head_base, cell_base) = self.table_bases(base, t);
                let first = super::table::first_ord(head, rows);
                let key = (item, first);
                let menu = super::inplace::table::menu(self, key, t, cx);
                let columns = self.columns(key, head, rows, &head_base, &cell_base, t, window);
                let room = self
                    .table_room
                    .borrow()
                    .get(&key)
                    .copied()
                    .unwrap_or_else(|| self.room());
                let widths = columns.widths(room);
                let total: f32 = widths.iter().sum();
                let cell = |rich: &Rich, column: usize, header: bool| {
                    let d = div()
                        .flex_none()
                        .w(px(widths
                            .get(column)
                            .copied()
                            .unwrap_or(super::table::CELL_PAD)))
                        .px(size::S3)
                        .py(size::S2);
                    let d = match align.get(column) {
                        Some(focus_md::Align::Right) => d.text_right(),
                        Some(focus_md::Align::Center) => d.text_center(),
                        _ => d,
                    };
                    d.child(self.text(
                        key_item,
                        rich,
                        if header { &head_base } else { &cell_base },
                        t,
                    ))
                };
                let rooms = self.table_room.clone();
                let dir = head
                    .iter()
                    .chain(rows.iter().flatten())
                    .find(|r| r.text.chars().any(char::is_alphabetic))
                    .map_or(Dir::current(), |r| Dir::current().of_text(&r.text));
                let row = || dir.flow(div()).w_full();
                let scroll = dir.rtl.then(|| {
                    let mut tables = self.table_scroll.borrow_mut();
                    let (handle, overflowing) = tables
                        .entry(key)
                        .or_insert_with(|| (gpui::ScrollHandle::new(), false));
                    let now = total > room;
                    if now && !*overflowing {
                        handle.set_offset(gpui::point(px(-1e6), px(0.)));
                    }
                    *overflowing = now;
                    handle.clone()
                });
                div()
                    .id(("table", (item << 16) | first as usize))
                    .flex()
                    .overflow_x_scroll()
                    .when_some(scroll, |this, handle| this.track_scroll(&handle))
                    .w_full()
                    .children(menu)
                    .child(
                        gpui::canvas(
                            move |bounds, window, _| {
                                let width = f32::from(bounds.size.width).floor();
                                let previous = rooms.borrow_mut().insert(key, width);
                                if previous.is_some_and(|p| (p - width).abs() >= 1.)
                                    || (previous.is_none() && (width - room).abs() >= 1.)
                                {
                                    window.refresh();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .when(dir.rtl, |this| this.ml_auto())
                            .flex_none()
                            .w(px(total))
                            .child(
                                row().border_b_1().border_color(t.line_2).children(
                                    head.iter().enumerate().map(|(i, r)| cell(r, i, true)),
                                ),
                            )
                            .children(rows.iter().enumerate().map(|(n, cells)| {
                                row()
                                    .id(("row", n))
                                    .border_b_1()
                                    .border_color(t.line)
                                    .hover(move |s| s.bg(t.bg_2))
                                    .children(
                                        cells.iter().enumerate().map(|(i, r)| cell(r, i, false)),
                                    )
                            })),
                    )
                    .into_any_element()
            }
            Node::Embed {
                label,
                goal,
                anchor,
                body,
            } => self.render_embed(
                label,
                *goal,
                anchor.clone(),
                body,
                key_item,
                item,
                base,
                t,
                window,
                cx,
            ),
            Node::Image(image) => match &image.source {
                ImageSource::Pdf(embed) => {
                    self.render_pdf_embed(embed, image.width, item, t, window, cx)
                }
                _ => self.render_image(image, t, cx),
            },
            Node::Rule => div()
                .py(size::S6)
                .child(div().h(px(1.)).bg(t.line))
                .into_any_element(),
            Node::Html(parts) => div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .children(parts.iter().map(|part| match part {
                    HtmlPart::Text(rich) => self.text(
                        key_item,
                        rich,
                        &Base {
                            family: base.mono.clone(),
                            size: px(13.),
                            line_height: px(21.),
                            color: t.ink_2,
                            ..base.clone()
                        },
                        t,
                    ),
                    HtmlPart::Image(image) => self.render_image(image, t, cx),
                }))
                .into_any_element(),
            Node::Footnotes(notes) => div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .pt(size::S6)
                .border_t_1()
                .border_color(t.line)
                .when_some(self.footnote_from, |this, from| {
                    this.child(
                        div().flow_row().child(control(
                            focus_kit::IconButton::new(
                                "footnote-back",
                                focus_kit::Dir::current().arrow_back(),
                                focus_i18n::t("reading-footnote-back"),
                            )
                            .tab_stop(false)
                            .dense(true)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.footnote_from = None;
                                    this.scroll_to_item(from);
                                    cx.notify();
                                },
                            )),
                        )),
                    )
                })
                .children({
                    let mut editor_drawn = false;
                    let small = Base {
                        size: px(13.),
                        line_height: px(20.),
                        color: t.ink_2,
                        ..base.clone()
                    };
                    notes
                        .iter()
                        .enumerate()
                        .filter_map(|(i, note)| {
                            match self.inner_slot(&note.lines, false, &mut editor_drawn) {
                                super::live::InnerSlot::Editor => Some(self.open_editor(t)),
                                super::live::InnerSlot::Hidden => None,
                                super::live::InnerSlot::Block => {
                                    let body = div()
                                        .flow_row()
                                        .gap(size::S2)
                                        .child(
                                            div()
                                                .flex_none()
                                                .w(px(24.))
                                                .mono_sm()
                                                .line_height(px(20.))
                                                .text_color(t.ink_3)
                                                .child(format!("{}.", i + 1)),
                                        )
                                        .child(div().flex().flex_col().flex_1().min_w_0().children(
                                            note.blocks.iter().map(|n| {
                                                self.render_node(
                                                    n, key_item, item, &small, t, window, cx,
                                                )
                                            }),
                                        ))
                                        .into_any_element();
                                    Some(self.inner_block(item, note.lines.clone(), body, t))
                                }
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .into_any_element(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_embed(
        &self,
        label: &Rich,
        goal: Option<focus_core::NoteId>,
        anchor: Option<String>,
        body: &EmbedBody,
        key_item: u32,
        item: usize,
        base: &Base,
        t: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = Base {
            family: self.typesetting.text_font.clone(),
            size: px(13.),
            line_height: px(20.),
            weight: FontWeight::SEMIBOLD,
            color: t.ink,
            ..base.clone()
        };
        let message = |key: &str| {
            div()
                .ui_sm()
                .text_color(t.ink_3)
                .child(focus_i18n::t(key))
                .into_any_element()
        };
        let content: Vec<AnyElement> = match body {
            EmbedBody::Content(nodes) => nodes
                .iter()
                .map(|n| self.render_node(n, key_item, item, base, t, window, cx))
                .collect(),
            EmbedBody::TitleOnly => vec![message("reading-embed-open")],
            EmbedBody::Missing => vec![message("reading-embed-missing")],
            EmbedBody::Cycle => vec![message("reading-embed-cycle")],
            EmbedBody::TooDeep => vec![message("reading-embed-depth")],
            EmbedBody::AnchorMissing(anchor) => vec![
                div()
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(focus_i18n::t_args(
                        "reading-embed-anchor-missing",
                        &[("anchor", anchor.as_str().into())],
                    ))
                    .into_any_element(),
            ],
            EmbedBody::Failed(reason) => vec![
                div()
                    .ui_sm()
                    .text_color(t.danger)
                    .child(reason.clone())
                    .into_any_element(),
            ],
        };
        let dir = match body {
            EmbedBody::Content(nodes) => dir_of(nodes),
            _ => Dir::current(),
        };
        dir.bar_start(
            dir.pad_start(
                div()
                    .id(("embed", (item << 16) | label.ord as usize))
                    .flex()
                    .flex_col()
                    .gap(size::S2)
                    .py(size::S2),
                size::S4,
            ),
            t.line_2,
        )
        .child(
            div()
                .id(("embed-title", (item << 16) | label.ord as usize))
                .when_some(goal, |this, id| {
                    this.cursor_pointer()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |_, event: &gpui::ClickEvent, _, cx| {
                            cx.emit(ReadingEvent::OpenLink {
                                goal: LinkGoal::Note {
                                    id,
                                    anchor: anchor.clone(),
                                },
                                new_tab: event.modifiers().control,
                            })
                        }))
                })
                .child(self.text(key_item, label, &title, t)),
        )
        .children(content)
        .into_any_element()
    }

    pub(super) fn decorated(
        &self,
        item: usize,
        lang: &str,
        code: &Rich,
    ) -> Option<Rc<super::DecoratedCode>> {
        let decoration = self.decoration.as_ref()?;
        self.decorated
            .borrow_mut()
            .entry((item, code.ord))
            .or_insert_with(|| {
                decoration.decorate(lang, &code.text).map(|d| {
                    let resolved = decorated(
                        &crate::model::code_rich(&d.resolved, lang, code.ord),
                        &d.resolved_spans,
                    );
                    let revealed = d.revealed.as_ref().map(|(text, spans)| {
                        decorated(&crate::model::code_rich(text, lang, code.ord), spans)
                    });
                    Rc::new(super::DecoratedCode {
                        raw: decorated(code, &d.spans),
                        resolved,
                        revealed,
                    })
                })
            })
            .clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_code(
        &self,
        key_item: u32,
        item: usize,
        label: &str,
        lang: &str,
        detected: bool,
        line: u32,
        code: &Rich,
        part: Option<(u32, u32)>,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let base = Base {
            family: self.typesetting.code_font.clone(),
            mono: self.typesetting.code_font.clone(),
            size: px(13.),
            line_height: px(21.),
            color: t.ink_body,
            weight: FontWeight::NORMAL,
            italic: false,
            tinted: self.typesetting.tinted,
        };
        let decorated = self.decorated(item, lang, code);
        let raw = self.raw_code.contains(&(item, code.ord));
        let clear = self.clear_code.contains(&(item, code.ord));
        let whole = decorated
            .as_ref()
            .map_or(code, |d| if raw { &d.raw } else { d.shown(clear) });
        let piece = part.map(|(p, _)| super::code_part(whole, p));
        let shown = piece.as_ref().unwrap_or(whole);
        self.drawn_code.borrow_mut().push(super::DrawnCode {
            line,
            part: part.map_or(0, |(p, _)| p),
            shown: shown.text.clone(),
            copied: decorated
                .as_ref()
                .map_or_else(|| code.text.clone(), |d| d.shown(true).text.clone()),
        });
        let (first, last) = part.map_or((true, true), |(p, n)| (p == 0, p + 1 == n));
        let id = (item << 16) | (code.ord as usize) << 8 | part.map_or(0, |(p, _)| p as usize);
        let group: SharedString = format!("code-{key_item}-{}", code.ord).into();
        let copied = self.copied.filter(|key| key >> 2 == id);
        let copy = |key: &str, slot: usize, text: String| {
            let which = (id << 2) | slot;
            let done = copied == Some(which);
            control(
                Button::new(
                    ("copy", which),
                    if done {
                        focus_i18n::t("reading-copied")
                    } else {
                        focus_i18n::t(key)
                    },
                )
                .tab_stop(false)
                .kind(ButtonKind::Ghost)
                .icon(if done { Icon::CHECK } else { Icon::COPY })
                .on_click(cx.listener(move |this, _, _, cx| this.copy(which, text.clone(), cx))),
            )
        };
        let bare = self.compact && code.text.lines().count() < 4;
        let copies: Vec<AnyElement> = match &decorated {
            Some(d) => vec![
                copy("reading-copy-resolved", 1, d.shown(true).text.clone()).into_any_element(),
                copy("reading-copy-raw", 2, code.text.clone()).into_any_element(),
            ],
            None => vec![copy("reading-copy", 0, code.text.clone()).into_any_element()],
        };
        let block = (item, code.ord);
        let view = cx.entity().downgrade();
        let view_toggle = decorated.as_ref().map(|_| {
            Segments::new(
                ("code-view", id),
                vec![
                    Segment::new(focus_i18n::t("reading-code-resolved")),
                    Segment::new(focus_i18n::t("reading-code-raw")),
                ],
                raw as usize,
            )
            .tab_stop(false)
            .on_select(move |choice, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    if choice == 1 {
                        this.raw_code.insert(block);
                    } else {
                        this.raw_code.remove(&block);
                    }
                    cx.notify();
                });
            })
        });
        let chooser = div()
            .id(("code-language", id))
            .flex()
            .items_center()
            .gap(size::S2)
            .h(px(20.))
            .px(size::S1)
            .rounded(size::RADIUS)
            .cursor_pointer()
            .hover(move |s| s.bg(t.card_2))
            .when(label.is_empty(), |this| {
                this.invisible().group_hover(group.clone(), |s| s.visible())
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if this.picker_dismissed.take() != Some(line) {
                        this.open_picker(line, event.position, window, cx);
                    }
                }),
            )
            .child(
                div()
                    .mono_sm()
                    .text_color(t.ink_3)
                    .child(if label.is_empty() {
                        focus_i18n::t("reading-language-choose")
                    } else {
                        label.to_string()
                    }),
            )
            .when(detected, |this| {
                this.child(
                    div()
                        .mono_sm()
                        .text_color(t.ink_4)
                        .child(focus_i18n::t("reading-code-detected")),
                )
            });
        if bare {
            return div()
                .id(("code", id))
                .group(group.clone())
                .relative()
                .bg(t.bg_2)
                .border_1()
                .border_color(t.line)
                .child(
                    div()
                        .id(("code-body", id))
                        .overflow_x_scroll()
                        .px(size::S3)
                        .py(size::S2)
                        .child(
                            div()
                                .whitespace_nowrap()
                                .child(self.text(key_item, shown, &base, t)),
                        ),
                )
                .child(
                    Dir::current()
                        .end(div().absolute(), px(2.))
                        .top(px(2.))
                        .flow_row()
                        .gap(size::S1)
                        .bg(t.bg_2)
                        .when(copied.is_none(), |this| {
                            this.invisible().group_hover(group.clone(), |s| s.visible())
                        })
                        .children(copies),
                )
                .into_any_element();
        }
        div()
            .id(("code", id))
            .group(group.clone())
            .flex()
            .flex_col()
            .bg(t.bg_2)
            .border_l_1()
            .border_r_1()
            .when(first, |this| this.border_t_1())
            .when(last, |this| this.border_b_1())
            .border_color(t.line)
            .when(first, |this| {
                this.child(
                    div()
                        .flow_row()
                        .items_center()
                        .h(px(28.))
                        .px(size::S2)
                        .border_b_1()
                        .border_color(t.line)
                        .gap(size::S2)
                        .child(chooser)
                        .children(self.code_variables.as_ref().map(|_| {
                            div()
                                .id(("code-variables", id))
                                .flex()
                                .items_center()
                                .h(px(20.))
                                .px(size::S1)
                                .rounded(size::RADIUS)
                                .cursor_pointer()
                                .hover(move |s| s.bg(t.card_2))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        if this.picker_dismissed.take() != Some(line) {
                                            this.open_picker_of(
                                                super::picker::PickerKind::Variable,
                                                line,
                                                event.position,
                                                window,
                                                cx,
                                            );
                                        }
                                    }),
                                )
                                .child(
                                    div()
                                        .ui_sm()
                                        .text_color(t.ink_3)
                                        .child(focus_i18n::t("reading-variables")),
                                )
                        }))
                        .child(div().flex_1())
                        .child(
                            div()
                                .flow_row()
                                .gap(size::S1)
                                .when(copied.is_none(), |this| {
                                    this.invisible().group_hover(group.clone(), |s| s.visible())
                                })
                                .children(copies),
                        )
                        .children(
                            decorated
                                .as_ref()
                                .filter(|d| d.revealed.is_some() && !raw)
                                .map(|_| {
                                    control(
                                        Button::new(
                                            ("code-clear", id),
                                            focus_i18n::t(if clear {
                                                "reading-code-mask"
                                            } else {
                                                "reading-code-unmask"
                                            }),
                                        )
                                        .tab_stop(false)
                                        .kind(ButtonKind::Ghost)
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                if !this.clear_code.remove(&block) {
                                                    this.clear_code.insert(block);
                                                }
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                }),
                        )
                        .children(view_toggle),
                )
            })
            .child(
                div()
                    .id(("code-body", id))
                    .overflow_x_scroll()
                    .px(size::S4)
                    .when(first, |this| this.pt(size::S3))
                    .when(last, |this| this.pb(size::S3))
                    .child(
                        div()
                            .whitespace_nowrap()
                            .child(self.text(key_item, shown, &base, t)),
                    ),
            )
            .into_any_element()
    }

    fn remote_frame(
        &self,
        url: &str,
        policy: crate::RemoteImages,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host = crate::remote::host_of(url).unwrap_or_else(|| url.to_string());
        let key = if policy == crate::RemoteImages::Never {
            "reading-remote-off"
        } else {
            "reading-remote-blocked"
        };
        div()
            .flow_row()
            .items_center()
            .gap(size::S2)
            .px(size::S3)
            .py(size::S2)
            .border_1()
            .border_color(t.line)
            .bg(t.bg_2)
            .child(
                div()
                    .flex_none()
                    .text_color(t.ink_3)
                    .child(icon(Icon::IMAGE, size::ICON_DENSE)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .ui_sm()
                    .text_color(t.ink_2)
                    .child(focus_kit::ellipsis(focus_i18n::t_args(
                        key,
                        &[("host", host.as_str().into())],
                    ))),
            )
            .when(policy == crate::RemoteImages::OnClick, |this| {
                this.child(control(
                    Button::new(
                        ("remote-load", key_of(&ImageSource::Url(url.to_string()))),
                        focus_i18n::t("reading-remote-load"),
                    )
                    .tab_stop(false)
                    .kind(ButtonKind::Secondary)
                    .on_click(cx.listener(|this, _, _, cx| this.load_remote_images(cx))),
                ))
            })
            .into_any_element()
    }

    pub(super) fn render_image(
        &self,
        image: &ImageRef,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let policy = crate::remote_images(cx);
        let held = match &image.source {
            ImageSource::Url(url)
                if policy == crate::RemoteImages::Never
                    || (policy == crate::RemoteImages::OnClick && !self.remote_allowed) =>
            {
                Some(self.remote_frame(url, policy, t, cx))
            }
            _ => None,
        };
        let figure = if let Some(frame) = held {
            frame
        } else {
            match self
                .media
                .update(cx, |media, cx| media.image(&image.source, cx))
            {
                Slot::Ready {
                    image: data,
                    width,
                    height,
                    ..
                } => {
                    let shown = image
                        .width
                        .map_or(width, |w| w as f32)
                        .min(self.room().min(self.typesetting.measure) - 2.)
                        .max(1.);
                    let full = data.clone();
                    self.drawn_images
                        .borrow_mut()
                        .push((data.clone(), width, height));
                    let group: SharedString = format!("image-{}", key_of(&image.source)).into();
                    let viewer = self.live.then(|| {
                        Dir::current()
                            .end(div().absolute(), px(4.))
                            .top(px(4.))
                            .invisible()
                            .group_hover(group.clone(), |s| s.visible())
                            .child(control(
                                focus_kit::IconButton::new(
                                    ("image-viewer", key_of(&image.source)),
                                    Icon::MAXIMIZE,
                                    focus_i18n::t("reading-image-view"),
                                )
                                .tab_stop(false)
                                .dense(true)
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.open_viewer(full.clone(), width, height, window, cx)
                                    },
                                )),
                            ))
                    });
                    let zone = super::Zone::Image {
                        image: data.clone(),
                        width,
                        height,
                    };
                    div()
                        .id(("image", key_of(&image.source)))
                        .flow_row()
                        .when(!self.live, |this| {
                            this.cursor(gpui::CursorStyle::PointingHand)
                        })
                        .child(
                            self.zone(
                                div()
                                    .id(group.clone())
                                    .group(group.clone())
                                    .relative()
                                    .w(px(shown + 2.))
                                    .max_w_full()
                                    .flex_shrink()
                                    .border_1()
                                    .border_color(t.line)
                                    .child({
                                        let mut room = div().w_full();
                                        room.style().aspect_ratio = Some(width / height.max(1.));
                                        room.child(img(data).size_full())
                                    })
                                    .children(viewer),
                                zone,
                            ),
                        )
                        .into_any_element()
                }
                Slot::Loading => div()
                    .h(px(48.))
                    .flow_row()
                    .items_center()
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(focus_i18n::t("reading-image-loading"))
                    .into_any_element(),
                Slot::Failed(message) => div()
                    .flow_row()
                    .items_center()
                    .gap(size::S2)
                    .py(size::S2)
                    .ui_sm()
                    .text_color(t.danger)
                    .child(icon(Icon::IMAGE, size::ICON_DENSE))
                    .child(div().flex_1().min_w_0().child(focus_kit::ellipsis(message)))
                    .into_any_element(),
            }
        };
        match &image.caption {
            Some(caption) => div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .child(figure)
                .child(
                    Dir::current()
                        .align_text(div(), caption)
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(caption.clone()),
                )
                .into_any_element(),
            None => figure,
        }
    }

    pub(super) fn render_mermaid(
        &self,
        source: &str,
        item: usize,
        t: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let slot = self.media.update(cx, |media, cx| {
            media.mermaid(source, t, window.scale_factor(), cx)
        });
        let zoom_source = source.to_string();
        let copy_source = source.to_string();
        let label = match &slot {
            Slot::Ready { label, .. } => label.clone(),
            _ => String::new(),
        };
        let body: AnyElement = match slot {
            Slot::Ready {
                image,
                width,
                height,
                links,
                ..
            } => {
                let room = self.figure_room() - 2. - 48.;
                let shown = width.min(room).max(1.);
                let figure = self.diagram_figure(image, &links, shown, width, height, item, t);
                div()
                    .flow_row()
                    .justify_center()
                    .p(size::S6)
                    .child(figure)
                    .into_any_element()
            }
            Slot::Loading => div()
                .h(px(96.))
                .flow_row()
                .items_center()
                .justify_center()
                .ui_sm()
                .text_color(t.ink_3)
                .child(focus_i18n::t("reading-diagram-loading"))
                .into_any_element(),
            Slot::Failed(message) => div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .p(size::S4)
                .child(div().ui_sm().text_color(t.danger).child(message))
                .child(
                    div()
                        .id(("mermaid-src", item))
                        .overflow_x_scroll()
                        .p(size::S3)
                        .bg(t.bg_2)
                        .border_1()
                        .border_color(t.line)
                        .font_family(self.typesetting.code_font.clone())
                        .text_size(px(13.))
                        .line_height(px(21.))
                        .text_color(t.ink_body)
                        .whitespace_nowrap()
                        .child(source.to_string()),
                )
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .border_1()
            .border_color(t.line)
            .bg(t.bg)
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .gap(size::S2)
                    .h(px(28.))
                    .px(size::S3)
                    .border_b_1()
                    .border_color(t.line)
                    .child(
                        div()
                            .ui_sm()
                            .text_color(t.ink_3)
                            .child(focus_i18n::t("reading-diagram")),
                    )
                    .child(div().mono_sm().text_color(t.ink_3).child(label))
                    .child(div().flex_1())
                    .child(control(
                        Button::new(("zoom", item), focus_i18n::t("reading-zoom"))
                            .tab_stop(false)
                            .kind(ButtonKind::Ghost)
                            .icon(Icon::MAXIMIZE)
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(ReadingEvent::Zoom(zoom_source.clone()))
                            })),
                    ))
                    .child({
                        let which = (item << 18) | 0xFFFF << 2 | 3;
                        let done = self.copied == Some(which);
                        control(
                            Button::new(
                                ("copy-src", item),
                                if done {
                                    focus_i18n::t("reading-copied")
                                } else {
                                    focus_i18n::t("reading-copy-source")
                                },
                            )
                            .tab_stop(false)
                            .kind(ButtonKind::Ghost)
                            .icon(if done { Icon::CHECK } else { Icon::COPY })
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.copy(which, copy_source.clone(), cx),
                            )),
                        )
                    }),
            )
            .child(body)
            .into_any_element()
    }
}

impl ReadingView {
    #[allow(clippy::too_many_arguments)]
    fn diagram_figure(
        &self,
        image: std::sync::Arc<gpui::RenderImage>,
        links: &[focus_md::mermaid::DiagramLink],
        shown: f32,
        width: f32,
        height: f32,
        item: usize,
        t: &Tokens,
    ) -> AnyElement {
        let scale = shown / width.max(1.);
        let targets = self.built.as_ref().map(|b| &b.diagram_targets);
        let hovered = self.hovered_diagram;
        let zones = links.iter().enumerate().map(|(n, link)| {
            let (goal, title) = match targets.and_then(|m| m.get(&link.target)) {
                Some(found) => found.clone(),
                None if link.external => {
                    (LinkGoal::External(link.target.clone()), link.target.clone())
                }
                None => (
                    LinkGoal::Unresolved(link.target.clone()),
                    link.target.clone(),
                ),
            };
            let hits = self.diagram_hits.clone();
            let id = (item, n);
            let over = hovered == Some(id);
            div()
                .absolute()
                .left(px(link.x * scale))
                .top(px(link.y * scale))
                .w(px(link.width * scale))
                .h(px(link.height * scale))
                .border_1()
                .border_color(if over {
                    t.ink
                } else {
                    gpui::transparent_black()
                })
                .child(
                    gpui::canvas(
                        move |bounds, _, _| {
                            hits.borrow_mut().push(super::DiagramHit {
                                bounds,
                                id,
                                goal: goal.clone(),
                            })
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .when(over, |this| {
                    this.child(
                        div()
                            .absolute()
                            .left_0()
                            .top(gpui::relative(1.))
                            .mt(px(6.))
                            .child(gpui::deferred(gpui::anchored().snap_to_window().child(
                                focus_kit::tooltip::tooltip_body(t, title.into(), None),
                            ))),
                    )
                })
        });
        div()
            .relative()
            .w(px(shown))
            .h(px(shown * height / width.max(1.)))
            .child(img(image).size_full())
            .children(zones)
            .into_any_element()
    }
}

impl ReadingView {
    pub(super) fn zone<E: ParentElement>(&self, element: E, zone: super::Zone) -> E {
        let zones = self.zones.clone();
        element.child(
            gpui::canvas(
                |_, _, _| {},
                move |bounds, _, _, _| zones.borrow_mut().push((bounds, zone.clone())),
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }
}

pub(super) const FOLD_ROOM: f32 = 24.;

pub(super) fn heading_space(level: u8) -> (f32, f32) {
    match level {
        1 => (40., 12.),
        2 => (32., 8.),
        3 => (24., 6.),
        _ => (20., 4.),
    }
}

fn control(element: impl IntoElement) -> gpui::Div {
    div()
        .flex_none()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(element)
}

fn key_of(source: &crate::model::ImageSource) -> usize {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{source:?}").hash(&mut hasher);
    hasher.finish() as usize
}

fn callout_icon(glyph: CalloutIcon) -> Icon {
    match glyph {
        CalloutIcon::Note => Icon::PENCIL,
        CalloutIcon::Abstract => Icon::CLIPBOARD_LIST,
        CalloutIcon::Info => Icon::INFO,
        CalloutIcon::Todo => Icon::CHECK_CIRCLE,
        CalloutIcon::Tip => Icon::FLAME,
        CalloutIcon::Success => Icon::CHECK,
        CalloutIcon::Question => Icon::HELP,
        CalloutIcon::Warning => Icon::ALERT,
        CalloutIcon::Failure => Icon::CLOSE,
        CalloutIcon::Danger => Icon::ZAP,
        CalloutIcon::Bug => Icon::BUG,
        CalloutIcon::Example => Icon::LIST,
        CalloutIcon::Quote => Icon::QUOTE,
    }
}
