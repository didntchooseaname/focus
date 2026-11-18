use std::ops::Range;
use std::sync::Arc;

use focus_core::NoteId;
use focus_search::{CancelToken, FuzzyScope, SearchOptions};
use gpui::{
    Action, Animation, AnimationExt as _, AppContext, Context, Entity, EventEmitter, FontWeight,
    HighlightStyle, InteractiveElement, IntoElement, ParentElement, Render, ScrollStrategy,
    SharedString, StatefulInteractiveElement, Styled, Subscription, UniformListScrollHandle,
    Window, div, prelude::FluentBuilder, px, uniform_list,
};
use gpui_component::RopeExt as _;
use gpui_component::input::{Input, InputEvent, InputState};

use crate::library::Library;
use focus_kit::Flow as _;
use focus_kit::{ActiveTokens, Icon, Kbd, Type, ellipsis, floating, icon, motion, size};

use crate::actions::{
    COMMANDS, ConfirmNewTab, SelectFirst, SelectLast, SelectNext, SelectPageDown, SelectPageUp,
    SelectPrev, display,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Notes,
    Templates,
    Commands,
    Tags,
    Headings,
    FullText,
    Page,
}

enum Hit {
    Note {
        id: NoteId,
        title: SharedString,
        path: SharedString,
        matched: Vec<Range<usize>>,
    },
    HostTemplate {
        index: usize,
        title: SharedString,
        matched: Vec<Range<usize>>,
    },
    Command {
        index: usize,
        label: SharedString,
        keys: SharedString,
        matched: Vec<Range<usize>>,
    },
    Heading {
        line: u32,
        level: u8,
        text: SharedString,
        matched: Vec<Range<usize>>,
    },
    Tag {
        text: SharedString,
        matched: Vec<Range<usize>>,
    },
    Line {
        id: NoteId,
        line: u32,
        title: SharedString,
        excerpt: SharedString,
        matched: Vec<Range<usize>>,
    },
    File {
        path: std::path::PathBuf,
        title: SharedString,
        folder: SharedString,
        matched: Vec<Range<usize>>,
    },
    Page {
        page: Option<usize>,
        count: usize,
    },
}

const LIMIT: usize = 200;

fn char_ranges(text: &str, positions: &[u32]) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for (i, (byte, ch)) in text.char_indices().enumerate() {
        if positions.contains(&(i as u32)) {
            let end = byte + ch.len_utf8();
            match out.last_mut() {
                Some(last) if last.end == byte => last.end = end,
                _ => out.push(byte..end),
            }
        }
    }
    out
}

pub enum PaletteEvent {
    OpenNote { id: NoteId, new_tab: bool },
    OpenAtLine { id: NoteId, line: u32 },
    SearchTag(String),
    Run(Box<dyn Action>),
    GoToLine(u32),
    OpenFile(std::path::PathBuf),
    GoToPage(usize),
    FromTemplate(NoteId),
    FromHostTemplate(usize),
    Close,
}

