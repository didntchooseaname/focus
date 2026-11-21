use focus_kit::{
    ActiveTokens, Button, ButtonKind, Checkbox, Field, Flow as _, Icon, IconButton, Type, size,
};
use gpui::{
    AnyElement, AppContext, Context, Entity, Focusable, InteractiveElement, IntoElement,
    KeystrokeEvent, ParentElement, Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::RopeExt as _;
use gpui_component::input::{InputEvent, InputState};

use crate::shell::Shell;

pub struct FindBar {
    pub query: Entity<InputState>,
    pub replacement: Entity<InputState>,
    pub case_sensitive: bool,
    pub replacing: bool,
    restore: Option<gpui::EntityId>,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    pub fn open_find(&mut self, replacing: bool, window: &mut Window, cx: &mut Context<Self>) {
        let pdf = self.front_pdf();
        let replacing = replacing && pdf.is_none();
        let mut restore = None;
        let selected = match (self.active_editor().cloned(), &pdf) {
            (_, Some(pdf)) => pdf.read(cx).selected_text().unwrap_or_default(),
            (Some(editor), None) => {
                if editor.read(cx).view == crate::editor::ViewMode::Reading {
                    restore = Some(editor.entity_id());
                    self.set_view(crate::editor::ViewMode::Source, window, cx);
                }
                editor.read(cx).typing_input().read(cx).selection_text()
            }
            (None, None) => return,
        };
        if self.find.is_none() {
            let query = cx.new(|cx| InputState::new(window, cx));
            let replacement = cx.new(|cx| InputState::new(window, cx));
            let subscriptions = vec![cx.subscribe_in(
                &query,
                window,
                |this: &mut Shell, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.refind(window, cx);
                    }
                },
            )];
            self.find = Some(FindBar {
                query,
                replacement,
                case_sensitive: false,
                replacing,
                restore: None,
                _subscriptions: subscriptions,
            });
        }
        if let Some(editor) = self.active_editor().cloned() {
            editor.update(cx, |editor, _| editor.spotted = false);
        }
        let Some(bar) = &mut self.find else { return };
        bar.replacing |= replacing;
        if restore.is_some() {
            bar.restore = restore;
        }
        let (query, replacement) = (bar.query.clone(), bar.replacement.clone());
        if !selected.is_empty() && !selected.contains('\n') {
            query.update(cx, |input, cx| input.set_value(selected, window, cx));
        }
        let focus_replacement = replacing && !query.read(cx).value().is_empty();
        if focus_replacement {
            replacement.update(cx, |input, cx| input.focus(window, cx));
        } else {
            query.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    pub fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let restore = self.find.take().and_then(|bar| bar.restore);
        if let Some(pdf) = self.front_pdf() {
            pdf.update(cx, |pdf, cx| pdf.close_find(cx));
            if let Some(pane) = self.active_pane.and_then(|i| self.panes.get(i)).copied() {
                self.focus_file(pane, window, cx);
            }
            cx.notify();
            return;
        }
        if let Some(editor) = self.active_editor().cloned() {
            let input = editor.read(cx).typing_input();
            let found = input
                .read(cx)
                .find_ranges(cx)
                .and_then(|(ranges, current)| ranges.get(current).cloned());
            input.update(cx, |input, cx| {
                if let Some(found) = &found {
                    input.set_selection(found.start, found.end, cx);
                }
                input.find_close(cx)
            });
            let back_to_reading = restore == Some(editor.entity_id())
                && editor.read(cx).view == crate::editor::ViewMode::Source;
            if back_to_reading {
                let line =
                    found.map(|found| input.read(cx).text().offset_to_position(found.start).line);
                self.set_view(crate::editor::ViewMode::Reading, window, cx);
                if let Some(line) = line {
                    editor.update(cx, |editor, cx| {
                        editor
                            .reading
                            .update(cx, |reading, _| reading.scroll_to_line(line))
                    });
                }
            }
        }
        self.focus_content(window, cx);
        cx.notify();
    }

    fn note_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        let Some(focused) = window.focused(cx) else {
            return true;
        };
        if focused == self.focus {
            return true;
        }
        if let Some(bar) = &self.find
            && (bar.query.read(cx).focus_handle(cx) == focused
                || bar.replacement.read(cx).focus_handle(cx) == focused)
        {
            return true;
        }
        if let Some(pdf) = self.front_pdf() {
            return pdf.read(cx).focus_handle(cx).contains_focused(window, cx);
        }
        self.active_editor().is_some_and(|editor| {
            let editor = editor.read(cx);
            editor.typing_input().read(cx).focus_handle(cx) == focused
                || editor.input.read(cx).focus_handle(cx) == focused
                || editor
                    .reading
                    .read(cx)
                    .focus_handle(cx)
                    .contains_focused(window, cx)
        })
    }

    fn refind(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(bar), Some(pdf)) = (&self.find, self.front_pdf()) {
            let query = bar.query.read(cx).value().to_string();
            pdf.update(cx, |pdf, cx| pdf.find(&query, cx));
            cx.notify();
            return;
        }
        let (Some(bar), Some(editor)) = (&self.find, self.active_editor().cloned()) else {
            return;
        };
        let query = bar.query.read(cx).value().to_string();
        let case_sensitive = bar.case_sensitive;
        let input = editor.read(cx).typing_input();
        input.update(cx, |input, cx| {
            input.find(&query, case_sensitive, window, cx)
        });
        cx.notify();
    }

    pub fn find_keys(
        &mut self,
        event: &KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let keystroke = &event.keystroke;
        let control = keystroke.modifiers.control;
        let find = crate::actions::pressed("ui-cmd-find", keystroke);
        let replace = crate::actions::pressed("ui-cmd-find-replace", keystroke);
        if find || replace {
            if self.find_target() && self.note_focused(window, cx) {
                self.open_find(replace, window, cx);
                return true;
            }
            return false;
        }
        let Some(bar) = &self.find else { return false };
        let plain_f3 = keystroke.key == "f3"
            && !control
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.platform;
        if plain_f3 && self.note_focused(window, cx) {
            let forward = !keystroke.modifiers.shift;
            if let Some(pdf) = self.front_pdf() {
                pdf.update(cx, |pdf, cx| pdf.find_next(forward, cx));
            } else if let Some(editor) = self.active_editor().cloned() {
                let input = editor.read(cx).typing_input();
                let in_text = !bar.query.read(cx).focus_handle(cx).is_focused(window)
                    && !bar.replacement.read(cx).focus_handle(cx).is_focused(window);
                input.update(cx, |input, cx| {
                    input.find_next(forward, window, cx);
                    let found = input
                        .find_ranges(cx)
                        .and_then(|(ranges, current)| ranges.get(current).cloned());
                    if in_text && let Some(found) = found {
                        input.set_selection(found.start, found.end, cx);
                    }
                });
            }
            cx.notify();
            return true;
        }
        let in_query = bar.query.read(cx).focus_handle(cx).is_focused(window);
        let in_replacement = bar.replacement.read(cx).focus_handle(cx).is_focused(window);
        if !in_query && !in_replacement {
            let plain_escape = keystroke.key == "escape" && !keystroke.modifiers.modified();
            if plain_escape
                && matches!(self.overlay, crate::shell::Overlay::None)
                && self.slash.is_none()
                && !self.selbar.showing()
                && self.note_focused(window, cx)
            {
                self.close_find(window, cx);
                return true;
            }
            return false;
        }
        if let Some(pdf) = self.front_pdf() {
            match keystroke.key.as_str() {
                "escape" => self.close_find(window, cx),
                "enter" => {
                    let forward = !keystroke.modifiers.shift;
                    pdf.update(cx, |pdf, cx| pdf.find_next(forward, cx));
                }
                _ => return false,
            }
            cx.notify();
            return true;
        }
        let Some(editor) = self.active_editor().cloned() else {
            return false;
        };
        let input = editor.read(cx).typing_input();
        match keystroke.key.as_str() {
            "escape" => self.close_find(window, cx),
            "tab" => {
                let target = if in_query {
                    bar.replacement.clone()
                } else {
                    bar.query.clone()
                };
                if let Some(bar) = &mut self.find {
                    bar.replacing = true;
                }
                target.update(cx, |input, cx| input.focus(window, cx));
            }
            "enter" if in_replacement && control => {
                let text = bar.replacement.read(cx).value().to_string();
                input.update(cx, |input, cx| input.replace_every(&text, window, cx));
                self.refind(window, cx);
            }
            "enter" if in_replacement => {
                let text = bar.replacement.read(cx).value().to_string();
                input.update(cx, |input, cx| input.replace_current(&text, window, cx));
                self.refind(window, cx);
            }
            "enter" => {
                let forward = !keystroke.modifiers.shift;
                input.update(cx, |input, cx| input.find_next(forward, window, cx));
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    pub fn find_target(&self) -> bool {
        (self.active_editor().is_some() || self.front_pdf().is_some())
            && (self.find.is_some() || matches!(self.overlay, crate::shell::Overlay::None))
    }

    pub fn render_find(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(pdf) = self.front_pdf() {
            return self.render_pdf_find(pdf, cx);
        }
        let bar = self.find.as_ref()?;
        let editor = self.active_editor().cloned()?;
        let t = *cx.tokens();
        let status = editor.read(cx).typing_input().read(cx).find_status(cx);
        let count = match status {
            Some((_, 0)) => focus_i18n::t("ui-find-none"),
            Some((current, total)) => focus_i18n::t_args(
                "ui-find-count",
                &[("current", current.into()), ("total", total.into())],
            ),
            None => String::new(),
        };
        let input = editor.read(cx).typing_input();
        let (query, replacement, case_sensitive, replacing) = (
            bar.query.clone(),
            bar.replacement.clone(),
            bar.case_sensitive,
            bar.replacing,
        );
        let next = input.clone();
        let previous = input.clone();
        let one = input.clone();
        let every = input.clone();
        let replacement_text = replacement.clone();
        let replacement_all = replacement.clone();
        let row = |child: AnyElement| div().flow_row().items_center().gap(size::S2).child(child);
        Some(
            div()
                .id("find-bar")
                .flex()
                .flex_col()
                .flex_none()
                .gap(size::S2)
                .px(size::S4)
                .py(size::S2)
                .border_b_1()
                .border_color(t.line)
                .bg(t.bg)
                .child(
                    row(div()
                        .w(px(320.))
                        .child(
                            Field::new(&query)
                                .icon(Icon::SEARCH)
                                .suffix(div().mono_sm().text_color(t.ink_3).child(count)),
                        )
                        .into_any_element())
                    .child(
                        IconButton::new(
                            "find-previous",
                            Icon::ARROW_UP,
                            focus_i18n::t("ui-find-previous"),
                        )
                        .shortcut(focus_i18n::keys_text("shift-enter"))
                        .on_click(cx.listener(move |_, _, window, cx| {
                            previous.update(cx, |input, cx| input.find_next(false, window, cx))
                        })),
                    )
                    .child(
                        IconButton::new(
                            "find-next",
                            Icon::CHEVRON_DOWN,
                            focus_i18n::t("ui-find-next"),
                        )
                        .shortcut(focus_i18n::keys_text("enter"))
                        .on_click(cx.listener(move |_, _, window, cx| {
                            next.update(cx, |input, cx| input.find_next(true, window, cx))
                        })),
                    )
                    .child(
                        Checkbox::new("find-case", case_sensitive)
                            .label(focus_i18n::t("ui-find-case"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some(bar) = &mut this.find {
                                    bar.case_sensitive = !bar.case_sensitive;
                                }
                                this.refind(window, cx);
                            })),
                    )
                    .child(div().flex_1())
                    .when(!replacing, |this| {
                        this.child(
                            Button::new("find-show-replace", focus_i18n::t("ui-find-replace"))
                                .kind(ButtonKind::Ghost)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_find(true, window, cx);
                                })),
                        )
                    })
                    .child(
                        IconButton::new("find-close", Icon::CLOSE, focus_i18n::t("ui-find-close"))
                            .shortcut(focus_i18n::keys_text("escape"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_find(window, cx)),
                            ),
                    ),
                )
                .when(replacing, |this| {
                    this.child(
                        row(div()
                            .w(px(320.))
                            .child(Field::new(&replacement))
                            .into_any_element())
                        .child(
                            Button::new("find-replace-one", focus_i18n::t("ui-find-replace"))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let text = replacement_text.read(cx).value().to_string();
                                    one.update(cx, |input, cx| {
                                        input.replace_current(&text, window, cx)
                                    });
                                    this.refind(window, cx);
                                })),
                        )
                        .child(
                            Button::new("find-replace-all", focus_i18n::t("ui-find-replace-all"))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let text = replacement_all.read(cx).value().to_string();
                                    every.update(cx, |input, cx| {
                                        input.replace_every(&text, window, cx)
                                    });
                                    this.refind(window, cx);
                                })),
                        )
                        .child(
                            div()
                                .ui_sm()
                                .text_color(t.ink_3)
                                .child(focus_i18n::t("ui-find-replace-keys")),
                        ),
                    )
                })
                .into_any_element(),
        )
    }

    fn render_pdf_find(
        &mut self,
        pdf: gpui::Entity<focus_view_media::PdfView>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let bar = self.find.as_ref()?;
        let t = *cx.tokens();
        let count = match pdf.read(cx).find_status() {
            Some((_, 0)) if pdf.read(cx).find_done() => focus_i18n::t("ui-find-none"),
            Some((current, total)) if total > 0 => focus_i18n::t_args(
                "ui-find-count",
                &[("current", current.into()), ("total", total.into())],
            ),
            _ => String::new(),
        };
        let (next, previous) = (pdf.clone(), pdf);
        Some(
            div()
                .id("find-bar")
                .flow_row()
                .flex_none()
                .items_center()
                .gap(size::S2)
                .px(size::S4)
                .py(size::S2)
                .border_b_1()
                .border_color(t.line)
                .bg(t.bg)
                .child(
                    div().w(px(320.)).child(
                        Field::new(&bar.query)
                            .icon(Icon::SEARCH)
                            .suffix(div().mono_sm().text_color(t.ink_3).child(count)),
                    ),
                )
                .child(
                    IconButton::new(
                        "find-previous",
                        Icon::ARROW_UP,
                        focus_i18n::t("ui-find-previous"),
                    )
                    .shortcut(focus_i18n::keys_text("shift-enter"))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        previous.update(cx, |pdf, cx| pdf.find_next(false, cx))
                    })),
                )
                .child(
                    IconButton::new(
                        "find-next",
                        Icon::CHEVRON_DOWN,
                        focus_i18n::t("ui-find-next"),
                    )
                    .shortcut(focus_i18n::keys_text("enter"))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        next.update(cx, |pdf, cx| pdf.find_next(true, cx))
                    })),
                )
                .child(div().flex_1())
                .child(
                    IconButton::new("find-close", Icon::CLOSE, focus_i18n::t("ui-find-close"))
                        .shortcut(focus_i18n::keys_text("escape"))
                        .on_click(cx.listener(|this, _, window, cx| this.close_find(window, cx))),
                )
                .into_any_element(),
        )
    }
}
