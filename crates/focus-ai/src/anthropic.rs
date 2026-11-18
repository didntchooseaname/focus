use std::collections::HashMap;

use serde_json::{Value, json};

use crate::{ChatRequest, Config, Error, Message, Role, StreamEvent, ToolCall, Usage};

pub const VERSION: &str = "2026-10-21";
pub const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
pub(crate) const DEFAULT_MAX_TOKENS: u32 = 64_000;

pub fn body(
    config: &Config,
    request: &ChatRequest,
    thinking: &HashMap<String, Vec<Value>>,
) -> String {
    let mut system: Vec<String> = Vec::new();
    let mut messages: Vec<Value> = Vec::new();
    let mut results: Vec<Value> = Vec::new();
    let flush = |results: &mut Vec<Value>, messages: &mut Vec<Value>| {
        if !results.is_empty() {
            messages.push(json!({ "role": "user", "content": std::mem::take(results) }));
        }
    };
    for message in &request.messages {
        match message.role {
            Role::System => system.push(message.content.clone()),
            Role::Tool => results.push(json!({ "type": "tool_result", "tool_use_id": message.tool_call_id.clone().unwrap_or_default(), "content": message.content })),
            Role::User => {
                flush(&mut results, &mut messages);
                messages.push(json!({ "role": "user", "content": message.content }));
            }
            Role::Assistant => {
                flush(&mut results, &mut messages);
                let mut blocks: Vec<Value> = message.tool_calls.first().and_then(|call| thinking.get(&call.id)).cloned().unwrap_or_default();
                if !message.content.is_empty() {
                    blocks.push(json!({ "type": "text", "text": message.content }));
                }
                for call in &message.tool_calls {
                    let input: Value = serde_json::from_str(&call.arguments).unwrap_or_else(|_| json!({}));
                    blocks.push(json!({ "type": "tool_use", "id": call.id, "name": call.name, "input": input }));
                }
                if !blocks.is_empty() {
                    messages.push(json!({ "role": "assistant", "content": blocks }));
                }
            }
        }
    }
    flush(&mut results, &mut messages);
    if request.json {
        system.push("Reply with a single JSON object and nothing else.".into());
    }
    let mut body = json!({
        "model": config.model,
        "max_tokens": config.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "stream": true,
        "thinking": { "type": "adaptive", "display": "summarized" },
        "output_config": { "effort": if config.thinking { "high" } else { "low" } },
        "messages": messages,
    });
    let map = body.as_object_mut().expect("object literal");
    if !system.is_empty() {
        map.insert("system".into(), json!([{ "type": "text", "text": system.join("\n\n"), "cache_control": { "type": "ephemeral" } }]));
    }
    if !request.tools.is_empty() {
        let tools: Vec<Value> = request.tools.iter().map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.parameters, "eager_input_streaming": true })).collect();
        map.insert("tools".into(), Value::Array(tools));
    }
    if config.fallbacks {
        map.insert("fallbacks".into(), json!("default"));
    }
    body.to_string()
}

enum Block {
    Text(String),
    Thinking {
        text: String,
        signature: String,
    },
    Redacted(Value),
    Tool {
        id: String,
        name: String,
        input: String,
    },
    Other,
}

pub struct Accumulator {
    blocks: Vec<Block>,
    content: String,
    reasoning: String,
    calls: Vec<ToolCall>,
    usage: Usage,
    stop: Option<String>,
    done: bool,
}

impl Accumulator {
    pub fn new() -> Accumulator {
        Accumulator {
            blocks: Vec::new(),
            content: String::new(),
            reasoning: String::new(),
            calls: Vec::new(),
            usage: Usage::default(),
            stop: None,
            done: false,
        }
    }

    pub fn finished(&self) -> bool {
        self.done || self.stop.is_some()
    }

    fn add_usage(&mut self, usage: &Value) {
        let number = |key: &str| {
            usage
                .get(key)
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(u64::from(u32::MAX)) as u32
        };
        let (input, read, created) = (
            number("input_tokens"),
            number("cache_read_input_tokens"),
            number("cache_creation_input_tokens"),
        );
        if usage.get("input_tokens").is_some() {
            self.usage.prompt_tokens = input + read + created;
            self.usage.cache_hit_tokens = read;
            self.usage.cache_miss_tokens = input + created;
        }
        if usage.get("output_tokens").is_some() {
            self.usage.completion_tokens = number("output_tokens");
        }
    }

