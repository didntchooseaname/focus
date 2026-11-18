use focus_core::VaultError;

#[derive(Clone, Debug, PartialEq)]
pub enum MessageArg {
    Text(String),
    Count(i64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub key: &'static str,
    pub args: Vec<(&'static str, MessageArg)>,
}

impl Message {
    pub fn new(key: &'static str) -> Message {
        Message {
            key,
            args: Vec::new(),
        }
    }

    pub fn text_arg(mut self, name: &'static str, value: impl Into<String>) -> Message {
        self.args.push((name, MessageArg::Text(value.into())));
        self
    }

    pub fn count_arg(mut self, name: &'static str, value: usize) -> Message {
        self.args.push((name, MessageArg::Count(value as i64)));
        self
    }

    pub fn text(&self) -> String {
        let args: Vec<(&str, focus_i18n::Arg)> = self
            .args
            .iter()
            .map(|(name, value)| {
                let value = match value {
                    MessageArg::Text(text) => focus_i18n::Arg::Str(text),
                    MessageArg::Count(count) => focus_i18n::Arg::Int(*count),
                };
                (*name, value)
            })
            .collect();
        focus_i18n::t_args(self.key, &args)
    }
}

pub fn error_message(error: &VaultError) -> Message {
    let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
    match error {
        VaultError::Io { path: p, .. } => Message::new("vault-error-io").text_arg("path", path(p)),
        VaultError::OutsideVault(p) => {
            Message::new("vault-error-outside").text_arg("path", path(p))
        }
        VaultError::AlreadyExists(p) => {
            Message::new("vault-error-exists").text_arg("path", path(p))
        }
        VaultError::UnknownNote(_) => Message::new("vault-error-unknown"),
        VaultError::Conflict(p) => Message::new("vault-error-conflict").text_arg("path", path(p)),
        VaultError::Other(_) => Message::new("vault-error-other"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_is_translated_when_read() {
        focus_i18n::init(None);
        let message = Message::new("vault-renamed")
            .text_arg("name", "B.md")
            .count_arg("count", 2);
        focus_i18n::set_language("en").ok();
        let english = message.text();
        focus_i18n::set_language("fr").ok();
        let french = message.text();
        assert!(english.contains("B.md") && french.contains("B.md"));
        assert_ne!(english, french);
    }
}