pub struct Palette {
    pub input: Entity<InputState>,
    library: Option<Arc<Library>>,
    note: Option<NoteId>,
    recent: Vec<NoteId>,
    ops: bool,
    ai: bool,
    templates: Option<Vec<(NoteId, std::path::PathBuf)>>,
    pages: Option<usize>,
    search_cancel: CancelToken,
    search_task: Option<gpui::Task<()>>,
    hits: Vec<Hit>,
    mode: Mode,
    selected: usize,
    scroll: UniformListScrollHandle,
    animate: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PaletteEvent> for Palette {}

fn find(haystack: &str, needle: &str) -> Option<Vec<Range<usize>>> {
    if needle.is_empty() {
        return Some(Vec::new());
    }
    let lower = haystack.to_lowercase();
    let start = lower.find(needle)?;
    if lower.len() != haystack.len() {
        return Some(Vec::new());
    }
    let matched: Range<usize> = start..start + needle.len();
    Some(std::iter::once(matched).collect())
}

impl Palette {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        library: Option<Arc<Library>>,
        note: Option<NoteId>,
        recent: Vec<NoteId>,
        ops: bool,
        ai: bool,
        initial: &str,
        animate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx)
                .placeholder(focus_i18n::t("ui-palette-placeholder"))
                .default_value(initial.to_string());
            let end = input.text().offset_to_position(initial.len());
            input.set_cursor_position(end, window, cx);
            input
        });
        let subscriptions = vec![cx.subscribe(
            &input,
            |this: &mut Palette, _, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    this.refilter(cx);
                    cx.notify();
                }
                InputEvent::PressEnter { secondary } => this.confirm(*secondary, cx),
                _ => {}
            },
        )];
        let mut palette = Palette {
            input,
            library,
            note,
            recent,
            ops,
            ai,
            templates: None,
            pages: None,
            search_cancel: CancelToken::new(),
            search_task: None,
            hits: Vec::new(),
            mode: Mode::Notes,
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            animate,
            _subscriptions: subscriptions,
        };
        palette.refilter(cx);
        palette
    }

    pub fn choose_template(
        &mut self,
        templates: Vec<(NoteId, std::path::PathBuf)>,
        cx: &mut Context<Self>,
    ) {
        self.templates = Some(templates);
        self.refilter(cx);
        cx.notify();
    }

    pub fn set_pages(&mut self, pages: usize, cx: &mut Context<Self>) {
        self.pages = Some(pages);
        self.refilter(cx);
        cx.notify();
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let raw = self.input.read(cx).value().to_string();
        let (mode, query) = match raw.chars().next() {
            _ if self.templates.is_some() => (Mode::Templates, raw.as_str()),
            Some('>') => (Mode::Commands, &raw[1..]),
            Some('#') => (Mode::Tags, &raw[1..]),
            Some('@') => (Mode::Headings, &raw[1..]),
            Some('/') => (Mode::FullText, &raw[1..]),
            Some(':') if self.pages.is_some() => (Mode::Page, &raw[1..]),
            _ => (Mode::Notes, raw.as_str()),
        };
        let query = query.trim().to_string();
        self.mode = mode;
        self.search_cancel.cancel();
        let library = self.library.clone();
        let fuzzy = |scope: FuzzyScope| {
            library
                .as_ref()
                .map(|l| l.fuzzy(&query, scope, &self.recent, LIMIT))
                .unwrap_or_default()
        };
        self.hits = match mode {
            Mode::Templates => {
                let needle = query.to_lowercase();
                let vault = self.templates.iter().flatten().filter_map(|(id, path)| {
                    let title = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let matched = find(&title, &needle)?;
                    let folder = path
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    Some(Hit::Note {
                        id: *id,
                        title: title.into(),
                        path: folder.into(),
                        matched,
                    })
                });
                let host = crate::templates::host_templates()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, (name, _))| {
                        let matched = find(name, &needle)?;
                        Some(Hit::HostTemplate {
                            index,
                            title: (*name).into(),
                            matched,
                        })
                    });
                vault.chain(host).collect()
            }
            Mode::Page => {
                let count = self.pages.unwrap_or(0);
                let page = query
                    .parse::<usize>()
                    .ok()
                    .filter(|p| (1..=count).contains(p));
                vec![Hit::Page { page, count }]
            }
            Mode::Notes => fuzzy(FuzzyScope::Notes)
                .into_iter()
                .filter_map(|hit| {
                    let meta = library.as_ref()?.note(hit.note?)?;
                    Some(Hit::Note {
                        id: meta.id,
                        matched: char_ranges(&hit.text, &hit.positions),
                        title: hit.text.into(),
                        path: meta
                            .path
                            .parent()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                            .into(),
                    })
                })
                .chain(
                    library
                        .as_ref()
                        .filter(|_| !query.is_empty())
                        .map(|l| crate::media::palette_files(l, &query, LIMIT))
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(path, title, folder, matched)| Hit::File {
                            path,
                            title: title.into(),
                            folder: folder.into(),
                            matched,
                        }),
                )
                .collect(),
            Mode::Tags => fuzzy(FuzzyScope::Tags)
                .into_iter()
                .map(|hit| Hit::Tag {
                    matched: char_ranges(&hit.text, &hit.positions),
                    text: hit.text.into(),
                })
                .collect(),
            Mode::Headings => match self.note {
                Some(note) => {
                    let headings = library
                        .as_ref()
                        .and_then(|l| l.note(note))
                        .map(|m| m.headings.clone())
                        .unwrap_or_default();
                    fuzzy(FuzzyScope::Headings(note))
                        .into_iter()
                        .map(|hit| {
                            let line = hit.line.unwrap_or(0);
                            let level = headings
                                .iter()
                                .find(|h| h.line == line)
                                .map_or(0, |h| h.level);
                            Hit::Heading {
                                line,
                                level,
                                matched: char_ranges(&hit.text, &hit.positions),
                                text: hit.text.into(),
                            }
                        })
                        .collect()
                }
                None => Vec::new(),
            },
            Mode::Commands => {
                let needle = query.to_lowercase();
                let mut seen = Vec::new();
                COMMANDS
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.in_palette && (self.ops || !c.ops) && (self.ai || !c.ai))
                    .filter(|(_, c)| {
                        let fresh = !seen.contains(&c.label);
                        seen.push(c.label);
                        fresh
                    })
                    .filter_map(|(index, c)| {
                        let label = focus_i18n::t(c.label);
                        let matched = find(&label, &needle)?;
                        Some(Hit::Command {
                            index,
                            label: label.into(),
                            keys: crate::actions::keys_of(c.label, false)
                                .first()
                                .map(|k| display(k))
                                .unwrap_or_default()
                                .into(),
                            matched,
                        })
                    })
                    .collect()
            }
            Mode::FullText => {
                self.search(query, cx);
                Vec::new()
            }
        };
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }

    fn search(&mut self, query: String, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        if query.is_empty() {
            return;
        }
        let cancel = CancelToken::new();
        self.search_cancel = cancel.clone();
        let task = cx.background_spawn(async move {
            let parsed = focus_search::parse_query(&query);
            let options = SearchOptions {
                max_files: LIMIT,
                max_matches_per_file: 1,
                ..SearchOptions::default()
            };
            let results = library.index().search(&parsed, &options, &cancel);
            let hits: Vec<Hit> = results
                .hits
                .into_iter()
                .filter_map(|file| {
                    let meta = library.note(file.note)?;
                    let line = file.matches.first();
                    Some(Hit::Line {
                        id: meta.id,
                        line: line.map_or(0, |l| l.line),
                        title: meta.title.to_string().into(),
                        excerpt: line
                            .map(|l| l.text.trim().to_string())
                            .unwrap_or_default()
                            .into(),
                        matched: line.map(|l| l.ranges.clone()).unwrap_or_default(),
                    })
                })
                .collect();
            (hits, results.cancelled)
        });
        self.search_task = Some(cx.spawn(async move |this, cx| {
            let (hits, cancelled) = task.await;
            if cancelled {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                if this.mode == Mode::FullText {
                    this.hits = hits;
                    this.selected = 0;
                    cx.notify();
                }
            });
        }));
    }

    fn select(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.hits.is_empty() {
            return;
        }
        let count = self.hits.len() as isize;
        self.selected = ((self.selected as isize + delta).rem_euclid(count)) as usize;
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Top);
        cx.notify();
    }

    fn select_at(&mut self, index: isize, cx: &mut Context<Self>) {
        if self.hits.is_empty() {
            return;
        }
        self.selected = index.clamp(0, self.hits.len() as isize - 1) as usize;
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Top);
        cx.notify();
    }

    fn confirm(&mut self, new_tab: bool, cx: &mut Context<Self>) {
        let Some(hit) = self.hits.get(self.selected) else {
            return;
        };
        let event = match hit {
            Hit::Note { id, .. } if self.mode == Mode::Templates => PaletteEvent::FromTemplate(*id),
            Hit::HostTemplate { index, .. } => PaletteEvent::FromHostTemplate(*index),
            Hit::Note { id, .. } => PaletteEvent::OpenNote { id: *id, new_tab },
            Hit::Line { id, line, .. } => PaletteEvent::OpenAtLine {
                id: *id,
                line: *line,
            },
            Hit::Tag { text, .. } => PaletteEvent::SearchTag(text.to_string()),
            Hit::Command { index, .. } => PaletteEvent::Run((COMMANDS[*index].action)()),
            Hit::Heading { line, .. } => PaletteEvent::GoToLine(*line),
            Hit::File { path, .. } => PaletteEvent::OpenFile(path.clone()),
            Hit::Page {
                page: Some(page), ..
            } => PaletteEvent::GoToPage(page - 1),
            Hit::Page { page: None, .. } => return,
        };
        cx.emit(event);
    }

    fn empty_text(&self) -> String {
        match self.mode {
            Mode::Notes => focus_i18n::t("ui-palette-no-match"),
            Mode::Templates => focus_i18n::t("ui-palette-no-template"),
            Mode::Commands => focus_i18n::t("ui-palette-no-command"),
            Mode::Headings => focus_i18n::t("ui-palette-no-heading"),
            Mode::Tags => focus_i18n::t("ui-left-tags-unavailable"),
            Mode::FullText => focus_i18n::t("ui-left-search-unavailable"),
            Mode::Page => String::new(),
        }
    }
}

