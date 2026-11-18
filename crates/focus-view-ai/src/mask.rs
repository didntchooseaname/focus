use std::collections::HashSet;
use std::sync::Arc;

use focus_ai::agent::{ToolHost, ToolOutcome, is_write_tool};
use focus_ai::redact::{self, Options, Replacement};
use focus_ai::{Cancel, Message, ToolCall, ToolSpec};
use parking_lot::Mutex;

const OPEN: char = '‹';
const CLOSE: char = '›';

pub struct Mask {
    options: Option<Options>,
    table: Mutex<Vec<Replacement>>,
    learned: Mutex<Vec<Replacement>>,
    used: Mutex<HashSet<String>>,
}

impl Mask {
    pub fn new(options: Option<Options>) -> Mask {
        Mask {
            options,
            table: Mutex::new(Vec::new()),
            learned: Mutex::new(Vec::new()),
            used: Mutex::new(HashSet::new()),
        }
    }

    pub fn active(&self) -> bool {
        self.options.is_some()
    }

    pub fn redact(&self, text: &str) -> String {
        let Some(options) = &self.options else {
            return text.to_string();
        };
        let mut known = self.learned.lock().clone();
        known.extend(self.table.lock().iter().cloned());
        let masked = redact::redact(text, options, &known);
        let mut table = self.table.lock();
        let mut used = self.used.lock();
        let learned = self.learned.lock();
        for replacement in masked.replacements {
            used.insert(replacement.token.clone());
            if !table.iter().any(|r| r.token == replacement.token)
                && !learned.iter().any(|r| r.token == replacement.token)
            {
                table.push(replacement);
            }
        }
        masked.text
    }

    pub fn learn(&self, secrets: &[String]) {
        if self.options.is_none() {
            return;
        }
        let mut learned = self.learned.lock();
        for secret in secrets.iter().filter(|s| s.chars().count() >= 4) {
            if !learned.iter().any(|r| r.original == *secret) {
                let token = format!("{OPEN}CRED_{}{CLOSE}", learned.len() + 1);
                learned.push(Replacement {
                    token,
                    original: secret.clone(),
                    kind: "secret",
                });
            }
        }
    }

    pub fn restore(&self, text: &str) -> String {
        if self.options.is_none() || !text.contains(OPEN) {
            return text.to_string();
        }
        let mut known = self.learned.lock().clone();
        known.extend(self.table.lock().iter().cloned());
        redact::restore(text, &known)
    }

    pub fn count(&self) -> usize {
        self.used.lock().len()
    }

    pub fn begin_run(&self) {
        self.used.lock().clear();
    }

    pub fn clear(&self) {
        self.table.lock().clear();
        self.used.lock().clear();
    }

    pub fn mask_message(&self, message: &Message) -> Message {
        Message {
            role: message.role,
            content: self.redact(&message.content),
            reasoning: message.reasoning.as_deref().map(|r| self.redact(r)),
            tool_calls: message
                .tool_calls
                .iter()
                .map(|c| ToolCall {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    arguments: self.redact(&c.arguments),
                })
                .collect(),
            tool_call_id: message.tool_call_id.clone(),
        }
    }

    pub fn restore_message(&self, message: &Message) -> Message {
        Message {
            role: message.role,
            content: self.restore(&message.content),
            reasoning: message.reasoning.as_deref().map(|r| self.restore(r)),
            tool_calls: message
                .tool_calls
                .iter()
                .map(|c| ToolCall {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    arguments: self.restore(&c.arguments),
                })
                .collect(),
            tool_call_id: message.tool_call_id.clone(),
        }
    }
}

#[derive(Default)]
pub struct StreamRestorer {
    held: String,
}

impl StreamRestorer {
    pub fn push(&mut self, mask: &Mask, chunk: &str) -> String {
        self.held.push_str(chunk);
        let cut = match self.held.rfind(OPEN) {
            Some(open) if !self.held[open..].contains(CLOSE) => open,
            _ => self.held.len(),
        };
        let ready: String = self.held.drain(..cut).collect();
        mask.restore(&ready)
    }

    pub fn flush(&mut self, mask: &Mask) -> String {
        let rest = std::mem::take(&mut self.held);
        mask.restore(&rest)
    }
}

pub struct MaskingHost {
    pub inner: Box<dyn ToolHost>,
    pub mask: Arc<Mask>,
    pub writes: bool,
}

impl ToolHost for MaskingHost {
    fn specs(&self) -> Vec<ToolSpec> {
        let specs = self.inner.specs();
        if self.writes {
            specs
        } else {
            specs
                .into_iter()
                .filter(|s| !is_write_tool(&s.name))
                .collect()
        }
    }

