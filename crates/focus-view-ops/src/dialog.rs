use std::path::PathBuf;
use std::sync::Arc;

use focus_core::Vault;
use focus_kit::{ActiveTokens, Button, ButtonKind, Field, Type, floating, size};
use focus_ops::{EngagementKind, NewEngagement, checklist, exam};
use gpui::{
    AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Render, Styled, Subscription, Window, div,
    prelude::FluentBuilder, px,
};
use gpui_component::input::{InputEvent, InputState};

use crate::dashboard::kind_key;
use crate::event::OpsEvent;
use crate::state::{OpsState, error_text};
use crate::ui::{section_label, t_};

const KINDS: [EngagementKind; 6] = [
    EngagementKind::ExternalPentest,
    EngagementKind::InternalPentest,
    EngagementKind::WebPentest,
    EngagementKind::RedTeam,
    EngagementKind::Lab,
    EngagementKind::Exam,
];

pub(crate) fn split_scope(text: &str) -> Vec<String> {
    text.split([',', ';', '\n', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

pub struct NewEngagementDialog {
    state: Entity<OpsState>,
    focus: FocusHandle,
    name: Entity<InputState>,
    client: Entity<InputState>,
    scope: Entity<InputState>,
    vault: Option<Arc<Vault>>,
    note_search: Entity<InputState>,
    note: Option<focus_core::NoteId>,
    generation: u64,
    kind: EngagementKind,
    exam: usize,
    template: Option<usize>,
    error: Option<String>,
    busy: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<OpsEvent> for NewEngagementDialog {}

impl Focusable for NewEngagementDialog {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl NewEngagementDialog {
    pub fn new(
        state: Entity<OpsState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> NewEngagementDialog {
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-new-name"))
        });
        let client = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-new-client"))
        });
        let scope = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-new-scope-placeholder"))
        });
        let enter = cx.subscribe_in(
            &scope,
            window,
            |this: &mut NewEngagementDialog, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.create(cx);
                }
            },
        );
        let enter_name = cx.subscribe_in(
            &name,
            window,
            |this: &mut NewEngagementDialog, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.create(cx);
                }
            },
        );
        let enter_client = cx.subscribe_in(
            &client,
            window,
            |this: &mut NewEngagementDialog, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.create(cx);
                }
            },
        );
        let note_search = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-new-note-search"))
        });
        let typed = cx.subscribe_in(
            &note_search,
            window,
            |_: &mut NewEngagementDialog, _, _: &InputEvent, _, cx| cx.notify(),
        );
        name.update(cx, |input, cx| input.focus(window, cx));
        let exam = state
            .read(cx)
            .settings
            .exam_profile
            .as_deref()
            .and_then(|id| exam::profiles().iter().position(|p| p.id == id))
            .unwrap_or(0);
        NewEngagementDialog {
            state,
            focus: cx.focus_handle(),
            name,
            client,
            scope,
            vault: None,
            note_search,
            note: None,
            generation: focus_i18n::generation(),
            kind: EngagementKind::ExternalPentest,
            exam,
            template: None,
            error: None,
            busy: false,
            _subscriptions: vec![enter, enter_name, enter_client, typed],
        }
    }

    pub fn with_note_templates(mut self, vault: Arc<Vault>) -> NewEngagementDialog {
        self.vault = Some(vault);
        self
    }

    fn candidate_notes(&self, cx: &gpui::App) -> Vec<(focus_core::NoteId, String)> {
        let Some(vault) = &self.vault else {
            return Vec::new();
        };
        let needle = self.note_search.read(cx).value().trim().to_lowercase();
        vault
            .notes()
            .into_iter()
            .filter(|n| n.tasks_open + n.tasks_done > 0)
            .filter(|n| {
                needle.is_empty()
                    || n.title.to_lowercase().contains(&needle)
                    || n.path.to_string_lossy().to_lowercase().contains(&needle)
            })
            .take(6)
            .map(|n| (n.id, n.title.to_string()))
            .collect()
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let name = self.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            self.error = Some(t_("ops-new-name-required"));
            cx.notify();
            return;
        }
        let client = self.client.read(cx).value().trim().to_string();
        let spec = NewEngagement {
            name,
            kind: self.kind,
            client: (!client.is_empty()).then_some(client),
            scope: split_scope(self.scope.read(cx).value().as_ref()),
            exam: (self.kind == EngagementKind::Exam)
                .then(|| exam::profiles()[self.exam].id.to_string()),
            checklist: self
                .template
                .map(|i| checklist::templates()[i].id.to_string()),
        };
        let ops_dir = self.state.read(cx).ops_dir.clone();
        let from_note = self.vault.clone().zip(self.note);
        self.busy = true;
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let made = cx
                .background_spawn(async move {
                    let engagement = focus_ops::create(&ops_dir, &spec)?;
                    if let Some((vault, id)) = from_note {
                        let path = engagement.dir.join("Checklist.md");
                        let text = vault
                            .read(id)
                            .map_err(|e| focus_ops::OpsError::Invalid(e.to_string()))?;
                        std::fs::write(&path, focus_ops::checklist::from_note(&text))
                            .map_err(|source| focus_ops::OpsError::Io { path, source })?;
                    }
                    Ok(engagement)
                })
                .await;
            this.update(cx, |this, cx| {
                this.busy = false;
                match made {
                    Ok(engagement) => {
                        let dir: PathBuf = engagement.dir;
                        this.state
                            .update(cx, |state, cx| state.set_active(dir.clone(), cx));
                        cx.emit(OpsEvent::EngagementCreated(dir));
                        cx.emit(OpsEvent::DialogClosed);
                    }
                    Err(error) => this.error = Some(error_text(&error)),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for NewEngagementDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        if self.generation != focus_i18n::generation() {
            self.generation = focus_i18n::generation();
            self.name.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-new-name"), window, cx)
            });
            self.client.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-new-client"), window, cx)
            });
            self.scope.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-new-scope-placeholder"), window, cx)
            });
            self.note_search.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-new-note-search"), window, cx)
            });
        }
        let notes = self.candidate_notes(cx);
        let option = |id: &'static str,
                      index: usize,
                      label: String,
                      selected: bool,
                      cx: &mut Context<Self>,
                      on: fn(&mut NewEngagementDialog, usize)| {
            Button::new((id, index), label)
                .kind(if selected {
                    ButtonKind::Primary
                } else {
                    ButtonKind::Secondary
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    on(this, index);
                    cx.notify();
                }))
        };
        let field = |label: String, input: &Entity<InputState>| {
            div()
                .flex()
                .flex_col()
                .gap(size::S1)
                .child(section_label(&t, label))
                .child(Field::new(input))
        };
        let kinds =
            crate::ui::row()
                .flex_wrap()
                .gap(size::S2)
                .children(KINDS.iter().enumerate().map(|(i, k)| {
                    option(
                        "ops-new-kind",
                        i,
                        t_(kind_key(*k)),
                        *k == self.kind,
                        cx,
                        |this, i| this.kind = KINDS[i],
                    )
                }));
        let profiles = (self.kind == EngagementKind::Exam).then(|| {
            div()
                .flex()
                .flex_col()
                .gap(size::S1)
                .child(section_label(&t, t_("ops-exam-profile")))
                .child(crate::ui::row().flex_wrap().gap(size::S2).children(
                    exam::profiles().iter().enumerate().map(|(i, p)| {
                        option(
                            "ops-new-exam",
                            i,
                            p.name.to_string(),
                            i == self.exam,
                            cx,
                            |this, i| this.exam = i,
                        )
                    }),
                ))
        });
        let templates = crate::ui::row()
            .flex_wrap()
            .gap(size::S2)
            .child(option(
                "ops-new-template",
                100,
                t_("ops-new-template-default"),
                self.template.is_none(),
                cx,
                |this, _| this.template = None,
            ))
            .children(checklist::templates().iter().enumerate().map(|(i, tpl)| {
                option(
                    "ops-new-template",
                    i,
                    t_(&format!("ops-checklist-{}-label", tpl.id)),
                    self.template == Some(i),
                    cx,
                    |this, i| this.template = Some(i),
                )
            }));

        floating(&t)
            .id("ops-new-engagement")
            .key_context("OpsDialog")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let plain = !event.keystroke.modifiers.modified();
                match event.keystroke.key.as_str() {
                    "escape" => cx.emit(OpsEvent::DialogClosed),
                    "enter" if plain => this.create(cx),
                    _ => {}
                }
            }))
            .on_mouse_down_out(
                cx.listener(|_, _: &gpui::MouseDownEvent, _, cx| cx.emit(OpsEvent::DialogClosed)),
            )
            .flex()
            .flex_col()
            .gap(size::S4)
            .w(px(560.))
            .max_w_full()
            .p(size::S6)
            .rounded(size::RADIUS)
            .child(
                div()
                    .display_sm()
                    .text_color(t.ink)
                    .child(t_("ops-new-engagement")),
            )
            .child(field(t_("ops-new-name"), &self.name))
            .child(
                crate::ui::row()
                    .gap(size::S3)
                    .child(
                        div()
                            .flex_1()
                            .child(field(t_("ops-new-client"), &self.client)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(field(t_("ops-section-scope"), &self.scope)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(size::S1)
                    .child(section_label(&t, t_("ops-new-kind")))
                    .child(kinds),
            )
            .children(profiles)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(size::S1)
                    .child(section_label(&t, t_("ops-new-template")))
                    .child(templates),
            )
            .when(self.vault.is_some(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(size::S1)
                        .child(section_label(&t, t_("ops-checklist-from-note")))
                        .child(Field::new(&self.note_search))
                        .child(div().flex().flex_wrap().gap(size::S2).children(
                            notes.into_iter().map(|(id, title)| {
                                Button::new(("ops-new-note", id as usize), title)
                                    .kind(if self.note == Some(id) {
                                        ButtonKind::Primary
                                    } else {
                                        ButtonKind::Secondary
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.note = if this.note == Some(id) {
                                            None
                                        } else {
                                            Some(id)
                                        };
                                        cx.notify();
                                    }))
                            }),
                        )),
                )
            })
            .children(
                self.error
                    .clone()
                    .map(|e| div().ui_sm().text_color(t.danger).child(e)),
            )
            .child(
                crate::ui::row()
                    .justify_end()
                    .gap(size::S2)
                    .child(
                        Button::new("ops-new-cancel", t_("ops-cancel"))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(OpsEvent::DialogClosed))),
                    )
                    .child(
                        Button::new("ops-new-create", t_("ops-new-create"))
                            .kind(ButtonKind::Primary)
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::split_scope;

    #[test]
    fn scope_entries_split_on_commas_and_spaces() {
        assert_eq!(
            split_scope("10.0.0.0/24, example.com 192.168.1.5;host"),
            ["10.0.0.0/24", "example.com", "192.168.1.5", "host"]
        );
        assert!(split_scope("  ").is_empty());
    }
}