fn highlighted(
    text: SharedString,
    matched: &[Range<usize>],
    strong: gpui::Hsla,
) -> focus_kit::ellipsis::Ellipsis {
    ellipsis(text).highlights(matched.iter().map(|r| {
        (
            r.clone(),
            HighlightStyle {
                color: Some(strong),
                font_weight: Some(FontWeight::SEMIBOLD),
                ..Default::default()
            },
        )
    }))
}

fn occurrences(
    text: SharedString,
    matched: &[Range<usize>],
    t: &focus_kit::Tokens,
    selected: bool,
) -> focus_kit::ellipsis::Ellipsis {
    let style = if selected {
        HighlightStyle {
            color: Some(t.btn_ink),
            background_color: Some(t.btn),
            ..Default::default()
        }
    } else {
        HighlightStyle {
            color: Some(t.ink),
            font_weight: Some(FontWeight::SEMIBOLD),
            underline: Some(gpui::UnderlineStyle {
                thickness: px(1.),
                color: Some(t.ink_3),
                wavy: false,
            }),
            ..Default::default()
        }
    };
    ellipsis(text).highlights(matched.iter().map(|r| (r.clone(), style)))
}

impl Render for Palette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let has_hits = !self.hits.is_empty();
        let top = window.viewport_size().height * 0.16;
        let visible = self.hits.len().min(size::PALETTE_VISIBLE);
        let list_height = size::PALETTE_ROW * visible as f32;
        let selected = self.selected;

        let body = floating(&t)
            .id("palette")
            .key_context(crate::actions::LIST_CONTEXT)
            .occlude()
            .flex()
            .flex_col()
            .w(size::PALETTE_W)
            .mt(top)
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.select(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select(1, cx)))
            .on_action(cx.listener(|this, _: &SelectPageUp, _, cx| {
                this.select_at(this.selected as isize - size::PALETTE_VISIBLE as isize, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectPageDown, _, cx| {
                this.select_at(this.selected as isize + size::PALETTE_VISIBLE as isize, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectFirst, _, cx| this.select_at(0, cx)))
            .on_action(cx.listener(|this, _: &SelectLast, _, cx| this.select_at(isize::MAX, cx)))
            .on_action(cx.listener(|this, _: &ConfirmNewTab, _, cx| this.confirm(true, cx)))
            .on_action(|_: &gpui_component::input::Enter, _, _| {})
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .gap(size::S3)
                    .h(size::PALETTE_INPUT)
                    .px(size::S4)
                    .border_b_1()
                    .border_color(t.line)
                    .child(icon(Icon::SEARCH, size::ICON).text_color(t.ink_3))
                    .child(
                        div().flex_1().ui_lg().text_color(t.ink).child(
                            Input::new(&self.input)
                                .appearance(false)
                                .text_size(px(14.))
                                .line_height(px(22.))
                                .px_0()
                                .py_0()
                                .h(px(22.)),
                        ),
                    ),
            )
            .map(|this| {
                if self.hits.is_empty() {
                    this.child(
                        div()
                            .flow_row()
                            .items_center()
                            .h(size::PALETTE_ROW * 2.)
                            .px(size::S4)
                            .ui()
                            .text_color(t.ink_3)
                            .child(self.empty_text()),
                    )
                } else {
                    this.child(
                        div()
                            .relative()
                            .h(list_height)
                            .child(
                                uniform_list(
                                    "palette-hits",
                                    self.hits.len(),
                                    cx.processor(move |this, range: Range<usize>, _, cx| {
                                        range
                                            .map(|i| {
                                                let current = i == selected;
                                                let row = div()
                                                    .id(i)
                                                    .relative()
                                                    .flow_row()
                                                    .w_full()
                                                    .items_center()
                                                    .gap(size::S3)
                                                    .h(size::PALETTE_ROW)
                                                    .px(size::S4)
                                                    .ui()
                                                    .text_color(t.ink_2)
                                                    .cursor_pointer()
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
                                                        row.hover(move |s| s.bg(t.card_2))
                                                    })
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            this.selected = i;
                                                            this.confirm(false, cx);
                                                        },
                                                    ));
                                                match &this.hits[i] {
                                                    Hit::Note {
                                                        title,
                                                        path,
                                                        matched,
                                                        ..
                                                    } => row
                                                        .child(
                                                            icon(Icon::FILE_TEXT, size::ICON)
                                                                .text_color(t.ink_3),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_shrink()
                                                                .min_w_0()
                                                                .max_w(px(360.))
                                                                .child(highlighted(
                                                                    title.clone(),
                                                                    matched,
                                                                    t.ink,
                                                                )),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .flow_row()
                                                                .justify_end()
                                                                .mono_sm()
                                                                .text_color(t.ink_3)
                                                                .child(ellipsis(path.clone())),
                                                        ),
                                                    Hit::HostTemplate {
                                                        title, matched, ..
                                                    } => row
                                                        .child(
                                                            icon(Icon::FILE_TEXT, size::ICON)
                                                                .text_color(t.ink_3),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_shrink()
                                                                .min_w_0()
                                                                .max_w(px(360.))
                                                                .child(highlighted(
                                                                    title.clone(),
                                                                    matched,
                                                                    t.ink,
                                                                )),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .flow_row()
                                                                .justify_end()
                                                                .ui_sm()
                                                                .text_color(t.ink_3)
                                                                .child(focus_i18n::t(
                                                                    "ui-template-provided",
                                                                )),
                                                        ),
                                                    Hit::Command {
                                                        label,
                                                        keys,
                                                        matched,
                                                        ..
                                                    } => row
                                                        .child(
                                                            icon(Icon::COMMAND, size::ICON)
                                                                .text_color(t.ink_3),
                                                        )
                                                        .child(div().flex_1().min_w_0().child(
                                                            highlighted(
                                                                label.clone(),
                                                                matched,
                                                                t.ink,
                                                            ),
                                                        ))
                                                        .when(!keys.is_empty(), |row| {
                                                            row.child(Kbd::new(keys.clone()))
                                                        }),
                                                    Hit::Heading {
                                                        level,
                                                        text,
                                                        matched,
                                                        ..
                                                    } => row
                                                        .child(
                                                            icon(Icon::HASH, size::ICON)
                                                                .text_color(t.ink_3),
                                                        )
                                                        .child(div().flex_1().min_w_0().child(
                                                            highlighted(
                                                                text.clone(),
                                                                matched,
                                                                t.ink,
                                                            ),
                                                        ))
                                                        .child(
                                                            div()
                                                                .mono_sm()
                                                                .text_color(t.ink_3)
                                                                .child(format!("H{level}")),
                                                        ),
                                                    Hit::Tag { text, matched } => row
                                                        .child(
                                                            icon(Icon::TAG, size::ICON)
                                                                .text_color(t.ink_3),
                                                        )
                                                        .child(
                                                            div().flex_1().min_w_0().mono().child(
                                                                highlighted(
                                                                    text.clone(),
                                                                    matched,
                                                                    t.ink,
                                                                ),
                                                            ),
                                                        ),
                                                    Hit::File {
                                                        title,
                                                        folder,
                                                        matched,
                                                        ..
                                                    } => row
                                                        .child(
                                                            icon(Icon::IMAGE, size::ICON)
                                                                .text_color(t.ink_3),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_shrink()
                                                                .min_w_0()
                                                                .max_w(px(360.))
                                                                .child(highlighted(
                                                                    title.clone(),
                                                                    matched,
                                                                    t.ink,
                                                                )),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .flow_row()
                                                                .justify_end()
                                                                .mono_sm()
                                                                .text_color(t.ink_3)
                                                                .child(ellipsis(folder.clone())),
                                                        ),
                                                    Hit::Page { page, count } => row
                                                        .child(
                                                            icon(Icon::FILE_TEXT, size::ICON)
                                                                .text_color(t.ink_3),
                                                        )
                                                        .child(div().flex_1().min_w_0().child(
                                                            ellipsis(match page {
                                                                Some(page) => focus_i18n::t_args(
                                                                    "ui-palette-page",
                                                                    &[
                                                                        ("page", (*page).into()),
                                                                        ("count", (*count).into()),
                                                                    ],
                                                                ),
                                                                None => focus_i18n::t_args(
                                                                    "ui-palette-page-hint",
                                                                    &[("count", (*count).into())],
                                                                ),
                                                            }),
                                                        )),
                                                    Hit::Line {
                                                        title,
                                                        excerpt,
                                                        matched,
                                                        ..
                                                    } => row
                                                        .child(
                                                            icon(Icon::FILE_SEARCH, size::ICON)
                                                                .text_color(t.ink_3),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_none()
                                                                .max_w(px(220.))
                                                                .child(ellipsis(title.clone())),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .ui_sm()
                                                                .text_color(t.ink_2)
                                                                .child(occurrences(
                                                                    excerpt.clone(),
                                                                    matched,
                                                                    &t,
                                                                    current,
                                                                )),
                                                        ),
                                                }
                                            })
                                            .collect()
                                    }),
                                )
                                .h(list_height)
                                .track_scroll(self.scroll.clone()),
                            )
                            .child(focus_kit::scrollbar(
                                "palette-scrollbar",
                                self.scroll.clone(),
                            )),
                    )
                }
            })
            .child(
                div()
                    .flow_row()
                    .items_center()
                    .gap(size::S4)
                    .h(size::PALETTE_FOOT)
                    .px(size::S4)
                    .border_t_1()
                    .border_color(t.line)
                    .ui_sm()
                    .text_color(t.ink_3)
                    .when(has_hits, |this| {
                        this.child(
                            div()
                                .flow_row()
                                .items_center()
                                .gap(size::S2)
                                .child(Kbd::new(focus_i18n::t("ui-key-enter")))
                                .child(focus_i18n::t("ui-palette-open")),
                        )
                        .child(
                            div()
                                .flow_row()
                                .items_center()
                                .gap(size::S2)
                                .child(Kbd::new(focus_i18n::t("ui-key-ctrl-enter")))
                                .child(focus_i18n::t("ui-palette-new-tab")),
                        )
                    })
                    .child(
                        div()
                            .flow_row()
                            .items_center()
                            .gap(size::S2)
                            .child(Kbd::new(focus_i18n::t("ui-key-up-down")))
                            .child(focus_i18n::t("ui-palette-browse")),
                    )
                    .child(
                        div()
                            .flow_row()
                            .items_center()
                            .gap(size::S2)
                            .child(Kbd::new(focus_i18n::t("ui-key-escape")))
                            .child(focus_i18n::t("ui-palette-close")),
                    ),
            );

        let veil = div()
            .id("palette-veil")
            .absolute()
            .inset_0()
            .flow_row()
            .justify_center()
            .items_start()
            .bg(t.veil)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.emit(PaletteEvent::Close)),
            )
            .child(body);
        if self.animate {
            veil.with_animation(
                "palette-fade",
                Animation::new(motion::FADE),
                |veil, delta| veil.opacity(delta),
            )
            .into_any_element()
        } else {
            veil.into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::find;

    #[test]
    fn substring_ranges() {
        assert_eq!(
            find("Kerberoasting", "roast"),
            Some(std::iter::once(5..10).collect())
        );
        assert_eq!(find("Kerberoasting", "xyz"), None);
        assert_eq!(find("Kerberoasting", ""), Some(vec![]));
    }
}
