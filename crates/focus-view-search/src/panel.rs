use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use focus_ai::Cancel;
use focus_ai::assist::VaultHint;
use focus_ai::redact::{self, Replacement};
use focus_core::{NoteId, Vault};
use focus_i18n::{Arg, t as tr, t_args};
use focus_kit::{
    ActiveTokens, Checkbox, Dir, Field, Flow as _, Icon, Kbd, Tokens, Tooltip, Type, ellipsis,
    floating, icon, size,
};
use focus_search::{CancelToken, FileHit, SearchIndex, SearchOptions, SearchResults, SortOrder};
use focus_view_reading::model::LinkGoal;
use focus_view_reading::{ReadingEvent, ReadingView, Typesetting};
use gpui::{
    AnyElement, AnyView, App, AppContext as _, ClickEvent, Context, Div, Entity, EventEmitter,
    FocusHandle, Focusable, FontWeight, HighlightStyle, InteractiveElement, IntoElement, Keystroke,
    ParentElement, Pixels, Render, ScrollStrategy, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Subscription, UnderlineStyle, UniformListScrollHandle,
    Window, div, prelude::FluentBuilder, px, relative, uniform_list,
};
use gpui_component::input::{Input, InputEvent, InputState};

use crate::answer::{self, Citations};
use crate::assist::{
    AssistBackend, Failure, IndexSearcher, Pipeline, Progress, Run, Source, Stage,
};
use crate::excerpt::excerpt;
use crate::rows::{self, Labels, Row};

mod replace_ui;
use replace_ui::ReplaceState;

const CASE_SENSITIVE: Icon = Icon("\u{E3D9}");
const WHOLE_WORD: Icon = Icon("\u{E3DF}");
const REGEX: Icon = Icon("\u{E1FC}");
const STOP: Icon = Icon("\u{E083}");
const REPLACE: Icon = Icon("\u{E3DB}");

const SORTS: [(SortOrder, &str); 4] = [
    (SortOrder::Relevance, "search-sort-relevance"),
    (SortOrder::Path, "search-sort-path"),
    (SortOrder::ModifiedNewest, "search-sort-newest"),
    (SortOrder::ModifiedOldest, "search-sort-oldest"),
];

const OPERATORS: [(&str, &str); 11] = [
    ("tag:#red-team", "search-op-tag"),
    ("path:Certifications", "search-op-path"),
    ("file:index", "search-op-file"),
    ("title:Kerberos", "search-op-title"),
    ("heading:Enumeration", "search-op-heading"),
    ("task:review", "search-op-task"),
    ("prop:status=ready", "search-op-prop"),
    ("golden -ticket", "search-op-exclude"),
    ("\"golden ticket\"", "search-op-phrase"),
    ("kerberoast OR asreproast", "search-op-or"),
    ("/CVE-\\d{4}-\\d+/", "search-op-regex"),
];

const MENU_REOPEN_GUARD: Duration = Duration::from_millis(200);

