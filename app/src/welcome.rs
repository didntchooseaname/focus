use focus_kit::{ActiveTokens, Button, ButtonKind, Flow as _, Icon, Type, ellipsis, icon, size};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::settings::schema::{LANGUAGE_CODES, Value};
use crate::shell::{Screen, Shell};

pub fn languages() -> Vec<&'static str> {
    LANGUAGE_CODES.iter().flatten().copied().collect()
}

pub fn preselected() -> usize {
    let system = focus_i18n::detect_system();
    languages()
        .iter()
        .position(|code| *code == system)
        .unwrap_or(0)
}

const WIDTH: f32 = 400.;
const ROW: f32 = 40.;

impl Shell {
    pub fn welcome_select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let codes = languages();
        let index = index.min(codes.len() - 1);
        self.screen = Screen::Welcome { selected: index };
        self.apply_language(codes[index], window, cx);
    }

    pub fn welcome_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Screen::Welcome { selected } = self.screen else {
            return;
        };
        let code = languages()[selected];
        let choice = LANGUAGE_CODES
            .iter()
            .position(|c| *c == Some(code))
            .unwrap_or(0);
        self.set_pref("general.language", Value::Choice(choice), cx);
        self.store.settings.language = Some(code.to_string());
        self.schedule_save(cx);
        if self.vault_root.is_some() {
            self.screen = Screen::Workspace;
        } else {
            self.screen = Screen::Vaults;
        }
        self.focus_content(window, cx);
        cx.notify();
    }

    pub fn render_welcome(&mut self, selected: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        let rows = languages().into_iter().enumerate().map(|(index, code)| {
            let current = index == selected;
            let name = focus_i18n::t(&format!("ui-language-name-{}", code.to_lowercase()));
            div()
                .id(("language", index))
                .relative()
                .flow_row()
                .items_center()
                .gap(size::S3)
                .h(px(ROW))
                .px(size::S4)
                .cursor_pointer()
                .ui_lg()
                .text_color(if current { t.ink } else { t.ink_2 })
                .when(current, |row| {
                    row.bg(t.card_3).child(
                        div()
                            .absolute()
                            .start_at(px(0.))
                            .top_0()
                            .bottom_0()
                            .w(px(2.))
                            .bg(t.ink),
                    )
                })
                .when(!current, |row| {
                    row.hover(move |s| s.bg(t.card_2).text_color(t.ink))
                })
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                        this.welcome_select(index, window, cx);
                        if event.click_count() > 1 {
                            this.welcome_confirm(window, cx);
                        }
                    }),
                )
                .child(div().flex_1().min_w_0().text_start().child(name))
                .when(current, |row| {
                    row.child(icon(Icon::CHECK, size::ICON).text_color(t.ink))
                })
        });
        centered(
            div()
                .flex()
                .flex_col()
                .w(px(WIDTH))
                .max_w_full()
                .gap(size::S6)
                .child(heading(
                    &t,
                    focus_i18n::t("ui-welcome-title"),
                    focus_i18n::t("ui-welcome-subtitle"),
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .border_t_1()
                        .border_b_1()
                        .border_color(t.line)
                        .children(rows),
                )
                .child(
                    div().flow_row().child(
                        Button::new("welcome-continue", focus_i18n::t("ui-welcome-continue"))
                            .kind(ButtonKind::Primary)
                            .large(true)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.welcome_confirm(window, cx)),
                            ),
                    ),
                ),
        )
    }

    pub fn render_vaults(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        let recent: Vec<_> = self
            .store
            .settings
            .recent_vaults
            .iter()
            .filter(|p| p.is_dir())
            .cloned()
            .collect();
        let rows = recent.into_iter().enumerate().map(|(index, path)| {
            let name: SharedString = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into();
            let full: SharedString = path.display().to_string().into();
            div()
                .id(("recent-vault", index))
                .flow_row()
                .items_center()
                .gap(size::S3)
                .h(px(ROW))
                .px(size::S4)
                .cursor_pointer()
                .hover(move |s| s.bg(t.card_2))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.screen = Screen::Workspace;
                    this.load_vault(path.clone(), window, cx);
                }))
                .child(icon(Icon::FOLDER, size::ICON).text_color(t.ink_3))
                .child(div().flex_none().ui().text_color(t.ink).child(name))
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
        });
        let rows: Vec<_> = rows.collect();
        centered(
            div()
                .flex()
                .flex_col()
                .w(px(WIDTH * 1.4))
                .max_w_full()
                .gap(size::S6)
                .child(heading(
                    &t,
                    focus_i18n::t("ui-vault-picker-title"),
                    focus_i18n::t("ui-vault-picker-subtitle"),
                ))
                .child(
                    div().flow_row().child(
                        Button::new("vault-open", focus_i18n::t("ui-vault-open-folder"))
                            .kind(ButtonKind::Primary)
                            .large(true)
                            .icon(Icon::FOLDER_OPEN)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.prompt_vault(window, cx)),
                            ),
                    ),
                )
                .when(!rows.is_empty(), |this| {
                    this.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(size::S2)
                            .child(
                                div()
                                    .ui_xs()
                                    .text_start()
                                    .text_color(t.ink_3)
                                    .child(focus_i18n::t("ui-vault-recent")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .border_t_1()
                                    .border_b_1()
                                    .border_color(t.line)
                                    .children(rows),
                            ),
                    )
                }),
        )
    }
}

fn heading(t: &focus_kit::Tokens, title: String, subtitle: String) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(size::S2)
        .child(
            div()
                .display_sm()
                .text_start()
                .text_color(t.ink)
                .child(title),
        )
        .child(div().ui().text_start().text_color(t.ink_3).child(subtitle))
}

fn centered(content: gpui::Div) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .px(size::S6)
        .child(content)
        .into_any_element()
}
