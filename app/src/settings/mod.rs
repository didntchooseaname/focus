mod actions_editor;
mod ai_page;
mod history_page;
mod mcp_editor;
mod recent;
pub mod schema;
pub mod shortcuts;
mod transfer;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use focus_kit::Flow as _;
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Checkbox, Field, Icon, IconButton, Segment, Segments,
    TextArea, TreeRow, Type, size,
};
use gpui::{
    AnyElement, App, AppContext, Bounds, Context, Corner, Entity, EventEmitter, FocusHandle,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Pixels, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement, Styled, Subscription, Window, anchored, canvas,
    deferred, div, prelude::FluentBuilder, px,
};
use gpui_component::input::{InputEvent, InputState};

use schema::{Control, Prefs, SETTINGS, Section, Setting, Value, ascii_fold};

#[derive(Clone, Default)]
pub struct Facts {
    pub vault: Option<String>,
    pub notes: usize,
    pub vault_open_ms: Option<f32>,
    pub index_ms: Option<f32>,
    pub window_ms: Option<f32>,
    pub first_frame_ms: Option<f32>,
    pub rss: Option<u64>,
    pub recent: Vec<std::path::PathBuf>,
    pub engagement: Option<String>,
    pub history: Option<std::sync::Arc<focus_history::History>>,
}

