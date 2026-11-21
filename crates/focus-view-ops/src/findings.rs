use std::path::PathBuf;

use focus_i18n::{Arg, t_args};
use focus_kit::{
    ActiveTokens, Button, ButtonKind, ContextChip, Field, Flow as _, Icon, IconButton, MenuRow,
    Segment, Segments, Tokens, Type, menu, size,
};
use focus_ops::cvss::{self, Metric};
use focus_ops::{Finding, FindingStatus, Severity, findings};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle,
    InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
    StatefulInteractiveElement, Styled, Subscription, Window, anchored, deferred, div,
    prelude::FluentBuilder, px,
};
use gpui_component::input::{InputEvent, InputState};

use crate::event::{NoticeTone, OpsEvent};
use crate::state::{OpsState, error_text};
use crate::ui::{
    finding_status_key, hatched_empty, mono_chip, screen_header, section_label, severity_key,
    severity_tag, t_,
};

const SEVERITIES: [Severity; 5] = [
    Severity::Critical,
    Severity::High,
    Severity::Medium,
    Severity::Low,
    Severity::Info,
];
const STATUSES: [FindingStatus; 5] = [
    FindingStatus::Draft,
    FindingStatus::Confirmed,
    FindingStatus::Reported,
    FindingStatus::Fixed,
    FindingStatus::Accepted,
];
const VERSIONS: [&str; 2] = ["3.1", "4.0"];

pub(crate) fn vector_of(version: &str, picks: &[usize]) -> String {
    let metrics = cvss::base_metrics(version);
    let parts: Vec<String> = metrics
        .iter()
        .zip(picks)
        .map(|(m, p)| format!("{}:{}", m.code, m.values.get(*p).map_or("", |v| v.0)))
        .collect();
    format!("CVSS:{version}/{}", parts.join("/"))
}

pub(crate) fn picks_of(vector: &str) -> Option<(&'static str, Vec<usize>)> {
    let version = VERSIONS
        .into_iter()
        .find(|v| vector.starts_with(&format!("CVSS:{v}/")))?;
    let metrics = cvss::base_metrics(version);
    let picks = metrics
        .iter()
        .map(|m| {
            vector
                .split('/')
                .find_map(|p| p.strip_prefix(&format!("{}:", m.code)))
                .and_then(|v| m.values.iter().position(|x| x.0 == v))
        })
        .collect::<Option<Vec<_>>>()?;
    Some((version, picks))
}

pub(crate) fn add_unique(list: &mut Vec<String>, entries: &str) -> usize {
    let before = list.len();
    for entry in entries.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        if !list.iter().any(|known| known.eq_ignore_ascii_case(entry)) {
            list.push(entry.to_string());
        }
    }
    list.len() - before
}

pub(crate) fn technique(entry: &str) -> Option<String> {
    let entry = entry.trim().to_ascii_uppercase();
    let (main, sub) = entry
        .split_once('.')
        .map_or((entry.as_str(), None), |(m, s)| (m, Some(s)));
    let digits = |text: &str, n: usize| text.len() == n && text.chars().all(|c| c.is_ascii_digit());
    (main.starts_with('T') && digits(&main[1..], 4) && sub.is_none_or(|s| digits(s, 3)))
        .then_some(entry)
}

fn metric_label(m: &Metric) -> String {
    t_(&format!("ops-cvss-metric-{}", m.code.to_lowercase()))
}

fn value_label(m: &Metric, value: &str) -> String {
    t_(&format!(
        "ops-cvss-value-{}-{}",
        m.code.to_lowercase(),
        value.to_lowercase()
    ))
}

pub struct FindingsList {
    state: Entity<OpsState>,
    severity: Option<Severity>,
    status: Option<FindingStatus>,
    selected: Option<PathBuf>,
    creating: bool,
    title: Entity<InputState>,
    generation: u64,
    new_severity: Severity,
    version: &'static str,
    picks: Vec<usize>,
    compact: crate::rich::Compact,
    sheet: Entity<focus_view_reading::ReadingView>,
    tab: Tab,
    by_creation: bool,
    editing: bool,
    open_filter: Option<u8>,
    menus: crate::ui::MenuFocus,
    focus: FocusHandle,
    rows_scroll: ScrollHandle,
    copied: crate::ui::CopyConfirm,
    asset_input: Entity<InputState>,
    technique_input: Entity<InputState>,
    cwe_input: Entity<InputState>,
    cwe_filter: Entity<InputState>,
    body_input: Entity<InputState>,
    body_read: Option<focus_ops::findings::Body>,
    pending_body: Option<String>,
    body_original: String,
    preview_generation: u64,
    saving: bool,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Sheet,
    Calculator,
}

impl EventEmitter<OpsEvent> for FindingsList {}

