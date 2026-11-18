use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use focus_core::Vault;
use focus_i18n::{Arg, t_args};
use focus_kit::typography::MONO;
use focus_kit::{ActiveTokens, Field, Icon, IconButton, Type, size};
use focus_ops::snippets::{self, Snippet};
use focus_ops::vars::Context as VarContext;
use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, Styled, StyledText, Subscription, Window, div, px,
    uniform_list,
};
use gpui_component::input::{InputEvent, InputState};

use crate::event::{NoticeTone, OpsEvent};
use crate::resolve::{code_block_variables, variable_highlights};
use crate::state::OpsState;
use crate::ui::{hatched_empty, mono_chip, screen_header, t_};

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub snippet: Snippet,
    pub note: PathBuf,
    pub title: String,
}

const MAX_SHOWN: usize = 300;

pub(crate) fn search(entries: &[Entry], query: &str, context: Option<&VarContext>) -> Vec<usize> {
    let words: Vec<&str> = query.split_whitespace().collect();
    let scored = |entry: &Entry| match context.filter(|_| !words.is_empty()) {
        Some(context) => {
            let resolved = code_block_variables(&entry.snippet.command, context).text;
            let both = snippets::Snippet {
                command: format!("{}\n{resolved}", entry.snippet.command),
                ..entry.snippet.clone()
            };
            snippets::score(&both, &words)
        }
        None => snippets::score(&entry.snippet, &words),
    };
    let mut hits: Vec<(usize, u32)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| scored(e).map(|s| (i, s)))
        .collect();
    hits.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    hits.into_iter().take(MAX_SHOWN).map(|(i, _)| i).collect()
}

fn in_folders(path: &std::path::Path, folders: &[PathBuf]) -> bool {
    folders.is_empty() || folders.iter().any(|f| path.starts_with(f))
}

pub struct CommandLibrary {
    vault: Arc<Vault>,
    state: Entity<OpsState>,
    folders: Vec<PathBuf>,
    entries: Vec<Entry>,
    building: bool,
    query: Entity<InputState>,
    generation: u64,
    shown: Vec<usize>,
    copied: crate::ui::CopyConfirm,
    focus: gpui::FocusHandle,
    scroll: gpui::UniformListScrollHandle,
    reveal: bool,
    drawn: std::cell::RefCell<Vec<(String, String)>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<OpsEvent> for CommandLibrary {}

impl CommandLibrary {
    pub fn new(
        vault: Arc<Vault>,
        state: Entity<OpsState>,
        folders: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> CommandLibrary {
        let query = cx.new(|cx| {
            InputState::new(window, cx)
                .validate(crate::ui::single_line)
                .placeholder(t_("ops-commands-search"))
        });
        let observe = cx.observe(&state, |this, _, cx| {
            this.research(cx);
            cx.notify();
        });
        let typed = cx.subscribe_in(
            &query,
            window,
            |this: &mut CommandLibrary, _, _: &InputEvent, _, cx| {
                this.research(cx);
                cx.notify();
            },
        );
        let mut library = CommandLibrary {
            generation: focus_i18n::generation(),
            vault,
            state,
            folders,
            entries: Vec::new(),
            building: false,
            query,
            shown: Vec::new(),
            copied: Default::default(),
            focus: crate::ui::list_focus(cx),
            scroll: gpui::UniformListScrollHandle::new(),
            reveal: false,
            drawn: Default::default(),
            _subscriptions: vec![observe, typed],
        };
        library.rebuild(cx);
        library
    }

    pub fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.update(cx, |input, cx| input.focus(window, cx));
    }

    pub fn script_drawn(&self) -> Vec<(String, String)> {
        self.drawn.borrow().clone()
    }

    pub fn is_building(&self) -> bool {
        self.building
    }

    pub fn set_folders(&mut self, folders: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.folders = folders;
        self.rebuild(cx);
    }

