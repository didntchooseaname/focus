use focus_i18n::{Arg, t_args};
use focus_kit::{Button, ButtonKind, Field, Icon, IconButton, Tokens, Type, size};
use focus_ops::proofs::{self, Proof};
use gpui::{AppContext as _, Context, Entity, ParentElement, Styled, Window, div};
use gpui_component::input::InputState;

use super::HostsTable;
use crate::event::{NoticeTone, OpsEvent};
use crate::state::error_text;
use crate::time::{local_iso, now_unix};
use crate::ui::{EYE, EYE_OFF, section_label, t_};

pub(super) struct Inputs {
    pub name: Entity<InputState>,
    pub value: Entity<InputState>,
}

impl Inputs {
    pub(super) fn new(window: &mut Window, cx: &mut Context<HostsTable>) -> Inputs {
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-proofs-name"))
        });
        let value = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .masked(true)
                .placeholder(t_("ops-proofs-value"))
        });
        Inputs { name, value }
    }
}

impl HostsTable {
    pub(super) fn ensure_proofs(&mut self, cx: &mut Context<Self>) {
        if self.proofs_for == self.selected {
            return;
        }
        self.proofs_for = self.selected.clone();
        self.proofs.clear();
        let Some(path) = self.selected.clone() else {
            return;
        };
        let wanted = path.clone();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move { proofs::read(&path) })
                .await;
            this.update(cx, |this, cx| {
                if this.proofs_for.as_ref() == Some(&wanted) {
                    this.proofs = read.unwrap_or_default();
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn record_proof(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(path), Some(mut engagement)) = (
            self.selected.clone(),
            self.state
                .read(cx)
                .active
                .as_ref()
                .map(|a| a.engagement.clone()),
        ) else {
            return;
        };
        let proof = Proof {
            name: self.proof_inputs.name.read(cx).value().trim().to_string(),
            value: self.proof_inputs.value.read(cx).value().trim().to_string(),
            taken: local_iso(now_unix()),
            capture: String::new(),
        };
        let machine = self.proof_machine;
        for input in [&self.proof_inputs.name, &self.proof_inputs.value] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.spawn(async move |this, cx| {
            let target = path.clone();
            let recorded = cx
                .background_spawn(async move {
                    proofs::record(&mut engagement, &target, &proof, machine)
                })
                .await;
            this.update(cx, |this, cx| {
                match recorded {
                    Ok(_) => {
                        this.proofs_for = None;
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

    pub(super) fn proofs_section(
        &self,
        t: &Tokens,
        exam_machines: usize,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let shown = self.proofs_shown;
        let rows = self.proofs.iter().map(|proof| {
            let value = if proof.value.is_empty() {
                String::new()
            } else if shown {
                proof.value.clone()
            } else {
                "\u{2022}".repeat(8)
            };
            crate::ui::row()
                .items_center()
                .gap(size::S2)
                .child(
                    div()
                        .flex_none()
                        .mono_sm()
                        .text_color(t.ink)
                        .child(proof.name.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .mono_sm()
                        .text_color(t.ink_2)
                        .child(focus_kit::ellipsis(value)),
                )
                .child(
                    div()
                        .flex_none()
                        .ui_xs()
                        .text_color(t.ink_3)
                        .child(proof.taken.replace('T', " ")),
                )
        });
        let machines = (exam_machines > 0).then(|| {
            crate::ui::row()
                .flex_wrap()
                .gap(size::S1)
                .items_center()
                .child(
                    div()
                        .ui_xs()
                        .text_color(t.ink_3)
                        .child(t_("ops-proofs-machine")),
                )
                .children((0..exam_machines).map(|i| {
                    Button::new(("ops-proof-machine", i), (i + 1).to_string())
                        .kind(if self.proof_machine == Some(i) {
                            ButtonKind::Primary
                        } else {
                            ButtonKind::Secondary
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.proof_machine = if this.proof_machine == Some(i) {
                                None
                            } else {
                                Some(i)
                            };
                            cx.notify();
                        }))
                }))
        });
        div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .child(
                crate::ui::row()
                    .items_center()
                    .justify_between()
                    .child(section_label(
                        t,
                        t_args(
                            "ops-proofs-title",
                            &[("count", Arg::from(self.proofs.len()))],
                        ),
                    ))
                    .child(
                        IconButton::new(
                            "ops-proofs-reveal",
                            if shown { EYE_OFF } else { EYE },
                            t_(if shown {
                                "ops-credentials-hide"
                            } else {
                                "ops-credentials-reveal"
                            }),
                        )
                        .dense(true)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.proofs_shown = !this.proofs_shown;
                            cx.notify();
                        })),
                    ),
            )
            .children(rows)
            .children(machines)
            .child(Field::new(&self.proof_inputs.name))
            .child(Field::new(&self.proof_inputs.value))
            .child(
                Button::new("ops-proof-add", t_("ops-proofs-add"))
                    .icon(Icon::PLUS)
                    .on_click(cx.listener(|this, _, window, cx| this.record_proof(window, cx))),
            )
    }
}
