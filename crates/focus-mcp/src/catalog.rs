use focus_ai::agent::ToolHost;
use focus_ai::prompts::{Output, Scope, Template, Vars, render};
use serde_json::Value;

use crate::tools::VaultTools;

pub const RESOURCE_PREFIX: &str = "focus://note/";
pub const PAGE_SIZE: usize = 200;

#[derive(Debug, Clone)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct PromptArgument {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

#[derive(Debug, Clone)]
pub struct PromptInfo {
    pub name: String,
    pub description: String,
    pub arguments: Vec<PromptArgument>,
}

pub trait McpHost: ToolHost + Send + Sync {
    fn resources(&self, _offset: usize, _limit: usize) -> Option<(Vec<Resource>, Option<usize>)> {
        None
    }
    fn read_resource(&self, _uri: &str) -> Result<String, String> {
        Err("no resources".into())
    }
    fn prompts(&self) -> Option<Vec<PromptInfo>> {
        None
    }
    fn prompt(
        &self,
        _name: &str,
        _arguments: &serde_json::Map<String, Value>,
    ) -> Result<(String, String), String> {
        Err("no prompts".into())
    }
}

pub fn uri_encode(path: &str) -> String {
    let mut out = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn uri_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(u8::from_str_radix(text.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    format!(
        "{:04}-{month:02}-{day:02}",
        yoe + era * 400 + i64::from(month <= 2)
    )
}

fn describe(template: &Template) -> String {
    if !template.builtin {
        return if template.description.is_empty() {
            template.label.clone()
        } else {
            template.description.clone()
        };
    }
    let first = template.prompt.lines().next().unwrap_or("");
    let sentence = first.split_inclusive(['.', ':']).next().unwrap_or(first);
    sentence
        .trim()
        .trim_end_matches([':', '.'])
        .chars()
        .take(140)
        .collect()
}

impl McpHost for VaultTools {
    fn resources(&self, offset: usize, limit: usize) -> Option<(Vec<Resource>, Option<usize>)> {
        let mut notes: Vec<_> = self
            .vault
            .notes()
            .into_iter()
            .filter(|m| self.shown(m.id))
            .collect();
        notes.sort_by(|a, b| a.path.cmp(&b.path));
        let page: Vec<Resource> = notes
            .iter()
            .skip(offset)
            .take(limit)
            .map(|m| Resource {
                uri: format!("{RESOURCE_PREFIX}{}", uri_encode(&m.path.to_string_lossy())),
                name: m.title.to_string(),
                description: m.path.display().to_string(),
            })
            .collect();
        let next = (offset + page.len() < notes.len()).then_some(offset + page.len());
        Some((page, next))
    }

    fn read_resource(&self, uri: &str) -> Result<String, String> {
        let encoded = uri
            .strip_prefix(RESOURCE_PREFIX)
            .ok_or_else(|| format!("unknown URI: {uri} (expected: {RESOURCE_PREFIX}<path>)"))?;
        let path = uri_decode(encoded).ok_or_else(|| format!("badly encoded URI: {uri}"))?;
        let (id, _) = self.locate(&path)?;
        let text = self.vault.read(id).map_err(crate::tools::describe)?;
        Ok(self.displayed(id, &text))
    }

    fn prompts(&self) -> Option<Vec<PromptInfo>> {
        let list: Vec<PromptInfo> = self
            .templates
            .iter()
            .filter(|t| t.enabled && (!t.ops_only || self.ops.is_some()))
            .map(|t| PromptInfo {
                name: t.id.clone(),
                description: describe(t),
                arguments: vec![
                    PromptArgument {
                        name: "path",
                        description: "Note the action applies to, relative to the vault.",
                        required: false,
                    },
                    PromptArgument {
                        name: "selection",
                        description: "Target text. Without it, the whole note.",
                        required: false,
                    },
                ],
            })
            .collect();
        Some(list)
    }

    fn prompt(
        &self,
        name: &str,
        arguments: &serde_json::Map<String, Value>,
    ) -> Result<(String, String), String> {
        let template = self
            .templates
            .iter()
            .find(|t| t.id == name && t.enabled)
            .ok_or_else(|| format!("unknown prompt: {name}"))?;
        let note = match arguments
            .get("path")
            .and_then(Value::as_str)
            .filter(|p| !p.trim().is_empty())
        {
            Some(path) => {
                let (id, relative) = self.locate(path)?;
                let meta = self
                    .vault
                    .note(id)
                    .ok_or_else(|| format!("note not found: {path}"))?;
                Some((
                    relative,
                    meta,
                    self.vault.read(id).map_err(crate::tools::describe)?,
                ))
            }
            None => None,
        };
        let selection = arguments
            .get("selection")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        if note.is_none() && selection.is_none() {
            return Err("argument `path` or `selection` required".into());
        }
        let note_text = note.as_ref().map(|n| n.2.as_str()).unwrap_or("");
        let target = selection.unwrap_or(note_text);
        let tags: Vec<String> = note.as_ref().map(|n| n.1.tags.clone()).unwrap_or_default();
        let title = note
            .as_ref()
            .map(|n| n.1.title.to_string())
            .unwrap_or_default();
        let path = note
            .as_ref()
            .map(|n| n.0.display().to_string())
            .unwrap_or_default();
        let date = today_utc();
        let language = focus_ai::prompts::detect_language(target);
        let vars = Vars {
            selection: target,
            note: note_text,
            title: &title,
            path: &path,
            tags: &tags,
            date: &date,
            language,
        };
        let text = render(template, &vars);
        let destination = match template.output {
            Output::Chat => "reply in the conversation",
            Output::Replace => "replacement text",
            Output::Insert => "text to insert",
            Output::Note => "content of a new note",
        };
        let scope = if template.scope == Scope::Selection {
            "selection or note"
        } else {
            "note"
        };
        Ok((
            format!("{} ({scope}: {destination})", describe(template)),
            text,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_encoding_roundtrips_and_rejects_garbage() {
        for path in [
            "Red Team/Note (1).md",
            "Isolated note.md",
            "a%b#c?d.md",
            "日本/語.md",
        ] {
            let encoded = uri_encode(path);
            assert!(
                encoded
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-._~/%".contains(&b)),
                "{encoded}"
            );
            assert_eq!(uri_decode(&encoded).as_deref(), Some(path));
        }
        assert_eq!(uri_encode("Red Team/Note.md"), "Red%20Team/Note.md");
        assert_eq!(uri_decode("%zz"), None);
        assert_eq!(uri_decode("%C3"), None);
        assert_eq!(uri_decode("100%"), None);
    }

    #[test]
    fn today_is_a_plausible_iso_date() {
        let today = today_utc();
        assert_eq!(today.len(), 10);
        assert!(today.starts_with("20"));
    }
}
