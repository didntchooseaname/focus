use focus_kit::{Button, ButtonKind, Field, Flow as _, Icon, Tokens, Type, ellipsis, size};
use focus_md::highlight::{self, Language};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, Focusable as _, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, Pixels, Point, StatefulInteractiveElement, Styled,
    Subscription, Window, div, point, prelude::FluentBuilder, px,
};
use gpui_component::input::{Escape, IndentInline, Input, InputEvent, InputState, OutdentInline};

use super::inplace::Edit;
use super::{LanguageChosen, ReadingView, VariableChosen, VariableMenu};

const ROWS: usize = 8;
const ROW: f32 = 26.;
const WIDTH: f32 = 260.;
const VARIABLES_WIDTH: f32 = 360.;
const VARIABLES_ROW: f32 = 30.;
const VARIABLE_FIELD: f32 = 26.;
const VARIABLE_NAME: f32 = 104.;

pub(super) struct Picker {
    kind: PickerKind,
    line: u32,
    input: Entity<InputState>,
    selected: usize,
    position: Point<Pixels>,
    _subscription: Subscription,
    values: Vec<ValueField>,
    adding: Option<Adding>,
}

impl Picker {
    pub(super) fn is_variables(&self) -> bool {
        self.kind == PickerKind::Variable
    }
}

struct ValueField {
    name: String,
    input: Entity<InputState>,
    given: String,
    secret: bool,
    _subscription: Subscription,
}

struct Adding {
    name: Entity<InputState>,
    value: Entity<InputState>,
    invalid: bool,
    _subscriptions: [Subscription; 2],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuFocus {
    Filter,
    Value(usize),
    AddName,
    AddValue,
    Elsewhere,
}

fn new_variable_name(text: &str) -> Option<String> {
    let name = variable_name(text)?.to_ascii_uppercase();
    (name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
    .then_some(name)
}

fn score(query: &str, text: &str) -> Option<i32> {
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let mut at = 0;
    let mut score = 0;
    let mut last: Option<usize> = None;
    for q in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let found = text[at..].iter().position(|c| *c == q)? + at;
        score += match last {
            Some(l) if l + 1 == found => 3,
            _ if found == 0 => 4,
            _ => 1,
        };
        last = Some(found);
        at = found + 1;
    }
    Some(score * 100 - text.len() as i32)
}

pub(super) fn matches(query: &str) -> Vec<&'static Language> {
    let mut found: Vec<(i32, &'static Language)> = highlight::languages()
        .iter()
        .filter_map(|l| {
            let label = focus_i18n::t(&format!("reading-language-{}", l.id));
            score(query, &label).max(score(query, l.id)).map(|s| (s, l))
        })
        .collect();
    if !query.trim().is_empty() {
        found.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
    }
    found.into_iter().map(|(_, l)| l).collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PickerKind {
    Language,
    Variable,
    CalloutType { start: usize, end: usize },
}

const CALLOUT_TYPES: [&str; 13] = [
    "note", "abstract", "info", "todo", "tip", "success", "question", "warning", "failure",
    "danger", "bug", "example", "quote",
];

fn callout_type(text: &str) -> Option<String> {
    let name = text.trim().to_lowercase();
    (!name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_'))
    .then_some(name)
}

struct Choice {
    id: String,
    label: String,
    detail: String,
    listed: bool,
}

fn variable_name(text: &str) -> Option<&str> {
    let name = text
        .trim()
        .trim_start_matches('$')
        .trim_start_matches('{')
        .trim_end_matches('}');
    (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')).then_some(name)
}

impl ReadingView {
    pub(super) fn open_picker(
        &mut self,
        line: u32,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_picker_of(PickerKind::Language, line, position, window, cx);
    }

    pub(super) fn open_picker_of(
        &mut self,
        kind: PickerKind,
        line: u32,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = match kind {
            PickerKind::Language => "reading-language-search",
            PickerKind::Variable => "reading-variable-search",
            PickerKind::CalloutType { .. } => "reading-menu-callout-type",
        };
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder(focus_i18n::t(placeholder)));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut ReadingView, input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    if let Some(picker) = this.picker.as_mut() {
                        picker.selected = 0;
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    let query = input.read(cx).value().to_string();
                    let Some((kind, selected)) = this.picker.as_ref().map(|p| (p.kind, p.selected))
                    else {
                        return;
                    };
                    let choices = this.choices(kind, &query);
                    match choices.into_iter().nth(selected) {
                        Some(choice) => this.choose(&choice.id, window, cx),
                        None if kind == PickerKind::Variable => this.last_row(&query, window, cx),
                        None => {}
                    }
                }
                _ => {}
            },
        );
        input.update(cx, |state, cx| state.focus(window, cx));
        let adding = (kind == PickerKind::Variable
            && self.code_variables.as_ref().is_some_and(|v| v.engaged))
        .then(|| self.adding_fields(window, cx));
        self.picker = Some(Picker {
            kind,
            line,
            input,
            selected: 0,
            position,
            _subscription: subscription,
            values: Vec::new(),
            adding,
        });
        self.sync_picker(window, cx);
        cx.notify();
    }

    pub(super) fn sync_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(variables) = self.code_variables.clone() else {
            return;
        };
        let Some(picker) = self.picker.as_mut().filter(|p| p.is_variables()) else {
            return;
        };
        if !variables.engaged {
            picker.values.clear();
            picker.adding = None;
            return;
        }
        picker.values.retain(|field| {
            variables.list.iter().any(|v| v.name == field.name)
                || field.input.focus_handle(cx).is_focused(window)
        });
        let mut created = Vec::new();
        for variable in &variables.list {
            match picker.values.iter_mut().find(|f| f.name == variable.name) {
                Some(field) => {
                    let typing = field.input.focus_handle(cx).is_focused(window);
                    if field.given != variable.value || field.secret != variable.secret {
                        field.given = variable.value.clone();
                        field.secret = variable.secret;
                        if !typing {
                            let value = variable.value.clone();
                            field
                                .input
                                .update(cx, |input, cx| input.set_value(value, window, cx));
                        }
                    }
                }
                None => created.push(variable.clone()),
            }
        }
        if picker.adding.is_none() {
            let adding = self.adding_fields(window, cx);
            if let Some(picker) = self.picker.as_mut() {
                picker.adding = Some(adding);
            }
        }
        for variable in created {
            let field = self.value_field(variable, window, cx);
            if let Some(picker) = self.picker.as_mut() {
                picker.values.push(field);
            }
        }
    }

