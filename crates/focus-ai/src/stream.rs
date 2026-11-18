use serde_json::Value;

use crate::{Error, Message, Role, StreamEvent, ToolCall, Usage};

pub enum Step {
    Continue,
    Done,
}

#[derive(Default)]
struct PendingCall {
    id: String,
    name: String,
    arguments: String,
    emitted: bool,
}

#[derive(Default)]
pub struct Accumulator {
    content: String,
    reasoning: String,
    calls: Vec<(u64, PendingCall)>,
    finish_reason: Option<String>,
    usage: Option<Usage>,
}

impl Accumulator {
    pub fn finished(&self) -> bool {
        self.finish_reason.is_some()
    }

    pub fn feed(&mut self, data: &str, emit: &mut dyn FnMut(StreamEvent)) -> Result<Step, Error> {
        let data = data.trim();
        if data == "[DONE]" {
            return Ok(Step::Done);
        }
        if data.is_empty() {
            return Ok(Step::Continue);
        }
        let chunk: Value = serde_json::from_str(data)
            .map_err(|e| Error::Protocol(format!("invalid JSON fragment: {e}")))?;
        if let Some(error) = chunk.get("error") {
            let text = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("error without a message");
            return Err(Error::Protocol(format!("error in the stream: {text}")));
        }
        if let Some(usage) = chunk.get("usage").filter(|u| !u.is_null()) {
            self.usage = Some(parse_usage(usage));
        }
        let Some(choice) = chunk.get("choices").and_then(|c| c.get(0)) else {
            return Ok(Step::Continue);
        };
        if let Some(delta) = choice.get("delta") {
            let reasoning = delta
                .get("reasoning_content")
                .or_else(|| delta.get("reasoning"))
                .and_then(Value::as_str);
            if let Some(text) = reasoning.filter(|t| !t.is_empty()) {
                self.reasoning.push_str(text);
                emit(StreamEvent::Reasoning(text.to_string()));
            }
            if let Some(text) = delta
                .get("content")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
            {
                self.content.push_str(text);
                emit(StreamEvent::Content(text.to_string()));
            }
            if let Some(fragments) = delta.get("tool_calls").and_then(Value::as_array) {
                for fragment in fragments {
                    self.tool_fragment(fragment);
                }
            }
        }
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish_reason = Some(reason.to_string());
            self.flush_calls(emit);
        }
        Ok(Step::Continue)
    }

    fn tool_fragment(&mut self, fragment: &Value) {
        let index = fragment.get("index").and_then(Value::as_u64).unwrap_or(0);
        let position = match self.calls.iter().position(|(i, _)| *i == index) {
            Some(position) => position,
            None => {
                self.calls.push((index, PendingCall::default()));
                self.calls.len() - 1
            }
        };
        let call = &mut self.calls[position].1;
        if let Some(id) = fragment.get("id").and_then(Value::as_str) {
            call.id.push_str(id);
        }
        if let Some(function) = fragment.get("function") {
            if let Some(name) = function.get("name").and_then(Value::as_str) {
                call.name.push_str(name);
            }
            if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                call.arguments.push_str(arguments);
            }
        }
    }

    fn flush_calls(&mut self, emit: &mut dyn FnMut(StreamEvent)) {
        for (index, call) in &mut self.calls {
            if call.emitted {
                continue;
            }
            call.emitted = true;
            emit(StreamEvent::ToolCall(complete(*index, call)));
        }
    }

    pub fn finish(mut self, emit: &mut dyn FnMut(StreamEvent)) -> Message {
        self.flush_calls(emit);
        if let Some(usage) = self.usage.take() {
            emit(StreamEvent::Usage(usage));
        }
        emit(StreamEvent::Done {
            finish_reason: self.finish_reason.clone().unwrap_or_else(|| "stop".into()),
        });
        let tool_calls = self
            .calls
            .iter()
            .map(|(index, call)| complete(*index, call))
            .collect();
        Message {
            role: Role::Assistant,
            content: self.content,
            reasoning: if self.reasoning.is_empty() {
                None
            } else {
                Some(self.reasoning)
            },
            tool_calls,
            tool_call_id: None,
        }
    }
}

fn complete(index: u64, call: &PendingCall) -> ToolCall {
    let arguments = if call.arguments.trim().is_empty() {
        "{}".to_string()
    } else {
        call.arguments.clone()
    };
    let id = if call.id.is_empty() {
        format!("call_{index}")
    } else {
        call.id.clone()
    };
    ToolCall {
        id,
        name: call.name.clone(),
        arguments,
    }
}

fn parse_usage(usage: &Value) -> Usage {
    let number = |value: Option<&Value>| {
        value
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(u64::from(u32::MAX)) as u32
    };
    let prompt = number(usage.get("prompt_tokens"));
    let hit = match usage.get("prompt_cache_hit_tokens") {
        Some(v) => number(Some(v)),
        None => number(usage.pointer("/prompt_tokens_details/cached_tokens")),
    };
    let miss = match usage.get("prompt_cache_miss_tokens") {
        Some(v) => number(Some(v)),
        None => prompt.saturating_sub(hit),
    };
    Usage {
        prompt_tokens: prompt,
        completion_tokens: number(usage.get("completion_tokens")),
        reasoning_tokens: number(usage.pointer("/completion_tokens_details/reasoning_tokens")),
        cache_hit_tokens: hit,
        cache_miss_tokens: miss,
    }
}

pub enum Any {
    OpenAi(Accumulator),
    Anthropic(crate::anthropic::Accumulator),
}

impl Default for Any {
    fn default() -> Any {
        Any::OpenAi(Accumulator::default())
    }
}

impl From<crate::anthropic::Accumulator> for Any {
    fn from(accumulator: crate::anthropic::Accumulator) -> Any {
        Any::Anthropic(accumulator)
    }
}

impl Any {
    pub fn finished(&self) -> bool {
        match self {
            Any::OpenAi(a) => a.finished(),
            Any::Anthropic(a) => a.finished(),
        }
    }

    pub fn feed(&mut self, data: &str, emit: &mut dyn FnMut(StreamEvent)) -> Result<Step, Error> {
        match self {
            Any::OpenAi(a) => a.feed(data, emit),
            Any::Anthropic(a) => a.feed(data, emit),
        }
    }

    pub fn thinking_to_keep(&self) -> Option<(String, Vec<Value>)> {
        match self {
            Any::OpenAi(_) => None,
            Any::Anthropic(a) => a.thinking_to_keep(),
        }
    }

    pub fn finish(self, emit: &mut dyn FnMut(StreamEvent)) -> Message {
        match self {
            Any::OpenAi(a) => a.finish(emit),
            Any::Anthropic(a) => a.finish(emit),
        }
    }
}
