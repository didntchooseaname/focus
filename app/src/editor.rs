use std::sync::Arc;
use std::time::{Duration, SystemTime};

use focus_core::{NoteId, NoteMeta, VaultError};
use gpui::{
    App, AppContext, Context, Entity, EntityInputHandler, EventEmitter, Subscription, Task, Window,
};
use gpui_component::RopeExt as _;
use gpui_component::input::{InputEvent, InputState, Position};

use crate::library::Library;
use focus_view_reading::model::{self, LinkGoal};
use focus_view_reading::view::{ReadingEvent, ReadingView, Typesetting};

mod conflict;

const REPARSE: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Source,
    Live,
    Reading,
}

pub enum Load {
    Loading,
    Ready,
    Failed(String),
}

pub enum EditorEvent {
    Changed,
    Edited,
    OpenLink {
        goal: LinkGoal,
        new_tab: bool,
    },
    Zoom(String),
    Hover {
        goal: Option<LinkGoal>,
        position: gpui::Point<gpui::Pixels>,
    },
    Failed(String),
    Notice(String),
    OpenAt {
        note: NoteId,
        line: Option<u32>,
        new_tab: bool,
    },
    LinkPage(usize),
    Crumb {
        folder: std::path::PathBuf,
        at: gpui::Point<gpui::Pixels>,
    },
    Variable(focus_view_reading::VariableMenu),
}

pub struct PendingWrite {
    library: Arc<Library>,
    id: NoteId,
    text: String,
    expected: Option<SystemTime>,
    loose: Option<std::path::PathBuf>,
}

impl PendingWrite {
    pub fn write(self) -> Result<(), VaultError> {
        let written = match &self.loose {
            Some(path) => crate::loose::write(path, &self.text, self.expected),
            None => self.library.write(self.id, &self.text, self.expected),
        };
        match written {
            Ok(_) => Ok(()),
            Err(VaultError::Conflict(path)) => {
                let note = match &self.loose {
                    Some(loose) => loose.clone(),
                    None => self
                        .library
                        .note(self.id)
                        .map_or(path, |meta| meta.path.to_path_buf()),
                };
                conflict::write_copy(
                    &self.library,
                    self.loose.as_deref(),
                    &note,
                    &self.text,
                    &focus_i18n::t("ui-conflict-copy"),
                )
                .map(|_| ())
            }
            Err(error) => Err(error),
        }
    }
}

pub struct EditorOptions {
    pub view: ViewMode,
    pub line_numbers: bool,
    pub typesetting: Typesetting,
    pub autosave: Duration,
    pub wrap: bool,
    pub tab_width: usize,
    pub font: crate::editing::EditorFont,
    pub bounds: crate::editing::EditorBounds,
    pub edit_in_place: bool,
}