impl FindingsList {
    pub fn new(
        state: Entity<OpsState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> FindingsList {
        let title = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-finding-title-placeholder"))
        });
        let observe = cx.observe(&state, |_, _, cx| cx.notify());
        let enter = cx.subscribe_in(
            &title,
            window,
            |this: &mut FindingsList, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.create(cx);
                }
            },
        );
        let (sheet, sheet_events) = crate::rich::rendering_view(360., cx);
        let asset_input = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-finding-asset-placeholder"))
        });
        let technique_input = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-finding-technique-placeholder"))
        });
        let add_asset = cx.subscribe_in(
            &asset_input,
            window,
            |this: &mut FindingsList, input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().to_string();
                    input.update(cx, |input, cx| input.set_value("", window, cx));
                    this.add_assets(&text, cx);
                }
            },
        );
        let add_technique = cx.subscribe_in(
            &technique_input,
            window,
            |this: &mut FindingsList, input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().to_string();
                    input.update(cx, |input, cx| input.set_value("", window, cx));
                    this.add_techniques(&text, cx);
                }
            },
        );
        let cwe_input = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-finding-cwe-placeholder"))
        });
        let cwe_filter = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-finding-cwe-filter"))
        });
        let add_cwe = cx.subscribe_in(
            &cwe_input,
            window,
            |this: &mut FindingsList, input, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).value().to_string();
                    input.update(cx, |input, cx| input.set_value("", window, cx));
                    this.add_cwe(&text, cx);
                }
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        );
        let filter_cwe = cx.subscribe_in(&cwe_filter, window, |_, _, event: &InputEvent, _, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        });
        let body_input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(8, 18)
                .placeholder(t_("ops-finding-body-placeholder"))
        });
        let body_changed = cx.subscribe_in(
            &body_input,
            window,
            |this: &mut FindingsList, input, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    this.schedule_preview(input.read(cx).value().to_string(), cx);
                    cx.notify();
                }
            },
        );
        let list = FindingsList {
            generation: focus_i18n::generation(),
            state,
            severity: None,
            status: None,
            selected: None,
            creating: false,
            title,
            new_severity: Severity::Medium,
            version: "3.1",
            picks: vec![0; cvss::base_metrics("3.1").len()],
            compact: crate::rich::Compact::new(),
            sheet,
            tab: Tab::Calculator,
            editing: false,
            by_creation: false,
            open_filter: None,
            menus: crate::ui::MenuFocus::new(cx),
            focus: crate::ui::list_focus(cx),
            rows_scroll: ScrollHandle::new(),
            copied: Default::default(),
            asset_input,
            technique_input,
            cwe_input,
            cwe_filter,
            body_input,
            body_read: None,
            pending_body: None,
            body_original: String::new(),
            preview_generation: 0,
            saving: false,
            _subscriptions: vec![
                observe,
                enter,
                sheet_events,
                add_asset,
                add_technique,
                add_cwe,
                filter_cwe,
                body_changed,
            ],
        };
        if let Some(n) = std::env::var("FOCUS_OPS_FINDING_EDIT")
            .ok()
            .and_then(|n| n.parse::<usize>().ok())
        {
            let query = std::env::var("FOCUS_OPS_CWE_QUERY").unwrap_or_default();
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(900))
                    .await;
                this.update_in(cx, |this, window, cx| {
                    this.select_nth(n, cx);
                    this.tab = Tab::Sheet;
                    this.editing = true;
                    this.cwe_input
                        .update(cx, |input, cx| input.set_value(query, window, cx));
                })
                .ok();
            })
            .detach();
        }
        list
    }

    pub fn load_vector(&mut self, vector: &str, cx: &mut Context<Self>) {
        if let Some((version, picks)) = picks_of(vector) {
            self.version = version;
            self.picks = picks;
            cx.notify();
        }
    }

    pub fn start_creating(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.creating = true;
        self.title.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub fn select_nth(&mut self, n: usize, cx: &mut Context<Self>) {
        let Some(active) = self.state.read(cx).active.clone() else {
            return;
        };
        let rows = self.visible(&active.findings, cx);
        if let Some(finding) = rows.get(n) {
            let (path, vector) = (finding.path.clone(), finding.cvss_vector.clone());
            if let Some(vector) = vector {
                self.load_vector(&vector, cx);
            }
            self.select(path, cx);
        }
    }

    fn jump(&mut self, jump: crate::ui::Jump, _: &mut Window, cx: &mut Context<Self>) {
        let Some(active) = self.state.read(cx).active.clone() else {
            return;
        };
        let rows = self.visible(&active.findings, cx);
        let current = self
            .selected
            .as_ref()
            .and_then(|path| rows.iter().position(|f| &f.path == path));
        let page = crate::ui::rows_per_page(&self.rows_scroll, 56.);
        if let Some(n) = crate::ui::landing(current, rows.len(), page, jump) {
            self.select_nth(n, cx);
            self.rows_scroll.scroll_to_item(n);
        }
    }

    pub fn show_calculator(&mut self, cx: &mut Context<Self>) {
        self.tab = Tab::Calculator;
        cx.notify();
    }

    fn select(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.selected = Some(path.clone());
        self.tab = Tab::Sheet;
        self.load_body(path, cx);
        cx.notify();
    }

    fn load_body(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let sheet = self.sheet.clone();
        let wanted = path.clone();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move { findings::read_body(&path) })
                .await;
            this.update(cx, |this, cx| {
                if this.selected.as_ref() != Some(&wanted) {
                    return;
                }
                match read {
                    Ok(body) => {
                        crate::rich::set_text(&sheet, body.text.clone(), cx);
                        this.body_original.clone_from(&body.text);
                        this.pending_body = Some(body.text.clone());
                        this.body_read = Some(body);
                    }
                    Err(error) => {
                        this.body_read = None;
                        cx.emit(OpsEvent::Notify {
                            tone: NoticeTone::Danger,
                            text: error_text(&error),
                        });
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn schedule_preview(&mut self, text: String, cx: &mut Context<Self>) {
        self.preview_generation += 1;
        let generation = self.preview_generation;
        let sheet = self.sheet.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(250))
                .await;
            this.update(cx, |this, cx| {
                if this.preview_generation == generation {
                    crate::rich::set_text(&sheet, text, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn body_is_dirty(&self, cx: &App) -> bool {
        self.body_read.is_some() && self.body_input.read(cx).value().as_ref() != self.body_original
    }

    fn save_body(&mut self, cx: &mut Context<Self>) {
        let (Some(path), Some(read)) = (
            self.selected.clone(),
            self.body_read.as_ref().map(|b| b.modified),
        ) else {
            return;
        };
        let text = self.body_input.read(cx).value().to_string();
        self.saving = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let target = path.clone();
            let written = text.clone();
            let saved = cx
                .background_spawn(async move { findings::save_body(&target, &written, read) })
                .await;
            this.update(cx, |this, cx| {
                this.saving = false;
                match saved {
                    Ok(modified) => {
                        this.body_original = text.clone();
                        this.body_read = Some(focus_ops::findings::Body { text, modified });
                        this.state.update(cx, |state, cx| state.reload(cx));
                    }
                    Err(error) => cx.emit(OpsEvent::Notify {
                        tone: NoticeTone::Danger,
                        text: error_text(&error),
                    }),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn reload_body(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.selected.clone() {
            self.load_body(path, cx);
        }
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        let title = self.title.read(cx).value().trim().to_string();
        if title.is_empty() {
            return;
        }
        let severity = self.new_severity;
        self.creating = false;
        let Some(dir) = self
            .state
            .read(cx)
            .active
            .as_ref()
            .map(|a| a.engagement.dir.clone())
        else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let made = cx
                .background_spawn(async move { findings::create(&dir, &title, severity) })
                .await;
            this.update(cx, |this, cx| match made {
                Ok(finding) => {
                    this.selected = Some(finding.path);
                    this.state.update(cx, |state, cx| state.reload(cx));
                    cx.notify();
                }
                Err(error) => cx.emit(OpsEvent::Notify {
                    tone: NoticeTone::Danger,
                    text: error_text(&error),
                }),
            })
            .ok();
        })
        .detach();
    }

    pub fn set_editing(&mut self, editing: bool, cx: &mut Context<Self>) {
        self.editing = editing;
        cx.notify();
    }

    fn edit_selected(
        &mut self,
        change: impl FnOnce(&mut Finding) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        let Some(mut finding) = self
            .state
            .read(cx)
            .active
            .as_ref()
            .and_then(|a| a.findings.iter().find(|f| f.path == path).cloned())
        else {
            return;
        };
        change(&mut finding);
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(async move { findings::save(&finding) })
                .await;
            this.update(cx, |this, cx| match saved {
                Ok(()) => this.state.update(cx, |state, cx| state.reload(cx)),
                Err(error) => cx.emit(OpsEvent::Notify {
                    tone: NoticeTone::Danger,
                    text: error_text(&error),
                }),
            })
            .ok();
        })
        .detach();
    }

    fn set_status(&mut self, status: FindingStatus, cx: &mut Context<Self>) {
        self.edit_selected(move |finding| finding.status = status, cx);
    }

    fn add_assets(&mut self, text: &str, cx: &mut Context<Self>) {
        let text = text.to_string();
        self.edit_selected(
            move |finding| {
                add_unique(&mut finding.assets, &text);
            },
            cx,
        );
    }

    fn remove_asset(&mut self, asset: String, cx: &mut Context<Self>) {
        self.edit_selected(move |finding| finding.assets.retain(|a| *a != asset), cx);
    }

    fn add_techniques(&mut self, text: &str, cx: &mut Context<Self>) {
        let (valid, invalid): (Vec<_>, Vec<_>) = text
            .split(',')
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .map(|e| (technique(e), e))
            .partition(|(t, _)| t.is_some());
        if !invalid.is_empty() {
            cx.emit(OpsEvent::Notify {
                tone: NoticeTone::Danger,
                text: t_args(
                    "ops-finding-technique-invalid",
                    &[("value", Arg::from(invalid[0].1))],
                ),
            });
        }
        let valid: Vec<String> = valid.into_iter().filter_map(|(t, _)| t).collect();
        if !valid.is_empty() {
            self.edit_selected(
                move |finding| {
                    add_unique(&mut finding.attack, &valid.join(","));
                },
                cx,
            );
        }
    }

    fn add_cwe(&mut self, text: &str, cx: &mut Context<Self>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let id = focus_ops::cwe::normalize(text).or_else(|| {
            focus_ops::cwe::search(text, 1)
                .first()
                .map(|w| focus_ops::cwe::format_id(w.id))
        });
        match id {
            Some(id) => self.edit_selected(
                move |finding| {
                    if !finding.cwe.contains(&id) {
                        finding.cwe.push(id);
                    }
                },
                cx,
            ),
            None => cx.emit(OpsEvent::Notify {
                tone: NoticeTone::Danger,
                text: t_args("ops-finding-cwe-invalid", &[("value", Arg::from(text))]),
            }),
        }
    }

    fn remove_cwe(&mut self, id: String, cx: &mut Context<Self>) {
        self.edit_selected(move |finding| finding.cwe.retain(|c| *c != id), cx);
    }

    fn visible(&self, findings: &[Finding], cx: &App) -> Vec<Finding> {
        let rows = if self.by_creation {
            newest_first(sorted(findings, self.severity, self.status))
        } else {
            sorted(findings, self.severity, self.status)
        };
        let wanted = self.cwe_filter.read(cx).value().trim().to_string();
        if wanted.is_empty() {
            return rows;
        }
        rows.into_iter()
            .filter(|f| matches_cwe(f, &wanted))
            .collect()
    }

    fn remove_technique(&mut self, technique: String, cx: &mut Context<Self>) {
        self.edit_selected(
            move |finding| finding.attack.retain(|a| *a != technique),
            cx,
        );
    }

    fn apply_score(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        let Some(mut finding) = self
            .state
            .read(cx)
            .active
            .as_ref()
            .and_then(|a| a.findings.iter().find(|f| f.path == path).cloned())
        else {
            return;
        };
        let Ok(score) = cvss::score(&vector_of(self.version, &self.picks)) else {
            return;
        };
        finding.cvss_vector = Some(score.vector);
        finding.cvss_score = Some(score.base);
        finding.severity = score.severity;
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(async move { findings::save(&finding) })
                .await;
            this.update(cx, |this, cx| match saved {
                Ok(()) => this.state.update(cx, |state, cx| state.reload(cx)),
                Err(error) => cx.emit(OpsEvent::Notify {
                    tone: NoticeTone::Danger,
                    text: error_text(&error),
                }),
            })
            .ok();
        })
        .detach();
    }

    fn right_panel(
        &mut self,
        t: &Tokens,
        active: &crate::state::Active,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let entity = cx.entity();
        let tabs = Segments::new(
            "ops-finding-tabs",
            vec![
                Segment::new(t_("ops-finding-sheet")),
                Segment::new(t_("ops-cvss-calculator")),
            ],
            if self.tab == Tab::Sheet { 0 } else { 1 },
        )
        .on_select(move |i, _, cx| {
            entity.update(cx, |this, cx| {
                this.tab = if i == 0 { Tab::Sheet } else { Tab::Calculator };
                cx.notify();
            })
        });
        let body: AnyElement = match (
            self.tab,
            self.selected
                .as_ref()
                .and_then(|p| active.findings.iter().find(|f| &f.path == p)),
        ) {
            (Tab::Sheet, Some(finding)) => self.sheet_content(t, finding, &active.hosts, cx),
            (Tab::Sheet, None) => {
                hatched_empty(t, t_("ops-finding-sheet-empty"), None).into_any_element()
            }
            (Tab::Calculator, _) => self.calculator(t, cx).into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .w(px(360.))
            .min_w(px(280.))
            .flex_shrink()
            .h_full()
            .gap(size::S3)
            .p(size::S4)
            .border_start_1()
            .border_color(t.line)
            .child(tabs)
            .child(body)
    }

    fn sheet_content(
        &mut self,
        t: &Tokens,
        finding: &Finding,
        hosts: &[focus_ops::Host],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = finding.path.clone();
        let editing = self.editing;
        let header = crate::ui::row()
            .items_center()
            .justify_between()
            .child(severity_tag(t, finding.severity))
            .child(
                Button::new(
                    "ops-finding-edit",
                    t_(if editing {
                        "ops-finding-edit-done"
                    } else {
                        "ops-finding-edit"
                    }),
                )
                .kind(if editing {
                    ButtonKind::Primary
                } else {
                    ButtonKind::Secondary
                })
                .icon(if editing { Icon::CHECK } else { Icon::PENCIL })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.editing = !this.editing;
                    cx.notify();
                })),
            );
        let form: AnyElement = if editing {
            let status = Segments::new(
                "ops-finding-status",
                STATUSES
                    .iter()
                    .map(|s| Segment::new(t_(finding_status_key(*s))))
                    .collect(),
                STATUSES
                    .iter()
                    .position(|s| *s == finding.status)
                    .unwrap_or(0),
            )
            .on_select({
                let entity = cx.entity();
                move |i, _, cx| entity.update(cx, |this, cx| this.set_status(STATUSES[i], cx))
            });
            let chips = |kind: &'static str,
                         values: &[String],
                         remove_label: String,
                         cx: &mut Context<Self>|
             -> AnyElement {
                crate::ui::row()
                    .flex_wrap()
                    .gap(size::S1)
                    .children(values.iter().enumerate().map(|(i, value)| {
                        let removed = value.clone();
                        ContextChip::new((kind, i), value.clone(), remove_label.clone()).on_remove(
                            cx.listener(move |this, _, _, cx| {
                                if kind == "ops-finding-asset" {
                                    this.remove_asset(removed.clone(), cx);
                                } else if kind == "ops-finding-cwe" {
                                    this.remove_cwe(removed.clone(), cx);
                                } else {
                                    this.remove_technique(removed.clone(), cx);
                                }
                            }),
                        )
                    }))
                    .into_any_element()
            };
            div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .child(section_label(t, t_("ops-finding-status")))
                .child(status)
                .child(section_label(t, t_("ops-finding-assets")))
                .child(chips(
                    "ops-finding-asset",
                    &finding.assets,
                    t_("ops-finding-asset-remove"),
                    cx,
                ))
                .child(Field::new(&self.asset_input))
                .child(section_label(t, t_("ops-finding-techniques")))
                .child(chips(
                    "ops-finding-technique",
                    &finding.attack,
                    t_("ops-finding-technique-remove"),
                    cx,
                ))
                .child(Field::new(&self.technique_input))
                .child(section_label(t, t_("ops-finding-cwe")))
                .child(chips(
                    "ops-finding-cwe",
                    &finding.cwe,
                    t_("ops-finding-cwe-remove"),
                    cx,
                ))
                .child(Field::new(&self.cwe_input))
                .children(self.cwe_suggestions(t, finding, cx))
                .child(section_label(t, t_("ops-finding-body")))
                .child(focus_kit::TextArea::new(&self.body_input))
                .child(
                    crate::ui::row()
                        .gap(size::S2)
                        .child(
                            Button::new("ops-finding-body-save", t_("ops-finding-body-save"))
                                .kind(ButtonKind::Primary)
                                .icon(Icon::CHECK)
                                .disabled(self.saving || !self.body_is_dirty(cx))
                                .on_click(cx.listener(|this, _, _, cx| this.save_body(cx))),
                        )
                        .child(
                            Button::new("ops-finding-body-reload", t_("ops-finding-body-reload"))
                                .disabled(self.saving || !self.body_is_dirty(cx))
                                .on_click(cx.listener(|this, _, _, cx| this.reload_body(cx))),
                        ),
                )
                .into_any_element()
        } else {
            let assets = self.compact.element(
                format!("sheet-assets-{}", finding.id),
                &finding.assets.join(", "),
                hosts,
                cx,
            );
            div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .child(
                    crate::ui::row()
                        .flex_wrap()
                        .items_center()
                        .gap(size::S2)
                        .child(
                            div()
                                .ui_sm()
                                .text_color(t.ink_2)
                                .child(t_(finding_status_key(finding.status))),
                        )
                        .children(finding.cvss_score.map(|s| mono_chip(t, format!("{s:.1}"))))
                        .children(finding.cwe.iter().map(|c| mono_chip(t, c.clone())))
                        .children(finding.attack.iter().map(|a| mono_chip(t, a.clone()))),
                )
                .child(section_label(t, t_("ops-finding-assets")))
                .child(div().min_h(px(20.)).child(assets))
                .into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(size::S3)
            .child(header)
            .child(
                div()
                    .display_sm()
                    .text_color(t.ink)
                    .child(finding.title.clone()),
            )
            .child(if editing {
                div()
                    .id("ops-finding-edit")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap(size::S3)
                    .child(form)
                    .child(section_label(t, t_("ops-finding-preview")))
                    .child(div().min_h(px(200.)).child(self.sheet.clone()))
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .gap(size::S3)
                    .child(form)
                    .child(
                        div()
                            .id("ops-finding-body")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(self.sheet.clone()),
                    )
                    .into_any_element()
            })
            .child(
                Button::new("ops-finding-open-note", t_("ops-open-note")).on_click(cx.listener(
                    move |_, _, _, cx| {
                        cx.emit(OpsEvent::OpenNote {
                            path: open.clone(),
                            line: None,
                        })
                    },
                )),
            )
            .into_any_element()
    }

    fn cwe_suggestions(
        &self,
        t: &Tokens,
        finding: &Finding,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let query = self.cwe_input.read(cx).value().to_string();
        if query.trim().is_empty() {
            return None;
        }
        let hits = focus_ops::cwe::search(&query, 6);
        if hits.is_empty() {
            return Some(
                div()
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(t_("ops-finding-cwe-none"))
                    .into_any_element(),
            );
        }
        Some(focus_kit::fade_in(
            "fade-ops-cwe-menu",
            menu(t).children(hits.into_iter().map(|weakness| {
                let id = focus_ops::cwe::format_id(weakness.id);
                let added = finding.cwe.contains(&id);
                MenuRow::new(
                    ("ops-finding-cwe-hit", weakness.id as usize),
                    format!("{id} {}", weakness.name),
                )
                .checked(added)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.cwe_input
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    this.add_cwe(&id, cx);
                }))
            })),
            cx,
        ))
    }

    fn calculator(&self, t: &Tokens, cx: &mut Context<Self>) -> gpui::Div {
        let vector = vector_of(self.version, &self.picks);
        let scored = cvss::score(&vector);
        let metrics = cvss::base_metrics(self.version);
        let entity = cx.entity();
        let version_entity = entity.clone();
        let headline: AnyElement = match &scored {
            Ok(score) => crate::ui::row()
                .items_center()
                .gap(size::S3)
                .child(
                    mono_chip(t, focus_i18n::format_decimal(f64::from(score.base), 1))
                        .font_weight(gpui::FontWeight::SEMIBOLD),
                )
                .child(severity_tag(t, score.severity))
                .into_any_element(),
            Err(_) => div()
                .ui()
                .text_color(t.danger)
                .child(t_("ops-cvss-invalid-vector"))
                .into_any_element(),
        };
        let copy_vector = vector.clone();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .gap(size::S3)
            .child(section_label(t, t_("ops-cvss-calculator")))
            .child(
                Segments::new(
                    "ops-cvss-version",
                    VERSIONS
                        .iter()
                        .map(|v| {
                            Segment::new(t_args("ops-cvss-version", &[("version", Arg::from(*v))]))
                        })
                        .collect(),
                    VERSIONS
                        .iter()
                        .position(|v| *v == self.version)
                        .unwrap_or(0),
                )
                .on_select(move |i, _, cx| {
                    version_entity.update(cx, |this, cx| {
                        this.version = VERSIONS[i];
                        this.picks = vec![0; cvss::base_metrics(this.version).len()];
                        cx.notify();
                    })
                }),
            )
            .child(headline)
            .child(
                crate::ui::row()
                    .items_start()
                    .gap(size::S2)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .mono_sm()
                            .text_left()
                            .text_color(t.ink_2)
                            .child(vector),
                    )
                    .child({
                        let (face, label) =
                            self.copied
                                .face("vector", Icon::COPY, "ops-finding-vector-copy");
                        let entity = entity.clone();
                        IconButton::new("ops-cvss-copy", face, label)
                            .dense(true)
                            .on_click(move |_, _, cx| {
                                focus_kit::clipboard::copy_text(cx, copy_vector.clone());
                                entity.update(cx, |this, cx| {
                                    this.copied.mark(
                                        "vector".into(),
                                        |view: &mut FindingsList| &mut view.copied,
                                        cx,
                                    )
                                });
                            })
                    }),
            )
            .child(
                div()
                    .id("ops-cvss-metrics")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap(size::S3)
                    .children(metrics.iter().enumerate().map(|(index, metric)| {
                        let entity = entity.clone();
                        div()
                            .flex()
                            .flex_col()
                            .gap(size::S1)
                            .child(
                                div()
                                    .ui_xs()
                                    .text_color(t.ink_3)
                                    .child(metric_label(metric)),
                            )
                            .child(
                                Segments::new(
                                    ("ops-cvss-metric", index),
                                    metric
                                        .values
                                        .iter()
                                        .map(|(code, _)| Segment::new(value_label(metric, code)))
                                        .collect(),
                                    self.picks.get(index).copied().unwrap_or(0),
                                )
                                .on_select(move |value, _, cx| {
                                    entity.update(cx, |this, cx| {
                                        if let Some(slot) = this.picks.get_mut(index) {
                                            *slot = value;
                                        }
                                        cx.notify();
                                    })
                                }),
                            )
                    })),
            )
            .child(
                Button::new("ops-cvss-apply", t_("ops-cvss-apply"))
                    .kind(ButtonKind::Primary)
                    .disabled(self.selected.is_none() || scored.is_err())
                    .on_click(cx.listener(|this, _, _, cx| this.apply_score(cx))),
            )
    }
}

pub(crate) fn newest_first(mut rows: Vec<Finding>) -> Vec<Finding> {
    rows.sort_by(|a, b| {
        id_number(&b.id)
            .cmp(&id_number(&a.id))
            .then_with(|| b.id.cmp(&a.id))
    });
    rows
}

fn id_number(id: &str) -> Option<u32> {
    id.strip_prefix("F-")?.parse().ok()
}

pub(crate) fn matches_cwe(finding: &Finding, wanted: &str) -> bool {
    match focus_ops::cwe::parse_id(wanted) {
        Some(id) => finding
            .cwe
            .iter()
            .any(|c| focus_ops::cwe::parse_id(c) == Some(id)),
        None => {
            let words: Vec<String> = wanted.split_whitespace().map(str::to_lowercase).collect();
            finding.cwe.iter().any(|c| {
                let name = focus_ops::cwe::parse_id(c)
                    .and_then(focus_ops::cwe::name)
                    .unwrap_or_default()
                    .to_lowercase();
                words
                    .iter()
                    .all(|w| name.contains(w) || c.to_lowercase().contains(w))
            })
        }
    }
}

pub(crate) fn sorted(
    findings: &[Finding],
    severity: Option<Severity>,
    status: Option<FindingStatus>,
) -> Vec<Finding> {
    let mut rows: Vec<Finding> = findings
        .iter()
        .filter(|f| {
            severity.is_none_or(|s| f.severity == s) && status.is_none_or(|s| f.status == s)
        })
        .cloned()
        .collect();
    rows.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| {
                b.cvss_score
                    .partial_cmp(&a.cvss_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.id.cmp(&b.id))
    });
    rows
}

