use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Issue {
    pub path: String,
    pub message: String,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fault {
    pub code: &'static str,
    pub message: String,
    pub issues: Vec<Issue>,
}

pub(crate) const READ_ONLY_MESSAGE: &str = "writes are disabled: this server is read-only";
const READ_ONLY_HINT: &str = "Start focus-mcp with --write to enable the write tools.";
const PATH_HINT: &str = "Give a path relative to the vault root, for example `Notes/Idea.md`. Call list_folder to browse the vault.";

impl Fault {
    pub fn new(code: &'static str, message: impl Into<String>) -> Fault {
        Fault {
            code,
            message: message.into(),
            issues: Vec::new(),
        }
    }

    pub fn with_issue(
        mut self,
        path: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Fault {
        self.issues.push(Issue {
            path: path.into(),
            message: message.into(),
            hint: hint.into(),
        });
        self
    }

    pub fn read_only() -> Fault {
        Fault::new("read_only", READ_ONLY_MESSAGE).with_issue(
            "tool",
            READ_ONLY_MESSAGE,
            READ_ONLY_HINT,
        )
    }

    pub fn denied() -> Fault {
        Fault::new("denied", "denied: the write was not authorized").with_issue(
            "tool",
            "the user did not approve this write",
            "Do not retry the same call. Ask the user what to change.",
        )
    }

    pub fn stale(argument: &str, reread: &str) -> Fault {
        Fault::new(
            "stale",
            "the note changed since it was read: nothing was written",
        )
        .with_issue(
            "if_match",
            format!("the etag sent for `{argument}` is not the current one"),
            reread,
        )
    }

    pub fn to_json(&self) -> Value {
        json!({
            "code": self.code,
            "message": self.message,
            "issues": self.issues.iter().map(|i| json!({ "path": i.path, "message": i.message, "hint": i.hint })).collect::<Vec<_>>(),
        })
    }

    pub fn from_message(message: &str, arguments: &Value, properties: Option<&Value>) -> Fault {
        let lower = message.to_ascii_lowercase();
        let at = |fallback: &str| argument_at_fault(message, arguments, properties, fallback);
        let has = |needle: &str| lower.contains(needle);

        if has("writes are disabled") || has("the server is read-only") {
            return Fault::read_only();
        }
        if has("unknown tool") {
            return Fault::new("unknown_tool", message).with_issue(
                "tool",
                message,
                "Call tools/list to see the tools this server offers.",
            );
        }
        if has("engagement secrets are not served") {
            return Fault::new("path_refused", message).with_issue(
                at("path"),
                "the note holds credentials",
                "Call ops_credentials_list, or start focus-mcp with --reveal-secrets.",
            );
        }
        if has("outside the vault")
            || has("write refused in a reserved folder")
            || has("path excluded")
            || has("unreadable path")
            || has("leads outside the vault")
            || has("outside the export folder")
        {
            let hint = if has("reserved folder") {
                "Folders whose name starts with a dot and node_modules are not writable: choose another folder."
            } else if has("export folder") {
                "Give a destination inside the export folder, relative to it."
            } else {
                PATH_HINT
            };
            return Fault::new("path_refused", message).with_issue(at("path"), message, hint);
        }
        if has("changed on disk since it was read") || has("modified on disk since it was read") {
            return Fault::new("conflict", message).with_issue(
                at("path"),
                "the file changed while the call ran",
                "Call read_note to read the note again, then retry.",
            );
        }
        if has("already exists") {
            let hint = if has("use append_to_note") {
                "Call append_to_note or replace_in_note to change it, or choose another path."
            } else {
                "Choose a path that does not exist yet."
            };
            return Fault::new("already_exists", message).with_issue(at("path"), message, hint);
        }
        if has("text not found in") {
            return Fault::new("text_not_found", message).with_issue(
                "old_text",
                "no exact match in the note",
                "Call read_note and copy the text exactly: whitespace, line endings and case count.",
            );
        }
        if has("text appears") {
            return Fault::new("ambiguous_text", message).with_issue(
                "old_text",
                "more than one match in the note",
                "Add the neighbouring lines to `old_text` so that it matches once.",
            );
        }
        if has("no task at line") {
            return Fault::new("not_found", message).with_issue(
                "line",
                message,
                "Call list_tasks with `path` to see the lines that hold a task.",
            );
        }
        if has("not found") || has("is not a folder") {
            let (path, hint) = if has("engagement not found") {
                ("engagement", "Call ops_list_engagements for the names.")
            } else if has("host not found") {
                ("host", "Call ops_hosts_list for the hosts.")
            } else if has("folder not found") {
                (
                    "path",
                    "Call list_folder with an empty `path` to browse from the root.",
                )
            } else {
                ("path", "Call search_notes or list_folder to find the note.")
            };
            let hint = match message.split_once("Similar notes: ") {
                Some((_, near)) => format!("Similar notes: {near}"),
                None => hint.to_string(),
            };
            return Fault::new("not_found", message).with_issue(at(path), message, hint);
        }
        if has("invalid json arguments") || has("must form a json object") {
            return Fault::new("invalid_argument", message).with_issue(
                "arguments",
                message,
                "Send the arguments as one JSON object.",
            );
        }
        if has("invalid query") {
            return Fault::new("invalid_argument", message).with_issue(
                at("query"),
                message,
                "Syntax: words, \"exact phrase\", a OR b, -excluded, /regex/, tag:#name, path:folder, file:name, title:word, heading:word, task:word, prop:key=value.",
            );
        }
        if let Some(word) = lower
            .strip_prefix("unknown ")
            .and_then(|rest| rest.split(':').next())
            && property_names(properties).iter().any(|n| n == word)
        {
            let hint = accepted_values(word, properties)
                .unwrap_or_else(|| "See the input schema of the tool.".to_string());
            return Fault::new("invalid_argument", message).with_issue(word, message, hint);
        }
        if let Some(path) = named_argument(message, properties) {
            let hint = accepted_values(&path, properties)
                .unwrap_or_else(|| "See the input schema of the tool.".to_string());
            return Fault::new("invalid_argument", message).with_issue(path, message, hint);
        }
        if has("empty path") || has("invalid path") || (has("path") && has("expected")) {
            return Fault::new("invalid_argument", message).with_issue(
                at("path"),
                message,
                PATH_HINT,
            );
        }
        if has("invalid") || has("empty") || has("too long") || has("at most") || has("unknown ") {
            return Fault::new("invalid_argument", message).with_issue(
                at("arguments"),
                message,
                "See the input schema of the tool.",
            );
        }
        Fault::new("failed", message).with_issue(
            "tool",
            message,
            "The call was valid but could not be completed. Check the message, then retry.",
        )
    }
}

fn property_names(properties: Option<&Value>) -> Vec<String> {
    properties
        .and_then(Value::as_object)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

fn named_argument(message: &str, properties: Option<&Value>) -> Option<String> {
    let names = property_names(properties);
    message
        .split('`')
        .skip(1)
        .step_by(2)
        .find(|token| names.iter().any(|n| n == token))
        .map(str::to_string)
}

fn argument_at_fault(
    message: &str,
    arguments: &Value,
    properties: Option<&Value>,
    fallback: &str,
) -> String {
    if let Some(name) = named_argument(message, properties) {
        return name;
    }
    let names = property_names(properties);
    for name in &names {
        if let Some(Value::String(value)) = arguments.get(name)
            && !value.trim().is_empty()
            && message.contains(value.trim())
        {
            return name.clone();
        }
    }
    if arguments.get(fallback).is_some() || names.iter().any(|n| n == fallback) {
        fallback.to_string()
    } else {
        "arguments".to_string()
    }
}

fn accepted_values(name: &str, properties: Option<&Value>) -> Option<String> {
    let schema = properties?.get(name)?;
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        let values: Vec<&str> = values.iter().filter_map(Value::as_str).collect();
        return Some(format!("Accepted values: {}.", values.join(", ")));
    }
    let kind = schema.get("type").and_then(Value::as_str)?;
    let range = match (
        schema.get("minimum").and_then(Value::as_u64),
        schema.get("maximum").and_then(Value::as_u64),
    ) {
        (Some(min), Some(max)) => format!(", from {min} to {max}"),
        (Some(min), None) => format!(", at least {min}"),
        _ => String::new(),
    };
    Some(format!("`{name}` takes a value of type {kind}{range}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn properties() -> Value {
        json!({
            "path": { "type": "string" },
            "limit": { "type": "integer", "minimum": 1, "maximum": 30 },
            "format": { "type": "string", "enum": ["html", "markdown", "pdf"] },
        })
    }

    fn classify(message: &str, arguments: Value) -> Fault {
        Fault::from_message(message, &arguments, Some(&properties()))
    }

    #[test]
    fn each_family_has_its_code_and_a_hint() {
        let missing = classify("parameter `path` is missing or is not text", json!({}));
        assert_eq!(missing.code, "invalid_argument");
        assert_eq!(missing.issues[0].path, "path");

        let range = classify(
            "parameter `limit`: positive integer expected",
            json!({ "limit": "a" }),
        );
        assert_eq!(range.issues[0].path, "limit");
        assert!(range.issues[0].hint.contains("from 1 to 30"), "{range:?}");

        let format = classify(
            "unknown format: docx (html, markdown or pdf)",
            json!({ "format": "docx" }),
        );
        assert_eq!(format.code, "invalid_argument");

        let outside = classify(
            "path outside the vault: ../x.md (`..` is refused)",
            json!({ "path": "../x.md" }),
        );
        assert_eq!(
            (outside.code, outside.issues[0].path.as_str()),
            ("path_refused", "path")
        );
        assert!(outside.issues[0].hint.contains("list_folder"));

        let missing_note = classify(
            "note not found: Kerberoastin. Similar notes: Red Team/Kerberoasting.md",
            json!({ "path": "Kerberoastin" }),
        );
        assert_eq!(missing_note.code, "not_found");
        assert!(
            missing_note.issues[0]
                .hint
                .contains("Red Team/Kerberoasting.md")
        );

        assert_eq!(classify(READ_ONLY_MESSAGE, json!({})).code, "read_only");
        assert_eq!(
            classify(
                "already exists: Home.md. Use append_to_note or replace_in_note.",
                json!({ "path": "Home.md" })
            )
            .code,
            "already_exists"
        );
        assert_eq!(
            classify(
                "text not found in Home.md: the comparison is exact",
                json!({})
            )
            .code,
            "text_not_found"
        );
        assert_eq!(
            classify(
                "text appears 2 times in Home.md (lines 1, 2): add context",
                json!({})
            )
            .code,
            "ambiguous_text"
        );
        assert_eq!(
            classify("unknown tool: nope", json!({})).code,
            "unknown_tool"
        );
        assert_eq!(
            classify("something broke in the disk", json!({})).code,
            "failed"
        );
    }

    #[test]
    fn a_hint_never_names_an_internal_function() {
        for message in [
            "parameter `path` is missing or is not text",
            "path outside the vault: /etc",
            "note not found: a",
            "engagement not found: x (y). Engagements: none",
            "text not found in a.md: exact",
        ] {
            for issue in classify(message, json!({})).issues {
                assert!(
                    !issue.hint.contains("fn ")
                        && !issue.hint.contains(".rs")
                        && !issue.hint.contains("::"),
                    "{issue:?}"
                );
            }
        }
    }
}