pub struct NoteEditor {
    pub id: NoteId,
    pub meta: Arc<NoteMeta>,
    pub input: Entity<InputState>,
    pub reading: Entity<ReadingView>,
    pub load: Load,
    pub dirty: bool,
    pub view: ViewMode,
    pub editing_view: ViewMode,
    pub spotted: bool,
    pub conflict: bool,
    theirs: Option<Arc<conflict::Theirs>>,
    conflict_task: Option<Task<()>>,
    pub copies: usize,
    copies_task: Option<Task<()>>,
    pub(crate) library: Arc<Library>,
    pub loose: Option<std::path::PathBuf>,
    pub live: Option<crate::live::LiveBlock>,
    pub(crate) edit_in_place: bool,
    pub(crate) block_handles: bool,
    pub block_font: crate::editing::EditorFont,
    pub(crate) bounds: crate::editing::EditorBounds,
    sheet_width: Option<f32>,
    loaded: String,
    disk_modified: Option<SystemTime>,
    pub autosave: Duration,
    save_task: Option<Task<()>>,
    parse_task: Option<Task<()>>,
    opened: std::time::Instant,
    first_build: bool,
    pub open_phases: Option<[f32; 3]>,
    decoration: Option<Arc<dyn focus_view_reading::CodeDecoration>>,
    code_variables: Option<Arc<focus_view_reading::CodeVariables>>,
    pub parked: bool,
    pub follower: bool,
    unsaved: Option<String>,
    parked_top: Option<usize>,
    crlf: bool,
    pub(crate) parsing: gpui_component::Rope,
    read_ms: f32,
    filled_ms: f32,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for NoteEditor {}

fn differs(rope: &gpui_component::Rope, text: &str) -> bool {
    rope.len() != text.len() || *rope != *text
}

impl NoteEditor {
    pub fn new(
        meta: Arc<NoteMeta>,
        library: Arc<Library>,
        options: EditorOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let EditorOptions {
            view,
            line_numbers,
            typesetting,
            autosave,
            wrap,
            tab_width,
            font,
            bounds,
            edit_in_place,
        } = options;
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor(focus_kit::theme::MARKDOWN)
                .tab_size(gpui_component::input::TabSize {
                    tab_size: tab_width,
                    hard_tabs: false,
                })
                .alignment(gpui_component::input::TextAlignment::ByParagraph)
                .line_number(line_numbers)
                .soft_wrap(wrap)
                .searchable(true)
        });
        let reading = cx.new(|cx| {
            let mut reading = ReadingView::new(typesetting, cx);
            reading.set_live(view == ViewMode::Live, cx);
            reading.set_in_place(edit_in_place.then(|| input.clone()), cx);
            reading.set_note_field(input.clone());
            reading
        });
        let subscriptions = vec![cx.subscribe(
            &input,
            |this: &mut NoteEditor, input, event: &InputEvent, cx| {
                if !matches!(event, InputEvent::Change)
                    || !matches!(this.load, Load::Ready)
                    || this.parked
                {
                    return;
                }
                let text = input.read(cx).text();
                if text.len() == this.parsing.len() && *text == this.parsing {
                    return;
                }
                if this.live.is_none() {
                    cx.emit(EditorEvent::Edited);
                }
                if this.spotted {
                    this.clear_spot(cx);
                }
                let dirty = if this.crlf {
                    this.on_disk(&input.read(cx).value()) != this.loaded
                } else {
                    differs(input.read(cx).text(), &this.loaded)
                };
                if dirty != this.dirty {
                    this.dirty = dirty;
                    cx.emit(EditorEvent::Changed);
                }
                if this.live.is_some() {
                    this.live_reparse(cx);
                } else if this.caret_in_place(cx) || this.reading.read(cx).caret_shown() {
                    this.in_place_changed(cx);
                } else {
                    this.schedule_parse(REPARSE, cx);
                }
                if dirty {
                    this.schedule_save(cx);
                }
                cx.notify();
            },
        )];
        let mut subscriptions = subscriptions;
        subscriptions.extend(Self::listen_reading(&reading, window, cx));
        subscriptions.push(Self::keep_field_unfocused(window, cx));
        NoteEditor {
            id: meta.id,
            meta,
            input,
            reading,
            load: Load::Loading,
            dirty: false,
            view,
            editing_view: if view == ViewMode::Source {
                ViewMode::Source
            } else {
                ViewMode::Live
            },
            conflict: false,
            theirs: None,
            conflict_task: None,
            copies: 0,
            copies_task: None,
            library,
            loose: None,
            live: None,
            edit_in_place,
            block_handles: true,
            block_font: font,
            bounds,
            sheet_width: None,
            crlf: false,
            loaded: String::new(),
            disk_modified: None,
            autosave,
            save_task: None,
            parse_task: None,
            opened: std::time::Instant::now(),
            first_build: true,
            open_phases: None,
            decoration: None,
            code_variables: None,
            parked: false,
            spotted: false,
            follower: false,
            unsaved: None,
            parked_top: None,
            parsing: gpui_component::Rope::new(),
            read_ms: 0.,
            filled_ms: 0.,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_code_variables(
        &mut self,
        variables: Option<Arc<focus_view_reading::CodeVariables>>,
        cx: &mut Context<Self>,
    ) {
        self.code_variables = variables.clone();
        self.reading
            .update(cx, |reading, cx| reading.set_code_variables(variables, cx));
    }

    fn insert_variable(
        &mut self,
        line: u32,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let token = format!("${name}");
        if let Some(block) = &self.live
            && block.lines.contains(&line)
        {
            block.input.clone().update(cx, |input, cx| {
                input.replace_text_in_range(None, &token, window, cx)
            });
            return;
        }
        self.close_block(true, window, cx);
        self.input.update(cx, |input, cx| {
            let text = input.text().clone();
            let row = line as usize + 1;
            if row >= text.lines_len() {
                return;
            }
            let content = text.slice_line(row).to_string();
            let content = content.trim_end_matches(['\n', '\r']);
            let at = text.line_start_offset(row) + content.len();
            let spaced = if content.is_empty() || content.ends_with(' ') {
                token.clone()
            } else {
                format!(" {token}")
            };
            let offset = text.offset_to_offset_utf16(at);
            input.replace_text_in_range(Some(offset..offset), &spaced, window, cx);
        });
    }

    pub fn set_code_decoration(
        &mut self,
        decoration: Option<Arc<dyn focus_view_reading::CodeDecoration>>,
        cx: &mut Context<Self>,
    ) {
        self.decoration = decoration.clone();
        self.reading.update(cx, |reading, cx| {
            reading.set_code_decoration(decoration, cx)
        });
    }

    fn write_language(
        &mut self,
        line: u32,
        lang: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.parked {
            return;
        }
        self.close_block(true, window, cx);
        self.input.update(cx, |input, cx| {
            let text = input.text().clone();
            let row = line as usize;
            if row >= text.lines_len() {
                return;
            }
            let content = text.slice_line(row).to_string();
            let indent = content.len() - content.trim_start().len();
            let fence: String = content[indent..]
                .chars()
                .take_while(|c| *c == '`' || *c == '~')
                .collect();
            if fence.len() < 3 {
                return;
            }
            let start = text.line_start_offset(row) + indent + fence.len();
            let end = text.line_start_offset(row) + content.trim_end().len();
            let range =
                text.offset_to_offset_utf16(start)..text.offset_to_offset_utf16(end.max(start));
            input.replace_text_in_range(Some(range), lang, window, cx);
        });
    }

    fn listen_reading(
        reading: &Entity<ReadingView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let chosen = cx.subscribe_in(
            reading,
            window,
            |this: &mut NoteEditor, _, event: &focus_view_reading::LanguageChosen, window, cx| {
                let lang = event.lang.clone();
                this.write_language(event.line, &lang, window, cx);
            },
        );
        let live = cx.subscribe_in(
            reading,
            window,
            |this: &mut NoteEditor, _, open: &focus_view_reading::LiveOpen, window, cx| {
                this.on_live_open(open, window, cx)
            },
        );
        let variable = cx.subscribe_in(
            reading,
            window,
            |this: &mut NoteEditor, _, chosen: &focus_view_reading::VariableChosen, window, cx| {
                this.insert_variable(chosen.line, &chosen.name, window, cx)
            },
        );
        let menu = cx.subscribe(
            reading,
            |_: &mut NoteEditor, _, menu: &focus_view_reading::VariableMenu, cx| {
                cx.emit(EditorEvent::Variable(menu.clone()))
            },
        );
        let crumb = cx.subscribe(
            reading,
            |this: &mut NoteEditor, _, clicked: &focus_view_reading::CrumbClicked, cx| {
                if this.loose.is_none() {
                    cx.emit(EditorEvent::Crumb {
                        folder: clicked.folder.clone(),
                        at: clicked.at,
                    });
                }
            },
        );
        let pages = cx.subscribe(
            reading,
            |_: &mut NoteEditor, _, page: &focus_view_reading::view::LinkPage, cx| {
                cx.emit(EditorEvent::LinkPage(page.0))
            },
        );
        let events = cx.subscribe_in(
            reading,
            window,
            |this: &mut NoteEditor, _, event: &ReadingEvent, window, cx| match event {
                ReadingEvent::OpenLink { goal, new_tab } => cx.emit(EditorEvent::OpenLink {
                    goal: goal.clone(),
                    new_tab: *new_tab,
                }),
                ReadingEvent::Zoom(source) => cx.emit(EditorEvent::Zoom(source.clone())),
                ReadingEvent::ToggleTask(line) => this.toggle_task(*line, window, cx),
                ReadingEvent::StartWriting => this.set_view(ViewMode::Live, window, cx),
                ReadingEvent::Hover { goal, position } => cx.emit(EditorEvent::Hover {
                    goal: goal.clone(),
                    position: *position,
                }),
            },
        );
        let toggled = cx.subscribe_in(
            reading,
            window,
            |this: &mut NoteEditor,
             _,
             toggled: &focus_view_reading::PropertyToggled,
             window,
             cx| { this.toggle_property(toggled, window, cx) },
        );
        let hit = cx.subscribe(
            reading,
            |_, _, hit: &focus_view_reading::QueryHitClicked, cx| {
                cx.emit(EditorEvent::OpenAt {
                    note: hit.note,
                    line: hit.line,
                    new_tab: hit.new_tab,
                })
            },
        );
        vec![
            events, chosen, live, crumb, variable, menu, toggled, hit, pages,
        ]
    }

    pub fn source(&self, cx: &App) -> String {
        match &self.unsaved {
            Some(text) if self.parked => text.clone(),
            _ if self.parked => self.loaded.clone(),
            _ => self.input.read(cx).value().to_string(),
        }
    }

    pub fn park(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.parked || !matches!(self.load, Load::Ready) {
            return;
        }
        if self.dirty {
            self.unsaved = Some(self.input.read(cx).value().to_string());
        }
        self.close_block(true, window, cx);
        self.parked = true;
        self.parked_top = self.reading.read(cx).top_item().filter(|item| *item > 0);
        self.parse_task = None;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        let typesetting = self.reading.read(cx).typesetting.clone();
        let sheet_width = self.sheet_width;
        let live = self.view == ViewMode::Live;
        let decoration = self.decoration.clone();
        let variables = self.code_variables.clone();
        let in_place = self.edit_in_place.then(|| self.input.clone());
        let handles = self.block_handles;
        self.reading = cx.new(|cx| {
            let mut reading = ReadingView::new(typesetting, cx);
            reading.set_code_decoration(decoration, cx);
            reading.set_code_variables(variables, cx);
            reading.set_live(live, cx);
            reading.set_in_place(in_place, cx);
            reading.set_note_field(self.input.clone());
            reading.set_sheet_width(sheet_width, cx);
            reading.set_block_handles(handles, cx);
            reading
        });
        let subscriptions = Self::listen_reading(&self.reading, window, cx);
        self._subscriptions.truncate(1);
        self._subscriptions.extend(subscriptions);
    }

    pub fn unpark(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Task<()>> {
        if !self.parked {
            return None;
        }
        self.parked = false;
        self.first_build = false;
        match self.unsaved.take() {
            Some(text) => {
                self.dirty = text != self.loaded;
                self.input
                    .update(cx, |input, cx| input.set_value(text, window, cx));
                self.schedule_parse(Duration::ZERO, cx);
                cx.emit(EditorEvent::Changed);
                None
            }
            None => Some(self.load(window, cx)),
        }
    }

    pub fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        let library = self.library.clone();
        let id = self.id;
        let loose = self.loose.clone();
        let task = cx.background_spawn(async move {
            match loose {
                Some(path) => crate::loose::read(&path),
                None => {
                    let modified = library.note(id).map(|m| m.modified);
                    (library.read(id), modified)
                }
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let (result, modified) = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.fill(result, modified, window, cx)
            });
        })
    }

