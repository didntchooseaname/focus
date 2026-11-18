use std::path::{Path, PathBuf};
use std::time::Duration;

use focus_ai::keystore::Location;
use focus_kit::Flow as _;
use focus_kit::{ActiveTokens, Button, ButtonKind, Icon, Type, size};
use gpui::{
    AnyElement, AppContext, Context, IntoElement, ParentElement, Styled, Task, Window, div,
    prelude::FluentBuilder,
};

use super::SettingsView;
use super::schema::Value;

#[derive(Clone, Copy, PartialEq)]
pub enum KeyStatus {
    Unknown,
    Missing,
    Found(Location),
}

#[derive(Clone, PartialEq)]
pub enum Probe {
    Idle,
    Running,
    Done { balance: Result<String, String> },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillState {
    Missing,
    Outdated,
    Current,
    Failed(String),
}

pub struct AiState {
    pub provider_key: Option<Option<&'static str>>,
    pub provider_models: Vec<String>,
    pub provider_test: Probe,
    pub skill: SkillState,
    pub agents: SkillState,
    pub key: KeyStatus,
    pub probe: Probe,
    pub models: Vec<String>,
    pub confirm_clear: bool,
    pub cleared: Option<usize>,
    pub copied: bool,
    copy_generation: u64,
    tasks: Vec<Task<()>>,
}

impl AiState {
    pub fn new() -> Self {
        AiState {
            provider_key: None,
            provider_models: Vec::new(),
            provider_test: Probe::Idle,
            skill: skill_state(),
            agents: SkillState::Missing,
            key: KeyStatus::Unknown,
            probe: Probe::Idle,
            models: Vec::new(),
            confirm_clear: false,
            cleared: None,
            copied: false,
            copy_generation: 0,
            tasks: Vec::new(),
        }
    }
}

fn error_key(error: &focus_ai::Error) -> &'static str {
    use focus_ai::Error;
    match error {
        Error::MissingKey => "settings-ai-error-missing-key",
        Error::InvalidKey => "settings-ai-error-invalid-key",
        Error::InsufficientBalance => "settings-ai-error-balance",
        Error::RateLimited { .. } => "settings-ai-error-rate",
        Error::Network(_) | Error::Server { .. } => "settings-ai-error-network",
        _ => "settings-ai-error-other",
    }
}

struct Outcome {
    models: Result<Vec<String>, focus_ai::Error>,
    balance: Result<String, focus_ai::Error>,
}

fn run_probe(base_url: String, typed: Option<String>) -> Outcome {
    let key = match typed.filter(|k| !k.trim().is_empty()) {
        Some(key) => Some(focus_ai::ApiKey::new(key.trim())),
        None => focus_ai::keystore::load().map(|(key, _)| key),
    };
    let Some(key) = key else {
        return Outcome {
            models: Err(focus_ai::Error::MissingKey),
            balance: Err(focus_ai::Error::MissingKey),
        };
    };
    let mut config = focus_ai::Config::new(key);
    if !base_url.trim().is_empty() {
        config.base_url = base_url.trim().to_string();
    }
    config.timeout_secs = 15;
    let client = focus_ai::Client::new(config);
    let models = client
        .models()
        .map(|models| models.into_iter().map(|m| m.id).collect());
    let balance = client.balance().map(|balance| {
        let amounts: Vec<String> = balance
            .balances
            .iter()
            .map(|(currency, amount)| format!("{amount} {currency}"))
            .collect();
        let amounts = amounts.join(", ");
        if balance.available {
            amounts
        } else {
            format!("{amounts} ({})", focus_i18n::t("settings-ai-balance-empty"))
        }
    });
    Outcome { models, balance }
}

const SKILL: &str = include_str!("../../../assets/skill/SKILL.md");

fn skills_dir() -> Option<PathBuf> {
    let base = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| focus_core::settings::home_dir().map(|home| home.join(".claude")))?;
    Some(base.join("skills"))
}

