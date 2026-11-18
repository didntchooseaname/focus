mod backend;
mod context;
mod diff;
mod dir;
mod errors;
mod events;
pub mod fake;
mod gate;
mod history_dir;
mod links;
mod mask;
mod mcp_secrets;
mod model;
mod panel;
mod settings;
mod view;

pub use backend::{ChatBackend, DeepSeek, MAX_STEPS, RunOptions};
pub use context::{EditorContext, NoteSnapshot, Snapshot};
pub use events::{AiEvent, NoticeTone};
pub use gate::WriteGate;
pub use history_dir::history_dir_for;
pub use mcp_secrets::{Migration, has_plaintext, migrate_plaintext, with_secrets, without_values};
pub use panel::{AiPanel, Services, bind};
pub use settings::{ActionEntry, AiSettings, language_name};

#[cfg(test)]
mod key_tests {
    #[test]
    fn every_key_used_in_the_sources_exists_in_english() {
        let english = focus_i18n::audit::embedded("en");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            for prefix in [
                "t_(\"",
                "t(\"",
                "t_args(\"",
                "translate(\"",
                "title_key: \"",
                "plain(\"",
                "if open { \"",
            ] {
                for part in text.split(prefix).skip(1) {
                    let key = part.split('"').next().unwrap().to_string();
                    if key.starts_with("ai-") && !english.messages.contains_key(&key) {
                        missing.push(key);
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "keys missing from locales/en/ai.ftl: {missing:?}"
        );
    }
}
