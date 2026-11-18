#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    DeepSeek,
    OpenAi,
    Ollama,
    LmStudio,
    Vllm,
    Anthropic,
    Compatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetField {
    MaxTokens,
    MaxCompletionTokens,
}

pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-opus-5-5";

#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub id: &'static str,
    pub label: &'static str,
    pub base_url: &'static str,
    pub key_required: bool,
    pub key_env: Option<&'static str>,
    pub local: bool,
    pub thinking_switch: bool,
    pub budget_field: BudgetField,
    pub balance: bool,
}

impl Provider {
    pub const ALL: [Provider; 7] = [
        Provider::DeepSeek,
        Provider::OpenAi,
        Provider::Ollama,
        Provider::LmStudio,
        Provider::Vllm,
        Provider::Anthropic,
        Provider::Compatible,
    ];

    pub fn default_model(self) -> Option<&'static str> {
        match self {
            Provider::DeepSeek => Some(crate::MODEL_FLASH),
            Provider::Anthropic => Some(ANTHROPIC_DEFAULT_MODEL),
            _ => None,
        }
    }

    pub fn self_hosted(self) -> bool {
        matches!(
            self,
            Provider::Ollama | Provider::LmStudio | Provider::Vllm | Provider::Compatible
        )
    }

    pub fn spec(self) -> Spec {
        match self {
            Provider::DeepSeek => Spec {
                id: "deepseek",
                label: "DeepSeek",
                base_url: "https://api.deepseek.com",
                key_required: true,
                key_env: Some("DEEPSEEK_API_KEY"),
                local: false,
                thinking_switch: true,
                budget_field: BudgetField::MaxTokens,
                balance: true,
            },
            Provider::OpenAi => Spec {
                id: "openai",
                label: "OpenAI",
                base_url: "https://api.openai.com/v1",
                key_required: true,
                key_env: Some("OPENAI_API_KEY"),
                local: false,
                thinking_switch: false,
                budget_field: BudgetField::MaxCompletionTokens,
                balance: false,
            },
            Provider::Ollama => Spec {
                id: "ollama",
                label: "Ollama",
                base_url: "http://localhost:11434/v1",
                key_required: false,
                key_env: None,
                local: true,
                thinking_switch: false,
                budget_field: BudgetField::MaxTokens,
                balance: false,
            },
            Provider::LmStudio => Spec {
                id: "lmstudio",
                label: "LM Studio",
                base_url: "http://localhost:1234/v1",
                key_required: false,
                key_env: None,
                local: true,
                thinking_switch: false,
                budget_field: BudgetField::MaxTokens,
                balance: false,
            },
            Provider::Vllm => Spec {
                id: "vllm",
                label: "vLLM",
                base_url: "http://localhost:8000/v1",
                key_required: false,
                key_env: None,
                local: true,
                thinking_switch: false,
                budget_field: BudgetField::MaxTokens,
                balance: false,
            },
            Provider::Anthropic => Spec {
                id: "anthropic",
                label: "Anthropic",
                base_url: "https://api.anthropic.com/v1",
                key_required: true,
                key_env: Some("ANTHROPIC_API_KEY"),
                local: false,
                thinking_switch: false,
                budget_field: BudgetField::MaxTokens,
                balance: false,
            },
            Provider::Compatible => Spec {
                id: "compatible",
                label: "OpenAI-compatible server",
                base_url: "http://localhost:8000/v1",
                key_required: false,
                key_env: None,
                local: false,
                thinking_switch: false,
                budget_field: BudgetField::MaxTokens,
                balance: false,
            },
        }
    }

    pub fn from_id(id: &str) -> Option<Provider> {
        Provider::ALL.into_iter().find(|p| p.spec().id == id)
    }

    pub fn is_loopback(base_url: &str) -> bool {
        let rest = base_url.split("://").nth(1).unwrap_or(base_url);
        let authority = rest.split('/').next().unwrap_or("");
        let host = match authority.strip_prefix('[') {
            Some(inside) => inside.split(']').next().unwrap_or(""),
            None => authority.split(':').next().unwrap_or(""),
        };
        matches!(host, "localhost" | "127.0.0.1" | "::1")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_is_found_by_its_identifier_and_has_a_base_url() {
        for provider in Provider::ALL {
            assert_eq!(Provider::from_id(provider.spec().id), Some(provider));
            assert!(provider.spec().base_url.starts_with("http"));
        }
        assert_eq!(Provider::from_id("nobody"), None);
    }

    #[test]
    fn only_deepseek_has_a_balance_and_a_thinking_switch_and_local_servers_need_no_key() {
        assert!(
            Provider::ALL
                .iter()
                .filter(|p| p.spec().balance)
                .eq([&Provider::DeepSeek])
        );
        assert!(
            Provider::ALL
                .iter()
                .filter(|p| p.spec().thinking_switch)
                .eq([&Provider::DeepSeek])
        );
        for provider in [Provider::Ollama, Provider::LmStudio, Provider::Vllm] {
            assert!(!provider.spec().key_required && provider.spec().local);
        }
        assert!(Provider::OpenAi.spec().key_required);
    }

    #[test]
    fn loopback_addresses_are_recognised() {
        for url in [
            "http://localhost:1234/v1",
            "http://127.0.0.1:8000/v1",
            "http://[::1]:11434/v1",
        ] {
            assert!(Provider::is_loopback(url), "{url}");
        }
        assert!(
            !Provider::is_loopback("https://api.openai.com/v1")
                && !Provider::is_loopback("http://192.168.1.5:8000/v1")
        );
    }
}