    fn fill(
        &mut self,
        result: Result<String, VaultError>,
        modified: Option<SystemTime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(text) => {
                self.close_block(false, window, cx);
                self.read_ms = self.opened.elapsed().as_secs_f32() * 1000.;
                self.input
                    .update(cx, |input, cx| input.set_value(text.clone(), window, cx));
                self.filled_ms = self.opened.elapsed().as_secs_f32() * 1000.;
                self.crlf = text.contains("\r\n");
                self.loaded = text;
                self.disk_modified = modified;
                self.dirty = false;
                self.conflict = false;
                self.theirs = None;
                self.load = Load::Ready;
                if let Some(meta) = self.library.note(self.id) {
                    self.meta = meta;
                }
                self.schedule_parse(Duration::ZERO, cx);
                self.scan_copies(cx);
            }
            Err(error) => self.load = Load::Failed(error.to_string()),
        }
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    pub fn schedule_parse(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().clone();
        self.parsing = text.clone();
        let resolver = self.resolver();
        let meta = self.sheet_meta();
        let reading = self.reading.clone();
        self.parse_task = Some(cx.spawn(async move |this, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            let built = cx
                .background_spawn(async move {
                    Arc::new(model::build_text(
                        &text.to_string(),
                        Some(&meta),
                        resolver.as_ref(),
                        None,
                    ))
                })
                .await;
            let _ = reading.update(cx, |reading, cx| reading.set_built(built, cx));
            let _ = this.update(cx, |this, cx| {
                if let Some(item) = this.parked_top.take() {
                    this.reading
                        .update(cx, |reading, _| reading.scroll_to_item(item));
                }
            });
            let _ = this.update(cx, |this, cx| {
                if this.first_build {
                    this.first_build = false;
                    this.open_phases = Some([
                        this.read_ms,
                        this.filled_ms,
                        this.opened.elapsed().as_secs_f32() * 1000.,
                    ]);
                }
                this.parse_task = None;
                cx.emit(EditorEvent::Changed);
            });
        }));
    }

    pub fn opened(&self) -> std::time::Instant {
        self.opened
    }

    pub fn busy(&self) -> bool {
        self.parse_task.is_some() || self.conflict_task.is_some() || self.copies_task.is_some()
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        if self.conflict || self.follower || self.conflict_task.is_some() {
            return;
        }
        let delay = self.autosave;
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                if this.conflict_task.is_none() {
                    drop(this.save(false, cx));
                }
            });
        }));
    }

    fn on_disk(&self, text: &str) -> String {
        crate::live::with_endings(text, self.crlf)
    }

    pub fn discard(&mut self) {
        self.dirty = false;
        self.save_task = None;
    }

    pub fn pending_write(&mut self, cx: &mut Context<Self>) -> Option<PendingWrite> {
        if !self.dirty || !matches!(self.load, Load::Ready) {
            return None;
        }
        self.save_task = None;
        Some(PendingWrite {
            library: self.library.clone(),
            id: self.id,
            text: self.on_disk(&self.source(cx)),
            expected: self.disk_modified,
            loose: self.loose.clone(),
        })
    }

    pub fn save(
        &mut self,
        force: bool,
        cx: &mut Context<Self>,
    ) -> impl Future<Output = ()> + use<> {
        let text = self.on_disk(&self.source(cx));
        let library = self.library.clone();
        let id = self.id;
        let expected = if force { None } else { self.disk_modified };
        let loose = self.loose.clone();
        let task = cx.background_spawn({
            let text = text.clone();
            async move {
                match loose {
                    Some(path) => crate::loose::write(&path, &text, expected),
                    None => library.write(id, &text, expected),
                }
            }
        });
        let (ended, finished) = smol::channel::bounded::<()>(1);
        self.save_task = Some(cx.spawn(async move |this, cx| {
            let _ended = ended;
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(modified) => {
                        this.disk_modified = Some(modified);
                        this.loaded = text;
                        this.conflict = false;
                        this.theirs = None;
                        this.dirty = this.on_disk(&this.source(cx)) != this.loaded;
                    }
                    Err(VaultError::Conflict(_)) if !this.conflict => this.check_conflict(cx),
                    Err(VaultError::Conflict(_)) => {}
                    Err(error) => cx.emit(EditorEvent::Failed(focus_i18n::t_args(
                        "reading-save-failed",
                        &[("error", crate::errors::vault(&error).as_str().into())],
                    ))),
                }
                cx.emit(EditorEvent::Changed);
                cx.notify();
            });
        }));
        async move {
            let _ = finished.recv().await;
        }
    }

    pub fn external_change(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<()>> {
        let modified = match &self.loose {
            Some(path) => std::fs::metadata(path).and_then(|m| m.modified()).ok(),
            None => {
                let meta = self.library.note(self.id)?;
                let modified = meta.modified;
                self.meta = meta;
                Some(modified)
            }
        };
        if modified == self.disk_modified {
            return None;
        }
        self.disk_changed(window, cx)
    }

    fn toggle_task(&mut self, line: u32, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dirty && !self.conflict && self.loose.is_none() {
            let library = self.library.clone();
            let id = self.id;
            let task = cx.background_spawn(async move { library.toggle_task(id, line) });
            cx.spawn(async move |this, cx| {
                if let Err(error) = task.await {
                    let _ = this.update(cx, |_, cx| {
                        cx.emit(EditorEvent::Failed(focus_i18n::t_args(
                            "reading-save-failed",
                            &[("error", crate::errors::vault(&error).as_str().into())],
                        )))
                    });
                }
            })
            .detach();
            return;
        }
        self.close_block(true, window, cx);
        self.input.update(cx, |input, cx| {
            let text = input.text().clone();
            let row = line as usize;
            if row >= text.lines_len() {
                return;
            }
            let start = text.line_start_offset(row);
            let content = text.slice_line(row).to_string();
            let Some(at) = ["[ ]", "[x]", "[X]"]
                .iter()
                .filter_map(|m| content.find(m))
                .min()
            else {
                return;
            };
            let mark = if &content[at..at + 3] == "[ ]" {
                "[x]"
            } else {
                "[ ]"
            };
            let range = text.offset_to_offset_utf16(start + at)
                ..text.offset_to_offset_utf16(start + at + 3);
            input.replace_text_in_range(Some(range), mark, window, cx);
        });
    }

    pub fn set_sheet_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.sheet_width = Some(width);
        self.reading
            .update(cx, |reading, cx| reading.set_sheet_width(Some(width), cx));
    }

    pub fn set_line_numbers(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.input
            .update(cx, |input, cx| input.set_line_number(on, window, cx));
    }

    pub fn cursor(&self, cx: &App) -> Position {
        match &self.live {
            Some(block) => {
                let caret = block.input.read(cx).cursor_position();
                Position {
                    line: block.lines.start + caret.line,
                    character: caret.character,
                }
            }
            None => self.input.read(cx).cursor_position(),
        }
    }

    pub fn scroll_by(&mut self, delta: gpui::Pixels, cx: &mut Context<Self>) {
        match self.view {
            ViewMode::Source => self
                .input
                .update(cx, |input, cx| input.scroll_by(delta, cx)),
            ViewMode::Live | ViewMode::Reading => self.reading.update(cx, |reading, cx| {
                reading.scroll_by(delta);
                cx.notify();
            }),
        }
    }

    pub fn line_count(&self, cx: &App) -> usize {
        self.input.read(cx).text().lines_len()
    }

    pub fn selection(&self, window: &mut Window, cx: &mut Context<Self>) -> String {
        self.typing_input().update(cx, |input, cx| {
            let Some(selection) = input.selected_text_range(true, window, cx) else {
                return String::new();
            };
            if selection.range.is_empty() {
                return String::new();
            }
            input
                .text_for_range(selection.range, &mut None, window, cx)
                .unwrap_or_default()
        })
    }

    pub fn insert_at_cursor(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.caret_in_view(cx) {
            let flat;
            let text = if self.reading.read(cx).caret_in_property(cx) {
                flat = text.lines().collect::<Vec<_>>().join(" ");
                flat.as_str()
            } else {
                text
            };
            self.reading
                .update(cx, |reading, cx| reading.insert_in_place(text, window, cx));
            return;
        }
        self.typing_input().update(cx, |input, cx| {
            input.replace_text_in_range(None, text, window, cx)
        });
    }

    pub fn insert_block(&mut self, block: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.typing_input().update(cx, |input, cx| {
            let text = input.text().to_string();
            let frontmatter_end = text
                .strip_prefix("---\n")
                .and_then(|rest| rest.find("\n---").map(|i| 4 + i + 4))
                .map(|end| text[end..].find('\n').map_or(text.len(), |n| end + n + 1));
            let cursor = input.cursor();
            let at = match frontmatter_end {
                Some(end) if cursor < end => end.min(text.len()),
                _ => cursor,
            };
            let before = if at == 0 || text[..at].ends_with('\n') {
                ""
            } else {
                "\n"
            };
            let after = if text[at..].starts_with('\n') {
                ""
            } else {
                "\n"
            };
            let range = input.text().offset_to_offset_utf16(at);
            input.replace_text_in_range(
                Some(range..range),
                &format!("{before}{block}{after}"),
                window,
                cx,
            );
        });
    }

    pub fn append(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.close_block(true, window, cx);
        self.input.update(cx, |input, cx| {
            let end = input.text().len();
            let end = input.text().offset_to_offset_utf16(end);
            input.replace_text_in_range(Some(end..end), text, window, cx);
        });
    }

    pub fn insert_after_selection(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.typing_input().update(cx, |input, cx| {
            let end = input
                .selected_text_range(true, window, cx)
                .map_or(0, |s| s.range.end);
            input.replace_text_in_range(Some(end..end), text, window, cx);
        });
    }

    pub fn replace_once(
        &mut self,
        original: &str,
        replacement: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.close_block(true, window, cx);
        self.input.update(cx, |input, cx| {
            let text = input.text().to_string();
            let Some(at) = text.find(original) else {
                return false;
            };
            let rope = input.text().clone();
            let range =
                rope.offset_to_offset_utf16(at)..rope.offset_to_offset_utf16(at + original.len());
            input.replace_text_in_range(Some(range), replacement, window, cx);
            true
        })
    }

    pub fn go_to_line(&mut self, line: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.set_cursor_position(Position { line, character: 0 }, window, cx)
        });
    }

    pub fn arrive(
        &mut self,
        line: u32,
        found: Option<gpui::SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reading
            .update(cx, |reading, _| reading.scroll_to_line(line));
        if self.view != ViewMode::Reading {
            self.go_to_line(line, window, cx);
        }
        match found {
            Some(text) if !text.is_empty() => {
                self.input.update(cx, |input, cx| {
                    let from = input.text().line_start_offset(line as usize);
                    input.find_at(&text, false, from, window, cx)
                });
                self.spotted = true;
                self.reading.update(cx, |_, cx| cx.notify());
                cx.notify();
            }
            _ => self.clear_spot(cx),
        }
    }

    pub fn clear_spot(&mut self, cx: &mut Context<Self>) {
        if !self.spotted {
            return;
        }
        self.spotted = false;
        self.input.update(cx, |input, cx| input.find_close(cx));
        self.reading.update(cx, |_, cx| cx.notify());
        cx.notify();
    }
}

