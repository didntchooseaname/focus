use focus_kit::Flow as _;
use focus_kit::{
    ActiveTokens, Icon, IconButton, Kbd, Segment, Segments, Type, dot, ellipsis, icon, size,
};
use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::actions::*;
use crate::editor::ViewMode;
use crate::shell::Shell;

pub fn resident_memory() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096)
}

impl Shell {
    pub fn render_titlebar(&mut self, _: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = *cx.tokens();
        let active = self.active_editor().map(|e| e.read(cx));
        let view = active.map(|e| e.view);
        let mut crumbs: Vec<(SharedString, Option<std::path::PathBuf>)> = Vec::new();
        let title = active.map(|e| SharedString::from(e.meta.title.to_string()));
        if let Some(editor) = active
            && editor.loose.is_some()
        {
            crumbs.push((focus_i18n::t("ui-loose-crumb").into(), None));
            if let Some(path) = &editor.loose {
                crumbs.extend(
                    crate::loose::crumbs(path)
                        .into_iter()
                        .map(|part| (SharedString::from(part), None)),
                );
            }
        } else {
            if let Some(library) = &self.library {
                crumbs.push((
                    library.name().to_string().into(),
                    Some(std::path::PathBuf::new()),
                ));
            }
            if let Some(editor) = active
                && let Some(parent) = editor.meta.path.parent()
            {
                let mut folder = std::path::PathBuf::new();
                for part in parent.iter() {
                    folder.push(part);
                    crumbs.push((
                        SharedString::from(part.to_string_lossy().into_owned()),
                        Some(folder.clone()),
                    ));
                }
            }
        }
        let ops = self.prefs.read(cx).bool("ops.enabled");
        let search_forced = self.forced("titlebar.search");
        if matches!(
            self.screen,
            crate::shell::Screen::Welcome { .. }
                | crate::shell::Screen::Updates { .. }
                | crate::shell::Screen::Vaults
        ) {
            return div()
                .flex_none()
                .h(size::TITLE_BAR)
                .border_b_1()
                .border_color(t.line)
                .into_any_element();
        }

        div()
            .flow_row()
            .flex_none()
            .items_center()
            .gap(size::S1)
            .h(size::TITLE_BAR)
            .px(size::S2)
            .border_b_1()
            .border_color(t.line)
            .child(
                IconButton::new(
                    "toggle-left",
                    focus_kit::Dir::current().panel_start(),
                    focus_i18n::t("ui-titlebar-toggle-left"),
                )
                .shortcut(crate::actions::shortcut("ui-cmd-toggle-left"))
                .active(self.layout.left_open)
                .tab_stop(false)
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleLeft), cx)),
            )
            .child(
                IconButton::new(
                    "back",
                    focus_kit::Dir::current().arrow_back(),
                    focus_i18n::t("ui-titlebar-back"),
                )
                .shortcut(crate::actions::shortcut("ui-cmd-back"))
                .disabled(!self.can_go_back())
                .tab_stop(false)
                .on_click(|_, window, cx| window.dispatch_action(Box::new(Back), cx)),
            )
            .child(
                IconButton::new(
                    "forward",
                    focus_kit::Dir::current().arrow_forward(),
                    focus_i18n::t("ui-titlebar-forward"),
                )
                .shortcut(crate::actions::shortcut("ui-cmd-forward"))
                .disabled(!self.can_go_forward())
                .tab_stop(false)
                .on_click(|_, window, cx| window.dispatch_action(Box::new(Forward), cx)),
            )
            .child(
                div()
                    .flow_row()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(size::S2)
                    .margin_start(size::S2)
                    .ui()
                    .overflow_hidden()
                    .children(crumbs.into_iter().enumerate().map(|(i, (crumb, folder))| {
                        div()
                            .flow_row()
                            .flex_shrink()
                            .min_w_0()
                            .items_center()
                            .gap(size::S2)
                            .when(i > 0, |this| {
                                this.child(div().flex_none().text_color(t.ink_4).child("/"))
                            })
                            .child(
                                div()
                                    .id(("crumb", i))
                                    .min_w_0()
                                    .px(px(2.))
                                    .rounded(size::RADIUS)
                                    .text_color(t.ink_3)
                                    .when_some(folder, |this, folder| {
                                        this.cursor_pointer()
                                            .hover(move |s| s.text_color(t.ink).bg(t.card_2))
                                            .active(move |s| s.bg(t.card_3))
                                            .on_click(cx.listener(
                                                move |this, event: &gpui::ClickEvent, _, cx| {
                                                    this.open_crumb_menu(
                                                        folder.clone(),
                                                        event.position().x,
                                                        cx,
                                                    )
                                                },
                                            ))
                                    })
                                    .child(ellipsis(crumb)),
                            )
                    }))
                    .when_some(title, |this, title| {
                        this.child(div().flex_none().text_color(t.ink_4).child("/"))
                            .child(
                                div()
                                    .id("crumb-note")
                                    .flex_shrink_0()
                                    .max_w(px(360.))
                                    .px(px(2.))
                                    .rounded(size::RADIUS)
                                    .text_color(t.ink)
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(t.card_2))
                                    .active(move |s| s.bg(t.card_3))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.reveal_note_in_files(cx)),
                                    )
                                    .child(ellipsis(title)),
                            )
                    }),
            )
            .child(
                div()
                    .id("titlebar-search")
                    .flow_row()
                    .flex_shrink()
                    .min_w(px(160.))
                    .items_center()
                    .gap(size::S2)
                    .w(px(260.))
                    .h(size::CONTROL)
                    .pad_start(size::S2)
                    .pad_end(size::S1)
                    .margin_end(size::S2)
                    .rounded(size::RADIUS)
                    .border_1()
                    .border_color(if search_forced.hover() {
                        t.ink_3
                    } else {
                        t.line_2
                    })
                    .bg(t.bg_2)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(t.ink_3))
                    .active(move |s| s.bg(t.card_2))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenPalette), cx))
                    .child(icon(Icon::SEARCH, size::ICON_DENSE).text_color(t.ink_3))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flow_row()
                            .ui()
                            .text_color(t.ink_3)
                            .child(ellipsis(focus_i18n::t("ui-titlebar-search-prompt"))),
                    )
                    .child(Kbd::new(crate::actions::shortcut("ui-cmd-open-note"))),
            )
            .child(
                Segments::new(
                    "view-mode",
                    vec![
                        Segment::new(focus_i18n::t("ui-view-source")),
                        Segment::new(focus_i18n::t("ui-view-live")),
                        Segment::new(focus_i18n::t("ui-view-reading")),
                    ],
                    match view {
                        Some(ViewMode::Live) => 1,
                        Some(ViewMode::Reading) => 2,
                        _ => 0,
                    },
                )
                .disabled(view.is_none())
                .tab_stop(false)
                .on_select(|index, window, cx| {
                    let action: Box<dyn gpui::Action> = match index {
                        1 => Box::new(ViewLive),
                        2 => Box::new(ViewReading),
                        _ => Box::new(ViewSource),
                    };
                    window.dispatch_action(action, cx);
                }),
            )
            .when_some(
                self.ops.as_ref().map(|ops| ops.indicator.clone()),
                |this, indicator| this.child(div().margin_start(size::S2).child(indicator)),
            )
            .when(ops && self.ops.is_none(), |this| {
                this.child(
                    div()
                        .flow_row()
                        .items_center()
                        .gap(size::S2)
                        .margin_start(size::S2)
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(dot(t.ink_3))
                        .child(focus_i18n::t("ui-ops-label")),
                )
            })
            .child(div().w(size::S2))
            .when(self.library.is_some(), |this| {
                this.child(
                    IconButton::new(
                        "vault-graph",
                        Icon::NETWORK,
                        focus_i18n::t("ui-cmd-graph-vault"),
                    )
                    .tab_stop(false)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenVaultGraph), cx)),
                )
            })
            .child(
                IconButton::new(
                    "toggle-bottom",
                    Icon::PANEL_BOTTOM,
                    focus_i18n::t("ui-titlebar-toggle-bottom"),
                )
                .shortcut(crate::actions::shortcut("ui-cmd-toggle-bottom"))
                .active(self.layout.bottom_open)
                .tab_stop(false)
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleBottom), cx)),
            )
            .child(
                IconButton::new("settings", Icon::SETTINGS, focus_i18n::t("settings-title"))
                    .shortcut(crate::actions::shortcut("ui-cmd-settings"))
                    .tab_stop(false)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenSettings), cx)),
            )
            .child(
                IconButton::new(
                    "toggle-right",
                    focus_kit::Dir::current().panel_end(),
                    focus_i18n::t("ui-titlebar-toggle-right"),
                )
                .shortcut(crate::actions::shortcut("ui-cmd-toggle-right"))
                .active(self.layout.right_open)
                .tab_stop(false)
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleRight), cx)),
            )
            .into_any_element()
    }

    pub fn render_statusbar(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = *cx.tokens();
        let editor = self.active_editor().map(|e| e.read(cx));
        let cursor = editor.and_then(|e| e.status_cursor(cx));
        let links = match (editor, &self.library) {
            (Some(editor), Some(library)) => {
                let backlinks = library.vault().backlinks(editor.id).len();
                let broken = self.prefs.read(cx).bool("editor.check_links").then(|| {
                    library
                        .vault()
                        .links_of(editor.id)
                        .iter()
                        .filter(|(_, target)| matches!(target, focus_core::LinkTarget::Unresolved))
                        .count()
                });
                Some((backlinks, broken))
            }
            _ => None,
        };
        let model = match self
            .prefs
            .read(cx)
            .get("ai.model")
            .filter(|_| self.prefs.read(cx).bool("ai.enabled"))
        {
            Some(crate::settings::schema::Value::Text(model)) if !model.is_empty() => {
                Some(model.to_string())
            }
            _ => None,
        };
        let item = || div().flow_row().flex_none().items_center().gap(size::S2);
        let (file_path, file_items) = self.render_file_status(cx);
        if matches!(
            self.screen,
            crate::shell::Screen::Welcome { .. }
                | crate::shell::Screen::Updates { .. }
                | crate::shell::Screen::Vaults
        ) {
            return div()
                .flex_none()
                .h(size::STATUS_BAR)
                .border_t_1()
                .border_color(t.line)
                .into_any_element();
        }
        div()
            .flow_row()
            .flex_none()
            .items_center()
            .gap(size::S4)
            .h(size::STATUS_BAR)
            .px(size::S3)
            .border_t_1()
            .border_color(t.line)
            .ui_sm()
            .text_color(t.ink_3)
            .when_some(editor, |this, editor| {
                this.child(
                    item()
                        .child(dot(if editor.conflict {
                            t.warn
                        } else if editor.dirty {
                            t.ink_3
                        } else {
                            t.ok
                        }))
                        .child(if editor.conflict {
                            focus_i18n::t("ui-status-conflict")
                        } else if editor.dirty {
                            focus_i18n::t("ui-status-modified")
                        } else {
                            focus_i18n::t("ui-status-saved")
                        }),
                )
                .child(
                    div()
                        .flex_shrink()
                        .min_w_0()
                        .mono_sm()
                        .line_height(px(18.))
                        .child(ellipsis(
                            editor
                                .loose
                                .as_deref()
                                .unwrap_or(&editor.meta.path)
                                .display()
                                .to_string(),
                        )),
                )
            })
            .children(file_path)
            .when_some(self.import, |this, import| {
                this.child(
                    item()
                        .child(focus_i18n::t_args(
                            "ui-status-copying",
                            &[
                                ("count", import.files.into()),
                                (
                                    "size",
                                    focus_i18n::format_bytes(import.bytes).as_str().into(),
                                ),
                            ],
                        ))
                        .child(
                            div()
                                .id("status-copy-cancel")
                                .flow_row()
                                .items_center()
                                .h(px(20.))
                                .px(px(6.))
                                .rounded(size::RADIUS)
                                .text_color(t.ink_2)
                                .cursor_pointer()
                                .hover(move |s| s.bg(t.card_2).text_color(t.ink))
                                .active(move |s| s.bg(t.card_3))
                                .on_click(cx.listener(|this, _, _, cx| this.cancel_import(cx)))
                                .child(focus_i18n::t("ui-status-copy-cancel")),
                        ),
                )
            })
            .when_some(self.history_error.clone(), |this, error| {
                let tip: SharedString = error.clone().into();
                this.child(
                    item()
                        .id("status-history")
                        .flex_shrink()
                        .min_w_0()
                        .max_w(px(480.))
                        .px(size::S1)
                        .rounded(size::RADIUS)
                        .text_color(t.danger)
                        .cursor_pointer()
                        .hover(move |s| s.bg(t.card_2))
                        .active(move |s| s.bg(t.card_3))
                        .child(dot(t.danger))
                        .child(ellipsis(focus_i18n::t_args(
                            "ui-status-history",
                            &[("error", error.as_str().into())],
                        )))
                        .tooltip(move |_, cx| {
                            focus_kit::tooltip::Tooltip::view(tip.clone(), None, cx)
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.history_error = None;
                            cx.notify();
                        })),
                )
            })
            .child(div().flex_1())
            .children(file_items)
            .when_some(cursor, |this, cursor| {
                this.child(
                    item()
                        .mono_sm()
                        .line_height(px(18.))
                        .child(focus_i18n::t_args(
                            "ui-status-cursor",
                            &[
                                ("line", (cursor.line + 1).into()),
                                ("column", (cursor.character + 1).into()),
                            ],
                        )),
                )
            })
            .when_some(editor, |this, editor| {
                this.child(counted("ui-status-words", editor.meta.words as i64))
            })
            .when_some(links, |this, (backlinks, broken)| {
                this.child(counted("ui-status-backlinks", backlinks as i64))
                    .when_some(broken.filter(|n| *n > 0), |this, broken| {
                        this.child(counted("ui-status-broken", broken as i64).text_color(t.warn))
                    })
            })
            .when(self.library.is_some(), |this| {
                this.child(counted("ui-status-index", self.facts.notes as i64))
            })
            .when_some(self.agent.last(), |this, last| {
                let tip: SharedString = focus_i18n::t_args(
                    "ui-status-agent-tip",
                    &[(
                        "time",
                        focus_i18n::format_datetime(last.timestamp())
                            .as_str()
                            .into(),
                    )],
                )
                .into();
                this.child(
                    item()
                        .id("status-agent")
                        .text_color(t.warn)
                        .child(dot(t.warn))
                        .child(focus_i18n::t("ui-status-agent"))
                        .tooltip(move |_, cx| {
                            focus_kit::tooltip::Tooltip::view(tip.clone(), None, cx)
                        }),
                )
            })
            .when_some(model, |this, model| {
                this.child(item().mono_sm().line_height(px(18.)).child(model))
            })
            .when_some(resident_memory(), |this, rss| {
                this.child(
                    item()
                        .mono_sm()
                        .line_height(px(18.))
                        .child(focus_i18n::format_bytes(rss)),
                )
            })
            .into_any_element()
    }
}

fn counted(key: &str, count: i64) -> gpui::Div {
    let shown = focus_i18n::format_number(count);
    let text = focus_i18n::t_args(
        key,
        &[
            ("count", focus_i18n::Arg::Int(count)),
            ("shown", shown.as_str().into()),
        ],
    );
    let row = div().flow_row().flex_none().items_center();
    match around(&text, &shown) {
        Some((before, after)) => row
            .child(before.to_string())
            .child(div().mono_sm().line_height(px(18.)).child(shown.clone()))
            .child(after.to_string()),
        None => row.child(text),
    }
}

fn around<'a>(text: &'a str, part: &str) -> Option<(&'a str, &'a str)> {
    let at = text.find(part)?;
    Some((&text[..at], &text[at + part.len()..]))
}

#[cfg(test)]
mod tests {
    use super::around;

    #[test]
    fn a_count_splits_around_its_number_or_stays_whole() {
        assert_eq!(around("1,204 words", "1,204"), Some(("", " words")));
        assert_eq!(
            around("已索引 82 篇笔记", "82"),
            Some(("已索引 ", " 篇笔记"))
        );
        assert_eq!(around("كلمتان", "2"), None);
    }
}
