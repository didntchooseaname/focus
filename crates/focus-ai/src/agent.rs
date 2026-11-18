use super::*;

#[derive(Debug, Clone)]
pub enum ToolOutcome {
    Ok(String),
    Error(String),
    Denied,
}

pub trait ToolHost {
    fn specs(&self) -> Vec<ToolSpec>;
    fn call(&self, call: &ToolCall, cancel: &Cancel) -> ToolOutcome;
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Stream(StreamEvent),
    ToolStarted(ToolCall),
    ToolFinished {
        call_id: String,
        outcome: ToolOutcome,
    },
    TurnFinished,
}

pub const MAX_TOOL_RESULT_BYTES: usize = 24_000;

pub fn run(
    client: &Client,
    messages: &mut Vec<Message>,
    host: &dyn ToolHost,
    max_steps: u32,
    cancel: &Cancel,
    on_event: &mut dyn FnMut(AgentEvent),
) -> Result<Usage, Error> {
    let specs = host.specs();
    let mut total = Usage::default();
    for step in 0..max_steps.max(1) {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let last = step + 1 >= max_steps.max(1);
        let request = ChatRequest {
            messages: messages.clone(),
            tools: if last { Vec::new() } else { specs.clone() },
            json: false,
        };
        let assistant = client.chat_stream(&request, cancel, &mut |event| {
            if let StreamEvent::Usage(usage) = &event {
                add(&mut total, usage);
            }
            on_event(AgentEvent::Stream(event));
        })?;
        let calls = assistant.tool_calls.clone();
        messages.push(assistant);
        if calls.is_empty() {
            on_event(AgentEvent::TurnFinished);
            return Ok(total);
        }
        let mut denied = false;
        for call in &calls {
            if denied {
                messages.push(tool_message(
                    call,
                    "Not executed: the user refused a previous action.",
                ));
                continue;
            }
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            on_event(AgentEvent::ToolStarted(call.clone()));
            let outcome = host.call(call, cancel);
            let content = match &outcome {
                ToolOutcome::Ok(text) => truncate(text),
                ToolOutcome::Error(text) => format!("Error: {}", truncate(text)),
                ToolOutcome::Denied => {
                    denied = true;
                    "The user refused this action. Do not repeat it unless asked.".to_string()
                }
            };
            messages.push(tool_message(call, &content));
            on_event(AgentEvent::ToolFinished {
                call_id: call.id.clone(),
                outcome,
            });
        }
        if denied {
            on_event(AgentEvent::TurnFinished);
            return Ok(total);
        }
    }
    on_event(AgentEvent::TurnFinished);
    Ok(total)
}

fn add(total: &mut Usage, usage: &Usage) {
    total.prompt_tokens += usage.prompt_tokens;
    total.completion_tokens += usage.completion_tokens;
    total.reasoning_tokens += usage.reasoning_tokens;
    total.cache_hit_tokens += usage.cache_hit_tokens;
    total.cache_miss_tokens += usage.cache_miss_tokens;
}

fn tool_message(call: &ToolCall, content: &str) -> Message {
    Message {
        role: Role::Tool,
        content: content.to_string(),
        reasoning: None,
        tool_calls: Vec::new(),
        tool_call_id: Some(call.id.clone()),
    }
}

pub fn truncate(text: &str) -> String {
    if text.len() <= MAX_TOOL_RESULT_BYTES {
        return text.to_string();
    }
    let mut end = MAX_TOOL_RESULT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n[result truncated: {} bytes omitted of {}]",
        &text[..end],
        text.len() - end,
        text.len()
    )
}

