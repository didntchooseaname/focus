use std::collections::BTreeMap;

use focus_kit::Flow as _;
use focus_kit::{ActiveTokens, Button, ButtonKind, Icon, IconButton, Kbd, Type, size};
use gpui::{
    AnyElement, App, Context, IntoElement, Keystroke, ParentElement, SharedString, Styled, div,
    prelude::FluentBuilder, px,
};

use super::SettingsView;
use super::schema::{Value, ascii_fold};
use crate::actions::{COMMANDS, Command, EDITOR_COMMANDS, FIXED_KEYS, FixedKey};

const KEY: &str = "shortcuts.overrides";
const LOCKED: &str = "ui-cmd-dismiss";

pub struct Group {
    pub label: &'static str,
    pub defaults: Vec<&'static str>,
    pub editor: bool,
}

pub struct Conflict {
    label: &'static str,
    keys: String,
    other: Taken,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taken {
    Command(&'static str),
    Fixed(&'static str),
}

impl Taken {
    fn label(self) -> &'static str {
        match self {
            Taken::Command(label) | Taken::Fixed(label) => label,
        }
    }
}

pub type Overrides = BTreeMap<String, Vec<String>>;

pub fn groups() -> Vec<Group> {
    fn collect(commands: &'static [Command], editor: bool, out: &mut Vec<Group>) {
        for command in commands {
            match out
                .iter_mut()
                .find(|g| g.label == command.label && g.editor == editor)
            {
                Some(group) => {
                    if !command.keys.is_empty() {
                        group.defaults.push(command.keys);
                    }
                }
                None => out.push(Group {
                    label: command.label,
                    defaults: if command.keys.is_empty() {
                        Vec::new()
                    } else {
                        vec![command.keys]
                    },
                    editor,
                }),
            }
        }
    }
    let mut out = Vec::new();
    collect(COMMANDS, false, &mut out);
    collect(EDITOR_COMMANDS, true, &mut out);
    out
}

pub fn parse_overrides(text: &str) -> Overrides {
    serde_json::from_str(text).unwrap_or_default()
}

pub fn effective_keys(group: &Group, overrides: &Overrides) -> Vec<String> {
    overrides
        .get(group.label)
        .cloned()
        .unwrap_or_else(|| group.defaults.iter().map(|k| k.to_string()).collect())
}

pub fn keys_of(keystroke: &Keystroke) -> Option<String> {
    let modifiers = &keystroke.modifiers;
    let function_key = keystroke.key.len() > 1
        && keystroke.key.starts_with('f')
        && keystroke.key[1..].chars().all(|c| c.is_ascii_digit());
    if matches!(
        keystroke.key.as_str(),
        "control" | "shift" | "alt" | "platform" | "function"
    ) {
        return None;
    }
    (modifiers.control || modifiers.alt || modifiers.platform || function_key)
        .then(|| keystroke.unparse())
}

fn conflict_of(groups: &[Group], overrides: &Overrides, label: &str, keys: &str) -> Option<Taken> {
    if let Some(group) = groups
        .iter()
        .filter(|g| g.label != label)
        .find(|g| effective_keys(g, overrides).iter().any(|k| k == keys))
    {
        return Some(Taken::Command(group.label));
    }
    FIXED_KEYS
        .iter()
        .filter(|fixed| fixed.everywhere)
        .find(|fixed| fixed.keys.contains(&keys))
        .map(|fixed| Taken::Fixed(fixed.label))
}

fn fixed_matches(fixed: &FixedKey, query: &str) -> bool {
    query.is_empty()
        || ascii_fold(&focus_i18n::t(fixed.label)).contains(query)
        || fixed
            .keys
            .iter()
            .any(|k| ascii_fold(&focus_i18n::keys_text(k)).contains(query))
}

impl SettingsView {
    fn overrides(&self, cx: &App) -> Overrides {
        parse_overrides(&self.prefs.read(cx).text(KEY))
    }

    fn write_overrides(&mut self, overrides: &Overrides, cx: &mut Context<Self>) {
        let json = serde_json::to_string(overrides).unwrap_or_else(|_| "{}".into());
        self.prefs
            .update(cx, |prefs, cx| prefs.set(KEY, Value::Text(json.into()), cx));
        cx.notify();
    }

    pub(super) fn begin_recording(&mut self, label: &'static str, cx: &mut Context<Self>) {
        self.shortcut_conflict = None;
        self.shortcut_error = None;
        self.recording = Some(label);
        let view = cx.entity().downgrade();
        self.intercept = Some(cx.intercept_keystrokes(move |event, _, cx| {
            cx.stop_propagation();
            let stroke = event.keystroke.clone();
            let _ = view.update(cx, |this, cx| this.record(&stroke, cx));
        }));
        cx.notify();
    }

    fn stop_recording(&mut self, cx: &mut Context<Self>) {
        self.recording = None;
        self.intercept = None;
        cx.notify();
    }

    fn record(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        let Some(label) = self.recording else { return };
        if keystroke.key == "escape" {
            self.stop_recording(cx);
            return;
        }
        let Some(keys) = keys_of(keystroke) else {
            if !matches!(
                keystroke.key.as_str(),
                "control" | "shift" | "alt" | "platform" | "function"
            ) {
                self.shortcut_error =
                    Some(focus_i18n::t("settings-shortcuts-needs-modifier").into());
                cx.notify();
            }
            return;
        };
        self.shortcut_error = None;
        let overrides = self.overrides(cx);
        match conflict_of(&groups(), &overrides, label, &keys) {
            Some(other) => {
                self.shortcut_conflict = Some(Conflict { label, keys, other });
                self.intercept = None;
                self.recording = None;
                cx.notify();
            }
            None => {
                self.assign(label, keys, cx);
            }
        }
    }

    fn assign(&mut self, label: &'static str, keys: String, cx: &mut Context<Self>) {
        let mut overrides = self.overrides(cx);
        overrides.insert(label.to_string(), vec![keys]);
        self.shortcut_conflict = None;
        self.stop_recording(cx);
        self.write_overrides(&overrides, cx);
    }

    fn replace_conflict(&mut self, cx: &mut Context<Self>) {
        let Some(conflict) = self.shortcut_conflict.take() else {
            return;
        };
        let Taken::Command(other) = conflict.other else {
            return;
        };
        let mut overrides = self.overrides(cx);
        if let Some(group) = groups().into_iter().find(|g| g.label == other) {
            let remaining: Vec<String> = effective_keys(&group, &overrides)
                .into_iter()
                .filter(|k| *k != conflict.keys)
                .collect();
            overrides.insert(other.to_string(), remaining);
        }
        overrides.insert(conflict.label.to_string(), vec![conflict.keys]);
        self.write_overrides(&overrides, cx);
    }

    fn reset_shortcut(&mut self, label: &str, cx: &mut Context<Self>) {
        let mut overrides = self.overrides(cx);
        if overrides.remove(label).is_some() {
            self.write_overrides(&overrides, cx);
        }
    }

    fn reset_all_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.shortcut_conflict = None;
        self.write_overrides(&Overrides::new(), cx);
    }

    pub(super) fn render_shortcuts(&self, query: &str, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        let overrides = self.overrides(cx);
        let all = groups();
        let rows: Vec<AnyElement> = all
            .iter()
            .filter(|group| {
                let keys: Vec<String> = effective_keys(group, &overrides)
                    .iter()
                    .map(|k| focus_i18n::keys_text(k))
                    .collect();
                query.is_empty()
                    || ascii_fold(&focus_i18n::t(group.label)).contains(query)
                    || keys.iter().any(|k| ascii_fold(k).contains(query))
            })
            .map(|group| {
                let label = group.label;
                let keys = effective_keys(group, &overrides);
                let changed = overrides.contains_key(label);
                let recording = self.recording == Some(label);
                let conflict = self.shortcut_conflict.as_ref().filter(|c| c.label == label);
                let message = if recording {
                    self.shortcut_error.clone()
                } else {
                    None
                };
                let row = div()
                    .flow_row()
                    .items_center()
                    .gap(size::S3)
                    .min_h(px(36.))
                    .child(
                        div()
                            .flex_1()
                            .ui()
                            .text_color(t.ink)
                            .child(focus_i18n::t(label)),
                    )
                    .child(if recording {
                        div()
                            .ui_sm()
                            .text_color(t.ink_2)
                            .child(focus_i18n::t("settings-shortcuts-press"))
                            .into_any_element()
                    } else if keys.is_empty() {
                        div()
                            .ui_sm()
                            .text_color(t.ink_3)
                            .child(focus_i18n::t("settings-shortcuts-none"))
                            .into_any_element()
                    } else {
                        div()
                            .flow_row()
                            .gap(size::S1)
                            .children(keys.iter().map(|k| Kbd::new(focus_i18n::keys_text(k))))
                            .into_any_element()
                    })
                    .child(if recording {
                        Button::new(
                            SharedString::from(format!("shortcut-cancel-{label}")),
                            focus_i18n::t("settings-ai-actions-cancel"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.stop_recording(cx)))
                        .into_any_element()
                    } else {
                        IconButton::new(
                            SharedString::from(format!("shortcut-edit-{label}")),
                            Icon::PENCIL,
                            focus_i18n::t("settings-shortcuts-edit"),
                        )
                        .dense(true)
                        .disabled(label == LOCKED)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.begin_recording(label, cx)),
                        )
                        .into_any_element()
                    })
                    .child(div().flex_none().w(px(28.)).child(if changed {
                        IconButton::new(
                            SharedString::from(format!("shortcut-reset-{label}")),
                            Icon::RESET,
                            focus_i18n::t("settings-reset"),
                        )
                        .dense(true)
                        .on_click(cx.listener(move |this, _, _, cx| this.reset_shortcut(label, cx)))
                        .into_any_element()
                    } else {
                        div().into_any_element()
                    }));
                div()
                    .flex()
                    .flex_col()
                    .border_b_1()
                    .border_color(t.line)
                    .child(row)
                    .when_some(message, |this, message| {
                        this.child(
                            div()
                                .pb(size::S2)
                                .ui_sm()
                                .text_color(t.danger)
                                .child(message),
                        )
                    })
                    .when_some(conflict, |this, conflict| {
                        let other = focus_i18n::t(conflict.other.label());
                        let fixed = matches!(conflict.other, Taken::Fixed(_));
                        let keys = focus_i18n::keys_text(&conflict.keys);
                        this.child(
                            div()
                                .flow_row()
                                .items_center()
                                .gap(size::S3)
                                .pb(size::S2)
                                .child(div().flex_1().ui_sm().text_color(t.danger).child(
                                    focus_i18n::t_args(
                                        if fixed {
                                            "settings-shortcuts-fixed-conflict"
                                        } else {
                                            "settings-shortcuts-conflict"
                                        },
                                        &[
                                            ("keys", keys.as_str().into()),
                                            ("command", other.as_str().into()),
                                        ],
                                    ),
                                ))
                                .when(!fixed, |row| {
                                    row.child(
                                        Button::new(
                                            "shortcut-replace",
                                            focus_i18n::t("settings-shortcuts-replace"),
                                        )
                                        .kind(ButtonKind::Danger)
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.replace_conflict(cx)),
                                        ),
                                    )
                                })
                                .child(
                                    Button::new(
                                        "shortcut-keep",
                                        focus_i18n::t("settings-ai-actions-cancel"),
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.shortcut_conflict = None;
                                            cx.notify();
                                        },
                                    )),
                                ),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .child(
                div().flow_row().justify_end().pb(size::S2).child(
                    Button::new(
                        "shortcuts-reset-all",
                        focus_i18n::t("settings-shortcuts-reset-all"),
                    )
                    .kind(ButtonKind::Ghost)
                    .icon(Icon::RESET)
                    .disabled(overrides.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.reset_all_shortcuts(cx))),
                ),
            )
            .children(rows)
            .children(self.render_fixed_keys(query, cx))
            .into_any_element()
    }

    fn render_fixed_keys(&self, query: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = *cx.tokens();
        let shown: Vec<&FixedKey> = FIXED_KEYS
            .iter()
            .filter(|fixed| fixed_matches(fixed, query))
            .collect();
        if shown.is_empty() {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .pt(size::S6)
                        .pb(size::S2)
                        .border_b_1()
                        .border_color(t.line)
                        .ui_xs()
                        .text_color(t.ink_3)
                        .child(focus_i18n::t("settings-shortcuts-fixed")),
                )
                .children(shown.into_iter().map(|fixed| {
                    div()
                        .flow_row()
                        .items_center()
                        .gap(size::S3)
                        .min_h(px(36.))
                        .border_b_1()
                        .border_color(t.line)
                        .child(
                            div()
                                .flex_1()
                                .ui()
                                .text_color(t.ink)
                                .child(focus_i18n::t(fixed.label)),
                        )
                        .child(
                            div().flow_row().gap(size::S1).children(
                                fixed
                                    .keys
                                    .iter()
                                    .map(|k| Kbd::new(focus_i18n::keys_text(k))),
                            ),
                        )
                        .child(div().flex_none().w(px(28.)))
                        .child(div().flex_none().w(px(28.)))
                }))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    fn stroke(key: &str, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.into(),
            key_char: None,
        }
    }

    #[test]
    fn a_key_without_modifier_is_refused_except_function_keys() {
        assert_eq!(
            keys_of(&stroke("p", Modifiers::control())),
            Some("ctrl-p".into())
        );
        assert_eq!(keys_of(&stroke("p", Modifiers::none())), None);
        assert_eq!(keys_of(&stroke("f5", Modifiers::none())), Some("f5".into()));
        assert_eq!(keys_of(&stroke("control", Modifiers::control())), None);
        assert_eq!(keys_of(&stroke("p", Modifiers::shift())), None);
    }

    #[test]
    fn a_key_used_anywhere_is_a_conflict() {
        let all = groups();
        let overrides = Overrides::new();
        assert_eq!(
            conflict_of(&all, &overrides, "ui-cmd-toggle-right", "ctrl-shift-b"),
            Some(Taken::Command("ui-cmd-toggle-left"))
        );
        assert_eq!(
            conflict_of(&all, &overrides, "ui-cmd-toggle-right", "ctrl-alt-q"),
            None
        );
        assert_eq!(
            conflict_of(&all, &overrides, "ui-cmd-bold", "ctrl-k"),
            Some(Taken::Command("ui-cmd-open-note")),
            "an editor key shadows a window key"
        );
        assert_eq!(
            conflict_of(&all, &overrides, "ui-cmd-toggle-left", "ctrl-b"),
            Some(Taken::Command("ui-cmd-bold")),
            "a window key shadowed by an editor key"
        );
        assert_eq!(
            conflict_of(&all, &overrides, "ui-cmd-toggle-left", "ctrl-f"),
            Some(Taken::Command("ui-cmd-find"))
        );
        assert_eq!(
            conflict_of(&all, &overrides, "ui-cmd-toggle-left", "ctrl-z"),
            Some(Taken::Fixed("ui-fixed-undo"))
        );
        assert_eq!(
            conflict_of(&all, &overrides, "ui-cmd-toggle-left", "ctrl-d"),
            None,
            "a key of the tree alone stays free"
        );
    }

    #[test]
    fn fixed_keys_are_found_by_the_search() {
        let undo = FIXED_KEYS
            .iter()
            .find(|f| f.label == "ui-fixed-undo")
            .expect("listed");
        assert!(fixed_matches(undo, ""));
        assert!(fixed_matches(undo, "ctrl z"));
        assert!(!fixed_matches(undo, "ctrl q"));
    }

    #[test]
    fn changes_replace_the_defaults_of_one_command() {
        let all = groups();
        let open = all
            .iter()
            .find(|g| g.label == "ui-cmd-open-note")
            .expect("listed");
        assert_eq!(
            effective_keys(open, &Overrides::new()),
            ["ctrl-k", "ctrl-p", "ctrl-o"]
        );
        let mut overrides = Overrides::new();
        overrides.insert("ui-cmd-open-note".into(), vec!["ctrl-alt-o".into()]);
        assert_eq!(effective_keys(open, &overrides), ["ctrl-alt-o"]);
        assert_eq!(
            conflict_of(&all, &overrides, "ui-cmd-back", "ctrl-p"),
            None,
            "a key given up is free"
        );
        overrides.insert("ui-cmd-open-note".into(), Vec::new());
        assert!(effective_keys(open, &overrides).is_empty());
    }

    #[test]
    fn malformed_changes_are_ignored() {
        assert!(parse_overrides("not json").is_empty());
        assert!(parse_overrides("{\"a\":[\"x\"]}").contains_key("a"));
    }
}