#[derive(Clone)]
pub struct SearchServices {
    pub vault: Arc<Vault>,
    pub index: Arc<SearchIndex>,
    pub assist: Option<Arc<dyn AssistBackend>>,
    pub redaction: redact::Options,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchSettings {
    pub respect_accents: bool,
    pub result_limit: usize,
}

impl Default for SearchSettings {
    fn default() -> Self {
        SearchSettings {
            respect_accents: false,
            result_limit: SearchOptions::default().max_files,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchEvent {
    OpenNote {
        note: NoteId,
        line: Option<u32>,
        new_tab: bool,
    },
    FocusReleased,
    NotesWritten(Vec<NoteId>),
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Timing {
    pub results_ms: f32,
    pub frame_ms: Option<f32>,
}

#[derive(Default)]
struct Assist {
    visible: bool,
    running: bool,
    stopped: bool,
    stage: Option<Stage>,
    intent: String,
    queries: Vec<String>,
    answer: String,
    sources: Vec<Source>,
    replacements: Vec<Replacement>,
    failure: Option<Failure>,
    cost_usd: Option<f64>,
    cancel: Option<Cancel>,
    editing: Option<usize>,
}

pub struct SearchPanel {
    services: Option<SearchServices>,
    indexing: Option<(usize, usize)>,
    input: Entity<InputState>,
    editor: Entity<InputState>,
    replacement: Entity<InputState>,
    replace: ReplaceState,
    cursor_on: bool,
    answer_view: Entity<ReadingView>,
    focus: FocusHandle,
    options: SearchOptions,
    ai_on: bool,
    help_open: bool,
    sort_open: bool,
    sort_closed_at: Option<Instant>,
    diagnostics: Vec<focus_search::Diagnostic>,
    diagnosed: String,
    local_hits: Vec<FileHit>,
    hits: Vec<FileHit>,
    has_results: bool,
    totals: (usize, usize),
    elapsed_ms: f32,
    truncated: bool,
    searching: bool,
    generation: u64,
    cancel: Option<CancelToken>,
    collapsed: HashSet<NoteId>,
    reasons: HashMap<NoteId, SharedString>,
    cites: HashMap<NoteId, usize>,
    rows: Vec<Row>,
    selected: Option<usize>,
    scroll: UniformListScrollHandle,
    visible: Range<usize>,
    assist: Assist,
    assist_run: u64,
    typed_at: Option<Instant>,
    timing: Timing,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SearchEvent> for SearchPanel {}

impl Focusable for SearchPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn single_line(text: &str, _: &mut Context<InputState>) -> bool {
    !text.contains(['\n', '\r'])
}

fn tool_button(
    id: &'static str,
    glyph: Icon,
    label: String,
    shortcut: &'static str,
    active: bool,
    t: Tokens,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(24.))
        .rounded(size::RADIUS)
        .cursor_pointer()
        .when(active, |this| this.bg(t.btn).text_color(t.btn_ink))
        .when(!active, |this| {
            this.text_color(t.ink_3)
                .hover(move |s| s.bg(t.card_2).text_color(t.ink))
                .active(move |s| s.bg(t.card_3).text_color(t.ink))
        })
        .child(icon(glyph, size::ICON_DENSE))
        .tooltip(move |_, cx| -> AnyView {
            Tooltip::view(label.clone().into(), Some(shortcut.into()), cx)
        })
}

fn inverse_mark(t: &Tokens) -> HighlightStyle {
    HighlightStyle {
        color: Some(t.btn_ink),
        background_color: Some(t.btn),
        ..Default::default()
    }
}

fn soft_mark(t: &Tokens) -> HighlightStyle {
    HighlightStyle {
        color: Some(t.ink),
        font_weight: Some(FontWeight::SEMIBOLD),
        underline: Some(UnderlineStyle {
            thickness: px(1.),
            color: Some(t.ink_3),
            wavy: false,
        }),
        ..Default::default()
    }
}

fn diagnostic_text(diagnostic: &focus_search::Diagnostic, source: &str) -> String {
    use focus_search::DiagnosticKind as Kind;
    let message = diagnostic_message(&diagnostic.kind);
    match source
        .get(diagnostic.range.clone())
        .map(str::trim)
        .filter(|s| {
            !s.is_empty()
                && s.chars().count() <= 24
                && !matches!(
                    diagnostic.kind,
                    Kind::UnknownOperator { .. } | Kind::MissingValue { .. }
                )
        }) {
        Some(snippet) => format!("{message} ({snippet})"),
        None => message,
    }
}

pub fn diagnostic_message(kind: &focus_search::DiagnosticKind) -> String {
    use focus_search::DiagnosticKind as Kind;
    match kind {
        Kind::UnclosedQuote => tr("search-diag-unclosed-quote"),
        Kind::InvalidRegex { message } => t_args(
            "search-diag-invalid-regex",
            &[(
                "detail",
                Arg::Str(message.lines().last().unwrap_or("").trim()),
            )],
        ),
        Kind::UnknownOperator { name } => {
            t_args("search-diag-unknown-operator", &[("name", Arg::Str(name))])
        }
        Kind::MissingValue { operator } => t_args(
            "search-diag-missing-value",
            &[("operator", Arg::Str(operator))],
        ),
        Kind::UnclosedParenthesis => tr("search-diag-unclosed-parenthesis"),
        Kind::UnmatchedParenthesis => tr("search-diag-unmatched-parenthesis"),
    }
}

fn failure_text(failure: &Failure) -> String {
    match failure {
        Failure::MissingKey => tr("search-ai-failed-missing-key"),
        Failure::InvalidKey => tr("search-ai-failed-invalid-key"),
        Failure::Balance => tr("search-ai-failed-balance"),
        Failure::RateLimited { retry_after_secs } => t_args(
            "search-ai-failed-rate-limit",
            &[("seconds", Arg::Int(i64::from(*retry_after_secs)))],
        ),
        Failure::Network => tr("search-ai-failed-network"),
        Failure::Other => tr("search-ai-failed-other"),
    }
}

fn stage_text(stage: Stage) -> String {
    tr(match stage {
        Stage::Expanding => "search-ai-stage-expanding",
        Stage::Searching => "search-ai-stage-searching",
        Stage::Ranking => "search-ai-stage-ranking",
        Stage::Answering => "search-ai-stage-answering",
    })
}

impl SearchPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr("search-placeholder"))
                .validate(single_line)
        });
        let editor = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr("search-ai-query-placeholder"))
                .validate(single_line)
        });
        let replacement = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr("search-replace-placeholder"))
                .validate(single_line)
        });
        let answer_view = cx.new(|cx| ReadingView::compact(Typesetting::compact(480.), cx));
        let weak = cx.weak_entity();
        let subscriptions = vec![
            cx.subscribe(&answer_view, |this, _, event: &ReadingEvent, cx| {
                if let ReadingEvent::OpenLink {
                    goal: LinkGoal::Note { id, .. },
                    new_tab,
                } = event
                {
                    this.open_cited(*id, *new_tab, cx);
                }
            }),
            cx.subscribe_in(
                &replacement,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.replace.report = None;
                        this.run_plan(window, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &input,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => this.on_change(window, cx),
                    InputEvent::PressEnter { secondary } => this.confirm(*secondary, window, cx),
                    _ => {}
                },
            ),
            cx.subscribe_in(
                &editor,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { .. } => this.commit_query(window, cx),
                    InputEvent::Blur if this.assist.editing.take().is_some() => cx.notify(),
                    _ => {}
                },
            ),
            cx.intercept_keystrokes(move |event, window, cx| {
                let handled = weak
                    .update(cx, |this, cx| {
                        this.on_keystroke(&event.keystroke, window, cx)
                    })
                    .unwrap_or(false);
                if handled {
                    cx.stop_propagation();
                }
            }),
        ];
        let panel = SearchPanel {
            services: None,
            indexing: None,
            input,
            editor,
            replacement,
            replace: ReplaceState::default(),
            cursor_on: true,
            answer_view,
            focus: cx.focus_handle(),
            options: SearchOptions::default(),
            ai_on: true,
            help_open: false,
            sort_open: false,
            sort_closed_at: None,
            diagnostics: Vec::new(),
            diagnosed: String::new(),
            local_hits: Vec::new(),
            hits: Vec::new(),
            has_results: false,
            totals: (0, 0),
            elapsed_ms: 0.0,
            truncated: false,
            searching: false,
            generation: 0,
            cancel: None,
            collapsed: HashSet::new(),
            reasons: HashMap::new(),
            cites: HashMap::new(),
            rows: Vec::new(),
            selected: None,
            scroll: UniformListScrollHandle::new(),
            visible: 0..0,
            assist: Assist::default(),
            assist_run: 0,
            typed_at: None,
            timing: Timing::default(),
            _subscriptions: subscriptions,
        };
        panel.watch_language(window, cx);
        panel
    }

    fn watch_language(&self, window: &Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let mut seen = focus_i18n::generation();
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let now = focus_i18n::generation();
                if now == seen {
                    continue;
                }
                seen = now;
                let alive = this.update_in(cx, |this, window, cx| {
                    this.input.update(cx, |input, cx| {
                        input.set_placeholder(tr("search-placeholder"), window, cx)
                    });
                    this.editor.update(cx, |input, cx| {
                        input.set_placeholder(tr("search-ai-query-placeholder"), window, cx)
                    });
                    this.replacement.update(cx, |input, cx| {
                        input.set_placeholder(tr("search-replace-placeholder"), window, cx)
                    });
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    pub fn set_services(
        &mut self,
        services: Option<SearchServices>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.indexing = None;
        self.services = services;
        self.run_search(window, cx);
    }

    pub fn set_indexing_progress(&mut self, done: usize, total: usize, cx: &mut Context<Self>) {
        self.indexing = Some((done, total));
        cx.notify();
    }

    pub fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.run_search(window, cx);
    }

    pub fn focus_field(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    pub fn query(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    pub fn set_query(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.set_value(text.to_string(), window, cx)
        });
    }

    pub fn last_timing(&self) -> Timing {
        self.timing
    }

    pub fn is_busy(&self) -> bool {
        self.searching
            || self.assist.running
            || self.services.is_none()
            || self.replace.building
            || self.replace.applying
    }

    pub fn rows_len(&self) -> usize {
        self.rows.len()
    }

    pub fn set_options(
        &mut self,
        case_sensitive: bool,
        whole_word: bool,
        regex: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.options.case_sensitive = case_sensitive;
        self.options.whole_word = whole_word;
        self.options.regex = regex;
        self.replace.exclusions = Default::default();
        self.replace.report = None;
        self.run_search(window, cx);
    }

    pub fn set_settings(
        &mut self,
        settings: SearchSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let limit = settings.result_limit.max(1);
        if self.options.max_files == limit
            && self.options.accent_sensitive == settings.respect_accents
        {
            return;
        }
        self.options.max_files = limit;
        self.options.accent_sensitive = settings.respect_accents;
        self.run_search(window, cx);
    }

    pub fn set_ai(&mut self, on: bool, cx: &mut Context<Self>) {
        self.ai_on = on;
        cx.notify();
    }

    pub fn set_help_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.help_open = open;
        cx.notify();
    }

    fn assist_backend(&self) -> Option<Arc<dyn AssistBackend>> {
        self.services.as_ref().and_then(|s| s.assist.clone())
    }

    fn on_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.typed_at = Some(Instant::now());
        self.stop_assist();
        self.assist = Assist::default();
        self.reasons.clear();
        self.cites.clear();
        self.replace.exclusions = Default::default();
        self.replace.report = None;
        self.run_search(window, cx);
    }

    fn run_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(previous) = self.cancel.take() {
            previous.cancel();
        }
        self.generation += 1;
        let generation = self.generation;
        let text = self.input.read(cx).value().to_string();
        self.selected = None;
        self.diagnostics.clear();
        if self.replace.open {
            self.run_plan(window, cx);
        }
        if text.trim().is_empty() {
            self.searching = false;
            self.has_results = false;
            self.local_hits.clear();
            self.hits.clear();
            self.rebuild_rows(cx);
            cx.notify();
            return;
        }
        let query = focus_search::parse_query(&text);
        self.diagnostics = query.diagnostics.clone();
        self.diagnosed = text.clone();
        if self.options.regex
            && let Err(error) = regex::Regex::new(&text)
        {
            self.diagnostics.push(focus_search::Diagnostic {
                kind: focus_search::DiagnosticKind::InvalidRegex {
                    message: error.to_string(),
                },
                range: 0..text.len(),
            });
            self.searching = false;
            self.has_results = false;
            self.hits.clear();
            self.local_hits.clear();
            self.rebuild_rows(cx);
            cx.notify();
            return;
        }
        let Some(index) = self.services.as_ref().map(|s| s.index.clone()) else {
            cx.notify();
            return;
        };
        let token = CancelToken::new();
        self.cancel = Some(token.clone());
        self.searching = true;
        let options = self.options.clone();
        let typed_at = self.typed_at;
        cx.spawn_in(window, async move |this, cx| {
            let results = cx
                .background_spawn(async move { index.search(&query, &options, &token) })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.apply_results(generation, results, typed_at, window, cx)
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn apply_results(
        &mut self,
        generation: u64,
        results: SearchResults,
        typed_at: Option<Instant>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if generation != self.generation || results.cancelled {
            return;
        }
        self.searching = false;
        self.has_results = true;
        self.totals = (results.total_files, results.total_matches);
        self.elapsed_ms = results.elapsed_ms;
        self.truncated = results.truncated;
        self.local_hits = results.hits;
        self.hits = self.local_hits.clone();
        self.rebuild_rows(cx);
        if let Some(started) = typed_at {
            self.timing = Timing {
                results_ms: started.elapsed().as_secs_f32() * 1000.0,
                frame_ms: None,
            };
            let weak = cx.weak_entity();
            window.on_next_frame(move |_, cx| {
                weak.update(cx, |this, _| {
                    this.timing.frame_ms = Some(started.elapsed().as_secs_f32() * 1000.0)
                })
                .ok();
            });
        }
        cx.notify();
    }

    fn note_label(&self, id: NoteId) -> Option<(SharedString, SharedString)> {
        let vault = &self.services.as_ref()?.vault;
        let meta = vault.note(id)?;
        let folder = meta
            .path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        Some((meta.title.to_string().into(), folder.into()))
    }

    fn rebuild_rows(&mut self, _: &mut Context<Self>) {
        let lookup = |id: NoteId| self.note_label(id);
        let labels = Labels {
            title_and_path: &lookup,
            reasons: &self.reasons,
            cites: &self.cites,
            collapsed: &self.collapsed,
        };
        let rows = match &self.replace.plan {
            Some(plan) if self.replace.open => {
                rows::flatten_plan(plan, &self.replace.exclusions, &self.collapsed)
            }
            _ if self.replace.open => Vec::new(),
            _ => rows::flatten(&self.hits, &labels),
        };
        self.rows = rows;
        self.selected = self.selected.filter(|&i| i < self.rows.len());
    }

    fn toggle_collapsed(&mut self, note: NoteId, cx: &mut Context<Self>) {
        if !self.collapsed.remove(&note) {
            self.collapsed.insert(note);
        }
        self.selected = None;
        self.rebuild_rows(cx);
        cx.notify();
    }

    fn select(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.select_row(rows::step(&self.rows, self.selected, delta), cx);
    }

    fn select_page(&mut self, direction: isize, cx: &mut Context<Self>) {
        let page = self.visible.len().saturating_sub(1).max(1) as isize;
        self.select_row(rows::jump(&self.rows, self.selected, direction * page), cx);
    }

    fn select_row(&mut self, next: Option<usize>, cx: &mut Context<Self>) {
        let Some(next) = next else { return };
        self.selected = Some(next);
        if next < self.visible.start {
            self.scroll.scroll_to_item(next, ScrollStrategy::Top);
        } else if next + 1 >= self.visible.end {
            self.scroll.scroll_to_item(next, ScrollStrategy::Bottom);
        }
        cx.notify();
    }

    fn open(&mut self, row: usize, new_tab: bool, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(row) else {
            return;
        };
        let (note, line) = row.target();
        cx.emit(SearchEvent::OpenNote {
            note,
            line,
            new_tab,
        });
    }

    fn confirm(&mut self, new_tab: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.assist.editing.is_some() {
            return;
        }
        if let Some(selected) = self.selected {
            self.open(selected, new_tab, cx);
            return;
        }
        let asking = self.ai_on
            && !self.replace.open
            && !new_tab
            && self.assist_backend().is_some()
            && !self.query(cx).trim().is_empty();
        if asking {
            self.start_assist(None, window, cx);
        } else if let Some(first) = self
            .rows
            .iter()
            .position(|r| matches!(r, Row::Header { .. }))
        {
            self.open(first, new_tab, cx);
        }
    }

    fn in_field(&self, window: &Window, cx: &App) -> bool {
        [&self.input, &self.editor, &self.replacement]
            .iter()
            .any(|field| field.read(cx).focus_handle(cx).is_focused(window))
    }

    fn on_keystroke(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.focus.contains_focused(window, cx) {
            return false;
        }
        let m = keystroke.modifiers;
        let plain = !m.control && !m.alt && !m.shift && !m.platform;
        match keystroke.key.as_str() {
            "down" if plain && self.assist.editing.is_none() => self.select(1, cx),
            "up" if plain && self.assist.editing.is_none() => self.select(-1, cx),
            "pagedown" if plain && self.assist.editing.is_none() => self.select_page(1, cx),
            "pageup" if plain && self.assist.editing.is_none() => self.select_page(-1, cx),
            "home" | "end"
                if (m.control && !m.alt && !m.shift || plain && !self.in_field(window, cx))
                    && self.assist.editing.is_none() =>
            {
                self.select_row(rows::end(&self.rows, keystroke.key == "end"), cx)
            }
            "escape" if plain => self.escape(window, cx),
            "c" if m.alt => self.set_options(
                !self.options.case_sensitive,
                self.options.whole_word,
                self.options.regex,
                window,
                cx,
            ),
            "w" if m.alt => self.set_options(
                self.options.case_sensitive,
                !self.options.whole_word,
                self.options.regex,
                window,
                cx,
            ),
            "r" if m.alt => self.set_options(
                self.options.case_sensitive,
                self.options.whole_word,
                !self.options.regex,
                window,
                cx,
            ),
            "h" if m.control && m.shift => self.set_replace_open(!self.replace.open, window, cx),
            "t" if m.alt && self.replace.open => {
                if let Some(row) = self.selected {
                    self.toggle_keep(row, cx);
                }
            }
            "h" if m.alt => self.set_help_open(!self.help_open, cx),
            "a" if m.alt && self.assist_backend().is_some() => self.set_ai(!self.ai_on, cx),
            "s" if m.alt => self.cycle_sort(window, cx),
            _ => return false,
        }
        true
    }

    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.assist.editing.take().is_some() {
            self.focus_field(window, cx);
            cx.notify();
        } else if self.sort_open {
            self.sort_open = false;
            cx.notify();
        } else if !self.query(cx).is_empty() {
            self.set_query("", window, cx);
        } else {
            cx.emit(SearchEvent::FocusReleased);
        }
    }

    fn cycle_sort(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let at = SORTS
            .iter()
            .position(|(order, _)| *order == self.options.sort)
            .unwrap_or(0);
        self.set_sort(SORTS[(at + 1) % SORTS.len()].0, window, cx);
    }

    fn set_sort(&mut self, sort: SortOrder, window: &mut Window, cx: &mut Context<Self>) {
        self.options.sort = sort;
        self.sort_open = false;
        self.run_search(window, cx);
    }

    fn stop_assist(&mut self) {
        self.assist_run += 1;
        if let Some(cancel) = self.assist.cancel.take() {
            cancel.cancel();
        }
        self.assist.running = false;
    }

    fn start_assist(
        &mut self,
        queries: Option<Vec<String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(services) = self.services.clone() else {
            return;
        };
        let Some(backend) = services.assist.clone() else {
            return;
        };
        let question = self.query(cx);
        self.stop_assist();
        let cancel = Cancel::new();
        self.assist = Assist {
            visible: true,
            running: true,
            stage: Some(if queries.is_some() {
                Stage::Searching
            } else {
                Stage::Expanding
            }),
            queries: queries.clone().unwrap_or_default(),
            cancel: Some(cancel.clone()),
            ..Assist::default()
        };
        self.reasons.clear();
        self.cites.clear();
        self.hits = self.local_hits.clone();
        self.rebuild_rows(cx);
        self.refresh_answer(cx);
        let run = self.assist_run;
        let (tx, rx) = smol::channel::unbounded();
        let local = self.local_hits.clone();
        let options = self.options.clone();
        let language = focus_i18n::language();
        cx.background_spawn(async move {
            let vault = services.vault.clone();
            let pipeline = Pipeline {
                backend,
                searcher: Arc::new(IndexSearcher(services.index.clone())),
                lookup: Arc::new({
                    let vault = vault.clone();
                    move |id| {
                        vault
                            .note(id)
                            .map(|m| (m.title.to_string(), m.path.display().to_string()))
                    }
                }),
                options,
                redaction: services.redaction.clone(),
                hint: vault_hint(&vault, &language),
            };
            pipeline.run(
                Run {
                    question,
                    local,
                    queries,
                },
                &cancel,
                &mut |progress| {
                    let _ = tx.send_blocking(progress);
                },
            );
        })
        .detach();
        self.cursor_on = true;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let running = this.update_in(cx, |this, _, cx| {
                    let running = this.assist_run == run && this.assist.running;
                    this.cursor_on = !running || !this.cursor_on;
                    this.refresh_answer(cx);
                    cx.notify();
                    running
                });
                if !running.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(progress) = rx.recv().await {
                let alive = this.update_in(cx, |this, _, cx| {
                    if this.assist_run == run {
                        this.on_progress(progress, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn on_progress(&mut self, progress: Progress, cx: &mut Context<Self>) {
        match progress {
            Progress::Stage(stage) => self.assist.stage = Some(stage),
            Progress::Expanded { intent, queries } => {
                self.assist.intent = intent;
                self.assist.queries = queries;
            }
            Progress::Merged(hits) => {
                self.totals = (
                    hits.len(),
                    hits.iter().map(|h| h.match_count as usize).sum(),
                );
                self.hits = hits;
                self.rebuild_rows(cx);
            }
            Progress::Ranked(ranked) => {
                let position: HashMap<NoteId, usize> = ranked
                    .iter()
                    .enumerate()
                    .map(|(i, (id, _))| (*id, i))
                    .collect();
                self.hits
                    .sort_by_key(|h| position.get(&h.note).copied().unwrap_or(usize::MAX));
                self.reasons = ranked
                    .into_iter()
                    .map(|(id, reason)| (id, reason.into()))
                    .collect();
                self.rebuild_rows(cx);
            }
            Progress::Sources(sources) => {
                self.cites = sources
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (s.note, i + 1))
                    .collect();
                self.assist.sources = sources;
                self.rebuild_rows(cx);
            }
            Progress::Replacements(replacements) => self.assist.replacements = replacements,
            Progress::Chunk(text) => self.assist.answer.push_str(&text),
            Progress::Failed(failure) => {
                self.assist.failure = Some(failure);
                self.assist.running = false;
                self.assist.stage = None;
            }
            Progress::Done { cost_usd } => {
                self.assist.running = false;
                self.assist.stage = None;
                self.assist.cost_usd = cost_usd;
            }
        }
        self.refresh_answer(cx);
        cx.notify();
    }

    fn stop_clicked(&mut self, cx: &mut Context<Self>) {
        self.stop_assist();
        self.assist.stopped = true;
        self.assist.stage = None;
        self.refresh_answer(cx);
        cx.notify();
    }

    pub fn edit_query(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.assist.queries.get(at).cloned().unwrap_or_default();
        self.assist.editing = Some(at);
        self.editor.update(cx, |editor, cx| {
            editor.set_value(text, window, cx);
            editor.focus(window, cx);
        });
        cx.notify();
    }

    fn remove_query(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        if at < self.assist.queries.len() {
            let mut queries = self.assist.queries.clone();
            queries.remove(at);
            self.start_assist(Some(queries), window, cx);
        }
    }

    fn commit_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.assist.editing.take() else {
            return;
        };
        let text = self.editor.read(cx).value().trim().to_string();
        let mut queries = self.assist.queries.clone();
        match (at < queries.len(), text.is_empty()) {
            (true, true) => {
                queries.remove(at);
            }
            (true, false) => queries[at] = text,
            (false, false) => queries.push(text),
            (false, true) => {}
        }
        self.focus_field(window, cx);
        self.start_assist(Some(queries), window, cx);
    }

    fn refresh_answer(&mut self, cx: &mut Context<Self>) {
        let a = &self.assist;
        let Some(services) = &self.services else {
            return;
        };
        let sources: Vec<NoteId> = a.sources.iter().map(|s| s.note).collect();
        let text = answer::markdown(
            &redact::restore(&a.answer, &a.replacements),
            sources.len(),
            a.running.then_some(self.cursor_on),
        );
        let meta = sources.first().and_then(|id| services.vault.note(*id));
        let resolver = Arc::new(Citations {
            vault: services.vault.clone(),
            sources,
        });
        self.answer_view
            .update(cx, |view, cx| view.set_markdown(text, meta, resolver, cx));
    }

    fn open_cited(&mut self, note: NoteId, new_tab: bool, cx: &mut Context<Self>) {
        let line = self
            .assist
            .sources
            .iter()
            .find(|s| s.note == note)
            .and_then(|s| s.line);
        cx.emit(SearchEvent::OpenNote {
            note,
            line,
            new_tab,
        });
    }

    fn total_line(&self, t: Tokens) -> Div {
        let text = if self.replace.open {
            if self.replace.building {
                tr("search-replace-reading")
            } else {
                self.replace_summary().unwrap_or_default()
            }
        } else if self.searching && !self.has_results {
            tr("search-searching")
        } else if self.has_results {
            t_args(
                "search-results-count",
                &[
                    ("count", Arg::Int(self.totals.1 as i64)),
                    ("files", Arg::Int(self.totals.0 as i64)),
                ],
            )
        } else {
            String::new()
        };
        let elapsed = (self.has_results && !self.searching && !self.replace.open).then(|| {
            focus_i18n::format_duration(Duration::from_millis(self.elapsed_ms.round() as u64))
        });
        div()
            .flow_row()
            .items_center()
            .gap(size::S2)
            .h(px(24.))
            .px(size::S3)
            .ui_sm()
            .text_color(t.ink_2)
            .child(div().flex().flex_1().min_w_0().child(ellipsis(text)))
            .when_some(elapsed, |this, elapsed| {
                this.child(
                    div()
                        .flex_none()
                        .mono_sm()
                        .text_color(t.ink_3)
                        .child(elapsed),
                )
            })
    }

    fn toolbar(&mut self, t: Tokens, cx: &mut Context<Self>) -> Div {
        let options = self.options.clone();
        let ai_available = self.assist_backend().is_some();
        let sort_label = tr(SORTS
            .iter()
            .find(|(o, _)| *o == options.sort)
            .map_or("search-sort", |(_, key)| key));
        div()
            .flow_row()
            .relative()
            .items_center()
            .gap(px(2.))
            .h(size::CONTROL)
            .px(size::S3)
            .child(
                tool_button(
                    "opt-case",
                    CASE_SENSITIVE,
                    tr("search-option-case"),
                    "Alt+C",
                    options.case_sensitive,
                    t,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.set_options(
                        !this.options.case_sensitive,
                        this.options.whole_word,
                        this.options.regex,
                        window,
                        cx,
                    )
                })),
            )
            .child(
                tool_button(
                    "opt-word",
                    WHOLE_WORD,
                    tr("search-option-word"),
                    "Alt+W",
                    options.whole_word,
                    t,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.set_options(
                        this.options.case_sensitive,
                        !this.options.whole_word,
                        this.options.regex,
                        window,
                        cx,
                    )
                })),
            )
            .child(
                tool_button(
                    "opt-regex",
                    REGEX,
                    tr("search-option-regex"),
                    "Alt+R",
                    options.regex,
                    t,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.set_options(
                        this.options.case_sensitive,
                        this.options.whole_word,
                        !this.options.regex,
                        window,
                        cx,
                    )
                })),
            )
            .child(
                tool_button(
                    "opt-replace",
                    REPLACE,
                    tr("search-replace-toggle"),
                    "Ctrl+Shift+H",
                    self.replace.open,
                    t,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.set_replace_open(!this.replace.open, window, cx)
                })),
            )
            .child(div().flex_1())
            .child(
                tool_button(
                    "sort",
                    Icon::ARROW_UP_DOWN,
                    format!("{}: {}", tr("search-sort"), sort_label),
                    "Alt+S",
                    self.sort_open,
                    t,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    let just_closed = this
                        .sort_closed_at
                        .take()
                        .is_some_and(|at| at.elapsed() < MENU_REOPEN_GUARD);
                    if !just_closed {
                        this.sort_open = !this.sort_open;
                    }
                    cx.notify();
                })),
            )
            .child(
                tool_button(
                    "help",
                    Icon::HELP,
                    tr("search-help-toggle"),
                    "Alt+H",
                    self.help_open,
                    t,
                )
                .on_click(cx.listener(|this, _, _, cx| this.set_help_open(!this.help_open, cx))),
            )
            .when(ai_available, |this| {
                let on = self.ai_on;
                this.child(
                    div()
                        .flow_row()
                        .id("ai-toggle")
                        .flex_shrink()
                        .min_w(px(24.))
                        .items_center()
                        .gap(px(4.))
                        .h(px(24.))
                        .px(size::S2)
                        .rounded(size::RADIUS)
                        .cursor_pointer()
                        .ui_xs()
                        .when(on, |d| d.bg(t.btn).text_color(t.btn_ink))
                        .when(!on, |d| {
                            d.text_color(t.ink_3)
                                .hover(move |s| s.bg(t.card_2).text_color(t.ink))
                                .active(move |s| s.bg(t.card_3).text_color(t.ink))
                        })
                        .child(icon(Icon::SPARKLES, size::ICON_SMALL))
                        .child(
                            div()
                                .flex()
                                .min_w_0()
                                .child(ellipsis(tr("search-ai-toggle"))),
                        )
                        .tooltip(|_, cx| -> AnyView {
                            Tooltip::view(
                                tr("search-ai-toggle-tip").into(),
                                Some("Alt+A".into()),
                                cx,
                            )
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.set_ai(!this.ai_on, cx))),
                )
            })
            .when(self.sort_open, |this| {
                let selected = options.sort;
                this.child(focus_kit::fade_in(
                    "fade-search-sort",
                    (floating(&t)
                        .absolute()
                        .top(px(28.))
                        .flex()
                        .flex_col()
                        .py(size::S1))
                    .start_at(size::S3)
                    .min_w(px(176.))
                    .occlude()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.sort_open = false;
                        this.sort_closed_at = Some(Instant::now());
                        cx.notify();
                    }))
                    .children(SORTS.iter().map(|(order, key)| {
                        let order = *order;
                        focus_kit::MenuRow::new(*key, tr(key))
                            .checked(order == selected)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_sort(order, window, cx)
                            }))
                    })),
                    cx,
                ))
            })
    }

    fn help(&self, t: Tokens, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .px(size::S3)
            .pb(size::S2)
            .border_b_1()
            .border_color(t.line)
            .children(OPERATORS.iter().enumerate().map(|(i, (example, key))| {
                let example = *example;
                div()
                    .id(("op", i))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .py(size::S1)
                    .cursor_pointer()
                    .hover(move |s| s.bg(t.card_2))
                    .tooltip(|_, cx| -> AnyView {
                        Tooltip::view(tr("search-help-insert").into(), None, cx)
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_query(example, window, cx);
                        this.focus_field(window, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .child(div().mono_sm().text_color(t.ink).child(ellipsis(example))),
                    )
                    .child(
                        div()
                            .flow_row()
                            .ui_sm()
                            .text_color(t.ink_3)
                            .child(ellipsis(tr(key))),
                    )
            }))
    }

    fn row(&self, i: usize, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.selected == Some(i);
        let mark = if selected {
            inverse_mark(&t)
        } else {
            soft_mark(&t)
        };
        let indent = |d: Stateful<Div>| d.pad_start(size::S3 + size::INDENT).pad_end(size::S3);
        let base = div()
            .flow_row()
            .id(("row", i))
            .relative()
            .w_full()
            .items_center()
            .gap(size::S2)
            .h(size::list_row())
            .px(size::S3)
            .cursor_pointer()
            .when(selected, |d| d.bg(t.card_3))
            .when(!selected, |d| {
                d.hover(move |s| s.bg(t.card_2))
                    .active(move |s| s.bg(t.card_3))
            });
        let bar = || (div().absolute().top_0().bottom_0().w(px(2.)).bg(t.ink)).start_at(px(0.));
        let click = |new_tab_default: bool| {
            cx.listener(move |this, event: &ClickEvent, _, cx| {
                this.selected = Some(i);
                let new_tab = new_tab_default || event.modifiers().secondary();
                this.open(i, new_tab, cx);
            })
        };
        let keep_box = |keep: bool| {
            Checkbox::new(("keep", i), keep).on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_keep(i, cx);
            }))
        };
        let number = |line: Option<u32>| {
            div()
                .flex_none()
                .w(px(32.))
                .flex()
                .justify_end()
                .mono_sm()
                .text_color(t.ink_3)
                .children(line.map(|l| (l + 1).to_string()))
        };
        let sign = |sign: &'static str| {
            div()
                .flex_none()
                .w(px(8.))
                .mono_sm()
                .text_color(t.ink_2)
                .child(sign)
        };
        match &self.rows[i] {
            Row::Header {
                note,
                title,
                path,
                title_ranges,
                count,
                collapsed,
                cite,
                keep,
            } => {
                let note = *note;
                let collapsed = *collapsed;
                let chevron = if collapsed {
                    Dir::current().forward()
                } else {
                    Icon::CHEVRON_DOWN
                };
                let label = tr(if collapsed {
                    "search-expand"
                } else {
                    "search-collapse"
                });
                let path_tip = path.clone();
                base.when(selected, |d| d.child(bar()))
                    .on_click(click(false))
                    .when_some(*keep, |d, keep| d.child(keep_box(keep)))
                    .child(
                        div()
                            .id(("fold", i))
                            .flex_none()
                            .text_color(t.ink_3)
                            .hover(move |s| s.text_color(t.ink))
                            .tooltip(move |_, cx| -> AnyView {
                                Tooltip::view(label.clone().into(), None, cx)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_collapsed(note, cx);
                            }))
                            .child(icon(chevron, size::ICON_DENSE)),
                    )
                    .when_some(*cite, |d, n| {
                        d.child(
                            div()
                                .flex_none()
                                .mono_sm()
                                .text_color(t.ink)
                                .child(format!("[{n}]")),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_shrink()
                            .min_w_0()
                            .ui_strong()
                            .text_color(t.ink)
                            .child(excerpt(title.clone(), title_ranges.clone(), mark).keep_start()),
                    )
                    .child(
                        div()
                            .flow_row()
                            .id(("path", i))
                            .flex_1()
                            .min_w_0()
                            .justify_end()
                            .mono_sm()
                            .text_color(t.ink_3)
                            .when(!path_tip.is_empty(), |d| {
                                d.tooltip(move |_, cx| -> AnyView {
                                    Tooltip::view(path_tip.clone(), None, cx)
                                })
                            })
                            .child(ellipsis(path.clone())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .mono_sm()
                            .text_color(t.ink_3)
                            .child(focus_i18n::format_number(i64::from(*count))),
                    )
                    .into_any_element()
            }
            Row::Reason { text, .. } => {
                let tip = text.clone();
                indent(
                    div()
                        .flow_row()
                        .id(("row", i))
                        .w_full()
                        .items_center()
                        .gap(size::S2)
                        .h(size::list_row()),
                )
                .ui_sm()
                .text_color(t.ink_3)
                .tooltip(move |_, cx| -> AnyView { Tooltip::view(tip.clone(), None, cx) })
                .child(icon(Icon::SPARKLES, size::ICON_SMALL))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .child(ellipsis(text.clone())),
                )
                .into_any_element()
            }
            Row::Line {
                line, text, ranges, ..
            } => indent(base)
                .when(selected, |d| d.child(bar()))
                .on_click(click(false))
                .child(number(Some(*line)))
                .child(
                    div()
                        .flow_row()
                        .flex_1()
                        .min_w_0()
                        .ui_sm()
                        .text_color(t.ink_2)
                        .child(excerpt(text.clone(), ranges.clone(), mark)),
                )
                .into_any_element(),
            Row::Before {
                line,
                text,
                ranges,
                keep,
                ..
            } => base
                .when(selected, |d| d.child(bar()))
                .on_click(click(false))
                .child(keep_box(*keep))
                .child(number(Some(*line)))
                .child(sign("-"))
                .child(
                    div()
                        .flow_row()
                        .flex_1()
                        .min_w_0()
                        .ui_sm()
                        .text_color(if *keep { t.ink_2 } else { t.ink_3 })
                        .child(excerpt(text.clone(), ranges.clone(), mark)),
                )
                .into_any_element(),
            Row::After {
                text, ranges, keep, ..
            } => div()
                .flow_row()
                .id(("row", i))
                .w_full()
                .items_center()
                .gap(size::S2)
                .h(size::list_row())
                .px(size::S3)
                .child(div().flex_none().w(size::CHECKBOX))
                .child(number(None))
                .child(sign("+"))
                .child(
                    div()
                        .flow_row()
                        .flex_1()
                        .min_w_0()
                        .ui_sm()
                        .text_color(if *keep { t.ink_2 } else { t.ink_3 })
                        .child(excerpt(text.clone(), ranges.clone(), soft_mark(&t))),
                )
                .into_any_element(),
        }
    }

    fn assist_card(&mut self, t: Tokens, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.assist.visible {
            return None;
        }
        let a = &self.assist;
        let answer = redact::restore(&a.answer, &a.replacements);
        let running = a.running;
        let failure = a.failure.clone();
        let stage = a.stage;
        let cost = a.cost_usd.map(|usd| {
            t_args(
                "search-ai-cost",
                &[("amount", Arg::Str(&focus_i18n::format_decimal(usd, 4)))],
            )
        });
        let nothing = !running
            && failure.is_none()
            && !a.stopped
            && answer.is_empty()
            && self.hits.is_empty();
        let queries = a.queries.clone();
        let editing = a.editing;

        let header = div()
            .flow_row()
            .items_center()
            .gap(size::S2)
            .h(px(24.))
            .child(icon(Icon::SPARKLES, size::ICON_DENSE).text_color(t.ink_3))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .ui_xs()
                    .text_color(t.ink_3)
                    .child(ellipsis(tr("search-ai-answer"))),
            )
            .when(running, |d| {
                d.child(
                    div()
                        .id("assist-stop")
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(4.))
                        .h(px(24.))
                        .px(size::S2)
                        .rounded(size::RADIUS)
                        .border_1()
                        .border_color(t.line_2)
                        .ui_xs()
                        .text_color(t.ink_2)
                        .cursor_pointer()
                        .hover(move |s| s.border_color(t.ink_3).text_color(t.ink))
                        .child(icon(STOP, size::ICON_SMALL))
                        .child(tr("search-ai-stop"))
                        .on_click(cx.listener(|this, _, _, cx| this.stop_clicked(cx))),
                )
            })
            .when_some(cost.filter(|_| !running), |d, cost| {
                d.child(div().flex_none().mono_sm().text_color(t.ink_3).child(cost))
            });

        let mut body = div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .p(size::S3)
            .border_1()
            .border_color(t.line_2)
            .child(header);
        if !a.intent.is_empty() {
            body = body
                .child(
                    div()
                        .ui_xs()
                        .text_color(t.ink_3)
                        .child(tr("search-ai-intent")),
                )
                .child(
                    div()
                        .ui_sm()
                        .text_color(t.ink_2)
                        .text_start()
                        .child(a.intent.clone()),
                );
        }
        if running
            && answer.is_empty()
            && let Some(stage) = stage
        {
            body = body.child(div().ui_sm().text_color(t.ink_3).child(stage_text(stage)));
        }
        if !answer.is_empty() {
            body = body.child(div().w_full().child(self.answer_view.clone()));
        }
        if self.assist.stopped {
            body = body.child(
                div()
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(tr("search-ai-stopped")),
            );
        }
        if nothing {
            body = body.child(
                div()
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(tr("search-ai-nothing")),
            );
        }
        if let Some(failure) = failure {
            body = body.child(focus_kit::ErrorCard::new(failure_text(&failure)));
        }
        if !queries.is_empty() || editing.is_some() {
            body = body
                .child(
                    div()
                        .ui_xs()
                        .text_color(t.ink_3)
                        .child(tr("search-ai-queries")),
                )
                .child(
                    div()
                        .flow_row()
                        .flex_wrap()
                        .gap(size::S1)
                        .children(
                            queries.iter().enumerate().map(|(i, query)| {
                                self.query_pill(i, query, editing == Some(i), t, cx)
                            }),
                        )
                        .child(if editing == Some(queries.len()) {
                            self.query_editor(t).into_any_element()
                        } else {
                            div()
                                .id("query-add")
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(24.))
                                .rounded(size::RADIUS)
                                .bg(t.card_2)
                                .text_color(t.ink_3)
                                .cursor_pointer()
                                .hover(move |s| s.text_color(t.ink))
                                .tooltip(|_, cx| -> AnyView {
                                    Tooltip::view(tr("search-ai-query-add").into(), None, cx)
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let at = this.assist.queries.len();
                                    this.edit_query(at, window, cx);
                                }))
                                .child(icon(Icon::PLUS, size::ICON_SMALL))
                                .into_any_element()
                        }),
                );
        }
        Some(
            div()
                .id("assist-card")
                .flex_none()
                .max_h(relative(0.55))
                .overflow_y_scroll()
                .px(size::S3)
                .pb(size::S2)
                .child(body)
                .into_any_element(),
        )
    }

    fn query_editor(&self, t: Tokens) -> Div {
        div()
            .flex()
            .items_center()
            .w(px(176.))
            .max_w(relative(1.))
            .h(px(24.))
            .px(size::S2)
            .rounded(size::RADIUS)
            .border_1()
            .border_color(t.ink)
            .bg(t.bg_2)
            .mono_sm()
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.editor)
                        .appearance(false)
                        .text_size(px(11.5))
                        .line_height(px(16.))
                        .px_0()
                        .py_0()
                        .h(px(16.)),
                ),
            )
    }

    fn query_pill(
        &self,
        i: usize,
        query: &str,
        editing: bool,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if editing {
            return self.query_editor(t).into_any_element();
        }
        div()
            .flow_row()
            .items_center()
            .gap(px(2.))
            .h(px(24.))
            .max_w(relative(1.))
            .pad_start(size::S2)
            .pad_end(px(2.))
            .rounded(size::RADIUS)
            .bg(t.card_2)
            .mono_sm()
            .text_color(t.ink_2)
            .child(
                div()
                    .id(("query-edit", i))
                    .flex()
                    .min_w_0()
                    .cursor_pointer()
                    .hover(move |s| s.text_color(t.ink))
                    .tooltip(|_, cx| -> AnyView {
                        Tooltip::view(tr("search-ai-query-edit").into(), None, cx)
                    })
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.edit_query(i, window, cx)),
                    )
                    .child(ellipsis(query.to_string())),
            )
            .child(
                div()
                    .id(("query-remove", i))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(20.))
                    .rounded(size::RADIUS)
                    .text_color(t.ink_3)
                    .cursor_pointer()
                    .hover(move |s| s.bg(t.card_3).text_color(t.ink))
                    .tooltip(|_, cx| -> AnyView {
                        Tooltip::view(tr("search-ai-query-remove").into(), None, cx)
                    })
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.remove_query(i, window, cx)),
                    )
                    .child(icon(Icon::CLOSE, size::ICON_SMALL)),
            )
            .into_any_element()
    }

    fn placeholder(
        &self,
        t: Tokens,
        title: String,
        hint: Option<String>,
        extra: Option<Div>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(size::S2)
            .px(size::S4)
            .pt(size::S8)
            .text_center()
            .child(icon(Icon::SEARCH, size::ICON).text_color(t.ink_4))
            .child(div().ui_strong().text_color(t.ink).child(title))
            .when_some(hint, |d, hint| {
                d.child(div().ui_sm().text_color(t.ink_3).child(hint))
            })
            .when_some(extra, |d, extra| d.child(extra))
    }

    fn body(&mut self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        if self.services.is_none() {
            let (done, total) = self.indexing.unwrap_or((0, 0));
            let text = if total == 0 {
                tr("search-searching")
            } else {
                t_args(
                    "search-index-building",
                    &[
                        ("done", Arg::Int(done as i64)),
                        ("total", Arg::Int(total as i64)),
                    ],
                )
            };
            let fraction = if total == 0 {
                0.0
            } else {
                (done as f32 / total as f32).clamp(0.0, 1.0)
            };
            return div()
                .flex()
                .flex_col()
                .gap(size::S2)
                .px(size::S3)
                .pt(size::S4)
                .child(div().ui_sm().text_color(t.ink_3).child(text))
                .child(
                    div()
                        .h(px(2.))
                        .w_full()
                        .bg(t.line)
                        .child(div().h_full().w(relative(fraction)).bg(t.ink)),
                )
                .into_any_element();
        }
        let query_empty = self.query(cx).trim().is_empty();
        if query_empty {
            let hints = div()
                .flow_row()
                .flex_wrap()
                .justify_center()
                .gap(size::S2)
                .pt(size::S2)
                .ui_sm()
                .text_color(t.ink_3);
            let hint = |keys: &'static str, key: &'static str| {
                div()
                    .flow_row()
                    .items_center()
                    .gap(size::S1)
                    .child(Kbd::new(focus_i18n::keys_text(keys)))
                    .child(tr(key))
            };
            let mut extra = hints
                .child(hint("enter", "search-hint-open"))
                .child(hint("ctrl-enter", "search-hint-new-tab"))
                .child(hint("escape", "search-hint-clear"));
            if self.assist_backend().is_some() && self.ai_on && !self.replace.open {
                extra = extra.child(hint("enter", "search-hint-ask"));
            }
            return self
                .placeholder(
                    t,
                    tr("search-empty-title"),
                    Some(tr("search-empty-hint")),
                    Some(extra),
                )
                .into_any_element();
        }
        if self.rows.is_empty() {
            if self.replace.open
                && (self.replace.building
                    || self.replace.plan.is_none()
                    || self.replace.report.is_some())
            {
                return div().into_any_element();
            }
            if !self.replace.open && (self.searching || !self.has_results) {
                return div().into_any_element();
            }
            if self.assist.visible && (self.assist.running || self.assist.failure.is_some()) {
                return div().into_any_element();
            }
            return self
                .placeholder(
                    t,
                    tr("search-no-results"),
                    Some(tr("search-none-hint")),
                    None,
                )
                .into_any_element();
        }
        let count = self.rows.len();
        let truncated = self.truncated.then(|| {
            t_args(
                "search-results-truncated",
                &[("limit", Arg::Int(self.options.max_files as i64))],
            )
        });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                uniform_list(
                    "search-rows",
                    count,
                    cx.processor(move |this, range: Range<usize>, _, cx| {
                        this.visible = range.clone();
                        range.map(|i| this.row(i, t, cx)).collect()
                    }),
                )
                .flex_1()
                .track_scroll(self.scroll.clone()),
            )
            .when_some(truncated, |d, text| {
                d.child(
                    div()
                        .flex_none()
                        .px(size::S3)
                        .py(size::S1)
                        .border_t_1()
                        .border_color(t.line)
                        .ui_sm()
                        .text_color(t.ink_3)
                        .child(text),
                )
            })
            .into_any_element()
    }
}