    fn value_field(
        &mut self,
        variable: crate::CodeVariable,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ValueField {
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder(variable.description.clone());
            input.set_value(variable.value.clone(), window, cx);
            input
        });
        let name = variable.name.clone();
        let subscription = cx.subscribe_in(
            &input,
            window,
            move |this: &mut ReadingView, _, event: &InputEvent, window, cx| match event {
                InputEvent::Blur => this.commit_value(&name, cx),
                InputEvent::PressEnter { .. } => {
                    this.commit_value(&name, cx);
                    this.focus_filter(window, cx);
                }
                InputEvent::Change | InputEvent::Focus => {}
            },
        );
        ValueField {
            name: variable.name,
            input,
            given: variable.value,
            secret: variable.secret,
            _subscription: subscription,
        }
    }

    fn adding_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Adding {
        let name = cx.new(|cx| {
            InputState::new(window, cx).placeholder(focus_i18n::t("reading-variable-add-name"))
        });
        let value = cx.new(|cx| {
            InputState::new(window, cx).placeholder(focus_i18n::t("reading-variable-add-value"))
        });
        let on_name = cx.subscribe_in(
            &name,
            window,
            |this: &mut ReadingView, input, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let valid = new_variable_name(input.read(cx).value().as_ref()).is_some();
                    if let Some(adding) = this.picker.as_mut().and_then(|p| p.adding.as_mut()) {
                        adding.invalid = !valid;
                        if valid {
                            adding.value.update(cx, |input, cx| input.focus(window, cx));
                        }
                    }
                    cx.notify();
                }
                InputEvent::Change => {
                    if let Some(adding) = this.picker.as_mut().and_then(|p| p.adding.as_mut()) {
                        adding.invalid = false;
                    }
                }
                _ => {}
            },
        );
        let on_value = cx.subscribe_in(
            &value,
            window,
            |this: &mut ReadingView, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.add_variable(window, cx);
                }
            },
        );
        Adding {
            name,
            value,
            invalid: false,
            _subscriptions: [on_name, on_value],
        }
    }

    fn value_of(&self, name: &str) -> Option<&ValueField> {
        self.picker.as_ref()?.values.iter().find(|f| f.name == name)
    }

    fn commit_value(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(field) = self.value_of(name) else {
            return;
        };
        let value = field.input.read(cx).value().trim().to_string();
        if value != field.given {
            cx.emit(VariableMenu::Set {
                name: name.to_string(),
                value,
            });
        }
    }

    fn add_variable(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(adding) = self.picker.as_mut().and_then(|p| p.adding.as_mut()) else {
            return;
        };
        let name = new_variable_name(adding.name.read(cx).value().as_ref());
        let value = adding.value.read(cx).value().trim().to_string();
        let Some(name) = name else {
            adding.invalid = true;
            adding.name.update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
            return;
        };
        if value.is_empty() {
            return;
        }
        for input in [adding.name.clone(), adding.value.clone()] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.emit(VariableMenu::Set { name, value });
        self.focus_filter(window, cx);
    }

    fn focus_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = &self.picker {
            picker.input.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn menu_focus(&self, window: &Window, cx: &gpui::App) -> MenuFocus {
        let Some(picker) = &self.picker else {
            return MenuFocus::Elsewhere;
        };
        let focused = |input: &Entity<InputState>| input.focus_handle(cx).is_focused(window);
        if focused(&picker.input) {
            return MenuFocus::Filter;
        }
        if let Some(i) = picker.values.iter().position(|f| focused(&f.input)) {
            return MenuFocus::Value(i);
        }
        match &picker.adding {
            Some(adding) if focused(&adding.name) => MenuFocus::AddName,
            Some(adding) if focused(&adding.value) => MenuFocus::AddValue,
            _ => MenuFocus::Elsewhere,
        }
    }

    fn dismiss_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Picker> {
        let picker = self.close_picker(cx);
        if picker.is_some() {
            window.focus(&self.focus);
        }
        picker
    }

    fn close_picker(&mut self, cx: &mut Context<Self>) -> Option<Picker> {
        let names: Vec<String> = self
            .picker
            .as_ref()
            .map(|p| p.values.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default();
        for name in names {
            self.commit_value(&name, cx);
        }
        self.picker.take()
    }

    fn choices(&self, kind: PickerKind, query: &str) -> Vec<Choice> {
        match kind {
            PickerKind::Language => matches(query)
                .into_iter()
                .map(|l| Choice {
                    id: l.id.to_string(),
                    label: focus_i18n::t(&format!("reading-language-{}", l.id)),
                    detail: l.id.to_string(),
                    listed: false,
                })
                .collect(),
            PickerKind::Variable => {
                let list = self
                    .code_variables
                    .as_deref()
                    .map(|v| v.list.as_slice())
                    .unwrap_or_default();
                let mut found: Vec<(i32, &crate::CodeVariable)> = list
                    .iter()
                    .filter_map(|v| score(query, &v.name).map(|s| (s, v)))
                    .collect();
                if !query.trim().is_empty() {
                    found.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
                }
                let typed = variable_name(query)
                    .filter(|name| !list.iter().any(|v| v.name.eq_ignore_ascii_case(name)));
                found
                    .into_iter()
                    .map(|(_, variable)| Choice {
                        id: variable.name.clone(),
                        label: format!("${}", variable.name),
                        detail: variable.description.clone(),
                        listed: true,
                    })
                    .chain(typed.map(|name| Choice {
                        id: name.to_string(),
                        label: format!("${name}"),
                        detail: focus_i18n::t("reading-variable-new"),
                        listed: false,
                    }))
                    .collect()
            }
            PickerKind::CalloutType { .. } => {
                let label = |id: &str| focus_i18n::t(&format!("reading-callout-{id}"));
                let mut found: Vec<(i32, &str)> = CALLOUT_TYPES
                    .iter()
                    .filter_map(|&id| {
                        score(query, &label(id))
                            .max(score(query, id))
                            .map(|s| (s, id))
                    })
                    .collect();
                if !query.trim().is_empty() {
                    found.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
                }
                let typed =
                    callout_type(query).filter(|name| !CALLOUT_TYPES.contains(&name.as_str()));
                found
                    .into_iter()
                    .map(|(_, id)| Choice {
                        id: id.to_string(),
                        label: label(id),
                        detail: id.to_string(),
                        listed: false,
                    })
                    .chain(typed.map(|name| Choice {
                        label: name.clone(),
                        detail: String::new(),
                        id: name,
                        listed: false,
                    }))
                    .collect()
            }
        }
    }

    fn last_row(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let typed = new_variable_name(query);
        match self.picker.as_ref().and_then(|p| p.adding.as_ref()) {
            Some(adding) => {
                let name = adding.name.clone();
                name.update(cx, |input, cx| {
                    if let Some(typed) = typed {
                        input.set_value(typed, window, cx);
                    }
                    input.focus(window, cx);
                });
            }
            None => {
                self.dismiss_picker(window, cx);
                cx.emit(VariableMenu::NewEngagement);
            }
        }
        cx.notify();
    }

    fn choose(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = self.dismiss_picker(window, cx) {
            match picker.kind {
                PickerKind::Language => cx.emit(LanguageChosen {
                    line: picker.line,
                    lang: id.to_string(),
                }),
                PickerKind::Variable => cx.emit(VariableChosen {
                    line: picker.line,
                    name: id.to_string(),
                }),
                PickerKind::CalloutType { start, end } => {
                    let caret = self.field_state(window, cx).map_or(start, |(_, _, head)| {
                        if head >= end {
                            head + id.len() - (end - start)
                        } else {
                            head
                        }
                    });
                    self.apply(
                        Edit {
                            range: start..end,
                            text: id.to_string(),
                            caret,
                        },
                        window,
                        cx,
                    );
                }
            }
            cx.notify();
        }
    }

    fn picker_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu_key(
            &event.keystroke.key,
            event.keystroke.modifiers.shift,
            window,
            cx,
        ) {
            cx.stop_propagation();
        }
    }

    fn picker_action(
        &mut self,
        key: &str,
        back: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.menu_key(key, back, window, cx) {
            cx.propagate();
        }
    }

    fn menu_key(
        &mut self,
        key: &str,
        back: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(picker) = self.picker.as_ref() else {
            return false;
        };
        let count = (self
            .choices(picker.kind, picker.input.read(cx).value().as_ref())
            .len()
            + usize::from(picker.is_variables()))
        .max(1);
        let focus = self.menu_focus(window, cx);
        match (focus, key) {
            (MenuFocus::Filter | MenuFocus::Elsewhere, "down") => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.selected = (picker.selected + 1) % count;
                }
            }
            (MenuFocus::Filter | MenuFocus::Elsewhere, "up") => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.selected = (picker.selected + count - 1) % count;
                }
            }
            (MenuFocus::Filter | MenuFocus::Elsewhere, "escape") => {
                self.dismiss_picker(window, cx);
            }
            (MenuFocus::Filter, "tab") if !back && picker.is_variables() => {
                let query = picker.input.read(cx).value().to_string();
                let selected = picker.selected;
                let choice = self
                    .choices(PickerKind::Variable, &query)
                    .into_iter()
                    .nth(selected);
                let field = match choice {
                    Some(choice) if choice.listed => picker
                        .values
                        .iter()
                        .find(|f| f.name == choice.id)
                        .map(|f| f.input.clone()),
                    Some(_) => None,
                    None => picker.adding.as_ref().map(|a| a.name.clone()),
                };
                if let Some(field) = field {
                    field.update(cx, |input, cx| input.focus(window, cx));
                }
            }
            (MenuFocus::Value(i), "escape") => {
                if let Some(field) = picker.values.get(i) {
                    let (input, given) = (field.input.clone(), field.given.clone());
                    input.update(cx, |input, cx| input.set_value(given, window, cx));
                }
                self.focus_filter(window, cx);
            }
            (MenuFocus::Value(_), "tab") => self.focus_filter(window, cx),
            (MenuFocus::AddName, "tab") if !back => {
                if let Some(adding) = &picker.adding {
                    let value = adding.value.clone();
                    value.update(cx, |input, cx| input.focus(window, cx));
                }
            }
            (MenuFocus::AddValue, "tab") if back => {
                if let Some(adding) = &picker.adding {
                    let name = adding.name.clone();
                    name.update(cx, |input, cx| input.focus(window, cx));
                }
            }
            (MenuFocus::AddName | MenuFocus::AddValue, "tab") => self.focus_filter(window, cx),
            (MenuFocus::AddName | MenuFocus::AddValue, "escape") => {
                if let Some(adding) = self.picker.as_mut().and_then(|p| p.adding.as_mut()) {
                    adding.invalid = false;
                    for input in [adding.name.clone(), adding.value.clone()] {
                        input.update(cx, |input, cx| input.set_value("", window, cx));
                    }
                }
                self.focus_filter(window, cx);
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    pub(super) fn render_picker(
        &self,
        t: &Tokens,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let picker = self.picker.as_ref()?;
        if picker.is_variables() {
            return Some(self.render_variables(picker, t, window, cx));
        }
        let found = self.choices(picker.kind, picker.input.read(cx).value().as_ref());
        let first = picker.selected.saturating_sub(ROWS - 1);
        let empty = match picker.kind {
            PickerKind::Language => Some("reading-language-none"),
            PickerKind::Variable => Some("reading-variable-none"),
            PickerKind::CalloutType { .. } => None,
        };
        let rows = found
            .iter()
            .enumerate()
            .skip(first)
            .take(ROWS)
            .map(|(i, choice)| {
                let id = choice.id.clone();
                div()
                    .id(("language", i))
                    .flex()
                    .items_center()
                    .gap(size::S2)
                    .h(px(ROW))
                    .px(size::S3)
                    .cursor_pointer()
                    .when(i == picker.selected, |this| this.bg(t.card_2))
                    .hover(move |s| s.bg(t.card_2))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| this.choose(&id, window, cx)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .ui()
                            .text_color(t.ink)
                            .child(choice.label.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .mono_sm()
                            .text_color(t.ink_3)
                            .child(choice.detail.clone()),
                    )
                    .into_any_element()
            });
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(picker.position + point(px(0.), px(4.)))
                    .snap_to_window()
                    .child(focus_kit::fade_in(
                        "fade-language-picker",
                        focus_kit::floating(t)
                            .id("language-picker")
                            .occlude()
                            .w(px(WIDTH))
                            .flex()
                            .flex_col()
                            .capture_key_down(cx.listener(
                                |this, event: &KeyDownEvent, window, cx| {
                                    this.picker_key(event, window, cx)
                                },
                            ))
                            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                                this.picker_action("escape", false, window, cx)
                            }))
                            .on_action(cx.listener(|this, _: &IndentInline, window, cx| {
                                this.picker_action("tab", false, window, cx)
                            }))
                            .on_action(cx.listener(|this, _: &OutdentInline, window, cx| {
                                this.picker_action("tab", true, window, cx)
                            }))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                if let Some(picker) = this.close_picker(cx) {
                                    this.picker_dismissed = Some(picker.line);
                                    cx.notify();
                                }
                            }))
                            .child(
                                div()
                                    .p(size::S2)
                                    .child(Field::new(&picker.input).icon(Icon::SEARCH)),
                            )
                            .child(div().h(px(1.)).bg(t.line))
                            .child(div().flex().flex_col().py(size::S1).children(rows))
                            .when_some(empty.filter(|_| found.is_empty()), |this, empty| {
                                this.child(
                                    div()
                                        .px(size::S3)
                                        .py(size::S2)
                                        .ui_sm()
                                        .text_color(t.ink_3)
                                        .child(focus_i18n::t(empty)),
                                )
                            }),
                        cx,
                    )),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

