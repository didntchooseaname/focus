use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use focus_kit::{ActiveTokens, Icon, IconButton, Mode, Tokens, Type, ZoomControls, size};
use gpui::{
    App, Bounds, Context, CursorStyle, Entity, EventEmitter, FocusHandle, Focusable, Hsla,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, Point, Render, RenderImage, Rgba, ScrollWheelEvent,
    Styled, Subscription, Window, canvas, div, point, prelude::FluentBuilder, px,
};
use image::{Frame, RgbaImage};

use crate::document::{Load, PdfDocument, TileKey, Wanted};
use crate::pages::{Layout, MAX_ZOOM, MIN_ZOOM, Zoom, clamp_scroll, scroll_keeping};
use crate::pdf::{Link, LinkTarget, OutlineItem, PageText, PdfError, Region};
use crate::{ZoomAction, zoom_label};

const AHEAD: usize = 1;

pub enum PdfEvent {
    OpenAddress(String),
}

type Inverse = ((usize, usize, usize), [f32; 4], Arc<RenderImage>);

pub struct Find {
    pub query: String,
    matches: Vec<(usize, usize, usize)>,
    current: Option<usize>,
    searched: HashSet<usize>,
    waiting: Option<bool>,
}

struct Selection {
    anchor: (usize, usize),
    head: (usize, usize),
}

impl Selection {
    fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    fn on_page(&self, page: usize, len: usize) -> Option<(usize, usize)> {
        let (start, end) = self.ordered();
        if page < start.0 || page > end.0 {
            return None;
        }
        let from = if page == start.0 { start.1 } else { 0 };
        let to = if page == end.0 { end.1 } else { len };
        (to > from).then_some((from, to))
    }
}

#[derive(Clone, Default)]
struct Bar(Rc<BarState>);

#[derive(Default)]
struct BarState {
    top: Cell<f32>,
    max: Cell<f32>,
    set: Cell<Option<f32>>,
    dragging: Cell<bool>,
}

impl focus_kit::ScrollSource for Bar {
    fn top(&self) -> Pixels {
        px(self.0.top.get())
    }
    fn max(&self) -> Pixels {
        px(self.0.max.get())
    }
    fn set_top(&self, top: Pixels) {
        let top = f32::from(top).clamp(0., self.0.max.get());
        self.0.top.set(top);
        self.0.set.set(Some(top));
    }
    fn drag_started(&self) {
        self.0.dragging.set(true);
    }
    fn drag_ended(&self) {
        self.0.dragging.set(false);
    }
}

struct Press {
    at: Point<Pixels>,
    page: Option<usize>,
    point: (f32, f32),
    moved: bool,
}

pub struct PdfView {
    path: PathBuf,
    focus: FocusHandle,
    document: Entity<PdfDocument>,
    load: Load,
    sizes: Vec<(f32, f32)>,
    zoom: Zoom,
    scroll: (f32, f32),
    area: crate::Area,
    scale_factor: f32,
    more: bool,
    document_busy: bool,
    texts: HashMap<usize, Arc<PageText>>,
    links: HashMap<usize, Arc<Vec<Link>>>,
    outline: Option<Arc<Vec<OutlineItem>>>,
    find: Option<Find>,
    selection: Option<Selection>,
    press: Option<Press>,
    pending: Option<(usize, Option<f32>)>,
    inverse: Option<Inverse>,
    bar: Bar,
    _observe: Subscription,
}

impl EventEmitter<PdfEvent> for PdfView {}

impl Focusable for PdfView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl PdfView {
    pub fn new(path: PathBuf, page: Option<usize>, cx: &mut Context<Self>) -> Self {
        let document = crate::document::open(&path, cx);
        let observe = cx.observe(&document, |view, _, cx| {
            view.sync(cx);
            cx.notify();
        });
        let mut view = PdfView {
            path,
            focus: cx.focus_handle(),
            document,
            load: Load::Opening,
            sizes: Vec::new(),
            zoom: Zoom::FitWidth,
            scroll: (0., 0.),
            area: Rc::new(Cell::new((Point::default(), (0., 0.)))),
            scale_factor: 1.,
            more: false,
            document_busy: true,
            texts: HashMap::new(),
            links: HashMap::new(),
            outline: None,
            find: None,
            selection: None,
            press: None,
            pending: page.map(|p| (p, None)),
            inverse: None,
            bar: Bar::default(),
            _observe: observe,
        };
        view.sync(cx);
        view
    }

