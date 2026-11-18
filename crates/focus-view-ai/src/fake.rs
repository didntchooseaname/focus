use std::collections::VecDeque;
use std::time::Duration;

use focus_ai::agent::{AgentEvent, ToolHost, ToolOutcome};
use focus_ai::{Cancel, Error, Message, Role, StreamEvent, ToolCall, Usage};
use parking_lot::Mutex;

use crate::backend::{ChatBackend, RunOptions};

#[derive(Clone, Debug)]
pub enum Beat {
    Reasoning(String),
    Content(String),
    Tool { name: String, arguments: String },
    Usage(Usage),
    Pause(Duration),
    Fail(FakeError),
}

#[derive(Clone, Debug)]
pub enum FakeError {
    MissingKey,
    InvalidKey,
    InsufficientBalance,
    RateLimited(u32),
    Server(u16),
    Network(String),
}

impl FakeError {
    fn build(&self) -> Error {
        match self {
            FakeError::MissingKey => Error::MissingKey,
            FakeError::InvalidKey => Error::InvalidKey,
            FakeError::InsufficientBalance => Error::InsufficientBalance,
            FakeError::RateLimited(secs) => Error::RateLimited {
                retry_after_secs: *secs,
            },
            FakeError::Server(status) => Error::Server { status: *status },
            FakeError::Network(message) => Error::Network(message.clone()),
        }
    }
}

pub struct FakeBackend {
    scripts: Mutex<VecDeque<Vec<Beat>>>,
    last: Mutex<Vec<Beat>>,
    chunk_pause: Duration,
    models: Mutex<Result<Vec<String>, FakeError>>,
}

impl FakeBackend {
    pub fn new(scripts: Vec<Vec<Beat>>, chunk_pause: Duration) -> FakeBackend {
        FakeBackend {
            scripts: Mutex::new(scripts.into()),
            last: Mutex::new(Vec::new()),
            chunk_pause,
            models: Mutex::new(Err(FakeError::Network("no model list".into()))),
        }
    }

    pub fn with_models(self, models: Vec<String>) -> FakeBackend {
        *self.models.lock() = Ok(models);
        self
    }

    fn next_script(&self) -> Vec<Beat> {
        let mut scripts = self.scripts.lock();
        match scripts.pop_front() {
            Some(script) => {
                *self.last.lock() = script.clone();
                script
            }
            None => self.last.lock().clone(),
        }
    }
}

fn sleep(total: Duration, cancel: &Cancel) -> Result<(), Error> {
    let step = Duration::from_millis(20);
    let mut waited = Duration::ZERO;
    while waited < total {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        std::thread::sleep(step.min(total - waited));
        waited += step;
    }
    Ok(())
}

