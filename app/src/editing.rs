use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Bounds, Context, Entity, EntityInputHandler, Focusable, KeystrokeEvent, Pixels,
    ScrollHandle, Window,
};
use gpui_component::RopeExt as _;
use gpui_component::input::InputState;

use crate::editor::NoteEditor;
use crate::shell::Shell;
use crate::slash::{self, Entry, Hit, Insert};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKind {
    Commands,
    Languages,
    Notes,
    Tags,
    Templates,
}

impl MenuKind {
    fn trigger(self) -> &'static str {
        match self {
            MenuKind::Commands => "/",
            MenuKind::Languages => "```",
            MenuKind::Notes => "[[",
            MenuKind::Tags => "#",
            MenuKind::Templates => "/",
        }
    }
}

pub struct Slash {
    pub editor: Entity<NoteEditor>,
    pub kind: MenuKind,
    pub anchor: usize,
    pub query: String,
    pub entries: Arc<Vec<Entry>>,
    pub hits: Vec<Hit>,
    pub selected: usize,
    pub scroll: ScrollHandle,
    pub drawn_at: Option<Bounds<Pixels>>,
    pub below: Option<(Pixels, bool)>,
}

impl Slash {
    fn child(&self, rank: usize) -> Option<usize> {
        slash::child_of(
            &self.entries,
            self.kind == MenuKind::Commands,
            &self.hits,
            &self.query,
            rank,
        )
    }

    fn select(&mut self, rank: usize) {
        self.selected = rank;
        self.scroll.scroll_to_item(self.child(rank).unwrap_or(0));
    }
}

pub struct SlashCatalog {
    pub entries: Arc<Vec<Entry>>,
    pub recent: Vec<String>,
    pub dismissed_link: Option<usize>,
}

impl SlashCatalog {
    pub fn new() -> Self {
        SlashCatalog {
            entries: Arc::new(slash::catalog()),
            recent: Vec::new(),
            dismissed_link: None,
        }
    }
}

fn unclosed_link(before: &str) -> Option<(usize, &str)> {
    let at = before.rfind("[[")?;
    let query = &before[at + 2..];
    (!query.contains(']') && query.chars().count() <= 60).then_some((at, query))
}

fn unclosed_tag(before: &str) -> Option<(usize, &str)> {
    let at = before.rfind('#')?;
    let query = &before[at + 1..];
    let opens = before[..at]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    (opens
        && !query.is_empty()
        && !query.contains(char::is_whitespace)
        && query.chars().count() <= 60)
        .then_some((at, query))
}

#[derive(Clone, Default)]
pub struct EditorBounds(pub Rc<Cell<Bounds<Pixels>>>);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorFont {
    pub family: &'static str,
    pub size: f32,
    pub line: f32,
}

impl EditorFont {
    pub fn from_choice(choice: usize) -> EditorFont {
        if choice == 1 {
            EditorFont {
                family: focus_kit::typography::MONO,
                size: 13.,
                line: 21.,
            }
        } else {
            EditorFont {
                family: focus_kit::typography::SANS,
                size: 15.,
                line: 25.,
            }
        }
    }
}

pub fn editor_surface(
    input: &Entity<InputState>,
    font: EditorFont,
    bounds: &EditorBounds,
    fill: bool,
) -> gpui::Div {
    use crate::actions::*;
    use gpui::{InteractiveElement, ParentElement, Styled, div, px};
    let on = |f: fn(&Entity<InputState>, &mut Window, &mut App)| {
        let input = input.clone();
        move |window: &mut Window, cx: &mut App| f(&input, window, cx)
    };
    use crate::format::{Inline, apply_field};
    let bold = on(|i, w, cx| apply_field(i, Some(Inline::Bold), w, cx));
    let italic = on(|i, w, cx| apply_field(i, Some(Inline::Italic), w, cx));
    let strike = on(|i, w, cx| apply_field(i, Some(Inline::Strike), w, cx));
    let highlight = on(|i, w, cx| apply_field(i, Some(Inline::Highlight), w, cx));
    let code = on(|i, w, cx| apply_field(i, Some(Inline::Code), w, cx));
    let url = on(|i, w, cx| apply_field(i, None, w, cx));
    let heading = on(|i, w, cx| line_prefix(i, "#", w, cx));
    let bullets = on(|i, w, cx| line_prefix(i, "- ", w, cx));
    let task = on(|i, w, cx| line_prefix(i, "- [ ] ", w, cx));
    let bounds = bounds.0.clone();
    let field = gpui_component::input::Input::new(input)
        .appearance(false)
        .text_size(px(font.size))
        .line_height(px(font.line))
        .px_0()
        .py_0();
    let field = if fill { field.h_full() } else { field };
    div()
        .relative()
        .w_full()
        .when(fill, |this| this.h_full())
        .key_context(EDITOR_CONTEXT)
        .font_family(font.family)
        .on_action(move |_: &FormatBold, window, cx| bold(window, cx))
        .on_action(move |_: &FormatItalic, window, cx| italic(window, cx))
        .on_action(move |_: &FormatStrike, window, cx| strike(window, cx))
        .on_action(move |_: &FormatHighlight, window, cx| highlight(window, cx))
        .on_action(move |_: &FormatCode, window, cx| code(window, cx))
        .on_action(move |_: &FormatLink, window, cx| url(window, cx))
        .on_action(move |_: &FormatHeading, window, cx| heading(window, cx))
        .on_action(move |_: &FormatBullets, window, cx| bullets(window, cx))
        .on_action(move |_: &FormatTask, window, cx| task(window, cx))
        .child(field)
        .child(
            gpui::canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        )
}

