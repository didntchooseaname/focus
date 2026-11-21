use focus_kit::Flow as _;
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Forced, NoteTab, Tokens, Type, ellipsis, hatch, size,
};
use gpui::{
    AnyElement, Context, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Pixels, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::editor::{Load, NoteEditor, ViewMode};
use crate::groups::{DraggedTab, TabItem, TabMenu};
use crate::shell::{Edge, Pane, Shell};

fn sheet_message(t: &Tokens, text: String, danger: bool) -> AnyElement {
    div()
        .pt(size::S8)
        .ui()
        .text_color(if danger { t.danger } else { t.ink_3 })
        .child(text)
        .into_any_element()
}

struct GroupView {
    second: bool,
    focused: bool,
    tabs: Vec<Entity<NoteEditor>>,
    active: Option<usize>,
    panes: Vec<Pane>,
    active_pane: Option<usize>,
}

impl GroupView {
    fn editor(&self) -> Option<Entity<NoteEditor>> {
        if self.active_pane.is_some() {
            return None;
        }
        self.active.and_then(|i| self.tabs.get(i)).cloned()
    }

    fn pane(&self) -> Option<Pane> {
        self.active_pane.and_then(|i| self.panes.get(i)).copied()
    }
}

impl Shell {
    fn group_view(&self, second: bool) -> GroupView {
        match &self.split {
            Some(split) if split.focused_second != second => GroupView {
                second,
                focused: false,
                tabs: split.other.tabs.clone(),
                active: split.other.active,
                panes: split.other.panes.clone(),
                active_pane: split.other.active_pane,
            },
            _ => GroupView {
                second,
                focused: true,
                tabs: self.tabs.clone(),
                active: self.active,
                panes: self.panes.clone(),
                active_pane: self.active_pane,
            },
        }
    }

    pub fn render_center(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = *cx.tokens();
        let viewport = window.viewport_size();
        let left = if self.layout.left_open {
            self.layout.left_w
        } else {
            px(0.)
        };
        let right = if self.layout.right_open {
            self.layout.right_w
        } else {
            px(0.)
        };
        let center_w = viewport.width - left - right;
        let Some(share) = self.split.as_ref().map(|split| split.share) else {
            let view = self.group_view(false);
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(self.render_group(view, center_w, cx))
                .into_any_element();
        };
        let first_w = center_w * share;
        let dragging = self.drag.as_ref().is_some_and(|d| d.edge == Edge::Split);
        let (first, second) = (self.group_view(false), self.group_view(true));
        div()
            .relative()
            .flow_row()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .w(first_w)
                    .h_full()
                    .min_w_0()
                    .child(self.render_group(first, first_w, cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .border_start_1()
                    .border_color(t.line)
                    .child(self.render_group(second, center_w - first_w - px(1.), cx)),
            )
            .child(
                crate::panes::handle(&t, "handle-split", true, dragging)
                    .start_at(first_w - size::HANDLE / 2. + px(0.5))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.start_drag(Edge::Split, event.position.x, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_group(
        &mut self,
        view: GroupView,
        width: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = *cx.tokens();
        let prefs = self.prefs.read(cx);
        let measure = px(prefs.number("appearance.measure") as f32);
        let show_hatch = prefs.bool("appearance.hatch");
        let sheet_w = measure + size::SHEET_PAD * 2. + px(2.);
        let editor = view.editor();
        let pane = view.pane();
        let (second, focused) = (view.second, view.focused);
        let framed = width >= sheet_w;
        let bounds = if focused {
            self.editor_bounds.clone()
        } else {
            crate::editing::EditorBounds::default()
        };
        let content: AnyElement =
            if let Some(pane) = pane.filter(|pane| !matches!(pane, Pane::Base(_))) {
                match pane {
                    Pane::Graph => self
                        .graph
                        .clone()
                        .map(|g| g.into_any_element())
                        .unwrap_or_else(|| div().into_any_element()),
                    Pane::Ops(screen) => self
                        .ops
                        .as_ref()
                        .map(|ops| ops.screen(screen))
                        .unwrap_or_else(|| div().into_any_element()),
                    Pane::Base(_) => div().into_any_element(),
                    Pane::File(id) => self
                        .file_element(id)
                        .unwrap_or_else(|| div().into_any_element()),
                }
            } else {
                let base = match pane {
                    Some(Pane::Base(id)) => self.base_page(id).map(|page| page.view.clone()),
                    _ => None,
                };
                let body: AnyElement = match (&base, &editor) {
                    (Some(view), _) => div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .max_w(measure)
                        .h_full()
                        .child(view.clone())
                        .into_any_element(),
                    (None, None) => sheet_message(&t, focus_i18n::t("ui-tab-none"), false),
                    (None, Some(editor)) => self.render_note(editor, measure, &bounds, cx),
                };
                let pad = if framed {
                    size::SHEET_PAD
                } else {
                    size::SHEET_PAD_NARROW
                };
                let sheet = div()
                    .flex()
                    .flex_col()
                    .h_full()
                    .min_w_0()
                    .bg(t.bg)
                    .px(pad)
                    .when(framed, |this| {
                        this.w(sheet_w)
                            .flex_none()
                            .border_start_1()
                            .border_end_1()
                            .border_color(t.line)
                    })
                    .when(!framed, |this| this.flex_1())
                    .child(body);
                let side = |t: &Tokens| {
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .when(show_hatch, |d| d.child(hatch()))
                        .bg(t.bg)
                };
                div()
                    .flow_row()
                    .size_full()
                    .when(framed, |this| this.child(side(&t)))
                    .child(sheet)
                    .when(framed, |this| this.child(side(&t)))
                    .into_any_element()
            };
        let conflict = match &editor {
            Some(editor) if editor.read(cx).conflict => {
                Some(self.render_conflict(editor, cx).into_any_element())
            }
            _ => None,
        };
        let copies = match &editor {
            Some(editor) if editor.read(cx).copies > 0 => {
                Some(self.render_copies(editor, cx).into_any_element())
            }
            _ => None,
        };
        let find = if focused { self.render_find(cx) } else { None };
        div()
            .id(if second { "group-2" } else { "group-1" })
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .when(!focused, |this| {
                this.capture_any_mouse_down(
                    cx.listener(move |this, _, window, cx| this.focus_group(second, window, cx)),
                )
            })
            .child(self.render_tabs(&view, cx))
            .on_drop(
                cx.listener(move |this, paths: &gpui::ExternalPaths, window, cx| {
                    let paths = paths.paths().to_vec();
                    this.focus_group(second, window, cx);
                    this.drop_files(&paths, window, cx);
                }),
            )
            .on_drop(cx.listener(move |this, dragged: &DraggedTab, window, cx| {
                this.move_tab(dragged, second, window, cx)
            }))
            .children(conflict)
            .children(copies)
            .children(find)
            .child(div().flow_row().flex_1().min_h_0().child(content))
            .when(focused, |this| {
                this.child(crate::selbar::layer(cx.entity().downgrade()))
            })
            .into_any_element()
    }

    fn render_conflict(
        &self,
        editor: &Entity<NoteEditor>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = *cx.tokens();
        let reload = editor.clone();
        let keep = editor.clone();
        let both = editor.clone();
        let merge = editor.clone();
        let can_merge = editor.read(cx).can_merge();
        div()
            .relative()
            .flow_row()
            .flex_none()
            .items_center()
            .gap(size::S3)
            .h(px(40.))
            .px(size::S4)
            .border_b_1()
            .border_color(t.line)
            .child(
                div()
                    .absolute()
                    .start_at(px(0.))
                    .top_0()
                    .bottom_0()
                    .w(px(2.))
                    .bg(t.warn),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .ui()
                    .text_color(t.ink)
                    .child(ellipsis(focus_i18n::t("ui-conflict-message"))),
            )
            .child(
                Button::new("conflict-reload", focus_i18n::t("ui-action-reload")).on_click(
                    cx.listener(move |_, _, window, cx| {
                        reload.update(cx, |editor, cx| editor.load(window, cx).detach());
                    }),
                ),
            )
            .child(
                Button::new("conflict-both", focus_i18n::t("ui-action-keep-both")).on_click(
                    cx.listener(move |_, _, window, cx| {
                        both.update(cx, |editor, cx| editor.keep_both(window, cx));
                    }),
                ),
            )
            .child(
                Button::new("conflict-keep", focus_i18n::t("ui-action-keep-mine"))
                    .kind(if can_merge {
                        ButtonKind::Secondary
                    } else {
                        ButtonKind::Primary
                    })
                    .on_click(cx.listener(move |_, _, _, cx| {
                        drop(keep.update(cx, |editor, cx| editor.save(true, cx)));
                    })),
            )
            .when(can_merge, |this| {
                this.child(
                    Button::new("conflict-merge", focus_i18n::t("ui-action-merge"))
                        .kind(ButtonKind::Primary)
                        .on_click(cx.listener(move |_, _, window, cx| {
                            merge.update(cx, |editor, cx| editor.merge(window, cx));
                        })),
                )
            })
    }

    fn render_copies(
        &self,
        editor: &Entity<NoteEditor>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = *cx.tokens();
        let (note, count) = {
            let editor = editor.read(cx);
            (editor.id, editor.copies)
        };
        div()
            .flow_row()
            .flex_none()
            .items_center()
            .gap(size::S3)
            .h(px(36.))
            .px(size::S4)
            .border_b_1()
            .border_color(t.line)
            .child(div().flex_none().size(px(6.)).rounded_full().bg(t.warn))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .ui()
                    .text_color(t.ink_2)
                    .child(ellipsis(focus_i18n::t_args(
                        "ui-copies-message",
                        &[("count", (count as i64).into())],
                    ))),
            )
            .child(
                Button::new("copies-versions", focus_i18n::t("ui-copies-show"))
                    .kind(ButtonKind::Ghost)
                    .on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.show_versions(note, window, cx)
                        }),
                    ),
            )
    }

    fn editor_element(
        &self,
        note: &NoteEditor,
        bounds: &crate::editing::EditorBounds,
        cx: &Context<Self>,
    ) -> AnyElement {
        let font = crate::editing::EditorFont::from_choice(
            self.prefs.read(cx).choice("appearance.editor_font"),
        );
        crate::editing::editor_surface(&note.input, font, bounds, true)
            .h_full()
            .into_any_element()
    }

    fn render_tabs(&mut self, view: &GroupView, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let (second, focused) = (view.second, view.focused);
        let front = view.active.filter(|_| view.active_pane.is_none());
        let carried = |item: TabItem, title: String| DraggedTab {
            second,
            item,
            title: title.into(),
        };
        let draggable = |id: (&'static str, usize), dragged: DraggedTab| {
            div()
                .id(id)
                .flex()
                .flex_shrink()
                .min_w(size::TAB_MIN)
                .max_w(size::TAB_MAX)
                .on_drag(dragged, |dragged, _, _, cx| {
                    Shell::tab_ghost(dragged.title.clone(), cx)
                })
        };
        div()
            .id(if second { "note-tabs-2" } else { "note-tabs" })
            .relative()
            .flow_row()
            .flex_none()
            .h(size::TAB_BAR)
            .overflow_x_scroll()
            .drag_over::<DraggedTab>(move |style, dragged, _, _| {
                if dragged.second != second {
                    style.bg(t.card_2)
                } else {
                    style
                }
            })
            .on_drop(
                cx.listener(move |this, paths: &gpui::ExternalPaths, window, cx| {
                    let (notes, rest): (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) =
                        paths.paths().iter().cloned().partition(|p| {
                            p.is_file()
                                && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"))
                        });
                    this.focus_group(second, window, cx);
                    this.open_paths.extend(notes);
                    this.open_given_paths(window, cx);
                    this.drop_files(&rest, window, cx);
                    cx.stop_propagation();
                }),
            )
            .children(view.tabs.iter().enumerate().map(|(i, editor)| {
                let entity = editor.entity_id();
                let editor = editor.read(cx);
                let title = match editor.loose {
                    Some(_) => focus_i18n::t_args(
                        "ui-loose-tab",
                        &[("title", editor.meta.title.as_ref().into())],
                    ),
                    None => editor.meta.title.to_string(),
                };
                let forced = match self.forced(&format!("tab:{i}")) {
                    Forced::Live if !focused && front == Some(i) && !editor.dirty => Forced::Hover,
                    forced if focused => forced,
                    _ => Forced::Live,
                };
                let tab = NoteTab::new(
                    (if second { "tab-2" } else { "tab" }, i),
                    title.clone(),
                    focus_i18n::t("ui-tab-close"),
                )
                .close_shortcut(crate::actions::shortcut("ui-cmd-close-tab"))
                .active(focused && front == Some(i))
                .dirty(editor.dirty)
                .forced(forced)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.focus_group(second, window, cx);
                    this.activate(i, window, cx)
                }))
                .on_close(cx.listener(move |this, _, window, cx| {
                    this.focus_group(second, window, cx);
                    this.close_tab(i, window, cx)
                }));
                draggable(
                    (if second { "tab-drag-2" } else { "tab-drag" }, i),
                    carried(TabItem::Note(entity), title),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                        this.open_tab_menu(
                            TabMenu {
                                second,
                                index: i,
                                x: event.position.x,
                                y: event.position.y,
                            },
                            cx,
                        )
                    }),
                )
                .child(tab)
            }))
            .children(view.panes.iter().enumerate().map(|(i, pane)| {
                let pane = *pane;
                let label = match pane {
                    Pane::Graph => focus_i18n::t("graph-title"),
                    Pane::Base(id) => self
                        .base_page(id)
                        .and_then(|page| page.path.file_stem())
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    Pane::Ops(screen) => screen.label(),
                    Pane::File(id) => self.file_tab_label(id),
                };
                let front_pane = view.active_pane == Some(i);
                let tab = NoteTab::new(
                    (if second { "pane-2" } else { "pane" }, i),
                    label.clone(),
                    focus_i18n::t("ui-tab-close"),
                )
                .close_shortcut(crate::actions::shortcut("ui-cmd-close-tab"))
                .active(focused && front_pane)
                .forced(if !focused && front_pane {
                    Forced::Hover
                } else {
                    Forced::Live
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.focus_group(second, window, cx);
                    this.show_pane(pane, window, cx)
                }))
                .on_close(cx.listener(move |this, _, window, cx| {
                    this.focus_group(second, window, cx);
                    if let Some(index) = this.panes.iter().position(|p| *p == pane) {
                        this.close_pane(index, window, cx)
                    }
                }));
                draggable(
                    (if second { "pane-drag-2" } else { "pane-drag" }, i),
                    carried(TabItem::Pane(pane), label),
                )
                .child(tab)
            }))
            .child(
                div().flex_1().h_full().relative().child(
                    div()
                        .absolute()
                        .start_at(px(0.))
                        .end_at(px(0.))
                        .bottom_0()
                        .h(px(1.))
                        .bg(t.line),
                ),
            )
    }

    fn render_note(
        &self,
        editor: &Entity<NoteEditor>,
        measure: Pixels,
        bounds: &crate::editing::EditorBounds,
        cx: &Context<Self>,
    ) -> AnyElement {
        let t = *cx.tokens();
        let note = editor.read(cx);
        match (&note.load, note.view) {
            (Load::Loading, _) => sheet_message(&t, focus_i18n::t("ui-note-loading"), false),
            (Load::Failed(error), _) => sheet_message(&t, error.clone(), true),
            (Load::Ready, ViewMode::Reading | ViewMode::Live) => div()
                .relative()
                .flex_1()
                .min_h_0()
                .w_full()
                .max_w(measure)
                .h_full()
                .capture_any_mouse_down(self.editor_menu_trigger(editor, cx))
                .on_mouse_down(MouseButton::Right, self.in_place_menu_trigger(editor, cx))
                .when(
                    note.view == ViewMode::Live && note.edit_in_place(),
                    |this| crate::format::in_place_actions(this, editor),
                )
                .child(note.reading.clone())
                .child(crate::window_keys::zoom_wheel(self.wheel_zoom.clone()))
                .into_any_element(),
            (Load::Ready, ViewMode::Source) => div()
                .relative()
                .flex_1()
                .min_h_0()
                .h_full()
                .w_full()
                .max_w(measure)
                .pt(size::S8)
                .capture_any_mouse_down(self.editor_menu_trigger(editor, cx))
                .child(self.editor_element(note, bounds, cx))
                .child(crate::window_keys::zoom_wheel(self.wheel_zoom.clone()))
                .into_any_element(),
        }
    }
}
