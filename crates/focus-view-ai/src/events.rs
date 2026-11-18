use std::path::PathBuf;

use focus_ai::prompts::Scope;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeTone {
    Neutral,
    Ok,
    Warn,
    Danger,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AiEvent {
    OpenSettings,
    OpenNote {
        path: PathBuf,
        line: Option<u32>,
    },
    InsertAtCursor(String),
    ReplaceText {
        scope: Scope,
        original: String,
        replacement: String,
    },
    InsertBelow {
        scope: Scope,
        text: String,
    },
    CreateNote {
        title: String,
        content: String,
    },
    SaveConversationAsNote {
        title: String,
        markdown: String,
    },
    OpenExternal(String),
    SearchTag(String),
    ZoomDiagram(String),
    ModelChosen(String),
    Notify {
        tone: NoticeTone,
        text: String,
    },
}