fn utf16(input: &InputState, range: Range<usize>) -> Range<usize> {
    let text = input.text();
    text.offset_to_offset_utf16(range.start)..text.offset_to_offset_utf16(range.end)
}

pub(crate) fn replace(
    input: &Entity<InputState>,
    range: Range<usize>,
    text: &str,
    cursor: usize,
    window: &mut Window,
    cx: &mut App,
) {
    input.update(cx, |input, cx| {
        let start = range.start;
        let range16 = utf16(input, range);
        input.replace_text_in_range(Some(range16), text, window, cx);
        let position = input.text().offset_to_position(start + cursor);
        input.set_cursor_position(position, window, cx);
    });
}

fn selection(input: &Entity<InputState>, window: &mut Window, cx: &mut App) -> Range<usize> {
    input.update(cx, |input, cx| {
        let selected = input
            .selected_text_range(false, window, cx)
            .map(|s| s.range)
            .unwrap_or(0..0);
        let text = input.text();
        text.offset_utf16_to_offset(selected.start)..text.offset_utf16_to_offset(selected.end)
    })
}

impl Shell {
    fn editor_focused(&self, window: &Window, cx: &App) -> Option<Entity<NoteEditor>> {
        let editor = self.active_editor()?.clone();
        let focused = editor
            .read(cx)
            .typing_input()
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
            || (editor.read(cx).caret_in_view(cx) && self.reading_focused(window, cx).is_some());
        focused.then_some(editor)
    }

    fn reading_focused(&self, window: &Window, cx: &App) -> Option<Entity<NoteEditor>> {
        let editor = self.active_editor()?.clone();
        let reading = editor.read(cx).reading.read(cx);
        gpui::Focusable::focus_handle(reading, cx)
            .is_focused(window)
            .then_some(editor)
    }

    pub fn on_edited(&mut self, editor: &Entity<NoteEditor>, cx: &mut Context<Self>) {
        let input = editor.read(cx).typing_input();
        let (text, cursor) = {
            let state = input.read(cx);
            (state.text().clone(), state.cursor())
        };
        if let Some(menu) = &self.slash {
            let (kind, anchor) = (menu.kind, menu.anchor);
            let start = anchor + kind.trigger().len();
            let valid = cursor >= start
                && start <= text.len()
                && text.is_char_boundary(anchor)
                && text.is_char_boundary(start)
                && text.slice(anchor..start) == kind.trigger();
            let query = if valid {
                text.slice(start..cursor).to_string()
            } else {
                String::new()
            };
            let spaced = !matches!(kind, MenuKind::Notes | MenuKind::Templates)
                && query.contains(char::is_whitespace);
            if !valid
                || spaced
                || query.contains('\n')
                || query.contains(']')
                || (kind == MenuKind::Tags && query.contains('#'))
                || query.chars().count() > 60
            {
                self.slash = None;
            } else {
                self.refilter_menu(query, cx);
                if self
                    .slash
                    .as_ref()
                    .is_some_and(|menu| menu.kind == MenuKind::Tags && menu.hits.is_empty())
                {
                    self.slash = None;
                }
            }
            cx.notify();
            return;
        }
        let ends = |s: &str| {
            cursor >= s.len()
                && text.is_char_boundary(cursor - s.len())
                && text.slice(cursor - s.len()..cursor) == s
        };
        let char_before = |end: usize| {
            let start = (1..=4.min(end))
                .map(|back| end - back)
                .find(|start| text.is_char_boundary(*start))?;
            text.slice(start..end).to_string().chars().next()
        };
        let line_start = text.line_start_offset(text.offset_to_position(cursor).line as usize);
        let before_caret = text.slice(line_start..cursor).to_string();
        if ends("[[") {
            self.open_menu(editor, MenuKind::Notes, cursor - 2, cx);
        } else if let Some((at, query)) = unclosed_link(&before_caret)
            && self.slash_catalog.dismissed_link != Some(line_start + at)
        {
            let query = query.to_string();
            self.open_menu(editor, MenuKind::Notes, line_start + at, cx);
            self.refilter_menu(query, cx);
        } else if ends("/") {
            let in_value = editor.read(cx).reading.read(cx).caret_in_property(cx);
            if !in_value && char_before(cursor - 1).is_none_or(|c| c.is_whitespace()) {
                self.open_menu(editor, MenuKind::Commands, cursor - 1, cx);
            }
        } else if ends("```") && (cursor == 3 || char_before(cursor - 3) == Some('\n')) {
            self.open_menu(editor, MenuKind::Languages, cursor - 3, cx);
        } else if let Some((at, query)) = unclosed_tag(&before_caret)
            .or_else(|| ends("#").then(|| (before_caret.len() - 1, "")))
            .filter(|(at, _)| {
                before_caret[..*at]
                    .chars()
                    .next_back()
                    .is_none_or(char::is_whitespace)
            })
            && self.slash_catalog.dismissed_link != Some(line_start + at)
            && !editor.read(cx).reading.read(cx).caret_in_property(cx)
        {
            let query = query.to_string();
            self.open_menu(editor, MenuKind::Tags, line_start + at, cx);
            self.refilter_menu(query, cx);
            if self.slash.as_ref().is_some_and(|menu| menu.hits.is_empty()) {
                self.slash = None;
            }
        }
    }

