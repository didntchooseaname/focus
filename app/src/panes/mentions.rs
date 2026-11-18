use std::sync::Arc;

use focus_core::{HistoryKind, NoteId};
use focus_i18n::{Arg, t_args};
use focus_kit::Flow as _;
use focus_kit::{Dir, Icon, IconButton, Tokens, Tone, TreeRow, Type, icon, size};
use focus_search::{CancelToken, LinkMentionError, LinkPath, Mention, MentionOptions, Mentions};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, EntityInputHandler as _, Global,
    InteractiveElement, IntoElement, MouseButton, ParentElement, StatefulInteractiveElement,
    Styled, Task, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::RopeExt as _;

use crate::editor::{Load, NoteEditor};
use crate::library::Library;
use crate::shell::Shell;

#[derive(Default)]
struct UnlinkedMentions {
    open: bool,
    asked: Option<(NoteId, u64)>,
    found: Option<(NoteId, Mentions)>,
    cancel: CancelToken,
    task: Option<Task<()>>,
}

struct Slot(Entity<UnlinkedMentions>);

impl Global for Slot {}

fn state(cx: &mut App) -> Entity<UnlinkedMentions> {
    if let Some(slot) = cx.try_global::<Slot>() {
        return slot.0.clone();
    }
    let state = cx.new(|_| UnlinkedMentions::default());
    cx.set_global(Slot(state.clone()));
    state
}

pub(crate) fn open_unlinked_mentions(cx: &mut App) {
    state(cx).update(cx, |s, _| s.open = true);
}