pub fn skill_state() -> SkillState {
    match skills_dir()
        .map(|dir| dir.join("focus").join("SKILL.md"))
        .and_then(|file| std::fs::read_to_string(file).ok())
    {
        None => SkillState::Missing,
        Some(text) if text == SKILL => SkillState::Current,
        Some(_) => SkillState::Outdated,
    }
}

const BEGIN: &str = "<!-- focus:begin -->";
const END: &str = "<!-- focus:end -->";

fn agents_block() -> String {
    let body = SKILL.splitn(3, "---").nth(2).unwrap_or(SKILL).trim();
    let body = body.replace("Tool names start with `mcp__focus__`.", "Tools are named as the server lists them (`search_notes`, `read_note`...); some clients show them with the name of the server in front.").replace("mcp__focus__", "").replace(" on behalf of this skill", "");
    format!("{BEGIN}\n{body}\n{END}\n")
}

pub fn merge_agents_md(existing: Option<&str>, block: &str) -> String {
    let Some(text) = existing.filter(|t| !t.trim().is_empty()) else {
        return block.to_string();
    };
    if let (Some(start), Some(end)) = (text.find(BEGIN), text.find(END))
        && start < end
    {
        let after = end + END.len();
        let after = if text[after..].starts_with('\n') {
            after + 1
        } else {
            after
        };
        return format!("{}{}{}", &text[..start], block, &text[after..]);
    }
    let separator = if text.ends_with("\n\n") {
        ""
    } else if text.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!("{text}{separator}{block}")
}

fn agents_path(vault: Option<&str>) -> Option<PathBuf> {
    vault.map(|v| Path::new(v).join("AGENTS.md"))
}

fn agents_state(vault: Option<&str>) -> SkillState {
    let Some(path) = agents_path(vault) else {
        return SkillState::Missing;
    };
    match std::fs::read_to_string(path) {
        Err(_) => SkillState::Missing,
        Ok(text) if text.contains(&agents_block()) => SkillState::Current,
        Ok(text) if text.contains(BEGIN) => SkillState::Outdated,
        Ok(_) => SkillState::Missing,
    }
}

fn install_skill(skills: &Path) -> std::io::Result<PathBuf> {
    let dir = skills.join("focus");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("SKILL.md");
    std::fs::write(&file, SKILL)?;
    Ok(file)
}

pub fn mcp_command(vault: Option<&str>) -> Option<String> {
    let vault = vault?;
    let binary = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("focus-mcp")))
        .filter(|path| path.is_file());
    let binary = binary.map_or_else(
        || "focus-mcp".to_string(),
        |path| quote(&path.display().to_string()),
    );
    Some(format!("{binary} --vault {}", quote(vault)))
}

fn quote(text: &str) -> String {
    if text
        .chars()
        .any(|c| c.is_whitespace() || "\"'\\$`".contains(c))
    {
        format!("'{}'", text.replace('\'', "'\\''"))
    } else {
        text.to_string()
    }
}

fn clear_folder(dir: &Path) -> std::io::Result<usize> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut removed = 0;
    for entry in entries {
        let path: PathBuf = entry?.path();
        if path.extension().is_some_and(|e| e == "json") {
            std::fs::remove_file(&path)?;
            removed += 1;
        }
    }
    Ok(removed)
}