    pub fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.building = true;
        let (vault, folders) = (self.vault.clone(), self.folders.clone());
        cx.spawn(async move |this, cx| {
            let entries = cx
                .background_spawn(async move {
                    let mut out = Vec::new();
                    for meta in vault.notes() {
                        if !in_folders(&meta.path, &folders) {
                            continue;
                        }
                        let Ok(text) = vault.read(meta.id) else {
                            continue;
                        };
                        for snippet in snippets::extract(&text) {
                            out.push(Entry {
                                snippet,
                                note: meta.path.to_path_buf(),
                                title: meta.title.to_string(),
                            });
                        }
                    }
                    out
                })
                .await;
            this.update(cx, |this, cx| {
                this.entries = entries;
                this.building = false;
                this.research(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn research(&mut self, cx: &mut Context<Self>) {
        let query = self.query.read(cx).value().to_string();
        self.shown = search(&self.entries, &query, self.context(cx).as_ref());
    }

    fn context(&self, cx: &App) -> Option<VarContext> {
        let state = self.state.read(cx);
        state.settings.substitute.then(|| {
            state
                .active
                .as_ref()
                .map(|a| a.context.clone())
                .unwrap_or_default()
        })
    }

    fn jump(&mut self, jump: crate::ui::Jump, _: &mut Window, cx: &mut Context<Self>) {
        crate::ui::scroll_jump(&self.scroll.0.borrow().base_handle, jump);
        cx.notify();
    }

    fn send_to_journal(&mut self, text: String, cx: &mut Context<Self>) {
        let target = self
            .state
            .read(cx)
            .active
            .as_ref()
            .and_then(|a| a.focus_host().map(|h| h.ip.clone()));
        self.state.update(cx, |state, cx| {
            state.write(
                cx,
                move |dir| focus_ops::journal::append(dir, target.as_deref(), &text),
                |_, _, _| {},
            );
        });
        cx.emit(OpsEvent::Notify {
            tone: NoticeTone::Ok,
            text: t_("ops-commands-journaled"),
        });
    }
}

impl Render for CommandLibrary {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        if self.generation != focus_i18n::generation() {
            self.generation = focus_i18n::generation();
            self.query.update(cx, |input, cx| {
                input.set_placeholder(t_("ops-commands-search"), window, cx)
            });
        }
        self.drawn.borrow_mut().clear();

        let total = self.shown.len();
        let has_secret = self.context(cx).is_some_and(|c| !c.secret.is_empty());
        let header = crate::ui::row()
            .items_center()
            .gap(size::S4)
            .child(screen_header(
                &t,
                t_("ops-section-commands"),
                Some(t_args(
                    "ops-commands-count",
                    &[("count", Arg::from(self.entries.len()))],
                )),
            ))
            .child(
                div()
                    .flex_1()
                    .max_w(px(480.))
                    .child(Field::new(&self.query).icon(Icon::SEARCH)),
            )
            .children(has_secret.then(|| {
                let shown = self.reveal;
                IconButton::new(
                    "ops-commands-reveal",
                    if shown {
                        crate::ui::EYE_OFF
                    } else {
                        crate::ui::EYE
                    },
                    t_(if shown {
                        "ops-commands-hide-secrets"
                    } else {
                        "ops-commands-show-secrets"
                    }),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.reveal = !this.reveal;
                    cx.notify();
                }))
            }));

        let body = if self.building && self.entries.is_empty() {
            hatched_empty(&t, t_("ops-loading"), None).into_any_element()
        } else if total == 0 {
            hatched_empty(
                &t,
                t_(if self.entries.is_empty() {
                    "ops-commands-empty"
                } else {
                    "ops-commands-no-match"
                }),
                None,
            )
            .into_any_element()
        } else {
            let entity = cx.entity();
            let reveal = self.reveal;
            uniform_list(
                "ops-command-list",
                total,
                move |range: Range<usize>, _window: &mut Window, cx: &mut App| {
                    let this = entity.read(cx);
                    let context = this.context(cx);
                    let root = this.vault.root().to_path_buf();
                    range
                        .filter_map(|row| {
                            this.shown
                                .get(row)
                                .and_then(|i| this.entries.get(*i))
                                .map(|e| (row, e.clone()))
                        })
                        .map(|(row, entry)| {
                            let resolved = match &context {
                                Some(context) if reveal => {
                                    code_block_variables(&entry.snippet.command, context)
                                }
                                Some(context) => focus_ops::vars::substitute_masked(
                                    &entry.snippet.command,
                                    context,
                                ),
                                None => focus_ops::vars::Substituted {
                                    text: entry.snippet.command.clone(),
                                    ..Default::default()
                                },
                            };
                            let copy_text = match &context {
                                Some(context) => {
                                    code_block_variables(&entry.snippet.command, context).text
                                }
                                None => entry.snippet.command.clone(),
                            };
                            let highlights = variable_highlights(&t, &resolved);
                            let first_line: SharedString = resolved
                                .text
                                .lines()
                                .next()
                                .unwrap_or_default()
                                .to_string()
                                .into();
                            let first_len = first_line.len();
                            this.drawn
                                .borrow_mut()
                                .push((first_line.to_string(), copy_text.clone()));
                            let highlights: Vec<_> = highlights
                                .into_iter()
                                .filter(|(r, _)| r.end <= first_len)
                                .collect();
                            let (resolved_text, raw) = (copy_text, entry.snippet.command.clone());
                            let resolved_ranges: Vec<Range<usize>> = resolved
                                .resolved
                                .iter()
                                .map(|(range, _)| range.clone())
                                .collect();
                            let written = resolved.written.clone();
                            let open_path = root.join(&entry.note);
                            let line = entry.snippet.line;
                            let (e1, e2, e3, e4) = (
                                entity.clone(),
                                entity.clone(),
                                entity.clone(),
                                entity.clone(),
                            );
                            let heading = entry.snippet.heading.clone();
                            div()
                                .id(("ops-command-row", row))
                                .flex()
                                .items_center()
                                .gap(size::S3)
                                .w_full()
                                .h(px(54.))
                                .px(size::S3)
                                .border_b_1()
                                .border_color(t.line)
                                .hover(move |s| s.bg(t.card_2))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .min_w_0()
                                        .gap(px(2.))
                                        .child(
                                            div()
                                                .mono()
                                                .text_color(t.ink)
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .child(
                                                    gpui::InteractiveText::new(
                                                        ("ops-command-text", row),
                                                        StyledText::new(first_line)
                                                            .with_default_highlights(
                                                                &gpui::TextStyle {
                                                                    font_family: MONO.into(),
                                                                    color: t.ink,
                                                                    ..Default::default()
                                                                },
                                                                highlights,
                                                            ),
                                                    )
                                                    .tooltip(move |index, _, cx| {
                                                        let at = resolved_ranges.iter().position(
                                                            |range| range.contains(&index),
                                                        )?;
                                                        Some(focus_kit::Tooltip::view(
                                                            written[at].clone().into(),
                                                            None,
                                                            cx,
                                                        ))
                                                    }),
                                                ),
                                        )
                                        .child(
                                            crate::ui::row()
                                                .items_center()
                                                .gap(size::S2)
                                                .ui_xs()
                                                .text_color(t.ink_3)
                                                .child(mono_chip(&t, entry.snippet.tool.clone()))
                                                .child(div().min_w_0().child(focus_kit::ellipsis(
                                                    format!(
                                                            "{}{}",
                                                            entry.title,
                                                            heading
                                                                .map(|h| format!(" > {h}"))
                                                                .unwrap_or_default()
                                                        ),
                                                ))),
                                        ),
                                )
                                .child(
                                    crate::ui::row()
                                        .flex_none()
                                        .child({
                                            let which = format!("resolved-{row}");
                                            let (face, label) = this.copied.face(
                                                &which,
                                                Icon::COPY,
                                                "ops-copy-resolved",
                                            );
                                            IconButton::new(("ops-command-copy", row), face, label)
                                                .dense(true)
                                                .on_click(move |_, _, cx| {
                                                    focus_kit::clipboard::copy_text(
                                                        cx,
                                                        resolved_text.clone(),
                                                    );
                                                    e1.update(cx, |this, cx| {
                                                        this.copied.mark(
                                                            which.clone(),
                                                            |view: &mut CommandLibrary| {
                                                                &mut view.copied
                                                            },
                                                            cx,
                                                        )
                                                    });
                                                })
                                        })
                                        .child({
                                            let which = format!("raw-{row}");
                                            let (face, label) = this.copied.face(
                                                &which,
                                                Icon::CODE,
                                                "ops-copy-raw",
                                            );
                                            IconButton::new(("ops-command-raw", row), face, label)
                                                .dense(true)
                                                .on_click(move |_, _, cx| {
                                                    focus_kit::clipboard::copy_text(
                                                        cx,
                                                        raw.clone(),
                                                    );
                                                    e2.update(cx, |this, cx| {
                                                        this.copied.mark(
                                                            which.clone(),
                                                            |view: &mut CommandLibrary| {
                                                                &mut view.copied
                                                            },
                                                            cx,
                                                        )
                                                    });
                                                })
                                        })
                                        .child(
                                            IconButton::new(
                                                ("ops-command-journal", row),
                                                Icon::SCROLL_TEXT,
                                                t_("ops-commands-to-journal"),
                                            )
                                            .dense(true)
                                            .on_click(
                                                {
                                                    let text = entry.snippet.command.clone();
                                                    move |_, _, cx| {
                                                        e3.update(cx, |this, cx| {
                                                            this.send_to_journal(text.clone(), cx)
                                                        })
                                                    }
                                                },
                                            ),
                                        )
                                        .child(
                                            IconButton::new(
                                                ("ops-command-open", row),
                                                Icon::FILE_TEXT,
                                                t_("ops-open-source-note"),
                                            )
                                            .dense(true)
                                            .on_click(
                                                move |_, _, cx| {
                                                    e4.update(cx, |_, cx| {
                                                        cx.emit(OpsEvent::OpenNote {
                                                            path: open_path.clone(),
                                                            line: Some(line),
                                                        })
                                                    })
                                                },
                                            ),
                                        ),
                                )
                        })
                        .collect()
                },
            )
            .track_scroll(self.scroll.clone())
            .flex_1()
            .into_any_element()
        };

        crate::ui::pageable(div().id("ops-commands"), &self.focus, cx, Self::jump)
            .flex()
            .flex_col()
            .size_full()
            .gap(size::S4)
            .p(size::S6)
            .bg(t.bg)
            .child(header)
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tool: &str, command: &str, heading: &str) -> Entry {
        Entry {
            snippet: Snippet {
                command: command.into(),
                lang: "bash".into(),
                tool: tool.into(),
                heading: Some(heading.into()),
                line: 1,
                comment: None,
                placeholders: vec![],
            },
            note: PathBuf::from("a.md"),
            title: "A".into(),
        }
    }

    #[test]
    fn search_ranks_the_tool_first_and_drops_non_matches() {
        let entries = vec![
            entry("curl", "curl http://$RHOST | grep nmap", "Web"),
            entry("nmap", "nmap -sV $RHOST", "Scan"),
            entry("ls", "ls -la", "Files"),
        ];
        assert_eq!(search(&entries, "nmap", None), [1, 0]);
        assert_eq!(search(&entries, "zzz", None), Vec::<usize>::new());
        assert_eq!(search(&entries, "", None).len(), 3);
    }

    #[test]
    fn a_word_matches_the_command_as_the_variables_resolve_it() {
        let entries = vec![
            entry("nxc", "nxc smb \"$DC\" -d \"$domain\" -u admin", "Auth"),
            entry("ls", "ls -la", "Files"),
        ];
        let context = VarContext {
            values: [
                ("DOMAIN".to_string(), "test.fr".to_string()),
                ("DC".to_string(), "10.0.0.1".to_string()),
            ]
            .into(),
            ..Default::default()
        };
        assert_eq!(search(&entries, "test.fr", Some(&context)), [0]);
        assert_eq!(search(&entries, "test.fr", None), Vec::<usize>::new());
        assert_eq!(search(&entries, "10.0.0.1 admin", Some(&context)), [0]);
        assert_eq!(
            search(&entries, "$domain", Some(&context)),
            [0],
            "the written form still matches"
        );
    }

    #[test]
    fn folders_limit_the_notes_read() {
        let folders = vec![PathBuf::from("Red Team")];
        assert!(in_folders(std::path::Path::new("Red Team/a.md"), &folders));
        assert!(!in_folders(std::path::Path::new("Other/a.md"), &folders));
        assert!(in_folders(std::path::Path::new("Other/a.md"), &[]));
    }
}