pub enum SettingsEvent {
    Close,
    OpenVault,
    SwitchVault(std::path::PathBuf),
    Action(&'static str),
}

const AI_WITHOUT_SWITCH: [&str; 6] = [
    "ai.enabled",
    "ai.mcp_command",
    "ai.skill",
    "ai.agents_md",
    "ai.agent_socket",
    "ai.skill_status",
];

gpui::actions!(settings_list, [PageUp, PageDown, FirstItem, LastItem]);

#[derive(Clone, Copy)]
enum Scroll {
    PageUp,
    PageDown,
    Top,
    Bottom,
}

struct ListKeysBound;
impl gpui::Global for ListKeysBound {}

pub struct SettingsView {
    focus: FocusHandle,
    pub section: Section,
    prefs: Entity<Prefs>,
    search: Entity<InputState>,
    inputs: HashMap<&'static str, Entity<InputState>>,
    errors: HashMap<&'static str, SharedString>,
    open_menu: Option<&'static str>,
    select_focus: HashMap<&'static str, FocusHandle>,
    menu_closed: Option<Instant>,
    menu_cursor: focus_kit::MenuCursor,
    menu_focus: FocusHandle,
    acted: Option<(&'static str, u64)>,
    pub facts: Facts,
    scroll: ScrollHandle,
    ai: ai_page::AiState,
    history: history_page::HistoryState,
    actions_draft: Option<actions_editor::Draft>,
    server_draft: Option<mcp_editor::ServerDraft>,
    server_checks: HashMap<String, mcp_editor::Check>,
    recording: Option<&'static str>,
    shortcut_conflict: Option<shortcuts::Conflict>,
    shortcut_error: Option<SharedString>,
    intercept: Option<Subscription>,
    notice: Option<(bool, SharedString)>,
    recent_focus: Vec<FocusHandle>,
    sections_focus: FocusHandle,
    row_focus: HashMap<&'static str, FocusHandle>,
    row_bounds: Rc<RefCell<HashMap<&'static str, Bounds<Pixels>>>>,
    focused_row: Option<&'static str>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SettingsEvent> for SettingsView {}

const MULTILINE: &[&str] = &["ai.preprompt", "ops.variables", "ops.library_folders"];

fn format_number(value: f64) -> String {
    if value.fract() == 0. {
        format!("{value:.0}")
    } else {
        focus_i18n::format_decimal(
            value,
            if (value * 10.).fract().abs() < 1e-9 {
                1
            } else {
                2
            },
        )
    }
}

pub(super) fn single_line(text: &str, _: &mut Context<InputState>) -> bool {
    !text.contains(['\n', '\r'])
}

fn field_row(t: &focus_kit::Tokens, label: String, control: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(size::S1)
        .child(div().ui_sm().text_color(t.ink_3).child(label))
        .child(control)
}

fn unit_label(unit: &str) -> String {
    match unit {
        "" => String::new(),
        "%" => focus_i18n::t("settings-unit-percent"),
        other => focus_i18n::t(&format!("settings-unit-{other}")),
    }
}

impl SettingsView {
    pub fn new(
        prefs: Entity<Prefs>,
        section: Section,
        facts: Facts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(focus_i18n::t("settings-search"))
                .validate(single_line)
        });
        let mut subscriptions =
            vec![
                cx.subscribe(&search, |this: &mut Self, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.scroll.set_offset(gpui::point(px(0.), px(0.)));
                        cx.notify();
                    }
                }),
            ];
        let mut inputs = HashMap::new();
        for setting in SETTINGS {
            let (initial, secret, placeholder) =
                match (&setting.control, prefs.read(cx).get(setting.key)) {
                    (Control::Number { .. }, Some(Value::Number(n))) => {
                        (format_number(*n), false, String::new())
                    }
                    (Control::Text { secret }, Some(Value::Text(text))) => {
                        (text.to_string(), *secret, setting.text().placeholder)
                    }
                    _ => continue,
                };
            let multiline = MULTILINE.contains(&setting.key);
            let input = cx.new(|cx| {
                let state = InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(secret)
                    .default_value(initial);
                if multiline {
                    state.multi_line(true).auto_grow(2, 6)
                } else {
                    state.validate(single_line)
                }
            });
            let key = setting.key;
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this: &mut Self, input, event: &InputEvent, window, cx| {
                    if key == "ai.provider_key" {
                        if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                            this.store_provider_key(window, cx);
                        }
                    } else if key == "ai.key" {
                        if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                            this.hand_over_key(window, cx);
                        }
                    } else if matches!(event, InputEvent::Change) {
                        let text = input.read(cx).value().to_string();
                        this.commit(key, &text, cx);
                    }
                },
            ));
            inputs.insert(setting.key, input);
        }
        let mut view = SettingsView {
            focus: {
                if !cx.has_global::<ListKeysBound>() {
                    const IN_PANEL: Option<&str> = Some("SettingsPanel");
                    cx.bind_keys([
                        gpui::KeyBinding::new("pageup", PageUp, IN_PANEL),
                        gpui::KeyBinding::new("pagedown", PageDown, IN_PANEL),
                        gpui::KeyBinding::new("home", FirstItem, IN_PANEL),
                        gpui::KeyBinding::new("end", LastItem, IN_PANEL),
                    ]);
                    cx.set_global(ListKeysBound);
                }
                cx.focus_handle()
            },
            section,
            prefs,
            search,
            inputs,
            errors: HashMap::new(),
            open_menu: None,
            select_focus: SETTINGS
                .iter()
                .filter(|setting| matches!(&setting.control, Control::Choice(options) if options.len() > 4))
                .map(|setting| (setting.key, cx.focus_handle()))
                .collect(),
            menu_closed: None,
            menu_cursor: focus_kit::MenuCursor::default(),
            menu_focus: cx.focus_handle(),
            acted: None,
            recent_focus: facts
                .recent
                .iter()
                .map(|_| cx.focus_handle().tab_stop(true))
                .collect(),
            sections_focus: cx.focus_handle().tab_stop(true),
            row_focus: SETTINGS
                .iter()
                .map(|setting| (setting.key, cx.focus_handle()))
                .collect(),
            row_bounds: Rc::default(),
            focused_row: None,
            facts,
            scroll: ScrollHandle::new(),
            ai: ai_page::AiState::new(),
            history: history_page::HistoryState::default(),
            actions_draft: None,
            server_draft: None,
            server_checks: HashMap::new(),
            recording: None,
            shortcut_conflict: None,
            shortcut_error: None,
            intercept: None,
            notice: None,
            _subscriptions: subscriptions,
        };
        view.refresh_key_status(cx);
        view.refresh_agents();
        view.refresh_history_size(cx);
        view.refresh_provider_key(cx);
        view.migrate_server_secrets(cx);
        if let Ok(request) = std::env::var("FOCUS_SETTINGS_OPEN") {
            view.open_for_capture(&request, window, cx);
        }
        view
    }

    fn open_for_capture(&mut self, request: &str, window: &mut Window, cx: &mut Context<Self>) {
        match request.split_once(':').unwrap_or((request, "")) {
            ("action", _) => self.open_draft(None, window, cx),
            ("server", _) => self.open_server_draft(None, window, cx),
            ("menu", key) => {
                self.open_menu = schema::setting(key).map(|setting| setting.key);
                self.menu_cursor = focus_kit::MenuCursor::at(
                    self.open_menu
                        .map_or(0, |key| self.prefs.read(cx).choice(key)),
                );
                focus_kit::focus_menu(&self.menu_focus, window, cx);
            }
            ("record", label) => {
                if let Some(group) = shortcuts::groups().into_iter().find(|g| g.label == label) {
                    self.begin_recording(group.label, cx);
                }
            }
            ("focus", key) => {
                let Some(input) = self.inputs.get(key).cloned() else {
                    return;
                };
                cx.spawn_in(window, async move |_, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(200))
                        .await;
                    let _ = cx
                        .update(|window, cx| input.update(cx, |input, cx| input.focus(window, cx)));
                })
                .detach();
            }
            _ => {}
        }
    }

    fn dismiss_innermost(&mut self) -> bool {
        if self.open_menu.take().is_some() {
            return true;
        }
        if self.actions_draft.take().is_some() || self.server_draft.take().is_some() {
            return true;
        }
        self.shortcut_conflict.take().is_some()
    }

    fn show_section(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        self.section = section;
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    fn step_section(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.modified() {
            return;
        }
        let at = Section::ALL
            .iter()
            .position(|s| *s == self.section)
            .unwrap_or(0);
        let target = match event.keystroke.key.as_str() {
            "up" => at.saturating_sub(1),
            "down" => (at + 1).min(Section::ALL.len() - 1),
            _ => return,
        };
        cx.stop_propagation();
        if target != at {
            self.show_section(Section::ALL[target], window, cx);
        }
    }

    fn reveal_focused_row(&mut self, shown: &[&'static Setting], window: &Window, cx: &App) {
        let focused = shown.iter().map(|s| s.key).find(|key| {
            self.row_focus
                .get(key)
                .is_some_and(|row| row.contains_focused(window, cx))
        });
        if focused == self.focused_row {
            return;
        }
        self.focused_row = focused;
        let Some(row) = focused.and_then(|key| self.row_bounds.borrow().get(key).copied()) else {
            return;
        };
        let view = self.scroll.bounds();
        let (offset, max) = (self.scroll.offset(), self.scroll.max_offset());
        let y = if row.top() < view.top() {
            offset.y + (view.top() - row.top()) + size::S4
        } else if row.bottom() > view.bottom() {
            offset.y - (row.bottom() - view.bottom()) - size::S4
        } else {
            return;
        };
        self.scroll
            .set_offset(gpui::point(offset.x, y.clamp(-max.height, px(0.))));
    }

    fn select_keys(
        &mut self,
        key: &'static str,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.modified()
            || !matches!(event.keystroke.key.as_str(), "down" | "up")
            || self.open_menu == Some(key)
        {
            return;
        }
        cx.stop_propagation();
        self.open_menu = Some(key);
        self.menu_cursor = focus_kit::MenuCursor::at(self.prefs.read(cx).choice(key));
        focus_kit::focus_menu(&self.menu_focus, window, cx);
        cx.notify();
    }

    fn focus_select(&self, key: Option<&'static str>, window: &mut Window) {
        match key.and_then(|key| self.select_focus.get(key)) {
            Some(select) => window.focus(select),
            None => window.focus(&self.focus),
        }
    }

    fn scroll_content(&mut self, to: Scroll, cx: &mut Context<Self>) {
        let (offset, max, height) = (
            self.scroll.offset(),
            self.scroll.max_offset(),
            self.scroll.bounds().size.height,
        );
        let y = match to {
            Scroll::PageUp => offset.y + height * 0.9,
            Scroll::PageDown => offset.y - height * 0.9,
            Scroll::Top => px(0.),
            Scroll::Bottom => -max.height,
        };
        self.scroll
            .set_offset(gpui::point(offset.x, y.clamp(-max.height, px(0.))));
        cx.notify();
    }

    pub fn search_input(&self) -> &Entity<InputState> {
        &self.search
    }

    fn commit(&mut self, key: &'static str, text: &str, cx: &mut Context<Self>) {
        let Some(setting) = schema::setting(key) else {
            return;
        };
        match setting.control {
            Control::Number { min, max, unit, .. } => {
                match text.trim().replace(',', ".").parse::<f64>() {
                    Ok(value) if (min..=max).contains(&value) => {
                        self.errors.remove(key);
                        self.prefs
                            .update(cx, |prefs, cx| prefs.set(key, Value::Number(value), cx));
                    }
                    _ => {
                        let unit = if unit.is_empty() {
                            String::new()
                        } else {
                            format!(" {}", unit_label(unit))
                        };
                        let message = focus_i18n::t_args(
                            "settings-number-refused",
                            &[
                                ("min", format_number(min).as_str().into()),
                                ("max", format_number(max).as_str().into()),
                                ("unit", unit.as_str().into()),
                            ],
                        );
                        self.errors.insert(key, message.into());
                    }
                }
            }
            Control::Text { .. } => match schema::text_refusal(key, text) {
                Some(reason) => {
                    self.errors.insert(key, focus_i18n::t(reason).into());
                }
                None => {
                    self.errors.remove(key);
                    self.prefs.update(cx, |prefs, cx| {
                        prefs.set(key, Value::Text(text.to_string().into()), cx)
                    });
                }
            },
            _ => {}
        }
        cx.notify();
    }

    fn reset(&mut self, setting: &'static Setting, window: &mut Window, cx: &mut Context<Self>) {
        self.prefs.update(cx, |prefs, cx| prefs.reset(setting, cx));
        self.errors.remove(setting.key);
        if let Some(default) = &setting.default {
            self.show_in_field(setting.key, default, window, cx);
        }
        cx.notify();
    }

    fn show_in_field(
        &mut self,
        key: &str,
        value: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.inputs.get(key) else {
            return;
        };
        let text = match value {
            Value::Number(n) => format_number(*n),
            Value::Text(t) => t.to_string(),
            _ => return,
        };
        input.update(cx, |input, cx| input.set_value(text, window, cx));
    }

    fn query(&self, cx: &App) -> String {
        ascii_fold(self.search.read(cx).value().trim())
    }

    fn render_control(
        &self,
        setting: &'static Setting,
        off: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = *cx.tokens();
        let prefs = self.prefs.read(cx);
        let key = setting.key;
        match &setting.control {
            Control::Toggle => {
                let on = prefs.bool(key);
                Checkbox::new(key, on)
                    .disabled(off)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.prefs
                            .update(cx, |prefs, cx| prefs.set(key, Value::Bool(!on), cx));
                    }))
                    .into_any_element()
            }
            Control::Choice(options) if options.len() <= 4 => Segments::new(
                key,
                options
                    .iter()
                    .map(|o| Segment::new(schema::option_label(o)))
                    .collect(),
                prefs.choice(key),
            )
            .disabled(off)
            .on_select({
                let prefs = self.prefs.clone();
                move |index, _, cx| {
                    prefs.update(cx, |prefs, cx| prefs.set(key, Value::Choice(index), cx))
                }
            })
            .into_any_element(),
            Control::Choice(options) => {
                let selected = prefs.choice(key);
                let open = self.open_menu == Some(key) && !off;
                div()
                    .relative()
                    .when(!off, |this| {
                        this.on_key_down(cx.listener(
                            move |this, event: &KeyDownEvent, window, cx| {
                                this.select_keys(key, event, window, cx)
                            },
                        ))
                    })
                    .child(
                        Button::new(key, schema::option_label(options[selected]))
                            .icon(Icon::CHEVRON_DOWN)
                            .disabled(off)
                            .when_some(self.select_focus.get(key), |button, focus| {
                                button.track_focus(focus)
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let just_closed = this
                                    .menu_closed
                                    .is_some_and(|at| at.elapsed() < Duration::from_millis(200));
                                this.open_menu = if this.open_menu == Some(key) || just_closed {
                                    None
                                } else {
                                    Some(key)
                                };
                                if this.open_menu.is_some() {
                                    this.menu_cursor = focus_kit::MenuCursor::default();
                                    focus_kit::focus_menu(&this.menu_focus, window, cx);
                                }
                                cx.notify();
                            })),
                    )
                    .when(open, |this| {
                        this.child(deferred(
                            anchored().anchor(Corner::TopRight).snap_to_window().child(
                                focus_kit::fade_in(
                                    "fade-settings-menu",
                                    focus_kit::keyboard_menu(self, &self.menu_focus.clone(), cx)
                                        .mt(size::S1),
                                    cx,
                                ),
                            ),
                        ))
                    })
                    .into_any_element()
            }
            Control::Number { .. } | Control::Text { .. } if self.inputs.contains_key(key) => {
                let input = &self.inputs[key];
                let unit = match &setting.control {
                    Control::Number { unit, .. } if !unit.is_empty() => Some(unit_label(unit)),
                    _ => None,
                };
                if MULTILINE.contains(&key) {
                    return div()
                        .w(px(320.))
                        .child(TextArea::new(input).disabled(off))
                        .when_some(self.errors.get(key).cloned(), |this, message| {
                            this.child(div().ui_sm().text_color(t.danger).child(message))
                        })
                        .into_any_element();
                }
                div()
                    .w(if matches!(setting.control, Control::Number { .. }) {
                        px(160.)
                    } else {
                        px(280.)
                    })
                    .child(
                        Field::new(input)
                            .disabled(off)
                            .error(self.errors.get(key).cloned())
                            .when_some(unit, |field, unit| {
                                field.suffix(div().ui_sm().text_color(t.ink_3).child(unit))
                            }),
                    )
                    .into_any_element()
            }
            Control::Action if off => Button::new(key, setting.text().action)
                .disabled(true)
                .into_any_element(),
            Control::Custom if off => div().into_any_element(),
            Control::Info if off => div()
                .mono()
                .text_color(t.ink_4)
                .child(self.info(key))
                .into_any_element(),
            Control::Action => self
                .ai_action(key, cx)
                .or_else(|| self.history_action(key, cx))
                .unwrap_or_else(|| {
                    Button::new(
                        key,
                        if self.acted.is_some_and(|(k, _)| k == key) {
                            focus_i18n::t("settings-action-done")
                        } else {
                            setting.text().action
                        },
                    )
                    .when(self.acted.is_some_and(|(k, _)| k == key), |button| {
                        button.icon(Icon::CHECK)
                    })
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.run_action(key, window, cx)),
                    )
                    .into_any_element()
                }),
            Control::Custom if key == "general.recent" => self.render_recent(cx),
            Control::Custom if key == "ai.models" => self.render_models(cx),
            Control::Custom if key == "ai.provider_models" => self.render_provider_models(cx),
            Control::Custom if key == "ai.actions" => self.render_actions(cx),
            Control::Custom if key == "ai.mcp_servers" => self.render_servers(cx),
            Control::Custom | Control::Stored => div().into_any_element(),
            Control::Info => self.ai_info(key, cx).unwrap_or_else(|| {
                div()
                    .mono()
                    .text_color(t.ink_2)
                    .child(self.info(key))
                    .into_any_element()
            }),
            _ => div().into_any_element(),
        }
    }

    fn run_action(&mut self, key: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        match key {
            "general.open" => cx.emit(SettingsEvent::OpenVault),
            "graph.reset" => {
                for setting in SETTINGS.iter().filter(|s| s.key.starts_with("graph.")) {
                    self.reset(setting, window, cx);
                }
            }
            "transfer.settings_export" => self.export_settings(cx),
            "transfer.settings_import" => self.import_settings(window, cx),
            other => cx.emit(SettingsEvent::Action(other)),
        }
        if matches!(key, "graph.reset" | "search.rebuild") {
            self.mark_acted(key, cx);
        }
    }

    fn mark_acted(&mut self, key: &'static str, cx: &mut Context<Self>) {
        let press = self.acted.map_or(0, |(_, n)| n) + 1;
        self.acted = Some((key, press));
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.acted == Some((key, press)) {
                    this.acted = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn info(&self, key: &str) -> SharedString {
        match key {
            "general.vault" => self
                .facts
                .vault
                .clone()
                .unwrap_or_else(|| focus_i18n::t("ui-none"))
                .into(),
            "search.index" => focus_i18n::t_args(
                "settings-search-index-count",
                &[("count", self.facts.notes.into())],
            )
            .into(),
            "files.history_used" => self.history_info().into(),
            "ops.active" => self
                .facts
                .engagement
                .clone()
                .unwrap_or_else(|| focus_i18n::t("ui-none"))
                .into(),
            _ => SharedString::default(),
        }
    }

    fn tracked_row(&self, key: &'static str) -> gpui::Div {
        let bounds = self.row_bounds.clone();
        div()
            .relative()
            .when_some(self.row_focus.get(key), |row, focus| row.track_focus(focus))
            .child(
                canvas(
                    move |area, _, _| {
                        bounds.borrow_mut().insert(key, area);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }

    fn render_row(&self, setting: &'static Setting, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        let changed = !self.prefs.read(cx).is_default(setting);
        let off = setting.section == Section::Ai
            && !AI_WITHOUT_SWITCH.contains(&setting.key)
            && !self.prefs.read(cx).bool("ai.enabled");
        let (label_ink, description_ink) = if off {
            (t.ink_4, t.ink_4)
        } else {
            (t.ink, t.ink_3)
        };
        if matches!(setting.control, Control::Custom) {
            return self
                .tracked_row(setting.key)
                .flex()
                .flex_col()
                .gap(size::S3)
                .py(size::S3)
                .border_b_1()
                .border_color(t.line)
                .child(div().ui().text_color(label_ink).child(setting.text().label))
                .child(
                    div()
                        .ui_sm()
                        .text_color(description_ink)
                        .child(setting.text().description),
                )
                .child(self.render_control(setting, off, cx))
                .into_any_element();
        }
        self.tracked_row(setting.key)
            .flow_row()
            .items_start()
            .gap(size::S6)
            .py(size::S3)
            .border_b_1()
            .border_color(t.line)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .pt(size::S1)
                    .child(div().ui().text_color(label_ink).child(setting.text().label))
                    .child(
                        div()
                            .ui_sm()
                            .text_color(description_ink)
                            .child(setting.text().description),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flow_row()
                    .items_start()
                    .child(self.render_control(setting, off, cx)),
            )
            .child(div().flex_none().w(size::ICON_HIT).child(if changed {
                IconButton::new(
                    SharedString::from(format!("reset-{}", setting.key)),
                    Icon::RESET,
                    focus_i18n::t("settings-reset"),
                )
                .dense(true)
                .disabled(off)
                .on_click(cx.listener(move |this, _, window, cx| this.reset(setting, window, cx)))
                .into_any_element()
            } else {
                div().into_any_element()
            }))
            .into_any_element()
    }

    fn render_about(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let ms = |v: Option<f32>| {
            v.map(|v| {
                focus_i18n::format_duration(std::time::Duration::from_millis(v.round() as u64))
            })
            .unwrap_or_else(|| focus_i18n::t("ui-not-measured"))
        };
        let vault = self.facts.vault.as_deref().map(std::path::Path::new);
        let rows: Vec<(String, String)> = vec![
            (
                focus_i18n::t("settings-about-version"),
                env!("CARGO_PKG_VERSION").to_string(),
            ),
            (
                focus_i18n::t("settings-about-window"),
                ms(self.facts.window_ms),
            ),
            (
                focus_i18n::t("settings-about-first-frame"),
                ms(self.facts.first_frame_ms),
            ),
            (
                focus_i18n::t("settings-about-vault-open"),
                ms(self.facts.vault_open_ms),
            ),
            (
                focus_i18n::t("settings-about-index"),
                ms(self.facts.index_ms),
            ),
            (
                focus_i18n::t("settings-about-memory"),
                self.facts
                    .rss
                    .map(focus_i18n::format_bytes)
                    .unwrap_or_else(|| focus_i18n::t("ui-not-measured")),
            ),
            (
                focus_i18n::t("settings-about-settings"),
                focus_core::settings::config_dir()
                    .join("settings.json")
                    .display()
                    .to_string(),
            ),
            (
                focus_i18n::t("settings-about-cache"),
                vault.map_or_else(
                    || focus_i18n::t("ui-none"),
                    |root| {
                        focus_core::settings::cache_dir_for(root)
                            .display()
                            .to_string()
                    },
                ),
            ),
            (
                focus_i18n::t("settings-about-conversations"),
                vault.map_or_else(
                    || focus_i18n::t("ui-none"),
                    |root| focus_view_ai::history_dir_for(root).display().to_string(),
                ),
            ),
            (
                focus_i18n::t("settings-about-fonts"),
                focus_i18n::t("settings-about-fonts-license"),
            ),
            (
                focus_i18n::t("settings-about-icons"),
                focus_i18n::t("settings-about-icons-license"),
            ),
        ];
        div()
            .flex()
            .flex_col()
            .children(rows.into_iter().map(|(label, value)| {
                div()
                    .flow_row()
                    .items_center()
                    .h(px(36.))
                    .border_b_1()
                    .border_color(t.line)
                    .child(
                        div()
                            .w(px(200.))
                            .flex_none()
                            .ui()
                            .text_color(t.ink_3)
                            .child(focus_kit::truncated(
                                gpui::ElementId::Name(format!("about-{label}").into()),
                                label,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .mono()
                            .text_color(t.ink)
                            .child(focus_kit::ellipsis(value)),
                    )
            }))
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let viewport = window.viewport_size();
        let width = (viewport.width - px(64.)).min(px(1040.));
        let height = (viewport.height - px(64.)).min(px(720.));
        let query = self.query(cx);
        let searching = !query.is_empty() && !matches!(self.section, Section::Shortcuts);
        let section = self.section;

        let matches: Vec<&'static Setting> = SETTINGS
            .iter()
            .filter(|s| s.shown())
            .filter(|s| self.applies_to_provider(s.key, cx))
            .filter(|s| {
                if searching {
                    let text = s.text();
                    ascii_fold(&text.label).contains(&query)
                        || ascii_fold(&text.description).contains(&query)
                } else {
                    s.section == section
                }
            })
            .collect();
        let section_changed = matches.iter().any(|s| !self.prefs.read(cx).is_default(s));
        self.reveal_focused_row(&matches, window, cx);
        let sections_focused = self.sections_focus.is_focused(window);
        focus_kit::keyboard::name_focus(&self.sections_focus, "section-list", cx);

        let content: AnyElement = match section {
            Section::Shortcuts => self.render_shortcuts(&query, cx),
            Section::About if !searching => self.render_about(cx).into_any_element(),
            _ if matches.is_empty() => div()
                .pt(size::S4)
                .ui()
                .text_color(t.ink_3)
                .child(focus_i18n::t("settings-no-match"))
                .into_any_element(),
            _ => {
                let mut column = div().flex().flex_col();
                let mut last_section = None;
                for setting in matches {
                    if searching && last_section != Some(setting.section) {
                        last_section = Some(setting.section);
                        column = column.child(
                            div()
                                .pt(size::S6)
                                .pb(size::S2)
                                .ui_xs()
                                .text_color(t.ink_3)
                                .child(setting.section.label()),
                        );
                    }
                    column = column.child(self.render_row(setting, cx));
                }
                column.into_any_element()
            }
        };

        div()
            .id("settings-panel")
            .key_context("SettingsPanel")
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this, _: &PageUp, _, cx| this.scroll_content(Scroll::PageUp, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PageDown, _, cx| this.scroll_content(Scroll::PageDown, cx)),
            )
            .on_action(
                cx.listener(|this, _: &FirstItem, _, cx| this.scroll_content(Scroll::Top, cx)),
            )
            .on_action(
                cx.listener(|this, _: &LastItem, _, cx| this.scroll_content(Scroll::Bottom, cx)),
            )
            .occlude()
            .flex()
            .flex_col()
            .w(width)
            .h(height)
            .bg(t.card)
            .border_1()
            .border_color(t.float_border)
            .on_action(cx.listener(|this, _: &crate::actions::Dismiss, _, cx| {
                if this.dismiss_innermost() {
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }))
            .child(
                div()
                    .flow_row()
                    .flex_none()
                    .items_center()
                    .gap(size::S3)
                    .h(px(56.))
                    .px(size::S4)
                    .border_b_1()
                    .border_color(t.line)
                    .child(
                        div()
                            .flex_1()
                            .display_sm()
                            .text_color(t.ink)
                            .child(focus_i18n::t("settings-title")),
                    )
                    .child(
                        div()
                            .w(px(280.))
                            .child(Field::new(&self.search).icon(Icon::SEARCH)),
                    )
                    .child(
                        IconButton::new(
                            "settings-close",
                            Icon::CLOSE,
                            focus_i18n::t("ui-action-close"),
                        )
                        .shortcut(focus_i18n::t("ui-key-escape"))
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(SettingsEvent::Close))),
                    ),
            )
            .child(
                div()
                    .flow_row()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .w(px(220.))
                            .py(size::S2)
                            .border_end_1()
                            .border_color(t.line)
                            .track_focus(&self.sections_focus)
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                this.step_section(event, window, cx)
                            }))
                            .children(Section::ALL.into_iter().map(|s| {
                                let current = s == section;
                                div()
                                    .relative()
                                    .child(
                                        TreeRow::new(("section", s as usize), s.label())
                                            .icon(s.icon())
                                            .active(!searching && current)
                                            .focused(sections_focused && current)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.show_section(s, window, cx)
                                            })),
                                    )
                                    .when(sections_focused && current, |this| {
                                        this.child(focus_kit::interaction::inset_ring(t.ink))
                                    })
                            })),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .mb(size::S4)
                            .child(
                                div()
                                    .id("settings-content")
                                    .size_full()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.scroll)
                                    .px(size::S8)
                                    .pt(size::S6)
                                    .pb(size::S8)
                                    .when(!searching, |this| {
                                        this.child(
                                            div()
                                                .flow_row()
                                                .items_center()
                                                .justify_between()
                                                .pb(size::S3)
                                                .border_b_1()
                                                .border_color(t.line_2)
                                                .child(
                                                    div()
                                                        .display_sm()
                                                        .text_color(t.ink)
                                                        .child(section.label()),
                                                )
                                                .when(
                                                    !matches!(
                                                        section,
                                                        Section::Shortcuts | Section::About
                                                    ),
                                                    |this| {
                                                        this.child(
                                                            Button::new(
                                                                "reset-section",
                                                                focus_i18n::t(
                                                                    "settings-reset-section",
                                                                ),
                                                            )
                                                            .kind(ButtonKind::Ghost)
                                                            .icon(Icon::RESET)
                                                            .disabled(!section_changed)
                                                            .on_click(cx.listener(
                                                                move |this, _, window, cx| {
                                                                    for setting in SETTINGS
                                                                        .iter()
                                                                        .filter(|s| {
                                                                            s.section == section
                                                                                && s.shown()
                                                                        })
                                                                    {
                                                                        this.reset(
                                                                            setting, window, cx,
                                                                        );
                                                                    }
                                                                },
                                                            )),
                                                        )
                                                    },
                                                ),
                                        )
                                    })
                                    .when_some(self.notice.clone(), |this, (failed, message)| {
                                        this.child(
                                            div()
                                                .pt(size::S3)
                                                .ui()
                                                .text_color(if failed { t.danger } else { t.ink_2 })
                                                .child(message),
                                        )
                                    })
                                    .child(content),
                            )
                            .child(focus_kit::scrollbar(
                                "settings-scrollbar",
                                self.scroll.clone(),
                            )),
                    ),
            )
    }
}

