use focus_i18n::{Arg, t_args};
use focus_kit::{ActiveTokens, Button, ButtonKind, Icon, Type, dot, size};
use focus_ops::{Severity, checklist};
use gpui::{
    AnyElement, Context, Entity, EventEmitter, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};
use gpui::{InteractiveElement, StatefulInteractiveElement};

use crate::event::OpsEvent;
use crate::state::{Active, OpsState};
use crate::ui::{
    clip_to, hatched_empty, mono_chip, rule_v, section_label, severity_color, severity_tag, t_,
    tile,
};

pub struct OpsDashboard {
    state: Entity<OpsState>,
    compact: crate::rich::Compact,
    _subscription: Subscription,
}

impl EventEmitter<OpsEvent> for OpsDashboard {}

impl OpsDashboard {
    pub fn new(state: Entity<OpsState>, cx: &mut Context<Self>) -> OpsDashboard {
        let subscription = cx.observe(&state, |_, _, cx| cx.notify());
        OpsDashboard {
            state,
            compact: crate::rich::Compact::new(),
            _subscription: subscription,
        }
    }
}

fn next_step(active: &Active) -> Option<(String, String)> {
    active.checklist.iter().find_map(|s: &checklist::Section| {
        s.next
            .as_ref()
            .map(|(_, text)| (s.title.clone(), text.clone()))
    })
}

