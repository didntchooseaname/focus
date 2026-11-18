use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use focus_history::DiffLine;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::fault::Fault;

const ETAG_HEX_CHARS: usize = 12;
const MAX_KEY_CHARS: usize = 128;
const IDEMPOTENCY_CAPACITY: usize = 256;
const MAX_PREVIEW_LINES: usize = 200;

pub(crate) fn etag(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    digest
        .iter()
        .take(ETAG_HEX_CHARS / 2)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WriteOptions {
    pub dry_run: bool,
    pub if_match: Option<String>,
    pub idempotency_key: Option<String>,
}

impl WriteOptions {
    pub fn parse(arguments: &Value) -> Result<WriteOptions, Fault> {
        let invalid = |name: &str, message: &str, hint: &str| {
            Fault::new("invalid_argument", format!("parameter `{name}`: {message}"))
                .with_issue(name, message, hint)
        };
        let dry_run = match arguments.get("dry_run") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => {
                return Err(invalid(
                    "dry_run",
                    "boolean expected",
                    "Send true or false.",
                ));
            }
        };
        let text = |name: &str| -> Result<Option<String>, Fault> {
            match arguments.get(name) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(s)) if s.trim().is_empty() => Err(invalid(
                    name,
                    "empty",
                    "Omit the argument, or send the value a previous answer gave.",
                )),
                Some(Value::String(s)) => Ok(Some(s.trim().to_string())),
                Some(_) => Err(invalid(name, "text expected", "Send a string.")),
            }
        };
        let if_match = text("if_match")?.map(|s| s.to_ascii_lowercase());
        if let Some(tag) = &if_match
            && !tag.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid(
                "if_match",
                "not an etag",
                "Send the `Etag` value that read_note returned for this note.",
            ));
        }
        let idempotency_key = text("idempotency_key")?;
        if let Some(key) = &idempotency_key
            && key.chars().count() > MAX_KEY_CHARS
        {
            return Err(invalid(
                "idempotency_key",
                "too long",
                "Use a key of at most 128 characters, for example a UUID.",
            ));
        }
        Ok(WriteOptions {
            dry_run,
            if_match,
            idempotency_key,
        })
    }

    pub fn check(&self, current: Option<&str>, argument: &str, reread: &str) -> Result<(), Fault> {
        let Some(wanted) = &self.if_match else {
            return Ok(());
        };
        match current {
            Some(content) if etag(content) == *wanted => Ok(()),
            _ => Err(Fault::stale(argument, reread)),
        }
    }
}

fn call_fingerprint(tool: &str, arguments: &Value) -> String {
    fn canonical(value: &Value, out: &mut String) {
        match value {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                out.push('{');
                for key in keys {
                    out.push_str(&Value::String(key.clone()).to_string());
                    out.push(':');
                    canonical(&map[key], out);
                    out.push(',');
                }
                out.push('}');
            }
            Value::Array(items) => {
                out.push('[');
                for item in items {
                    canonical(item, out);
                    out.push(',');
                }
                out.push(']');
            }
            other => out.push_str(&other.to_string()),
        }
    }
    let mut stripped = arguments.clone();
    if let Some(map) = stripped.as_object_mut() {
        for name in ["dry_run", "if_match", "idempotency_key"] {
            map.remove(name);
        }
    }
    let mut text = format!("{tool}\n");
    canonical(&stripped, &mut text);
    etag_full(&text)
}

fn etag_full(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

enum Slot {
    Running,
    Done { fingerprint: String, result: String },
}

#[derive(Default)]
struct Entries {
    order: VecDeque<String>,
    slots: HashMap<String, Slot>,
}

#[derive(Default)]
pub(crate) struct Idempotency {
    entries: Mutex<Entries>,
}

pub(crate) enum Begin<'a> {
    Fresh(Reservation<'a>),
    Replay(String),
    Refused(Fault),
}

pub(crate) struct Reservation<'a> {
    store: &'a Idempotency,
    id: String,
    fingerprint: String,
    done: bool,
}

impl Idempotency {
    fn lock(&self) -> std::sync::MutexGuard<'_, Entries> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn begin(&self, tool: &str, key: &str, arguments: &Value) -> Begin<'_> {
        let id = format!("{tool}\n{key}");
        let fingerprint = call_fingerprint(tool, arguments);
        let mut entries = self.lock();
        match entries.slots.get(&id) {
            Some(Slot::Done {
                fingerprint: first,
                result,
            }) if *first == fingerprint => return Begin::Replay(result.clone()),
            Some(Slot::Done { .. }) => {
                return Begin::Refused(
                    Fault::new(
                        "idempotency_conflict",
                        "this idempotency_key was used for a call with other arguments",
                    )
                    .with_issue(
                        "idempotency_key",
                        "same key, different arguments",
                        "Use a new key for a different call, or resend the first call unchanged.",
                    ),
                );
            }
            Some(Slot::Running) => {
                return Begin::Refused(
                    Fault::new(
                        "in_progress",
                        "a call with this idempotency_key is still running",
                    )
                    .with_issue(
                        "idempotency_key",
                        "the first call has not finished",
                        "Wait, then send the same call with the same key to get its result.",
                    ),
                );
            }
            None => {}
        }
        entries.slots.insert(id.clone(), Slot::Running);
        entries.order.push_back(id.clone());
        while entries.order.len() > IDEMPOTENCY_CAPACITY {
            if let Some(oldest) = entries.order.pop_front()
                && !matches!(entries.slots.get(&oldest), Some(Slot::Running))
            {
                entries.slots.remove(&oldest);
            }
        }
        drop(entries);
        Begin::Fresh(Reservation {
            store: self,
            id,
            fingerprint,
            done: false,
        })
    }

