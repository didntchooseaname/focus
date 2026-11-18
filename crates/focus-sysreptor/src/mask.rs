use std::path::Path;

use focus_ops::report;

pub(crate) const MASK: &str = "[masked]";

pub(crate) struct Masker {
    secrets: Vec<String>,
    active: bool,
    pub(crate) replaced: usize,
}

impl Masker {
    pub(crate) fn new(active: bool, engagement_dir: &Path) -> Masker {
        let secrets = if active {
            report::secrets_of(engagement_dir)
        } else {
            Vec::new()
        };
        Masker {
            secrets,
            active,
            replaced: 0,
        }
    }

    pub(crate) fn text(&mut self, text: &str) -> String {
        if !self.active {
            return text.to_string();
        }
        let out = report::masked(text, &self.secrets);
        self.replaced += out
            .matches(MASK)
            .count()
            .saturating_sub(text.matches(MASK).count());
        out
    }

    pub(crate) fn proof_value(&mut self, value: &str) -> String {
        if !self.active || value.is_empty() {
            return value.to_string();
        }
        self.replaced += 1;
        MASK.to_string()
    }
}
