use std::path::PathBuf;

use focus_ai::prompts::{self, Output, Scope, Template};
use focus_kit::Flow as _;
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Checkbox, Field, Icon, IconButton, Segment, Segments,
    TextArea, Type, size,
};
use gpui::{
    AnyElement, AppContext, Context, Entity, IntoElement, ParentElement, SharedString, Styled,
    Window, div, prelude::FluentBuilder, px,
};
use gpui_component::input::InputState;

use super::SettingsView;
use super::schema::Value;

const KEY: &str = "ai.actions_json";
const SCOPES: [Scope; 2] = [Scope::Selection, Scope::Note];
const OUTPUTS: [Output; 4] = [Output::Chat, Output::Replace, Output::Insert, Output::Note];
const VARIABLES: &str =
    "{{selection}}, {{note}}, {{title}}, {{path}}, {{tags}}, {{date}}, {{language}}";

pub struct Draft {
    id: Option<String>,
    builtin: bool,
    enabled: bool,
    label: Entity<InputState>,
    description: Entity<InputState>,
    prompt: Entity<InputState>,
    model: Entity<InputState>,
    scope: usize,
    output: usize,
    ops_only: bool,
    error: Option<SharedString>,
}

fn shown(text: &str) -> String {
    if text.starts_with("ai-action-") {
        focus_i18n::t(text)
    } else {
        text.to_string()
    }
}

fn new_id(label: &str, taken: &[String]) -> String {
    let mut slug: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        slug = "action".into();
    }
    let mut candidate = slug.clone();
    let mut n = 2;
    while taken.contains(&candidate) {
        candidate = format!("{slug}-{n}");
        n += 1;
    }
    candidate
}

fn valid_for_import(template: &Template) -> bool {
    !template.id.trim().is_empty()
        && template
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !template.label.trim().is_empty()
        && !template.prompt.trim().is_empty()
}

pub fn parse_actions(text: &str) -> Option<(Vec<Template>, usize)> {
    let json: serde_json::Value = serde_json::from_str(text).ok()?;
    let items = match json {
        serde_json::Value::Array(items) => items,
        object @ serde_json::Value::Object(_) => vec![object],
        _ => return None,
    };
    let total = items.len();
    let builtin_ids: Vec<String> = prompts::builtin().into_iter().map(|t| t.id).collect();
    let accepted: Vec<Template> = items
        .into_iter()
        .filter_map(|item| serde_json::from_value::<Template>(item).ok())
        .filter(valid_for_import)
        .map(|mut template| {
            template.builtin = builtin_ids.contains(&template.id);
            template
        })
        .collect();
    let refused = total - accepted.len();
    (!accepted.is_empty()).then_some((accepted, refused))
}

impl SettingsView {
    fn saved_actions(&self, cx: &gpui::App) -> Vec<Template> {
        serde_json::from_str(&self.prefs.read(cx).text(KEY)).unwrap_or_default()
    }

    fn write_actions(&mut self, saved: &[Template], cx: &mut Context<Self>) {
        let json = serde_json::to_string(saved).unwrap_or_else(|_| "[]".into());
        self.prefs
            .update(cx, |prefs, cx| prefs.set(KEY, Value::Text(json.into()), cx));
        cx.notify();
    }

    fn upsert_action(&mut self, template: Template, cx: &mut Context<Self>) {
        let mut saved = self.saved_actions(cx);
        saved.retain(|t| t.id != template.id);
        if prompts::builtin()
            .iter()
            .all(|original| *original != template)
        {
            saved.push(template);
        }
        self.write_actions(&saved, cx);
    }

