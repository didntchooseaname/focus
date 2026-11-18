use std::collections::HashMap;

use focus_i18n::{Arg, t_args};
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Field, Icon, IconButton, MenuRow, Tokens, TreeRow, Type, dot,
    icon, menu, size,
};
use focus_ops::{exam, vars};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window, div,
    prelude::FluentBuilder, px,
};
use gpui_component::input::{InputEvent, InputState};

use crate::dashboard::{kind_key, status_key};
use crate::event::{NoticeTone, OpsEvent, OpsPage, ShowPage};
use crate::state::{Active, OpsState, error_text};
use crate::time::{clock_text, now_unix};
use crate::ui::{hatched_empty, section_label, t_};

const STATUSES: [focus_ops::EngagementStatus; 4] = [
    focus_ops::EngagementStatus::Planning,
    focus_ops::EngagementStatus::InProgress,
    focus_ops::EngagementStatus::Reporting,
    focus_ops::EngagementStatus::Done,
];
const EYE: Icon = Icon("\u{E0BA}");
const EYE_OFF: Icon = Icon("\u{E0BB}");

const BASE_VARIABLES: [&str; 6] = ["RHOST", "LHOST", "LPORT", "DOMAIN", "USER", "PASS"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Open {
    None,
    Engagements,
    Targets,
    Status,
    Credentials,
    Trash,
}

pub(crate) struct ExamView {
    pub profile: &'static exam::Profile,
    pub clock: Option<exam::Clock>,
    pub points: u32,
}

pub(crate) fn exam_view(active: &Active, now: u64) -> Option<ExamView> {
    let profile = exam::profiles()
        .iter()
        .find(|p| Some(p.id) == active.engagement.exam.as_deref())?;
    let timer = focus_ops::timer::Timer::of(&active.engagement);
    let clock = timer
        .started
        .map(|started| exam::clock(profile, started, timer.clock_now(now)));
    let points = if profile.machines.is_empty() {
        active.overview.points
    } else {
        exam::score(profile, &active.engagement.exam_done)
    };
    Some(ExamView {
        profile,
        clock,
        points,
    })
}

pub(crate) fn variable_names(active: &Active) -> Vec<String> {
    names_of(&active.context.values, &active.engagement.vars)
}

fn names_of(
    context: &std::collections::BTreeMap<String, String>,
    stored: &std::collections::BTreeMap<String, String>,
) -> Vec<String> {
    let mut names: Vec<String> = BASE_VARIABLES.iter().map(|s| s.to_string()).collect();
    let mut extra: Vec<String> = context
        .keys()
        .chain(stored.keys())
        .map(|k| focus_ops::vars::canonical_name(k))
        .filter(|k| !names.contains(k))
        .collect();
    extra.sort();
    extra.dedup();
    names.extend(extra);
    names
}

pub struct OpsSidebar {
    state: Entity<OpsState>,
    open: Open,
    menus: crate::ui::MenuFocus,
    rules_open: bool,
    choice_for: Option<String>,
    revealed: std::collections::HashSet<String>,
    masked: std::collections::HashMap<String, bool>,
    generation: u64,
    variables: HashMap<String, Entity<InputState>>,
    entry: Entity<InputState>,
    new_name: Entity<InputState>,
    new_value: Entity<InputState>,
    scope: Entity<InputState>,
    dirty_sync: bool,
    _subscriptions: Vec<Subscription>,
    ticker: crate::ui::Ticker,
}

impl EventEmitter<OpsEvent> for OpsSidebar {}
impl EventEmitter<crate::event::ShowPage> for OpsSidebar {}

impl OpsSidebar {
    pub fn new(state: Entity<OpsState>, window: &mut Window, cx: &mut Context<Self>) -> OpsSidebar {
        let entry = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-journal-placeholder"))
        });
        let new_name = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-var-name-placeholder"))
        });
        let new_value = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-var-value-placeholder"))
        });
        let scope = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-scope-placeholder"))
        });
        let save_scope = cx.subscribe_in(
            &scope,
            window,
            |this: &mut OpsSidebar, input, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let scope = crate::dialog::split_scope(input.read(cx).value().as_ref());
                    this.edit_engagement(move |engagement| engagement.scope = scope, cx);
                }
            },
        );
        let observe = cx.observe(&state, |this, state, cx| {
            this.dirty_sync = true;
            let running = crate::ui::Ticker::running(state.read(cx));
            this.ticker.sync(running, cx);
            cx.notify();
        });
        let enter = cx.subscribe_in(
            &entry,
            window,
            |this: &mut OpsSidebar, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.add_entry(window, cx);
                }
            },
        );
        OpsSidebar {
            state,
            open: Open::None,
            menus: crate::ui::MenuFocus::new(cx),
            rules_open: false,
            choice_for: None,
            revealed: Default::default(),
            masked: Default::default(),
            generation: focus_i18n::generation(),
            variables: HashMap::new(),
            entry,
            new_name,
            new_value,
            scope,
            dirty_sync: true,
            _subscriptions: vec![observe, enter, save_scope],
            ticker: Default::default(),
        }
    }

    pub fn set_variable(&mut self, name: &str, value: &str, cx: &mut Context<Self>) {
        self.save_variable(name.to_uppercase(), value.to_string(), cx);
    }

    pub fn add_variable(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.new_name.read(cx).value().trim().to_uppercase();
        let value = self.new_value.read(cx).value().trim().to_string();
        let valid = !name.is_empty()
            && name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            cx.emit(OpsEvent::Notify {
                tone: NoticeTone::Warn,
                text: t_("ops-var-invalid-name"),
            });
            return;
        }
        if value.is_empty() {
            return;
        }
        for input in [&self.new_name, &self.new_value] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.save_variable(name, value, cx);
    }

    pub fn request_evidence(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.state.read(cx).active.as_ref() else {
            return;
        };
        let target: String = active.focus_host().map_or("no-target".to_string(), |h| {
            h.ip.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect()
        });
        cx.emit(OpsEvent::AttachEvidence {
            directory: active.engagement.dir.join("Evidence"),
            name_prefix: format!("{}-{target}", crate::time::local_stamp(now_unix())),
        });
    }

    pub fn focus_entry(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.entry.update(cx, |input, cx| input.focus(window, cx));
    }

    fn add_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.entry.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        let target = self
            .state
            .read(cx)
            .active
            .as_ref()
            .and_then(|a| a.focus_host().map(|h| h.ip.clone()));
        self.entry
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.state.update(cx, |state, cx| {
            state.write(
                cx,
                move |dir| focus_ops::journal::append(dir, target.as_deref(), &text),
                |_, _, _| {},
            );
        });
    }

    fn variable_input(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(input) = self.variables.get(name) {
            return input.clone();
        }
        let input = cx.new(|cx| InputState::new(window, cx).validate(crate::ui::single_line));
        let key = name.to_string();
        let sub = cx.subscribe_in(
            &input,
            window,
            move |this: &mut OpsSidebar, input, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    let value = input.read(cx).value().trim().to_string();
                    this.save_variable(key.clone(), value, cx);
                }
            },
        );
        self._subscriptions.push(sub);
        self.variables.insert(name.to_string(), input.clone());
        input
    }

    fn save_variable(&mut self, name: String, value: String, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.set_variable(&name, &value, cx));
    }

    fn link_variable(
        &mut self,
        name: String,
        credential: Option<focus_ops::Credential>,
        cx: &mut Context<Self>,
    ) {
        self.open = Open::None;
        self.choice_for = None;
        self.dirty_sync = true;
        self.state.update(cx, |state, cx| {
            state.link_variable(&name, credential.as_ref(), cx)
        });
        cx.notify();
    }

    pub fn script_link(&mut self, name: &str, user: &str, cx: &mut Context<Self>) -> bool {
        let Some(active) = self.state.read(cx).active.as_ref() else {
            return false;
        };
        let found = focus_ops::links::choices(&active.credentials, name)
            .into_iter()
            .rev()
            .find(|c| c.credential.user == user);
        match found {
            Some(choice) => {
                self.link_variable(
                    focus_ops::vars::canonical_name(name),
                    Some(choice.credential),
                    cx,
                );
                true
            }
            None => false,
        }
    }

    fn dismiss_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&mut Window, &mut gpui::App) + Clone + 'static {
        let entity = cx.entity();
        move |_, cx| {
            entity.update(cx, |this, cx| {
                this.open = Open::None;
                this.choice_for = None;
                this.menus.dismissed();
                cx.notify();
            })
        }
    }

    fn choose_target(&mut self, stem: Option<String>, cx: &mut Context<Self>) {
        self.open = Open::None;
        let Some(mut engagement) = self
            .state
            .read(cx)
            .active
            .as_ref()
            .map(|a| a.engagement.clone())
        else {
            return;
        };
        engagement.focus = stem;
        crate::state::drop_typed_rhost(&mut engagement);
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(async move { focus_ops::save(&engagement) })
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
        cx.notify();
    }

    fn edit_engagement(
        &mut self,
        change: impl FnOnce(&mut focus_ops::Engagement) + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, cx| {
            state.write(
                cx,
                move |dir| {
                    let mut engagement = focus_ops::load(dir)?;
                    change(&mut engagement);
                    focus_ops::save(&engagement)
                },
                |_, (), _| {},
            )
        });
    }

    fn move_timer(
        &mut self,
        change: fn(&mut focus_ops::Engagement, u64) -> bool,
        cx: &mut Context<Self>,
    ) {
        let now = now_unix();
        self.state.update(cx, |state, cx| {
            state.write(
                cx,
                move |dir| {
                    let mut engagement = focus_ops::load(dir)?;
                    if change(&mut engagement, now) {
                        focus_ops::save(&engagement)
                    } else {
                        Ok(())
                    }
                },
                |_, (), _| {},
            )
        });
    }

    fn timer_controls(
        &self,
        t: &Tokens,
        active: &Active,
        with_start: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        use focus_ops::timer::{self, State};
        let timer_state = timer::Timer::of(&active.engagement);
        let row = crate::ui::row().gap(size::S2).flex_wrap();
        match timer_state.state() {
            State::NotStarted if !with_start => row,
            State::NotStarted => row.child(
                Button::new("ops-timer-start", t_("ops-timer-start"))
                    .kind(ButtonKind::Primary)
                    .icon(Icon::CLOCK)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.move_timer(
                            |e, now| {
                                timer::start(e, now);
                                e.status = focus_ops::EngagementStatus::InProgress;
                                true
                            },
                            cx,
                        )
                    })),
            ),
            State::Running => row
                .child(
                    Button::new("ops-timer-pause", t_("ops-timer-pause"))
                        .on_click(cx.listener(|this, _, _, cx| this.move_timer(timer::pause, cx))),
                )
                .child(
                    Button::new("ops-timer-stop", t_("ops-timer-stop"))
                        .icon(Icon::SQUARE)
                        .on_click(cx.listener(|this, _, _, cx| this.move_timer(timer::stop, cx))),
                ),
            State::Paused => row
                .child(
                    Button::new("ops-timer-resume", t_("ops-timer-resume"))
                        .kind(ButtonKind::Primary)
                        .on_click(cx.listener(|this, _, _, cx| this.move_timer(timer::resume, cx))),
                )
                .child(
                    Button::new("ops-timer-stop", t_("ops-timer-stop"))
                        .icon(Icon::SQUARE)
                        .on_click(cx.listener(|this, _, _, cx| this.move_timer(timer::stop, cx))),
                ),
            State::Stopped => row.child(div().ui_sm().text_color(t.ink_3).child(t_args(
                "ops-timer-stopped",
                &[(
                    "duration",
                    Arg::from(clock_text(timer_state.worked(now_unix())).as_str()),
                )],
            ))),
        }
    }

    pub fn start_exam(&mut self, cx: &mut Context<Self>) {
        let started = crate::time::local_iso(now_unix());
        self.state.update(cx, |state, cx| {
            state.write(
                cx,
                move |dir| {
                    let mut engagement = focus_ops::load(dir)?;
                    engagement.started = Some(started);
                    engagement.status = focus_ops::EngagementStatus::InProgress;
                    focus_ops::save(&engagement)
                },
                |_, (), _| {},
            )
        });
    }

    pub fn toggle_exam_item_at(&mut self, n: usize, cx: &mut Context<Self>) {
        let Some(profile) = self.state.read(cx).active.as_ref().and_then(|a| {
            exam::profiles()
                .iter()
                .find(|p| Some(p.id) == a.engagement.exam.as_deref())
        }) else {
            return;
        };
        let keys: Vec<String> = profile
            .machines
            .iter()
            .flat_map(|m| {
                if m.milestones.is_empty() {
                    vec![exam::item_key(m, None)]
                } else {
                    m.milestones
                        .iter()
                        .map(|(k, _)| exam::item_key(m, Some(k)))
                        .collect()
                }
            })
            .collect();
        if let Some(key) = keys.get(n) {
            self.toggle_exam_item(key.clone(), cx);
        }
    }

    fn toggle_exam_item(&mut self, key: String, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            state.write(
                cx,
                move |dir| {
                    let mut engagement = focus_ops::load(dir)?;
                    match engagement.exam_done.iter().position(|k| *k == key) {
                        Some(position) => {
                            engagement.exam_done.remove(position);
                        }
                        None => engagement.exam_done.push(key),
                    }
                    focus_ops::save(&engagement)
                },
                |_, (), _| {},
            )
        });
    }

    fn exam_section(
        &self,
        t: &Tokens,
        active: &Active,
        view: &ExamView,
        done: &[String],
        alert_minutes: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let profile = view.profile;
        let points = match (profile.pass_points, profile.total_points) {
            (Some(pass), Some(total)) => t_args(
                "ops-exam-points-of",
                &[
                    ("points", Arg::from(view.points)),
                    ("pass", Arg::from(pass)),
                    ("total", Arg::from(total)),
                ],
            ),
            (Some(pass), None) => t_args(
                "ops-exam-points-pass",
                &[
                    ("points", Arg::from(view.points)),
                    ("pass", Arg::from(pass)),
                ],
            ),
            _ => t_args("ops-exam-points", &[("points", Arg::from(view.points))]),
        };
        let mut section = div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .child(section_label(t, t_("ops-section-exam")))
            .child(
                crate::ui::start(div().ui().text_color(t.ink))
                    .child(format!("{} ({})", profile.name, profile.vendor)),
            );
        match &view.clock {
            None => {
                section = section
                    .child(
                        crate::ui::start(div().ui_sm().text_color(t.ink_3)).child(t_args(
                            "ops-exam-not-started",
                            &[("minutes", Arg::from(profile.exam_minutes))],
                        )),
                    )
                    .child(
                        Button::new("ops-exam-start", t_("ops-exam-start"))
                            .kind(ButtonKind::Primary)
                            .icon(Icon::CLOCK)
                            .on_click(cx.listener(|this, _, _, cx| this.start_exam(cx))),
                    );
            }
            Some(clock) => {
                let alert = !clock.expired && clock.remaining_secs < alert_minutes * 60;
                let (label, tone) = if clock.expired {
                    (t_("ops-exam-time-over"), t.danger)
                } else {
                    (
                        t_args(
                            "ops-exam-time-left",
                            &[(
                                "duration",
                                Arg::from(clock_text(clock.remaining_secs).as_str()),
                            )],
                        ),
                        if alert { t.danger } else { t.ink },
                    )
                };
                section = section
                    .child(
                        div()
                            .stat()
                            .font_family(focus_kit::typography::MONO)
                            .text_color(tone)
                            .child(clock_text(if clock.expired {
                                0
                            } else {
                                clock.remaining_secs
                            })),
                    )
                    .child(crate::ui::start(div().ui_sm().text_color(tone)).child(label))
                    .child(
                        crate::ui::start(div().ui_sm().text_color(t.ink_3)).child(t_(
                            if clock.report_phase {
                                "ops-exam-phase-report"
                            } else {
                                "ops-exam-phase-practical"
                            },
                        )),
                    )
                    .when(alert, |this| {
                        this.child(crate::ui::start(div().ui_sm().text_color(t.danger)).child(
                            t_args("ops-exam-alert", &[("minutes", Arg::from(alert_minutes))]),
                        ))
                    });
            }
        }
        section = section.child(self.timer_controls(t, active, false, cx));
        section = section.child(crate::ui::start(div().mono().text_color(t.ink)).child(points));
        if !profile.machines.is_empty() {
            section = section.child(
                div().flex().flex_col().gap(size::S1).children(
                    profile
                        .machines
                        .iter()
                        .enumerate()
                        .map(|(machine_index, machine)| {
                            let items: Vec<(String, String, u32)> = if machine.milestones.is_empty()
                            {
                                vec![(
                                    exam::item_key(machine, None),
                                    t_(machine.label_key),
                                    machine.points,
                                )]
                            } else {
                                machine
                                    .milestones
                                    .iter()
                                    .map(|(key, points)| {
                                        (exam::item_key(machine, Some(key)), t_(key), *points)
                                    })
                                    .collect()
                            };
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .pt(size::S1)
                                .when(!machine.milestones.is_empty(), |this| {
                                    this.child(
                                        crate::ui::start(div().ui_sm().text_color(t.ink_2))
                                            .child(t_(machine.label_key)),
                                    )
                                })
                                .children(items.into_iter().enumerate().map(
                                    |(item_index, (key, label, points))| {
                                        let reached = done.contains(&key);
                                        crate::ui::row()
                                            .items_center()
                                            .gap(size::S2)
                                            .h(px(24.))
                                            .child(
                                                focus_kit::Checkbox::new(
                                                    (
                                                        "ops-exam-item",
                                                        machine_index * 8 + item_index,
                                                    ),
                                                    reached,
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.toggle_exam_item(key.clone(), cx)
                                                })),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .ui_sm()
                                                    .text_color(if reached {
                                                        t.ink
                                                    } else {
                                                        t.ink_2
                                                    })
                                                    .child(label),
                                            )
                                            .child(
                                                div()
                                                    .mono_sm()
                                                    .text_color(t.ink_3)
                                                    .child(points.to_string()),
                                            )
                                    },
                                ))
                        }),
                ),
            );
        }
        section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(size::S1)
                    .child(
                        crate::ui::row()
                            .id("ops-exam-rules-toggle")
                            .items_center()
                            .gap(size::S1)
                            .h(px(22.))
                            .cursor_pointer()
                            .text_color(t.ink_3)
                            .hover({
                                let ink = t.ink_2;
                                move |s| s.text_color(ink)
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.rules_open = !this.rules_open;
                                cx.notify();
                            }))
                            .child(icon(
                                if self.rules_open {
                                    Icon::CHEVRON_DOWN
                                } else {
                                    crate::dir::Dir::current().forward()
                                },
                                size::ICON_SMALL,
                            ))
                            .child(div().ui_xs().child(t_args(
                                "ops-exam-rules-count",
                                &[("count", Arg::from(profile.rule_keys.len()))],
                            ))),
                    )
                    .when(self.rules_open, |this| {
                        this.children(profile.rule_keys.iter().map(|key| {
                            crate::ui::start(div().ui_sm().text_color(t.ink_2)).child(t_(key))
                        }))
                    }),
            )
            .into_any_element()
    }
}