    fn sync(&mut self, cx: &mut Context<Self>) {
        let document = self.document.read(cx);
        self.load = document.load();
        self.document_busy = !document.idle();
        if self.sizes.is_empty() {
            self.sizes = document.sizes().to_vec();
        }
        if self.outline.is_none() {
            self.outline = document.outline();
        }
        let links: Vec<(usize, Arc<Vec<Link>>)> = (0..self.sizes.len())
            .filter(|p| !self.links.contains_key(p))
            .filter_map(|p| document.links(p).map(|l| (p, l.clone())))
            .collect();
        let mut texts: Vec<(usize, Arc<PageText>)> = document
            .texts()
            .filter(|(p, _)| !self.texts.contains_key(p))
            .map(|(p, t)| (*p, t.clone()))
            .collect();
        texts.sort_by_key(|(p, _)| *p);
        self.links.extend(links);
        for (page, text) in texts {
            self.searched(page, &text);
            self.texts.insert(page, text);
        }
        if let Some(forward) = self
            .find
            .as_mut()
            .and_then(|f| (!f.matches.is_empty()).then(|| f.waiting.take()).flatten())
        {
            self.find_next(forward, cx);
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn error(&self) -> Option<PdfError> {
        match self.load {
            Load::Failed(error) => Some(error),
            _ => None,
        }
    }

    pub fn busy(&self) -> bool {
        match self.load {
            Load::Opening => true,
            Load::Failed(_) => false,
            Load::Ready => self.document_busy || self.more || self.pending.is_some(),
        }
    }

    pub fn page_count(&self) -> usize {
        self.sizes.len()
    }

    pub fn outline(&self) -> Option<Arc<Vec<OutlineItem>>> {
        self.outline.clone()
    }

    fn layout(&self) -> Layout {
        let (_, area) = self.area.get();
        let page = self.current_page().unwrap_or(0);
        Layout::new(&self.sizes, self.zoom.resolve(&self.sizes, area, page))
    }

    pub fn current_page(&self) -> Option<usize> {
        if self.sizes.is_empty() {
            return None;
        }
        let (_, area) = self.area.get();
        let zoom = match self.zoom {
            Zoom::Fixed(z) => z,
            other => other.resolve(&self.sizes, area, 0),
        };
        Some(Layout::new(&self.sizes, zoom).page_at(self.scroll.1 + area.1 / 2.))
    }

    pub fn zoom_percent(&self) -> i64 {
        (self.layout().zoom * 100.).round() as i64
    }

    pub fn go_to(&mut self, page: usize, top: Option<f32>, cx: &mut Context<Self>) {
        if !matches!(self.load, Load::Ready) || self.area.get().1.0 <= 0. {
            self.pending = Some((page, top));
            return;
        }
        let layout = self.layout();
        let page = page.min(self.sizes.len().saturating_sub(1));
        let y = match top {
            Some(top) => layout.rect(page).1 + top * layout.zoom,
            None => layout.top(page),
        };
        self.scroll = clamp_scroll(&layout, (self.scroll.0, y), self.area.get().1);
        cx.notify();
    }

    pub fn zoom(&mut self, action: ZoomAction, cx: &mut Context<Self>) {
        let (_, area) = self.area.get();
        let center = (area.0 / 2., area.1 / 2.);
        match action {
            ZoomAction::In => self.zoom_around(self.layout().zoom * crate::canvas::STEP, center),
            ZoomAction::Out => self.zoom_around(self.layout().zoom / crate::canvas::STEP, center),
            ZoomAction::Percent(percent) => self.zoom_around(percent / 100., center),
            ZoomAction::Fit => self.set_zoom(Zoom::FitPage),
            ZoomAction::FitWidth => self.set_zoom(Zoom::FitWidth),
        }
        cx.notify();
    }

    fn set_zoom(&mut self, zoom: Zoom) {
        let page = self.current_page().unwrap_or(0);
        self.zoom = zoom;
        let layout = self.layout();
        self.scroll = clamp_scroll(&layout, (0., layout.top(page)), self.area.get().1);
    }

    fn zoom_around(&mut self, zoom: f32, pointer: (f32, f32)) {
        let (_, area) = self.area.get();
        let before = self.layout();
        self.zoom = Zoom::Fixed(zoom.clamp(MIN_ZOOM, MAX_ZOOM));
        let after = self.layout();
        self.scroll = scroll_keeping(&before, &after, self.scroll, pointer, area);
    }

    pub fn scroll_pixels(&mut self, dy: f32, cx: &mut Context<Self>) {
        self.scroll_by(0., dy);
        cx.notify();
    }

    fn scroll_by(&mut self, dx: f32, dy: f32) {
        let layout = self.layout();
        self.scroll = clamp_scroll(
            &layout,
            (self.scroll.0 + dx, self.scroll.1 + dy),
            self.area.get().1,
        );
    }

    fn wanted_tile(&self, layout: &Layout, page: usize, scale: f32) -> (TileKey, Region) {
        let key_scale = (scale * 1000.).round() as u32;
        let size = self.sizes[page];
        if let Some(region) = Region::whole(size, scale) {
            return (
                (page, key_scale, [0, 0, region.width, region.height]),
                region,
            );
        }
        let (_, area) = self.area.get();
        let (x, y, _, _) = layout.rect(page);
        let left = layout.left(area.0);
        let ratio = scale / layout.zoom;
        let x0 = ((self.scroll.0 - left - x) * ratio).max(0.);
        let y0 = ((self.scroll.1 - y) * ratio).max(0.);
        let snap = 512.;
        let x0 = (x0 / snap).floor() * snap;
        let y0 = (y0 / snap).floor() * snap;
        let full = ((size.0 * scale).ceil(), (size.1 * scale).ceil());
        let side = crate::pdf::MAX_BITMAP_SIDE as f32;
        let width = (area.0 * self.scale_factor + snap * 2.)
            .min(side)
            .min(full.0 - x0)
            .max(1.);
        let height = (area.1 * self.scale_factor + snap * 2.)
            .min(side)
            .min(full.1 - y0)
            .max(1.);
        let region = Region {
            scale,
            x: x0 as u32,
            y: y0 as u32,
            width: width as u32,
            height: height as u32,
        };
        (
            (
                page,
                key_scale,
                [region.x, region.y, region.width, region.height],
            ),
            region,
        )
    }

    fn next_request(&self, document: &PdfDocument) -> Option<Wanted> {
        let (_, area) = self.area.get();
        if area.0 <= 0. || self.sizes.is_empty() {
            return None;
        }
        let layout = self.layout();
        let scale = layout.zoom * self.scale_factor;
        let visible = layout.visible(self.scroll.1, area.1);
        let render = |page: usize| {
            if document.failed(page) {
                return None;
            }
            let (key, region) = self.wanted_tile(&layout, page, scale);
            (!document.has_tile(&key)).then_some(Wanted::Render(key, region))
        };
        visible
            .clone()
            .find_map(render)
            .or_else(|| {
                visible
                    .clone()
                    .find(|p| !self.links.contains_key(p))
                    .map(Wanted::Links)
            })
            .or_else(|| {
                let before = visible.start.saturating_sub(AHEAD)..visible.start;
                let after = visible.end..(visible.end + AHEAD).min(self.sizes.len());
                after.chain(before.rev()).find_map(render)
            })
            .or_else(|| {
                let find = self.find.as_ref()?;
                let start = visible.start;
                (start..self.sizes.len())
                    .chain(0..start)
                    .find(|p| !find.searched.contains(p))
                    .map(Wanted::Text)
            })
    }

    fn pump(&mut self, cx: &mut Context<Self>) {
        if self.load != Load::Ready {
            return;
        }
        if self.bar.0.dragging.get() {
            self.more = true;
            return;
        }
        let document = self.document.read(cx);
        let wanted = self.next_request(document);
        self.more = wanted.is_some();
        if let Some(wanted) = wanted
            && document.idle()
        {
            self.document_busy = true;
            self.document
                .update(cx, |document, cx| document.ask(wanted, cx));
        }
    }

    fn searched(&mut self, page: usize, text: &PageText) {
        let Some(find) = &mut self.find else { return };
        if !find.searched.insert(page) {
            return;
        }
        let found = text.find(&find.query);
        let at = find.matches.partition_point(|m| m.0 < page);
        let added = found.len();
        find.matches
            .splice(at..at, found.into_iter().map(|(s, e)| (page, s, e)));
        if let Some(current) = &mut find.current
            && *current >= at
        {
            *current += added;
        }
    }

    pub fn find(&mut self, query: &str, cx: &mut Context<Self>) {
        if query.is_empty() {
            self.find = None;
            self.clear_inverse(cx);
            cx.notify();
            return;
        }
        let mut find = Find {
            query: query.to_string(),
            matches: Vec::new(),
            current: None,
            searched: HashSet::new(),
            waiting: None,
        };
        let mut known: Vec<usize> = self.texts.keys().copied().collect();
        known.sort();
        for page in known {
            let found = self.texts[&page].find(query);
            find.matches
                .extend(found.into_iter().map(|(s, e)| (page, s, e)));
            find.searched.insert(page);
        }
        self.find = Some(find);
        self.clear_inverse(cx);
        self.pump(cx);
        cx.notify();
    }

    pub fn close_find(&mut self, cx: &mut Context<Self>) {
        self.find = None;
        self.clear_inverse(cx);
        cx.notify();
    }

    pub fn find_next(&mut self, forward: bool, cx: &mut Context<Self>) {
        let page = self.current_page().unwrap_or(0);
        let Some(find) = &mut self.find else { return };
        if find.matches.is_empty() {
            find.waiting = Some(forward);
            return;
        }
        let count = find.matches.len();
        let next = match find.current {
            Some(current) if forward => (current + 1) % count,
            Some(current) => (current + count - 1) % count,
            None if forward => find.matches.iter().position(|m| m.0 >= page).unwrap_or(0),
            None => find
                .matches
                .iter()
                .rposition(|m| m.0 <= page)
                .unwrap_or(count - 1),
        };
        find.current = Some(next);
        let (page, start, end) = find.matches[next];
        self.clear_inverse(cx);
        self.reveal(page, start, end, cx);
    }

    fn reveal(&mut self, page: usize, start: usize, end: usize, cx: &mut Context<Self>) {
        let Some(text) = self.texts.get(&page).cloned() else {
            return;
        };
        let Some(rect) = text.rects(start, end).first().copied() else {
            return;
        };
        let layout = self.layout();
        let (_, area) = self.area.get();
        let (x, y, _, _) = layout.rect(page);
        let top = y + rect[1] * layout.zoom;
        let bottom = y + rect[3] * layout.zoom;
        if top < self.scroll.1 || bottom > self.scroll.1 + area.1 {
            self.scroll.1 = top - area.1 / 3.;
        }
        let left = x + rect[0] * layout.zoom;
        if left < self.scroll.0 || left > self.scroll.0 + area.0 {
            self.scroll.0 = left - area.0 / 3.;
        }
        self.scroll = clamp_scroll(&layout, self.scroll, area);
        cx.notify();
    }

    pub fn find_status(&self) -> Option<(usize, usize)> {
        let find = self.find.as_ref()?;
        Some((find.current.map_or(0, |c| c + 1), find.matches.len()))
    }

    pub fn find_done(&self) -> bool {
        self.find
            .as_ref()
            .is_some_and(|f| f.searched.len() >= self.sizes.len())
    }

    pub fn selected_text(&self) -> Option<String> {
        let selection = self.selection.as_ref()?;
        let (start, end) = selection.ordered();
        let mut parts = Vec::new();
        for page in start.0..=end.0 {
            let text = self.texts.get(&page)?;
            if let Some((from, to)) = selection.on_page(page, text.chars.len()) {
                parts.push(text.slice(from, to));
            }
        }
        (!parts.is_empty()).then(|| parts.join("\n"))
    }

    pub fn drag_scrollbar(&mut self, fraction: f32, cx: &mut Context<Self>) -> usize {
        use focus_kit::ScrollSource as _;
        let before = self.document.read(cx).asked_total();
        self.bar.drag_started();
        self.bar
            .set_top(px(fraction.clamp(0., 1.) * self.bar.0.max.get()));
        if let Some(top) = self.bar.0.set.take() {
            self.scroll.1 = top;
        }
        self.pump(cx);
        let during = self.document.read(cx).asked_total() - before;
        self.bar.drag_ended();
        cx.notify();
        during
    }

    pub fn select(&mut self, anchor: (usize, usize), head: (usize, usize), cx: &mut Context<Self>) {
        self.selection = Some(Selection { anchor, head });
        cx.notify();
    }

    fn hit(&self, at: Point<Pixels>) -> Option<(usize, (f32, f32))> {
        let (origin, area) = self.area.get();
        let layout = self.layout();
        let local = (
            f32::from(at.x - origin.x) + self.scroll.0 - layout.left(area.0),
            f32::from(at.y - origin.y) + self.scroll.1,
        );
        let page = layout.page_at(local.1);
        let (x, y, w, h) = layout.rect(page);
        let inside = local.0 >= x && local.0 <= x + w && local.1 >= y && local.1 <= y + h;
        inside.then(|| {
            (
                page,
                ((local.0 - x) / layout.zoom, (local.1 - y) / layout.zoom),
            )
        })
    }

    fn link_at(&self, at: Point<Pixels>) -> Option<LinkTarget> {
        let (page, (x, y)) = self.hit(at)?;
        self.links
            .get(&page)?
            .iter()
            .find(|l| x >= l.rect[0] && x <= l.rect[2] && y >= l.rect[1] && y <= l.rect[3])
            .map(|l| l.target.clone())
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        let hit = self.hit(event.position);
        self.press = Some(Press {
            at: event.position,
            page: hit.map(|h| h.0),
            point: hit.map_or((0., 0.), |h| h.1),
            moved: false,
        });
        self.selection = None;
        if let Some((page, point)) = hit {
            match self.texts.get(&page) {
                Some(text) => {
                    if let Some(index) = text.index_at(point) {
                        let (anchor, head) = if event.click_count >= 2 {
                            word_around(text, index)
                        } else {
                            (index, index)
                        };
                        self.selection = Some(Selection {
                            anchor: (page, anchor),
                            head: (page, head),
                        });
                    }
                }
                None => self.ask_text(page, cx),
            }
        }
        cx.notify();
    }

    fn ask_text(&mut self, page: usize, cx: &mut Context<Self>) {
        self.document
            .update(cx, |document, cx| document.ask(Wanted::Text(page), cx));
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(press) = &mut self.press else { return };
        if event.pressed_button != Some(MouseButton::Left) {
            self.press = None;
            return;
        }
        let distance =
            (event.position.x - press.at.x).abs() + (event.position.y - press.at.y).abs();
        if distance > px(3.) {
            press.moved = true;
        }
        if press.page.is_none() {
            return;
        }
        let (origin, _) = self.area.get();
        let layout = self.layout();
        let y = f32::from(event.position.y - origin.y) + self.scroll.1;
        let page = layout.page_at(y.clamp(0., layout.height));
        let Some((_, point)) = self.hit_clamped(event.position, page) else {
            return;
        };
        let Some(text) = self.texts.get(&page).cloned() else {
            self.ask_text(page, cx);
            return;
        };
        let Some(index) = text.index_at(point) else {
            return;
        };
        if self.selection.is_none()
            && let Some(press) = &self.press
            && let Some(start) = press.page
            && let Some(first) = self.texts.get(&start)
        {
            let anchor = first.index_at(press.point).unwrap_or(0);
            self.selection = Some(Selection {
                anchor: (start, anchor),
                head: (page, index),
            });
        } else if let Some(selection) = &mut self.selection {
            selection.head = (page, index);
        }
        cx.notify();
    }

    fn hit_clamped(&self, at: Point<Pixels>, page: usize) -> Option<(usize, (f32, f32))> {
        let (origin, area) = self.area.get();
        let layout = self.layout();
        let (x, y, w, h) = layout.rect(page);
        let local = (
            f32::from(at.x - origin.x) + self.scroll.0 - layout.left(area.0),
            f32::from(at.y - origin.y) + self.scroll.1,
        );
        Some((
            page,
            (
                (local.0.clamp(x, x + w) - x) / layout.zoom,
                (local.1.clamp(y, y + h) - y) / layout.zoom,
            ),
        ))
    }

    fn mouse_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(press) = self.press.take() else {
            return;
        };
        if press.moved {
            return;
        }
        match self.link_at(event.position) {
            Some(LinkTarget::Page { page, top }) => {
                self.selection = None;
                self.go_to(page, top, cx);
            }
            Some(LinkTarget::Uri(uri)) => {
                self.selection = None;
                cx.emit(PdfEvent::OpenAddress(uri));
            }
            None => {}
        }
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let (_, area) = self.area.get();
        let keystroke = &event.keystroke;
        if keystroke.modifiers.control || keystroke.modifiers.platform {
            if keystroke.key == "c"
                && let Some(text) = self.selected_text()
            {
                focus_kit::clipboard::copy_text(cx, text);
                cx.stop_propagation();
            }
            return;
        }
        let page = self.current_page().unwrap_or(0);
        let line = 48.;
        match keystroke.key.as_str() {
            "pagedown" => self.go_to((page + 1).min(self.sizes.len().saturating_sub(1)), None, cx),
            "pageup" => {
                let layout = self.layout();
                let target = if self.scroll.1 > layout.top(page) + 1. {
                    page
                } else {
                    page.saturating_sub(1)
                };
                self.go_to(target, None, cx)
            }
            "home" => self.go_to(0, None, cx),
            "end" => self.go_to(self.sizes.len().saturating_sub(1), None, cx),
            "down" => self.scroll_by(0., line),
            "up" => self.scroll_by(0., -line),
            "right" => self.scroll_by(line, 0.),
            "left" => self.scroll_by(-line, 0.),
            "space" if keystroke.modifiers.shift => self.scroll_by(0., -area.1 * 0.9),
            "space" => self.scroll_by(0., area.1 * 0.9),
            "+" | "=" => self.zoom(ZoomAction::In, cx),
            "-" => self.zoom(ZoomAction::Out, cx),
            "0" => self.zoom(ZoomAction::Fit, cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn wheel(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(window.line_height());
        if event.modifiers.control {
            let (origin, _) = self.area.get();
            let pointer = (
                f32::from(event.position.x - origin.x),
                f32::from(event.position.y - origin.y),
            );
            let factor = (f32::from(delta.y) / 200.).exp();
            self.zoom_around(self.layout().zoom * factor, pointer);
        } else {
            let (dx, dy) = if event.modifiers.shift {
                (-f32::from(delta.y), 0.)
            } else {
                (-f32::from(delta.x), -f32::from(delta.y))
            };
            self.scroll_by(dx, dy);
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn current_inverse(
        &mut self,
        layout: &Layout,
        cx: &mut Context<Self>,
    ) -> Option<([f32; 4], Arc<RenderImage>)> {
        let find = self.find.as_ref()?;
        let current = find.matches.get(find.current?).copied()?;
        if let Some((key, rect, image)) = &self.inverse
            && *key == current
        {
            return Some((*rect, image.clone()));
        }
        let (page, start, end) = current;
        let rect = *self.texts.get(&page)?.rects(start, end).first()?;
        let scale = layout.zoom * self.scale_factor;
        let key_scale = (scale * 1000.).round() as u32;
        let (key, image) = self
            .document
            .read(cx)
            .tiles(|k| k.0 == page && k.1 == key_scale)
            .into_iter()
            .next()?;
        let bytes = image.as_bytes(0)?;
        let width = key.2[2];
        let x0 = ((rect[0] * scale) as u32).saturating_sub(key.2[0]);
        let y0 = ((rect[1] * scale) as u32).saturating_sub(key.2[1]);
        let x1 = ((rect[2] * scale).ceil() as u32)
            .saturating_sub(key.2[0])
            .min(width);
        let y1 = ((rect[3] * scale).ceil() as u32)
            .saturating_sub(key.2[1])
            .min(key.2[3]);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let paper = Tokens::of(Mode::Light);
        let (ground, ink) = (paper.btn.to_rgb(), paper.btn_ink.to_rgb());
        let inverse = |source: u8, ground: f32, ink: f32| {
            let lightness = f32::from(source) / 255.;
            ((ink + (ground - ink) * lightness) * 255.).round() as u8
        };
        let mut cut = RgbaImage::new(x1 - x0, y1 - y0);
        for (x, y, pixel) in cut.enumerate_pixels_mut() {
            let i = (((y0 + y) * width + x0 + x) * 4) as usize;
            let source = bytes.get(i..i + 4)?;
            pixel.0 = [
                inverse(source[0], ground.b, ink.b),
                inverse(source[1], ground.g, ink.g),
                inverse(source[2], ground.r, ink.r),
                255,
            ];
        }
        let shown = [
            (x0 + key.2[0]) as f32 / scale,
            (y0 + key.2[1]) as f32 / scale,
            (x1 + key.2[0]) as f32 / scale,
            (y1 + key.2[1]) as f32 / scale,
        ];
        let image = Arc::new(RenderImage::new(vec![Frame::new(cut)]));
        self.clear_inverse(cx);
        self.inverse = Some((current, shown, image.clone()));
        Some((shown, image))
    }
}

fn under_ink(color: Hsla) -> Hsla {
    let target = color.to_rgb();
    let alpha = 1. - target.r.min(target.g).min(target.b);
    if alpha <= 0. {
        return color.opacity(0.);
    }
    let channel = |c: f32| ((c - (1. - alpha)) / alpha).clamp(0., 1.);
    Rgba {
        r: channel(target.r),
        g: channel(target.g),
        b: channel(target.b),
        a: alpha,
    }
    .into()
}

fn word_around(text: &PageText, index: usize) -> (usize, usize) {
    let is_word = |i: usize| text.chars.get(i).is_some_and(|c| c.is_alphanumeric());
    let mut start = index.min(text.chars.len());
    while start > 0 && is_word(start - 1) {
        start -= 1;
    }
    let mut end = index;
    while is_word(end) {
        end += 1;
    }
    (start, end)
}

enum Draw {
    Sheet(Bounds<Pixels>),
    Image(Bounds<Pixels>, Arc<RenderImage>),
    Fill(Bounds<Pixels>, gpui::Hsla),
}

fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), gpui::size(px(w), px(h)))
}

impl Render for PdfView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        self.scale_factor = window.scale_factor();
        if let Some(top) = self.bar.0.set.take() {
            self.scroll.1 = top;
        }
        if let Some((page, top)) = self.pending
            && matches!(self.load, Load::Ready)
            && self.area.get().1.0 > 0.
        {
            self.pending = None;
            self.go_to(page, top, cx);
        }
        self.pump(cx);
        let draws = self.draws(cx);
        let (_, area_size) = self.area.get();
        let layout = self.layout();
        self.bar.0.top.set(self.scroll.1);
        self.bar.0.max.set((layout.height - area_size.1).max(0.));
        let area = self.area.clone();
        let failed = self.error();
        let ready = matches!(self.load, Load::Ready);
        let percent = self.zoom_percent();
        let button = |id: &'static str, glyph: Icon, key: &str, keys: &str| {
            IconButton::new(id, glyph, focus_i18n::t(key)).shortcut(keys.to_string())
        };
        div()
            .id("pdf-view")
            .key_context("FocusFileViewer")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(t.bg)
            .cursor(CursorStyle::IBeam)
            .on_scroll_wheel(cx.listener(|this, event, window, cx| this.wheel(event, window, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, window, cx| this.mouse_down(event, window, cx)),
            )
            .on_mouse_move(cx.listener(|this, event, _, cx| this.mouse_move(event, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.mouse_up(event, cx)),
            )
            .on_key_down(cx.listener(|this, event, _, cx| this.key_down(event, cx)))
            .child(
                canvas(
                    move |bounds, window, _| {
                        let next = (
                            bounds.origin,
                            (f32::from(bounds.size.width), f32::from(bounds.size.height)),
                        );
                        if area.get() != next {
                            area.set(next);
                            window.request_animation_frame();
                        }
                    },
                    move |bounds, _, window, _| {
                        let shift =
                            |b: Bounds<Pixels>| Bounds::new(b.origin + bounds.origin, b.size);
                        for draw in draws {
                            match draw {
                                Draw::Sheet(b) => window.paint_quad(gpui::outline(
                                    shift(b),
                                    t.line,
                                    gpui::BorderStyle::Solid,
                                )),
                                Draw::Image(b, image) => {
                                    if let Err(error) = window.paint_image(
                                        shift(b),
                                        gpui::Corners::default(),
                                        image,
                                        0,
                                        false,
                                    ) {
                                        eprintln!("focus: painting a PDF page: {error}");
                                    }
                                }
                                Draw::Fill(b, color) => {
                                    window.paint_quad(gpui::fill(shift(b), color))
                                }
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .when_some(failed, |this, error| {
                this.child(
                    div()
                        .absolute()
                        .top(size::S8)
                        .w_full()
                        .flex()
                        .justify_center()
                        .px(size::S6)
                        .ui()
                        .text_color(t.danger)
                        .child(pdf_error_text(error)),
                )
            })
            .when(ready, |this| {
                this.child(
                    focus_kit::Dir::current()
                        .end(div().absolute(), size::S3)
                        .bottom(size::S3)
                        .occlude()
                        .child(
                            ZoomControls::new(
                                zoom_label(percent),
                                button("pdf-zoom-out", Icon::MINUS, "reading-zoom-out", "-")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.zoom(ZoomAction::Out, cx)
                                        }),
                                    ),
                                button("pdf-zoom-in", Icon::PLUS, "reading-zoom-in", "+").on_click(
                                    cx.listener(|this, _, _, cx| this.zoom(ZoomAction::In, cx)),
                                ),
                                button("pdf-zoom-fit", Icon::MAXIMIZE, "media-zoom-fit-page", "0")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.zoom(ZoomAction::Fit, cx)
                                        }),
                                    ),
                            )
                            .fit_width(
                                IconButton::new(
                                    "pdf-zoom-width",
                                    Icon::FIT_WIDTH,
                                    focus_i18n::t("media-zoom-fit-width"),
                                )
                                .active(self.zoom == Zoom::FitWidth)
                                .on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.zoom(ZoomAction::FitWidth, cx)
                                    }),
                                ),
                            ),
                        ),
                )
                .child(focus_kit::scrollbar("pdf-scrollbar", self.bar.clone()))
            })
            .into_any_element()
    }
}