    fn call(&self, call: &ToolCall, cancel: &Cancel) -> ToolOutcome {
        if !self.writes && is_write_tool(&call.name) {
            return ToolOutcome::Error("write tools are disabled in the settings".to_string());
        }
        let restored = ToolCall {
            id: call.id.clone(),
            name: call.name.clone(),
            arguments: self.mask.restore(&call.arguments),
        };
        match self.inner.call(&restored, cancel) {
            ToolOutcome::Ok(text) => ToolOutcome::Ok(self.mask.redact(&text)),
            ToolOutcome::Error(text) => ToolOutcome::Error(self.mask.redact(&text)),
            ToolOutcome::Denied => ToolOutcome::Denied,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask() -> Mask {
        Mask::new(Some(Options {
            secrets: true,
            ip_addresses: true,
            hostnames: false,
        }))
    }

    #[test]
    fn tokens_are_stable_across_messages_and_restored() {
        let mask = mask();
        let first = mask.redact("password=Sup3rS3cret!x on 10.10.11.5");
        assert!(
            !first.contains("Sup3rS3cret") && !first.contains("10.10.11.5"),
            "{first}"
        );
        let second = mask.redact("again 10.10.11.5");
        let token = first.split_whitespace().last().unwrap();
        assert!(second.contains(token), "{second} / {first}");
        assert_eq!(mask.restore(&first), "password=Sup3rS3cret!x on 10.10.11.5");
        assert_eq!(mask.count(), 2);
    }

    struct ServerTool;

    impl ToolHost for ServerTool {
        fn specs(&self) -> Vec<ToolSpec> {
            vec![ToolSpec {
                name: "docs__lookup".into(),
                description: String::new(),
                parameters: serde_json::json!({}),
            }]
        }
        fn call(&self, call: &ToolCall, _: &Cancel) -> ToolOutcome {
            ToolOutcome::Ok(format!(
                "arguments {} then password=Sup3rS3cret!x on 10.10.11.5",
                call.arguments
            ))
        }
    }

    #[test]
    fn what_a_server_tool_returns_is_masked_before_the_model_sees_it() {
        let mask = Arc::new(mask());
        let host = MaskingHost {
            inner: Box::new(ServerTool),
            mask: mask.clone(),
            writes: true,
        };
        let call = ToolCall {
            id: "1".into(),
            name: "docs__lookup".into(),
            arguments: "{}".into(),
        };
        let ToolOutcome::Ok(text) = host.call(&call, &Cancel::new()) else {
            panic!("the tool answers")
        };
        assert!(
            !text.contains("Sup3rS3cret") && !text.contains("10.10.11.5"),
            "{text}"
        );
        assert_eq!(
            mask.restore(&text),
            "arguments {} then password=Sup3rS3cret!x on 10.10.11.5"
        );
    }

    #[test]
    fn a_learned_secret_is_masked_without_any_keyword_around_it() {
        let mask = mask();
        mask.learn(&["Zebra-Plain-77".to_string(), "abc".to_string()]);
        let masked = mask.redact("| alice | password | Zebra-Plain-77 | abc |");
        assert!(
            !masked.contains("Zebra-Plain-77") && masked.contains("abc"),
            "a value under four characters is not learned: {masked}"
        );
        assert_eq!(
            mask.restore(&masked),
            "| alice | password | Zebra-Plain-77 | abc |"
        );
        assert!(
            Mask::new(None).redact("Zebra-Plain-77") == "Zebra-Plain-77",
            "a disabled mask learns nothing"
        );
    }

    #[test]
    fn a_new_conversation_keeps_the_credentials_that_were_learned() {
        let mask = mask();
        mask.learn(&["Zebra-Plain-77".to_string()]);
        mask.redact("password=Other-Secret-12345 on the share");
        mask.clear();
        let masked = mask.redact("| alice | Zebra-Plain-77 |");
        assert!(
            !masked.contains("Zebra-Plain-77"),
            "a learned credential must survive `clear`: {masked}"
        );
        assert_eq!(mask.restore(&masked), "| alice | Zebra-Plain-77 |");
        assert_eq!(
            mask.count(),
            1,
            "the tokens of the old conversation are gone"
        );
    }

    #[test]
    fn disabled_mask_changes_nothing() {
        let mask = Mask::new(None);
        assert_eq!(
            mask.redact("10.0.0.7 password=hunter2hunter2"),
            "10.0.0.7 password=hunter2hunter2"
        );
        assert!(!mask.active());
    }

    #[test]
    fn a_token_split_between_chunks_is_restored_whole() {
        let mask = mask();
        let masked = mask.redact("host 10.10.11.5");
        let token = masked.split_whitespace().last().unwrap().to_string();
        let (head, tail) = token.split_at(token.char_indices().nth(3).unwrap().0);
        let mut restorer = StreamRestorer::default();
        let mut out = restorer.push(&mask, &format!("seen at {head}"));
        out.push_str(&restorer.push(&mask, &format!("{tail} today")));
        out.push_str(&restorer.flush(&mask));
        assert_eq!(out, "seen at 10.10.11.5 today");
    }

    #[test]
    fn count_restarts_with_each_run() {
        let mask = mask();
        mask.redact("10.10.11.5");
        mask.begin_run();
        assert_eq!(mask.count(), 0);
    }
}
