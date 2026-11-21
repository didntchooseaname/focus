use std::ops::Range;
use std::sync::Arc;

use focus_core::{NoteId, Placement};
use focus_kit::Flow as _;
use focus_kit::{ActiveTokens, Icon, Kbd, Type, ellipsis, floating, icon, size};
use focus_search::FuzzyScope;
use gpui::{
    AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, HighlightStyle,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Render, ScrollStrategy,
    SharedString, StatefulInteractiveElement, Styled, Subscription, UniformListScrollHandle,
    Window, div, prelude::FluentBuilder, px, uniform_list,
};
use gpui_component::input::{Input, InputEvent, InputState};

use crate::actions::{
    SelectFirst, SelectLast, SelectNext, SelectPageDown, SelectPageUp, SelectPrev,
};
use crate::library::Library;

const LIMIT: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    Note(NoteId),
    New(String),
}

pub enum PickerEvent {
    Chosen(Choice, Placement),
    Close,
}

struct Hit {
    id: NoteId,
    title: SharedString,
    folder: SharedString,
    matched: Vec<Range<usize>>,
}

pub struct NotePicker {
    pub input: Entity<InputState>,
    library: Arc<Library>,
    source: NoteId,
    recent: Vec<NoteId>,
    hits: Vec<Hit>,
    selected: usize,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PickerEvent> for NotePicker {}

impl Focusable for NotePicker {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

fn char_ranges(text: &str, positions: &[u32]) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for (i, (byte, ch)) in text.char_indices().enumerate() {
        if positions.contains(&(i as u32)) {
            let end = byte + ch.len_utf8();
            match out.last_mut() {
                Some(last) if last.end == byte => last.end = end,
                _ => out.push(byte..end),
            }
        }
    }
    out
}

impl NotePicker {
    pub fn new(
        library: Arc<Library>,
        source: NoteId,
        recent: Vec<NoteId>,
        placeholder: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscriptions = vec![cx.subscribe(
            &input,
            |this: &mut NotePicker, _, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    this.refilter(cx);
                    cx.notify();
                }
                InputEvent::PressEnter { secondary } => this.confirm(*secondary, false, cx),
                _ => {}
            },
        )];
        let mut picker = NotePicker {
            input,
            library,
            source,
            recent,
            hits: Vec::new(),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        picker.refilter(cx);
        picker
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value().trim().to_string();
        let library = self.library.clone();
        self.hits = library
            .fuzzy(&query, FuzzyScope::Notes, &self.recent, LIMIT)
            .into_iter()
            .filter_map(|hit| {
                let meta = library.note(hit.note?)?;
                (meta.id != self.source).then(|| Hit {
                    id: meta.id,
                    matched: char_ranges(&hit.text, &hit.positions),
                    title: hit.text.into(),
                    folder: meta
                        .path
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                        .into(),
                })
            })
            .collect();
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }

    fn select(&mut self, to: isize, wrap: bool, cx: &mut Context<Self>) {
        if self.hits.is_empty() {
            return;
        }
        let count = self.hits.len() as isize;
        self.selected = if wrap {
            to.rem_euclid(count)
        } else {
            to.clamp(0, count - 1)
        } as usize;
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Top);
        cx.notify();
    }

    fn confirm(&mut self, new: bool, start: bool, cx: &mut Context<Self>) {
        let placement = if start {
            Placement::Start
        } else {
            Placement::End
        };
        let choice = if new {
            let name = self.input.read(cx).value().trim().to_string();
            if name.is_empty() {
                return;
            }
            Choice::New(name)
        } else {
            match self.hits.get(self.selected) {
                Some(hit) => Choice::Note(hit.id),
                None => return,
            }
        };
        cx.emit(PickerEvent::Chosen(choice, placement));
    }

    fn on_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let m = event.keystroke.modifiers;
        if event.keystroke.key == "enter" && m.shift && !m.control && !m.alt && !m.platform {
            cx.stop_propagation();
            self.confirm(false, true, cx);
        }
    }

    fn render_row(
        &self,
        i: usize,
        t: &focus_kit::Tokens,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let hit = &self.hits[i];
        let current = i == self.selected;
        div()
            .id(i)
            .relative()
            .flow_row()
            .w_full()
            .items_center()
            .gap(size::S3)
            .h(size::PALETTE_ROW)
            .px(size::S4)
            .ui()
            .text_color(t.ink_2)
            .cursor_pointer()
            .when(current, |row| {
                row.bg(t.card_3).child(
                    div()
                        .absolute()
                        .start_at(px(0.))
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .bg(t.ink),
                )
            })
            .when(!current, |row| {
                let (hover, press) = (t.card_2, t.card_3);
                row.hover(move |s| s.bg(hover)).active(move |s| s.bg(press))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected = i;
                this.confirm(false, false, cx);
            }))
            .child(icon(Icon::FILE_TEXT, size::ICON).text_color(t.ink_3))
            .child(div().flex_shrink().min_w_0().max_w(px(360.)).child(
                ellipsis(hit.title.clone()).highlights(hit.matched.iter().map(|r| {
                    (
                        r.clone(),
                        HighlightStyle {
                            color: Some(t.ink),
                            font_weight: Some(FontWeight::SEMIBOLD),
                            ..Default::default()
                        },
                    )
                })),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flow_row()
                    .justify_end()
                    .mono_sm()
                    .text_color(t.ink_3)
                    .child(ellipsis(hit.folder.clone())),
            )
            .into_any_element()
    }
}