    pub fn menu_line(&self) -> String {
        let Some(menu) = &self.slash else {
            return "none".into();
        };
        let kind = match menu.kind {
            MenuKind::Commands => "commands",
            MenuKind::Languages => "languages",
            MenuKind::Notes => "notes",
            MenuKind::Tags => "tags",
            MenuKind::Templates => "templates",
        };
        let selected = menu
            .hits
            .get(menu.selected)
            .map_or("", |hit| menu.entries[hit.entry].id.as_str());
        let shown = self.menu_row(&menu.selected.to_string()).is_some();
        format!(
            "{kind} | {} | {selected} | {}",
            menu.query,
            if shown { "shown" } else { "hidden" }
        )
    }

    pub fn show_menu_row(&mut self, row: &str, cx: &mut Context<Self>) -> Option<usize> {
        let menu = self.slash.as_mut()?;
        if row == "title" {
            menu.scroll.scroll_to_item(0);
            cx.notify();
            return (menu.child(0)? > 0).then_some(0);
        }
        let rank = match row.parse::<usize>() {
            Ok(rank) => rank,
            Err(_) => menu
                .hits
                .iter()
                .position(|hit| menu.entries[hit.entry].id == row)?,
        };
        menu.child(rank)?;
        menu.select(rank);
        cx.notify();
        Some(rank)
    }

    pub fn menu_row(&self, row: &str) -> Option<gpui::Point<Pixels>> {
        let menu = self.slash.as_ref()?;
        let child = match row {
            "title" => 0,
            rank => menu.child(rank.parse().ok()?)?,
        };
        let at = menu.scroll.bounds_for_item(child)?.center() + menu.scroll.offset();
        menu.scroll.bounds().contains(&at).then_some(at)
    }

    pub fn open_menu(
        &mut self,
        editor: &Entity<NoteEditor>,
        kind: MenuKind,
        anchor: usize,
        cx: &mut Context<Self>,
    ) {
        let entries = match kind {
            MenuKind::Commands if !self.prefs.read(cx).bool("ai.enabled") => Arc::new(
                self.slash_catalog
                    .entries
                    .iter()
                    .filter(|entry| !matches!(entry.insert, Insert::Ai(_)))
                    .cloned()
                    .collect(),
            ),
            MenuKind::Commands => self.slash_catalog.entries.clone(),
            MenuKind::Languages => Arc::new(slash::languages()),
            MenuKind::Notes | MenuKind::Tags => Arc::new(Vec::new()),
            MenuKind::Templates => Arc::new(
                self.library
                    .as_ref()
                    .map(|library| {
                        slash::vault_templates(&focus_view_vault::templates(
                            library.vault(),
                            &self.prefs.read(cx).text("general.templates"),
                        ))
                    })
                    .unwrap_or_default(),
            ),
        };
        self.slash = Some(Slash {
            editor: editor.clone(),
            kind,
            anchor,
            query: String::new(),
            entries,
            hits: Vec::new(),
            selected: 0,
            scroll: ScrollHandle::new(),
            drawn_at: None,
            below: None,
        });
        self.refilter_menu(String::new(), cx);
        cx.notify();
    }