pub fn standard_tools(include_write_tools: bool) -> Vec<ToolSpec> {
    use serde_json::json;
    let tool = |name: &str, description: &str, properties: serde_json::Value, required: &[&str]| {
        ToolSpec {
            name: name.into(),
            description: description.into(),
            parameters: json!({ "type": "object", "properties": properties, "required": required }),
        }
    };
    let path = json!({ "type": "string", "description": "Note path relative to the vault root, for example `Red Team/Note.md`. The .md extension may be omitted." });
    let mut tools = vec![
        tool(
            "search_notes",
            "Search the text of the notes in the vault. Returns the matching notes with their path, title and matching lines (line number, excerpt). Syntax: words separated by spaces (all required), \"exact phrase\", a OR b, -excluded, /regex/, tag:#name, path:folder, file:name, title:word, heading:word, task:word, prop:key=value. An uppercase letter in a term makes the search case sensitive.",
            json!({
                "query": { "type": "string", "description": "Query in the Focus search syntax." },
                "limit": { "type": "integer", "description": "Maximum number of notes returned. Default 10, at most 30.", "minimum": 1, "maximum": 30 }
            }),
            &["query"],
        ),
        tool(
            "read_note",
            "Read the Markdown content of a note, one numbered line per line (`12: text`). The numbers are not part of the text. A line range limits the read; a long note is cut at `max_chars` and the end of the result says which line to resume from.",
            json!({
                "path": path,
                "start_line": { "type": "integer", "description": "First line to read. Default 1.", "minimum": 1 },
                "end_line": { "type": "integer", "description": "Last line to read, inclusive. Default: the end of the note.", "minimum": 1 },
                "max_chars": { "type": "integer", "description": "Maximum number of characters returned. Default 8000, at most 20000.", "minimum": 1, "maximum": 20000 }
            }),
            &["path"],
        ),
        tool(
            "list_folder",
            "List the notes and direct subfolders of a vault folder, with the title of each note.",
            json!({ "path": { "type": "string", "description": "Folder relative to the root; empty string for the root." } }),
            &["path"],
        ),
        tool(
            "get_backlinks",
            "List the notes that link to the given note, with the line of the link.",
            json!({ "path": path }),
            &["path"],
        ),
        tool(
            "get_outline",
            "Return the headings of the note with their level and line number.",
            json!({ "path": path }),
            &["path"],
        ),
        tool(
            "list_tags",
            "List the tags of the vault with the number of notes that carry them (descendants included), from most to least frequent.",
            json!({ "limit": { "type": "integer", "description": "Maximum number of tags. Default 100.", "minimum": 1 } }),
            &[],
        ),
        tool(
            "get_neighbors",
            "List the neighbors of a note: the notes it points to (outgoing links, with the line of the link, unresolved links flagged) and the notes that point to it (incoming links). With `depth` 2 or 3, lists the notes reached by following links in both directions, layer by layer.",
            json!({ "path": path, "depth": { "type": "integer", "description": "Depth. Default 1, at most 3.", "minimum": 1, "maximum": 3 } }),
            &["path"],
        ),
    ];
    if include_write_tools {
        tools.push(tool(
            "create_note",
            "Create a note. Fails if the path already exists. Missing folders are created. The write may need the user's approval.",
            json!({ "path": path, "content": { "type": "string", "description": "Complete Markdown content, frontmatter included if needed." } }),
            &["path", "content"],
        ));
        tools.push(tool(
            "append_to_note",
            "Append text to the end of an existing note, on a new line. The write may need the user's approval.",
            json!({ "path": path, "content": { "type": "string", "description": "Markdown text to append." } }),
            &["path", "content"],
        ));
        tools.push(tool(
            "replace_in_note",
            "Replace an exact piece of text in a note. Fails if `old_text` is missing or appears more than once: give enough context to make it unique. The line numbers of `read_note` are not part of the text. The write may need the user's approval.",
            json!({
                "path": path,
                "old_text": { "type": "string", "description": "Current text, exactly as written." },
                "new_text": { "type": "string", "description": "Replacement text." }
            }),
            &["path", "old_text", "new_text"],
        ));
    }
    tools
}

pub fn is_write_tool(name: &str) -> bool {
    matches!(
        name,
        "create_note"
            | "append_to_note"
            | "replace_in_note"
            | "toggle_task"
            | "rename_note"
            | "trash_note"
            | "export_note"
            | "app_insert_at_cursor"
            | "ops_journal_append"
            | "ops_import_nmap"
            | "ops_finding_create"
            | "ops_credential_add"
            | "ops_vars_set"
            | "ops_set_focus"
    )
}