    pub(super) fn open_draft(
        &mut self,
        template: Option<&Template>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = |value: String,
                    placeholder: String,
                    window: &mut Window,
                    cx: &mut Context<Self>|
         -> Entity<InputState> {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
                    .validate(super::single_line)
            })
        };
        let label = text(
            template.map(|t| shown(&t.label)).unwrap_or_default(),
            String::new(),
            window,
            cx,
        );
        let description = text(
            template.map(|t| shown(&t.description)).unwrap_or_default(),
            String::new(),
            window,
            cx,
        );
        let model = text(
            template.and_then(|t| t.model.clone()).unwrap_or_default(),
            focus_i18n::t("settings-ai-actions-model-placeholder"),
            window,
            cx,
        );
        let prompt = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(4, 10)
                .default_value(template.map(|t| t.prompt.clone()).unwrap_or_default())
        });
        self.actions_draft = Some(Draft {
            id: template.map(|t| t.id.clone()),
            builtin: template.is_some_and(|t| t.builtin),
            enabled: template.is_none_or(|t| t.enabled),
            label,
            description,
            prompt,
            model,
            scope: template
                .and_then(|t| SCOPES.iter().position(|s| *s == t.scope))
                .unwrap_or(0),
            output: template
                .and_then(|t| OUTPUTS.iter().position(|o| *o == t.output))
                .unwrap_or(0),
            ops_only: template.is_some_and(|t| t.ops_only),
            error: None,
        });
        cx.notify();
    }

    fn save_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.actions_draft.as_mut() else {
            return;
        };
        let label = draft.label.read(cx).value().trim().to_string();
        let prompt = draft.prompt.read(cx).value().trim().to_string();
        if label.is_empty() {
            draft.error = Some(focus_i18n::t("settings-ai-actions-error-label").into());
            cx.notify();
            return;
        }
        if prompt.is_empty() {
            draft.error = Some(focus_i18n::t("settings-ai-actions-error-prompt").into());
            cx.notify();
            return;
        }
        let description = draft.description.read(cx).value().trim().to_string();
        let model = draft.model.read(cx).value().trim().to_string();
        let (id, builtin, enabled, scope, output, ops_only) = (
            draft.id.clone(),
            draft.builtin,
            draft.enabled,
            SCOPES[draft.scope],
            OUTPUTS[draft.output],
            draft.ops_only,
        );
        let taken: Vec<String> = prompts::merged(&self.saved_actions(cx))
            .into_iter()
            .map(|t| t.id)
            .collect();
        let id = id.unwrap_or_else(|| new_id(&label, &taken));
        let original = prompts::builtin().into_iter().find(|t| t.id == id);
        let keep = |text: String, key: Option<&String>| match key {
            Some(key) if shown(key) == text => key.clone(),
            _ => text,
        };
        let template = Template {
            label: keep(label, original.as_ref().map(|o| &o.label)),
            description: keep(description, original.as_ref().map(|o| &o.description)),
            prompt,
            scope,
            output,
            model: (!model.is_empty()).then_some(model),
            ops_only,
            enabled,
            builtin,
            id,
        };
        self.actions_draft = None;
        self.upsert_action(template, cx);
    }

    fn duplicate_action(&mut self, template: &Template, cx: &mut Context<Self>) {
        let taken: Vec<String> = prompts::merged(&self.saved_actions(cx))
            .into_iter()
            .map(|t| t.id)
            .collect();
        let label = format!(
            "{} {}",
            shown(&template.label),
            focus_i18n::t("settings-ai-actions-copy-suffix")
        );
        let copy = Template {
            id: new_id(&label, &taken),
            label,
            description: shown(&template.description),
            builtin: false,
            enabled: true,
            ..template.clone()
        };
        self.upsert_action(copy, cx);
    }

    fn toggle_action(&mut self, template: &Template, cx: &mut Context<Self>) {
        self.upsert_action(
            Template {
                enabled: !template.enabled,
                ..template.clone()
            },
            cx,
        );
    }

    fn delete_or_restore_action(&mut self, id: &str, cx: &mut Context<Self>) {
        let mut saved = self.saved_actions(cx);
        saved.retain(|t| t.id != id);
        self.write_actions(&saved, cx);
    }

    pub(super) fn export_actions(&mut self, cx: &mut Context<Self>) {
        let saved = self.saved_actions(cx);
        let home = focus_core::settings::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let chosen = cx.prompt_for_new_path(&home, Some("focus-actions.json"));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            let written = cx
                .background_spawn(async move {
                    let text =
                        serde_json::to_string_pretty(&saved).map_err(std::io::Error::other)?;
                    std::fs::write(&path, text + "\n").map(|()| path)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.notice = Some(match written {
                    Ok(path) => (
                        false,
                        focus_i18n::t_args(
                            "settings-ai-actions-exported",
                            &[("path", path.display().to_string().as_str().into())],
                        )
                        .into(),
                    ),
                    Err(error) => {
                        eprintln!("focus: exporting the actions: {error}");
                        (
                            true,
                            focus_i18n::t("settings-ai-actions-export-failed").into(),
                        )
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn import_actions(&mut self, cx: &mut Context<Self>) {
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
            let read = cx
                .background_spawn(async move {
                    std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|text| parse_actions(&text))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.notice = Some(match read {
                    Some((templates, refused)) => {
                        let count = templates.len();
                        for template in templates {
                            this.upsert_action(template, cx);
                        }
                        (
                            false,
                            focus_i18n::t_args(
                                "settings-ai-actions-imported",
                                &[("count", count.into()), ("refused", refused.into())],
                            )
                            .into(),
                        )
                    }
                    None => (
                        true,
                        focus_i18n::t("settings-ai-actions-import-failed").into(),
                    ),
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        let saved = self.saved_actions(cx);
        let mut all = prompts::merged(&saved);
        all.sort_by_key(|template| template.builtin);
        let form = self.render_draft(cx);
        let rows = all.into_iter().enumerate().map(|(index, template)| {
            let modified = saved.iter().any(|s| s.id == template.id);
            let builtin = template.builtin;
            let id = template.id.clone();
            let toggle = template.clone();
            let edit = template.clone();
            let copy = template.clone();
            let enabled = template.enabled;
            let scope = focus_i18n::t(match template.scope {
                Scope::Selection => "settings-ai-actions-scope-selection",
                Scope::Note => "settings-ai-actions-scope-note",
            });
            let output = focus_i18n::t(match template.output {
                Output::Chat => "settings-ai-actions-output-chat",
                Output::Replace => "settings-ai-actions-output-replace",
                Output::Insert => "settings-ai-actions-output-insert",
                Output::Note => "settings-ai-actions-output-note",
            });
            div()
                .flow_row()
                .items_center()
                .gap(size::S3)
                .min_h(px(40.))
                .border_b_1()
                .border_color(t.line)
                .child(
                    Checkbox::new(("action-enabled", index), enabled).on_click(
                        cx.listener(move |this, _, _, cx| this.toggle_action(&toggle, cx)),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .py(size::S2)
                        .child(
                            div()
                                .ui()
                                .text_color(if enabled { t.ink } else { t.ink_3 })
                                .child(shown(&template.label)),
                        )
                        .child(div().ui_sm().text_color(t.ink_3).child({
                            let mut parts = vec![scope, output];
                            if builtin {
                                parts.push(focus_i18n::t("settings-ai-actions-builtin"));
                            }
                            if builtin && modified {
                                parts.push(focus_i18n::t("settings-ai-actions-modified"));
                            }
                            if template.ops_only {
                                parts.push(focus_i18n::t("settings-ai-actions-ops"));
                            }
                            parts.join(&focus_i18n::t("ui-list-separator"))
                        })),
                )
                .child(
                    IconButton::new(
                        ("action-edit", index),
                        Icon::PENCIL,
                        focus_i18n::t("settings-ai-actions-edit"),
                    )
                    .dense(true)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_draft(Some(&edit), window, cx)
                    })),
                )
                .child(
                    IconButton::new(
                        ("action-copy", index),
                        Icon::COPY,
                        focus_i18n::t("settings-ai-actions-duplicate"),
                    )
                    .dense(true)
                    .on_click(cx.listener(move |this, _, _, cx| this.duplicate_action(&copy, cx))),
                )
                .child(if builtin {
                    IconButton::new(
                        ("action-restore", index),
                        Icon::RESET,
                        focus_i18n::t("settings-ai-actions-restore"),
                    )
                    .dense(true)
                    .disabled(!modified)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.delete_or_restore_action(&id, cx)),
                    )
                } else {
                    IconButton::new(
                        ("action-delete", index),
                        Icon::TRASH,
                        focus_i18n::t("settings-ai-actions-delete"),
                    )
                    .dense(true)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.delete_or_restore_action(&id, cx)),
                    )
                })
        });
        let toolbar = div()
            .flow_row()
            .gap(size::S2)
            .child(
                Button::new("actions-add", focus_i18n::t("settings-ai-actions-add"))
                    .icon(Icon::PLUS)
                    .on_click(cx.listener(|this, _, window, cx| this.open_draft(None, window, cx))),
            )
            .child(
                Button::new(
                    "actions-import",
                    focus_i18n::t("settings-ai-actions-import"),
                )
                .on_click(cx.listener(|this, _, _, cx| this.import_actions(cx))),
            )
            .child(
                Button::new(
                    "actions-export",
                    focus_i18n::t("settings-ai-actions-export"),
                )
                .on_click(cx.listener(|this, _, _, cx| this.export_actions(cx))),
            );
        div()
            .flex()
            .flex_col()
            .gap(size::S3)
            .child(toolbar)
            .when_some(form, |this, form| this.child(form))
            .child(div().flex().flex_col().children(rows))
            .into_any_element()
    }

    fn render_draft(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let draft = self.actions_draft.as_ref()?;
        let t = *cx.tokens();
        let scope = Segments::new(
            "action-scope",
            SCOPES
                .iter()
                .map(|s| {
                    Segment::new(focus_i18n::t(if *s == Scope::Selection {
                        "settings-ai-actions-scope-selection"
                    } else {
                        "settings-ai-actions-scope-note"
                    }))
                })
                .collect(),
            draft.scope,
        )
        .on_select({
            let view = cx.entity();
            move |index, _, cx| view.update(cx, |this, cx| this.set_draft_scope(index, cx))
        });
        let output = Segments::new(
            "action-output",
            [
                "settings-ai-actions-output-chat",
                "settings-ai-actions-output-replace",
                "settings-ai-actions-output-insert",
                "settings-ai-actions-output-note",
            ]
            .into_iter()
            .map(|key| Segment::new(focus_i18n::t(key)))
            .collect(),
            draft.output,
        )
        .on_select({
            let view = cx.entity();
            move |index, _, cx| view.update(cx, |this, cx| this.set_draft_output(index, cx))
        });
        let ops_only = draft.ops_only;
        Some(
            div()
                .flex()
                .flex_col()
                .gap(size::S3)
                .p(size::S4)
                .border_1()
                .border_color(t.line_2)
                .child(super::field_row(
                    &t,
                    focus_i18n::t("settings-ai-actions-field-label"),
                    Field::new(&draft.label),
                ))
                .child(super::field_row(
                    &t,
                    focus_i18n::t("settings-ai-actions-field-description"),
                    Field::new(&draft.description),
                ))
                .child(super::field_row(
                    &t,
                    focus_i18n::t("settings-ai-actions-field-prompt"),
                    TextArea::new(&draft.prompt),
                ))
                .child(div().ui_sm().text_color(t.ink_3).child(focus_i18n::t_args(
                    "settings-ai-actions-prompt-help",
                    &[("variables", VARIABLES.into())],
                )))
                .child(super::field_row(
                    &t,
                    focus_i18n::t("settings-ai-actions-field-scope"),
                    scope,
                ))
                .child(super::field_row(
                    &t,
                    focus_i18n::t("settings-ai-actions-field-output"),
                    output,
                ))
                .child(super::field_row(
                    &t,
                    focus_i18n::t("settings-ai-actions-field-model"),
                    Field::new(&draft.model),
                ))
                .child(
                    div()
                        .flow_row()
                        .items_center()
                        .gap(size::S2)
                        .child(
                            Checkbox::new("action-ops-only", ops_only).on_click(cx.listener(
                                |this, _, _, cx| {
                                    if let Some(draft) = this.actions_draft.as_mut() {
                                        draft.ops_only = !draft.ops_only;
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            div()
                                .ui()
                                .text_color(t.ink)
                                .child(focus_i18n::t("settings-ai-actions-field-ops")),
                        ),
                )
                .when_some(draft.error.clone(), |this, message| {
                    this.child(div().ui_sm().text_color(t.danger).child(message))
                })
                .child(
                    div()
                        .flow_row()
                        .gap(size::S2)
                        .child(
                            Button::new("action-save", focus_i18n::t("settings-ai-actions-save"))
                                .kind(ButtonKind::Primary)
                                .on_click(cx.listener(|this, _, _, cx| this.save_draft(cx))),
                        )
                        .child(
                            Button::new(
                                "action-cancel",
                                focus_i18n::t("settings-ai-actions-cancel"),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.actions_draft = None;
                                cx.notify();
                            })),
                        ),
                )
                .into_any_element(),
        )
    }

    fn set_draft_scope(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(draft) = self.actions_draft.as_mut() {
            draft.scope = index;
        }
        cx.notify();
    }

    fn set_draft_output(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(draft) = self.actions_draft.as_mut() {
            draft.output = index;
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_slugs_and_unique() {
        assert_eq!(new_id("Résumé en 3 points!", &[]), "r-sum-en-3-points");
        assert_eq!(
            new_id("Fix it", &["fix-it".into(), "fix-it-2".into()]),
            "fix-it-3"
        );
        assert_eq!(new_id("???", &[]), "action");
    }

    #[test]
    fn an_import_keeps_valid_actions_and_counts_the_others() {
        let text = r#"[
            {"id":"mine","label":"Mine","prompt":"Do {{selection}}","scope":"selection","output":"chat","model":null,"ops_only":false,"enabled":true,"builtin":true},
            {"id":"","label":"No id","prompt":"x","scope":"note","output":"chat","model":null,"ops_only":false,"enabled":true,"builtin":false},
            {"id":"summarize","label":"Mine too","prompt":"p","scope":"note","output":"note","model":null,"ops_only":true,"enabled":false,"builtin":false},
            {"nonsense": 1}
        ]"#;
        let (accepted, refused) = parse_actions(text).expect("two actions are valid");
        assert_eq!((accepted.len(), refused), (2, 2));
        assert!(
            !accepted[0].builtin,
            "the flag follows the identifier, not the file"
        );
        assert!(
            accepted[1].builtin,
            "an action that has a built-in identifier replaces it"
        );
        assert!(parse_actions("[]").is_none());
        assert!(parse_actions("not json").is_none());
    }

    #[test]
    fn one_action_object_is_accepted() {
        let text = r#"{"id":"one","label":"One","prompt":"p","scope":"selection","output":"insert","model":"deepseek-v4-pro","ops_only":false,"enabled":true,"builtin":false}"#;
        assert_eq!(parse_actions(text).map(|(a, r)| (a.len(), r)), Some((1, 0)));
    }
}