    fn refilter_menu(&mut self, query: String, _: &mut Context<Self>) {
        let library = self.library.clone();
        let Some(menu) = &mut self.slash else { return };
        match menu.kind {
            MenuKind::Commands => {
                menu.hits = slash::filter(&menu.entries, &query, &self.slash_catalog.recent)
            }
            MenuKind::Languages | MenuKind::Templates => {
                menu.hits = slash::filter(&menu.entries, &query, &[])
            }
            MenuKind::Notes => {
                let (entries, hits) = library
                    .map(|l| slash::notes(&l, &query))
                    .unwrap_or_default();
                menu.entries = Arc::new(entries);
                menu.hits = hits;
            }
            MenuKind::Tags => {
                let (entries, hits) = library.map(|l| slash::tags(&l, &query)).unwrap_or_default();
                menu.entries = Arc::new(entries);
                menu.hits = hits;
            }
        }
        menu.query = query;
        menu.select(0);
    }

    pub fn intercept(
        &mut self,
        event: &KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.keys_outside_overlay(event, window, cx) {
            return true;
        }
        let keystroke = &event.keystroke;
        let plain = !keystroke.modifiers.control
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.platform;
        if let crate::shell::Screen::Welcome { selected } = self.screen
            && matches!(self.overlay, crate::shell::Overlay::None)
        {
            let count = crate::welcome::languages().len();
            match keystroke.key.as_str() {
                "down" if plain => self.welcome_select((selected + 1) % count, window, cx),
                "up" if plain => self.welcome_select((selected + count - 1) % count, window, cx),
                "enter" if plain => self.welcome_confirm(cx),
                _ => return false,
            }
            return true;
        }
        if let crate::shell::Screen::Updates { offered } = self.screen
            && matches!(self.overlay, crate::shell::Overlay::None)
        {
            match keystroke.key.as_str() {
                "down" | "up" if plain => self.updates_select(!offered, cx),
                "enter" if plain => self.updates_confirm(window, cx),
                _ => return false,
            }
            return true;
        }
        if self.find_keys(event, window, cx) {
            return true;
        }
        if self.selection_bar_keys(event, window, cx) {
            return true;
        }
        if !matches!(self.overlay, crate::shell::Overlay::Settings { .. }) {
            let editor = self.editor_focused(window, cx).is_some();
            match crate::actions::remap(keystroke, editor) {
                Some(crate::actions::Remap::Run(action)) => {
                    window.dispatch_action(action, cx);
                    return true;
                }
                Some(crate::actions::Remap::Drop) => return true,
                None => {}
            }
        }
        if self.slash.is_some() && self.editor_focused(window, cx).is_none() {
            self.slash = None;
            cx.notify();
            return false;
        }
        if let Some(menu) = &mut self.slash {
            let count = menu.hits.len();
            match keystroke.key.as_str() {
                "down" if plain && count > 0 => menu.select((menu.selected + 1) % count),
                "up" if plain && count > 0 => menu.select((menu.selected + count - 1) % count),
                "pageup" | "pagedown" if plain && count > 0 => {
                    let page = slash::VISIBLE_ROWS;
                    menu.select(if keystroke.key == "pageup" {
                        menu.selected.saturating_sub(page)
                    } else {
                        (menu.selected + page).min(count - 1)
                    });
                }
                "enter" | "tab" if plain => {
                    let pick = menu.selected;
                    self.pick_slash(pick, window, cx);
                }
                "escape" => {
                    if matches!(menu.kind, MenuKind::Notes | MenuKind::Tags) {
                        self.slash_catalog.dismissed_link = Some(menu.anchor);
                    }
                    menu.editor
                        .clone()
                        .update(cx, |editor, cx| editor.source_if_unseen(cx));
                    self.slash = None;
                }
                _ => return false,
            }
            cx.notify();
            return true;
        }
        if self.live_keys(event, window, cx) {
            return true;
        }
        if plain
            && self.prefs.read(cx).bool("editor.close_pairs")
            && let Some(editor) = self.editor_focused(window, cx)
            && !editor.read(cx).caret_in_view(cx)
            && let Some(typed) = keystroke.key_char.as_deref()
            && close_pair(&editor.read(cx).typing_input(), typed, window, cx)
        {
            return true;
        }
        if keystroke.key == "v"
            && keystroke.modifiers.control
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.shift
        {
            if let Some(editor) = self.editor_focused(window, cx) {
                self.paste_clipboard(editor, true, window, cx);
                return true;
            }
            if let Some(editor) = self.reading_focused(window, cx) {
                self.paste_clipboard(editor, false, window, cx);
                return true;
            }
        }
        let paging = matches!(keystroke.key.as_str(), "pageup" | "pagedown");
        if paging
            && plain
            && keystroke.modifiers.shift
            && let Some(editor) = self.editor_focused(window, cx)
            && editor.read(cx).view == crate::editor::ViewMode::Source
        {
            let input = editor.read(cx).typing_input();
            crate::window_keys::select_page(input, keystroke.key == "pagedown", window, cx);
            return true;
        }
        if keystroke.key == "tab"
            && plain
            && let Some(editor) = self.editor_focused(window, cx)
            && editor.read(cx).view == crate::editor::ViewMode::Source
        {
            let input = editor.read(cx).typing_input();
            if tab_in_list(&input, keystroke.modifiers.shift, window, cx) {
                return true;
            }
        }
        if keystroke.key == "enter"
            && plain
            && !keystroke.modifiers.shift
            && self.prefs.read(cx).bool("editor.continue_lists")
            && let Some(editor) = self.editor_focused(window, cx)
            && !editor.read(cx).caret_in_view(cx)
        {
            return continue_list(&editor.read(cx).typing_input(), window, cx);
        }
        false
    }