impl PdfView {
    fn draws(&mut self, cx: &mut Context<Self>) -> Vec<Draw> {
        let (_, area) = self.area.get();
        if !matches!(self.load, Load::Ready) || area.0 <= 0. {
            return Vec::new();
        }
        let paper = Tokens::of(Mode::Light);
        let found = under_ink(paper.card_3);
        let layout = self.layout();
        let left = layout.left(area.0) - self.scroll.0;
        let scale = layout.zoom * self.scale_factor;
        let key_scale = (scale * 1000.).round() as u32;
        let inverse = self.current_inverse(&layout, cx);
        let mut draws = Vec::new();
        for page in layout.visible(self.scroll.1, area.1) {
            let (x, y, w, h) = layout.rect(page);
            let (x, y) = (x + left, y - self.scroll.1);
            draws.push(Draw::Sheet(bounds(x - 1., y - 1., w + 2., h + 2.)));
            let document = self.document.read(cx);
            let mut tiles = document.tiles(|k| k.0 == page && k.1 == key_scale);
            if tiles.is_empty() {
                tiles = document.tiles(|k| k.0 == page && k.2[0] == 0 && k.2[1] == 0);
                tiles.truncate(1);
            }
            let keys: Vec<TileKey> = tiles.iter().map(|(k, _)| *k).collect();
            for (key, image) in keys.iter().zip(tiles.into_iter().map(|(_, v)| v)) {
                let tile_scale = key.1 as f32 / 1000.;
                let ratio = layout.zoom / tile_scale;
                draws.push(Draw::Image(
                    bounds(
                        x + key.2[0] as f32 * ratio,
                        y + key.2[1] as f32 * ratio,
                        key.2[2] as f32 * ratio,
                        key.2[3] as f32 * ratio,
                    ),
                    image,
                ));
            }
            self.document.update(cx, |document, _| {
                for key in &keys {
                    document.tile(key);
                }
            });
            let to_area = |r: [f32; 4]| {
                bounds(
                    x + r[0] * layout.zoom,
                    y + r[1] * layout.zoom,
                    (r[2] - r[0]) * layout.zoom,
                    (r[3] - r[1]) * layout.zoom,
                )
            };
            if let (Some(find), Some(text)) = (&self.find, self.texts.get(&page)) {
                let current = find.current.and_then(|c| find.matches.get(c)).copied();
                for m in find.matches.iter().filter(|m| m.0 == page) {
                    if Some(*m) == current {
                        continue;
                    }
                    for r in text.rects(m.1, m.2) {
                        draws.push(Draw::Fill(to_area(r), found));
                        let line = to_area(r);
                        draws.push(Draw::Fill(
                            bounds(
                                f32::from(line.origin.x),
                                f32::from(line.origin.y + line.size.height) - 1.,
                                f32::from(line.size.width),
                                1.,
                            ),
                            paper.ink_3,
                        ));
                    }
                }
                if let (Some(m), Some((rect, image))) = (current, &inverse)
                    && m.0 == page
                {
                    draws.push(Draw::Image(to_area(*rect), image.clone()));
                }
            }
            if let Some(selection) = &self.selection
                && let Some(text) = self.texts.get(&page)
                && let Some((start, end)) = selection.on_page(page, text.chars.len())
            {
                for r in text.rects(start, end) {
                    draws.push(Draw::Fill(to_area(r), paper.sel));
                }
            }
        }
        draws
    }

