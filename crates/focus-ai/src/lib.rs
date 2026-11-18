use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(key: impl Into<String>) -> Self {
        ApiKey(key.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn masked(&self) -> String {
        let tail: String = self
            .0
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("sk-…{tail}")
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.masked())
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub provider: Provider,
    pub fallbacks: bool,
    pub base_url: String,
    pub api_key: ApiKey,
    pub model: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub thinking: bool,
    pub timeout_secs: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("API key missing")]
    MissingKey,
    #[error("API key rejected (401)")]
    InvalidKey,
    #[error("insufficient balance or quota (402)")]
    InsufficientBalance,
    #[error("rate limit reached (429), retry possible in {retry_after_secs} s")]
    RateLimited { retry_after_secs: u32 },
    #[error("the model declined the request ({category})")]
    Refused { category: String },
    #[error("no model set for this provider")]
    NoModel,
    #[error("request rejected ({status}): {message}")]
    BadRequest { status: u16, message: String },
    #[error("server unavailable ({status})")]
    Server { status: u16 },
    #[error("network: {0}")]
    Network(String),
    #[error("unreadable response: {0}")]
    Protocol(String),
    #[error("cancelled")]
    Cancelled,
}

impl Error {
    pub(crate) fn without(self, secret: &str) -> Error {
        let clean = |text: String| {
            if secret.len() >= 8 {
                text.replace(secret, "[key]")
            } else {
                text
            }
        };
        match self {
            Error::BadRequest { status, message } => Error::BadRequest {
                status,
                message: clean(message),
            },
            Error::Network(text) => Error::Network(clean(text)),
            Error::Protocol(text) => Error::Protocol(clean(text)),
            other => other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub json: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub reasoning_tokens: u32,
    pub cache_hit_tokens: u32,
    pub cache_miss_tokens: u32,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Reasoning(String),
    Content(String),
    ToolCall(ToolCall),
    Usage(Usage),
    ToolsDisabled,
    Done { finish_reason: String },
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct Balance {
    pub available: bool,
    pub balances: Vec<(String, String)>,
}

pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
pub const MODEL_FLASH: &str = "deepseek-flash";
pub const MODEL_PRO: &str = "deepseek-v4-pro";

impl Config {
    pub fn for_provider(provider: Provider, api_key: ApiKey) -> Self {
        let spec = provider.spec();
        Config {
            provider,
            base_url: spec.base_url.to_string(),
            model: provider.default_model().unwrap_or_default().to_string(),
            thinking: true,
            ..Config::new(api_key)
        }
    }

    pub fn new(api_key: ApiKey) -> Self {
        Config {
            provider: Provider::DeepSeek,
            fallbacks: true,
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key,
            model: MODEL_FLASH.to_string(),
            temperature: None,
            max_tokens: None,
            thinking: true,
            timeout_secs: 60,
        }
    }
}

pub struct Client {
    config: Config,
    retry_base: Duration,
    thinking_blocks:
        Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<serde_json::Value>>>>,
    listed_model: Arc<std::sync::Mutex<Option<String>>>,
    tools_refused: Arc<std::sync::atomic::AtomicBool>,
    output_limit: Arc<std::sync::Mutex<OutputLimit>>,
}

type OutputLimit = Option<(String, Option<u32>)>;

impl Client {
    pub fn new(config: Config) -> Self {
        Client {
            config,
            retry_base: Duration::from_secs(1),
            thinking_blocks: Arc::default(),
            listed_model: Arc::default(),
            tools_refused: Arc::default(),
            output_limit: Arc::default(),
        }
    }
    pub fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) fn derive(&self, change: impl FnOnce(&mut Config)) -> Client {
        let mut config = self.config.clone();
        change(&mut config);
        Client {
            config,
            retry_base: self.retry_base,
            thinking_blocks: self.thinking_blocks.clone(),
            listed_model: self.listed_model.clone(),
            tools_refused: self.tools_refused.clone(),
            output_limit: self.output_limit.clone(),
        }
    }

    pub fn chat_stream(
        &self,
        request: &ChatRequest,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<Message, Error> {
        self.chat_stream_unscrubbed(request, cancel, on_event)
            .map_err(|error| error.without(self.config.api_key.expose()))
    }

    fn chat_stream_unscrubbed(
        &self,
        request: &ChatRequest,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<Message, Error> {
        use std::sync::atomic::Ordering;
        let bare = || ChatRequest {
            tools: Vec::new(),
            ..request.clone()
        };
        if request.tools.is_empty() {
            return self.send(request, cancel, on_event);
        }
        if self.tools_refused.load(Ordering::Relaxed) {
            return self.send(&bare(), cancel, on_event);
        }
        match self.send(request, cancel, on_event) {
            Err(Error::BadRequest {
                status: 400,
                message,
            }) if self.config.provider.self_hosted()
                && message.to_ascii_lowercase().contains("tool") =>
            {
                self.tools_refused.store(true, Ordering::Relaxed);
                on_event(StreamEvent::ToolsDisabled);
                self.send(&bare(), cancel, on_event)
            }
            other => other,
        }
    }

    fn send(
        &self,
        request: &ChatRequest,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<Message, Error> {
        if self.config.api_key.expose().is_empty() && self.config.provider.spec().key_required {
            return Err(Error::MissingKey);
        }
        let anthropic = self.config.provider == Provider::Anthropic;
        let mut config = self.config.clone();
        if config.model.trim().is_empty() {
            config.model = self.model_without_setting()?;
        }
        if anthropic && let Some(limit) = self.output_limit_of(&config.model) {
            config.max_tokens = Some(
                config
                    .max_tokens
                    .unwrap_or(anthropic::DEFAULT_MAX_TOKENS)
                    .min(limit),
            );
        }
        let body = if anthropic {
            anthropic::body(
                &config,
                request,
                &self
                    .thinking_blocks
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()),
            )
        } else {
            wire::chat_body(&config, request)
        };
        let url = transport::join(
            &self.config.base_url,
            if anthropic {
                "/messages"
            } else {
                "/chat/completions"
            },
        );
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let failure = match self.stream_once(&url, &body, cancel, on_event)? {
                Ok(message) => return Ok(message),
                Err(failure) => failure,
            };
            let wait = match &failure {
                Error::RateLimited { retry_after_secs } => Some(
                    Duration::from_secs(u64::from(*retry_after_secs)).max(self.backoff(attempt)),
                ),
                Error::Server { .. } => Some(self.backoff(attempt)),
                _ => None,
            };
            match wait {
                Some(wait) if attempt < MAX_ATTEMPTS => transport::sleep_cancellable(wait, cancel)?,
                _ => return Err(failure),
            }
        }
    }

    fn model_without_setting(&self) -> Result<String, Error> {
        if let Some(model) = self.config.provider.default_model() {
            return Ok(model.to_string());
        }
        if !self.config.provider.self_hosted() {
            return Err(Error::NoModel);
        }
        let mut cached = self.listed_model.lock().unwrap_or_else(|e| e.into_inner());
        if cached.is_none() {
            *cached = self.models().ok().and_then(|list| {
                list.into_iter()
                    .map(|m| m.id)
                    .find(|id| !id.trim().is_empty())
            });
        }
        cached.clone().ok_or(Error::NoModel)
    }

    fn output_limit_of(&self, model: &str) -> Option<u32> {
        let mut cached = self.output_limit.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((known, limit)) = cached.as_ref()
            && known == model
        {
            return *limit;
        }
        let readable = model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'));
        let limit = readable
            .then(|| {
                self.get(&format!("/models/{model}"))
                    .ok()
                    .and_then(|text| wire::parse_output_limit(&text))
            })
            .flatten();
        *cached = Some((model.to_string(), limit));
        limit
    }

    fn backoff(&self, attempt: u32) -> Duration {
        self.retry_base * 2u32.saturating_pow(attempt - 1)
    }

    fn stream_once(
        &self,
        url: &str,
        body: &str,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(StreamEvent),
    ) -> Result<Result<Message, Error>, Error> {
        let mut response = match transport::start(
            &self.config,
            transport::Method::Post,
            url,
            Some(body.to_string()),
            cancel,
        ) {
            Ok(response) => response,
            Err(error) => return Ok(Err(error)),
        };
        if !(200..300).contains(&response.status) {
            let text = response.read_all(cancel, MAX_ERROR_BODY)?;
            return Ok(Err(wire::status_error(
                response.status,
                response.retry_after,
                &text,
            )));
        }
        let mut parser = sse::Parser::new();
        let mut accumulator = if self.config.provider == Provider::Anthropic {
            anthropic::Accumulator::new().into()
        } else {
            stream::Any::default()
        };
        let mut emitted = false;
        let mut failure: Option<Error> = None;
        let idle = Duration::from_secs(u64::from(self.config.timeout_secs.max(1)));
        'read: loop {
            let mut payloads: Vec<String> = Vec::new();
            match response.next_chunk(cancel, idle) {
                Ok(Some(chunk)) => parser.push(&chunk, &mut |data| payloads.push(data)),
                Ok(None) => parser.finish(&mut |data| payloads.push(data)),
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
            for data in payloads {
                let step = accumulator.feed(&data, &mut |event| {
                    emitted = true;
                    on_event(event);
                });
                match step {
                    Ok(stream::Step::Continue) => {}
                    Ok(stream::Step::Done) => break 'read,
                    Err(error) => return if emitted { Err(error) } else { Ok(Err(error)) },
                }
            }
            if response.is_finished() {
                break;
            }
        }
        if let Some(error) = failure {
            if !accumulator.finished() {
                return if emitted { Err(error) } else { Ok(Err(error)) };
            }
        } else if !accumulator.finished() {
            let error = Error::Network("stream interrupted before the end of the response".into());
            return if emitted { Err(error) } else { Ok(Err(error)) };
        }
        if let Some((id, blocks)) = accumulator.thinking_to_keep() {
            self.thinking_blocks
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(id, blocks);
        }
        Ok(Ok(accumulator.finish(on_event)))
    }

    pub fn models(&self) -> Result<Vec<ModelInfo>, Error> {
        self.get("/models")
            .and_then(|text| wire::parse_models(&text))
            .map_err(|error| error.without(self.config.api_key.expose()))
    }

    pub fn balance(&self) -> Result<Balance, Error> {
        self.get("/user/balance")
            .and_then(|text| wire::parse_balance(&text))
            .map_err(|error| error.without(self.config.api_key.expose()))
    }

    fn get(&self, path: &str) -> Result<String, Error> {
        if self.config.api_key.expose().is_empty() && self.config.provider.spec().key_required {
            return Err(Error::MissingKey);
        }
        let cancel = Cancel::new();
        let url = transport::join(&self.config.base_url, path);
        let mut response =
            transport::start(&self.config, transport::Method::Get, &url, None, &cancel)?;
        let text = response.read_all(&cancel, MAX_JSON_BODY)?;
        if !(200..300).contains(&response.status) {
            return Err(wire::status_error(
                response.status,
                response.retry_after,
                &text,
            ));
        }
        Ok(text)
    }
}

const MAX_ATTEMPTS: u32 = 3;
const MAX_ERROR_BODY: usize = 16 * 1024;
const MAX_JSON_BODY: usize = 4 * 1024 * 1024;

pub use pricing::{estimate_cost_usd, estimate_cost_usd_at, estimate_tokens};
pub use provider::Provider;

pub mod agent;
pub mod assist;
pub mod history;
pub mod keystore;
pub mod mcp;
pub mod prompts;
pub mod provider;
pub mod redact;

mod anthropic;
mod pricing;
mod sse;
mod stream;
mod timefmt;
mod transport;
mod wire;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod testserver;