impl Render for FindingsList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.menus.sync(self.open_filter.is_some(), window, cx);
        if let Some(text) = self.pending_body.take() {
            self.body_input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
        let t = *cx.tokens();
        if self.generation != focus_i18n::generation() {
            self.generation = focus_i18n::generation();
            self.title.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-finding-title-placeholder"), window, cx)
            });
            for (input, key) in [
                (&self.cwe_input, "ops-finding-cwe-placeholder"),
                (&self.cwe_filter, "ops-finding-cwe-filter"),
                (&self.body_input, "ops-finding-body-placeholder"),
                (&self.asset_input, "ops-finding-asset-placeholder"),
                (&self.technique_input, "ops-finding-technique-placeholder"),
            ] {
                input.update(cx, |input, cx| input.set_placeholder(t_(key), window, cx));
            }
        }

        let Some(active) = self.state.read(cx).active.clone() else {
            return div()
                .id("ops-findings")
                .size_full()
                .bg(t.bg)
                .flex()
                .child(hatched_empty(&t, t_("ops-no-engagement"), None));
        };
        let rows = self.visible(&active.findings, cx);
        let entity = cx.entity();
        let sev_entity = entity.clone();
        let status_entity = entity.clone();
        let new_entity = entity.clone();

        let mut sev_items = vec![Segment::new(t_("ops-filter-all"))];
        sev_items.extend(
            SEVERITIES
                .iter()
                .map(|s| Segment::new(t_(severity_key(*s)))),
        );
        let mut status_items = vec![Segment::new(t_("ops-filter-all"))];
        status_items.extend(
            STATUSES
                .iter()
                .map(|s| Segment::new(t_(finding_status_key(*s)))),
        );

        let narrow = window.viewport_size().width < px(1400.);
        let severity_value = self
            .severity
            .map_or_else(|| t_("ops-filter-all"), |s| t_(severity_key(s)));
        let status_value = self
            .status
            .map_or_else(|| t_("ops-filter-all"), |s| t_(finding_status_key(s)));
        let mut toolbar = crate::ui::row()
            .max_w_full()
            .flex_wrap()
            .items_center()
            .gap(size::S3);
        toolbar = if narrow {
            let open = self.open_filter;
            let menu_button =
                |id: &'static str,
                 which: u8,
                 label: String,
                 rows: Vec<(String, bool)>,
                 cx: &mut Context<Self>| {
                    div()
                        .relative()
                        .child(Button::new(id, label).icon(Icon::CHEVRON_DOWN).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.open_filter =
                                    if this.menus.click_opens(this.open_filter == Some(which)) {
                                        Some(which)
                                    } else {
                                        None
                                    };
                                cx.notify();
                            }),
                        ))
                        .when(open == Some(which), |this| {
                            let entity = cx.entity();
                            let dismiss = move |_: &mut Window, cx: &mut gpui::App| {
                                entity.update(cx, |this, cx| {
                                    this.open_filter = None;
                                    this.menus.dismissed();
                                    cx.notify();
                                })
                            };
                            this.child(deferred(anchored().snap_to_window().child(
                                focus_kit::fade_in(
                                    "fade-ops-findings-menu",
                                    self.menus.attach(menu(&t).mt(size::S1), dismiss).children(
                                        rows.into_iter().enumerate().map(
                                            |(index, (text, checked))| {
                                                MenuRow::new((id, index), text)
                                                    .checked(checked)
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.open_filter = None;
                                                        match which {
                                                            0 => {
                                                                this.severity = index
                                                                    .checked_sub(1)
                                                                    .map(|i| SEVERITIES[i])
                                                            }
                                                            _ => {
                                                                this.status = index
                                                                    .checked_sub(1)
                                                                    .map(|i| STATUSES[i])
                                                            }
                                                        }
                                                        cx.notify();
                                                    }))
                                            },
                                        ),
                                    ),
                                    cx,
                                ),
                            )))
                        })
                };
            let mut severity_rows = vec![(t_("ops-filter-all"), self.severity.is_none())];
            severity_rows.extend(
                SEVERITIES
                    .iter()
                    .map(|s| (t_(severity_key(*s)), self.severity == Some(*s))),
            );
            let mut status_rows = vec![(t_("ops-filter-all"), self.status.is_none())];
            status_rows.extend(
                STATUSES
                    .iter()
                    .map(|s| (t_(finding_status_key(*s)), self.status == Some(*s))),
            );
            toolbar
                .child(menu_button(
                    "ops-filter-severity-menu",
                    0,
                    t_args(
                        "ops-filter-severity",
                        &[("value", Arg::from(severity_value.as_str()))],
                    ),
                    severity_rows,
                    cx,
                ))
                .child(menu_button(
                    "ops-filter-status-menu",
                    1,
                    t_args(
                        "ops-filter-status",
                        &[("value", Arg::from(status_value.as_str()))],
                    ),
                    status_rows,
                    cx,
                ))
        } else {
            toolbar
                .child(
                    Segments::new(
                        "ops-filter-severity",
                        sev_items,
                        self.severity.map_or(0, |s| {
                            SEVERITIES.iter().position(|x| *x == s).map_or(0, |i| i + 1)
                        }),
                    )
                    .on_select(move |i, _, cx| {
                        sev_entity.update(cx, |this, cx| {
                            this.severity = i.checked_sub(1).map(|i| SEVERITIES[i]);
                            cx.notify();
                        })
                    }),
                )
                .child(
                    Segments::new(
                        "ops-filter-status",
                        status_items,
                        self.status.map_or(0, |s| {
                            STATUSES.iter().position(|x| *x == s).map_or(0, |i| i + 1)
                        }),
                    )
                    .on_select(move |i, _, cx| {
                        status_entity.update(cx, |this, cx| {
                            this.status = i.checked_sub(1).map(|i| STATUSES[i]);
                            cx.notify();
                        })
                    }),
                )
        };
        let toolbar = toolbar
            .child(div().w(px(150.)).child(Field::new(&self.cwe_filter)))
            .child(div().flex_1())
            .child(
                Button::new(
                    "ops-finding-sort",
                    t_(if self.by_creation {
                        "ops-sort-creation"
                    } else {
                        "ops-sort-severity"
                    }),
                )
                .kind(ButtonKind::Ghost)
                .icon(Icon::ARROW_UP_DOWN)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.by_creation = !this.by_creation;
                    cx.notify();
                })),
            )
            .child(
                Button::new("ops-finding-new", t_("ops-finding-new"))
                    .icon(Icon::PLUS)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.creating = !this.creating;
                        if this.creating {
                            this.title.update(cx, |input, cx| input.focus(window, cx));
                        }
                        cx.notify();
                    })),
            );

        let creation = self.creating.then(|| {
            crate::ui::row()
                .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        this.creating = false;
                        cx.notify();
                    }
                }))
                .flex_wrap()
                .items_center()
                .gap(size::S3)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(160.))
                        .child(Field::new(&self.title)),
                )
                .child(
                    Segments::new(
                        "ops-finding-new-severity",
                        SEVERITIES
                            .iter()
                            .map(|s| Segment::new(t_(severity_key(*s))))
                            .collect(),
                        SEVERITIES
                            .iter()
                            .position(|s| *s == self.new_severity)
                            .unwrap_or(0),
                    )
                    .on_select(move |i, _, cx| {
                        new_entity.update(cx, |this, cx| {
                            this.new_severity = SEVERITIES[i];
                            cx.notify();
                        })
                    }),
                )
                .child(
                    Button::new("ops-finding-create", t_("ops-finding-create"))
                        .kind(ButtonKind::Primary)
                        .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
                )
        });

        let has_rows = !rows.is_empty();
        let list: AnyElement = if rows.is_empty() {
            hatched_empty(&t, t_("ops-findings-empty"), None).into_any_element()
        } else {
            div()
                .id("ops-finding-rows")
                .flex()
                .flex_col()
                .flex_shrink()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.rows_scroll)
                .children(rows.into_iter().enumerate().map(|(i, f)| {
                    let selected = self.selected.as_ref() == Some(&f.path);
                    let path = f.path.clone();
                    let open_path = f.path.clone();
                    let vector = f.cvss_vector.clone();
                    div()
                        .id(("ops-finding-row", i))
                        .flex()
                        .flex_col()
                        .justify_center()
                        .gap(px(2.))
                        .h(px(if narrow { 76. } else { 56. }))
                        .px(size::S3)
                        .border_b_1()
                        .border_color(t.line_2)
                        .cursor_pointer()
                        .when(selected, |this| this.bg(t.card_2))
                        .hover(move |s| s.bg(t.card_2))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(vector) = &vector {
                                this.load_vector(vector, cx);
                            }
                            this.select(path.clone(), cx);
                        }))
                        .child(
                            crate::ui::row()
                                .items_center()
                                .gap(size::S3)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .ui()
                                        .text_color(t.ink)
                                        .child(focus_kit::ellipsis(f.title.clone())),
                                )
                                .child(crate::ui::row().flex_none().w(px(36.)).children(
                                    f.cvss_score.map(|s| mono_chip(&t, format!("{s:.1}"))),
                                ))
                                .child(
                                    IconButton::new(
                                        ("ops-finding-open", i),
                                        Icon::FILE_TEXT,
                                        t_("ops-open-note"),
                                    )
                                    .dense(true)
                                    .on_click(cx.listener(
                                        move |_, _, _, cx| {
                                            cx.emit(OpsEvent::OpenNote {
                                                path: open_path.clone(),
                                                line: None,
                                            })
                                        },
                                    )),
                                ),
                        )
                        .child(
                            crate::ui::row()
                                .items_center()
                                .gap(size::S3)
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(76.))
                                        .child(severity_tag(&t, f.severity)),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(44.))
                                        .mono_sm()
                                        .text_color(t.ink_3)
                                        .child(f.id.clone()),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .ui_sm()
                                        .text_color(t.ink_2)
                                        .child(t_(finding_status_key(f.status))),
                                )
                                .when(!narrow, |this| {
                                    this.children(
                                        f.cwe.iter().take(3).map(|c| mono_chip(&t, c.clone())),
                                    )
                                })
                                .when(!narrow, |this| {
                                    this.child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .ui_sm()
                                            .text_color(t.ink_3)
                                            .child(focus_kit::ellipsis(f.assets.join(", "))),
                                    )
                                }),
                        )
                        .when(narrow, |this| {
                            this.child(
                                div().min_w_0().ui_sm().text_color(t.ink_3).child(
                                    focus_kit::ellipsis(
                                        f.cwe
                                            .iter()
                                            .chain(&f.assets)
                                            .cloned()
                                            .collect::<Vec<_>>()
                                            .join(", "),
                                    ),
                                ),
                            )
                        })
                }))
                .into_any_element()
        };

        crate::ui::pageable(
            crate::ui::row().id("ops-findings"),
            &self.focus,
            cx,
            Self::jump,
        )
        .size_full()
        .bg(t.bg)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(270.))
                .gap(size::S4)
                .p(size::S4)
                .child(screen_header(
                    &t,
                    t_("ops-section-findings"),
                    Some(t_args(
                        "ops-finding-count",
                        &[("count", Arg::from(active.findings.len()))],
                    )),
                ))
                .child(toolbar)
                .children(creation)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .child(list)
                        .children(has_rows.then(crate::ui::table_filler)),
                ),
        )
        .child(self.right_panel(&t, &active, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(id: &str, severity: Severity, score: Option<f32>, status: FindingStatus) -> Finding {
        Finding {
            path: PathBuf::from(id),
            id: id.into(),
            title: id.into(),
            severity,
            cvss_vector: None,
            cvss_score: score,
            status,
            assets: vec![],
            cwe: vec![],
            attack: vec![],
        }
    }

    #[test]
    fn findings_sort_by_severity_then_score_then_id() {
        let all = vec![
            finding("2", Severity::High, Some(7.5), FindingStatus::Draft),
            finding("1", Severity::Critical, None, FindingStatus::Draft),
            finding("3", Severity::High, Some(8.1), FindingStatus::Draft),
        ];
        let ids: Vec<_> = sorted(&all, None, None).into_iter().map(|f| f.id).collect();
        assert_eq!(ids, ["1", "3", "2"]);
    }

    #[test]
    fn the_cwe_filter_takes_an_identifier_or_words_of_a_name() {
        let mut f = finding("1", Severity::High, None, FindingStatus::Draft);
        f.cwe = vec!["CWE-79".into(), "CWE-89".into()];
        assert!(
            matches_cwe(&f, "79")
                && matches_cwe(&f, "cwe-89")
                && matches_cwe(&f, "sql command")
                && matches_cwe(&f, "web page")
        );
        assert!(
            !matches_cwe(&f, "7") && !matches_cwe(&f, "CWE-22") && !matches_cwe(&f, "pathname")
        );
    }

    #[test]
    fn entries_are_added_once_whatever_their_case() {
        let mut assets = vec!["web01".to_string()];
        assert_eq!(add_unique(&mut assets, "WEB01, db01 , ,10.0.0.5, db01"), 2);
        assert_eq!(assets, ["web01", "db01", "10.0.0.5"]);
    }

    #[test]
    fn techniques_are_upper_case_identifiers() {
        assert_eq!(technique(" t1059.001 ").as_deref(), Some("T1059.001"));
        assert_eq!(technique("T1059").as_deref(), Some("T1059"));
        for refused in ["1059", "T105", "T1059.1", "T1059.0011", "Kerberoasting", ""] {
            assert_eq!(technique(refused), None, "{refused}");
        }
    }

    #[test]
    fn the_newest_finding_comes_first_whatever_its_severity() {
        let all = vec![
            finding("F-002", Severity::Critical, None, FindingStatus::Draft),
            finding("F-010", Severity::Low, None, FindingStatus::Draft),
            finding("F-003", Severity::High, None, FindingStatus::Draft),
        ];
        let ids: Vec<_> = newest_first(all).into_iter().map(|f| f.id).collect();
        assert_eq!(ids, ["F-010", "F-003", "F-002"]);
    }

    #[test]
    fn filters_combine() {
        let all = vec![
            finding("1", Severity::High, None, FindingStatus::Draft),
            finding("2", Severity::High, None, FindingStatus::Fixed),
            finding("3", Severity::Low, None, FindingStatus::Fixed),
        ];
        assert_eq!(
            sorted(&all, Some(Severity::High), Some(FindingStatus::Fixed)).len(),
            1
        );
        assert_eq!(sorted(&all, None, Some(FindingStatus::Fixed)).len(), 2);
    }

    #[test]
    fn default_picks_give_the_known_vectors_and_scores() {
        let v31 = vector_of("3.1", &[0; 8]);
        assert_eq!(v31, "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H");
        assert_eq!(cvss::score(&v31).unwrap().base, 9.8);
        let v40 = vector_of("4.0", &[0; 11]);
        assert!(cvss::score(&v40).is_ok(), "{v40}");
    }

    #[test]
    fn a_vector_round_trips_through_the_picks() {
        let vector = "CVSS:3.1/AV:L/AC:H/PR:L/UI:R/S:C/C:L/I:N/A:H";
        let (version, picks) = picks_of(vector).unwrap();
        assert_eq!(vector_of(version, &picks), vector);
        assert!(picks_of("garbage").is_none());
    }
}
