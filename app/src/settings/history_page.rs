use focus_history::History;
use focus_kit::{ActiveTokens, Button, ButtonKind, Type, size};
use gpui::{
    AnyElement, AppContext, Context, IntoElement, ParentElement, Styled, Task, div,
    prelude::FluentBuilder,
};

use super::SettingsView;

#[derive(Default)]
pub struct HistoryState {
    pub used: Option<u64>,
    pub confirm_clear: bool,
    pub freed: Option<u64>,
    tasks: Vec<Task<()>>,
}

fn empty(history: &History) -> focus_history::Result<(u64, u64)> {
    let report = history.clear()?;
    Ok((report.bytes, history.size()?))
}

impl SettingsView {
    pub(super) fn refresh_history_size(&mut self, cx: &mut Context<Self>) {
        let Some(history) = self.facts.history.clone() else {
            return;
        };
        let task = cx.background_spawn(async move { history.size() });
        let handle = cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(bytes) => this.history.used = Some(bytes),
                    Err(error) => eprintln!("focus: measuring the note history: {error}"),
                }
                cx.notify();
            });
        });
        self.history.tasks.push(handle);
    }

    pub(super) fn clear_versions(&mut self, cx: &mut Context<Self>) {
        if !self.history.confirm_clear {
            self.history.confirm_clear = true;
            self.history.freed = None;
            cx.notify();
            return;
        }
        self.history.confirm_clear = false;
        let Some(history) = self.facts.history.clone() else {
            return;
        };
        let task = cx.background_spawn(async move { empty(&history) });
        let handle = cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((freed, left)) => {
                        this.history.freed = Some(freed);
                        this.history.used = Some(left);
                    }
                    Err(error) => {
                        eprintln!("focus: deleting the note history: {error}");
                        this.notice = Some((
                            true,
                            focus_i18n::t("settings-files-history-clear-failed").into(),
                        ));
                    }
                }
                cx.notify();
            });
        });
        self.history.tasks.push(handle);
        cx.notify();
    }

    pub(super) fn history_action(
        &self,
        key: &'static str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if key != "files.history_clear" {
            return None;
        }
        let t = *cx.tokens();
        let confirm = self.history.confirm_clear;
        Some(
            div()
                .flex()
                .flex_col()
                .items_end()
                .gap(size::S2)
                .child(
                    Button::new(
                        key,
                        focus_i18n::t(if confirm {
                            "settings-files-history-clear-confirm"
                        } else {
                            "settings-files-history-clear-action"
                        }),
                    )
                    .kind(if confirm {
                        ButtonKind::Danger
                    } else {
                        ButtonKind::Secondary
                    })
                    .disabled(self.facts.history.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.clear_versions(cx))),
                )
                .when_some(self.history.freed, |this, bytes| {
                    this.child(div().ui_sm().text_color(t.ink_3).child(focus_i18n::t_args(
                        "settings-files-history-cleared",
                        &[("size", focus_i18n::format_bytes(bytes).as_str().into())],
                    )))
                })
                .into_any_element(),
        )
    }

    pub(super) fn history_info(&self) -> String {
        match (&self.facts.history, self.history.used) {
            (None, _) => focus_i18n::t("ui-none"),
            (Some(_), Some(bytes)) => focus_i18n::format_bytes(bytes),
            (Some(_), None) => "…".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focus_history::{HistoryConfig, Kind};
    use std::path::Path;

    #[test]
    fn deleting_every_version_empties_the_store_and_keeps_the_settings() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/test-tmp/settings-history-empty");
        let _ = std::fs::remove_dir_all(&dir);
        let config = HistoryConfig {
            enabled: true,
            days: 9,
            max_bytes: 5_000_000,
        };
        let history = History::open(&dir, config).unwrap();
        for note in ["a.md", "b/c.md"] {
            for text in ["one", "two"] {
                history
                    .keep(Path::new(note), Kind::Bulk, || Ok(text.as_bytes().to_vec()))
                    .unwrap();
            }
        }
        assert!(history.size().unwrap() > 0);
        let (freed, left) = empty(&history).unwrap();
        assert_eq!((freed, left), (6, 0));
        assert!(history.list(Path::new("a.md")).unwrap().is_empty());
        assert!(history.list(Path::new("b/c.md")).unwrap().is_empty());
        assert_eq!(history.config(), config);
        history
            .keep(Path::new("a.md"), Kind::Bulk, || Ok(b"three".to_vec()))
            .unwrap();
        assert_eq!(history.list(Path::new("a.md")).unwrap().len(), 1);
    }
}
