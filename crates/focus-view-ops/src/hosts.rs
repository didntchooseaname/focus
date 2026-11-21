#[path = "hosts_proofs.rs"]
mod proofs;
#[path = "hosts_targets.rs"]
mod targets;

use std::cmp::Ordering;
use std::net::Ipv4Addr;
use std::path::PathBuf;

use focus_i18n::{Arg, t_args};
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Flow as _, Icon, IconButton, Tokens, Type, icon, size,
};
use focus_ops::{Host, hosts};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, EventEmitter, FocusHandle, InteractiveElement,
    IntoElement, ParentElement, Render, ScrollHandle, StatefulInteractiveElement, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::input::{Input, InputState};

use crate::event::{NoticeTone, OpsEvent};
use crate::state::{OpsState, error_text};
use crate::ui::{hatched_empty, host_status_key, mono_chip, screen_header, t_};

const STATUSES: [focus_ops::HostStatus; 5] = [
    focus_ops::HostStatus::New,
    focus_ops::HostStatus::Enum,
    focus_ops::HostStatus::Foothold,
    focus_ops::HostStatus::User,
    focus_ops::HostStatus::Owned,
];
const CHEVRON_UP: Icon = Icon("\u{E070}");
const CHEVRON_DOWN: Icon = Icon("\u{E06D}");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Column {
    Address,
    Name,
    System,
    Status,
    Ports,
}

fn address_key(ip: &str) -> (u8, u32, String) {
    match ip.parse::<Ipv4Addr>() {
        Ok(v4) => (0, u32::from(v4), String::new()),
        Err(_) => (1, 0, ip.to_string()),
    }
}

pub(crate) fn compare(a: &Host, b: &Host, column: Column) -> Ordering {
    let text = |h: &Host, field: Column| match field {
        Column::Name => h.hostname.clone().unwrap_or_default().to_lowercase(),
        _ => h.os.clone().unwrap_or_default().to_lowercase(),
    };
    match column {
        Column::Address => address_key(&a.ip).cmp(&address_key(&b.ip)),
        Column::Name | Column::System => {
            let (x, y) = (text(a, column), text(b, column));
            x.is_empty().cmp(&y.is_empty()).then(x.cmp(&y))
        }
        Column::Status => a.status.cmp(&b.status),
        Column::Ports => a.services.len().cmp(&b.services.len()),
    }
}

pub(crate) fn ports_text(host: &Host) -> String {
    let open: Vec<String> = host
        .services
        .iter()
        .filter(|s| s.state == "open" || s.state.is_empty())
        .map(|s| s.port.to_string())
        .collect();
    if open.len() > 6 {
        format!("{} +{}", open[..6].join(" "), open.len() - 6)
    } else {
        open.join(" ")
    }
}

enum Outcome {
    Report(hosts::ImportReport),
    Failed(String),
}