pub struct Shared {
    text: String,
    loaded: String,
    disk_modified: Option<SystemTime>,
    dirty: bool,
    conflict: bool,
    theirs: Option<Arc<conflict::Theirs>>,
}

impl NoteEditor {
    pub fn shared(&self, cx: &App) -> Option<Shared> {
        if !matches!(self.load, Load::Ready) {
            return None;
        }
        Some(Shared {
            text: self.source(cx),
            loaded: self.loaded.clone(),
            disk_modified: self.disk_modified,
            dirty: self.dirty,
            conflict: self.conflict,
            theirs: self.theirs.clone(),
        })
    }

    pub fn behind(&self, shared: &Shared, cx: &App) -> bool {
        if shared.disk_modified < self.disk_modified {
            return false;
        }
        let text_differs = if self.parked {
            self.source(cx) != shared.text
        } else {
            differs(self.input.read(cx).text(), &shared.text)
        };
        self.disk_modified != shared.disk_modified
            || self.dirty != shared.dirty
            || self.conflict != shared.conflict
            || (takes_theirs(self.follower, shared) && !same_theirs(&self.theirs, &shared.theirs))
            || !matches!(self.load, Load::Ready)
            || text_differs
    }

    pub fn follow(&mut self, shared: &Shared, window: &mut Window, cx: &mut Context<Self>) {
        self.loaded = shared.loaded.clone();
        self.disk_modified = shared.disk_modified;
        self.dirty = shared.dirty;
        self.conflict = shared.conflict;
        if takes_theirs(self.follower, shared) {
            self.theirs = shared.theirs.clone();
        }
        self.crlf = shared.loaded.contains("\r\n");
        if self.parked {
            self.unsaved = shared.dirty.then(|| shared.text.clone());
        } else if !matches!(self.load, Load::Ready) {
            self.input.update(cx, |input, cx| {
                input.set_value(shared.text.clone(), window, cx)
            });
            self.load = Load::Ready;
            self.schedule_parse(Duration::ZERO, cx);
        } else if differs(self.input.read(cx).text(), &shared.text) {
            self.close_block(false, window, cx);
            self.input.update(cx, |input, cx| {
                replace_changed_span(input, &shared.text, window, cx)
            });
            self.schedule_parse(REPARSE, cx);
        }
        if self.dirty {
            self.schedule_save(cx);
        }
        cx.notify();
    }