fn vault_hint(vault: &Vault, language: &str) -> VaultHint {
    let mut folders: Vec<String> = Vec::new();
    for meta in vault.notes() {
        if let Some(first) = meta
            .path
            .components()
            .next()
            .filter(|_| meta.path.components().count() > 1)
        {
            let name = first.as_os_str().to_string_lossy().to_string();
            if !folders.contains(&name) {
                folders.push(name);
            }
        }
    }
    folders.truncate(30);
    let mut tags = vault.tags();
    tags.sort_by_key(|tag| std::cmp::Reverse(tag.count));
    VaultHint {
        folders,
        tags: tags.into_iter().take(30).map(|t| t.tag).collect(),
        languages: vec![language.to_string()],
    }
}

fn field_row(field: Field, top: Pixels) -> Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .px(size::S3)
        .pt(top)
        .pb(size::S1)
        .child(div().flex_1().min_w_0().child(field))
}

impl Render for SearchPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let clear = (!self.input.read(cx).value().is_empty()).then(|| {
            div()
                .id("clear")
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(20.))
                .rounded(size::RADIUS)
                .text_color(t.ink_3)
                .cursor_pointer()
                .hover(move |s| s.text_color(t.ink))
                .tooltip(|_, cx| -> AnyView {
                    Tooltip::view(
                        tr("search-clear").into(),
                        Some(focus_i18n::keys_text("escape").into()),
                        cx,
                    )
                })
                .on_click(cx.listener(|this, _, window, cx| this.set_query("", window, cx)))
                .child(icon(Icon::CLOSE, size::ICON_DENSE))
        });
        let message: Option<SharedString> = (!self.diagnostics.is_empty()).then(|| {
            self.diagnostics
                .iter()
                .map(|d| diagnostic_text(d, &self.diagnosed))
                .collect::<Vec<_>>()
                .join(" ")
                .into()
        });
        let field = Field::new(&self.input)
            .icon(Icon::SEARCH)
            .error(message)
            .when_some(clear, |f, clear| f.suffix(clear));
        let toolbar = self.toolbar(t, cx);
        let total = self.total_line(t);
        let help = self.help_open.then(|| self.help(t, cx));
        let card = self.assist_card(t, cx);
        let replace_card = self.replace_card(t);
        let replace_field = self.replace.open.then(|| {
            Field::new(&self.replacement)
                .icon(REPLACE)
                .error(self.replace_error().map(SharedString::from))
        });
        let footer = self.replace_footer(t, cx);
        let body = self.body(t, cx);

        div()
            .id("search-panel")
            .key_context("SearchPanel")
            .track_focus(&self.focus)
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .bg(t.bg)
            .text_color(t.ink_2)
            .child(field_row(field, size::S2))
            .when_some(replace_field, |d, field| d.child(field_row(field, px(0.))))
            .child(toolbar)
            .child(total)
            .child(div().flex_none().h(px(1.)).bg(t.line))
            .when_some(help, |d, help| {
                d.child(
                    div()
                        .id("search-help")
                        .flex_none()
                        .max_h(relative(0.5))
                        .overflow_y_scroll()
                        .child(help),
                )
            })
            .when_some(card, |d, card| {
                d.child(div().flex_none().pt(size::S2).child(card))
            })
            .when_some(replace_card, |d, card| {
                d.child(div().flex_none().px(size::S3).pt(size::S2).child(card))
            })
            .child(body)
            .when_some(footer, |d, footer| d.child(footer))
    }
}
