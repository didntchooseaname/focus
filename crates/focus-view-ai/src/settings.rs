use focus_ai::prompts::Template;

#[derive(Clone, Debug)]
pub struct AiSettings {
    pub enabled: bool,
    pub provider: focus_ai::Provider,
    pub model: String,
    pub base_url: String,
    pub redact: bool,
    pub redact_ips: bool,
    pub temperature: Option<f32>,
    pub thinking: bool,
    pub preprompt: String,
    pub actions: Vec<Template>,
    pub reply_language: String,
    pub ops_mode: bool,
    pub reduce_motion: bool,
    pub search: Option<bool>,
    pub write_tools: bool,
    pub max_tokens: Option<u32>,
    pub max_steps: u32,
    pub keep_history: bool,
    pub mcp_servers: Vec<focus_ai::mcp::ServerConfig>,
}

impl Default for AiSettings {
    fn default() -> Self {
        AiSettings {
            enabled: true,
            provider: focus_ai::Provider::DeepSeek,
            model: focus_ai::MODEL_FLASH.to_string(),
            base_url: focus_ai::DEFAULT_BASE_URL.to_string(),
            redact: true,
            redact_ips: false,
            temperature: None,
            thinking: false,
            preprompt: String::new(),
            actions: Vec::new(),
            reply_language: "interface".to_string(),
            ops_mode: false,
            reduce_motion: false,
            search: None,
            write_tools: true,
            max_tokens: None,
            max_steps: crate::backend::MAX_STEPS,
            keep_history: true,
            mcp_servers: Vec::new(),
        }
    }
}

fn stored<'a>(
    settings: &'a focus_core::settings::Settings,
    key: &str,
) -> Option<&'a serde_json::Value> {
    settings.ui.get("settings")?.get(key)
}

fn custom_actions(settings: &focus_core::settings::Settings) -> Vec<Template> {
    let mut actions: Vec<Template> = settings
        .ai_actions
        .iter()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect();
    let edited: Vec<Template> = stored(settings, "ai.actions_json")
        .and_then(serde_json::Value::as_str)
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_default();
    for action in edited {
        actions.retain(|a| a.id != action.id);
        actions.push(action);
    }
    actions
}