impl Render for OpsDashboard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let state = self.state.read(cx);
        let Some(active) = state.active.clone() else {
            let message = if state.loading {
                t_("ops-loading")
            } else {
                t_("ops-no-engagement")
            };
            let action: Option<AnyElement> = (!state.loading).then(|| {
                Button::new("ops-dashboard-new", t_("ops-new-engagement"))
                    .kind(ButtonKind::Primary)
                    .icon(Icon::PLUS)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(OpsEvent::NewEngagementRequested)))
                    .into_any_element()
            });
            return div()
                .id("ops-dashboard")
                .flex()
                .size_full()
                .bg(t.bg)
                .child(hatched_empty(&t, message, action));
        };
        let overview = &active.overview;
        let exam = active
            .engagement
            .exam
            .as_deref()
            .and_then(|id| focus_ops::exam::profiles().iter().find(|p| p.id == id));
        let figure = |n: usize| n.to_string();
        let mut tiles = vec![
            tile(&t, figure(overview.hosts), t_("ops-stat-hosts"), None),
            tile(&t, figure(overview.hosts_owned), t_("ops-stat-owned"), None),
        ];
        for severity in [
            Severity::Critical,
            Severity::High,
            Severity::Medium,
            Severity::Low,
        ] {
            let n = overview.findings[severity as usize]
                + if severity == Severity::Low {
                    overview.findings[Severity::Info as usize]
                } else {
                    0
                };
            tiles.push(tile(
                &t,
                figure(n),
                t_(crate::ui::severity_key(severity)),
                Some(severity_color(&t, severity)),
            ));
        }
        tiles.push(tile(
            &t,
            figure(overview.credentials),
            t_("ops-section-credentials"),
            None,
        ));
        tiles.push(tile(
            &t,
            figure(overview.journal_entries),
            t_("ops-section-journal"),
            None,
        ));
        if let Some(profile) = exam {
            let goal = profile
                .pass_points
                .map(|p| format!(" / {p}"))
                .unwrap_or_default();
            let points = if profile.machines.is_empty() {
                overview.points
            } else {
                focus_ops::exam::score(profile, &active.engagement.exam_done)
            };
            tiles.push(tile(
                &t,
                format!("{points}{goal}"),
                t_("ops-exam-points-label"),
                None,
            ));
        }
        let mut tile_row = crate::ui::row()
            .flex_none()
            .py(size::S4)
            .border_y_1()
            .border_color(t.line);
        let count = tiles.len();
        for (i, one) in tiles.into_iter().enumerate() {
            tile_row = tile_row.child(one);
            if i + 1 < count {
                tile_row = tile_row.child(rule_v(&t));
            }
        }

        let status_color = match active.engagement.status {
            focus_ops::EngagementStatus::InProgress => t.ok,
            focus_ops::EngagementStatus::Done => t.ink_3,
            _ => t.warn,
        };
        let header = div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .child(
                crate::ui::start(div().display().text_color(t.ink))
                    .child(active.engagement.name.clone()),
            )
            .child(
                crate::ui::row()
                    .items_center()
                    .gap(size::S3)
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(
                        crate::ui::row()
                            .items_center()
                            .gap(size::S2)
                            .child(dot(status_color))
                            .child(
                                div()
                                    .text_color(t.ink_2)
                                    .child(t_(status_key(active.engagement.status))),
                            ),
                    )
                    .child(div().child(t_(kind_key(active.engagement.kind))))
                    .when_some(active.engagement.client.clone(), |this, client| {
                        this.child(div().child(client))
                    })
                    .when_some(active.engagement.deadline.clone(), |this, d| {
                        this.child(div().mono_sm().child(d))
                    }),
            );

        let done = overview.checklist_done;
        let total = overview.checklist_total;
        let fraction = if total == 0 {
            0.
        } else {
            done as f32 / total as f32
        };
        let progress =
            div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .child(section_label(&t, t_("ops-section-checklists")))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(size::S2)
                        .child(crate::ui::start(div().ui().text_color(t.ink)).child(t_args(
                            "ops-checklist-progress",
                            &[("done", Arg::from(done)), ("total", Arg::from(total))],
                        )))
                        .child(
                            div().relative().h(px(4.)).w_full().bg(t.line).child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .h_full()
                                    .w(gpui::relative(fraction))
                                    .bg(t.ink),
                            ),
                        )
                        .children(next_step(&active).map(|(section, text)| {
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(crate::ui::start(div().ui_xs().text_color(t.ink_3)).child(
                                    format!("{}: {}", t_("ops-checklist-next-step"), section),
                                ))
                                .child(
                                    crate::ui::start(div().ui().text_color(t.ink_body))
                                        .child(clip_to(&text, 140)),
                                )
                        })),
                );

        let events = {
            let latest: Vec<_> = active.journal.iter().rev().take(6).cloned().collect();
            let cells: Vec<AnyElement> = latest
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    self.compact
                        .element(format!("event-{i}"), &e.text, &active.hosts, cx)
                })
                .collect();
            let body: AnyElement =
                if latest.is_empty() {
                    hatched_empty(&t, t_("ops-journal-empty"), None).into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(size::S1)
                        .children(latest.into_iter().zip(cells).enumerate().map(
                            |(i, (e, cell))| {
                                let whole: SharedString =
                                    crate::rich::plain(&e.text, &active.hosts).into();
                                crate::ui::row()
                                    .gap(size::S3)
                                    .h(px(22.))
                                    .items_center()
                                    .child(
                                        div()
                                            .flex_none()
                                            .mono_sm()
                                            .text_color(t.ink_3)
                                            .child(e.timestamp),
                                    )
                                    .children(e.target.map(|target| {
                                        div()
                                            .flex_none()
                                            .mono_sm()
                                            .text_color(t.ink_2)
                                            .child(target)
                                    }))
                                    .child(
                                        div()
                                            .id(("dashboard-event", i))
                                            .flex_1()
                                            .min_w_0()
                                            .h(px(20.))
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .tooltip(move |_, cx| {
                                                focus_kit::Tooltip::view(whole.clone(), None, cx)
                                            })
                                            .child(cell),
                                    )
                            },
                        ))
                        .into_any_element()
                };
            div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .child(section_label(&t, t_("ops-latest-events")))
                .child(body)
        };

        let findings = {
            let mut recent = active.findings.clone();
            recent.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.id.cmp(&b.id)));
            recent.truncate(6);
            let body: AnyElement = if recent.is_empty() {
                hatched_empty(&t, t_("ops-findings-empty"), None).into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .children(recent.into_iter().enumerate().map(|(i, f)| {
                        let path = f.path.clone();
                        div()
                            .id(("ops-dash-finding", i))
                            .flex()
                            .items_center()
                            .gap(size::S3)
                            .h(px(26.))
                            .cursor_pointer()
                            .hover(move |s| s.bg(t.card_2))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(OpsEvent::OpenNote {
                                    path: path.clone(),
                                    line: None,
                                })
                            }))
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(84.))
                                    .child(severity_tag(&t, f.severity)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .ui_sm()
                                    .text_color(t.ink_body)
                                    .child(focus_kit::ellipsis(f.title)),
                            )
                            .children(f.cvss_score.map(|s| mono_chip(&t, format!("{s:.1}"))))
                    }))
                    .into_any_element()
            };
            div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .child(section_label(&t, t_("ops-section-findings")))
                .child(body)
        };

        let hosts = {
            let mut recent = active.hosts.clone();
            recent.sort_by(|a, b| b.status.cmp(&a.status).then_with(|| a.ip.cmp(&b.ip)));
            recent.truncate(6);
            let body: AnyElement =
                if recent.is_empty() {
                    hatched_empty(&t, t_("ops-hosts-empty"), None).into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .children(recent.into_iter().enumerate().map(|(i, h)| {
                            let path = h.path.clone();
                            div()
                                .id(("ops-dash-host", i))
                                .flex()
                                .items_center()
                                .gap(size::S3)
                                .h(px(26.))
                                .cursor_pointer()
                                .hover(move |s| s.bg(t.card_2))
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    if !path.as_os_str().is_empty() {
                                        cx.emit(OpsEvent::OpenNote {
                                            path: path.clone(),
                                            line: None,
                                        })
                                    }
                                }))
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(112.))
                                        .mono_sm()
                                        .text_color(t.ink)
                                        .child(h.ip.clone()),
                                )
                                .child(div().flex_1().min_w_0().ui_sm().text_color(t.ink_2).child(
                                    focus_kit::ellipsis(h.hostname.clone().unwrap_or_default()),
                                ))
                                .child(
                                    div()
                                        .flex_none()
                                        .ui_sm()
                                        .text_color(t.ink_3)
                                        .child(t_(crate::ui::host_status_key(h.status))),
                                )
                        }))
                        .into_any_element()
                };
            div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .child(section_label(&t, t_("ops-section-hosts")))
                .child(body)
        };

        div()
            .id("ops-dashboard")
            .size_full()
            .overflow_y_scroll()
            .bg(t.bg)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(size::S6)
                    .p(size::S6)
                    .child(header)
                    .child(tile_row)
                    .child(
                        crate::ui::row()
                            .gap(size::S8)
                            .items_start()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(size::S6)
                                    .child(progress)
                                    .child(events),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(size::S6)
                                    .child(findings)
                                    .child(hosts),
                            ),
                    ),
            )
    }
}

pub(crate) fn status_key(status: focus_ops::EngagementStatus) -> &'static str {
    match status {
        focus_ops::EngagementStatus::Planning => "ops-status-planning",
        focus_ops::EngagementStatus::InProgress => "ops-status-in-progress",
        focus_ops::EngagementStatus::Reporting => "ops-status-reporting",
        focus_ops::EngagementStatus::Done => "ops-status-done",
    }
}

pub(crate) fn kind_key(kind: focus_ops::EngagementKind) -> &'static str {
    use focus_ops::EngagementKind::*;
    match kind {
        ExternalPentest => "ops-kind-external-pentest",
        InternalPentest => "ops-kind-internal-pentest",
        WebPentest => "ops-kind-web-pentest",
        RedTeam => "ops-kind-red-team",
        Lab => "ops-kind-lab",
        Exam => "ops-kind-exam",
    }
}