    pub fn peek(&self, tool: &str, key: &str, arguments: &Value) -> bool {
        let id = format!("{tool}\n{key}");
        matches!(
            self.lock().slots.get(&id),
            Some(Slot::Done { fingerprint, .. }) if *fingerprint == call_fingerprint(tool, arguments)
        )
    }
}

impl Reservation<'_> {
    pub fn complete(mut self, result: &str) {
        self.done = true;
        let mut entries = self.store.lock();
        entries.slots.insert(
            self.id.clone(),
            Slot::Done {
                fingerprint: self.fingerprint.clone(),
                result: result.to_string(),
            },
        );
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if !self.done {
            let mut entries = self.store.lock();
            entries.slots.remove(&self.id);
            entries.order.retain(|id| *id != self.id);
        }
    }
}

pub(crate) fn changed_lines(before: &str, after: &str, show: &dyn Fn(&str) -> String) -> String {
    let (mut old_number, mut new_number) = (0usize, 0usize);
    let mut lines = Vec::new();
    for line in focus_history::diff(before, after) {
        match line {
            DiffLine::Same(_) => {
                old_number += 1;
                new_number += 1;
            }
            DiffLine::Removed(text) => {
                old_number += 1;
                lines.push(format!(
                    "-{old_number}: {}",
                    show(text.trim_end_matches(['\n', '\r']))
                ));
            }
            DiffLine::Added(text) => {
                new_number += 1;
                lines.push(format!(
                    "+{new_number}: {}",
                    show(text.trim_end_matches(['\n', '\r']))
                ));
            }
        }
    }
    if lines.is_empty() {
        return "No line would change.\n".to_string();
    }
    let total = lines.len();
    let removed = lines.iter().filter(|l| l.starts_with('-')).count();
    let mut out = format!("{removed} line(s) removed, {} added\n", total - removed);
    for line in lines.iter().take(MAX_PREVIEW_LINES) {
        out.push_str(line);
        out.push('\n');
    }
    if total > MAX_PREVIEW_LINES {
        out.push_str(&format!(
            "[{} more changed lines not shown]\n",
            total - MAX_PREVIEW_LINES
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn etag_is_twelve_hex_characters_of_the_sha256() {
        assert_eq!(etag("abc"), "ba7816bf8f01");
        assert_ne!(etag("abc"), etag("abd"));
    }

    #[test]
    fn options_are_validated_and_optional() {
        let none = WriteOptions::parse(&json!({ "path": "a" })).unwrap();
        assert!(!none.dry_run && none.if_match.is_none() && none.idempotency_key.is_none());
        let all = WriteOptions::parse(
            &json!({ "dry_run": true, "if_match": " BA78 ", "idempotency_key": "k1" }),
        )
        .unwrap();
        assert!(all.dry_run);
        assert_eq!(all.if_match.as_deref(), Some("ba78"));
        for bad in [
            json!({ "dry_run": "yes" }),
            json!({ "if_match": "not hex!" }),
            json!({ "if_match": "" }),
            json!({ "idempotency_key": 4 }),
        ] {
            let fault = WriteOptions::parse(&bad).unwrap_err();
            assert_eq!(fault.code, "invalid_argument", "{bad}");
        }
    }

    #[test]
    fn a_key_replays_its_result_and_refuses_other_arguments() {
        let store = Idempotency::default();
        let first = json!({ "path": "a", "content": "x", "idempotency_key": "k" });
        let Begin::Fresh(reservation) = store.begin("create_note", "k", &first) else {
            panic!("first call");
        };
        assert!(matches!(
            store.begin("create_note", "k", &first),
            Begin::Refused(f) if f.code == "in_progress"
        ));
        reservation.complete("created");
        let same = json!({ "content": "x", "path": "a", "dry_run": false });
        assert!(
            matches!(store.begin("create_note", "k", &same), Begin::Replay(r) if r == "created")
        );
        assert!(store.peek("create_note", "k", &same));
        let other = json!({ "path": "b", "content": "x" });
        assert!(matches!(
            store.begin("create_note", "k", &other),
            Begin::Refused(f) if f.code == "idempotency_conflict"
        ));
        assert!(matches!(
            store.begin("append_to_note", "k", &first),
            Begin::Fresh(_)
        ));
    }

    #[test]
    fn a_failed_call_frees_its_key_and_the_store_is_bounded() {
        let store = Idempotency::default();
        let arguments = json!({});
        match store.begin("t", "k", &arguments) {
            Begin::Fresh(reservation) => drop(reservation),
            _ => panic!("first call"),
        }
        assert!(matches!(store.begin("t", "k", &arguments), Begin::Fresh(_)));
        for n in 0..IDEMPOTENCY_CAPACITY + 10 {
            if let Begin::Fresh(r) = store.begin("t", &format!("key-{n}"), &arguments) {
                r.complete("done");
            }
        }
        assert!(store.lock().slots.len() <= IDEMPOTENCY_CAPACITY);
    }

    #[test]
    fn changed_lines_number_removed_and_added_lines() {
        let text = changed_lines("a\nb\nc\n", "a\nB\nc\nd\n", &|s| s.to_string());
        assert_eq!(text, "1 line(s) removed, 2 added\n-2: b\n+2: B\n+4: d\n");
        assert_eq!(
            changed_lines("same\n", "same\n", &|s| s.to_string()),
            "No line would change.\n"
        );
    }
}