pub struct HostsTable {
    state: Entity<OpsState>,
    sort: (Column, bool),
    importing: bool,
    adding: bool,
    target_inputs: targets::Inputs,
    target_preview: Option<focus_ops::targets::Preview>,
    target_message: Option<targets::Message>,
    paste: Entity<InputState>,
    status_menu: bool,
    menus: crate::ui::MenuFocus,
    focus: FocusHandle,
    rows_scroll: ScrollHandle,
    generation: u64,
    outcome: Option<Outcome>,
    busy: bool,
    selected: Option<PathBuf>,
    proof_inputs: proofs::Inputs,
    proofs: Vec<focus_ops::proofs::Proof>,
    proofs_for: Option<PathBuf>,
    proofs_shown: bool,
    proof_machine: Option<usize>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<OpsEvent> for HostsTable {}

impl HostsTable {
    pub fn new(state: Entity<OpsState>, window: &mut Window, cx: &mut Context<Self>) -> HostsTable {
        let paste = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(6, 12)
                .placeholder(t_("ops-hosts-paste-placeholder"))
        });
        let subscription = cx.observe(&state, |_, _, cx| cx.notify());
        let mut table = HostsTable {
            generation: focus_i18n::generation(),
            state,
            sort: (Column::Address, true),
            importing: false,
            adding: false,
            target_inputs: targets::Inputs::new(window, cx),
            target_preview: None,
            target_message: None,
            paste,
            status_menu: false,
            menus: crate::ui::MenuFocus::new(cx),
            focus: crate::ui::list_focus(cx),
            rows_scroll: ScrollHandle::new(),
            outcome: None,
            busy: false,
            selected: None,
            proof_inputs: proofs::Inputs::new(window, cx),
            proofs: Vec::new(),
            proofs_for: None,
            proofs_shown: false,
            proof_machine: None,
            _subscriptions: vec![subscription],
        };
        if let Some(text) = std::env::var_os("FOCUS_OPS_TARGETS_FILE")
            .and_then(|path| std::fs::read_to_string(path).ok())
        {
            table.adding = true;
            table
                .target_inputs
                .paste
                .update(cx, |input, cx| input.set_value(text.clone(), window, cx));
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(900))
                    .await;
                this.update(cx, |this, cx| this.preview_text(text, cx)).ok();
            })
            .detach();
        }
        table
    }

    pub fn show_import(
        &mut self,
        text: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.importing = true;
        self.outcome = None;
        if let Some(text) = text {
            self.paste
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
        cx.notify();
    }

    pub fn import_pasted(&mut self, cx: &mut Context<Self>) {
        let text = self.paste.read(cx).value().to_string();
        self.import_text(text, cx);
    }

    pub fn select_nth(&mut self, n: usize, cx: &mut Context<Self>) {
        let Some(active) = self.state.read(cx).active.clone() else {
            return;
        };
        let mut rows = active.hosts;
        let (column, ascending) = self.sort;
        rows.sort_by(|a, b| {
            if ascending {
                compare(a, b, column)
            } else {
                compare(b, a, column)
            }
        });
        self.selected = rows
            .get(n)
            .map(|h| h.path.clone())
            .filter(|p| !p.as_os_str().is_empty());
        cx.notify();
    }

    fn jump(&mut self, jump: crate::ui::Jump, _: &mut Window, cx: &mut Context<Self>) {
        let Some(active) = self.state.read(cx).active.clone() else {
            return;
        };
        let mut rows = active.hosts;
        let (column, ascending) = self.sort;
        rows.sort_by(|a, b| {
            if ascending {
                compare(a, b, column)
            } else {
                compare(b, a, column)
            }
        });
        let current = self
            .selected
            .as_ref()
            .and_then(|path| rows.iter().position(|h| &h.path == path));
        let page = crate::ui::rows_per_page(&self.rows_scroll, 30.);
        if let Some(n) = crate::ui::landing(current, rows.len(), page, jump) {
            self.select_nth(n, cx);
            self.rows_scroll.scroll_to_item(n);
        }
    }

    pub fn sort_by(&mut self, column: &str, cx: &mut Context<Self>) {
        let column = match column {
            "address" => Column::Address,
            "name" => Column::Name,
            "system" => Column::System,
            "status" => Column::Status,
            "ports" => Column::Ports,
            _ => return,
        };
        self.sort = if self.sort.0 == column {
            (column, !self.sort.1)
        } else {
            (column, true)
        };
        cx.notify();
    }

    pub fn import_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match std::fs::read_to_string(&path) {
            Ok(text) => self.import_text(text, cx),
            Err(error) => {
                self.outcome = Some(Outcome::Failed(error_text(&focus_ops::OpsError::Io {
                    path,
                    source: error,
                })));
                self.importing = true;
                cx.notify();
            }
        }
    }

    fn import_text(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(dir) = self
            .state
            .read(cx)
            .active
            .as_ref()
            .map(|a| a.engagement.dir.clone())
        else {
            return;
        };
        self.busy = true;
        self.importing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    hosts::parse_nmap(&text).and_then(|parsed| hosts::import(&dir, &parsed))
                })
                .await;
            this.update(cx, |this, cx| {
                this.busy = false;
                this.outcome = Some(match result {
                    Ok(report) => Outcome::Report(report),
                    Err(error) => Outcome::Failed(error_text(&error)),
                });
                this.state.update(cx, |state, cx| state.reload(cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn set_target(&mut self, stem: String, clear: bool, cx: &mut Context<Self>) {
        let Some(mut engagement) = self
            .state
            .read(cx)
            .active
            .as_ref()
            .map(|a| a.engagement.clone())
        else {
            return;
        };
        engagement.focus = if clear { None } else { Some(stem) };
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
    }

    fn set_host_status(
        &mut self,
        path: &std::path::Path,
        status: focus_ops::HostStatus,
        cx: &mut Context<Self>,
    ) {
        let Some(mut host) = self
            .state
            .read(cx)
            .active
            .as_ref()
            .and_then(|a| a.hosts.iter().find(|h| h.path == path).cloned())
        else {
            return;
        };
        let Some(dir) = self
            .state
            .read(cx)
            .active
            .as_ref()
            .map(|a| a.engagement.dir.clone())
        else {
            return;
        };
        host.status = status;
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(async move { hosts::upsert(&dir, &host) })
                .await;
            this.update(cx, |this, cx| match saved {
                Ok(_) => this.state.update(cx, |state, cx| state.reload(cx)),
                Err(error) => cx.emit(OpsEvent::Notify {
                    tone: NoticeTone::Danger,
                    text: error_text(&error),
                }),
            })
            .ok();
        })
        .detach();
    }

    fn choose_scan_file(&mut self, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(mut paths))) = chosen.await else {
                return;
            };
            let Some(path) = paths.pop() else { return };
            this.update(cx, |this, cx| this.import_path(path, cx)).ok();
        })
        .detach();
    }

    fn host_panel(
        &self,
        t: &Tokens,
        active: &crate::state::Active,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        use focus_ops::relations::RelationKind;
        let host = active.hosts.iter().find(|h| h.path == path)?;
        let cited = |kind: RelationKind| {
            active
                .relations
                .iter()
                .filter(move |r| r.kind == kind && r.to == path)
        };
        let findings: Vec<_> = cited(RelationKind::FindingHost)
            .map(|r| {
                (
                    r.label.clone(),
                    r.from.clone(),
                    active
                        .findings
                        .iter()
                        .find(|f| f.path == r.from)
                        .map(|f| f.severity),
                )
            })
            .collect();
        let credentials: Vec<_> = cited(RelationKind::CredentialHost)
            .map(|r| (r.label.clone(), r.from.clone()))
            .collect();
        let none = || {
            div()
                .ui_sm()
                .text_color(t.ink_3)
                .child(t_("ops-host-cited-none"))
        };
        let list_row = |id: (&'static str, usize), target: PathBuf, cx: &mut Context<Self>| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(size::S2)
                .min_h(px(26.))
                .cursor_pointer()
                .ui_sm()
                .text_color(t.ink)
                .hover({
                    let bg = t.card_2;
                    move |s| s.bg(bg)
                })
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(OpsEvent::OpenNote {
                        path: target.clone(),
                        line: None,
                    })
                }))
        };
        Some(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .w(px(320.))
                .h_full()
                .gap(size::S3)
                .p(size::S4)
                .border_start_1()
                .border_color(t.line)
                .child(div().mono().text_color(t.ink).child(host.ip.clone()))
                .children(
                    host.hostname
                        .clone()
                        .map(|n| div().ui().text_color(t.ink_2).child(n)),
                )
                .child(
                    div()
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(host.os.clone().unwrap_or_default()),
                )
                .child(crate::ui::dropdown(
                    t,
                    "ops-host-status",
                    t_(host_status_key(host.status)),
                    STATUSES
                        .iter()
                        .map(|s| (t_(host_status_key(*s)), *s == host.status))
                        .collect(),
                    self.status_menu,
                    &self.menus,
                    {
                        let entity = cx.entity();
                        move |_, cx| {
                            entity.update(cx, |this, cx| {
                                this.status_menu = this.menus.click_opens(this.status_menu);
                                cx.notify();
                            })
                        }
                    },
                    {
                        let entity = cx.entity();
                        let path = host.path.clone();
                        move |i, _, cx| {
                            entity.update(cx, |this, cx| {
                                this.status_menu = false;
                                this.set_host_status(&path, STATUSES[i], cx);
                            })
                        }
                    },
                    {
                        let entity = cx.entity();
                        move |_, cx| {
                            entity.update(cx, |this, cx| {
                                this.status_menu = false;
                                this.menus.dismissed();
                                cx.notify();
                            })
                        }
                    },
                    cx,
                ))
                .child(
                    self.proofs_section(
                        t,
                        active
                            .engagement
                            .exam
                            .as_deref()
                            .and_then(|id| focus_ops::exam::profiles().iter().find(|p| p.id == id))
                            .map_or(0, |p| p.machines.len()),
                        cx,
                    ),
                )
                .child(crate::ui::section_label(t, t_("ops-host-cited-findings")))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .children(findings.iter().enumerate().map(
                            |(i, (label, target, severity))| {
                                list_row(("ops-host-finding", i), target.clone(), cx)
                                    .children(severity.map(|s| {
                                        div()
                                            .flex_none()
                                            .w(px(84.))
                                            .child(crate::ui::severity_tag(t, s))
                                    }))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .child(focus_kit::ellipsis(label.clone())),
                                    )
                            },
                        )),
                )
                .children(findings.is_empty().then(none))
                .child(crate::ui::section_label(
                    t,
                    t_("ops-host-cited-credentials"),
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .children(credentials.iter().enumerate().map(|(i, (label, target))| {
                            list_row(("ops-host-credential", i), target.clone(), cx)
                                .child(div().mono_sm().child(label.clone()))
                        })),
                )
                .children(credentials.is_empty().then(none))
                .child(
                    Button::new("ops-host-close", t_("ops-close")).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.selected = None;
                            cx.notify();
                        },
                    )),
                ),
        )
    }

    fn header_cell(
        &self,
        t: &Tokens,
        column: Column,
        label: String,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let active = self.sort.0 == column;
        div()
            .id(("ops-hosts-sort", column as usize))
            .flex()
            .items_center()
            .gap(size::S1)
            .cursor_pointer()
            .text_color(if active { t.ink } else { t.ink_3 })
            .hover({
                let ink = t.ink;
                move |s| s.text_color(ink)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.sort = if this.sort.0 == column {
                    (column, !this.sort.1)
                } else {
                    (column, true)
                };
                cx.notify();
            }))
            .child(div().ui_xs().child(label))
            .when(active, |this| {
                this.child(icon(
                    if self.sort.1 {
                        CHEVRON_UP
                    } else {
                        CHEVRON_DOWN
                    },
                    size::ICON_SMALL,
                ))
            })
    }
}

