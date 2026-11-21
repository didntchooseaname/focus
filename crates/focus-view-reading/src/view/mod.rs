use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use focus_kit::Flow as _;
use focus_kit::{ActiveTokens, Tokens, Type, size};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, EventEmitter, FocusHandle, FontWeight,
    InteractiveElement, IntoElement, KeyDownEvent, ListAlignment, ListOffset, ListState,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point,
    Render, StatefulInteractiveElement, Styled, Window, actions, div, list, prelude::FluentBuilder,
    px,
};

use super::media::{Media, media};
use super::model::{Built, LinkGoal, Node, Rich, texts_of};
use super::text::{Base, Registry, RichText, TextKey, runs};
use crate::CodeDecoration;

mod blocks;
mod handles;
mod inplace;
mod live;
mod pdf_embed;
mod picker;
mod properties;
pub(crate) mod query;
mod table;
mod tabs;
mod viewer;
mod walk;

pub(crate) use inplace::bindings as in_place_bindings;
pub use live::{LiveBlock, LiveKind, LiveOpen};
pub use properties::{PropertyToggled, property_checkbox_edit, set_property_type_host};
pub use viewer::LinkPage;

const CODE_PART: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Row {
    item: usize,
    part: u32,
    parts: u32,
    editor: bool,
}

const COPIED_FOR: Duration = Duration::from_millis(1500);
const REVEAL_INNER_FRAMES: u8 = 8;
const AUTOSCROLL_RAMP: f32 = 80.;
const AUTOSCROLL_SLOW: f32 = 125.;
const AUTOSCROLL_FAST: f32 = 1625.;
const AUTOSCROLL_MAX_STEP: f32 = 0.1;

actions!(
    focus_reading,
    [
        CopySelection,
        SelectAllNote,
        PageUp,
        PageDown,
        NoteStart,
        NoteEnd,
        FocusNext,
        FocusPrevious,
        ActivateFocused,
        FollowLinkAtCaret,
        ToggleAtCaret,
        LeaveFocus
    ]
);

pub const CONTEXT: &str = "FocusReading";

