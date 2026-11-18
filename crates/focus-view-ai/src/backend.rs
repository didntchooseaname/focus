use focus_ai::agent::{self, AgentEvent, ToolHost};
use focus_ai::{ApiKey, Cancel, Client, Config, Error, Message, Provider, Usage};

pub const MAX_STEPS: u32 = 8;

pub struct RunOptions {
    pub base_url: String,
    pub model: String,
    pub thinking: bool,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub max_steps: u32,
}

pub trait ChatBackend: Send + Sync {
    fn run(
        &self,
        options: &RunOptions,
        messages: &mut Vec<Message>,
        host: &dyn ToolHost,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(AgentEvent),
    ) -> Result<Usage, Error>;

    fn models(&self) -> Result<Vec<String>, Error> {
        Err(Error::Network("model list not available".into()))
    }
}

pub struct DeepSeek {
    provider: Provider,
    key: ApiKey,
    base_url: String,
}

impl DeepSeek {
    pub fn new(key: ApiKey, base_url: impl Into<String>) -> DeepSeek {
        DeepSeek {
            provider: Provider::DeepSeek,
            key,
            base_url: base_url.into(),
        }
    }

    pub fn for_provider(
        provider: Provider,
        key: Option<ApiKey>,
        base_url: impl Into<String>,
    ) -> DeepSeek {
        DeepSeek {
            provider,
            key: key.unwrap_or_else(|| ApiKey::new("")),
            base_url: base_url.into(),
        }
    }

    pub fn connect_provider(enabled: bool, provider: Provider, base_url: &str) -> Option<DeepSeek> {
        if !enabled {
            return None;
        }
        if provider == Provider::DeepSeek {
            return DeepSeek::connect(true, base_url);
        }
        let key = focus_ai::keystore::provider_key::load(provider);
        if key.is_none() && provider.spec().key_required {
            return None;
        }
        Some(DeepSeek::for_provider(provider, key, base_url))
    }

    pub fn connect(enabled: bool, base_url: &str) -> Option<DeepSeek> {
        if !enabled {
            return None;
        }
        let from_env = std::env::var("DEEPSEEK_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty())
            .map(|k| ApiKey::new(k.trim()));
        let key = from_env.or_else(|| focus_ai::keystore::load().map(|(key, _)| key))?;
        Some(DeepSeek::new(key, base_url))
    }
}

impl ChatBackend for DeepSeek {
    fn models(&self) -> Result<Vec<String>, Error> {
        let mut config = Config::for_provider(self.provider, self.key.clone());
        config.base_url = self.base_url.clone();
        Ok(Client::new(config)
            .models()?
            .into_iter()
            .map(|m| m.id)
            .collect())
    }

    fn run(
        &self,
        options: &RunOptions,
        messages: &mut Vec<Message>,
        host: &dyn ToolHost,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(AgentEvent),
    ) -> Result<Usage, Error> {
        let mut config = Config::for_provider(self.provider, self.key.clone());
        if !options.base_url.trim().is_empty() {
            config.base_url = options.base_url.trim().to_string();
        } else {
            config.base_url = self.base_url.clone();
        }
        config.model = options.model.clone();
        config.thinking = options.thinking;
        config.temperature = options.temperature;
        config.max_tokens = options.max_tokens;
        agent::run(
            &Client::new(config),
            messages,
            host,
            options.max_steps.max(1),
            cancel,
            on_event,
        )
    }
}