    pub fn lead(&mut self, cx: &mut Context<Self>) {
        self.follower = false;
        if self.conflict && self.theirs.is_none() && self.conflict_task.is_none() {
            self.check_conflict(cx);
        }
        if self.dirty {
            self.schedule_save(cx);
        }
    }
}

fn takes_theirs(follower: bool, shared: &Shared) -> bool {
    follower || !shared.conflict || shared.theirs.is_some()
}

fn same_theirs(a: &Option<Arc<conflict::Theirs>>, b: &Option<Arc<conflict::Theirs>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn replace_changed_span(
    input: &mut InputState,
    new: &str,
    window: &mut Window,
    cx: &mut Context<InputState>,
) {
    let old = input.value().to_string();
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let mut tail = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(old.len() - start)
        .min(new.len() - start);
    while !old.is_char_boundary(old.len() - tail) || !new.is_char_boundary(new.len() - tail) {
        tail -= 1;
    }
    let (old_end, new_end) = (old.len() - tail, new.len() - tail);
    let rope = input.text().clone();
    let head = input.cursor();
    let anchor = input
        .selected_text_range(true, window, cx)
        .map(|s| {
            let (from, to) = (
                rope.offset_utf16_to_offset(s.range.start),
                rope.offset_utf16_to_offset(s.range.end),
            );
            if head == from { to } else { from }
        })
        .unwrap_or(head);
    let range = rope.offset_to_offset_utf16(start)..rope.offset_to_offset_utf16(old_end);
    input.replace_text_in_range(Some(range), &new[start..new_end], window, cx);
    let kept = |offset: usize| {
        if offset <= start {
            offset
        } else if offset >= old_end {
            offset - old_end + new_end
        } else {
            new_end
        }
    };
    input.set_selection(kept(anchor), kept(head), cx);
}

#[cfg(test)]
mod syntax_tests {
    use std::ops::Range;
    use std::time::Duration;

    use gpui::HighlightStyle;
    use gpui_component::Rope;
    use gpui_component::highlighter::{HighlightTheme, SyntaxHighlighter};

    const LANGUAGE: &str = "json";

    fn json_text(items: usize) -> String {
        let body: Vec<String> = (0..items)
            .map(|i| format!("  {{\"id\": {i}, \"name\": \"item {i}\", \"on\": true}}"))
            .collect();
        format!("[\n{}\n]\n", body.join(",\n"))
    }

    fn styles(highlighter: &SyntaxHighlighter, text: &Rope) -> Vec<(Range<usize>, HighlightStyle)> {
        highlighter.styles(&(0..text.len()), &HighlightTheme::default_dark())
    }

    fn parsed_at_once(text: &Rope) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut highlighter = SyntaxHighlighter::new(LANGUAGE);
        highlighter.update(None, text);
        styles(&highlighter, text)
    }

    fn settle(highlighter: &mut SyntaxHighlighter) {
        while let Some(job) = highlighter.parse_within(Duration::ZERO) {
            highlighter.finish(job.run());
        }
    }

    fn replace(
        highlighter: &mut SyntaxHighlighter,
        text: &Rope,
        range: Range<usize>,
        inserted: &str,
    ) -> Rope {
        let mut edited = text.clone();
        edited.remove(range.clone());
        edited.insert(range.start, inserted);
        highlighter.edit(range, inserted.len(), &edited);
        edited
    }

    #[test]
    fn edits_made_while_a_parse_runs_elsewhere_are_not_lost() {
        let text = Rope::from(json_text(4000).as_str());
        let mut highlighter = SyntaxHighlighter::new(LANGUAGE);
        highlighter.set_text(&text);
        let job = highlighter
            .parse_within(Duration::ZERO)
            .expect("no time to parse here: a job");
        assert!(
            highlighter.parse_within(Duration::ZERO).is_none(),
            "one job at a time"
        );
        let text = replace(&mut highlighter, &text, 2..2, "  {\"id\": -1},\n");
        let line = text.to_string().find("  {\"id\": 7,").expect("item 7");
        let end = line + text.to_string()[line..].find('\n').expect("its line end") + 1;
        let text = replace(&mut highlighter, &text, line..end, "");
        assert!(
            highlighter.finish(job.run()),
            "the edits ask for another parse"
        );
        settle(&mut highlighter);
        assert_eq!(styles(&highlighter, &text), parsed_at_once(&text));
    }

    #[test]
    fn an_edit_moves_the_colors_before_the_parse() {
        let text = Rope::from(json_text(40).as_str());
        let mut highlighter = SyntaxHighlighter::new(LANGUAGE);
        highlighter.update(None, &text);
        let before = highlighter.version();
        let text = replace(&mut highlighter, &text, 0..0, "    ");
        assert!(
            highlighter.version() > before,
            "styles kept for drawn lines are stale"
        );
        assert_eq!(
            styles(&highlighter, &text),
            parsed_at_once(&text),
            "spaces before the array: every node moved, nothing parsed yet"
        );
    }

    #[test]
    fn a_new_highlighter_never_takes_the_version_of_another() {
        let first = Rope::from("# Heading of the note\n\nPlain words.\n");
        let second = Rope::from("Plain first line now.\n# Heading of the note\n");
        let mut shown = SyntaxHighlighter::new(LANGUAGE);
        shown.set_text(&first);
        settle(&mut shown);
        let mut reloaded = SyntaxHighlighter::new(LANGUAGE);
        reloaded.set_text(&second);
        assert_ne!(reloaded.version(), shown.version(), "text set");
        settle(&mut reloaded);
        assert_ne!(reloaded.version(), shown.version(), "text parsed");
        let mut updated = SyntaxHighlighter::new(LANGUAGE);
        updated.update(None, &second);
        assert_ne!(updated.version(), shown.version(), "text parsed at once");
    }

    #[test]
    fn the_parse_of_a_replaced_text_is_dropped() {
        let text = Rope::from(json_text(4000).as_str());
        let mut highlighter = SyntaxHighlighter::new(LANGUAGE);
        highlighter.set_text(&text);
        let job = highlighter.parse_within(Duration::ZERO).expect("a job");
        let other = Rope::from("{\"other\": true}\n");
        highlighter.set_text(&other);
        assert!(!highlighter.finish(job.run()));
        settle(&mut highlighter);
        assert_eq!(styles(&highlighter, &other), parsed_at_once(&other));
    }
}