    fn chain_menu(&mut self, from: &Slash, kind: MenuKind, anchor: usize, cx: &mut Context<Self>) {
        self.open_menu(&from.editor, kind, anchor, cx);
        if let Some(menu) = &mut self.slash {
            menu.drawn_at = from.drawn_at;
        }
    }

    pub fn pick_slash(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.slash.take() else {
            return;
        };
        let Some(hit) = menu.hits.get(index) else {
            return;
        };
        let entry = menu.entries[hit.entry].clone();
        let input = menu.editor.read(cx).typing_input();
        let editor = menu.editor.clone();
        let replace = |range: Range<usize>,
                       text: &str,
                       caret: usize,
                       window: &mut Window,
                       cx: &mut Context<Self>| {
            editor.update(cx, |editor, cx| {
                editor.replace_range(range, text, caret, window, cx)
            })
        };
        let seen = |cx: &mut Context<Self>| {
            editor.update(cx, |editor, cx| editor.source_if_unseen(cx));
            cx.notify();
        };
        let cursor = input.read(cx).cursor();
        let range = menu.anchor..cursor.max(menu.anchor + 1);
        let query_range = menu.anchor + menu.kind.trigger().len()
            ..cursor.max(menu.anchor + menu.kind.trigger().len());
        let insertion = match &entry.insert {
            Insert::Language(id) => {
                let after_fence = input
                    .read(cx)
                    .text()
                    .slice(query_range.end..input.read(cx).text().len())
                    .to_string();
                let skip = if after_fence.starts_with('\n') { 1 } else { 0 };
                replace(query_range, id, id.len() + skip, window, cx);
                return seen(cx);
            }
            Insert::Note { title, path } => {
                let rest = input
                    .read(cx)
                    .text()
                    .slice(query_range.end..input.read(cx).text().len())
                    .to_string();
                let from = menu.editor.read(cx).meta.path.to_path_buf();
                let (style, form) = {
                    let prefs = self.prefs.read(cx);
                    (
                        prefs.choice("editor.link_style"),
                        prefs.choice("editor.link_path"),
                    )
                };
                let in_value = menu.editor.read(cx).reading.read(cx).caret_in_property(cx);
                let style = if in_value { 1 } else { style };
                let unique = self.library.as_ref().is_some_and(|library| {
                    library
                        .vault()
                        .summaries()
                        .iter()
                        .filter(|s| s.path.file_stem() == path.file_stem())
                        .count()
                        == 1
                });
                let target = link_target(&from, path, form, unique);
                if style == 0 {
                    let end = query_range.end + if rest.starts_with("]]") { 2 } else { 0 };
                    let encoded = target.replace(' ', "%20");
                    let text = format!("[{title}]({encoded}.md)");
                    let cursor = text.len();
                    replace(menu.anchor..end, &text, cursor, window, cx);
                } else {
                    let closing = if rest.starts_with("]]") { "" } else { "]]" };
                    let shown = if target == *title {
                        target.clone()
                    } else {
                        format!("{target}|{title}")
                    };
                    let text = format!("{shown}{closing}");
                    replace(query_range, &text, shown.len() + 2, window, cx);
                }
                return seen(cx);
            }
            Insert::Tag(tag) => {
                self.slash_catalog.dismissed_link = Some(menu.anchor);
                replace(query_range, tag, tag.len(), window, cx);
                return seen(cx);
            }
            Insert::Unavailable => return,
            Insert::ChooseTemplate => {
                replace(range.clone(), "/", 1, window, cx);
                return self.chain_menu(&menu, MenuKind::Templates, range.start, cx);
            }
            Insert::VaultTemplate(template) => {
                let Some(library) = self.library.clone() else {
                    return;
                };
                let template = match library.read(*template) {
                    Ok(text) => text,
                    Err(error) => {
                        return self.notify(
                            focus_kit::Tone::Danger,
                            crate::notices::NoticeText::Vault(focus_view_vault::error_message(
                                &error,
                            )),
                            cx,
                        );
                    }
                };
                let title = menu.editor.read(cx).meta.title.to_string();
                let mut text = input.read(cx).value().to_string();
                text.replace_range(range.clone(), "");
                let (date, time) = crate::capture::now();
                let (text, caret) = focus_view_vault::insert_template(
                    &text,
                    range.start,
                    &template,
                    &title,
                    date,
                    time,
                );
                let whole = 0..input.read(cx).text().len();
                replace(whole, &text, caret, window, cx);
                return seen(cx);
            }
            Insert::Ai(action) => {
                replace(range, "", 0, window, cx);
                match action.as_str() {
                    "ask" => self.show_ai(window, cx),
                    "translate" => {
                        let target = if focus_i18n::language() == "en" {
                            "translate-fr"
                        } else {
                            "translate-en"
                        };
                        self.run_ai_action(target, window, cx);
                    }
                    action => self.run_ai_action(action, window, cx),
                }
                return;
            }
            Insert::Attachment => {
                replace(range, "", 0, window, cx);
                self.pick_attachment(window, cx);
                return;
            }
            Insert::Template(template) => template.clone(),
            Insert::Date => chrono::Local::now().format("%Y-%m-%d").to_string() + "$0",
            Insert::Time => chrono::Local::now().format("%H:%M").to_string() + "$0",
            Insert::DateTime => chrono::Local::now().format("%Y-%m-%d %H:%M").to_string() + "$0",
            Insert::Toc => table_of_contents(&input.read(cx).value()),
        };
        let insertion =
            if insertion.starts_with("---") && under_text(input.read(cx).text(), range.start) {
                format!("\n{insertion}")
            } else {
                insertion
            };
        let at = insertion.find("$0").unwrap_or(insertion.len());
        let text = insertion.replacen("$0", "", 1);
        replace(range.clone(), &text, at, window, cx);
        match entry.id.as_str() {
            "codeblock" => self.chain_menu(&menu, MenuKind::Languages, range.start, cx),
            "wikilink" => self.chain_menu(&menu, MenuKind::Notes, range.start, cx),
            _ => seen(cx),
        }
        self.slash_catalog.recent.retain(|id| id != &entry.id);
        self.slash_catalog.recent.insert(0, entry.id.clone());
        self.slash_catalog.recent.truncate(5);
        cx.notify();
    }
}

