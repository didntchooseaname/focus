use std::collections::HashSet;

use focus_i18n::{Arg, t_args};
use focus_kit::{
    ActiveTokens, Button, ButtonKind, Field, Flow as _, Icon, IconButton, MenuRow, Type, icon,
    menu, size,
};
use focus_ops::{Credential, loot};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};

use crate::event::{NoticeTone, OpsEvent};
use crate::state::{OpsState, error_text};
use crate::ui::{hatched_empty, mono_chip, screen_header, t_};

const EYE: Icon = Icon("\u{E0BA}");
const EYE_OFF: Icon = Icon("\u{E0BB}");
const KINDS: [&str; 7] = [
    "password", "ntlm", "aes256", "ticket", "ssh-key", "token", "hash",
];
const MASK: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

pub struct CredentialsTable {
    state: Entity<OpsState>,
    revealed: HashSet<(String, String, String)>,
    user: Entity<InputState>,
    generation: u64,
    domain: Entity<InputState>,
    secret: Entity<InputState>,
    kind: usize,
    kind_menu: bool,
    menus: crate::ui::MenuFocus,
    copied: crate::ui::CopyConfirm,
    focus: gpui::FocusHandle,
    rows_scroll: gpui::ScrollHandle,
    compact: crate::rich::Compact,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<OpsEvent> for CredentialsTable {}

fn identity(c: &Credential) -> (String, String, String) {
    (c.user.clone(), c.kind.clone(), c.domain.clone())
}

impl CredentialsTable {
    pub fn use_credential(&mut self, credential: focus_ops::Credential, cx: &mut Context<Self>) {
        let names = self
            .state
            .update(cx, |state, cx| state.use_credential(&credential, cx));
        if !names.is_empty() {
            cx.emit(OpsEvent::Notify {
                tone: NoticeTone::Ok,
                text: t_args(
                    "ops-credentials-used",
                    &[("names", Arg::from(names.join(", ").as_str()))],
                ),
            });
        }
    }

    fn jump(&mut self, jump: crate::ui::Jump, _: &mut Window, cx: &mut Context<Self>) {
        crate::ui::scroll_jump(&self.rows_scroll, jump);
        cx.notify();
    }

    pub fn new(
        state: Entity<OpsState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> CredentialsTable {
        let user = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-credentials-column-user"))
        });
        let domain = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-credentials-column-domain"))
        });
        let secret = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-credentials-column-secret"))
                .masked(true)
        });
        let observe = cx.observe(&state, |_, _, cx| cx.notify());
        let enter = cx.subscribe_in(
            &secret,
            window,
            |this: &mut CredentialsTable, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.add(window, cx);
                }
            },
        );
        let enter_user = cx.subscribe_in(
            &user,
            window,
            |this: &mut CredentialsTable, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.confirm_or_next(window, cx);
                }
            },
        );
        let enter_domain = cx.subscribe_in(
            &domain,
            window,
            |this: &mut CredentialsTable, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.confirm_or_next(window, cx);
                }
            },
        );
        CredentialsTable {
            generation: focus_i18n::generation(),
            state,
            revealed: HashSet::new(),
            user,
            domain,
            secret,
            kind: 0,
            kind_menu: false,
            menus: crate::ui::MenuFocus::new(cx),
            copied: Default::default(),
            focus: crate::ui::list_focus(cx),
            rows_scroll: gpui::ScrollHandle::new(),
            compact: crate::rich::Compact::new(),
            _subscriptions: vec![observe, enter, enter_user, enter_domain],
        }
    }

    pub fn toggle_reveal(&mut self, n: usize, cx: &mut Context<Self>) {
        if let Some(credential) = self
            .state
            .read(cx)
            .active
            .as_ref()
            .and_then(|a| a.credentials.get(n))
        {
            let key = identity(credential);
            if !self.revealed.remove(&key) {
                self.revealed.insert(key);
            }
            cx.notify();
        }
    }

    fn confirm_or_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.secret.read(cx).value().is_empty() {
            self.secret.update(cx, |input, cx| input.focus(window, cx));
        } else {
            self.add(window, cx);
        }
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let read = |input: &Entity<InputState>, cx: &Context<Self>| {
            input.read(cx).value().trim().to_string()
        };
        let credential = Credential {
            user: read(&self.user, cx),
            kind: KINDS[self.kind].into(),
            secret: read(&self.secret, cx),
            domain: read(&self.domain, cx),
            source: String::new(),
            valid_on: String::new(),
            notes: String::new(),
        };
        if credential.user.is_empty() || credential.secret.is_empty() {
            return;
        }
        let Some(dir) = self
            .state
            .read(cx)
            .active
            .as_ref()
            .map(|a| a.engagement.dir.clone())
        else {
            return;
        };
        for input in [&self.user, &self.domain, &self.secret] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.save(dir, credential, cx);
    }

    pub fn save(
        &mut self,
        dir: std::path::PathBuf,
        credential: Credential,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let written = credential.clone();
            let added = cx
                .background_spawn(async move { loot::add(&dir, &written) })
                .await;
            this.update(cx, |this, cx| match added {
                Ok(true) => this.state.update(cx, |state, cx| {
                    state.credential_added(credential, cx);
                    state.reload(cx);
                }),
                Ok(false) => cx.emit(OpsEvent::Notify {
                    tone: NoticeTone::Warn,
                    text: t_("ops-credentials-duplicate"),
                }),
                Err(error) => cx.emit(OpsEvent::Notify {
                    tone: NoticeTone::Danger,
                    text: error_text(&error),
                }),
            })
            .ok();
        })
        .detach();
    }
}

