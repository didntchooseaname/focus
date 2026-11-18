use std::path::{Path, PathBuf};
use std::sync::Arc;

use focus_core::{LinkTarget, NoteId, Vault};
use focus_view_reading::Resolver;

const SCHEME: &str = "focus-source:";
const CURSOR: char = '\u{2588}';
const CURSOR_OFF: char = '\u{2007}';

pub fn markdown(answer: &str, sources: usize, cursor: Option<bool>) -> String {
    let mut out = String::with_capacity(answer.len() + 32);
    let mut last = 0;
    for (range, number) in focus_ai::assist::citations(answer) {
        if number == 0 || number as usize > sources {
            continue;
        }
        out.push_str(&answer[last..range.start]);
        out.push_str(&format!("[\\[{number}\\]]({SCHEME}{number})"));
        last = range.end;
    }
    out.push_str(&answer[last..]);
    if let Some(visible) = cursor {
        out.push(if visible { CURSOR } else { CURSOR_OFF });
    }
    out
}

pub struct Citations {
    pub vault: Arc<Vault>,
    pub sources: Vec<NoteId>,
}

impl Resolver for Citations {
    fn resolve_link(&self, from: NoteId, target: &str) -> LinkTarget {
        match target
            .strip_prefix(SCHEME)
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| self.sources.get(n.checked_sub(1)?))
        {
            Some(&id) => LinkTarget::Note { id, anchor: None },
            None => self.vault.resolve_link(from, target),
        }
    }

    fn absolute(&self, relative: &Path) -> PathBuf {
        self.vault.absolute(relative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_citations_of_existing_sources_become_links() {
        let text = markdown(
            "Seen in [1][2], not in [5] or [0]. Link [3](https://x.test).",
            3,
            None,
        );
        assert_eq!(
            text,
            "Seen in [\\[1\\]](focus-source:1)[\\[2\\]](focus-source:2), not in [5] or [0]. Link [3](https://x.test)."
        );
    }

    #[test]
    fn answer_without_sources_keeps_its_text() {
        assert_eq!(
            markdown("Cited [1] but nothing to open.", 0, None),
            "Cited [1] but nothing to open."
        );
    }

    #[test]
    fn the_cursor_keeps_its_width_when_it_blinks() {
        let on = markdown("text", 0, Some(true));
        let off = markdown("text", 0, Some(false));
        assert!(
            on.ends_with(CURSOR)
                && off.ends_with(CURSOR_OFF)
                && on.chars().count() == off.chars().count()
        );
    }
}