fn under_text(text: &gpui_component::Rope, at: usize) -> bool {
    let row = text.offset_to_position(at).line as usize;
    row > 0
        && text.line_start_offset(row) == at
        && !text.slice_line(row - 1).to_string().trim().is_empty()
}

fn link_target(from: &std::path::Path, to: &std::path::Path, form: usize, unique: bool) -> String {
    let to = to.with_extension("");
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let root = to.to_string_lossy().replace('\\', "/");
    match form {
        1 => root,
        2 if unique => name,
        2 => root,
        _ => {
            let base: Vec<_> = from
                .parent()
                .map(|p| p.components().collect())
                .unwrap_or_default();
            let target: Vec<_> = to.components().collect();
            let shared = base.iter().zip(&target).take_while(|(a, b)| a == b).count();
            let mut parts: Vec<String> =
                std::iter::repeat_n("..".to_string(), base.len() - shared).collect();
            parts.extend(
                target[shared..]
                    .iter()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned()),
            );
            parts.join("/")
        }
    }
}

const PAIRS: [(&str, &str); 3] = [("(", ")"), ("[", "]"), ("{", "}")];

fn close_pair(input: &Entity<InputState>, typed: &str, window: &mut Window, cx: &mut App) -> bool {
    let selected = selection(input, window, cx);
    let text = input.read(cx).text().clone();
    if let Some((open, close)) = PAIRS.iter().find(|(open, _)| *open == typed) {
        let inner = text.slice(selected.clone()).to_string();
        let cursor = open.len() + inner.len();
        replace(
            input,
            selected,
            &format!("{open}{inner}{close}"),
            cursor,
            window,
            cx,
        );
        return true;
    }
    if PAIRS.iter().any(|(_, close)| *close == typed)
        && selected.is_empty()
        && text
            .char_at(selected.start)
            .is_some_and(|c| typed.starts_with(c))
    {
        let position = text.offset_to_position(selected.start + typed.len());
        input.update(cx, |input, cx| {
            input.set_cursor_position(position, window, cx)
        });
        return true;
    }
    false
}