const MASK: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

fn value_input(
    input: &Entity<InputState>,
    invalid: bool,
    secret: bool,
    t: &Tokens,
    window: &Window,
    cx: &gpui::App,
) -> gpui::Div {
    let focused = input.focus_handle(cx).is_focused(window);
    let masked = secret && !focused && !input.read(cx).value().is_empty();
    let border = if invalid {
        t.danger
    } else if focused {
        t.ink
    } else {
        t.line_2
    };
    let state = input.clone();
    div()
        .flex_1()
        .min_w_0()
        .flow_row()
        .items_center()
        .h(px(VARIABLE_FIELD))
        .px(size::S2)
        .rounded(size::RADIUS)
        .border_1()
        .border_color(border)
        .bg(t.bg_2)
        .text_color(t.ink)
        .when(!focused && !invalid, |this| {
            this.hover(move |s| s.border_color(t.ink_3))
        })
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            if !window.default_prevented() {
                state.update(cx, |input, cx| input.focus(window, cx));
                window.prevent_default();
            }
        })
        .child(div().flex_1().min_w_0().mono().map(|this| {
            if masked {
                this.cursor_text().child(MASK)
            } else {
                this.child(
                    Input::new(input)
                        .appearance(false)
                        .text_size(px(12.5))
                        .line_height(px(20.))
                        .px_0()
                        .py_0()
                        .h(px(20.)),
                )
            }
        }))
}