impl AiSettings {
    pub fn from_core(settings: &focus_core::settings::Settings) -> AiSettings {
        let provider = stored(settings, "ai.provider")
            .and_then(serde_json::Value::as_u64)
            .and_then(|i| focus_ai::Provider::ALL.get(i as usize).copied())
            .unwrap_or_default();
        AiSettings {
            enabled: settings
                .ui
                .get("settings")
                .and_then(|s| s.get("ai.enabled"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            provider,
            model: if provider == focus_ai::Provider::DeepSeek {
                settings.ai_model.clone()
            } else {
                stored(settings, &format!("ai.{}.model", provider.spec().id))
                    .and_then(serde_json::Value::as_str)
                    .filter(|m| !m.trim().is_empty())
                    .or(provider.default_model())
                    .unwrap_or_default()
                    .to_string()
            },
            base_url: if provider == focus_ai::Provider::DeepSeek {
                settings.ai_base_url.clone()
            } else {
                stored(settings, &format!("ai.{}.base_url", provider.spec().id))
                    .and_then(serde_json::Value::as_str)
                    .filter(|u| !u.trim().is_empty())
                    .unwrap_or(provider.spec().base_url)
                    .to_string()
            },
            redact: settings.ai_redact,
            redact_ips: settings.ai_redact_ips,
            temperature: settings
                .ai_temperature
                .filter(|t| (t - 1.0).abs() > f32::EPSILON),
            thinking: settings.ai_thinking,
            preprompt: settings.ai_preprompt.clone(),
            actions: custom_actions(settings),
            reply_language: settings.ai_reply_language.clone(),
            ops_mode: settings.ops_enabled,
            reduce_motion: settings.reduce_motion,
            search: settings.ai_search,
            write_tools: settings.ai_write_tools,
            max_tokens: stored(settings, "ai.max_tokens")
                .and_then(serde_json::Value::as_f64)
                .filter(|n| *n > 0.)
                .map(|n| n as u32),
            max_steps: stored(settings, "ai.max_steps")
                .and_then(serde_json::Value::as_f64)
                .map_or(crate::backend::MAX_STEPS, |n| (n as u32).clamp(1, 20)),
            keep_history: stored(settings, "ai.history")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            mcp_servers: stored(settings, "ai.mcp_json")
                .and_then(serde_json::Value::as_str)
                .and_then(|text| serde_json::from_str(text).ok())
                .unwrap_or_default(),
        }
    }

    pub fn response_language(&self, note_text: &str) -> String {
        match self.reply_language.as_str() {
            "interface" => focus_i18n::language(),
            "note" => match focus_ai::prompts::detect_language(note_text) {
                "French" => "fr".to_string(),
                _ => "en".to_string(),
            },
            code => code.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionEntry {
    pub id: String,
    pub label: String,
    pub description: String,
}

impl AiSettings {
    pub fn visible_actions(&self) -> Vec<ActionEntry> {
        let shown = |text: &str| {
            if text.starts_with("ai-action-") {
                focus_i18n::t(text)
            } else {
                text.to_string()
            }
        };
        focus_ai::prompts::merged(&self.actions)
            .into_iter()
            .filter(|a| a.enabled && (!a.ops_only || self.ops_mode))
            .map(|a| ActionEntry {
                label: shown(&a.label),
                description: shown(&a.description),
                id: a.id,
            })
            .collect()
    }
}

pub fn language_name(code: &str) -> String {
    match code
        .split('-')
        .next()
        .unwrap_or(code)
        .to_ascii_lowercase()
        .as_str()
    {
        "en" => "English".to_string(),
        "fr" => "French".to_string(),
        "ar" => "Arabic".to_string(),
        "zh" => "Chinese".to_string(),
        _ => code.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::AiSettings;

    #[test]
    fn core_settings_are_read() {
        let core = focus_core::settings::Settings {
            ai_reply_language: "note".into(),
            ai_write_tools: false,
            ai_search: Some(false),
            ..Default::default()
        };
        let settings = AiSettings::from_core(&core);
        assert_eq!(settings.reply_language, "note");
        assert!(!settings.write_tools);
        assert_eq!(settings.search, Some(false));
    }

    #[test]
    fn stored_values_without_a_typed_field_are_read() {
        let mut core = focus_core::settings::Settings::default();
        let template = serde_json::json!([{ "id": "mine", "label": "Mine", "prompt": "{{selection}}", "scope": "selection", "output": "chat", "model": null, "ops_only": false, "enabled": true, "builtin": false }]);
        core.ui.insert("settings".into(), serde_json::json!({ "ai.max_tokens": 1024.0, "ai.max_steps": 3.0, "ai.history": false, "ai.actions_json": template.to_string(), "ai.mcp_json": "[{\"name\":\"docs\",\"command\":\"docs-server\"}]" }));
        let settings = AiSettings::from_core(&core);
        assert_eq!(
            (
                settings.max_tokens,
                settings.max_steps,
                settings.keep_history
            ),
            (Some(1024), 3, false)
        );
        assert!(settings.actions.iter().any(|a| a.id == "mine"));
        assert_eq!(settings.mcp_servers.len(), 1);
        assert!(settings.mcp_servers[0].enabled, "enabled by default");
    }

    #[test]
    fn ai_is_on_unless_the_settings_say_otherwise() {
        let mut core = focus_core::settings::Settings::default();
        assert!(AiSettings::from_core(&core).enabled);
        core.ui.insert(
            "settings".into(),
            serde_json::json!({ "ai.enabled": false }),
        );
        assert!(!AiSettings::from_core(&core).enabled);
    }

    #[test]
    fn the_provider_and_its_own_address_and_model_are_read_from_the_settings() {
        let mut core = focus_core::settings::Settings::default();
        assert_eq!(
            AiSettings::from_core(&core).provider,
            focus_ai::Provider::DeepSeek
        );
        core.ui.insert(
            "settings".into(),
            serde_json::json!({ "ai.provider": 3, "ai.lmstudio.model": "qwen3-8b" }),
        );
        let settings = AiSettings::from_core(&core);
        assert_eq!(
            (
                settings.provider,
                settings.base_url.as_str(),
                settings.model.as_str()
            ),
            (
                focus_ai::Provider::LmStudio,
                "http://localhost:1234/v1",
                "qwen3-8b"
            )
        );
        core.ui.insert("settings".into(), serde_json::json!({ "ai.provider": 3, "ai.lmstudio.base_url": "http://192.168.1.9:1234/v1" }));
        assert_eq!(
            AiSettings::from_core(&core).base_url,
            "http://192.168.1.9:1234/v1"
        );
    }

    #[test]
    fn the_default_temperature_is_not_sent_but_another_one_is() {
        let mut core = focus_core::settings::Settings {
            ai_temperature: Some(1.0),
            ..Default::default()
        };
        assert_eq!(AiSettings::from_core(&core).temperature, None);
        core.ai_temperature = Some(0.2);
        assert_eq!(AiSettings::from_core(&core).temperature, Some(0.2));
    }

    #[test]
    fn hidden_and_ops_actions_are_left_out_of_the_visible_list() {
        let mut hidden = focus_ai::prompts::builtin()
            .into_iter()
            .find(|a| a.id == "summarize")
            .expect("built in");
        hidden.enabled = false;
        let mut settings = AiSettings {
            actions: vec![hidden],
            ..AiSettings::default()
        };
        let ids = |s: &AiSettings| {
            s.visible_actions()
                .into_iter()
                .map(|a| a.id)
                .collect::<Vec<_>>()
        };
        assert!(!ids(&settings).contains(&"summarize".to_string()));
        assert!(!ids(&settings).iter().any(|id| id.starts_with("ops-")));
        settings.ops_mode = true;
        assert!(ids(&settings).iter().any(|id| id.starts_with("ops-")));
    }

    #[test]
    fn explicit_code_is_passed_through() {
        let settings = AiSettings {
            reply_language: "ar".into(),
            ..AiSettings::default()
        };
        assert_eq!(settings.response_language("any text"), "ar");
    }

    #[test]
    fn note_language_follows_the_text() {
        let settings = AiSettings {
            reply_language: "note".into(),
            ..AiSettings::default()
        };
        assert_eq!(
            settings.response_language("Le chat est dans la maison et il dort pour les enfants."),
            "fr"
        );
        assert_eq!(
            settings.response_language("The cat is in the house and it sleeps for the children."),
            "en"
        );
    }
}