impl SettingsView {
    pub(super) fn provider(&self, cx: &Context<Self>) -> focus_ai::Provider {
        focus_ai::Provider::ALL
            .get(self.prefs.read(cx).choice("ai.provider"))
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn applies_to_provider(&self, key: &str, cx: &Context<Self>) -> bool {
        let provider = self.provider(cx);
        let deepseek_only = matches!(
            key,
            "ai.key"
                | "ai.key_status"
                | "ai.test"
                | "ai.balance"
                | "ai.base_url"
                | "ai.model"
                | "ai.models"
        );
        if provider == focus_ai::Provider::DeepSeek {
            return !key.starts_with("ai.provider_")
                && !(key.starts_with("ai.")
                    && (key.ends_with(".base_url") || key.ends_with(".model"))
                    && !deepseek_only);
        }
        if deepseek_only {
            return false;
        }
        if key == "ai.provider_key" {
            return !provider.spec().local;
        }
        match key
            .strip_prefix("ai.")
            .and_then(|rest| rest.split_once('.'))
        {
            Some((id, "base_url" | "model")) => id == provider.spec().id,
            _ => true,
        }
    }

    pub(super) fn refresh_provider_key(&mut self, cx: &mut Context<Self>) {
        let provider = self.provider(cx);
        if provider == focus_ai::Provider::DeepSeek {
            return;
        }
        let task = cx.background_spawn(async move {
            let from_env = provider
                .spec()
                .key_env
                .and_then(|name| std::env::var(name).ok())
                .is_some_and(|v| !v.trim().is_empty());
            if from_env {
                Some("settings-ai-key-environment")
            } else {
                focus_ai::keystore::provider_key::load(provider).map(|_| "settings-ai-key-keyring")
            }
        });
        let handle = cx.spawn(async move |this, cx| {
            let found = task.await;
            let _ = this.update(cx, |this, cx| {
                this.ai.provider_key = Some(found);
                cx.notify();
            });
        });
        self.ai.tasks.push(handle);
    }

    pub(super) fn store_provider_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.inputs.get("ai.provider_key").cloned() else {
            return;
        };
        let key = input.read(cx).value().trim().to_string();
        if key.is_empty() {
            return;
        }
        input.update(cx, |input, cx| input.set_value("", window, cx));
        let provider = self.provider(cx);
        let task = cx.background_spawn(async move {
            focus_ai::keystore::provider_key::store(provider, &focus_ai::ApiKey::new(key))
        });
        let handle = cx.spawn(async move |this, cx| {
            let stored = task.await;
            let _ = this.update(cx, |this, cx| {
                match stored {
                    Ok(()) => this.refresh_provider_key(cx),
                    Err(error) => {
                        eprintln!("focus: storing the key: {error}");
                        this.ai.provider_test =
                            Probe::Failed(focus_i18n::t("settings-ai-key-store-failed"));
                    }
                }
                cx.notify();
            });
        });
        self.ai.tasks.push(handle);
    }

    pub(super) fn test_provider(&mut self, cx: &mut Context<Self>) {
        if self.ai.provider_test == Probe::Running {
            return;
        }
        self.ai.provider_test = Probe::Running;
        let provider = self.provider(cx);
        let base_url = self
            .prefs
            .read(cx)
            .text(&format!("ai.{}.base_url", provider.spec().id));
        let typed = self
            .inputs
            .get("ai.provider_key")
            .map(|input| input.read(cx).value().to_string())
            .filter(|k| !k.trim().is_empty());
        let task = cx.background_spawn(async move {
            let key = typed
                .map(|k| focus_ai::ApiKey::new(k.trim()))
                .or_else(|| focus_ai::keystore::provider_key::load(provider))
                .unwrap_or_else(|| focus_ai::ApiKey::new(""));
            let mut config = focus_ai::Config::for_provider(provider, key);
            if !base_url.trim().is_empty() {
                config.base_url = base_url.trim().to_string();
            }
            config.timeout_secs = 15;
            focus_ai::Client::new(config)
                .models()
                .map(|models| models.into_iter().map(|m| m.id).collect::<Vec<_>>())
        });
        let handle = cx.spawn(async move |this, cx| {
            let listed = task.await;
            let _ = this.update(cx, |this, cx| {
                match listed {
                    Ok(models) => {
                        this.ai.provider_models = models;
                        this.ai.provider_test = Probe::Done {
                            balance: Ok(String::new()),
                        };
                    }
                    Err(error) => {
                        eprintln!("focus: connection test: {error}");
                        this.ai.provider_models.clear();
                        this.ai.provider_test = Probe::Failed(focus_i18n::t(error_key(&error)));
                    }
                }
                cx.notify();
            });
        });
        self.ai.tasks.push(handle);
        cx.notify();
    }