impl focus_kit::KeyboardMenu for SettingsView {
    fn menu_name(&self) -> &'static str {
        "choice-menu"
    }

    fn menu_items(&self, cx: &gpui::App) -> Vec<focus_kit::MenuItem> {
        let Some(key) = self.open_menu else {
            return Vec::new();
        };
        let Some(Control::Choice(options)) = schema::setting(key).map(|s| &s.control) else {
            return Vec::new();
        };
        let selected = self.prefs.read(cx).choice(key);
        options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                focus_kit::MenuItem::from(
                    focus_kit::MenuEntry::new(
                        format!("choice-{index}"),
                        schema::option_label(option),
                    )
                    .checked(index == selected),
                )
            })
            .collect()
    }

    fn menu_cursor(&self) -> focus_kit::MenuCursor {
        self.menu_cursor
    }

    fn set_menu_cursor(&mut self, cursor: focus_kit::MenuCursor) {
        self.menu_cursor = cursor;
    }

    fn menu_run(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.open_menu.take();
        if let Some(key) = key {
            self.prefs
                .update(cx, |prefs, cx| prefs.set(key, Value::Choice(row), cx));
        }
        self.focus_select(key, window);
        cx.notify();
    }

    fn menu_dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_closed = Some(Instant::now());
        let key = self.open_menu.take();
        self.focus_select(key, window);
        cx.notify();
    }
}