fn table_of_contents(source: &str) -> String {
    let document = focus_md::parse(source, &focus_md::ParseOptions::default());
    let mut lines = Vec::new();
    for block in &document.blocks {
        if let focus_md::BlockKind::Heading { level, inlines, .. } = &block.kind {
            let text = focus_md::plain_text(inlines);
            lines.push(format!("{}- [[#{text}]]", "  ".repeat(*level as usize - 1)));
        }
    }
    if lines.is_empty() {
        return format!("{}$0", focus_i18n::t("ui-slash-toc-empty"));
    }
    lines.join("\n") + "\n$0"
}

fn list_marker(line: &str) -> Option<(usize, usize, String)> {
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    for bullet in ["- ", "* ", "+ "] {
        if let Some(after) = rest.strip_prefix(bullet) {
            for task in ["[ ] ", "[x] ", "[X] ", "[/] ", "[-] "] {
                if after.starts_with(task) {
                    return Some((indent, bullet.len() + task.len(), format!("{bullet}[ ] ")));
                }
            }
            return Some((indent, bullet.len(), bullet.to_string()));
        }
    }
    if rest.starts_with("> ") {
        return Some((indent, 2, "> ".into()));
    }
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits < 10 {
        let separator = rest[digits..].chars().next()?;
        if (separator == '.' || separator == ')') && rest[digits + 1..].starts_with(' ') {
            let number: u64 = rest[..digits].parse().ok()?;
            return Some((indent, digits + 2, format!("{}{separator} ", number + 1)));
        }
    }
    None
}

fn item_marker(line: &str) -> Option<(usize, usize)> {
    let (indent, marker, _) = list_marker(line)?;
    (!line[indent..].starts_with('>')).then_some((indent, marker))
}

pub fn shift_item(
    text: &str,
    caret: usize,
    outdent: bool,
) -> Option<(Range<usize>, String, usize)> {
    let lines: Vec<(usize, &str)> = {
        let mut at = 0;
        text.split('\n')
            .map(|line| {
                let start = at;
                at += line.len() + 1;
                (start, line.trim_end_matches('\r'))
            })
            .collect()
    };
    let row = lines
        .iter()
        .rposition(|(start, line)| *start <= caret && caret <= start + line.len())?;
    let (indent, _) = item_marker(lines[row].1)?;
    let above = lines[..row]
        .iter()
        .rev()
        .take_while(|(_, line)| !line.trim().is_empty())
        .filter_map(|(_, line)| item_marker(line));
    let target = if outdent {
        if indent == 0 {
            return Some((caret..caret, String::new(), caret));
        }
        above.map(|(i, _)| i).find(|i| *i < indent).unwrap_or(0)
    } else {
        match above.into_iter().find(|(i, _)| *i <= indent) {
            Some((i, marker)) if i == indent => i + marker,
            _ => return Some((caret..caret, String::new(), caret)),
        }
    };
    let end = lines[row + 1..]
        .iter()
        .position(|(_, line)| {
            let depth = line.len() - line.trim_start().len();
            line.trim().is_empty() || depth <= indent
        })
        .map_or(lines.len(), |n| row + 1 + n);
    let delta = target as isize - indent as isize;
    let start = lines[row].0;
    let last = &lines[end - 1];
    let range = start..last.0 + last.1.len();
    let mut out = Vec::new();
    for (_, line) in &lines[row..end] {
        let depth = line.len() - line.trim_start().len();
        let kept = (depth as isize + delta).max(0) as usize;
        out.push(format!("{}{}", " ".repeat(kept), &line[depth..]));
    }
    let moved = (caret as isize + delta).max(start as isize + target as isize) as usize;
    Some((range, out.join("\n"), moved))
}

fn tab_in_list(
    input: &Entity<InputState>,
    outdent: bool,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let (text, cursor, selected) = {
        let state = input.read(cx);
        (
            state.text().to_string(),
            state.cursor(),
            state.selection_text(),
        )
    };
    if selected.contains('\n') {
        return false;
    }
    let Some((range, written, caret)) = shift_item(&text, cursor, outdent) else {
        return false;
    };
    if !range.is_empty() || !written.is_empty() {
        let at = caret - range.start;
        replace(input, range, &written, at, window, cx);
    }
    true
}

fn continue_list(input: &Entity<InputState>, window: &mut Window, cx: &mut App) -> bool {
    let (line, line_start, cursor) = {
        let state = input.read(cx);
        let text = state.text();
        let cursor = state.cursor();
        let row = text.offset_to_position(cursor).line as usize;
        let start = text.line_start_offset(row);
        (text.slice(start..cursor).to_string(), start, cursor)
    };
    let Some((indent, marker_len, next)) = list_marker(&line) else {
        return false;
    };
    if line[indent + marker_len..].trim().is_empty() {
        replace(input, line_start..cursor, "", 0, window, cx);
    } else {
        let insertion = format!("\n{}{next}", &line[..indent]);
        let at = insertion.len();
        replace(input, cursor..cursor, &insertion, at, window, cx);
    }
    true
}