fn pieces(text: &str, size: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for word in text.split_inclusive(char::is_whitespace) {
        current.push_str(word);
        if current.chars().count() >= size {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

impl ChatBackend for FakeBackend {
    fn models(&self) -> Result<Vec<String>, Error> {
        self.models.lock().clone().map_err(|e| e.build())
    }

    fn run(
        &self,
        _options: &RunOptions,
        messages: &mut Vec<Message>,
        host: &dyn ToolHost,
        cancel: &Cancel,
        on_event: &mut dyn FnMut(AgentEvent),
    ) -> Result<Usage, Error> {
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut usage = Usage::default();
        let mut next_call = 0;
        for beat in self.next_script() {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            match beat {
                Beat::Reasoning(text) => {
                    for piece in pieces(&text, 24) {
                        reasoning.push_str(&piece);
                        on_event(AgentEvent::Stream(StreamEvent::Reasoning(piece)));
                        sleep(self.chunk_pause, cancel)?;
                    }
                }
                Beat::Content(text) => {
                    for piece in pieces(&text, 24) {
                        content.push_str(&piece);
                        on_event(AgentEvent::Stream(StreamEvent::Content(piece)));
                        sleep(self.chunk_pause, cancel)?;
                    }
                }
                Beat::Tool { name, arguments } => {
                    next_call += 1;
                    let call = ToolCall {
                        id: format!("call_{next_call}"),
                        name,
                        arguments,
                    };
                    on_event(AgentEvent::Stream(StreamEvent::ToolCall(call.clone())));
                    messages.push(Message {
                        role: Role::Assistant,
                        content: std::mem::take(&mut content),
                        reasoning: (!reasoning.is_empty()).then(|| std::mem::take(&mut reasoning)),
                        tool_calls: vec![call.clone()],
                        tool_call_id: None,
                    });
                    on_event(AgentEvent::ToolStarted(call.clone()));
                    let outcome = host.call(&call, cancel);
                    let text = match &outcome {
                        ToolOutcome::Ok(text) => text.clone(),
                        ToolOutcome::Error(text) => format!("Error: {text}"),
                        ToolOutcome::Denied => "The user refused this action.".to_string(),
                    };
                    messages.push(Message {
                        role: Role::Tool,
                        content: text,
                        reasoning: None,
                        tool_calls: Vec::new(),
                        tool_call_id: Some(call.id.clone()),
                    });
                    let denied = matches!(outcome, ToolOutcome::Denied);
                    on_event(AgentEvent::ToolFinished {
                        call_id: call.id,
                        outcome,
                    });
                    if denied {
                        on_event(AgentEvent::TurnFinished);
                        return Ok(usage);
                    }
                }
                Beat::Usage(counted) => {
                    usage = counted.clone();
                    on_event(AgentEvent::Stream(StreamEvent::Usage(counted)));
                }
                Beat::Pause(duration) => sleep(duration, cancel)?,
                Beat::Fail(error) => return Err(error.build()),
            }
        }
        messages.push(Message {
            role: Role::Assistant,
            content,
            reasoning: (!reasoning.is_empty()).then_some(reasoning),
            tool_calls: Vec::new(),
            tool_call_id: None,
        });
        on_event(AgentEvent::Stream(StreamEvent::Done {
            finish_reason: "stop".into(),
        }));
        on_event(AgentEvent::TurnFinished);
        Ok(usage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoTools;
    impl ToolHost for NoTools {
        fn specs(&self) -> Vec<focus_ai::ToolSpec> {
            Vec::new()
        }
        fn call(&self, _: &ToolCall, _: &Cancel) -> ToolOutcome {
            ToolOutcome::Ok("done".into())
        }
    }

    fn options() -> RunOptions {
        RunOptions {
            base_url: String::new(),
            model: "deepseek-flash".into(),
            thinking: false,
            temperature: None,
            max_tokens: None,
            max_steps: 8,
        }
    }

    #[test]
    fn replays_text_and_appends_the_answer() {
        let fake = FakeBackend::new(
            vec![vec![Beat::Content(
                "one two three four five six seven".into(),
            )]],
            Duration::ZERO,
        );
        let mut messages = vec![];
        let mut text = String::new();
        let usage = fake.run(
            &options(),
            &mut messages,
            &NoTools,
            &Cancel::new(),
            &mut |event| {
                if let AgentEvent::Stream(StreamEvent::Content(piece)) = event {
                    text.push_str(&piece);
                }
            },
        );
        assert!(usage.is_ok());
        assert_eq!(text, "one two three four five six seven");
        assert_eq!(messages.last().unwrap().content, text);
    }

    #[test]
    fn a_failure_ends_the_run() {
        let fake = FakeBackend::new(
            vec![vec![Beat::Fail(FakeError::RateLimited(12))]],
            Duration::ZERO,
        );
        let result = fake.run(
            &options(),
            &mut vec![],
            &NoTools,
            &Cancel::new(),
            &mut |_| {},
        );
        assert!(matches!(
            result,
            Err(Error::RateLimited {
                retry_after_secs: 12
            })
        ));
    }

    #[test]
    fn cancel_stops_a_pause() {
        let fake = FakeBackend::new(
            vec![vec![Beat::Pause(Duration::from_secs(30))]],
            Duration::ZERO,
        );
        let cancel = Cancel::new();
        cancel.cancel();
        let result = fake.run(&options(), &mut vec![], &NoTools, &cancel, &mut |_| {});
        assert!(matches!(result, Err(Error::Cancelled)));
    }
}