pub enum ReadingEvent {
    OpenLink {
        goal: LinkGoal,
        new_tab: bool,
    },
    Hover {
        goal: Option<LinkGoal>,
        position: Point<Pixels>,
    },
    ToggleTask(u32),
    Zoom(String),
    StartWriting,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LanguageChosen {
    pub line: u32,
    pub lang: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariableChosen {
    pub line: u32,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VariableMenu {
    Set { name: String, value: String },
    NewEngagement,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CrumbClicked {
    pub folder: std::path::PathBuf,
    pub at: Point<Pixels>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoldsChanged;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FoldState {
    pub sections: Vec<String>,
    pub collapsibles: Vec<(u32, u32, bool)>,
    pub tabs: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum ScriptMouse {
    Move(Point<Pixels>),
    Down {
        at: Point<Pixels>,
        count: usize,
        shift: bool,
    },
    Press {
        at: Point<Pixels>,
        count: usize,
        modifiers: gpui::Modifiers,
    },
    Drag(Point<Pixels>),
    Up(Point<Pixels>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Grain {
    Char,
    Word,
    Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Caret {
    key: TextKey,
    offset: usize,
}

#[derive(Clone)]
struct DiagramHit {
    bounds: gpui::Bounds<Pixels>,
    id: (usize, usize),
    goal: LinkGoal,
}

#[derive(Clone)]
enum Zone {
    Image {
        image: Arc<gpui::RenderImage>,
        width: f32,
        height: f32,
    },
    PdfPage {
        relative: std::path::PathBuf,
        page: usize,
    },
    Task(u32),
    Tag(String),
    Fold(usize),
    Crumb(usize),
    CalloutIcon {
        item: usize,
        callout: u32,
    },
    PropName(usize),
    PropCheck(usize),
    PropField(usize),
    QueryHit {
        note: focus_core::NoteId,
        line: Option<u32>,
    },
}

type Zones = Rc<RefCell<Vec<(gpui::Bounds<Pixels>, Zone)>>>;

type BlockKey = (usize, u32);

struct DrawnCode {
    line: u32,
    part: u32,
    shown: String,
    copied: String,
}

struct DecoratedCode {
    raw: Rich,
    resolved: Rich,
    revealed: Option<Rich>,
}

impl DecoratedCode {
    fn shown(&self, clear: bool) -> &Rich {
        self.revealed
            .as_ref()
            .filter(|_| clear)
            .unwrap_or(&self.resolved)
    }
}

type PendingText = (
    String,
    Option<Arc<focus_core::NoteMeta>>,
    Arc<dyn crate::Resolver>,
);
type InnerBounds = Rc<RefCell<HashMap<usize, Vec<(Range<u32>, gpui::Bounds<Pixels>)>>>>;
type TextSource = (Option<Arc<focus_core::NoteMeta>>, Arc<dyn crate::Resolver>);

#[derive(Clone, PartialEq)]
pub struct Typesetting {
    pub base: f32,
    pub measure: f32,
    pub leading: f32,
    pub tinted: bool,
    pub text_font: gpui::SharedString,
    pub code_font: gpui::SharedString,
}

impl Typesetting {
    pub fn compact(measure: f32) -> Typesetting {
        Typesetting {
            base: 13.,
            measure,
            leading: 20. / 13.,
            tinted: true,
            text_font: focus_kit::typography::SANS.into(),
            code_font: focus_kit::typography::MONO.into(),
        }
    }
}

pub struct ReadingView {
    built: Option<Arc<Built>>,
    list: ListState,
    visible: Vec<usize>,
    rows: Vec<Row>,
    folded: HashSet<usize>,
    callouts: HashMap<(usize, u32), bool>,
    pending_folds: Option<FoldState>,
    tab_titles: Vec<String>,
    tab_index: HashMap<(usize, u32), usize>,
    tab_focus: RefCell<HashMap<(usize, u32), FocusHandle>>,
    inner_bounds: InnerBounds,
    editor_bounds: Rc<std::cell::Cell<Option<gpui::Bounds<Pixels>>>>,
    parked: bool,
    editor_height: Rc<std::cell::Cell<f32>>,
    open_floor: f32,
    remote_allowed: bool,
    live: bool,
    open: Option<live::OpenBlock>,
    pending_open_lines: Option<Range<u32>>,
    props_bounds: Rc<std::cell::Cell<Option<gpui::Bounds<Pixels>>>>,
    down_here: bool,
    registry: Registry,
    anchor: Option<Caret>,
    head: Option<Caret>,
    dragging: bool,
    moved: bool,
    hovered: Option<(TextKey, u16)>,
    media: Entity<Media>,
    focus: FocusHandle,
    pub typesetting: Typesetting,
    pub compact: bool,
    footnote_tip: Option<(usize, Point<Pixels>)>,
    footnote_from: Option<usize>,
    parse_task: Option<gpui::Task<()>>,
    last_text: Option<String>,
    last_source: Option<TextSource>,
    host_links: Option<Arc<dyn crate::HostLinks>>,
    queries: query::QueryRefresh,
    pending: Option<PendingText>,
    cursor: Option<bool>,
    cursor_key: Option<TextKey>,
    grain: Grain,
    pointer: Point<Pixels>,
    autoscroll: Option<std::time::Instant>,
    autoscroll_armed: bool,
    decoration: Option<Arc<dyn CodeDecoration>>,
    raw_code: HashSet<BlockKey>,
    clear_code: HashSet<BlockKey>,
    decorated: RefCell<HashMap<BlockKey, Option<Rc<DecoratedCode>>>>,
    tables: RefCell<HashMap<BlockKey, Rc<table::Columns>>>,
    table_scroll: RefCell<HashMap<BlockKey, (gpui::ScrollHandle, bool)>>,
    code_scroll: RefCell<HashMap<usize, gpui::ScrollHandle>>,
    table_room: Rc<RefCell<HashMap<BlockKey, f32>>>,
    table_menu: inplace::table::Menus,
    picker: Option<picker::Picker>,
    code_variables: Option<Arc<crate::CodeVariables>>,
    drawn_code: RefCell<Vec<DrawnCode>>,
    viewer: Option<viewer::Viewer>,
    width: f32,
    sheet_width: Option<f32>,
    diagram_hits: Rc<RefCell<Vec<DiagramHit>>>,
    hovered_diagram: Option<(usize, usize)>,
    drawn_images: RefCell<Vec<(Arc<gpui::RenderImage>, f32, f32)>>,
    pdf_documents: RefCell<pdf_embed::Documents>,
    zones: Zones,
    copied: Option<usize>,
    copy_reset: Option<gpui::Task<()>>,
    picker_dismissed: Option<u32>,
    press_alt: bool,
    reveal_inner: Option<(usize, u32, u8)>,
    in_place: Option<Entity<gpui_component::input::InputState>>,
    note_field: Option<Entity<gpui_component::input::InputState>>,
    editing: Option<inplace::Editing>,
    in_place_frame: Option<inplace::Frame>,
    focus_target: Option<walk::Target>,
    in_place_press: Option<usize>,
    in_place_text: Option<gpui_component::Rope>,
    prop_menu: Option<properties::PropMenu>,
    prop_refused: Option<(String, properties::FieldError)>,
    prop_typing: Option<(String, focus_core::PropertyKind)>,
    handles: bool,
    handle_hover: Option<handles::HandleAt>,
    handle_drag: Option<handles::HandleDrag>,
    handle_quiet: bool,
    handle_pointer: Option<Point<Pixels>>,
    handle_pointed: bool,
    entry_bounds: handles::EntryBounds,
    folds_after_move: Option<FoldState>,
}

impl EventEmitter<ReadingEvent> for ReadingView {}
impl EventEmitter<LanguageChosen> for ReadingView {}
impl EventEmitter<LinkPage> for ReadingView {}
impl EventEmitter<FoldsChanged> for ReadingView {}
impl EventEmitter<LiveOpen> for ReadingView {}
impl EventEmitter<CrumbClicked> for ReadingView {}
impl EventEmitter<VariableChosen> for ReadingView {}
impl EventEmitter<VariableMenu> for ReadingView {}
impl EventEmitter<PropertyToggled> for ReadingView {}

impl gpui::Focusable for ReadingView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ReadingView {
    pub fn new(typesetting: Typesetting, cx: &mut Context<Self>) -> Self {
        let media = media(cx);
        cx.observe(&media, |_, _, cx| cx.notify()).detach();
        ReadingView {
            built: None,
            list: ListState::new(0, ListAlignment::Top, px(600.)),
            visible: Vec::new(),
            rows: Vec::new(),
            folded: HashSet::new(),
            callouts: HashMap::new(),
            pending_folds: None,
            tab_titles: Vec::new(),
            tab_index: HashMap::new(),
            tab_focus: RefCell::new(HashMap::new()),
            inner_bounds: Rc::new(RefCell::new(HashMap::new())),
            editor_bounds: Rc::new(std::cell::Cell::new(None)),
            parked: false,
            editor_height: Rc::new(std::cell::Cell::new(0.)),
            open_floor: 0.,
            remote_allowed: false,
            live: false,
            open: None,
            pending_open_lines: None,
            props_bounds: Rc::new(std::cell::Cell::new(None)),
            down_here: false,
            registry: Registry::default(),
            anchor: None,
            head: None,
            dragging: false,
            moved: false,
            hovered: None,
            media,
            focus: cx.focus_handle(),
            typesetting,
            compact: false,
            footnote_tip: None,
            footnote_from: None,
            parse_task: None,
            last_text: None,
            last_source: None,
            host_links: None,
            queries: Default::default(),
            pending: None,
            cursor: None,
            cursor_key: None,
            grain: Grain::Char,
            pointer: Point::default(),
            autoscroll: None,
            autoscroll_armed: false,
            decoration: None,
            raw_code: HashSet::new(),
            clear_code: HashSet::new(),
            decorated: RefCell::new(HashMap::new()),
            tables: RefCell::new(HashMap::new()),
            table_room: Rc::new(RefCell::new(HashMap::new())),
            table_menu: Default::default(),
            table_scroll: RefCell::new(HashMap::new()),
            code_scroll: RefCell::default(),
            picker: None,
            code_variables: None,
            viewer: None,
            drawn_images: RefCell::new(Vec::new()),
            drawn_code: RefCell::new(Vec::new()),
            pdf_documents: RefCell::default(),
            zones: Rc::new(RefCell::new(Vec::new())),
            copied: None,
            copy_reset: None,
            picker_dismissed: None,
            press_alt: false,
            reveal_inner: None,
            in_place: None,
            note_field: None,
            editing: None,
            in_place_frame: None,
            focus_target: None,
            in_place_press: None,
            in_place_text: None,
            prop_menu: None,
            prop_refused: None,
            prop_typing: None,
            width: 0.,
            sheet_width: None,
            diagram_hits: Rc::new(RefCell::new(Vec::new())),
            hovered_diagram: None,
            handles: true,
            handle_hover: None,
            handle_drag: None,
            handle_quiet: false,
            handle_pointer: None,
            handle_pointed: false,
            entry_bounds: Rc::new(RefCell::new(HashMap::new())),
            folds_after_move: None,
        }
    }

    pub fn script_mouse(
        &mut self,
        input: ScriptMouse,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = gpui::Modifiers::default();
        match input {
            ScriptMouse::Move(position) => self.on_move(
                &MouseMoveEvent {
                    position,
                    pressed_button: None,
                    modifiers,
                },
                window,
                cx,
            ),
            ScriptMouse::Down { at, count, shift } => self.on_down(
                &MouseDownEvent {
                    button: MouseButton::Left,
                    position: at,
                    modifiers: gpui::Modifiers { shift, ..modifiers },
                    click_count: count,
                    first_mouse: false,
                },
                window,
                cx,
            ),
            ScriptMouse::Press {
                at,
                count,
                modifiers,
            } => self.on_down(
                &MouseDownEvent {
                    button: MouseButton::Left,
                    position: at,
                    modifiers,
                    click_count: count,
                    first_mouse: false,
                },
                window,
                cx,
            ),
            ScriptMouse::Drag(position) => self.drag_to(position, cx),
            ScriptMouse::Up(position) => {
                self.on_up(
                    &MouseUpEvent {
                        button: MouseButton::Left,
                        position,
                        modifiers,
                        click_count: 1,
                    },
                    window,
                    cx,
                );
                self.end_drag(cx);
            }
        }
    }

    pub fn script_diagram_point(&self, nth: usize) -> Option<Point<Pixels>> {
        self.diagram_hits
            .borrow()
            .get(nth)
            .map(|hit| hit.bounds.center())
    }

    pub fn script_zone_point(&self, kind: &str, nth: usize) -> Option<Point<Pixels>> {
        let zones = self.zones.borrow();
        let mut found = zones.iter().filter(|(_, zone)| match zone {
            Zone::Image { .. } | Zone::PdfPage { .. } => kind == "image",
            Zone::Task(_) => kind == "task",
            Zone::Tag(_) => kind == "tag",
            Zone::Fold(_) => kind == "fold",
            Zone::Crumb(_) => kind == "crumb",
            Zone::CalloutIcon { .. } => kind == "callout",
            Zone::PropName(_) => kind == "property",
            Zone::PropCheck(_) => kind == "checkbox",
            Zone::PropField(_) => kind == "field",
            Zone::QueryHit { .. } => kind == "query",
        });
        found.nth(nth).map(|(bounds, _)| bounds.center())
    }

    pub fn script_open_viewer(
        &mut self,
        nth: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let found = self.drawn_images.borrow().get(nth).cloned();
        match found {
            Some((image, width, height)) => {
                self.open_viewer(image, width, height, window, cx);
                true
            }
            None => false,
        }
    }

    pub fn script_toggle_secrets(&mut self, line: u32, cx: &mut Context<Self>) -> bool {
        let Some(built) = self.built.clone() else {
            return false;
        };
        let Some((item, Node::Code { code, .. })) = built
            .items
            .iter()
            .enumerate()
            .find(|(_, i)| i.lines.start == line)
            .map(|(item, i)| (item, &i.node))
        else {
            return false;
        };
        let block = (item, code.ord);
        if !self.clear_code.remove(&block) {
            self.clear_code.insert(block);
        }
        cx.notify();
        true
    }

    pub fn script_show_copied(&mut self, line: u32, cx: &mut Context<Self>) -> bool {
        let Some(built) = self.built.clone() else {
            return false;
        };
        let Some((item, entry)) = built
            .items
            .iter()
            .enumerate()
            .find(|(_, i)| i.lines.start == line)
        else {
            return false;
        };
        let which = match &entry.node {
            Node::Code { code, .. } => ((item << 16) | (code.ord as usize) << 8) << 2,
            Node::Mermaid { .. } => (item << 18) | 0xFFFF << 2 | 3,
            _ => return false,
        };
        self.confirm_copied(which, cx);
        true
    }

    pub fn set_code_variables(
        &mut self,
        variables: Option<Arc<crate::CodeVariables>>,
        cx: &mut Context<Self>,
    ) {
        if variables.is_none() && self.picker.as_ref().is_some_and(|p| p.is_variables()) {
            self.picker = None;
        }
        self.code_variables = variables;
        cx.notify();
    }

    pub fn script_code(&self) -> Vec<(u32, String, String)> {
        let mut out: Vec<(u32, String, String)> = Vec::new();
        for drawn in self.drawn_code.borrow().iter() {
            match out.last_mut() {
                Some(last) if last.0 == drawn.line && drawn.part > 0 => {
                    last.1.push_str(&drawn.shown)
                }
                _ => out.push((drawn.line, drawn.shown.clone(), drawn.copied.clone())),
            }
        }
        out
    }

    pub fn script_open_variables(
        &mut self,
        line: u32,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_picker_of(picker::PickerKind::Variable, line, at, window, cx);
    }

    pub fn script_open_language(
        &mut self,
        line: u32,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_picker(line, at, window, cx);
    }

    pub fn set_code_decoration(
        &mut self,
        decoration: Option<Arc<dyn CodeDecoration>>,
        cx: &mut Context<Self>,
    ) {
        self.decoration = decoration;
        self.decorated.borrow_mut().clear();
        cx.notify();
    }

    pub fn set_host_links(
        &mut self,
        links: Option<Arc<dyn crate::HostLinks>>,
        cx: &mut Context<Self>,
    ) {
        self.host_links = links;
        if let (Some(text), Some((note, resolver))) =
            (self.last_text.take(), self.last_source.clone())
        {
            self.set_markdown(text, note, resolver, cx);
        }
    }

    pub fn fold_all_sections(&mut self, cx: &mut Context<Self>) {
        let Some(built) = &self.built else { return };
        self.folded = built
            .headings
            .iter()
            .filter(|h| !h.inner)
            .map(|h| h.item)
            .collect();
        self.refold(cx);
    }

    pub fn unfold_all_sections(&mut self, cx: &mut Context<Self>) {
        self.folded.clear();
        self.refold(cx);
    }

    pub fn load_remote_images(&mut self, cx: &mut Context<Self>) {
        self.remote_allowed = true;
        cx.notify();
    }

    pub fn fold_state(&self) -> FoldState {
        let Some(built) = &self.built else {
            return self.pending_folds.clone().unwrap_or_default();
        };
        let mut sections: Vec<String> = built
            .headings
            .iter()
            .filter(|h| !h.inner && self.folded.contains(&h.item))
            .map(|h| h.text.clone())
            .collect();
        sections.dedup();
        let mut collapsibles: Vec<(u32, u32, bool)> = self
            .callouts
            .iter()
            .filter_map(|(&(item, key), &open)| {
                Some((built.items.get(item)?.lines.start, key, open))
            })
            .collect();
        collapsibles.sort_unstable();
        FoldState {
            sections,
            collapsibles,
            tabs: self.tab_titles.clone(),
        }
    }

    pub fn set_fold_state(&mut self, state: FoldState, cx: &mut Context<Self>) {
        if self.parse_task.is_some() || self.built.is_none() {
            self.pending_folds = Some(state);
            return;
        }
        self.apply_folds(&state);
        self.refold(cx);
    }

    fn apply_folds(&mut self, state: &FoldState) {
        let Some(built) = &self.built else { return };
        self.folded = built
            .headings
            .iter()
            .filter(|h| !h.inner && state.sections.contains(&h.text))
            .map(|h| h.item)
            .collect();
        self.callouts = state
            .collapsibles
            .iter()
            .filter_map(|&(line, key, open)| {
                Some((
                    (built.items.iter().position(|i| i.lines.start == line)?, key),
                    open,
                ))
            })
            .collect();
        self.tab_titles = state.tabs.clone();
    }

    fn refold(&mut self, cx: &mut Context<Self>) {
        self.recompute();
        let top = self.list.logical_scroll_top();
        self.list.reset(self.rows.len() + 1);
        self.list.scroll_to(top);
        cx.notify();
    }

    pub fn point_of(&self, target: &str) -> Option<Point<Pixels>> {
        let (text, nth) = match target.rsplit_once('|') {
            Some((text, n)) if !text.is_empty() => match n.parse::<usize>() {
                Ok(n) => (text, Some(n)),
                Err(_) => (target, None),
            },
            _ => (target, None),
        };
        let hits = self.registry.0.borrow();
        hits.iter().find_map(|hit| {
            let at = hit.rich.text.find(text)?;
            let line = hit.layout.line_height();
            match nth {
                Some(n) => {
                    let byte = at + text.char_indices().nth(n).map_or(text.len(), |(i, _)| i);
                    let point = hit.layout.position_for_index(byte)?;
                    Some(point + gpui::point(px(0.), line / 2.))
                }
                None => {
                    let start = hit.layout.position_for_index(at)?;
                    Some(start + gpui::point(px(4.), line / 2.))
                }
            }
        })
    }

    pub fn compact(typesetting: Typesetting, cx: &mut Context<Self>) -> Self {
        let mut view = ReadingView::new(typesetting, cx);
        view.compact = true;
        view.list = ListState::new(0, ListAlignment::Top, px(200.));
        view
    }

    pub fn set_markdown(
        &mut self,
        text: impl Into<String>,
        note: Option<Arc<focus_core::NoteMeta>>,
        resolver: Arc<dyn crate::Resolver>,
        cx: &mut Context<Self>,
    ) {
        let text = text.into();
        if self.parse_task.is_some() {
            self.pending = Some((text, note, resolver));
            return;
        }
        if self.built.is_some() && self.last_text.as_deref() == Some(text.as_str()) {
            return;
        }
        self.last_text = Some(text.clone());
        self.last_source = Some((note.clone(), resolver.clone()));
        let host_links = self.host_links.clone();
        let task = cx.background_spawn(async move {
            Arc::new(super::model::build_text(
                &text,
                note.as_deref(),
                resolver.as_ref(),
                host_links.as_deref(),
            ))
        });
        self.parse_task = Some(cx.spawn(async move |this, cx| {
            let built = task.await;
            let _ = this.update(cx, |this, cx| {
                this.parse_task = None;
                this.set_built(built, cx);
                if let Some((text, note, resolver)) = this.pending.take() {
                    this.set_markdown(text, note, resolver, cx);
                }
            });
        }));
    }

    pub fn set_base_page(
        &mut self,
        path: std::path::PathBuf,
        resolver: Arc<dyn crate::Resolver>,
        cx: &mut Context<Self>,
    ) {
        self.last_text = None;
        self.last_source = None;
        let task = cx.background_spawn(async move {
            Arc::new(super::model::build_base_page(&path, resolver.as_ref()))
        });
        self.parse_task = Some(cx.spawn(async move |this, cx| {
            let built = task.await;
            let _ = this.update(cx, |this, cx| {
                this.parse_task = None;
                this.set_built(built, cx);
            });
        }));
    }

    pub fn set_markdown_sync(
        &mut self,
        text: impl Into<String>,
        note: Option<Arc<focus_core::NoteMeta>>,
        resolver: &dyn crate::Resolver,
        cx: &mut Context<Self>,
    ) {
        let text = text.into();
        if self.built.is_some() && self.last_text.as_deref() == Some(text.as_str()) {
            return;
        }
        let built = Arc::new(super::model::build_text(
            &text,
            note.as_deref(),
            resolver,
            self.host_links.as_deref(),
        ));
        self.last_text = Some(text);
        self.parse_task = None;
        self.pending = None;
        self.set_built(built, cx);
    }

    pub fn set_cursor(&mut self, cursor: Option<bool>, cx: &mut Context<Self>) {
        if self.cursor != cursor {
            self.cursor = cursor;
            cx.notify();
        }
    }

    pub fn busy(&self) -> bool {
        self.parse_task.is_some()
    }

    fn last_prose_key(&self) -> Option<TextKey> {
        let built = self.built.as_ref()?;
        let item = *self.visible.last()?;
        let ord = last_prose(&built.items[item].node)?;
        Some((item as u32 + 1, ord))
    }

    fn standalone_cursor(&self, t: &Tokens) -> Option<AnyElement> {
        let visible = self.cursor?;
        if self.cursor_key.is_some() {
            return None;
        }
        let base = self.base(t);
        Some(
            div()
                .w_full()
                .font_family(base.mono.clone())
                .text_size(base.size)
                .line_height(base.line_height)
                .text_color(if visible { t.ink } else { t.ink.opacity(0.) })
                .child("\u{2588}")
                .into_any_element(),
        )
    }

    fn footnotes_item(&self) -> Option<usize> {
        let built = self.built.as_ref()?;
        built
            .items
            .iter()
            .rposition(|i| matches!(i.node, Node::Footnotes(_)))
    }

    fn follow_footnote(&mut self, from: Option<usize>, cx: &mut Context<Self>) {
        if let Some(item) = self.footnotes_item() {
            self.footnote_from = from;
            self.footnote_tip = None;
            self.scroll_to_item(item);
            cx.notify();
        }
    }

    pub fn built(&self) -> Option<&Arc<Built>> {
        self.built.as_ref()
    }

    pub fn set_built(&mut self, built: Arc<Built>, cx: &mut Context<Self>) {
        if !self.accepts_in_place(&built, cx) {
            return;
        }
        if self.caret_shown() && self.built.is_some() {
            let before = self.built.as_ref().map_or(0, |b| b.items.len()) as i64;
            let delta = built.items.len() as i64 - before;
            self.built = Some(built);
            self.decorated.borrow_mut().clear();
            self.tables.borrow_mut().clear();
            self.folded.retain(|i| {
                self.built
                    .as_ref()
                    .is_some_and(|b| b.headings.iter().any(|h| !h.inner && h.item == *i))
            });
            self.splice_changed_rows(None, delta);
            cx.notify();
            return;
        }
        let top = self.top_item();
        let in_header = Some(self.list.logical_scroll_top()).filter(|t| t.item_ix == 0);
        let first = self.built.is_none();
        if self
            .built
            .as_ref()
            .is_none_or(|b| b.note != built.note || built.note.is_none())
        {
            self.remote_allowed = false;
        }
        self.built = Some(built);
        self.decorated.borrow_mut().clear();
        self.tables.borrow_mut().clear();
        self.table_room.borrow_mut().clear();
        self.table_scroll.borrow_mut().clear();
        self.raw_code.clear();
        self.clear_code.clear();
        self.picker = None;
        self.anchor = None;
        self.head = None;
        match self.pending_folds.take() {
            Some(state) => self.apply_folds(&state),
            None => self.folded.retain(|i| {
                self.built
                    .as_ref()
                    .is_some_and(|b| b.headings.iter().any(|h| !h.inner && h.item == *i))
            }),
        }
        if let Some(open) = &mut self.open {
            let from = match self.pending_open_lines.take() {
                Some(lines) => {
                    let from = open.lines.start.min(lines.start);
                    open.lines = lines;
                    from
                }
                None => open.lines.start,
            };
            self.relayout_below(Some(from), cx);
            return;
        }
        self.recompute();
        if !self.compact {
            self.list.reset(self.rows.len() + 1);
            match (first, in_header, top) {
                (true, ..) => {}
                (false, Some(offset), _) => self.list.scroll_to(offset),
                (false, None, Some(item)) => self.scroll_to_item(item),
                (false, None, None) => {}
            }
        }
        cx.notify();
    }

    fn recompute(&mut self) {
        self.rows.clear();
        let Some(built) = &self.built else {
            self.visible.clear();
            return;
        };
        self.visible = visible_items(built, &self.folded);
        for &item in &self.visible {
            let parts = match &built.items[item].node {
                Node::Code { code, .. } if !self.compact => {
                    code.text.lines().count().div_ceil(CODE_PART).max(1) as u32
                }
                _ => 1,
            };
            self.rows.extend((0..parts).map(|part| Row {
                item,
                part,
                parts,
                editor: false,
            }));
        }
        self.place_editor();
    }

    fn row_of(&self, item: usize) -> Option<usize> {
        self.rows.iter().position(|r| r.item == item).map(|p| p + 1)
    }

    pub fn top_item(&self) -> Option<usize> {
        let top = self.list.logical_scroll_top();
        if top.item_ix == 0 {
            return self
                .visible
                .first()
                .copied()
                .filter(|_| top.offset_in_item > px(0.))
                .or(Some(0))
                .filter(|_| self.built.is_some());
        }
        self.rows.get(top.item_ix - 1).map(|r| r.item)
    }

    pub fn scroll_to_item(&mut self, item: usize) {
        self.unfold_around(item);
        let row = self
            .rows
            .iter()
            .position(|r| r.item >= item)
            .map_or(self.rows.len(), |p| p + 1);
        self.list.scroll_to(ListOffset {
            item_ix: row,
            offset_in_item: px(0.),
        });
    }

    pub fn scroll_to_line(&mut self, line: u32) {
        let Some(built) = self.built.clone() else {
            return;
        };
        let item = built.item_at_line(line);
        self.reveal_tab(item, line);
        self.scroll_to_item(item);
        let inside = built
            .items
            .get(item)
            .is_some_and(|i| live::is_container(&i.node) && line > i.lines.start);
        self.reveal_inner = inside.then_some((item, line, REVEAL_INNER_FRAMES));
    }

    fn reveal_inner_line(&mut self, cx: &mut Context<Self>) {
        let Some((item, line, frames)) = self.reveal_inner.take() else {
            return;
        };
        let found = self.inner_bounds.borrow().get(&item).and_then(|blocks| {
            blocks
                .iter()
                .filter(|(lines, _)| lines.contains(&line))
                .min_by(|a, b| f32::from(a.1.size.height).total_cmp(&f32::from(b.1.size.height)))
                .map(|(_, b)| *b)
        });
        match found {
            Some(bounds) => self
                .list
                .scroll_by(bounds.top() - self.list.viewport_bounds().top()),
            None if frames > 0 => self.reveal_inner = Some((item, line, frames - 1)),
            None => return,
        }
        cx.notify();
    }

    pub fn set_typesetting(&mut self, typesetting: Typesetting, cx: &mut Context<Self>) {
        if self.typesetting != typesetting {
            self.typesetting = typesetting;
            self.tables.borrow_mut().clear();
            self.list.reset(self.rows.len() + 1);
            cx.notify();
        }
    }

    pub fn set_sheet_width(&mut self, width: Option<f32>, cx: &mut Context<Self>) {
        if self.sheet_width != width {
            self.sheet_width = width;
            self.tables.borrow_mut().clear();
            self.list.reset(self.rows.len() + 1);
            cx.notify();
        }
    }

    fn gutter(&self) -> f32 {
        let Some(sheet) = self.sheet_width.filter(|_| !self.compact) else {
            return 0.;
        };
        let pad = ((sheet - self.typesetting.measure) / 2.).max(0.);
        let framed = sheet + 2.;
        if self.width <= 0. || self.width >= framed {
            ((self.width - framed) / 2.).max(0.).round() + 1. + pad
        } else {
            f32::from(size::SHEET_PAD_NARROW)
        }
    }

    fn room(&self) -> f32 {
        if self.compact || self.width <= 0. {
            self.typesetting.measure
        } else {
            (self.width - self.gutter() * 2.).max(1.)
        }
    }

    fn follow_width(&mut self, width: f32) {
        if width == self.width {
            return;
        }
        let gutter = self.gutter();
        self.width = width;
        if !self.compact && self.gutter() != gutter {
            self.tables.borrow_mut().clear();
            let count = self.list.item_count();
            self.list.splice(0..count, count);
        }
    }

    pub fn scroll_by(&mut self, delta: Pixels) {
        self.list.scroll_by(delta);
    }

    pub fn scroll_to_fraction(&mut self, fraction: f32) {
        let row = (self.rows.len() as f32 * fraction).round() as usize;
        self.list.scroll_to(ListOffset {
            item_ix: row,
            offset_in_item: px(0.),
        });
    }

    pub fn toggle_section(&mut self, item: usize, cx: &mut Context<Self>) {
        let Some(built) = self.built.clone() else {
            return;
        };
        let Some(end) = built
            .headings
            .iter()
            .position(|h| !h.inner && h.item == item)
            .map(|h| built.section_end[h])
        else {
            return;
        };
        if !self.folded.remove(&item) {
            self.folded.insert(item);
        }
        let span = |rows: &[Row]| {
            let first = rows
                .iter()
                .position(|r| r.item >= item)
                .unwrap_or(rows.len());
            let after = rows
                .iter()
                .position(|r| r.item >= end)
                .unwrap_or(rows.len());
            (first + 1, after.max(first) + 1)
        };
        let (start, old_end) = span(&self.rows);
        self.recompute();
        let (_, new_end) = span(&self.rows);
        self.list.splice(start..old_end, new_end - start);
        cx.emit(FoldsChanged);
        cx.notify();
    }

    fn unfold_around(&mut self, item: usize) -> bool {
        let Some(built) = self.built.clone() else {
            return false;
        };
        let hiding: Vec<usize> = built
            .headings
            .iter()
            .enumerate()
            .filter(|(h, head)| {
                !head.inner
                    && head.item < item
                    && item < built.section_end[*h]
                    && self.folded.contains(&head.item)
            })
            .map(|(_, head)| head.item)
            .collect();
        if hiding.is_empty() {
            return false;
        }
        for heading in hiding {
            self.folded.remove(&heading);
        }
        self.recompute();
        self.list.reset(self.rows.len() + 1);
        true
    }

    fn selection_range(&self, key: TextKey, len: usize) -> Option<Range<usize>> {
        let (a, b) = match (self.anchor, self.head) {
            (Some(a), Some(b)) if a != b => (a.min(b), a.max(b)),
            _ => return None,
        };
        if key < a.key || key > b.key {
            return None;
        }
        let start = if key == a.key { a.offset } else { 0 };
        let end = if key == b.key { b.offset } else { len };
        (start < end).then_some(start..end)
    }

    pub fn selected_text(&self) -> Option<String> {
        let built = self.built.as_ref()?;
        let (a, b) = match (self.anchor, self.head) {
            (Some(a), Some(b)) if a != b => (a.min(b), a.max(b)),
            _ => return None,
        };
        let mut parts = Vec::new();
        for item in a.key.0..=b.key.0 {
            let mut texts = Vec::new();
            if item == 0 {
                texts.push((built.header.title.ord, built.header.title.text.clone()));
            } else if let Some(entry) = built.items.get(item as usize - 1) {
                match (
                    &entry.node,
                    self.rows.iter().find(|r| r.item == item as usize - 1),
                ) {
                    (Node::Code { code, lang, .. }, row) => {
                        let shown = self
                            .decorated(item as usize - 1, lang, code)
                            .filter(|_| !self.raw_code.contains(&(item as usize - 1, code.ord)))
                            .map(|d| {
                                d.shown(self.clear_code.contains(&(item as usize - 1, code.ord)))
                                    .clone()
                            });
                        let shown = shown.as_ref().unwrap_or(code);
                        match row {
                            Some(row) if row.parts > 1 => texts.extend(
                                (0..row.parts).map(|part| (part, code_part(shown, part).text)),
                            ),
                            _ => texts.push((shown.ord, shown.text.clone())),
                        }
                    }
                    (node, _) => texts_of(node, &mut texts),
                }
            }
            texts.sort_by_key(|(ord, _)| *ord);
            for (ord, text) in texts {
                let key = (item, ord);
                if let Some(range) = self.selection_range(key, text.len()) {
                    parts.push(super::model::without_isolates(&text[range]));
                }
            }
        }
        Some(parts.join("\n"))
    }

    fn in_open_editor(&self, position: Point<Pixels>) -> bool {
        self.open.is_some()
            && !self.parked
            && self
                .editor_bounds
                .get()
                .is_some_and(|b| b.contains(&position))
    }

    fn on_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_target = None;
        if self.in_open_editor(event.position) {
            self.down_here = false;
            return;
        }
        self.down_here = true;
        self.press_alt = event.modifiers.alt;
        window.focus(&self.focus);
        self.dragging = true;
        self.moved = false;
        self.pointer = event.position;
        self.press_in_place(event, window, cx);
        let Some((key, offset, rich)) = self.registry.locate(event.position) else {
            self.anchor = None;
            self.head = None;
            cx.notify();
            return;
        };
        if event.modifiers.shift && self.anchor.is_some() {
            self.grain = Grain::Char;
            self.head = Some(Caret { key, offset });
            self.moved = true;
            cx.notify();
            return;
        }
        self.grain = match event.click_count {
            1 => Grain::Char,
            2 => Grain::Word,
            _ => Grain::Text,
        };
        let range = match self.grain {
            Grain::Char => offset..offset,
            Grain::Word => word_at(&rich.text, offset),
            Grain::Text => 0..rich.text.len(),
        };
        self.anchor = Some(Caret {
            key,
            offset: range.start,
        });
        self.head = Some(Caret {
            key,
            offset: range.end,
        });
        self.moved = self.grain != Grain::Char;
        cx.notify();
    }

    fn drag_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.pointer = position;
        self.extend_to(position, cx);
        let viewport = self.list.viewport_bounds();
        let outside = position.y < viewport.top() || position.y > viewport.bottom();
        if outside && self.autoscroll.is_none() {
            self.autoscroll = Some(std::time::Instant::now());
            cx.notify();
        }
    }

    fn follow_autoscroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.autoscroll.is_none() || self.autoscroll_armed {
            return;
        }
        self.autoscroll_armed = true;
        let entity = cx.entity().downgrade();
        window.on_next_frame(move |_, cx| {
            let _ = entity.update(cx, |this, cx| {
                this.autoscroll_armed = false;
                this.autoscroll_step(cx);
            });
        });
    }

    fn extend_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.drag_in_place(position, cx);
        let Some((key, offset, rich)) = self.registry.locate(position) else {
            return;
        };
        let Some(anchor) = self.anchor else { return };
        let caret = match self.grain {
            Grain::Char => Caret { key, offset },
            Grain::Word => {
                let word = word_at(&rich.text, offset);
                if (key, offset) < (anchor.key, anchor.offset) {
                    Caret {
                        key,
                        offset: word.start,
                    }
                } else {
                    Caret {
                        key,
                        offset: word.end,
                    }
                }
            }
            Grain::Text => {
                if (key, offset) < (anchor.key, anchor.offset) {
                    Caret { key, offset: 0 }
                } else {
                    Caret {
                        key,
                        offset: rich.text.len(),
                    }
                }
            }
        };
        if Some(caret) != self.head {
            self.head = Some(caret);
            self.moved = self.moved || self.head != self.anchor;
            cx.notify();
        }
    }

    fn autoscroll_step(&mut self, cx: &mut Context<Self>) {
        let Some(last) = self.autoscroll else { return };
        let viewport = self.list.viewport_bounds();
        let past = if self.pointer.y < viewport.top() {
            self.pointer.y - viewport.top()
        } else if self.pointer.y > viewport.bottom() {
            self.pointer.y - viewport.bottom()
        } else {
            px(0.)
        };
        if !self.dragging || past == px(0.) {
            self.autoscroll = None;
            return;
        }
        let now = std::time::Instant::now();
        let elapsed = now
            .duration_since(last)
            .as_secs_f32()
            .min(AUTOSCROLL_MAX_STEP);
        self.autoscroll = Some(now);
        let speed = AUTOSCROLL_SLOW
            + (f32::from(past).abs() / AUTOSCROLL_RAMP).min(1.)
                * (AUTOSCROLL_FAST - AUTOSCROLL_SLOW);
        self.list
            .scroll_by(px((speed * elapsed).copysign(f32::from(past))));
        let clamped = gpui::point(
            self.pointer.x,
            self.pointer
                .y
                .clamp(viewport.top() + px(1.), viewport.bottom() - px(1.)),
        );
        self.extend_to(clamped, cx);
        cx.notify();
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.dragging {
            self.dragging = false;
            self.autoscroll = None;
            cx.notify();
        }
    }

    fn diagram_at(&self, position: Point<Pixels>) -> Option<DiagramHit> {
        self.diagram_hits
            .borrow()
            .iter()
            .find(|hit| hit.bounds.contains(&position))
            .cloned()
    }

    fn on_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.dragging && event.pressed_button == Some(MouseButton::Left) {
            return;
        }
        self.hover_handle(event.position, cx);
        let diagram = self.diagram_at(event.position).map(|hit| hit.id);
        if diagram != self.hovered_diagram {
            self.hovered_diagram = diagram;
            cx.notify();
        }
        if diagram.is_some() {
            return;
        }
        let hit = self.link_at(event.position);
        let hovered = hit.as_ref().map(|(key, index, _)| (*key, *index));
        if hovered != self.hovered {
            self.hovered = hovered;
            let goal = hit.map(|(_, _, goal)| goal);
            self.footnote_tip = match &goal {
                Some(LinkGoal::Footnote(i)) => Some((*i, event.position)),
                _ => None,
            };
            match goal {
                Some(LinkGoal::Footnote(_)) => {}
                goal => cx.emit(ReadingEvent::Hover {
                    goal,
                    position: event.position,
                }),
            }
            cx.notify();
        }
    }

    fn link_at(&self, position: Point<Pixels>) -> Option<(TextKey, u16, LinkGoal)> {
        let hits = self.registry.0.borrow();
        let hit = hits.iter().find(|h| h.bounds.contains(&position))?;
        let index = hit.layout.index_for_position(position).ok()?;
        let (n, link) = hit
            .rich
            .links
            .iter()
            .enumerate()
            .find(|(_, l)| l.range.contains(&index))?;
        Some((hit.key, n as u16, link.goal.clone()))
    }

    fn copy(&mut self, which: usize, text: String, cx: &mut Context<Self>) {
        focus_kit::clipboard::copy_text(cx, text);
        self.confirm_copied(which, cx);
    }

    fn confirm_copied(&mut self, which: usize, cx: &mut Context<Self>) {
        self.copied = Some(which);
        self.copy_reset = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn zone_at(&self, position: Point<Pixels>) -> Option<Zone> {
        self.zones
            .borrow()
            .iter()
            .rev()
            .find(|(bounds, _)| bounds.contains(&position))
            .map(|(_, zone)| zone.clone())
    }

    fn on_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.picker_dismissed = None;
        self.dragging = false;
        self.autoscroll = None;
        if !self.moved {
            self.anchor = None;
            self.head = None;
            let source = self.live && (event.modifiers.alt || self.press_alt);
            let acted = !source && self.click_at(event, window, cx);
            if !acted
                && self.live
                && self.down_here
                && let Some(open) = self.live_target(event.position)
                && (source || !self.caret_on_click(&open, window, cx))
            {
                cx.emit(open);
            }
            if !acted && !self.live && self.down_here {
                self.caret_on_value_click(window, cx);
            }
        }
        self.down_here = false;
        self.press_alt = false;
        cx.notify();
    }

    fn click_at(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.zone_at(event.position) {
            Some(Zone::Tag(tag)) => {
                cx.emit(ReadingEvent::OpenLink {
                    goal: LinkGoal::Tag(tag),
                    new_tab: event.modifiers.control,
                });
                return true;
            }
            Some(Zone::Task(line)) => {
                cx.emit(ReadingEvent::ToggleTask(line));
                return true;
            }
            Some(Zone::Fold(item)) => {
                self.toggle_section(item, cx);
                return true;
            }
            Some(Zone::Crumb(rank)) => {
                if let Some(built) = &self.built {
                    let folder: std::path::PathBuf =
                        built.header.crumbs.iter().take(rank + 1).collect();
                    cx.emit(CrumbClicked {
                        folder,
                        at: event.position,
                    });
                }
                return true;
            }
            Some(Zone::Image {
                image,
                width,
                height,
            }) if !self.live => {
                self.open_viewer(image, width, height, window, cx);
                return true;
            }
            Some(Zone::Image { .. }) => return false,
            Some(Zone::QueryHit { note, line }) => {
                cx.emit(query::QueryHitClicked {
                    note,
                    line,
                    new_tab: event.modifiers.secondary(),
                });
                return true;
            }
            Some(Zone::PdfPage { relative, page }) if !self.live => {
                cx.emit(LinkPage(page));
                cx.emit(ReadingEvent::OpenLink {
                    goal: LinkGoal::Attachment(relative),
                    new_tab: event.modifiers.control,
                });
                return true;
            }
            Some(Zone::PdfPage { .. }) => return false,
            Some(Zone::CalloutIcon { item, callout })
                if inplace::quote::icon_clicked(self, item, callout, window, cx) =>
            {
                return true;
            }
            Some(Zone::PropName(index)) => {
                self.open_prop_menu(index, event.position, window, cx);
                return true;
            }
            Some(Zone::PropCheck(index)) => {
                self.toggle_prop(index, window, cx);
                return true;
            }
            Some(Zone::CalloutIcon { .. } | Zone::PropField(_)) | None => {}
        }
        let link = self
            .diagram_at(event.position)
            .map(|hit| (TextKey::default(), 0, hit.goal))
            .or_else(|| self.link_at(event.position));
        match link {
            Some((key, _, goal)) => {
                if matches!(goal, LinkGoal::Attachment(_))
                    && let Some(page) = self.link_page(event.position)
                {
                    cx.emit(LinkPage(page));
                }
                let item = key.0.checked_sub(1).map(|i| i as usize);
                self.follow(item, goal, event.modifiers.control, cx)
            }
            None => return false,
        }
        true
    }

    fn base(&self, t: &Tokens) -> Base {
        let size = self.typesetting.base;
        Base {
            family: self.typesetting.text_font.clone(),
            mono: self.typesetting.code_font.clone(),
            size: px(size),
            line_height: px((size * self.typesetting.leading).round()),
            color: t.ink_body,
            weight: FontWeight::NORMAL,
            italic: false,
            tinted: self.typesetting.tinted,
        }
    }

    fn text(&self, key_item: u32, rich: &Rich, base: &Base, t: &Tokens) -> AnyElement {
        self.text_aligned(key_item, rich, base, t, None)
    }

    fn text_aligned(
        &self,
        key_item: u32,
        rich: &Rich,
        base: &Base,
        t: &Tokens,
        align: Option<gpui::TextAlign>,
    ) -> AnyElement {
        let key = (key_item, rich.ord);
        let with_cursor;
        let rich = match self.cursor {
            Some(visible) if self.cursor_key == Some(key) => {
                with_cursor = cursor_appended(rich, visible);
                &with_cursor
            }
            _ => rich,
        };
        let decor = self.decorate(key, rich, t);
        let revealed;
        let rich = match decor.as_ref().and_then(|d| d.rich.clone()) {
            Some(shown) => {
                revealed = shown;
                &revealed
            }
            None => rich,
        };
        let hovered = self.hovered.filter(|(k, _)| *k == key).map(|(_, i)| i);
        let runs = runs(rich, base, t, hovered);
        let focused_link = match self.focus_target {
            Some(walk::Target::Link { key: k, link, .. }) if k == key => {
                rich.links.get(link as usize).map(|l| l.range.clone())
            }
            _ => None,
        };
        let paints = crate::text::paints(rich, base, t, hovered).outline(focused_link, t.ink);
        let selection = match &decor {
            Some(decor) => decor.selection.clone(),
            None => self.selection_range(key, rich.text.len()),
        };
        let line_height = if rich.text.chars().any(is_han) {
            base.line_height
                .max(px((f32::from(base.size) * 1.75).round()))
        } else {
            base.line_height
        };
        match align {
            Some(align) => div().text_align(align),
            None => focus_kit::Dir::current().align_text(div(), &rich.text),
        }
        .w_full()
        .min_w_0()
        .font_family(base.family.clone())
        .text_size(base.size)
        .line_height(line_height)
        .text_color(base.color)
        .child(
            RichText::new(
                key,
                Arc::new(rich.clone()),
                runs,
                selection,
                t.sel,
                self.registry.clone(),
            )
            .editing(
                match &decor {
                    Some(d) => d.map.clone(),
                    None => self.click_map(rich),
                },
                decor.as_ref().and_then(|d| d.caret),
                decor.as_ref().and_then(|d| d.marked.clone()),
                decor.map(|d| d.found).unwrap_or_default(),
            )
            .paints(paints),
        )
        .into_any_element()
    }

    fn render_header(&self, built: &Built, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let header = &built.header;
        let title = Base {
            family: focus_kit::typography::DISPLAY.into(),
            size: px(34. * self.reading_scale()),
            line_height: px((40. * self.reading_scale()).round()),
            color: t.ink,
            weight: FontWeight::SEMIBOLD,
            ..self.base(t)
        };
        div()
            .flex()
            .flex_col()
            .w_full()
            .pt(if self.compact { size::S6 } else { size::S8 })
            .pb(size::S6)
            .when(!self.live && !header.crumbs.is_empty(), |this| {
                this.child(
                    div()
                        .pb(size::S2)
                        .flow_row()
                        .flex_wrap()
                        .ui_sm()
                        .text_color(t.ink_3)
                        .children(header.crumbs.iter().enumerate().flat_map(|(rank, folder)| {
                            let crumb = self
                                .zone(
                                    div()
                                        .id(("crumb", rank))
                                        .relative()
                                        .cursor_pointer()
                                        .hover(move |s| s.text_color(t.ink))
                                        .child(folder.clone()),
                                    Zone::Crumb(rank),
                                )
                                .into_any_element();
                            let separator = (rank > 0)
                                .then(|| div().px(size::S1).child("/").into_any_element());
                            separator.into_iter().chain([crumb])
                        })),
                )
            })
            .when(self.live && !header.crumbs.is_empty(), |this| {
                this.child(
                    div()
                        .pb(size::S2)
                        .flow_row()
                        .flex_wrap()
                        .ui_sm()
                        .invisible()
                        .children(header.crumbs.iter().enumerate().flat_map(|(rank, folder)| {
                            let separator = (rank > 0)
                                .then(|| div().px(size::S1).child("/").into_any_element());
                            separator
                                .into_iter()
                                .chain([div().child(folder.clone()).into_any_element()])
                        })),
                )
            })
            .map(|this| match self.open_in_header(built) {
                Some(live::LiveKind::Title) => this.child(self.open_editor(t)),
                _ if self.live && built.title_lines.is_some() => this.child(
                    div()
                        .id("live-title")
                        .group("live-title")
                        .relative()
                        .child(self.text(0, &header.title, &title, t))
                        .child(self.hover_rule("live-title".into(), 0., 0., 0., t.line_2)),
                ),
                _ => this.child(self.text(0, &header.title, &title, t)),
            })
            .when(
                self.open_in_header(built) == Some(live::LiveKind::Properties),
                |this| this.child(div().pt(size::S3).child(self.open_editor(t))),
            )
            .when(
                self.open_in_header(built) != Some(live::LiveKind::Properties)
                    && (!header.props.is_empty() || !header.tags.is_empty()),
                |this| {
                    let bounds = self.props_bounds.clone();
                    let rule = (self.live && built.frontmatter_lines.is_some()).then(|| {
                        self.hover_rule("live-props".into(), 0., f32::from(size::S3), 0., t.line_2)
                    });
                    this.child(
                        div()
                            .id("live-props")
                            .group("live-props")
                            .relative()
                            .child(
                                gpui::canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full(),
                            )
                            .children(rule)
                            .flow_row()
                            .flex_wrap()
                            .items_start()
                            .gap(size::S1)
                            .pt(size::S3)
                            .children(self.prop_chips(t, cx)),
                    )
                },
            )
            .when(
                self.open_in_header(built) != Some(live::LiveKind::Properties),
                |this| {
                    this.children(inplace::props::add_chip(self, t, cx).map(|chip| {
                        div()
                            .relative()
                            .h_0()
                            .child(div().absolute().top(px(1.)).start_at(px(0.)).child(chip))
                    }))
                },
            )
            .when(
                !self.live && !self.compact && built.items.is_empty(),
                |this| {
                    let (hover, ink) = (t.card_2, t.ink);
                    this.child(
                        div().pt(size::S6).flow_row().child(
                            div()
                                .id("reading-empty")
                                .px(size::S1)
                                .rounded(size::RADIUS)
                                .ui()
                                .text_color(t.ink_3)
                                .cursor_pointer()
                                .hover(move |s| s.bg(hover).text_color(ink))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(
                                    cx.listener(|_, _, _, cx| cx.emit(ReadingEvent::StartWriting)),
                                )
                                .child(focus_i18n::t("reading-empty-note")),
                        ),
                    )
                },
            )
            .into_any_element()
    }
}

impl Render for ReadingView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        self.read_frame(window, cx);
        self.keep_caret_line();
        self.open_unseen(window, cx);
        self.keep_caret_visible(window);
        self.registry.clear();
        self.drawn_images.borrow_mut().clear();
        self.drawn_code.borrow_mut().clear();
        self.sync_picker(window, cx);
        self.pdf_documents.borrow_mut().drawn.clear();
        self.follow_width(f32::from(self.list.viewport_bounds().size.width));
        let drawn_width = self.width;
        if !self.compact {
            self.keep_open_block_visible(window);
            self.reveal_inner_line(cx);
        }
        self.follow_autoscroll(window, cx);
        self.cursor_key = self.cursor.and_then(|_| self.last_prose_key());
        let compact_rows: Option<Vec<AnyElement>> = self.compact.then(|| {
            let count = self.visible.len() + 1;
            (1..count)
                .map(|row| self.render_row(row, window, cx))
                .collect()
        });
        self.diagram_hits.borrow_mut().clear();
        self.zones.borrow_mut().clear();
        let hovering_link = self.hovered.is_some() || self.hovered_diagram.is_some();
        let dragging = self.dragging;
        let holding_handle = self.handle_dragged();
        let entity = cx.entity().downgrade();
        div()
            .id("reading")
            .key_context({
                let mut context = gpui::KeyContext::default();
                context.add(CONTEXT);
                if self.live {
                    context.add("live");
                } else if !self.caret_shown() {
                    context.add("walk");
                }
                context
            })
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .when(hovering_link, |this| this.cursor_pointer())
            .when(!hovering_link && !holding_handle, |this| this.cursor_text())
            .when(holding_handle, |this| {
                this.cursor(gpui::CursorStyle::ClosedHand)
            })
            .on_action(cx.listener(|this, _: &CopySelection, window, cx| {
                if this.caret_shown() {
                    return this.copy_source(false, window, cx);
                }
                if let Some(text) = this.selected_text() {
                    focus_kit::clipboard::copy_text(cx, text);
                }
            }))
            .on_action(cx.listener(|this, _: &SelectAllNote, _, cx| {
                if this.caret_shown() {
                    return this.select_note(cx);
                }
                if let Some(built) = &this.built {
                    this.anchor = Some(Caret {
                        key: (0, 0),
                        offset: 0,
                    });
                    this.head = Some(Caret {
                        key: (built.items.len() as u32 + 1, 0),
                        offset: 0,
                    });
                    cx.notify();
                }
            }))
            .on_action(
                cx.listener(|this, _: &FocusNext, window, cx| this.focus_step(true, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &FocusPrevious, window, cx| {
                    this.focus_step(false, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &ActivateFocused, window, cx| this.activate(window, cx)),
            )
            .on_action(cx.listener(|this, _: &FollowLinkAtCaret, _, cx| this.follow_at_caret(cx)))
            .on_action(cx.listener(|this, _: &ToggleAtCaret, _, cx| this.toggle_at_caret(cx)))
            .on_action(cx.listener(|this, _: &LeaveFocus, window, cx| {
                if !this.leave_focus(window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| this.page_key(-1., cx)))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| this.page_key(1., cx)))
            .on_action(cx.listener(|this, _: &NoteStart, _, cx| {
                if this.open.is_some() {
                    return cx.propagate();
                }
                this.list.scroll_to(ListOffset::default());
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NoteEnd, _, cx| {
                if this.open.is_some() {
                    return cx.propagate();
                }
                this.list.scroll_to(ListOffset {
                    item_ix: this.rows.len() + 1,
                    offset_in_item: px(0.),
                });
                cx.notify();
            }))
            .map(|root| inplace::on_actions(root, cx))
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_down))
            .on_mouse_move(cx.listener(Self::on_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_up))
            .when(self.compact, |this| this.h_auto().w_full())
            .child(if let Some(rows) = compact_rows {
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .children(rows)
                    .children(self.standalone_cursor(&t))
                    .into_any_element()
            } else {
                list(
                    self.list.clone(),
                    cx.processor(|this, row: usize, window, cx| {
                        let body = this.render_row(row, window, cx);
                        this.live_hover(row, body, cx)
                    }),
                )
                .size_full()
                .pb(size::S12)
                .into_any_element()
            })
            .when(!self.compact, |this| {
                this.child(focus_kit::scrollbar("reading-scrollbar", self.list.clone()))
                    .child(
                        gpui::canvas(
                            move |bounds, window, _| {
                                if f32::from(bounds.size.width) != drawn_width {
                                    window.refresh();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
            })
            .children(self.parked_editor())
            .children(self.landing_line(&t))
            .children(self.handle_listeners(cx))
            .children(self.input_canvas(cx))
            .when(dragging, |this| {
                this.child(
                    gpui::canvas(
                        |_, _, _| {},
                        move |_, _, window, _| {
                            let moved = entity.clone();
                            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                                if phase == gpui::DispatchPhase::Bubble
                                    && event.pressed_button == Some(MouseButton::Left)
                                {
                                    let _ = moved
                                        .update(cx, |this, cx| this.drag_to(event.position, cx));
                                }
                            });
                            let released = entity.clone();
                            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                                if phase == gpui::DispatchPhase::Bubble {
                                    let _ = released.update(cx, |this, cx| this.end_drag(cx));
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_0(),
                )
            })
            .children(self.render_footnote_tip(&t, window, cx))
            .children(self.render_picker(&t, window, cx))
            .children(self.render_prop_menu(&t, cx))
            .children(self.render_viewer(&t, window, cx))
    }
}

impl ReadingView {
    fn page_key(&mut self, direction: f32, cx: &mut Context<Self>) {
        if self.open.is_some() || self.compact {
            return cx.propagate();
        }
        let page = (self.list.viewport_bounds().size.height
            - px(self.typesetting.base * self.typesetting.leading))
        .max(px(1.));
        self.list.scroll_by(page * direction);
        cx.notify();
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewer_key(&event.keystroke.key, window) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if event.keystroke.key != "escape" {
            return;
        }
        let picker = self.picker.take().is_some();
        if picker {
            window.focus(&self.focus);
        }
        let left = !picker && self.leave_focus(window, cx);
        if self.viewer.take().is_some() || picker || left || self.footnote_tip.take().is_some() {
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn render_footnote_tip(
        &self,
        t: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (index, position) = self.footnote_tip?;
        let item = self.footnotes_item()?;
        let built = self.built.clone()?;
        let Node::Footnotes(notes) = &built.items[item].node else {
            return None;
        };
        let blocks = &notes.get(index)?.blocks;
        let base = Base {
            size: px(13.),
            line_height: px(20.),
            color: t.ink_2,
            ..self.base(t)
        };
        let key_item = u32::MAX - index as u32;
        let body: Vec<AnyElement> = blocks
            .iter()
            .map(|n| self.render_node(n, key_item, item, &base, t, window, cx))
            .collect();
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(position + gpui::point(px(0.), px(16.)))
                    .snap_to_window()
                    .child(
                        focus_kit::floating(t)
                            .w(px(320.))
                            .flex()
                            .flex_col()
                            .gap(size::S2)
                            .px(size::S3)
                            .py(size::S2)
                            .children(body),
                    ),
            )
            .into_any_element(),
        )
    }
}

fn code_part(code: &Rich, part: u32) -> Rich {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(code.text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let len = code.text.len();
    let start = starts
        .get(part as usize * CODE_PART)
        .copied()
        .unwrap_or(len);
    let end = starts
        .get((part as usize + 1) * CODE_PART)
        .map_or(len, |next| next - 1);
    let marks = code
        .marks
        .iter()
        .filter(|(r, _)| r.start < end && r.end > start)
        .map(|(r, m)| (r.start.max(start) - start..r.end.min(end) - start, *m))
        .filter(|(r, _)| r.start < r.end)
        .collect();
    Rich {
        ord: part,
        text: code.text[start..end].to_string(),
        marks,
        links: Vec::new(),
        map: code
            .map
            .as_ref()
            .map(|map| Arc::new(crate::srcmap::slice(map, start..end))),
    }
}

fn last_prose(node: &Node) -> Option<u32> {
    match node {
        Node::Paragraph(rich) | Node::Heading { rich, .. } => Some(rich.ord),
        Node::List { entries, .. } => entries
            .last()
            .and_then(|e| e.children.last())
            .and_then(last_prose),
        Node::Quote(children) | Node::Callout { children, .. } | Node::Details { children, .. } => {
            children.last().and_then(last_prose)
        }
        _ => None,
    }
}

fn cursor_appended(rich: &Rich, visible: bool) -> Rich {
    let mut out = rich.clone();
    let start = out.text.len();
    out.text.push(if visible { '\u{2588}' } else { ' ' });
    out.marks.push((
        start..out.text.len(),
        crate::model::Marks {
            code: true,
            token: Some(focus_md::highlight::Token::Heading),
            ..Default::default()
        },
    ));
    out
}

fn word_at(text: &str, offset: usize) -> Range<usize> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let offset = offset.min(text.len());
    let at = text[offset..]
        .chars()
        .next()
        .or_else(|| text[..offset].chars().next_back());
    match at {
        Some(c) if word(c) => {
            let start = text[..offset]
                .char_indices()
                .rev()
                .take_while(|(_, c)| word(*c))
                .last()
                .map_or(offset, |(i, _)| i);
            let end = text[offset..]
                .char_indices()
                .find(|(_, c)| !word(*c))
                .map_or(text.len(), |(i, _)| offset + i);
            start..end.max(start)
        }
        Some(c) if offset < text.len() => offset..offset + c.len_utf8(),
        _ => offset..offset,
    }
}

pub(super) fn is_han(c: char) -> bool {
    matches!(u32::from(c), 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F)
}

fn visible_items(built: &Built, folded: &HashSet<usize>) -> Vec<usize> {
    let mut visible = Vec::new();
    let mut i = 0;
    while i < built.items.len() {
        visible.push(i);
        if folded.contains(&i)
            && let Some(h) = built.headings.iter().position(|h| !h.inner && h.item == i)
        {
            i = built.section_end[h];
            continue;
        }
        i += 1;
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Decor, Marks, decorated};

    struct GroupedBase;

    impl crate::Resolver for GroupedBase {
        fn resolve_link(&self, _: focus_core::NoteId, _: &str) -> focus_core::LinkTarget {
            focus_core::LinkTarget::Unresolved
        }
        fn absolute(&self, relative: &std::path::Path) -> std::path::PathBuf {
            relative.to_path_buf()
        }
        fn base_views(
            &self,
            _: &std::path::Path,
            _: Option<&std::path::Path>,
        ) -> Option<Result<crate::BaseViews, focus_bases::BasesError>> {
            use focus_bases::{Column, Table, TableGroup, TableRow, Value};
            let group = |key: &str, notes: &[(focus_core::NoteId, &str)]| TableGroup {
                key: Some(Value::Text(key.into())),
                rows: notes
                    .iter()
                    .map(|(note, name)| TableRow {
                        note: *note,
                        cells: vec![Ok(Value::Text((*name).into()))],
                    })
                    .collect(),
                summaries: Vec::new(),
            };
            let table = Table {
                view: Some("Board".into()),
                columns: vec![Column {
                    id: "file.name".into(),
                    label: "name".into(),
                    named: false,
                    link: true,
                }],
                groups: vec![
                    group("done", &[(1, "Alpha"), (2, "Beta")]),
                    group("todo", &[(3, "Gamma")]),
                ],
                matched: 3,
            };
            Some(Ok(vec![(Some("Board".into()), Ok(table))]))
        }
    }

    #[test]
    fn a_folded_group_hides_its_rows_and_keeps_its_title() {
        let built = crate::model::build_base_page(std::path::Path::new("Board.base"), &GroupedBase);
        let titles: Vec<_> = built
            .headings
            .iter()
            .map(|h| (h.text.as_str(), h.item))
            .collect();
        assert_eq!(titles, [("Board", 0), ("done", 1), ("todo", 3)]);
        let rows_of = |item: usize| match &built.items[item].node {
            Node::Base { sections, .. } => match &sections[0].table {
                Node::Table { rows, .. } => rows.len(),
                other => panic!("a table: {other:?}"),
            },
            other => panic!("a base: {other:?}"),
        };
        assert_eq!((rows_of(2), rows_of(4)), (2, 1));
        let unfolded: Vec<usize> = (0..6).collect();
        assert_eq!(visible_items(&built, &HashSet::new()), unfolded);
        assert_eq!(visible_items(&built, &HashSet::from([1])), [0, 1, 3, 4, 5]);
        assert_eq!(visible_items(&built, &HashSet::new()), unfolded);
        assert_eq!(visible_items(&built, &HashSet::from([3])), [0, 1, 2, 3, 5]);
        assert_eq!(visible_items(&built, &HashSet::from([0, 3])), [0]);
    }

    fn rich(ord: u32, text: &str) -> Rich {
        Rich {
            ord,
            text: text.into(),
            marks: Vec::new(),
            links: Vec::new(),
            map: None,
        }
    }

    #[test]
    fn the_cursor_goes_to_the_last_prose_of_the_last_block() {
        use crate::model::ListEntry;
        assert_eq!(last_prose(&Node::Paragraph(rich(3, "x"))), Some(3));
        let list = Node::List {
            ordered: false,
            tight: true,
            entries: vec![
                ListEntry {
                    marker: "-".into(),
                    task: None,
                    line: 0,
                    children: vec![Node::Paragraph(rich(5, "a"))],
                },
                ListEntry {
                    marker: "-".into(),
                    task: None,
                    line: 1,
                    children: vec![Node::Paragraph(rich(7, "b"))],
                },
            ],
        };
        assert_eq!(last_prose(&list), Some(7));
        assert_eq!(
            last_prose(&Node::Quote(vec![Node::Paragraph(rich(9, "q"))])),
            Some(9)
        );
        assert_eq!(last_prose(&Node::Rule), None);
        assert_eq!(
            last_prose(&Node::Mermaid {
                source: String::new()
            }),
            None
        );
    }

    #[test]
    fn the_cursor_is_appended_or_kept_blank_at_the_same_width() {
        let shown = cursor_appended(&rich(0, "text"), true);
        let blank = cursor_appended(&rich(0, "text"), false);
        assert_eq!(shown.text, "text\u{2588}");
        assert_eq!(blank.text, "text ");
        assert_eq!(
            shown.marks.last().map(|(r, m)| (r.clone(), m.code)),
            Some((4..7, true))
        );
    }

    #[test]
    fn a_double_click_selects_the_word_or_the_character() {
        let text = "run nmap_scan, then wait";
        assert_eq!(&text[word_at(text, 6)], "nmap_scan");
        assert_eq!(&text[word_at(text, 13)], ",");
        assert_eq!(&text[word_at(text, text.len())], "wait");
        assert_eq!(&"كتابة الملاحظات"[word_at("كتابة الملاحظات", 2)], "كتابة");
    }

    #[test]
    fn a_decoration_splits_the_marks_it_covers() {
        let rich = Rich {
            ord: 0,
            text: "nc -lvnp $LPORT".into(),
            marks: vec![(
                0..2,
                Marks {
                    code: true,
                    ..Marks::default()
                },
            )],
            links: Vec::new(),
            map: None,
        };
        let out = decorated(&rich, &[(9..15, Decor::Resolved)]);
        let spans: Vec<(std::ops::Range<usize>, Option<Decor>, bool)> = out
            .marks
            .iter()
            .map(|(r, m)| (r.clone(), m.decor, m.code))
            .collect();
        assert_eq!(
            spans,
            [
                (0..2, None, true),
                (2..9, None, false),
                (9..15, Some(Decor::Resolved), false)
            ]
        );
    }

    #[test]
    fn a_long_code_block_is_cut_into_parts_of_whole_lines() {
        let text: String = (0..250)
            .map(|i| format!("line {i}\n"))
            .collect::<String>()
            .trim_end()
            .to_string();
        let code = Rich {
            ord: 0,
            text: text.clone(),
            marks: vec![(
                0..text.len(),
                Marks {
                    code: true,
                    ..Marks::default()
                },
            )],
            links: Vec::new(),
            map: None,
        };
        let last = 249 / CODE_PART;
        let parts: Vec<Rich> = (0..=last as u32).map(|p| code_part(&code, p)).collect();
        assert!(
            parts[0].text.starts_with("line 0\n")
                && parts[0].text.ends_with(&format!("line {}", CODE_PART - 1))
        );
        assert!(
            parts[last]
                .text
                .starts_with(&format!("line {}", last * CODE_PART))
                && parts[last].text.ends_with("line 249")
        );
        assert_eq!(
            parts
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            text
        );
        assert!(parts.iter().all(|p| p.marks
            == vec![(
                0..p.text.len(),
                Marks {
                    code: true,
                    ..Marks::default()
                }
            )]));
    }

    #[test]
    fn table_columns_share_the_room_by_what_they_lack() {
        let columns = table::Columns {
            min: vec![97., 82., 69.],
            max: vec![97., 170., 69.],
        };
        assert_eq!(columns.widths(400.), vec![97., 170., 69.]);
        let fitted = columns.widths(287.);
        assert!(fitted.iter().sum::<f32>() <= 287. && fitted[0] == 97. && fitted[2] == 69.);
        let wide = table::Columns {
            min: vec![60., 80., 90.],
            max: vec![120., 600., 100.],
        };
        assert_eq!(wide.widths(200.), vec![120., 280., 100.]);
    }
}
