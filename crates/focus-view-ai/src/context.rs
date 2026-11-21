use std::path::PathBuf;
use std::sync::Arc;

use gpui::App;

#[derive(Clone, Debug, PartialEq)]
pub struct NoteSnapshot {
    pub path: PathBuf,
    pub title: String,
    pub tags: Vec<String>,
    pub text: Arc<str>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub note: Option<NoteSnapshot>,
    pub selection: Option<Arc<str>>,
}

pub trait EditorContext {
    fn snapshot(&self, cx: &App) -> Snapshot;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChipKind {
    Note,
    Selection,
    Added,
}

#[derive(Clone, Debug)]
pub(crate) struct Chip {
    pub kind: ChipKind,
    pub label: String,
    pub path: Option<PathBuf>,
    pub text: Arc<str>,
    pub tokens: u32,
}

impl Chip {
    pub fn new(kind: ChipKind, label: String, path: Option<PathBuf>, text: Arc<str>) -> Chip {
        let tokens = focus_ai::estimate_tokens(&text);
        Chip {
            kind,
            label,
            path,
            text,
            tokens,
        }
    }
}

pub(crate) fn compose(chips: &[Chip], question: &str) -> String {
    if chips.is_empty() {
        return question.to_string();
    }
    let mut out = String::from("Context provided by the user:\n");
    for chip in chips {
        let heading = match (chip.kind, &chip.path) {
            (ChipKind::Selection, Some(path)) => format!("Selection in {}", path.display()),
            (ChipKind::Selection, None) => "Selection".to_string(),
            (ChipKind::Note, Some(path)) => format!("Current note {}", path.display()),
            (_, Some(path)) => format!("Note {}", path.display()),
            (_, None) => chip.label.clone(),
        };
        out.push_str(&format!("\n### {heading}\n\n{}\n", chip.text.trim_end()));
    }
    out.push_str("\n---\n\n");
    out.push_str(question);
    out
}

pub(crate) fn format_tokens(count: u32) -> String {
    if count >= 1000 {
        let thousands = focus_i18n::format_decimal(f64::from(count) / 1000., 1);
        focus_i18n::t_args(
            "ai-tokens-approx-thousands",
            &[("count", thousands.as_str().into())],
        )
    } else {
        focus_i18n::t_args("ai-tokens-approx", &[("count", count.into())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_alone_is_unchanged() {
        assert_eq!(compose(&[], "What is this?"), "What is this?");
    }

    #[test]
    fn every_chip_is_in_the_message_and_nothing_else() {
        let chips = vec![
            Chip::new(
                ChipKind::Note,
                "A".into(),
                Some("a.md".into()),
                "alpha".into(),
            ),
            Chip::new(
                ChipKind::Selection,
                "Selection".into(),
                Some("a.md".into()),
                "beta".into(),
            ),
        ];
        let text = compose(&chips, "Q");
        assert!(text.contains("alpha") && text.contains("beta") && text.ends_with("Q"));
        assert!(text.contains("Current note a.md") && text.contains("Selection in a.md"));
    }

    #[test]
    fn tokens_are_compact_above_a_thousand() {
        assert_eq!(format_tokens(950), "~950");
        assert_eq!(format_tokens(1240), "~1.2k");
    }
}