impl Shell {
    pub(super) fn unlinked_mention_rows(
        &mut self,
        library: &Arc<Library>,
        note: NoteId,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let state = state(cx);
        let generation = library.index().generation();
        let (open, asked) = {
            let s = state.read(cx);
            (s.open, s.asked)
        };
        if open && asked != Some((note, generation)) {
            self.search_mentions(&state, library, note, generation, cx);
        }
        let s = state.read(cx);
        let found = s
            .found
            .as_ref()
            .filter(|(of, _)| *of == note)
            .map(|(_, m)| m);
        let toggle = state.clone();
        let header = div()
            .id("unlinked-mentions")
            .flow_row()
            .items_center()
            .gap(size::S2)
            .px(size::S4)
            .pt(size::S4)
            .pb(size::S1)
            .ui_xs()
            .text_color(t.ink_3)
            .cursor_pointer()
            .hover(|s| s.text_color(t.ink_2))
            .child(icon(
                if open {
                    Icon::CHEVRON_DOWN
                } else {
                    Dir::current().forward()
                },
                size::ICON_DENSE,
            ))
            .child(focus_i18n::t("ui-links-unlinked"))
            .when_some(found.filter(|_| open), |this, found| {
                this.child(
                    div()
                        .mono_sm()
                        .child(focus_i18n::format_number(found.total_mentions as i64)),
                )
            })
            .on_click(cx.listener(move |_, _, _, cx| {
                toggle.update(cx, |s, _| s.open = !s.open);
                cx.notify();
            }));
        let mut rows = vec![header.into_any_element()];
        if !open {
            return rows;
        }
        let line = |text: String| {
            div()
                .px(size::S4)
                .py(size::S1)
                .ui_sm()
                .text_color(t.ink_3)
                .child(text)
                .into_any_element()
        };
        let Some(found) = found else {
            rows.push(line(focus_i18n::t("search-searching")));
            return rows;
        };
        if found.hits.is_empty() && !found.partial {
            rows.push(line(focus_i18n::t("ui-links-unlinked-none")));
        }
        let mut n: usize = 0;
        for hit in &found.hits {
            let source = hit.note;
            let Some(meta) = library.note(source) else {
                continue;
            };
            for mention in &hit.mentions {
                let at = mention.line;
                let link = mention.clone();
                let button = div()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        IconButton::new(
                            ("link-mention", n),
                            Icon::LINK,
                            focus_i18n::t("ui-links-link"),
                        )
                        .dense(true)
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.link_mention(note, source, link.clone(), window, cx);
                            },
                        )),
                    );
                rows.push(
                    TreeRow::new(("mention", n), meta.title.to_string())
                        .icon(Icon::FILE_TEXT)
                        .trailing(button)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_note(source, false, window, cx);
                            this.pending_line = Some((source, at));
                        }))
                        .into_any_element(),
                );
                rows.push(
                    div()
                        .pad_start(px(42.))
                        .pad_end(size::S3)
                        .pb(size::S1)
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(focus_kit::ellipsis(mention.text.clone()))
                        .into_any_element(),
                );
                n += 1;
            }
        }
        if found.total_notes > found.hits.len() {
            rows.push(line(t_args(
                "ui-links-unlinked-more",
                &[(
                    "count",
                    Arg::Int((found.total_notes - found.hits.len()) as i64),
                )],
            )));
        }
        if found.partial {
            rows.push(line(focus_i18n::t("ui-links-unlinked-partial")));
        }
        rows
    }

    fn search_mentions(
        &mut self,
        state: &Entity<UnlinkedMentions>,
        library: &Arc<Library>,
        note: NoteId,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let cancel = CancelToken::new();
        state.update(cx, |s, _| {
            s.cancel.cancel();
            s.cancel = cancel.clone();
            s.asked = Some((note, generation));
        });
        let index = library.index().clone();
        let work = cx.background_spawn(async move {
            index.unlinked_mentions(note, &MentionOptions::default(), &cancel)
        });
        let slot = state.clone();
        let task = cx.spawn(async move |this, cx| {
            let found = work.await;
            if found.cancelled {
                return;
            }
            let _ = this.update(cx, |_, cx| {
                slot.update(cx, |s, _| {
                    s.found = Some((note, found));
                    s.task = None;
                });
                cx.notify();
            });
        });
        state.update(cx, |s, _| s.task = Some(task));
    }

    fn link_mention(
        &mut self,
        of: NoteId,
        source: NoteId,
        mention: Mention,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let (Some(from), Some(to)) = (library.note(source), library.note(of)) else {
            return;
        };
        let Some(words) = mention.text.get(mention.range.clone()).map(str::to_string) else {
            return;
        };
        let (style, form) = {
            let prefs = self.prefs.read(cx);
            (
                prefs.choice("editor.link_style"),
                prefs.choice("editor.link_path"),
            )
        };
        let form = match form {
            1 => LinkPath::Root,
            2 => LinkPath::Shortest,
            _ => LinkPath::Relative,
        };
        let unique = library
            .vault()
            .summaries()
            .iter()
            .filter(|s| s.path.file_stem() == to.path.file_stem())
            .count()
            == 1;
        let link =
            focus_search::mention_link(&words, &from.path, &to.path, style == 0, form, unique);
        let tab = self.all_tabs().into_iter().find(|tab| {
            let editor = tab.read(cx);
            editor.id == source
                && editor.loose.is_none()
                && !editor.parked
                && matches!(editor.load, Load::Ready)
        });
        if let Some(editor) = tab {
            self.link_in_tab(
                &library,
                editor,
                from.path.to_path_buf(),
                mention,
                words,
                link,
                window,
                cx,
            );
            return;
        }
        let vault = library.vault().clone();
        let span = mention.span.clone();
        let write = cx.background_spawn(async move {
            focus_search::write_mention_link(&vault, source, span, &words, &link)
        });
        cx.spawn(async move |this, cx| {
            let result = write.await;
            let _ = this.update(cx, |this, cx| {
                this.mention_linked(source, &mention, result, cx)
            });
        })
        .detach();
    }

    #[allow(clippy::too_many_arguments)]
    fn link_in_tab(
        &mut self,
        library: &Arc<Library>,
        editor: Entity<NoteEditor>,
        path: std::path::PathBuf,
        mention: Mention,
        words: String,
        link: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = editor.read(cx).id;
        editor.update(cx, |editor, cx| editor.close_block(false, window, cx));
        let input = editor.read(cx).input.clone();
        let (row, column) = (mention.line as usize, mention.column);
        let place = move |cx: &App| {
            let text = input.read(cx).text();
            if row >= text.lines_len() {
                return None;
            }
            let start = text.line_start_offset(row) + column;
            let end = start + words.len();
            (end <= text.len() && text.slice(start..end) == words.as_str()).then_some(start..end)
        };
        if place(cx).is_none() {
            self.mention_linked(source, &mention, Err(LinkMentionError::Changed), cx);
            return;
        }
        let shown = editor.read(cx).input.read(cx).value().to_string();
        let history = library.vault().history();
        let keep = cx.background_spawn(async move {
            match history {
                Some(history) => history
                    .keep(&path, HistoryKind::Bulk, || Ok(shown.into_bytes()))
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                None => Ok(()),
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let kept = keep.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Err(error) = kept {
                    this.notify_user(
                        Tone::Danger,
                        t_args("ui-links-link-failed", &[("error", Arg::Str(&error))]),
                        cx,
                    );
                    return;
                }
                let Some(range) = place(cx) else {
                    this.mention_linked(source, &mention, Err(LinkMentionError::Changed), cx);
                    return;
                };
                editor.update(cx, |editor, cx| {
                    editor.input.update(cx, |input, cx| {
                        let range = input.text().offset_to_offset_utf16(range.start)
                            ..input.text().offset_to_offset_utf16(range.end);
                        input.replace_text_in_range(Some(range), &link, window, cx);
                    })
                });
                this.mention_linked(source, &mention, Ok(()), cx);
            });
        })
        .detach();
    }

    fn mention_linked(
        &mut self,
        source: NoteId,
        mention: &Mention,
        result: Result<(), LinkMentionError>,
        cx: &mut Context<Self>,
    ) {
        let state = state(cx);
        match result {
            Ok(()) => {
                state.update(cx, |s, _| {
                    if let Some((_, found)) = &mut s.found {
                        for hit in found.hits.iter_mut().filter(|h| h.note == source) {
                            hit.mentions.retain(|m| m.span != mention.span);
                        }
                        found.total_mentions = found.total_mentions.saturating_sub(1);
                    }
                });
                self.notify_user(Tone::Ok, focus_i18n::t("ui-links-linked"), cx);
            }
            Err(LinkMentionError::Changed) => {
                state.update(cx, |s, _| s.asked = None);
                self.notify_user(Tone::Warn, focus_i18n::t("ui-links-link-changed"), cx);
            }
            Err(LinkMentionError::Vault(error)) => self.notify_user(
                Tone::Danger,
                t_args(
                    "ui-links-link-failed",
                    &[("error", Arg::Str(&crate::errors::vault(&error)))],
                ),
                cx,
            ),
        }
        cx.notify();
    }
}