    pub fn release(&mut self, cx: &mut App) {
        self.clear_inverse(cx);
    }

    fn clear_inverse(&mut self, cx: &mut App) {
        if let Some((_, _, image)) = self.inverse.take() {
            cx.drop_image(image, None);
        }
    }
}

pub fn pdf_error_text(error: PdfError) -> String {
    focus_i18n::t(match error {
        PdfError::Protected => "media-pdf-protected",
        PdfError::UnsupportedEncryption => "media-pdf-encryption",
        PdfError::Unreadable => "media-pdf-unreadable",
        PdfError::TooLarge => "media-pdf-too-large",
        PdfError::WorkerStopped => "media-pdf-stopped",
    })
}

#[cfg(test)]
mod tests {
    use super::{Selection, under_ink};
    use focus_kit::{Mode, Tokens};

    #[test]
    fn a_match_on_white_paper_shows_card_3_and_keeps_black_ink_black() {
        let card_3 = Tokens::of(Mode::Light).card_3.to_rgb();
        let veil = under_ink(Tokens::of(Mode::Light).card_3).to_rgb();
        let alpha = veil.a;
        let over = |paper: f32, ink: f32| paper * (1. - alpha) + ink * alpha;
        let byte = |c: f32| (c * 255.).round() as i32;
        assert_eq!(byte(over(1., veil.r)), byte(card_3.r));
        assert_eq!(byte(over(1., veil.g)), byte(card_3.g));
        assert_eq!(byte(over(1., veil.b)), byte(card_3.b));
        assert!(byte(over(0., veil.r)) <= 2 && byte(over(0., veil.b)) <= 2);
    }

    #[test]
    fn a_selection_over_three_pages_takes_the_middle_page_whole() {
        let selection = Selection {
            anchor: (4, 30),
            head: (2, 10),
        };
        assert_eq!(selection.on_page(1, 100), None);
        assert_eq!(selection.on_page(2, 100), Some((10, 100)));
        assert_eq!(selection.on_page(3, 80), Some((0, 80)));
        assert_eq!(selection.on_page(4, 100), Some((0, 30)));
        assert_eq!(selection.on_page(5, 100), None);
        let empty = Selection {
            anchor: (1, 5),
            head: (1, 5),
        };
        assert_eq!(empty.on_page(1, 100), None);
    }
}