impl ReadingView {
    fn render_variables(
        &self,
        picker: &Picker,
        t: &Tokens,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = *t;
        let query = picker.input.read(cx).value().to_string();
        let found = self.choices(PickerKind::Variable, &query);
        let engaged = self.code_variables.as_ref().is_some_and(|v| v.engaged);
        let last = found.len();
        let first = picker.selected.saturating_sub(ROWS - 1);
        let row = |i: usize| {
            let current = i == picker.selected;
            div()
                .id(("variable-row", i))
                .relative()
                .flow_row()
                .items_center()
                .gap(size::S2)
                .h(px(VARIABLES_ROW))
                .px(size::S3)
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
                .when(!current, |row| row.hover(move |s| s.bg(t.card_2)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let rows: Vec<AnyElement> = found
            .iter()
            .enumerate()
            .chain(std::iter::once((
                last,
                &Choice {
                    id: String::new(),
                    label: String::new(),
                    detail: String::new(),
                    listed: false,
                },
            )))
            .skip(first)
            .take(ROWS)
            .map(|(i, choice)| {
                if i == last {
                    return match &picker.adding {
                        Some(adding) if engaged => row(i)
                            .child(div().flex_none().w(px(VARIABLE_NAME)).flow_row().child(
                                value_input(&adding.name, adding.invalid, false, &t, window, cx),
                            ))
                            .child(value_input(&adding.value, false, false, &t, window, cx))
                            .into_any_element(),
                        _ => {
                            row(i)
                                .child(div().flex_1().min_w_0().ui_sm().text_color(t.ink_3).child(
                                    ellipsis(focus_i18n::t("reading-variable-no-engagement")),
                                ))
                                .child(
                                    Button::new(
                                        "variable-new-engagement",
                                        focus_i18n::t("reading-variable-new-engagement"),
                                    )
                                    .kind(ButtonKind::Ghost)
                                    .icon(Icon::PLUS)
                                    .on_click(cx.listener(
                                        |this, _, window, cx| {
                                            this.dismiss_picker(window, cx);
                                            cx.emit(VariableMenu::NewEngagement);
                                            cx.notify();
                                        },
                                    )),
                                )
                                .into_any_element()
                        }
                    };
                }
                let id = choice.id.clone();
                let name = div()
                    .id(("variable-name", i))
                    .flex_none()
                    .w(px(VARIABLE_NAME))
                    .h_full()
                    .flow_row()
                    .items_center()
                    .cursor_pointer()
                    .mono()
                    .text_color(if i == picker.selected { t.ink } else { t.ink_2 })
                    .on_click(cx.listener(move |this, _, window, cx| this.choose(&id, window, cx)))
                    .child(ellipsis(choice.label.clone()));
                let field = choice
                    .listed
                    .then(|| picker.values.iter().find(|f| f.name == choice.id))
                    .flatten();
                let rest = match field {
                    Some(field) => value_input(&field.input, false, field.secret, &t, window, cx)
                        .into_any_element(),
                    None => div()
                        .flex_1()
                        .min_w_0()
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(ellipsis(choice.detail.clone()))
                        .into_any_element(),
                };
                row(i).child(name).child(rest).into_any_element()
            })
            .collect();
        gpui::deferred(
            gpui::anchored()
                .position(picker.position + point(px(0.), px(4.)))
                .snap_to_window()
                .child(focus_kit::fade_in(
                    "fade-variables-menu",
                    focus_kit::floating(&t)
                        .id("variables-menu")
                        .occlude()
                        .w(px(VARIABLES_WIDTH))
                        .flex()
                        .flex_col()
                        .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                            this.picker_key(event, window, cx)
                        }))
                        .on_action(cx.listener(|this, _: &Escape, window, cx| {
                            this.picker_action("escape", false, window, cx)
                        }))
                        .on_action(cx.listener(|this, _: &IndentInline, window, cx| {
                            this.picker_action("tab", false, window, cx)
                        }))
                        .on_action(cx.listener(|this, _: &OutdentInline, window, cx| {
                            this.picker_action("tab", true, window, cx)
                        }))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            if let Some(picker) = this.close_picker(cx) {
                                this.picker_dismissed = Some(picker.line);
                                cx.notify();
                            }
                        }))
                        .child(
                            div()
                                .p(size::S2)
                                .child(Field::new(&picker.input).icon(Icon::SEARCH)),
                        )
                        .child(div().h(px(1.)).bg(t.line))
                        .when(found.is_empty(), |this| {
                            this.child(
                                div()
                                    .flow_row()
                                    .items_center()
                                    .h(px(VARIABLES_ROW))
                                    .px(size::S3)
                                    .ui_sm()
                                    .text_color(t.ink_3)
                                    .child(focus_i18n::t("reading-variable-none")),
                            )
                        })
                        .child(div().flex().flex_col().py(size::S1).children(rows)),
                    cx,
                )),
        )
        .with_priority(1)
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{callout_type, new_variable_name, score, variable_name};

    #[test]
    fn a_typed_variable_is_a_plain_name() {
        assert_eq!(variable_name(" $TARGET "), Some("TARGET"));
        assert_eq!(variable_name("${LHOST}"), Some("LHOST"));
        assert_eq!(variable_name("two words"), None);
        assert_eq!(variable_name("$"), None);
    }

    #[test]
    fn a_new_variable_name_is_upper_case_and_starts_with_a_letter() {
        assert_eq!(
            new_variable_name(" $proxy_port ").as_deref(),
            Some("PROXY_PORT")
        );
        assert_eq!(new_variable_name("${WEB}").as_deref(), Some("WEB"));
        assert_eq!(new_variable_name("9LIVES"), None);
        assert_eq!(new_variable_name("é"), None);
    }

    #[test]
    fn fuzzy_score_prefers_prefixes_and_runs() {
        assert!(score("py", "python").unwrap() > score("py", "happy").unwrap_or(i32::MIN));
        assert!(score("ps", "powershell").is_some());
        assert!(score("xz", "python").is_none());
        assert!(score("", "bash").is_some());
    }

    #[test]
    fn a_typed_callout_type_is_a_plain_name() {
        assert_eq!(callout_type(" Custom-1 ").as_deref(), Some("custom-1"));
        assert_eq!(callout_type("two words"), None);
        assert_eq!(callout_type(""), None);
    }
}