fn cells(t: &Tokens, host: &Host) -> [AnyElement; 5] {
    let status_ink = match host.status {
        focus_ops::HostStatus::Owned => t.ok,
        focus_ops::HostStatus::Foothold | focus_ops::HostStatus::User => t.warn,
        _ => t.ink_2,
    };
    [
        div()
            .flex_none()
            .w(px(132.))
            .mono()
            .text_color(t.ink)
            .child(host.ip.clone())
            .into_any_element(),
        div()
            .flex_1()
            .min_w_0()
            .ui()
            .text_color(t.ink_body)
            .child(focus_kit::ellipsis(
                host.hostname.clone().unwrap_or_default(),
            ))
            .into_any_element(),
        div()
            .flex_1()
            .min_w_0()
            .ui()
            .text_color(t.ink_2)
            .child(focus_kit::ellipsis(host.os.clone().unwrap_or_default()))
            .into_any_element(),
        div()
            .flex_none()
            .w(px(96.))
            .ui()
            .text_color(status_ink)
            .child(focus_kit::truncated(
                gpui::ElementId::Name(format!("ops-host-status-{}", host.ip).into()),
                t_(host_status_key(host.status)),
            ))
            .into_any_element(),
        div()
            .flex_1()
            .min_w_0()
            .mono_sm()
            .text_color(t.ink_2)
            .child(focus_kit::ellipsis(ports_text(host)))
            .into_any_element(),
    ]
}