pub fn line_prefix(input: &Entity<InputState>, prefix: &str, window: &mut Window, cx: &mut App) {
    let (text, cursor) = {
        let state = input.read(cx);
        (state.text().to_string(), state.cursor())
    };
    let change = crate::format::prefix_line(&text, cursor, prefix);
    let at = change.select.start - change.range.start;
    replace(input, change.range, &change.text, at, window, cx);
}

#[cfg(test)]
mod tests {
    use super::{list_marker, unclosed_tag};

    #[test]
    fn a_tag_being_typed_opens_its_completion() {
        assert_eq!(unclosed_tag("see #me"), Some((4, "me")));
        assert_eq!(unclosed_tag("#method"), Some((0, "method")));
        assert_eq!(unclosed_tag("# Title"), None);
        assert_eq!(unclosed_tag("C#"), None);
        assert_eq!(unclosed_tag("page#anchor"), None);
        assert_eq!(unclosed_tag("#done and more"), None);
        assert_eq!(unclosed_tag("a #"), None);
    }

    #[test]
    fn link_targets_follow_the_path_setting() {
        use std::path::Path;
        let from = Path::new("Red Team/Notes/Kerberoasting.md");
        let to = Path::new("Red Team/Tools/Rubeus.md");
        assert_eq!(super::link_target(from, to, 0, true), "../Tools/Rubeus");
        assert_eq!(
            super::link_target(from, to, 1, true),
            "Red Team/Tools/Rubeus"
        );
        assert_eq!(super::link_target(from, to, 2, true), "Rubeus");
        assert_eq!(
            super::link_target(from, to, 2, false),
            "Red Team/Tools/Rubeus"
        );
    }

    #[test]
    fn an_unclosed_link_before_the_caret_is_found_whole() {
        assert_eq!(super::unclosed_link("Voir [[Ci"), Some((5, "Ci")));
        assert_eq!(
            super::unclosed_link("[[Écri"),
            Some((0, "Écri")),
            "a query of several byte characters"
        );
        assert_eq!(super::unclosed_link("[[Cible]] et "), None, "a closed link");
        assert_eq!(super::unclosed_link("rien"), None);
        assert_eq!(
            super::unclosed_link("[[a]] puis [[b"),
            Some((11, "b")),
            "the last one"
        );
    }

    #[test]
    fn a_line_start_right_under_text_is_told_from_one_after_a_blank_line() {
        let under =
            |text: &str, at: usize| super::under_text(&gpui_component::Rope::from(text), at);
        assert!(under("Notes.\n/", 7));
        assert!(!under("Notes.\n\n/", 8), "a blank line above");
        assert!(!under("/", 0), "the first line");
        assert!(!under("Notes. /", 7), "inside a line");
        assert!(!under("Notes.\n  /", 9), "after an indent");
    }

    #[test]
    fn tab_indents_and_outdents_a_list_item_with_its_children() {
        let apply = |text: &str, caret: usize, outdent: bool| {
            let (range, written, moved) =
                super::shift_item(text, caret, outdent).expect("a list item");
            let out = format!("{}{written}{}", &text[..range.start], &text[range.end..]);
            (out, moved)
        };
        let text = "- one\n- two\n  - child\nafter";
        let caret = text.find("two").expect("in the text") + 3;
        let (indented, moved) = apply(text, caret, false);
        assert_eq!(indented, "- one\n  - two\n    - child\nafter");
        assert_eq!(&indented[moved - 3..moved], "two");
        let (back, _) = apply(&indented, moved, true);
        assert_eq!(back, text);
        let (first, _) = apply("- one\n", 3, false);
        assert_eq!(first, "- one\n", "no sibling above: nothing moves");
        let (numbered, _) = apply("1. a\n2. b", 8, false);
        assert_eq!(numbered, "1. a\n   2. b");
        assert!(super::shift_item("plain text", 3, false).is_none());
        assert!(super::shift_item("> quote", 3, false).is_none());
    }

    #[test]
    fn markers_continue() {
        assert_eq!(list_marker("- point").map(|m| m.2), Some("- ".into()));
        assert_eq!(
            list_marker("  - [x] fait").map(|m| (m.0, m.2)),
            Some((2, "- [ ] ".into()))
        );
        assert_eq!(list_marker("9. neuf").map(|m| m.2), Some("10. ".into()));
        assert_eq!(list_marker("texte"), None);
    }
}