    pub fn feed(
        &mut self,
        data: &str,
        emit: &mut dyn FnMut(StreamEvent),
    ) -> Result<crate::stream::Step, Error> {
        use crate::stream::Step;
        let data = data.trim();
        if data.is_empty() {
            return Ok(Step::Continue);
        }
        let event: Value = serde_json::from_str(data)
            .map_err(|e| Error::Protocol(format!("invalid JSON event: {e}")))?;
        match event.get("type").and_then(Value::as_str).unwrap_or("") {
            "message_start" => {
                if let Some(usage) = event.pointer("/message/usage") {
                    self.add_usage(usage);
                }
            }
            "content_block_start" => {
                let block = event.get("content_block").cloned().unwrap_or(Value::Null);
                self.blocks
                    .push(match block.get("type").and_then(Value::as_str) {
                        Some("text") => Block::Text(String::new()),
                        Some("thinking") => Block::Thinking {
                            text: String::new(),
                            signature: String::new(),
                        },
                        Some("redacted_thinking") => Block::Redacted(block),
                        Some("tool_use") => Block::Tool {
                            id: block
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            name: block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            input: String::new(),
                        },
                        _ => Block::Other,
                    });
            }
            "content_block_delta" => {
                let delta = event.get("delta").cloned().unwrap_or(Value::Null);
                match (
                    self.blocks.last_mut(),
                    delta.get("type").and_then(Value::as_str),
                ) {
                    (Some(Block::Text(text)), Some("text_delta")) => {
                        let piece = delta.get("text").and_then(Value::as_str).unwrap_or("");
                        text.push_str(piece);
                        self.content.push_str(piece);
                        if !piece.is_empty() {
                            emit(StreamEvent::Content(piece.to_string()));
                        }
                    }
                    (Some(Block::Thinking { text, .. }), Some("thinking_delta")) => {
                        let piece = delta.get("thinking").and_then(Value::as_str).unwrap_or("");
                        text.push_str(piece);
                        self.reasoning.push_str(piece);
                        if !piece.is_empty() {
                            emit(StreamEvent::Reasoning(piece.to_string()));
                        }
                    }
                    (Some(Block::Thinking { signature, .. }), Some("signature_delta")) => signature
                        .push_str(delta.get("signature").and_then(Value::as_str).unwrap_or("")),
                    (Some(Block::Tool { input, .. }), Some("input_json_delta")) => input.push_str(
                        delta
                            .get("partial_json")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                    ),
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let Some(Block::Tool { id, name, input }) = self.blocks.last() {
                    let arguments = if input.trim().is_empty() {
                        "{}".to_string()
                    } else {
                        input.clone()
                    };
                    let call = ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments,
                    };
                    emit(StreamEvent::ToolCall(call.clone()));
                    self.calls.push(call);
                }
            }
            "message_delta" => {
                if let Some(usage) = event.get("usage") {
                    self.add_usage(usage);
                }
                if let Some(reason) = event.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    if reason == "refusal" {
                        let category = event
                            .pointer("/delta/stop_details/category")
                            .and_then(Value::as_str)
                            .unwrap_or("unspecified")
                            .to_string();
                        return Err(Error::Refused { category });
                    }
                    self.stop = Some(reason.to_string());
                }
            }
            "message_stop" => {
                self.done = true;
                return Ok(Step::Done);
            }
            "error" => {
                let text = event
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("error without a message");
                return Err(Error::Protocol(format!("error in the stream: {text}")));
            }
            _ => {}
        }
        Ok(Step::Continue)
    }

    pub fn thinking_to_keep(&self) -> Option<(String, Vec<Value>)> {
        let first = self.calls.first()?.id.clone();
        let kept: Vec<Value> = self
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Thinking { text, signature } => {
                    Some(json!({ "type": "thinking", "thinking": text, "signature": signature }))
                }
                Block::Redacted(value) => Some(value.clone()),
                _ => None,
            })
            .collect();
        (!kept.is_empty()).then_some((first, kept))
    }

    pub fn finish(self, emit: &mut dyn FnMut(StreamEvent)) -> Message {
        emit(StreamEvent::Usage(self.usage.clone()));
        let finish_reason = match self.stop.as_deref() {
            Some("tool_use") => "tool_calls",
            Some("max_tokens") => "length",
            Some("pause_turn") => "length",
            _ => "stop",
        };
        emit(StreamEvent::Done {
            finish_reason: finish_reason.to_string(),
        });
        Message {
            role: Role::Assistant,
            content: self.content,
            reasoning: if self.reasoning.is_empty() {
                None
            } else {
                Some(self.reasoning)
            },
            tool_calls: self.calls,
            tool_call_id: None,
        }
    }
}
