use serde_json::{Value, json};

use crate::provider::{BudgetField, Provider};
use crate::{Balance, ChatRequest, Config, Error, Message, ModelInfo, Role};

pub fn chat_body(config: &Config, request: &ChatRequest) -> String {
    let send_reasoning = !request.tools.is_empty() && config.provider == Provider::DeepSeek;
    let messages: Vec<Value> = request
        .messages
        .iter()
        .map(|m| message(m, send_reasoning))
        .collect();
    let spec = config.provider.spec();
    let mut body = json!({
        "model": config.model,
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    let map = body.as_object_mut().expect("object literal");
    if spec.thinking_switch {
        map.insert(
            "thinking".into(),
            json!({ "type": if config.thinking { "enabled" } else { "disabled" } }),
        );
    }
    if let Some(max_tokens) = config.max_tokens {
        let field = match spec.budget_field {
            BudgetField::MaxTokens => "max_tokens",
            BudgetField::MaxCompletionTokens => "max_completion_tokens",
        };
        map.insert(field.into(), json!(max_tokens));
    }
    if let Some(temperature) = config
        .temperature
        .filter(|_| !spec.thinking_switch || !config.thinking)
    {
        map.insert("temperature".into(), json!(temperature));
    }
    if !request.tools.is_empty() {
        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.parameters } }))
            .collect();
        map.insert("tools".into(), Value::Array(tools));
    }
    if request.json {
        map.insert("response_format".into(), json!({ "type": "json_object" }));
    }
    body.to_string()
}

fn message(message: &Message, send_reasoning: bool) -> Value {
    let role = match message.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };
    let mut value = json!({ "role": role, "content": message.content });
    let map = value.as_object_mut().expect("object literal");
    if message.role == Role::Assistant {
        if send_reasoning && let Some(reasoning) = &message.reasoning {
            map.insert("reasoning_content".into(), json!(reasoning));
        }
        if !message.tool_calls.is_empty() {
            let calls: Vec<Value> = message
                .tool_calls
                .iter()
                .map(|c| json!({ "id": c.id, "type": "function", "function": { "name": c.name, "arguments": c.arguments } }))
                .collect();
            map.insert("tool_calls".into(), Value::Array(calls));
        }
    }
    if let Some(id) = &message.tool_call_id {
        map.insert("tool_call_id".into(), json!(id));
    }
    value
}

pub fn status_error(status: u16, retry_after: Option<u32>, body: &str) -> Error {
    match status {
        401 => Error::InvalidKey,
        402 => Error::InsufficientBalance,
        429 => Error::RateLimited {
            retry_after_secs: retry_after.unwrap_or(0),
        },
        500..=599 => Error::Server { status },
        _ => Error::BadRequest {
            status,
            message: error_message(body),
        },
    }
}

fn error_message(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body)
        && let Some(text) = value.pointer("/error/message").and_then(Value::as_str)
    {
        return text.to_string();
    }
    let trimmed = body.trim();
    if trimmed.is_empty() {
        "empty response body".into()
    } else {
        trimmed.chars().take(300).collect()
    }
}

pub fn parse_models(text: &str) -> Result<Vec<ModelInfo>, Error> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| Error::Protocol(format!("model list: {e}")))?;
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Protocol("model list: missing `data` field".into()))?;
    Ok(data
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str))
        .map(|id| ModelInfo { id: id.to_string() })
        .collect())
}

pub fn parse_output_limit(text: &str) -> Option<u32> {
    let value: Value = serde_json::from_str(text).ok()?;
    value
        .get("max_tokens")
        .and_then(Value::as_u64)
        .filter(|limit| *limit > 0)
        .and_then(|limit| u32::try_from(limit).ok())
}

pub fn parse_balance(text: &str) -> Result<Balance, Error> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| Error::Protocol(format!("balance: {e}")))?;
    let available = value
        .get("is_available")
        .and_then(Value::as_bool)
        .ok_or_else(|| Error::Protocol("balance: missing `is_available` field".into()))?;
    let infos = value
        .get("balance_infos")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Protocol("balance: missing `balance_infos` field".into()))?;
    let balances = infos
        .iter()
        .filter_map(|info| {
            Some((
                info.get("currency")?.as_str()?.to_string(),
                info.get("total_balance")?.as_str()?.to_string(),
            ))
        })
        .collect();
    Ok(Balance {
        available,
        balances,
    })
}