impl Render for HostsTable {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        self.menus.sync(self.status_menu, window, cx);
        if self.generation != focus_i18n::generation() {
            self.generation = focus_i18n::generation();
            self.paste.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-hosts-paste-placeholder"), window, cx)
            });
            for (input, key) in self.target_inputs.placeholders() {
                input.update(cx, |input, cx| input.set_placeholder(t_(key), window, cx));
            }
        }

        let Some(active) = self.state.read(cx).active.clone() else {
            return div()
                .id("ops-hosts")
                .size_full()
                .bg(t.bg)
                .flex()
                .child(hatched_empty(&t, t_("ops-no-engagement"), None));
        };
        let mut rows: Vec<Host> = active.hosts.clone();
        let (column, ascending) = self.sort;
        rows.sort_by(|a, b| {
            if ascending {
                compare(a, b, column)
            } else {
                compare(b, a, column)
            }
        });
        let focus = active.focus_host().map(|h| h.path.clone());
        let count = t_args("ops-hosts-count", &[("count", Arg::from(rows.len()))]);

        let toolbar = crate::ui::row()
            .items_center()
            .justify_between()
            .child(screen_header(&t, t_("ops-section-hosts"), Some(count)))
            .child(
                crate::ui::row()
                    .gap(size::S2)
                    .child(
                        Button::new("ops-hosts-add", t_("ops-targets-add-button"))
                            .icon(Icon::PLUS)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.show_targets(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("ops-hosts-import", t_("ops-hosts-import-nmap"))
                            .icon(Icon::FILE_INPUT)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_import(None, window, cx);
                            })),
                    ),
            );
        let targets_panel = self.adding.then(|| self.targets_panel(&t, cx));

        let import_panel = self.importing.then(|| {
            let message: Option<AnyElement> = match &self.outcome {
                Some(Outcome::Report(r)) => Some(
                    div()
                        .ui_sm()
                        .text_color(t.ok)
                        .child(t_args(
                            "ops-hosts-import-report",
                            &[
                                ("created", Arg::from(r.created.len())),
                                ("updated", Arg::from(r.updated.len())),
                                ("services", Arg::from(r.services)),
                            ],
                        ))
                        .into_any_element(),
                ),
                Some(Outcome::Failed(text)) => Some(
                    div()
                        .ui_sm()
                        .text_color(t.danger)
                        .child(text.clone())
                        .into_any_element(),
                ),
                None => None,
            };
            let text = self.paste.read(cx).value().to_string();
            div()
                .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        this.importing = false;
                        this.outcome = None;
                        cx.notify();
                    }
                }))
                .flex()
                .flex_col()
                .gap(size::S2)
                .p(size::S3)
                .border_1()
                .border_color(t.line_2)
                .rounded(size::RADIUS)
                .child(
                    div()
                        .ui_xs()
                        .text_color(t.ink_3)
                        .child(t_("ops-hosts-import-hint")),
                )
                .child(
                    div()
                        .px(size::S2)
                        .py(size::S1)
                        .rounded(size::RADIUS)
                        .border_1()
                        .border_color(t.line_2)
                        .bg(t.bg_2)
                        .child(
                            Input::new(&self.paste)
                                .appearance(false)
                                .px_0()
                                .py_0()
                                .text_size(px(12.5))
                                .line_height(px(18.)),
                        ),
                )
                .children(message)
                .child(
                    crate::ui::row()
                        .gap(size::S2)
                        .child(
                            Button::new("ops-hosts-do-import", t_("ops-hosts-import"))
                                .kind(ButtonKind::Primary)
                                .disabled(self.busy || text.trim().is_empty())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let text = this.paste.read(cx).value().to_string();
                                    this.import_text(text, cx);
                                })),
                        )
                        .child(
                            Button::new("ops-hosts-import-file", t_("ops-hosts-import-file"))
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| this.choose_scan_file(cx))),
                        )
                        .child(
                            Button::new("ops-hosts-close-import", t_("ops-close")).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.importing = false;
                                    this.outcome = None;
                                    cx.notify();
                                }),
                            ),
                        ),
                )
        });

        let header = crate::ui::row()
            .items_center()
            .gap(size::S3)
            .h(px(28.))
            .px(size::S3)
            .border_b_1()
            .border_color(t.line)
            .child(div().flex_none().w(px(132.)).child(self.header_cell(
                &t,
                Column::Address,
                t_("ops-hosts-column-address"),
                cx,
            )))
            .child(div().flex_1().min_w_0().child(self.header_cell(
                &t,
                Column::Name,
                t_("ops-hosts-column-name"),
                cx,
            )))
            .child(div().flex_1().min_w_0().child(self.header_cell(
                &t,
                Column::System,
                t_("ops-hosts-column-os"),
                cx,
            )))
            .child(div().flex_none().w(px(96.)).child(self.header_cell(
                &t,
                Column::Status,
                t_("ops-hosts-column-status"),
                cx,
            )))
            .child(div().flex_1().min_w_0().child(self.header_cell(
                &t,
                Column::Ports,
                t_("ops-hosts-column-ports"),
                cx,
            )))
            .child(div().flex_none().w(px(60.)));

        let has_rows = !rows.is_empty();
        let body: AnyElement = if rows.is_empty() {
            hatched_empty(&t, t_("ops-hosts-empty"), None).into_any_element()
        } else {
            div()
                .id("ops-hosts-rows")
                .flex_shrink()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.rows_scroll)
                .children(rows.into_iter().enumerate().map(|(i, host)| {
                    let is_target = focus.as_ref().is_some_and(|f| *f == host.path);
                    let stem = host
                        .path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let path = host.path.clone();
                    let has_note = !path.as_os_str().is_empty();
                    let [address, name, system, status, ports] = cells(&t, &host);
                    let selected = self.selected.clone();
                    div()
                        .id(("ops-host-row", i))
                        .relative()
                        .flex()
                        .items_center()
                        .gap(size::S3)
                        .h(px(30.))
                        .px(size::S3)
                        .border_b_1()
                        .border_color(t.line)
                        .cursor_pointer()
                        .when(selected.as_ref() == Some(&host.path), |this| {
                            this.bg(t.card_2)
                        })
                        .on_click(cx.listener({
                            let path = host.path.clone();
                            move |this, _, _, cx| {
                                this.selected = (!path.as_os_str().is_empty()
                                    && this.selected.as_ref() != Some(&path))
                                .then(|| path.clone());
                                cx.notify();
                            }
                        }))
                        .hover(move |s| s.bg(t.card_2))
                        .when(is_target, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .bottom_0()
                                    .w(px(2.))
                                    .bg(t.ink),
                            )
                        })
                        .child(address)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap(size::S2)
                                .child(name)
                                .when(is_target, |this| {
                                    this.child(mono_chip(&t, t_("ops-target")))
                                }),
                        )
                        .child(system)
                        .child(status)
                        .child(ports)
                        .child(
                            crate::ui::row()
                                .flex_none()
                                .w(px(60.))
                                .child(
                                    IconButton::new(
                                        ("ops-host-target", i),
                                        Icon::CROSSHAIR,
                                        t_("ops-set-target"),
                                    )
                                    .dense(true)
                                    .active(is_target)
                                    .disabled(!has_note)
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            this.set_target(stem.clone(), is_target, cx)
                                        },
                                    )),
                                )
                                .child(
                                    IconButton::new(
                                        ("ops-host-open", i),
                                        Icon::FILE_TEXT,
                                        t_("ops-open-note"),
                                    )
                                    .dense(true)
                                    .disabled(!has_note)
                                    .on_click(cx.listener(
                                        move |_, _, _, cx| {
                                            cx.emit(OpsEvent::OpenNote {
                                                path: path.clone(),
                                                line: None,
                                            })
                                        },
                                    )),
                                ),
                        )
                }))
                .into_any_element()
        };

        self.ensure_proofs(cx);
        let panel = self
            .selected
            .clone()
            .and_then(|path| self.host_panel(&t, &active, &path, cx));
        let main = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap(size::S4)
            .p(size::S6)
            .child(toolbar)
            .children(targets_panel)
            .children(import_panel)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(header)
                    .child(body)
                    .children(has_rows.then(crate::ui::table_filler)),
            );
        crate::ui::pageable(
            crate::ui::row().id("ops-hosts"),
            &self.focus,
            cx,
            Self::jump,
        )
        .size_full()
        .bg(t.bg)
        .child(main)
        .children(panel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focus_ops::{HostStatus, Service};

    fn host(ip: &str, name: Option<&str>, status: HostStatus, ports: usize) -> Host {
        Host {
            path: PathBuf::new(),
            ip: ip.into(),
            hostname: name.map(Into::into),
            os: None,
            status,
            services: (0..ports)
                .map(|p| Service {
                    port: 20 + p as u16,
                    proto: "tcp".into(),
                    state: "open".into(),
                    name: String::new(),
                    product: String::new(),
                    version: String::new(),
                    notes: String::new(),
                })
                .collect(),
            tags: vec![],
            flags: vec![],
            points: None,
        }
    }

    #[test]
    fn addresses_sort_numerically() {
        let (a, b) = (
            host("10.10.10.9", None, HostStatus::New, 0),
            host("10.10.10.10", None, HostStatus::New, 0),
        );
        assert_eq!(compare(&a, &b, Column::Address), Ordering::Less);
    }

    #[test]
    fn missing_names_sort_after_named_hosts() {
        let (named, unnamed) = (
            host("1.1.1.1", Some("web"), HostStatus::New, 0),
            host("1.1.1.2", None, HostStatus::New, 0),
        );
        assert_eq!(compare(&named, &unnamed, Column::Name), Ordering::Less);
    }

    #[test]
    fn status_and_port_columns_compare_by_value() {
        let (a, b) = (
            host("1.1.1.1", None, HostStatus::Owned, 1),
            host("1.1.1.2", None, HostStatus::New, 3),
        );
        assert_eq!(compare(&a, &b, Column::Status), Ordering::Greater);
        assert_eq!(compare(&a, &b, Column::Ports), Ordering::Less);
    }

    #[test]
    fn long_port_lists_are_cut() {
        assert_eq!(
            ports_text(&host("1.1.1.1", None, HostStatus::New, 8)),
            "20 21 22 23 24 25 +2"
        );
    }
}
