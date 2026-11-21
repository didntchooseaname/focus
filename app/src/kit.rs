use focus_kit::Flow as _;
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Checkbox, Chip, Field, Forced, Icon, IconButton, Kbd,
    MenuRow, NoteTab, Segment, Segments, Toast, Tokens, Tone, TreeRow, Type, dot, hatch, menu,
    menu_separator, size, tooltip_body,
};
use gpui::{
    AnyElement, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::input::InputState;

pub struct Gallery {
    empty: Entity<InputState>,
    filled: Entity<InputState>,
    focused: Entity<InputState>,
    invalid: Entity<InputState>,
    disabled: Entity<InputState>,
    language: u64,
}

impl Gallery {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field =
            |value: &str, placeholder: &str, window: &mut Window, cx: &mut Context<Self>| {
                let (value, placeholder) = (value.to_string(), placeholder.to_string());
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(placeholder)
                        .default_value(value)
                })
            };
        Gallery {
            empty: field("", &focus_i18n::t("ui-left-search-placeholder"), window, cx),
            filled: field("Method/Evidence handling", "", window, cx),
            focused: field("Kerberoasting", "", window, cx),
            invalid: field("42", "", window, cx),
            disabled: field("deepseek-v4-flash", "", window, cx),
            language: focus_i18n::generation(),
        }
    }
}

const STATES: [(Forced, bool, &str); 5] = [
    (Forced::Live, false, "ui-kit-state-rest"),
    (Forced::Hover, false, "ui-kit-state-hover"),
    (Forced::Pressed, false, "ui-kit-state-pressed"),
    (Forced::Focus, false, "ui-kit-state-focus"),
    (Forced::Live, true, "ui-kit-state-disabled"),
];

fn section(t: &Tokens, title: String) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(size::S3)
        .child(div().ui_xs().text_color(t.ink_3).child(title))
}

fn state_header(t: &Tokens, first: gpui::Pixels) -> gpui::Div {
    div()
        .flow_row()
        .child(div().w(first))
        .children(STATES.iter().map(|(_, _, label)| {
            div()
                .w(px(108.))
                .ui_xs()
                .text_color(t.ink_4)
                .child(focus_i18n::t(label))
        }))
}