impl Render for CredentialsTable {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.menus.sync(self.kind_menu, window, cx);
        let t = *cx.tokens();
        let mask = self.state.read(cx).settings.mask_secrets;
        if self.generation != focus_i18n::generation() {
            self.generation = focus_i18n::generation();
            self.user.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-credentials-column-user"), window, cx)
            });
            self.domain.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-credentials-column-domain"), window, cx)
            });
            self.secret.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-credentials-column-secret"), window, cx)
            });
        }

        let Some(active) = self.state.read(cx).active.clone() else {
            return div()
                .id("ops-credentials")
                .size_full()
                .bg(t.bg)
                .flex()
                .child(hatched_empty(&t, t_("ops-no-engagement"), None));
        };
        let cells: Vec<(AnyElement, AnyElement)> = active
            .credentials
            .iter()
            .enumerate()
            .map(|(i, c)| {
                (
                    self.compact
                        .element(format!("source-{i}"), &c.source, &active.hosts, cx),
                    self.compact
                        .element(format!("valid-{i}"), &c.valid_on, &active.hosts, cx),
                )
            })
            .collect();
        let mut cells = cells.into_iter();
        let count = t_args(
            "ops-credentials-count",
            &[("count", Arg::from(active.credentials.len()))],
        );

        let kind_button = div()
            .id("ops-credential-kind")
            .flow_row()
            .items_center()
            .justify_between()
            .gap(size::S2)
            .flex_none()
            .w(px(120.))
            .h(size::CONTROL)
            .px(size::S2)
            .rounded(size::RADIUS)
            .border_1()
            .border_color(t.line_2)
            .bg(t.bg_2)
            .cursor_pointer()
            .hover(move |s| s.border_color(t.ink_3))
            .ui()
            .text_color(t.ink)
            .on_click(cx.listener(|this, _, _, cx| {
                this.kind_menu = this.menus.click_opens(this.kind_menu);
                cx.notify();
            }))
            .child(div().mono_sm().child(KINDS[self.kind]))
            .child(icon(Icon::CHEVRON_DOWN, size::ICON_SMALL).text_color(t.ink_3));
        let kind_menu = self.kind_menu.then(|| {
            let entity = cx.entity();
            let dismiss = move |_: &mut Window, cx: &mut gpui::App| {
                entity.update(cx, |this, cx| {
                    this.kind_menu = false;
                    this.menus.dismissed();
                    cx.notify();
                })
            };
            focus_kit::fade_in(
                "fade-ops-kind-menu",
                self.menus
                    .attach(div().absolute().top(px(32.)).start_at(px(0.)), dismiss)
                    .child(
                        menu(&t)
                            .min_w(px(120.))
                            .children(KINDS.iter().enumerate().map(|(i, kind)| {
                                MenuRow::new(("ops-credential-kind-row", i), *kind)
                                    .checked(i == self.kind)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.kind = i;
                                        this.kind_menu = false;
                                        cx.notify();
                                    }))
                            })),
                    ),
                cx,
            )
        });

        let quick_add = crate::ui::row()
            .items_start()
            .gap(size::S2)
            .child(div().flex_1().min_w_0().child(Field::new(&self.user)))
            .child(div().flex_1().min_w_0().child(Field::new(&self.domain)))
            .child(div().flex_1().min_w_0().child(Field::new(&self.secret)))
            .child(div().relative().child(kind_button).children(kind_menu))
            .child(
                Button::new("ops-credential-add", t_("ops-credentials-add"))
                    .kind(ButtonKind::Primary)
                    .icon(Icon::PLUS)
                    .on_click(cx.listener(|this, _, window, cx| this.add(window, cx))),
            );

        let header = crate::ui::row()
            .items_center()
            .gap(size::S3)
            .h(px(28.))
            .px(size::S3)
            .border_b_1()
            .border_color(t.line)
            .ui_xs()
            .text_color(t.ink_3)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(t_("ops-credentials-column-user")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(t_("ops-credentials-column-domain")),
            )
            .child(div().flex_none().w(px(84.)).child(focus_kit::truncated(
                "ops-credentials-kind-header",
                t_("ops-credentials-column-kind"),
            )))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(t_("ops-credentials-column-secret")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(t_("ops-credentials-column-source")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(t_("ops-credentials-column-valid")),
            )
            .child(div().flex_none().w(px(92.)));

        let has_rows = !active.credentials.is_empty();
        let body: AnyElement = if active.credentials.is_empty() {
            hatched_empty(&t, t_("ops-credentials-empty"), None).into_any_element()
        } else {
            div()
                .id("ops-credential-rows")
                .flex_shrink()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.rows_scroll)
                .children(active.credentials.iter().enumerate().map(|(i, c)| {
                    let (source_cell, valid_cell) =
                        cells.next().expect("one pair of cells per credential");
                    let key = identity(c);
                    let shown = !mask || self.revealed.contains(&key);
                    let secret = c.secret.clone();
                    let copy = c.secret.clone();
                    crate::ui::row()
                        .items_center()
                        .gap(size::S3)
                        .h(px(30.))
                        .px(size::S3)
                        .border_b_1()
                        .border_color(t.line)
                        .hover(move |s| s.bg(t.card_2))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .mono()
                                .text_color(t.ink)
                                .child(focus_kit::ellipsis(c.user.clone())),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .ui()
                                .text_color(t.ink_2)
                                .child(focus_kit::ellipsis(c.domain.clone())),
                        )
                        .child(
                            crate::ui::row()
                                .flex_none()
                                .w(px(84.))
                                .child(mono_chip(&t, c.kind.clone())),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .mono()
                                .text_color(if shown { t.ink } else { t.ink_3 })
                                .child(focus_kit::ellipsis(if shown {
                                    secret
                                } else {
                                    MASK.to_string()
                                })),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .h(px(20.))
                                .overflow_hidden()
                                .child(source_cell),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .h(px(20.))
                                .overflow_hidden()
                                .child(valid_cell),
                        )
                        .child(
                            crate::ui::row()
                                .flex_none()
                                .w(px(92.))
                                .child(
                                    IconButton::new(
                                        ("ops-credential-use", i),
                                        Icon::CHECK_CIRCLE,
                                        t_("ops-credentials-use"),
                                    )
                                    .dense(true)
                                    .on_click({
                                        let credential = c.clone();
                                        cx.listener(move |this, _, _, cx| {
                                            this.use_credential(credential.clone(), cx)
                                        })
                                    }),
                                )
                                .child(
                                    IconButton::new(
                                        ("ops-credential-reveal", i),
                                        if shown { EYE_OFF } else { EYE },
                                        t_(if shown {
                                            "ops-credentials-hide"
                                        } else {
                                            "ops-credentials-reveal"
                                        }),
                                    )
                                    .dense(true)
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if !this.revealed.remove(&key) {
                                                this.revealed.insert(key.clone());
                                            }
                                            cx.notify();
                                        },
                                    )),
                                )
                                .child({
                                    let (face, label) = self.copied.face(
                                        &format!("credential-{i}"),
                                        Icon::COPY,
                                        "ops-credentials-copy",
                                    );
                                    IconButton::new(("ops-credential-copy", i), face, label)
                                        .dense(true)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            focus_kit::clipboard::copy_text(cx, copy.clone());
                                            this.copied.mark(
                                                format!("credential-{i}"),
                                                |view: &mut CredentialsTable| &mut view.copied,
                                                cx,
                                            );
                                        }))
                                }),
                        )
                }))
                .into_any_element()
        };

        crate::ui::pageable(div().id("ops-credentials"), &self.focus, cx, Self::jump)
            .flex()
            .flex_col()
            .size_full()
            .gap(size::S4)
            .p(size::S6)
            .bg(t.bg)
            .child(screen_header(
                &t,
                t_("ops-section-credentials"),
                Some(count),
            ))
            .child(quick_add)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(header)
                    .child(body)
                    .children(has_rows.then(crate::ui::table_filler)),
            )
    }
}