    pub(super) fn render_provider_models(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        if self.ai.provider_models.is_empty() {
            return div()
                .ui_sm()
                .text_color(t.ink_3)
                .child(focus_i18n::t("settings-ai-models-empty"))
                .into_any_element();
        }
        let key = format!("ai.{}.model", self.provider(cx).spec().id);
        let current = self.prefs.read(cx).text(&key);
        div()
            .flow_row()
            .flex_wrap()
            .gap(size::S2)
            .children(
                self.ai
                    .provider_models
                    .iter()
                    .take(40)
                    .enumerate()
                    .map(|(index, model)| {
                        let (selected, chosen, key) =
                            (*model == current, model.clone(), key.clone());
                        Button::new(("provider-model", index), model.clone())
                            .kind(if selected {
                                ButtonKind::Primary
                            } else {
                                ButtonKind::Secondary
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(setting) =
                                    super::schema::SETTINGS.iter().find(|s| s.key == key)
                                {
                                    if let Some(input) = this.inputs.get(setting.key).cloned() {
                                        input.update(cx, |input, cx| {
                                            input.set_value(chosen.clone(), window, cx)
                                        });
                                    }
                                    this.prefs.update(cx, |prefs, cx| {
                                        prefs.set(
                                            setting.key,
                                            Value::Text(chosen.clone().into()),
                                            cx,
                                        )
                                    });
                                }
                            }))
                    }),
            )
            .into_any_element()
    }

    pub(super) fn refresh_key_status(&mut self, cx: &mut Context<Self>) {
        let task =
            cx.background_spawn(async { focus_ai::keystore::load().map(|(_, location)| location) });
        let handle = cx.spawn(async move |this, cx| {
            let found = task.await;
            let _ = this.update(cx, |this, cx| {
                this.ai.key = found.map_or(KeyStatus::Missing, KeyStatus::Found);
                cx.notify();
            });
        });
        self.ai.tasks.push(handle);
    }

    pub(super) fn hand_over_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.inputs.get("ai.key").cloned() else {
            return;
        };
        let key = input.read(cx).value().trim().to_string();
        if key.is_empty() {
            return;
        }
        input.update(cx, |input, cx| input.set_value("", window, cx));
        self.prefs.update(cx, |prefs, cx| {
            prefs.set("ai.key", Value::Text(key.into()), cx)
        });
        self.ai.key = KeyStatus::Unknown;
        let prefs = self.prefs.clone();
        let handle = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(600))
                .await;
            let _ = prefs.update(cx, |prefs, cx| {
                prefs.set("ai.key", Value::Text("".into()), cx)
            });
            cx.background_executor()
                .timer(Duration::from_millis(1400))
                .await;
            let _ = this.update(cx, |this, cx| this.refresh_key_status(cx));
        });
        self.ai.tasks.push(handle);
        cx.notify();
    }

    pub(super) fn run_probe(&mut self, cx: &mut Context<Self>) {
        if self.ai.probe == Probe::Running {
            return;
        }
        self.ai.probe = Probe::Running;
        let base_url = self.prefs.read(cx).text("ai.base_url");
        let typed = self
            .inputs
            .get("ai.key")
            .map(|input| input.read(cx).value().to_string());
        let task = cx.background_spawn(async move { run_probe(base_url, typed) });
        let handle = cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                match outcome.models {
                    Ok(models) => {
                        this.ai.models = models;
                        this.ai.probe = Probe::Done {
                            balance: outcome.balance.map_err(|e| focus_i18n::t(error_key(&e))),
                        };
                    }
                    Err(error) => {
                        eprintln!("focus: connection test: {error}");
                        this.ai.models.clear();
                        this.ai.probe = Probe::Failed(focus_i18n::t(error_key(&error)));
                    }
                }
                cx.notify();
            });
        });
        self.ai.tasks.push(handle);
        cx.notify();
    }

    pub(super) fn clear_history(&mut self, cx: &mut Context<Self>) {
        if !self.ai.confirm_clear {
            self.ai.confirm_clear = true;
            self.ai.cleared = None;
            cx.notify();
            return;
        }
        self.ai.confirm_clear = false;
        let Some(vault) = self.facts.vault.clone() else {
            return;
        };
        let dir = focus_view_ai::history_dir_for(Path::new(&vault));
        let task = cx.background_spawn(async move { clear_folder(&dir) });
        let handle = cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(count) => this.ai.cleared = Some(count),
                    Err(error) => {
                        eprintln!("focus: clearing the history: {error}");
                        this.notice =
                            Some((true, focus_i18n::t("settings-ai-clear-failed").into()));
                    }
                }
                cx.notify();
            });
        });
        self.ai.tasks.push(handle);
        cx.notify();
    }

    pub(super) fn refresh_agents(&mut self) {
        self.ai.agents = agents_state(self.facts.vault.as_deref());
    }

    pub(super) fn write_agents_md(&mut self, cx: &mut Context<Self>) {
        let Some(path) = agents_path(self.facts.vault.as_deref()) else {
            self.ai.agents = SkillState::Failed(focus_i18n::t("settings-ai-agents-no-vault"));
            cx.notify();
            return;
        };
        let task = cx.background_spawn(async move {
            let existing = std::fs::read_to_string(&path).ok();
            std::fs::write(&path, merge_agents_md(existing.as_deref(), &agents_block()))
        });
        let handle = cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.ai.agents = match result {
                    Ok(()) => agents_state(this.facts.vault.as_deref()),
                    Err(error) => SkillState::Failed(focus_i18n::t_args(
                        "settings-ai-agents-failed",
                        &[("detail", error.to_string().as_str().into())],
                    )),
                };
                cx.notify();
            });
        });
        self.ai.tasks.push(handle);
    }

    pub(super) fn install_skill(&mut self, cx: &mut Context<Self>) {
        let Some(skills) = skills_dir() else {
            self.ai.skill = SkillState::Failed(focus_i18n::t("settings-ai-skill-no-folder"));
            cx.notify();
            return;
        };
        let task = cx.background_spawn(async move { install_skill(&skills) });
        let handle = cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.ai.skill = match result {
                    Ok(_) => skill_state(),
                    Err(error) => SkillState::Failed(focus_i18n::t_args(
                        "settings-ai-skill-failed-detail",
                        &[("detail", error.to_string().as_str().into())],
                    )),
                };
                cx.notify();
            });
        });
        self.ai.tasks.push(handle);
    }

    pub(super) fn copy_mcp_command(&mut self, cx: &mut Context<Self>) {
        let Some(command) = mcp_command(self.facts.vault.as_deref()) else {
            return;
        };
        focus_kit::clipboard::copy_text(cx, command);
        self.ai.copied = true;
        cx.notify();
        self.ai.copy_generation += 1;
        let generation = self.ai.copy_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1500))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.ai.copy_generation == generation {
                    this.ai.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn ai_info(&self, key: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = *cx.tokens();
        let plain = |text: String, color| {
            div()
                .ui()
                .text_color(color)
                .max_w(gpui::px(420.))
                .child(text)
                .into_any_element()
        };
        let mono = |text: String, color| {
            div()
                .mono()
                .text_color(color)
                .max_w(gpui::px(420.))
                .child(text)
                .into_any_element()
        };
        match key {
            "ai.key_status" => {
                let text = match self.ai.key {
                    KeyStatus::Unknown => focus_i18n::t("settings-ai-key-checking"),
                    KeyStatus::Missing => focus_i18n::t("settings-ai-key-missing"),
                    KeyStatus::Found(Location::Environment) => {
                        focus_i18n::t("settings-ai-key-environment")
                    }
                    KeyStatus::Found(Location::Keyring) => focus_i18n::t("settings-ai-key-keyring"),
                    KeyStatus::Found(Location::File) => focus_i18n::t("settings-ai-key-file"),
                };
                let color = if self.ai.key == KeyStatus::Missing {
                    t.ink_3
                } else {
                    t.ink
                };
                Some(plain(text, color))
            }
            "ai.provider_status" => {
                let provider = self.provider(cx);
                let text = match (&self.ai.provider_test, self.ai.provider_key) {
                    (Probe::Failed(message), _) => return Some(plain(message.clone(), t.danger)),
                    (_, _) if !provider.spec().key_required && provider.spec().local => {
                        focus_i18n::t("settings-ai-provider-no-key-needed")
                    }
                    (_, None) => focus_i18n::t("settings-ai-key-checking"),
                    (_, Some(None)) => focus_i18n::t("settings-ai-key-missing"),
                    (_, Some(Some(source))) => focus_i18n::t(source),
                };
                let url = self
                    .prefs
                    .read(cx)
                    .text(&format!("ai.{}.base_url", provider.spec().id));
                let place = if focus_ai::Provider::is_loopback(&url) {
                    focus_i18n::t_args(
                        "settings-ai-provider-local",
                        &[("url", url.as_str().into())],
                    )
                } else {
                    focus_i18n::t_args(
                        "settings-ai-provider-remote",
                        &[("url", url.as_str().into())],
                    )
                };
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap(size::S1)
                        .child(plain(text, t.ink))
                        .child(plain(place, t.ink_3))
                        .into_any_element(),
                )
            }
            "ai.balance" => Some(match &self.ai.probe {
                Probe::Idle => plain(focus_i18n::t("settings-ai-not-tested"), t.ink_3),
                Probe::Running => plain(focus_i18n::t("settings-ai-testing"), t.ink_3),
                Probe::Failed(message) => plain(message.clone(), t.danger),
                Probe::Done {
                    balance: Ok(amount),
                } => mono(amount.clone(), t.ink),
                Probe::Done {
                    balance: Err(message),
                } => plain(message.clone(), t.danger),
            }),
            "ai.mcp_command" => {
                let command = mcp_command(self.facts.vault.as_deref());
                Some(match command {
                    None => plain(focus_i18n::t("settings-mcp-no-vault"), t.ink_3),
                    Some(command) => div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(size::S2)
                        .child(
                            div()
                                .mono()
                                .text_color(t.ink)
                                .max_w(gpui::px(420.))
                                .child(command),
                        )
                        .child(
                            Button::new(
                                "copy-mcp-command",
                                focus_i18n::t(if self.ai.copied {
                                    "settings-ai-copied"
                                } else {
                                    "settings-ai-copy"
                                }),
                            )
                            .icon(if self.ai.copied {
                                Icon::CHECK
                            } else {
                                Icon::COPY
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.copy_mcp_command(cx))),
                        )
                        .into_any_element(),
                })
            }
            _ => None,
        }
    }

    pub(super) fn ai_action(
        &self,
        key: &'static str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match key {
            "ai.test" => Some(
                Button::new(
                    key,
                    focus_i18n::t(if self.ai.probe == Probe::Running {
                        "settings-ai-testing"
                    } else {
                        "settings-ai-test-action"
                    }),
                )
                .disabled(self.ai.probe == Probe::Running)
                .on_click(cx.listener(|this, _, _, cx| this.run_probe(cx)))
                .into_any_element(),
            ),
            "ai.agents_md" => {
                let t = *cx.tokens();
                let path = agents_path(self.facts.vault.as_deref())
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                let (label, current) = match self.ai.agents {
                    SkillState::Current => ("settings-ai-skill-done", true),
                    SkillState::Outdated => ("settings-ai-skill-update", false),
                    _ => ("settings-ai-agents-action", false),
                };
                let line = match &self.ai.agents {
                    SkillState::Current => {
                        div().ui_sm().text_color(t.ok).child(focus_i18n::t_args(
                            "settings-ai-agents-current",
                            &[("path", path.as_str().into())],
                        ))
                    }
                    SkillState::Outdated => {
                        div().ui_sm().text_color(t.warn).child(focus_i18n::t_args(
                            "settings-ai-agents-outdated",
                            &[("path", path.as_str().into())],
                        ))
                    }
                    SkillState::Missing => {
                        div().ui_sm().text_color(t.ink_3).child(focus_i18n::t_args(
                            "settings-ai-agents-missing",
                            &[("path", path.as_str().into())],
                        ))
                    }
                    SkillState::Failed(message) => {
                        div().ui_sm().text_color(t.danger).child(message.clone())
                    }
                };
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(size::S2)
                        .child(
                            Button::new(key, focus_i18n::t(label))
                                .icon(if current {
                                    Icon::CHECK
                                } else {
                                    Icon::FILE_TEXT
                                })
                                .disabled(self.facts.vault.is_none())
                                .on_click(cx.listener(|this, _, _, cx| this.write_agents_md(cx))),
                        )
                        .child(line.max_w(gpui::px(420.)).mono_sm())
                        .into_any_element(),
                )
            }
            "ai.provider_test" => Some(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(size::S2)
                    .child(
                        Button::new(
                            key,
                            focus_i18n::t(if self.ai.provider_test == Probe::Running {
                                "settings-ai-testing"
                            } else {
                                "settings-ai-provider-test-action"
                            }),
                        )
                        .disabled(self.ai.provider_test == Probe::Running)
                        .on_click(cx.listener(|this, _, _, cx| this.test_provider(cx))),
                    )
                    .child(match &self.ai.provider_test {
                        Probe::Done { .. } => {
                            div()
                                .ui_sm()
                                .text_color(cx.tokens().ok)
                                .child(focus_i18n::t_args(
                                    "settings-ai-provider-models-found",
                                    &[("count", self.ai.provider_models.len().into())],
                                ))
                        }
                        _ => div(),
                    })
                    .into_any_element(),
            ),
            "ai.skill" => {
                let t = *cx.tokens();
                let path = skills_dir()
                    .map(|dir| dir.join("focus").join("SKILL.md").display().to_string())
                    .unwrap_or_default();
                let (label, current) = match self.ai.skill {
                    SkillState::Current => ("settings-ai-skill-done", true),
                    SkillState::Outdated => ("settings-ai-skill-update", false),
                    _ => ("settings-ai-skill-action", false),
                };
                let line = match &self.ai.skill {
                    SkillState::Current => {
                        div().ui_sm().text_color(t.ok).child(focus_i18n::t_args(
                            "settings-ai-skill-installed",
                            &[("path", path.as_str().into())],
                        ))
                    }
                    SkillState::Outdated => {
                        div().ui_sm().text_color(t.warn).child(focus_i18n::t_args(
                            "settings-ai-skill-outdated",
                            &[("path", path.as_str().into())],
                        ))
                    }
                    SkillState::Missing => {
                        div().ui_sm().text_color(t.ink_3).child(focus_i18n::t_args(
                            "settings-ai-skill-missing",
                            &[("path", path.as_str().into())],
                        ))
                    }
                    SkillState::Failed(message) => {
                        div().ui_sm().text_color(t.danger).child(message.clone())
                    }
                };
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(size::S2)
                        .child(
                            Button::new(key, focus_i18n::t(label))
                                .icon(if current { Icon::CHECK } else { Icon::SPARKLES })
                                .on_click(cx.listener(|this, _, _, cx| this.install_skill(cx))),
                        )
                        .child(line.max_w(gpui::px(420.)).mono_sm())
                        .into_any_element(),
                )
            }
            "ai.clear" => {
                let confirm = self.ai.confirm_clear;
                let t = *cx.tokens();
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(size::S2)
                        .child(
                            Button::new(
                                key,
                                focus_i18n::t(if confirm {
                                    "settings-ai-clear-confirm"
                                } else {
                                    "settings-ai-clear-action"
                                }),
                            )
                            .kind(if confirm {
                                ButtonKind::Danger
                            } else {
                                ButtonKind::Secondary
                            })
                            .disabled(self.facts.vault.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.clear_history(cx))),
                        )
                        .when_some(self.ai.cleared, |this, count| {
                            this.child(div().ui_sm().text_color(t.ink_3).child(focus_i18n::t_args(
                                "settings-ai-cleared",
                                &[("count", count.into())],
                            )))
                        })
                        .into_any_element(),
                )
            }
            _ => None,
        }
    }

    pub(super) fn render_models(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        if self.ai.models.is_empty() {
            return div()
                .ui_sm()
                .text_color(t.ink_3)
                .child(focus_i18n::t("settings-ai-models-empty"))
                .into_any_element();
        }
        let current = self.prefs.read(cx).text("ai.model");
        div()
            .flow_row()
            .flex_wrap()
            .gap(size::S2)
            .children(self.ai.models.iter().enumerate().map(|(index, model)| {
                let selected = *model == current;
                let model = model.clone();
                Button::new(("model", index), model.clone())
                    .kind(if selected {
                        ButtonKind::Primary
                    } else {
                        ButtonKind::Secondary
                    })
                    .on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.choose_model(&model, window, cx)
                        }),
                    )
            }))
            .into_any_element()
    }

    fn choose_model(&mut self, model: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.inputs.get("ai.model").cloned() {
            input.update(cx, |input, cx| {
                input.set_value(model.to_string(), window, cx)
            });
        }
        self.prefs.update(cx, |prefs, cx| {
            prefs.set("ai.model", Value::Text(model.to_string().into()), cx)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BEGIN, END, agents_block, clear_folder, install_skill, mcp_command, merge_agents_md, quote,
    };

    #[test]
    fn paths_with_spaces_are_quoted_for_a_shell() {
        assert_eq!(quote("/home/a b/vault"), "'/home/a b/vault'");
        assert_eq!(quote("/home/a/vault"), "/home/a/vault");
        assert_eq!(quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn clearing_removes_the_conversations_and_nothing_else() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/settings-clear-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test folder");
        for name in ["a.json", "b.json", "keep.txt"] {
            std::fs::write(dir.join(name), "{}").expect("test file");
        }
        assert_eq!(clear_folder(&dir).expect("cleared"), 2);
        assert!(dir.join("keep.txt").exists() && !dir.join("a.json").exists());
        assert_eq!(
            clear_folder(&dir.join("missing")).expect("a missing folder holds nothing"),
            0
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_skill_is_written_under_its_own_folder() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/settings-skill-test");
        let _ = std::fs::remove_dir_all(&dir);
        let file = install_skill(&dir).expect("installed");
        assert!(file.ends_with("focus/SKILL.md"));
        let text = std::fs::read_to_string(&file).expect("readable");
        assert!(
            text.starts_with("---") && text.contains("name: focus"),
            "{}",
            &text[..60.min(text.len())]
        );
        assert!(
            install_skill(&dir).is_ok(),
            "installing again replaces the file"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_command_names_the_vault() {
        let command = mcp_command(Some("/tmp/my vault")).expect("a vault is open");
        assert!(command.ends_with("--vault '/tmp/my vault'"), "{command}");
        assert!(mcp_command(None).is_none());
    }

    #[test]
    fn the_managed_block_is_created_replaced_and_never_duplicated() {
        let block = agents_block();
        assert!(
            block.starts_with(BEGIN)
                && block.trim_end().ends_with(END)
                && !block.contains("name: focus")
        );
        assert!(
            !block.contains("mcp__")
                && !block.to_lowercase().contains("skill")
                && block.contains("`search_notes`"),
            "{block}"
        );
        assert_eq!(merge_agents_md(None, &block), block);
        let mine = "# My project\n\nRules of the project.\n";
        let first = merge_agents_md(Some(mine), &block);
        assert!(first.starts_with(mine) && first.ends_with(&block));
        let second = merge_agents_md(Some(&first), &block);
        assert_eq!(second, first, "a second write changes nothing");
        let edited = first.replace("Rules of the project.", "Other rules.");
        let stale = edited.replace("Search before reading", "Old text");
        let updated = merge_agents_md(Some(&stale), &block);
        assert!(
            updated.contains("Other rules.")
                && updated.contains("Search before reading")
                && !updated.contains("Old text")
                && updated.matches(BEGIN).count() == 1
        );
        let after = format!("{first}\n## Notes after\n");
        let kept = merge_agents_md(Some(&after), &block);
        assert!(kept.ends_with("## Notes after\n") && kept.matches(BEGIN).count() == 1);
    }
}