fn column(t: &Tokens) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(size::S6)
        .p(size::S6)
        .border_end_1()
        .border_color(t.line)
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.language != focus_i18n::generation() {
            self.language = focus_i18n::generation();
            self.empty.update(cx, |state, cx| {
                state.set_placeholder(focus_i18n::t("ui-left-search-placeholder"), window, cx)
            });
        }
        let t = *cx.tokens();
        let label_w = px(84.);

        let kinds = [
            (
                ButtonKind::Primary,
                focus_i18n::t("ui-kit-primary"),
                focus_i18n::t("ui-action-export"),
            ),
            (
                ButtonKind::Secondary,
                focus_i18n::t("ui-kit-secondary"),
                focus_i18n::t("ui-action-rename"),
            ),
            (
                ButtonKind::Ghost,
                focus_i18n::t("ui-kit-ghost"),
                focus_i18n::t("ui-action-cancel"),
            ),
            (
                ButtonKind::Danger,
                focus_i18n::t("ui-kit-danger"),
                focus_i18n::t("ui-action-delete"),
            ),
        ];
        let buttons =
            section(&t, focus_i18n::t("ui-kit-buttons"))
                .child(state_header(&t, label_w))
                .children(kinds.into_iter().map(|(kind, name, label)| {
                    let id = format!("{kind:?}");
                    div()
                        .flow_row()
                        .items_center()
                        .child(div().w(label_w).ui_sm().text_color(t.ink_3).child(name))
                        .children(STATES.iter().enumerate().map(
                            move |(i, (forced, disabled, _))| {
                                div().w(px(108.)).child(
                                    Button::new(
                                        SharedString::from(format!("{id}-{i}")),
                                        label.clone(),
                                    )
                                    .kind(kind)
                                    .forced(*forced)
                                    .disabled(*disabled),
                                )
                            },
                        ))
                }))
                .child(
                    div()
                        .flow_row()
                        .items_center()
                        .child(
                            div()
                                .w(label_w)
                                .ui_sm()
                                .text_color(t.ink_3)
                                .child(focus_i18n::t("ui-kit-icon")),
                        )
                        .children(STATES.iter().enumerate().map(|(i, (forced, disabled, _))| {
                            div().w(px(108.)).child(
                                IconButton::new(
                                    ("icon", i),
                                    focus_kit::Dir::current().panel_start(),
                                    focus_i18n::t("ui-titlebar-toggle-left"),
                                )
                                .forced(*forced)
                                .disabled(*disabled),
                            )
                        })),
                )
                .child(
                    div()
                        .flow_row()
                        .items_center()
                        .gap(size::S2)
                        .child(
                            div()
                                .w(label_w - size::S2)
                                .ui_sm()
                                .text_color(t.ink_3)
                                .child(focus_i18n::t("ui-kit-large")),
                        )
                        .child(
                            Button::new("large-primary", focus_i18n::t("ui-vault-open-folder"))
                                .kind(ButtonKind::Primary)
                                .large(true)
                                .icon(Icon::FOLDER_OPEN),
                        )
                        .child(
                            Button::new("large-secondary", focus_i18n::t("ui-menu-export-html"))
                                .large(true),
                        ),
                );

        let segments = section(&t, focus_i18n::t("ui-kit-segments"))
            .children(
                [
                    (focus_i18n::t("ui-kit-state-rest"), None, false),
                    (
                        focus_i18n::t("ui-kit-state-hover"),
                        Some((1, Forced::Hover)),
                        false,
                    ),
                    (
                        focus_i18n::t("ui-kit-state-pressed"),
                        Some((2, Forced::Pressed)),
                        false,
                    ),
                    (
                        focus_i18n::t("ui-kit-state-focus"),
                        Some((0, Forced::Focus)),
                        false,
                    ),
                    (focus_i18n::t("ui-kit-state-disabled"), None, true),
                ]
                .into_iter()
                .enumerate()
                .map(|(i, (label, forced, disabled))| {
                    let items = vec![
                        Segment::new(focus_i18n::t("ui-view-source")),
                        Segment::new(focus_i18n::t("ui-view-live")),
                        Segment::new(focus_i18n::t("ui-view-reading")),
                    ];
                    let mut control = Segments::new(("segments", i), items, 0).disabled(disabled);
                    if let Some((index, state)) = forced {
                        control = control.forced(index, state);
                    }
                    div()
                        .flow_row()
                        .items_center()
                        .child(div().w(label_w).ui_sm().text_color(t.ink_3).child(label))
                        .child(control)
                }),
            )
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .child(
                        div()
                            .w(label_w)
                            .ui_sm()
                            .text_color(t.ink_3)
                            .child(focus_i18n::t("ui-kit-with-note")),
                    )
                    .child(Segments::new(
                        "segments-note",
                        vec![
                            Segment {
                                label: focus_i18n::t("ui-kit-all").into(),
                                icon: None,
                                note: None,
                                disabled: false,
                            },
                            Segment {
                                label: focus_i18n::t("ui-kit-critical").into(),
                                icon: None,
                                note: Some("3".into()),
                                disabled: false,
                            },
                            Segment {
                                label: focus_i18n::t("ui-kit-high").into(),
                                icon: None,
                                note: Some("7".into()),
                                disabled: false,
                            },
                        ],
                        1,
                    )),
            );

        let type_scale = section(&t, focus_i18n::t("ui-kit-typography"))
            .child(div().display().text_color(t.ink).child("Kerberoasting"))
            .child(
                div()
                    .display_sm()
                    .text_color(t.ink)
                    .child(focus_i18n::t("ui-kit-type-display-sm")),
            )
            .child(
                div()
                    .ui_lg()
                    .text_color(t.ink)
                    .child(focus_i18n::t("ui-kit-type-ui-lg")),
            )
            .child(
                div()
                    .ui()
                    .text_color(t.ink_2)
                    .child(focus_i18n::t("ui-kit-type-ui")),
            )
            .child(
                div()
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(focus_i18n::t("ui-kit-type-ui-sm")),
            )
            .child(
                div()
                    .mono()
                    .text_color(t.ink_body)
                    .child("GetUserSPNs.py -request corp.local/svc_sql"),
            )
            .child(
                div()
                    .flow_row()
                    .items_baseline()
                    .gap(size::S4)
                    .child(
                        div()
                            .stat()
                            .text_color(t.ink)
                            .child(focus_i18n::format_number(2545)),
                    )
                    .child(
                        div()
                            .ui_sm()
                            .text_color(t.ink_3)
                            .child(focus_i18n::t("ui-kit-sample-notes")),
                    ),
            );

        let fields = section(&t, focus_i18n::t("ui-kit-fields"))
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .child(
                        div()
                            .w(label_w)
                            .ui_sm()
                            .text_color(t.ink_3)
                            .child(focus_i18n::t("ui-kit-state-rest")),
                    )
                    .child(
                        div()
                            .w(px(300.))
                            .child(Field::new(&self.empty).icon(Icon::SEARCH)),
                    ),
            )
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .child(
                        div()
                            .w(label_w)
                            .ui_sm()
                            .text_color(t.ink_3)
                            .child(focus_i18n::t("ui-kit-state-hover")),
                    )
                    .child(
                        div()
                            .w(px(300.))
                            .child(Field::new(&self.filled).forced(Forced::Hover)),
                    ),
            )
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .child(
                        div()
                            .w(label_w)
                            .ui_sm()
                            .text_color(t.ink_3)
                            .child(focus_i18n::t("ui-kit-state-focus")),
                    )
                    .child(
                        div().w(px(300.)).child(
                            Field::new(&self.focused)
                                .forced(Forced::Focus)
                                .suffix(Kbd::new(focus_i18n::keys_text("ctrl-k"))),
                        ),
                    ),
            )
            .child(
                div()
                    .flow_row()
                    .items_start()
                    .child(
                        div()
                            .w(label_w)
                            .pt(px(5.))
                            .ui_sm()
                            .text_color(t.ink_3)
                            .child(focus_i18n::t("ui-kit-error")),
                    )
                    .child(
                        div().w(px(300.)).child(
                            Field::new(&self.invalid)
                                .suffix(
                                    div()
                                        .ui_sm()
                                        .text_color(t.ink_3)
                                        .child(focus_i18n::t("settings-unit-px")),
                                )
                                .error(Some(
                                    focus_i18n::t_args(
                                        "settings-number-refused",
                                        &[
                                            ("min", focus_i18n::Arg::Int(13)),
                                            ("max", focus_i18n::Arg::Int(20)),
                                            (
                                                "unit",
                                                format!(" {}", focus_i18n::t("settings-unit-px"))
                                                    .as_str()
                                                    .into(),
                                            ),
                                        ],
                                    )
                                    .into(),
                                )),
                        ),
                    ),
            )
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .child(
                        div()
                            .w(label_w)
                            .ui_sm()
                            .text_color(t.ink_3)
                            .child(focus_i18n::t("ui-kit-state-disabled")),
                    )
                    .child(
                        div()
                            .w(px(300.))
                            .child(Field::new(&self.disabled).disabled(true)),
                    ),
            );

        let checks = section(&t, focus_i18n::t("ui-kit-checkboxes")).child(
            div()
                .flow_row()
                .flex_wrap()
                .gap(size::S4)
                .child(Checkbox::new("check-rest", false).label(focus_i18n::t("ui-kit-state-rest")))
                .child(
                    Checkbox::new("check-hover", false)
                        .forced(Forced::Hover)
                        .label(focus_i18n::t("ui-kit-state-hover")),
                )
                .child(Checkbox::new("check-on", true).label(focus_i18n::t("ui-kit-checked")))
                .child(
                    Checkbox::new("check-focus", true)
                        .forced(Forced::Focus)
                        .label(focus_i18n::t("ui-kit-state-focus")),
                )
                .child(
                    Checkbox::new("check-off", true)
                        .disabled(true)
                        .label(focus_i18n::t("ui-kit-state-disabled")),
                ),
        );

        let chips = section(&t, focus_i18n::t("ui-kit-chips"))
            .child(
                div()
                    .flow_row()
                    .flex_wrap()
                    .gap(size::S2)
                    .child(Chip::new().key("status").value("ready"))
                    .child(Chip::new().key("type").value("technique"))
                    .child(
                        Chip::new()
                            .value("12")
                            .key(focus_i18n::t("ui-kit-sample-backlinks")),
                    )
                    .child(Chip::new().key("CVSS").value("8,8"))
                    .child(Chip::tag("attack/T1558"))
                    .child(Chip::tag("active-directory")),
            )
            .child(
                div()
                    .flow_row()
                    .gap(size::S2)
                    .child(Kbd::new(focus_i18n::keys_text("ctrl-k")))
                    .child(Kbd::new(focus_i18n::keys_text("ctrl-shift-f")))
                    .child(Kbd::new(focus_i18n::keys_text("escape")))
                    .child(Kbd::new(focus_i18n::keys_text("f2"))),
            );

        let status = section(&t, focus_i18n::t("ui-kit-status"))
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .gap(size::S4)
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(
                        div()
                            .flow_row()
                            .items_center()
                            .gap(size::S2)
                            .child(dot(t.ok))
                            .child(focus_i18n::t("ui-status-saved")),
                    )
                    .child(
                        div()
                            .flow_row()
                            .items_center()
                            .gap(size::S2)
                            .child(dot(t.ink_3))
                            .child(focus_i18n::t("ui-status-modified")),
                    )
                    .child(
                        div()
                            .flow_row()
                            .items_center()
                            .gap(size::S2)
                            .child(dot(t.warn))
                            .child(focus_i18n::t("ui-kit-conflict")),
                    )
                    .child(
                        div()
                            .flow_row()
                            .items_center()
                            .gap(size::S2)
                            .child(dot(t.danger))
                            .child(focus_i18n::t("ui-kit-index-error")),
                    ),
            )
            .child(div().flow_row().child(tooltip_body(
                &t,
                focus_i18n::t("ui-titlebar-toggle-left").into(),
                Some(focus_i18n::keys_text("ctrl-b").into()),
            )));

        let tabs = section(&t, focus_i18n::t("ui-kit-tabs"))
            .child(
                div()
                    .flow_row()
                    .border_start_1()
                    .border_color(t.line)
                    .child(
                        NoteTab::new("tab-active", "Kerberoasting", focus_i18n::t("ui-tab-close"))
                            .close_shortcut(crate::actions::shortcut("ui-cmd-close-tab"))
                            .active(true),
                    )
                    .child(
                        NoteTab::new("tab-rest", "AS-REP Roasting", focus_i18n::t("ui-tab-close"))
                            .close_shortcut(crate::actions::shortcut("ui-cmd-close-tab")),
                    )
                    .child(
                        NoteTab::new("tab-hover", "00_Home", focus_i18n::t("ui-tab-close"))
                            .close_shortcut(crate::actions::shortcut("ui-cmd-close-tab"))
                            .forced(Forced::Hover),
                    ),
            )
            .child(
                div()
                    .flow_row()
                    .border_start_1()
                    .border_color(t.line)
                    .child(
                        NoteTab::new(
                            "tab-dirty",
                            "Constats de l'engagement interne",
                            focus_i18n::t("ui-tab-close"),
                        )
                        .close_shortcut(crate::actions::shortcut("ui-cmd-close-tab"))
                        .dirty(true),
                    )
                    .child(
                        NoteTab::new(
                            "tab-pressed",
                            "NTLM coercing",
                            focus_i18n::t("ui-tab-close"),
                        )
                        .close_shortcut(crate::actions::shortcut("ui-cmd-close-tab"))
                        .forced(Forced::Pressed),
                    )
                    .child(
                        NoteTab::new("tab-focus", "IPV6 relaying", focus_i18n::t("ui-tab-close"))
                            .close_shortcut(crate::actions::shortcut("ui-cmd-close-tab"))
                            .forced(Forced::Focus),
                    ),
            );

        let tree = section(&t, focus_i18n::t("ui-kit-tree")).child(
            div()
                .w(px(300.))
                .border_1()
                .border_color(t.line)
                .py(size::S1)
                .child(TreeRow::new("tree-0", "Daily").folder(false).count(84))
                .child(TreeRow::new("tree-1", "Method").folder(true).count(612))
                .child(
                    TreeRow::new("tree-2", "01_Information_Gathering")
                        .depth(1)
                        .folder(false)
                        .count(97)
                        .forced(Forced::Hover),
                )
                .child(
                    TreeRow::new("tree-3", "03_Exploitation")
                        .depth(1)
                        .folder(true)
                        .count(41),
                )
                .child(
                    TreeRow::new("tree-4", "Kerberoasting")
                        .depth(2)
                        .active(true),
                )
                .child(
                    TreeRow::new("tree-5", "NTLM coercing")
                        .depth(2)
                        .forced(Forced::Pressed),
                )
                .child(
                    TreeRow::new("tree-6", "IPV6 relaying")
                        .depth(2)
                        .forced(Forced::Focus),
                )
                .child(TreeRow::new("tree-7", "Exploit_Tools").depth(2)),
        );

        let context_menu = section(&t, focus_i18n::t("ui-kit-menu")).child(
            div().flow_row().child(
                menu(&t)
                    .w(px(260.))
                    .child(
                        MenuRow::new("menu-open", focus_i18n::t("ui-menu-open-new-tab"))
                            .icon(Icon::FILE_TEXT)
                            .shortcut(focus_i18n::keys_text("ctrl-enter")),
                    )
                    .child(
                        MenuRow::new("menu-rename", focus_i18n::t("ui-action-rename"))
                            .icon(Icon::PENCIL)
                            .shortcut(focus_i18n::keys_text("f2"))
                            .highlighted(true),
                    )
                    .child(
                        MenuRow::new("menu-move", focus_i18n::t("ui-menu-move-to"))
                            .icon(Icon::FOLDER),
                    )
                    .child(
                        MenuRow::new("menu-copy", focus_i18n::t("ui-menu-copy-link"))
                            .icon(Icon::LINK)
                            .forced(Forced::Pressed),
                    )
                    .child(
                        MenuRow::new("menu-export", focus_i18n::t("ui-menu-export-html"))
                            .icon(Icon::EXTERNAL)
                            .disabled(true),
                    )
                    .child(menu_separator(&t))
                    .child(
                        MenuRow::new("menu-delete", focus_i18n::t("ui-action-delete"))
                            .icon(Icon::TRASH)
                            .danger(true)
                            .shortcut(focus_i18n::keys_text("delete")),
                    ),
            ),
        );

        let toasts = section(&t, focus_i18n::t("ui-kit-toasts"))
            .child(
                div()
                    .flow_row()
                    .child(Toast::new(Tone::Neutral, focus_i18n::t("ui-toast-renamed"))),
            )
            .child(
                div().flow_row().child(Toast::new(
                    Tone::Ok,
                    focus_i18n::t_args(
                        "ui-toast-indexed",
                        &[
                            ("count", focus_i18n::format_number(2545).as_str().into()),
                            (
                                "duration",
                                focus_i18n::format_duration(std::time::Duration::from_millis(1840))
                                    .as_str()
                                    .into(),
                            ),
                        ],
                    ),
                )),
            )
            .child(
                div()
                    .flow_row()
                    .child(Toast::new(Tone::Warn, focus_i18n::t("ui-toast-conflict"))),
            )
            .child(div().flow_row().child(Toast::new(
                Tone::Danger,
                focus_i18n::t("errors-ai-invalid-key"),
            )));

        let hatched = section(&t, focus_i18n::t("ui-kit-hatch")).child(
            div()
                .flow_row()
                .h(px(108.))
                .border_1()
                .border_color(t.line)
                .child(div().flex_1().h_full().child(hatch()))
                .child(
                    div()
                        .w(px(160.))
                        .h_full()
                        .border_start_1()
                        .border_end_1()
                        .border_color(t.line)
                        .bg(t.bg)
                        .p(size::S3)
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(focus_i18n::t("ui-kit-sheet")),
                )
                .child(div().flex_1().h_full().child(hatch())),
        );

        let columns: Vec<AnyElement> = vec![
            column(&t)
                .w(px(672.))
                .child(buttons)
                .child(segments)
                .child(type_scale)
                .into_any_element(),
            column(&t)
                .w(px(432.))
                .child(fields)
                .child(checks)
                .child(chips)
                .child(status)
                .child(hatched)
                .into_any_element(),
            column(&t)
                .flex_1()
                .min_w_0()
                .border_r_0()
                .child(tabs)
                .child(tree)
                .child(context_menu)
                .child(toasts)
                .into_any_element(),
        ];

        div()
            .id("kit")
            .size_full()
            .overflow_y_scroll()
            .bg(t.bg)
            .child(div().flow_row().min_h_full().children(columns))
    }
}
