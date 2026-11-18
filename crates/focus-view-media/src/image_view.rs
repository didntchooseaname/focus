use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use focus_kit::{ActiveTokens, Icon, IconButton, Type, ZoomControls, size};
use gpui::{
    App, AppContext as _, Context, CursorStyle, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    ParentElement, Pixels, Point, Render, RenderImage, ScrollWheelEvent, Styled, Task, Window,
    canvas, div, img, prelude::FluentBuilder, px,
};
use image::Frame;

use crate::canvas::{Canvas, STEP};
use crate::raster::{self, DecodeError, Kind};
use crate::{ZoomAction, zoom_label};

pub const MARGIN: f32 = 24.;
const MAX_DECODED: u32 = 8192;

pub enum ImageEvent {
    Neighbor { forward: bool },
    Copied,
    CopyFailed(String),
}

enum Load {
    Loading,
    Failed(String),
    Ready,
}

pub struct ImageView {
    path: PathBuf,
    kind: Kind,
    focus: FocusHandle,
    load: Load,
    pub file: Option<((u32, u32), u64)>,
    image: Option<Arc<RenderImage>>,
    decoded: u32,
    decoding: Option<u32>,
    rotation: u8,
    turn: u8,
    canvas: Canvas,
    area: crate::Area,
    grab: Option<Point<Pixels>>,
    _task: Option<Task<()>>,
    _copy: Option<Task<()>>,
}

impl EventEmitter<ImageEvent> for ImageView {}

impl Focusable for ImageView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn read_error(error: std::io::Error) -> String {
    focus_i18n::t_args(
        "media-image-unreadable",
        &[("error", error.to_string().as_str().into())],
    )
}

fn decode_error(error: &DecodeError) -> String {
    match error {
        DecodeError::Unreadable(reason) => focus_i18n::t_args(
            "media-image-unreadable",
            &[("error", reason.as_str().into())],
        ),
        DecodeError::TooLarge { width, height } => focus_i18n::t_args(
            "media-image-too-large",
            &[("width", (*width).into()), ("height", (*height).into())],
        ),
    }
}

impl ImageView {
    pub fn new(path: PathBuf, longest: u32, cx: &mut Context<Self>) -> Self {
        let kind = raster::Kind::of(&path).unwrap_or(Kind::Image);
        let mut view = ImageView {
            path,
            kind,
            focus: cx.focus_handle(),
            load: Load::Loading,
            file: None,
            image: None,
            decoded: 0,
            decoding: None,
            rotation: 0,
            turn: 0,
            canvas: Canvas::default(),
            area: Rc::new(Cell::new((Point::default(), (0., 0.)))),
            grab: None,
            _task: None,
            _copy: None,
        };
        view.decode(longest.max(1), 0, cx);
        view
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn busy(&self) -> bool {
        self.decoding.is_some()
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.load, Load::Ready)
    }

    pub fn error(&self) -> Option<&str> {
        match &self.load {
            Load::Failed(error) => Some(error),
            _ => None,
        }
    }

    fn content(&self) -> (f32, f32) {
        let ((w, h), _) = self.file.unwrap_or(((1, 1), 0));
        if self.rotation % 2 == 1 {
            (h as f32, w as f32)
        } else {
            (w as f32, h as f32)
        }
    }

