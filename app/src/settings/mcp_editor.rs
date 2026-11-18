use std::collections::BTreeMap;

use focus_ai::mcp::{McpError, Server, ServerConfig};
use focus_kit::Flow as _;
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Checkbox, Field, Icon, IconButton, TextArea, Type, ellipsis,
    size,
};
use gpui::{
    AnyElement, AppContext, Context, Entity, IntoElement, ParentElement, SharedString, Styled,
    Window, div, prelude::FluentBuilder, px,
};
use gpui_component::input::InputState;

use super::SettingsView;
use super::schema::Value;

const KEY: &str = "ai.mcp_json";

#[derive(Clone)]
pub enum Check {
    Running,
    Tools(Vec<String>),
    Failed(String),
}

pub struct ServerDraft {
    index: Option<usize>,
    enabled: bool,
    name: Entity<InputState>,
    command: Entity<InputState>,
    args: Entity<InputState>,
    env: Entity<InputState>,
    allowed: Entity<InputState>,
    error: Option<SharedString>,
}

pub fn split_arguments(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, c) => current.push(c),
        }
    }
    if started || !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn join_arguments(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.is_empty() || a.chars().any(char::is_whitespace) {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_environment(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

fn names(text: &str) -> Vec<String> {
    text.split([',', ' ', '\n'])
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .collect()
}

fn error_text(error: &McpError) -> String {
    match error {
        McpError::Spawn { command, .. } => focus_i18n::t_args(
            "settings-mcp-error-spawn",
            &[("command", command.as_str().into())],
        ),
        McpError::Timeout { .. } => focus_i18n::t("settings-mcp-error-timeout"),
        McpError::Exited { .. } => focus_i18n::t("settings-mcp-error-exited"),
        McpError::InvalidConfig { .. } => focus_i18n::t("settings-mcp-error-config"),
        McpError::Protocol { .. } | McpError::Cancelled => {
            focus_i18n::t("settings-mcp-error-protocol")
        }
    }
}

fn list_tools(config: &ServerConfig) -> Result<Vec<String>, McpError> {
    let open = ServerConfig {
        allowed_tools: Vec::new(),
        enabled: true,
        ..config.clone()
    };
    let server = Server::start(&open)?;
    let tools = server
        .tools()
        .iter()
        .filter_map(|spec| server.raw_name(&spec.name).map(str::to_string))
        .collect();
    server.stop();
    Ok(tools)
}

#[derive(Debug, PartialEq)]
pub struct EnvPlan {
    pub server: String,
    pub store: Vec<(String, String)>,
    pub move_from: Option<(String, Vec<String>)>,
    pub delete: Vec<(String, String)>,
}

impl EnvPlan {
    pub fn needs_keyring(&self) -> bool {
        !self.store.is_empty()
            || self
                .move_from
                .as_ref()
                .is_some_and(|(_, names)| !names.is_empty())
    }
}

pub fn plan_env(
    old: Option<&ServerConfig>,
    new_name: &str,
    lines: BTreeMap<String, String>,
) -> (EnvPlan, BTreeMap<String, String>) {
    let mut plan = EnvPlan {
        server: new_name.to_string(),
        store: Vec::new(),
        move_from: None,
        delete: Vec::new(),
    };
    let mut env = BTreeMap::new();
    let renamed_from = old.filter(|o| o.name != new_name).map(|o| o.name.clone());
    let mut moved = Vec::new();
    for (name, value) in &lines {
        if !value.is_empty() {
            plan.store.push((name.clone(), value.clone()));
            env.insert(name.clone(), String::new());
        } else {
            env.insert(
                name.clone(),
                old.and_then(|o| o.env.get(name))
                    .cloned()
                    .unwrap_or_default(),
            );
            if renamed_from.is_some() && old.is_some_and(|o| o.env.contains_key(name)) {
                moved.push(name.clone());
            }
        }
    }
    if let Some(from) = renamed_from {
        plan.move_from = Some((from, moved));
    }
    if let Some(old) = old {
        let owner = &old.name;
        plan.delete = old
            .env
            .keys()
            .filter(|n| !lines.contains_key(*n))
            .map(|n| (owner.clone(), n.clone()))
            .collect();
        if old.name != new_name {
            plan.delete.extend(
                lines
                    .keys()
                    .filter(|n| old.env.contains_key(*n))
                    .map(|n| (owner.clone(), n.clone())),
            );
        }
    }
    (plan, env)
}

fn apply_plan(plan: &EnvPlan) -> Result<(), &'static str> {
    use focus_ai::keystore::server_env;
    if plan.needs_keyring() && !server_env::available() {
        return Err("settings-mcp-error-keyring");
    }
    if let Some((from, names)) = &plan.move_from {
        for name in names {
            if let Some(value) = server_env::load(from, name) {
                server_env::store(&plan.server, name, &value)
                    .map_err(|_| "settings-mcp-error-keyring")?;
            }
        }
    }
    for (name, value) in &plan.store {
        server_env::store(&plan.server, name, value).map_err(|_| "settings-mcp-error-keyring")?;
    }
    for (server, name) in &plan.delete {
        server_env::delete(server, name);
    }
    Ok(())
}

impl SettingsView {
    pub(super) fn servers(&self, cx: &gpui::App) -> Vec<ServerConfig> {
        serde_json::from_str(&self.prefs.read(cx).text(KEY)).unwrap_or_default()
    }

    fn write_servers(&mut self, servers: &[ServerConfig], cx: &mut Context<Self>) {
        let json = serde_json::to_string(servers).unwrap_or_else(|_| "[]".into());
        self.prefs
            .update(cx, |prefs, cx| prefs.set(KEY, Value::Text(json.into()), cx));
        cx.notify();
    }

    pub(super) fn open_server_draft(
        &mut self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = index.and_then(|i| self.servers(cx).get(i).cloned());
        let single =
            |value: String, placeholder: String, window: &mut Window, cx: &mut Context<Self>| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(placeholder)
                        .default_value(value)
                        .validate(super::single_line)
                })
            };
        let name = single(
            existing
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_default(),
            String::new(),
            window,
            cx,
        );
        let command = single(
            existing
                .as_ref()
                .map(|s| s.command.clone())
                .unwrap_or_default(),
            focus_i18n::t("settings-mcp-command-placeholder"),
            window,
            cx,
        );
        let args = single(
            existing
                .as_ref()
                .map(|s| join_arguments(&s.args))
                .unwrap_or_default(),
            String::new(),
            window,
            cx,
        );
        let allowed = single(
            existing
                .as_ref()
                .map(|s| s.allowed_tools.join(", "))
                .unwrap_or_default(),
            focus_i18n::t("settings-mcp-allowed-placeholder"),
            window,
            cx,
        );
        let env_text = existing
            .as_ref()
            .map(|s| {
                s.env
                    .keys()
                    .map(|k| format!("{k}="))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        let env = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(2, 6)
                .default_value(env_text)
        });
        self.server_draft = Some(ServerDraft {
            index,
            enabled: existing.as_ref().is_none_or(|s| s.enabled),
            name,
            command,
            args,
            env,
            allowed,
            error: None,
        });
        cx.notify();
    }

    fn save_server_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.server_draft.as_ref() else {
            return;
        };
        let name = draft.name.read(cx).value().trim().to_string();
        let command = draft.command.read(cx).value().trim().to_string();
        let servers = self.servers(cx);
        let refused = if !name.chars().any(|c| c.is_alphanumeric()) {
            Some("settings-mcp-error-name")
        } else if servers
            .iter()
            .enumerate()
            .any(|(i, s)| s.name == name && Some(i) != draft.index)
        {
            Some("settings-mcp-error-duplicate")
        } else if command.is_empty() {
            Some("settings-mcp-error-command")
        } else {
            None
        };
        if let Some(key) = refused {
            if let Some(draft) = self.server_draft.as_mut() {
                draft.error = Some(focus_i18n::t(key).into());
            }
            cx.notify();
            return;
        }
        let old = draft.index.and_then(|i| servers.get(i)).cloned();
        let (plan, env) = plan_env(
            old.as_ref(),
            &name,
            parse_environment(draft.env.read(cx).value().as_ref()),
        );
        let config = ServerConfig {
            name,
            command,
            args: split_arguments(draft.args.read(cx).value().as_ref()),
            env,
            allowed_tools: names(draft.allowed.read(cx).value().as_ref()),
            enabled: draft.enabled,
        };
        let index = draft.index;
        let finish = move |this: &mut Self, cx: &mut Context<Self>| {
            let mut servers = this.servers(cx);
            match index {
                Some(i) if i < servers.len() => servers[i] = config,
                _ => servers.push(config),
            }
            this.server_draft = None;
            this.write_servers(&servers, cx);
        };
        if plan.store.is_empty() && plan.delete.is_empty() && plan.move_from.is_none() {
            finish(self, cx);
            return;
        }
        let task = cx.background_spawn(async move { apply_plan(&plan) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(()) => finish(this, cx),
                Err(key) => {
                    if let Some(draft) = this.server_draft.as_mut() {
                        draft.error = Some(focus_i18n::t(key).into());
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn migrate_server_secrets(&mut self, cx: &mut Context<Self>) {
        let json = self.prefs.read(cx).text(KEY);
        if !focus_view_ai::has_plaintext(&json) {
            return;
        }
        let task = cx.background_spawn(async move {
            let migration =
                focus_view_ai::migrate_plaintext(&json, focus_ai::keystore::server_env::store);
            migration.map(|m| (json, m))
        });
        cx.spawn(async move |this, cx| {
            let Some((before, migration)) = task.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if this.prefs.read(cx).text(KEY) == before && migration.moved > 0 {
                    this.prefs.update(cx, |prefs, cx| {
                        prefs.set(KEY, Value::Text(migration.json.clone().into()), cx)
                    });
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn check_server(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(config) = self.servers(cx).get(index).cloned() else {
            return;
        };
        let name = config.name.clone();
        let config_loaded =
            focus_view_ai::with_secrets(&config, focus_ai::keystore::server_env::load);
        self.server_checks.insert(name.clone(), Check::Running);
        let task = cx.background_spawn(async move { list_tools(&config_loaded) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let check = match result {
                    Ok(tools) => Check::Tools(tools),
                    Err(error) => {
                        eprintln!("focus: MCP server check: {error}");
                        Check::Failed(error_text(&error))
                    }
                };
                this.server_checks.insert(name, check);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn toggle_server(&mut self, index: usize, cx: &mut Context<Self>) {
        let mut servers = self.servers(cx);
        if let Some(server) = servers.get_mut(index) {
            server.enabled = !server.enabled;
            self.write_servers(&servers, cx);
        }
    }

    fn remove_server(&mut self, index: usize, cx: &mut Context<Self>) {
        let mut servers = self.servers(cx);
        if index < servers.len() {
            let removed = servers.remove(index);
            self.server_checks.remove(&removed.name);
            let secrets: Vec<(String, String)> = removed
                .env
                .keys()
                .map(|n| (removed.name.clone(), n.clone()))
                .collect();
            cx.background_spawn(async move {
                for (server, name) in secrets {
                    focus_ai::keystore::server_env::delete(&server, &name);
                }
            })
            .detach();
            self.write_servers(&servers, cx);
        }
    }

    fn toggle_allowed_tool(&mut self, tool: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.server_draft.as_ref() else {
            return;
        };
        let mut allowed = names(draft.allowed.read(cx).value().as_ref());
        match allowed.iter().position(|a| a == tool) {
            Some(position) => {
                allowed.remove(position);
            }
            None => allowed.push(tool.to_string()),
        }
        let input = draft.allowed.clone();
        input.update(cx, |input, cx| {
            input.set_value(allowed.join(", "), window, cx)
        });
        cx.notify();
    }

    pub(super) fn render_servers(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        let servers = self.servers(cx);
        let rows = servers.iter().enumerate().map(|(index, server)| {
            let state = if !server.enabled {
                (focus_i18n::t("settings-mcp-disabled"), t.ink_3)
            } else {
                match self.server_checks.get(&server.name) {
                    None => (focus_i18n::t("settings-mcp-enabled"), t.ink_2),
                    Some(Check::Running) => (focus_i18n::t("settings-mcp-checking"), t.ink_3),
                    Some(Check::Tools(tools)) => (
                        focus_i18n::t_args("settings-mcp-tools", &[("count", tools.len().into())]),
                        t.ink,
                    ),
                    Some(Check::Failed(message)) => (message.clone(), t.danger),
                }
            };
            let allowed = if server.allowed_tools.is_empty() {
                focus_i18n::t("settings-mcp-all-tools")
            } else {
                focus_i18n::t_args(
                    "settings-mcp-allowed-count",
                    &[("count", server.allowed_tools.len().into())],
                )
            };
            let command = format!("{} {}", server.command, join_arguments(&server.args));
            div()
                .flow_row()
                .items_center()
                .gap(size::S3)
                .min_h(px(48.))
                .border_b_1()
                .border_color(t.line)
                .child(
                    Checkbox::new(("mcp-enabled", index), server.enabled)
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_server(index, cx))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .py(size::S2)
                        .child(div().mono().text_color(t.ink).child(server.name.clone()))
                        .child(
                            div()
                                .ui_sm()
                                .text_color(t.ink_3)
                                .child(ellipsis(command.trim().to_string())),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .flex_none()
                        .max_w(px(220.))
                        .child(div().ui_sm().text_color(state.1).child(state.0))
                        .child(div().ui_sm().text_color(t.ink_3).child(allowed)),
                )
                .child(
                    Button::new(("mcp-check", index), focus_i18n::t("settings-mcp-check"))
                        .disabled(
                            !server.enabled
                                || matches!(
                                    self.server_checks.get(&server.name),
                                    Some(Check::Running)
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.check_server(index, cx))),
                )
                .child(
                    IconButton::new(
                        ("mcp-edit", index),
                        Icon::PENCIL,
                        focus_i18n::t("settings-mcp-edit"),
                    )
                    .dense(true)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_server_draft(Some(index), window, cx)
                    })),
                )
                .child(
                    IconButton::new(
                        ("mcp-remove", index),
                        Icon::TRASH,
                        focus_i18n::t("settings-mcp-remove"),
                    )
                    .dense(true)
                    .on_click(cx.listener(move |this, _, _, cx| this.remove_server(index, cx))),
                )
        });
        div()
            .flex()
            .flex_col()
            .gap(size::S3)
            .when(
                focus_view_ai::has_plaintext(&self.prefs.read(cx).text(KEY)),
                |this| {
                    this.child(
                        div()
                            .ui_sm()
                            .text_color(t.danger)
                            .child(focus_i18n::t("settings-mcp-plaintext-warning")),
                    )
                },
            )
            .child(
                div().flow_row().child(
                    Button::new("mcp-add", focus_i18n::t("settings-ai-mcp-servers-action"))
                        .icon(Icon::PLUS)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_server_draft(None, window, cx)
                        })),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .children(rows)
                    .when(servers.is_empty(), |this| {
                        this.child(
                            div()
                                .py(size::S2)
                                .ui_sm()
                                .text_color(t.ink_3)
                                .child(focus_i18n::t("settings-mcp-none")),
                        )
                    }),
            )
            .when_some(self.render_server_draft(&servers, cx), |this, form| {
                this.child(form)
            })
            .into_any_element()
    }

    fn render_server_draft(
        &self,
        servers: &[ServerConfig],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let draft = self.server_draft.as_ref()?;
        let t = *cx.tokens();
        let current = names(draft.allowed.read(cx).value().as_ref());
        let offered: Vec<String> = draft
            .index
            .and_then(|i| servers.get(i))
            .and_then(|s| match self.server_checks.get(&s.name) {
                Some(Check::Tools(tools)) => Some(tools.clone()),
                _ => None,
            })
            .unwrap_or_default();
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
                    focus_i18n::t("settings-mcp-field-name"),
                    Field::new(&draft.name),
                ))
                .child(super::field_row(
                    &t,
                    focus_i18n::t("settings-mcp-field-command"),
                    Field::new(&draft.command),
                ))
                .child(super::field_row(
                    &t,
                    focus_i18n::t("settings-mcp-field-args"),
                    Field::new(&draft.args),
                ))
                .child(super::field_row(
                    &t,
                    focus_i18n::t("settings-mcp-field-env"),
                    TextArea::new(&draft.env),
                ))
                .child(
                    div()
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(focus_i18n::t("settings-mcp-env-help")),
                )
                .child(super::field_row(
                    &t,
                    focus_i18n::t("settings-mcp-field-allowed"),
                    Field::new(&draft.allowed),
                ))
                .when(!offered.is_empty(), |this| {
                    this.child(div().flow_row().flex_wrap().gap(size::S2).children(
                        offered.iter().enumerate().map(|(i, tool)| {
                            let selected = current.contains(tool);
                            let tool_name = tool.clone();
                            Button::new(("mcp-tool", i), tool.clone())
                                .kind(if selected {
                                    ButtonKind::Primary
                                } else {
                                    ButtonKind::Secondary
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.toggle_allowed_tool(&tool_name, window, cx)
                                }))
                        }),
                    ))
                })
                .when_some(draft.error.clone(), |this, message| {
                    this.child(div().ui_sm().text_color(t.danger).child(message))
                })
                .child(
                    div()
                        .flow_row()
                        .gap(size::S2)
                        .child(
                            Button::new("mcp-save", focus_i18n::t("settings-ai-actions-save"))
                                .kind(ButtonKind::Primary)
                                .on_click(cx.listener(|this, _, _, cx| this.save_server_draft(cx))),
                        )
                        .child(
                            Button::new("mcp-cancel", focus_i18n::t("settings-ai-actions-cancel"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.server_draft = None;
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_split_like_a_shell() {
        assert_eq!(
            split_arguments(r#"-y @scope/server --root "/my docs" ''"#),
            ["-y", "@scope/server", "--root", "/my docs", ""]
        );
        assert_eq!(split_arguments("  a   b "), ["a", "b"]);
        assert!(split_arguments("").is_empty());
    }

    #[test]
    fn arguments_round_trip_through_the_field() {
        let args = vec![
            "--root".to_string(),
            "/my docs".to_string(),
            "x".to_string(),
        ];
        assert_eq!(split_arguments(&join_arguments(&args)), args);
    }

    #[test]
    fn environment_lines_and_tool_names_are_read() {
        let env = parse_environment("TOKEN = abc=def\nbroken\n=x\nHOME=/tmp");
        assert_eq!(env.get("TOKEN").map(String::as_str), Some("abc=def"));
        assert_eq!(env.len(), 2);
        assert_eq!(
            names("read, write  list\nsearch"),
            ["read", "write", "list", "search"]
        );
    }
}

#[cfg(test)]
mod plan_tests {
    use super::*;

    fn server(name: &str, env: &[(&str, &str)]) -> ServerConfig {
        ServerConfig {
            name: name.into(),
            command: "c".into(),
            args: vec![],
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            allowed_tools: vec![],
            enabled: true,
        }
    }

    fn lines(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn typed_values_go_to_the_keyring_and_the_file_keeps_names() {
        let (plan, env) = plan_env(None, "docs", lines(&[("TOKEN", "abc"), ("MODE", "")]));
        assert_eq!(plan.store, [("TOKEN".to_string(), "abc".to_string())]);
        assert_eq!(env, lines(&[("TOKEN", ""), ("MODE", "")]));
        assert!(plan.needs_keyring());
    }

    #[test]
    fn a_name_left_empty_keeps_the_stored_value_and_needs_no_keyring() {
        let old = server("docs", &[("TOKEN", "")]);
        let (plan, env) = plan_env(Some(&old), "docs", lines(&[("TOKEN", "")]));
        assert!(!plan.needs_keyring() && plan.store.is_empty() && plan.delete.is_empty());
        assert_eq!(env, lines(&[("TOKEN", "")]));
    }

    #[test]
    fn a_value_still_in_the_file_is_not_lost_by_an_edit() {
        let old = server("docs", &[("TOKEN", "left-in-file")]);
        let (_, env) = plan_env(Some(&old), "docs", lines(&[("TOKEN", "")]));
        assert_eq!(env.get("TOKEN").map(String::as_str), Some("left-in-file"));
    }

    #[test]
    fn removed_variables_are_deleted_and_a_rename_moves_the_values() {
        let old = server("docs", &[("TOKEN", ""), ("OLD", "")]);
        let (plan, _) = plan_env(Some(&old), "manuals", lines(&[("TOKEN", "")]));
        assert_eq!(
            plan.move_from,
            Some(("docs".to_string(), vec!["TOKEN".to_string()]))
        );
        assert!(
            plan.delete
                .contains(&("docs".to_string(), "OLD".to_string()))
        );
        assert!(
            plan.delete
                .contains(&("docs".to_string(), "TOKEN".to_string())),
            "the old entry goes once the new one exists"
        );
        assert!(plan.needs_keyring());
    }
}
