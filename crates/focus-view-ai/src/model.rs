use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use focus_ai::Usage;
use focus_ai::prompts::{Output, Scope, Template};
use focus_view_reading::ReadingView;
use gpui::{Entity, Subscription};

use crate::context::Chip;
use crate::diff::{self, DiffLine};
use crate::errors::Failure;

pub(crate) enum Entry {
    User(UserTurn),
    Assistant(Assistant),
    Failure(FailureRow),
}

pub(crate) struct UserTurn {
    pub text: String,
    pub chips: Vec<Chip>,
    pub masked: usize,
    pub wire_start: usize,
    pub prompt: String,
    pub action: Option<ActionRun>,
}

#[derive(Clone)]
pub(crate) struct ActionRun {
    pub template: Template,
    pub scope: Scope,
    pub target: String,
}

pub(crate) struct FailureRow {
    pub failure: Failure,
    pub retry_at: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct Assistant {
    pub parts: Vec<Part>,
    pub usage: Option<Usage>,
    pub cost: Option<f64>,
    pub live: bool,
    pub stopped: bool,
    pub offer: Option<Offer>,
}

pub(crate) enum Part {
    Reasoning {
        text: String,
        open: bool,
        started: Instant,
        took: Option<Duration>,
    },
    Text {
        text: String,
        view: Option<Entity<ReadingView>>,
        dirty: bool,
        subscriptions: Vec<Subscription>,
    },
    Tool(ToolLine),
    Write(WriteCard),
    Notice(&'static str),
}

pub(crate) enum ToolState {
    Running,
    Done(String),
    Failed(String),
    Refused,
}

pub(crate) struct ToolLine {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub started: Instant,
    pub took: Option<Duration>,
    pub state: ToolState,
    pub open: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    Pending,
    Applied,
    Rejected,
}

pub(crate) struct WriteCard {
    pub title_key: &'static str,
    pub path: Option<PathBuf>,
    pub note: Option<String>,
    pub diff: Vec<DiffLine>,
    pub decision: Decision,
    pub reply: Option<mpsc::Sender<bool>>,
}

impl WriteCard {
    pub fn proposal(
        tool: &str,
        path: PathBuf,
        before: Option<&str>,
        after: &str,
        reply: mpsc::Sender<bool>,
    ) -> WriteCard {
        let (note, diff) = if EDITS_TEXT.contains(&tool) {
            (None, diff::lines(before.unwrap_or(""), after))
        } else {
            (Some(after.to_string()), Vec::new())
        };
        WriteCard {
            title_key: "ai-confirm-title",
            path: Some(path),
            note,
            diff,
            decision: Decision::Pending,
            reply: Some(reply),
        }
    }

    pub fn replacement(original: &str, replacement: &str) -> WriteCard {
        WriteCard {
            title_key: "ai-proposal-replace",
            path: None,
            note: None,
            diff: diff::lines(original, replacement),
            decision: Decision::Pending,
            reply: None,
        }
    }
}

const EDITS_TEXT: [&str; 4] = [
    "create_note",
    "append_to_note",
    "replace_in_note",
    "toggle_task",
];

pub(crate) struct Offer {
    pub output: Output,
    pub scope: Scope,
    pub original: String,
    pub replacement: String,
    pub title: String,
    pub card: WriteCard,
}

impl Assistant {
    pub fn new() -> Assistant {
        Assistant {
            live: true,
            ..Assistant::default()
        }
    }

    pub fn push_reasoning(&mut self, chunk: &str) {
        if let Some(Part::Reasoning { text, .. }) = self.parts.last_mut() {
            text.push_str(chunk);
        } else {
            self.parts.push(Part::Reasoning {
                text: chunk.to_string(),
                open: false,
                started: Instant::now(),
                took: None,
            });
        }
    }

    pub fn push_content(&mut self, chunk: &str) {
        self.close_reasoning();
        if let Some(Part::Text { text, dirty, .. }) = self.parts.last_mut() {
            text.push_str(chunk);
            *dirty = true;
        } else {
            self.parts.push(Part::Text {
                text: chunk.to_string(),
                view: None,
                dirty: true,
                subscriptions: Vec::new(),
            });
        }
    }

    pub fn close_reasoning(&mut self) {
        for part in &mut self.parts {
            if let Part::Reasoning { started, took, .. } = part
                && took.is_none()
            {
                *took = Some(started.elapsed());
            }
        }
    }

    pub fn answer(&self) -> String {
        let texts: Vec<&str> = self
            .parts
            .iter()
            .filter_map(|p| {
                if let Part::Text { text, .. } = p {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect();
        texts.join("\n\n")
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

pub(crate) fn strip_fence(answer: &str) -> String {
    let trimmed = answer.trim();
    if let Some(rest) = trimmed.strip_prefix("```")
        && let Some(body) = rest.split_once('\n').map(|(_, body)| body)
        && let Some(inner) = body.strip_suffix("```")
        && !inner.contains("```")
    {
        return inner.trim_end().to_string();
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_of_text_accumulate_in_one_part() {
        let mut turn = Assistant::new();
        turn.push_content("Hello ");
        turn.push_content("world");
        assert_eq!(turn.parts.len(), 1);
        assert_eq!(turn.answer(), "Hello world");
        assert!(matches!(
            &turn.parts[0],
            Part::Text {
                view: None,
                dirty: true,
                ..
            }
        ));
    }

    #[test]
    fn reasoning_gets_its_duration_when_content_starts() {
        let mut turn = Assistant::new();
        turn.push_reasoning("thinking");
        turn.push_content("answer");
        assert!(matches!(
            turn.parts[0],
            Part::Reasoning { took: Some(_), .. }
        ));
    }

    #[test]
    fn fence_around_the_whole_answer_is_removed() {
        assert_eq!(
            strip_fence("```md\nline one\nline two\n```"),
            "line one\nline two"
        );
        assert_eq!(strip_fence("plain"), "plain");
        assert_eq!(
            strip_fence("```a\nx\n```\ntext\n```b\ny\n```"),
            "```a\nx\n```\ntext\n```b\ny\n```"
        );
    }
}