fn hint(keys: String, label: String) -> impl IntoElement {
    div()
        .flow_row()
        .items_center()
        .gap(size::S2)
        .min_w_0()
        .child(Kbd::new(keys))
        .child(ellipsis(label))
}

impl Render for NotePicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let top = window.viewport_size().height * 0.16;
        let visible = self.hits.len().min(size::PALETTE_VISIBLE);
        let list_height = size::PALETTE_ROW * visible as f32;
        let typed = !self.input.read(cx).value().trim().is_empty();
        let body = floating(&t)
            .id("note-picker")
            .key_context(crate::actions::LIST_CONTEXT)
            .track_focus(&self.focus)
            .occlude()
            .flex()
            .flex_col()
            .w(size::PALETTE_W)
            .max_w(gpui::relative(0.94))
            .mt(top)
            .capture_key_down(cx.listener(Self::on_key))
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| {
                this.select(this.selected as isize - 1, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| {
                this.select(this.selected as isize + 1, true, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectPageUp, _, cx| {
                this.select(
                    this.selected as isize - size::PALETTE_VISIBLE as isize,
                    false,
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &SelectPageDown, _, cx| {
                this.select(
                    this.selected as isize + size::PALETTE_VISIBLE as isize,
                    false,
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &SelectFirst, _, cx| this.select(0, false, cx)))
            .on_action(
                cx.listener(|this, _: &SelectLast, _, cx| this.select(isize::MAX, false, cx)),
            )
            .on_action(|_: &gpui_component::input::Enter, _, _| {})
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .gap(size::S3)
                    .h(size::PALETTE_INPUT)
                    .px(size::S4)
                    .border_b_1()
                    .border_color(t.line)
                    .child(icon(Icon::SEARCH, size::ICON).text_color(t.ink_3))
                    .child(
                        div().flex_1().ui_lg().text_color(t.ink).child(
                            Input::new(&self.input)
                                .appearance(false)
                                .text_size(px(14.))
                                .line_height(px(22.))
                                .px_0()
                                .py_0()
                                .h(px(22.)),
                        ),
                    ),
            )
            .map(|this| {
                if self.hits.is_empty() {
                    this.child(
                        div()
                            .flow_row()
                            .items_center()
                            .h(size::PALETTE_ROW * 2.)
                            .px(size::S4)
                            .ui()
                            .text_color(t.ink_3)
                            .child(focus_i18n::t("ui-palette-no-match")),
                    )
                } else {
                    this.child(
                        div()
                            .relative()
                            .h(list_height)
                            .child(
                                uniform_list(
                                    "note-picker-hits",
                                    self.hits.len(),
                                    cx.processor(move |this, range: Range<usize>, _, cx| {
                                        range.map(|i| this.render_row(i, &t, cx)).collect()
                                    }),
                                )
                                .h(list_height)
                                .track_scroll(self.scroll.clone()),
                            )
                            .child(focus_kit::scrollbar(
                                "note-picker-scrollbar",
                                self.scroll.clone(),
                            )),
                    )
                }
            })
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .gap(size::S4)
                    .h(size::PALETTE_FOOT)
                    .px(size::S4)
                    .border_t_1()
                    .border_color(t.line)
                    .ui_sm()
                    .text_color(t.ink_3)
                    .overflow_hidden()
                    .when(!self.hits.is_empty(), |this| {
                        this.child(hint(
                            focus_i18n::t("ui-key-enter"),
                            focus_i18n::t("ui-compose-at-end"),
                        ))
                        .child(hint(
                            focus_i18n::t("ui-key-shift-enter"),
                            focus_i18n::t("ui-compose-at-start"),
                        ))
                    })
                    .when(typed, |this| {
                        this.child(hint(
                            focus_i18n::t("ui-key-ctrl-enter"),
                            focus_i18n::t("ui-compose-new-note"),
                        ))
                    })
                    .child(hint(
                        focus_i18n::t("ui-key-escape"),
                        focus_i18n::t("ui-palette-close"),
                    )),
            );
        let veil = div()
            .id("note-picker-veil")
            .absolute()
            .inset_0()
            .flow_row()
            .justify_center()
            .items_start()
            .bg(t.veil)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.emit(PickerEvent::Close)),
            )
            .child(body);
        focus_kit::fade_in("fade-note-picker", veil, cx)
    }
}