impl Render for OpsSidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        self.menus.sync(self.open != Open::None, window, cx);
        if self.generation != focus_i18n::generation() {
            self.generation = focus_i18n::generation();
            self.entry.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-journal-placeholder"), window, cx)
            });
            self.new_name.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-var-name-placeholder"), window, cx)
            });
            self.new_value.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-var-value-placeholder"), window, cx)
            });
        }

        let state = self.state.read(cx);
        let Some(active) = state.active.clone() else {
            let engagements = state.engagements.len();
            let _ = engagements;
            return div()
                .id("ops-sidebar")
                .flex()
                .flex_col()
                .size_full()
                .bg(t.bg)
                .p(size::S3)
                .gap(size::S3)
                .child(hatched_empty(
                    &t,
                    t_("ops-no-engagement"),
                    Some(
                        Button::new("ops-sidebar-new", t_("ops-new-engagement"))
                            .kind(ButtonKind::Primary)
                            .icon(Icon::PLUS)
                            .on_click(
                                cx.listener(|_, _, _, cx| {
                                    cx.emit(OpsEvent::NewEngagementRequested)
                                }),
                            )
                            .into_any_element(),
                    ),
                ));
        };
        let engagements = state.engagements.clone();
        let names = variable_names(&active);
        let inputs: Vec<(String, Entity<InputState>)> = names
            .iter()
            .map(|n| (n.clone(), self.variable_input(n, window, cx)))
            .collect();
        if self.dirty_sync {
            self.dirty_sync = false;
            let scope_text = active.engagement.scope.join(", ");
            if self.scope.read(cx).value() != scope_text {
                self.scope
                    .update(cx, |input, cx| input.set_value(scope_text, window, cx));
            }
            for (name, input) in &inputs {
                let value = active.context.values.get(name).cloned().unwrap_or_default();
                if input.read(cx).value() != value {
                    input.update(cx, |input, cx| input.set_value(value, window, cx));
                }
            }
        }
        for (name, input) in &inputs {
            let canonical = focus_ops::vars::canonical_name(name);
            let hidden = active.engagement.var_links.contains_key(&canonical)
                && focus_ops::links::is_secret(&canonical)
                && !self.revealed.contains(name);
            if self.masked.insert(name.clone(), hidden) != Some(hidden) {
                input.update(cx, |input, cx| input.set_masked(hidden, window, cx));
            }
        }
        let trash = self.state.read(cx).trash.clone();
        let exam = exam_view(&active, now_unix());
        let alert_minutes = u64::from(self.state.read(cx).settings.alert_minutes);

        let status_color = match active.engagement.status {
            focus_ops::EngagementStatus::InProgress => t.ok,
            focus_ops::EngagementStatus::Done => t.ink_3,
            _ => t.warn,
        };
        let engagement = div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .child(section_label(&t, t_("ops-section-overview")))
            .child(
                div()
                    .id("ops-sidebar-engagement")
                    .flex()
                    .items_center()
                    .gap(size::S2)
                    .h(size::CONTROL)
                    .px(size::S2)
                    .rounded(size::RADIUS)
                    .border_1()
                    .border_color(t.line_2)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(t.ink_3))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = if this.menus.click_opens(this.open == Open::Engagements) {
                            Open::Engagements
                        } else {
                            Open::None
                        };
                        cx.notify();
                    }))
                    .child(dot(status_color))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .ui_strong()
                            .text_color(t.ink)
                            .child(focus_kit::ellipsis(active.engagement.name.clone())),
                    )
                    .child(icon(Icon::CHEVRON_DOWN, size::ICON_SMALL).text_color(t.ink_3)),
            )
            .children(trash.as_ref().map(|trash| {
                let name = active.engagement.name.clone();
                let key = if trash.system {
                    "ops-trash-confirm-system"
                } else {
                    "ops-trash-confirm-vault"
                };
                div()
                    .flex()
                    .flex_col()
                    .gap(size::S1)
                    .child(
                        Button::new("ops-sidebar-trash", t_("ops-trash"))
                            .icon(Icon::TRASH)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open = if this.menus.click_opens(this.open == Open::Trash) {
                                    Open::Trash
                                } else {
                                    Open::None
                                };
                                cx.notify();
                            })),
                    )
                    .when(self.open == Open::Trash, |this| {
                        this.child(
                            self.menus
                                .attach(
                                    menu(&t).min_w_0().p(size::S3).gap(size::S2),
                                    self.dismiss_menu(cx),
                                )
                                .child(div().ui_strong().text_color(t.ink).child(t_args(
                                    "ops-trash-title",
                                    &[("name", Arg::from(name.as_str()))],
                                )))
                                .child(
                                    div()
                                        .ui_sm()
                                        .text_color(t.ink_2)
                                        .child(t_args(key, &[("name", Arg::from(name.as_str()))])),
                                )
                                .child(
                                    crate::ui::row()
                                        .gap(size::S2)
                                        .child(
                                            Button::new("ops-trash-cancel", t_("ops-cancel"))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.open = Open::None;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            Button::new("ops-trash-confirm", t_("ops-trash-do"))
                                                .kind(ButtonKind::Primary)
                                                .icon(Icon::TRASH)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.open = Open::None;
                                                    this.state.update(cx, |state, cx| {
                                                        state.trash_active(cx)
                                                    });
                                                    cx.notify();
                                                })),
                                        ),
                                ),
                        )
                    })
            }))
            .child(
                crate::ui::start(div().ui_sm().text_color(t.ink_3))
                    .child(t_(kind_key(active.engagement.kind))),
            )
            .child(crate::ui::dropdown(
                &t,
                "ops-engagement-status",
                t_(status_key(active.engagement.status)),
                STATUSES
                    .iter()
                    .map(|s| (t_(status_key(*s)), *s == active.engagement.status))
                    .collect(),
                self.open == Open::Status,
                &self.menus,
                {
                    let entity = cx.entity();
                    move |_, cx| {
                        entity.update(cx, |this, cx| {
                            this.open = if this.menus.click_opens(this.open == Open::Status) {
                                Open::Status
                            } else {
                                Open::None
                            };
                            cx.notify();
                        })
                    }
                },
                {
                    let entity = cx.entity();
                    move |i, _, cx| {
                        entity.update(cx, |this, cx| {
                            this.open = Open::None;
                            this.edit_engagement(
                                move |engagement| engagement.status = STATUSES[i],
                                cx,
                            );
                        })
                    }
                },
                self.dismiss_menu(cx),
            ))
            .child(section_label(&t, t_("ops-scope")))
            .child(Field::new(&self.scope))
            .when(self.open == Open::Engagements, |this| {
                this.child(
                    self.menus
                        .attach(menu(&t).min_w_0(), self.dismiss_menu(cx))
                        .children(engagements.iter().enumerate().map(|(i, e)| {
                            let dir = e.dir.clone();
                            MenuRow::new(("ops-sidebar-engagement-row", i), e.name.clone())
                                .checked(e.dir == active.engagement.dir)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open = Open::None;
                                    let dir = dir.clone();
                                    this.state
                                        .update(cx, |state, cx| state.set_active(dir.clone(), cx));
                                    cx.emit(OpsEvent::ActiveChanged(dir));
                                    cx.notify();
                                }))
                        }))
                        .child(
                            MenuRow::new("ops-sidebar-new-row", t_("ops-new-engagement"))
                                .icon(Icon::PLUS)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open = Open::None;
                                    cx.emit(OpsEvent::NewEngagementRequested);
                                    cx.notify();
                                })),
                        ),
                )
            });

        let pages = div().flex().flex_col().children(
            [
                (
                    OpsPage::Overview,
                    "ops-section-overview",
                    Icon::LIST_CHECKS,
                    None,
                ),
                (
                    OpsPage::Hosts,
                    "ops-section-hosts",
                    Icon::NETWORK,
                    Some(active.hosts.len()),
                ),
                (
                    OpsPage::Findings,
                    "ops-section-findings",
                    Icon::BUG,
                    Some(active.findings.len()),
                ),
                (
                    OpsPage::Credentials,
                    "ops-section-credentials",
                    Icon::SHIELD_CHECK,
                    Some(active.credentials.len()),
                ),
                (
                    OpsPage::Commands,
                    "ops-section-commands",
                    Icon::TERMINAL,
                    None,
                ),
                (OpsPage::Report, "ops-section-report", Icon::FILE_TEXT, None),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, (page, key, symbol, count))| {
                let row = TreeRow::new(("ops-page", i), t_(key))
                    .icon(symbol)
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(ShowPage(page))));
                match count {
                    Some(n) => row.count(n as u32),
                    None => row,
                }
            }),
        );

        let focus = active.focus_host().cloned();
        let target = div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .child(section_label(&t, t_("ops-target")))
            .child(
                div()
                    .id("ops-sidebar-target")
                    .flex()
                    .items_center()
                    .gap(size::S2)
                    .h(size::CONTROL)
                    .px(size::S2)
                    .rounded(size::RADIUS)
                    .border_1()
                    .border_color(t.line_2)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(t.ink_3))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = if this.menus.click_opens(this.open == Open::Targets) {
                            Open::Targets
                        } else {
                            Open::None
                        };
                        cx.notify();
                    }))
                    .child(icon(Icon::CROSSHAIR, size::ICON_DENSE).text_color(t.ink_3))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .mono()
                            .text_color(if focus.is_some() { t.ink } else { t.ink_3 })
                            .child(focus_kit::ellipsis(
                                focus
                                    .as_ref()
                                    .map(|h| h.ip.clone())
                                    .unwrap_or_else(|| t_("ops-no-target")),
                            )),
                    )
                    .children(
                        focus
                            .as_ref()
                            .and_then(|h| h.hostname.clone())
                            .map(|n| div().flex_none().ui_sm().text_color(t.ink_3).child(n)),
                    )
                    .child(icon(Icon::CHEVRON_DOWN, size::ICON_SMALL).text_color(t.ink_3)),
            )
            .when(self.open == Open::Targets, |this| {
                this.child(
                    self.menus
                        .attach(menu(&t).min_w_0().max_h(px(220.)), self.dismiss_menu(cx))
                        .child(
                            MenuRow::new("ops-sidebar-no-target", t_("ops-no-target"))
                                .checked(focus.is_none())
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.choose_target(None, cx)),
                                ),
                        )
                        .children(
                            active
                                .hosts
                                .iter()
                                .enumerate()
                                .filter(|(_, h)| !h.path.as_os_str().is_empty())
                                .map(|(i, h)| {
                                    let stem = h
                                        .path
                                        .file_stem()
                                        .map(|s| s.to_string_lossy().into_owned());
                                    let selected = focus.as_ref().is_some_and(|f| f.path == h.path);
                                    MenuRow::new(
                                        ("ops-sidebar-target-row", i),
                                        format!(
                                            "{} {}",
                                            h.ip,
                                            h.hostname.clone().unwrap_or_default()
                                        ),
                                    )
                                    .checked(selected)
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| this.choose_target(stem.clone(), cx),
                                    ))
                                }),
                        ),
                )
            });

        let described: HashMap<&str, &str> = vars::known().iter().copied().collect();
        let substitute = self.state.read(cx).settings.substitute;
        let variables_title = crate::ui::row()
            .items_center()
            .gap(size::S2)
            .child(
                div()
                    .id("ops-substitute-box")
                    .tooltip(crate::ui::tip(t_("ops-substitute-hint")))
                    .child(
                        focus_kit::Checkbox::new("ops-substitute", substitute).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.state
                                    .update(cx, |state, cx| state.set_substitute(!substitute, cx));
                            }),
                        ),
                    ),
            )
            .child(section_label(&t, t_("ops-variables")));
        let credentials = focus_ops::loot::list(&active.engagement.dir);
        let variables = div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .child(variables_title)
            .children(inputs.iter().enumerate().map(|(row_index, (name, input))| {
                let canonical = focus_ops::vars::canonical_name(name);
                let linked = active.engagement.var_links.contains_key(&canonical);
                let secret = linked && focus_ops::links::is_secret(&canonical);
                let choices = if focus_ops::links::field_of(&canonical).is_some() {
                    focus_ops::links::choices(&credentials, name)
                } else {
                    Vec::new()
                };
                let offering = self.choice_for.as_deref() == Some(name.as_str())
                    && self.open == Open::Credentials;
                let shown = self.revealed.contains(name);
                let open_name = name.clone();
                let reveal_name = name.clone();
                let mut row = crate::ui::row()
                    .items_center()
                    .gap(size::S2)
                    .child(
                        div()
                            .id(("ops-var-label", name.len()))
                            .flex_none()
                            .w(px(76.))
                            .mono_sm()
                            .text_color(t.ink_2)
                            .tooltip({
                                let text = described
                                    .get(name.as_str())
                                    .map(|_| t_(&format!("ops-var-{}-desc", name.to_lowercase())))
                                    .unwrap_or_default();
                                crate::ui::tip(text)
                            })
                            .child(focus_ops::vars::display_name(name).to_string()),
                    )
                    .child(div().flex_1().min_w_0().child(Field::new(input)));
                if secret {
                    row = row.child(
                        IconButton::new(
                            ("ops-var-reveal", row_index),
                            if shown { EYE_OFF } else { EYE },
                            t_(if shown {
                                "ops-credentials-hide"
                            } else {
                                "ops-credentials-reveal"
                            }),
                        )
                        .dense(true)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.revealed.remove(&reveal_name) {
                                this.revealed.insert(reveal_name.clone());
                            }
                            cx.notify();
                        })),
                    );
                }
                if !choices.is_empty() || linked {
                    row = row.child(
                        IconButton::new(
                            ("ops-var-credentials", row_index),
                            Icon::CHEVRON_DOWN,
                            t_(if linked {
                                "ops-var-credential-linked"
                            } else {
                                "ops-var-credential-pick"
                            }),
                        )
                        .dense(true)
                        .active(linked || offering)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let opens = this.menus.click_opens(
                                this.open == Open::Credentials
                                    && this.choice_for.as_deref() == Some(open_name.as_str()),
                            );
                            this.open = if opens { Open::Credentials } else { Open::None };
                            this.choice_for = opens.then(|| open_name.clone());
                            cx.notify();
                        })),
                    );
                }
                let list = offering.then(|| {
                    let name = name.clone();
                    self.menus
                        .attach(menu(&t).min_w_0().max_h(px(200.)), self.dismiss_menu(cx))
                        .children(choices.into_iter().enumerate().map(|(i, choice)| {
                            let (credential, variable) = (choice.credential.clone(), name.clone());
                            MenuRow::new(("ops-var-choice", i), choice.label).on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.link_variable(
                                        variable.clone(),
                                        Some(credential.clone()),
                                        cx,
                                    )
                                },
                            ))
                        }))
                        .when(linked, |menu| {
                            let variable = name.clone();
                            menu.child(
                                MenuRow::new("ops-var-unlink", t_("ops-var-credential-unlink"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.link_variable(variable.clone(), None, cx)
                                    })),
                            )
                        })
                });
                div()
                    .flex()
                    .flex_col()
                    .gap(size::S1)
                    .child(row)
                    .children(list)
            }));

        let add_variable = crate::ui::row()
            .items_center()
            .gap(size::S2)
            .child(
                div()
                    .w(px(76.))
                    .flex_none()
                    .child(Field::new(&self.new_name)),
            )
            .child(div().flex_1().min_w_0().child(Field::new(&self.new_value)))
            .child(
                IconButton::new("ops-var-add", Icon::PLUS, t_("ops-var-add"))
                    .on_click(cx.listener(|this, _, window, cx| this.add_variable(window, cx))),
            );
        let evidence = Button::new("ops-evidence", t_("ops-evidence-paste"))
            .icon(Icon::IMAGE)
            .on_click(cx.listener(|this, _, _, cx| this.request_evidence(cx)));
        let journal = div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .child(section_label(&t, t_("ops-section-journal")))
            .child(
                crate::ui::row()
                    .gap(size::S2)
                    .child(div().flex_1().min_w_0().child(Field::new(&self.entry)))
                    .child(
                        IconButton::new(
                            "ops-sidebar-journal-add",
                            Icon::PLUS,
                            t_("ops-journal-add"),
                        )
                        .on_click(cx.listener(|this, _, window, cx| this.add_entry(window, cx))),
                    ),
            );

        div()
            .id("ops-sidebar")
            .size_full()
            .overflow_y_scroll()
            .bg(t.bg)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(size::S4)
                    .p(size::S3)
                    .child(engagement)
                    .child(pages)
                    .child(target)
                    .children(exam.as_ref().map(|view| {
                        self.exam_section(
                            &t,
                            &active,
                            view,
                            &active.engagement.exam_done,
                            alert_minutes,
                            cx,
                        )
                    }))
                    .children(exam.is_none().then(|| {
                        let worked =
                            focus_ops::timer::Timer::of(&active.engagement).worked(now_unix());
                        div()
                            .flex()
                            .flex_col()
                            .gap(size::S2)
                            .child(section_label(&t, t_("ops-timer")))
                            .child(
                                crate::ui::start(
                                    div()
                                        .stat()
                                        .font_family(focus_kit::typography::MONO)
                                        .text_color(t.ink),
                                )
                                .child(clock_text(worked)),
                            )
                            .child(self.timer_controls(&t, &active, true, cx))
                    }))
                    .child(variables.child(add_variable))
                    .child(journal)
                    .child(evidence),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_readme_using_exegol_names_gets_one_row_per_variable() {
        let stored = map(&[
            ("TARGET", "10.0.0.5"),
            ("DC_IP", "10.0.0.1"),
            ("PASSWORD", "x"),
            ("DOMAIN", "corp.local"),
            ("USER", "alice"),
        ]);
        let engagement = focus_ops::Engagement {
            vars: stored.clone(),
            ..bare()
        };
        let context = focus_ops::vars::context_for(&engagement, None);
        let names = names_of(&context.values, &stored);
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "{names:?}");
        assert!(
            names.contains(&"DC".to_string())
                && !names.contains(&"DC_IP".to_string())
                && !names.contains(&"PASSWORD".to_string())
                && !names.contains(&"TARGET".to_string()),
            "{names:?}"
        );
    }

    #[test]
    fn other_variables_follow_the_base_ones_once() {
        let stored = map(&[("my-var", "1"), ("MY_VAR", "2"), ("ALPHA", "3")]);
        let names = names_of(&BTreeMap::new(), &stored);
        assert_eq!(&names[BASE_VARIABLES.len()..], ["ALPHA", "MY_VAR"]);
    }

    fn bare() -> focus_ops::Engagement {
        focus_ops::Engagement {
            dir: Default::default(),
            name: "x".into(),
            kind: focus_ops::EngagementKind::Lab,
            status: focus_ops::EngagementStatus::InProgress,
            client: None,
            started: None,
            deadline: None,
            scope: vec![],
            out_of_scope: vec![],
            vars: Default::default(),
            var_links: Default::default(),
            pauses: vec![],
            stopped: None,
            exam: None,
            exam_done: vec![],
            focus: None,
        }
    }
}
