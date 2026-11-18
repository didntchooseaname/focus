use std::path::Path;

use focus_kit::Flow as _;
use focus_kit::{ActiveTokens, Icon, Type, ellipsis, icon, size};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, KeyDownEvent, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use super::{SettingsEvent, SettingsView};

const ROW: f32 = 40.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecentState {
    Open,
    Missing,
    Available,
}

pub fn states(recent: &[std::path::PathBuf], open: Option<&str>) -> Vec<RecentState> {
    recent
        .iter()
        .map(|path| {
            if open.is_some_and(|open| Path::new(open) == path) {
                RecentState::Open
            } else if !path.is_dir() {
                RecentState::Missing
            } else {
                RecentState::Available
            }
        })
        .collect()
}

impl SettingsView {
    pub fn activate_recent(&mut self, rank: usize, cx: &mut Context<Self>) -> bool {
        let states = states(&self.facts.recent, self.facts.vault.as_deref());
        match (self.facts.recent.get(rank), states.get(rank)) {
            (Some(path), Some(RecentState::Available)) => {
                cx.emit(SettingsEvent::SwitchVault(path.clone()));
                true
            }
            _ => false,
        }
    }

    pub(super) fn render_recent(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        if self.facts.recent.is_empty() {
            return div()
                .ui()
                .text_color(t.ink_3)
                .child(focus_i18n::t("ui-none"))
                .into_any_element();
        }
        let states = states(&self.facts.recent, self.facts.vault.as_deref());
        let rows = self
            .facts
            .recent
            .iter()
            .zip(states)
            .enumerate()
            .map(|(rank, (path, state))| {
                let name: SharedString = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
                    .into();
                let full: SharedString = path.display().to_string().into();
                let active = state == RecentState::Available;
                let status = match state {
                    RecentState::Open => Some(focus_i18n::t("settings-recent-open")),
                    RecentState::Missing => Some(focus_i18n::t("settings-recent-missing")),
                    RecentState::Available => None,
                };
                let mut row = div()
                    .id(("settings-recent", rank))
                    .flow_row()
                    .items_center()
                    .gap(size::S3)
                    .h(px(ROW))
                    .px(size::S3)
                    .border_start_2()
                    .border_color(gpui::transparent_black());
                if active {
                    let focus = self.recent_focus.get(rank).cloned();
                    row = row
                        .when_some(focus, |row, focus| row.track_focus(&focus).tab_index(0))
                        .focus(|style| style.border_color(t.ink))
                        .cursor_pointer()
                        .hover(|style| style.bg(t.card_2))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.activate_recent(rank, cx);
                        }))
                        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                this.activate_recent(rank, cx);
                                cx.stop_propagation();
                            }
                        }));
                }
                row.child(icon(Icon::FOLDER, size::ICON).text_color(t.ink_3))
                    .child(
                        div()
                            .flex_none()
                            .ui()
                            .text_color(if state == RecentState::Missing {
                                t.ink_3
                            } else {
                                t.ink
                            })
                            .child(name),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flow_row()
                            .justify_end()
                            .mono_sm()
                            .text_color(t.ink_3)
                            .child(ellipsis(full)),
                    )
                    .when_some(status, |row, status| {
                        row.child(
                            div()
                                .flex_none()
                                .flow_row()
                                .items_center()
                                .gap(size::S1)
                                .ui_sm()
                                .text_color(t.ink_3)
                                .when(state == RecentState::Open, |this| {
                                    this.child(
                                        icon(Icon::CHECK, size::ICON_DENSE).text_color(t.ink),
                                    )
                                })
                                .child(status),
                        )
                    })
            });
        div()
            .flex()
            .flex_col()
            .border_t_1()
            .border_b_1()
            .border_color(t.line)
            .children(rows)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_open_vault_is_marked_and_a_missing_one_is_inactive() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-tmp/recent");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Work")).unwrap();
        std::fs::create_dir_all(dir.join("Personal")).unwrap();
        let recent = vec![dir.join("Work"), dir.join("Gone"), dir.join("Personal")];
        let open = dir.join("Work").display().to_string();
        assert_eq!(
            states(&recent, Some(&open)),
            [
                RecentState::Open,
                RecentState::Missing,
                RecentState::Available
            ]
        );
        assert_eq!(
            states(&recent, None),
            [
                RecentState::Available,
                RecentState::Missing,
                RecentState::Available
            ]
        );
    }
}