    fn decode(&mut self, longest: u32, rotation: u8, cx: &mut Context<Self>) {
        let (path, kind) = (self.path.clone(), self.kind);
        self.decoding = Some(longest);
        let task = cx.background_spawn(async move {
            let bytes = std::fs::read(&path).map_err(read_error)?;
            let weight = bytes.len() as u64;
            let hint = raster::format_hint(&path);
            let size = raster::dimensions(&bytes, kind, hint).map_err(|e| decode_error(&e))?;
            let picture = raster::decode(&bytes, kind, hint, longest)
                .map_err(|e| decode_error(&e))?
                .rotated(rotation);
            Ok::<_, String>((size, weight, picture))
        });
        self._task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                view.decoding = None;
                match result {
                    Ok((size, weight, picture)) => {
                        view.file = Some((size, weight));
                        view.decoded = longest;
                        if view.rotation != rotation {
                            view.rotation = rotation;
                            view.canvas.fit();
                        }
                        let image = Arc::new(RenderImage::new(vec![Frame::new(picture.bgra)]));
                        if let Some(old) = view.image.replace(image) {
                            cx.drop_image(old, None);
                        }
                        view.load = Load::Ready;
                    }
                    Err(error) => view.load = Load::Failed(error),
                }
                cx.notify();
            });
        }));
    }

    fn sharpen(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(((w, h), _)) = self.file else { return };
        let (_, area) = self.area.get();
        if area.0 <= 0. || self.decoding.is_some() || !self.is_ready() {
            return;
        }
        let place = self.canvas.placement(area, self.content(), MARGIN);
        let shown = (place.width.max(place.height) * window.scale_factor()).ceil() as u32;
        let full = w.max(h);
        let ceiling = full.min(MAX_DECODED);
        if shown > self.decoded && self.decoded < ceiling {
            let next = shown.next_power_of_two().min(ceiling);
            self.decode(next, self.rotation, cx);
        }
    }

    pub fn zoom(&mut self, action: ZoomAction, cx: &mut Context<Self>) {
        let (_, area) = self.area.get();
        let content = self.content();
        let center = (area.0 / 2., area.1 / 2.);
        match action {
            ZoomAction::In => self.canvas.zoom_by(STEP, center, area, content, MARGIN),
            ZoomAction::Out => self
                .canvas
                .zoom_by(1. / STEP, center, area, content, MARGIN),
            ZoomAction::Fit | ZoomAction::FitWidth => self.canvas.fit(),
            ZoomAction::Percent(percent) => {
                self.canvas
                    .zoom_to(percent / 100., center, area, content, MARGIN)
            }
        }
        cx.notify();
    }

    pub fn zoom_percent(&self) -> i64 {
        let (_, area) = self.area.get();
        (self.canvas.zoom(area, self.content(), MARGIN) * 100.).round() as i64
    }

    pub fn rotate(&mut self, clockwise: bool, cx: &mut Context<Self>) {
        self.turn = (self.turn + if clockwise { 1 } else { 3 }) % 4;
        self.decode(self.decoded.max(1), self.turn, cx);
        cx.notify();
    }

    pub fn rotation(&self) -> u8 {
        self.rotation
    }

    pub fn copy(&mut self, cx: &mut Context<Self>) {
        let (path, kind) = (self.path.clone(), self.kind);
        let task = cx.background_spawn(async move {
            let bytes = std::fs::read(&path).map_err(read_error)?;
            raster::png_for_clipboard(&bytes, kind, raster::format_hint(&path))
                .map_err(|e| decode_error(&e))
        });
        self._copy = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |_, cx| match result {
                Ok(png) => {
                    focus_kit::clipboard::copy_image(cx, png);
                    cx.emit(ImageEvent::Copied);
                }
                Err(error) => cx.emit(ImageEvent::CopyFailed(error)),
            });
        }));
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.control {
            if keystroke.key == "c" {
                self.copy(cx);
                cx.stop_propagation();
            }
            return;
        }
        let rtl = focus_i18n::direction() == focus_i18n::Direction::Rtl;
        match keystroke.key.as_str() {
            "left" => cx.emit(ImageEvent::Neighbor { forward: rtl }),
            "right" => cx.emit(ImageEvent::Neighbor { forward: !rtl }),
            "+" | "=" => self.zoom(ZoomAction::In, cx),
            "-" => self.zoom(ZoomAction::Out, cx),
            "0" => self.zoom(ZoomAction::Fit, cx),
            "1" => self.zoom(ZoomAction::Percent(100.), cx),
            "r" | "R" => self.rotate(!keystroke.modifiers.shift && keystroke.key == "r", cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn local(&self, at: Point<Pixels>) -> (f32, f32) {
        let (origin, _) = self.area.get();
        (f32::from(at.x - origin.x), f32::from(at.y - origin.y))
    }

    fn wheel(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        let (_, area) = self.area.get();
        let content = self.content();
        let delta = event.delta.pixel_delta(window.line_height());
        if event.modifiers.control {
            let factor = (f32::from(delta.y) / 200.).exp();
            let at = self.local(event.position);
            self.canvas.zoom_by(factor, at, area, content, MARGIN);
        } else {
            self.canvas.pan(
                (f32::from(delta.x), f32::from(delta.y)),
                area,
                content,
                MARGIN,
            );
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        if event.click_count >= 2 {
            let (_, area) = self.area.get();
            let content = self.content();
            if self.canvas.is_fitted() {
                let at = self.local(event.position);
                self.canvas.zoom_to(1., at, area, content, MARGIN);
            } else {
                self.canvas.fit();
            }
        } else {
            self.grab = Some(event.position);
        }
        cx.notify();
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(grab) = self.grab else { return };
        if event.pressed_button != Some(MouseButton::Left) {
            self.grab = None;
            cx.notify();
            return;
        }
        let (_, area) = self.area.get();
        let content = self.content();
        let delta = event.position - grab;
        self.canvas.pan(
            (f32::from(delta.x), f32::from(delta.y)),
            area,
            content,
            MARGIN,
        );
        self.grab = Some(event.position);
        cx.notify();
    }

    pub fn release(&mut self, cx: &mut App) {
        if let Some(image) = self.image.take() {
            cx.drop_image(image, None);
        }
    }
}

impl Render for ImageView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        self.sharpen(window, cx);
        let (_, area) = self.area.get();
        let place = self.canvas.placement(area, self.content(), MARGIN);
        let area_cell = self.area.clone();
        let percent = self.zoom_percent();
        let button = |id: &'static str, glyph: Icon, key: &str, keys: &str| {
            IconButton::new(id, glyph, focus_i18n::t(key)).shortcut(keys.to_string())
        };
        div()
            .id("image-view")
            .key_context("FocusFileViewer")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(t.bg)
            .cursor(if self.grab.is_some() {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .on_scroll_wheel(cx.listener(|this, event, window, cx| this.wheel(event, window, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, window, cx| this.mouse_down(event, window, cx)),
            )
            .on_mouse_move(cx.listener(|this, event, _, cx| this.mouse_move(event, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.grab = None;
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(|this, event, _, cx| this.key_down(event, cx)))
            .child(
                canvas(
                    move |bounds, window, _| {
                        let next = (
                            bounds.origin,
                            (f32::from(bounds.size.width), f32::from(bounds.size.height)),
                        );
                        if area_cell.get() != next {
                            area_cell.set(next);
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when_some(self.image.clone().filter(|_| area.0 > 0.), |this, image| {
                this.child(
                    img(image)
                        .absolute()
                        .left(px(place.x))
                        .top(px(place.y))
                        .w(px(place.width))
                        .h(px(place.height)),
                )
            })
            .when_some(self.error().map(str::to_string), |this, error| {
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
                        .child(error),
                )
            })
            .when(self.is_ready(), |this| {
                this.child(
                    focus_kit::Dir::current()
                        .end(div().absolute(), size::S3)
                        .bottom(size::S3)
                        .occlude()
                        .child(ZoomControls::new(
                            zoom_label(percent),
                            button("image-zoom-out", Icon::MINUS, "reading-zoom-out", "-")
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.zoom(ZoomAction::Out, cx)),
                                ),
                            button("image-zoom-in", Icon::PLUS, "reading-zoom-in", "+").on_click(
                                cx.listener(|this, _, _, cx| this.zoom(ZoomAction::In, cx)),
                            ),
                            button("image-zoom-fit", Icon::MAXIMIZE, "reading-zoom-fit", "0")
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.zoom(ZoomAction::Fit, cx)),
                                ),
                        )),
                )
            })
    }
}
