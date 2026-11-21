use std::sync::Arc;

use focus_kit::{Icon, IconButton, Tokens, ZoomControls, size};
use focus_view_media::canvas::{Canvas, STEP};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement, Pixels, Point, RenderImage, ScrollWheelEvent, Styled, Window,
    div, img, point, prelude::FluentBuilder, px,
};

use super::ReadingView;

const MARGIN: f32 = 48.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkPage(pub usize);

pub(super) struct Viewer {
    image: Arc<RenderImage>,
    width: f32,
    height: f32,
    canvas: Canvas,
    grab: Option<Point<Pixels>>,
    press: Option<Point<Pixels>>,
}

fn area(window: &Window) -> (f32, f32) {
    let size = window.viewport_size();
    (f32::from(size.width), f32::from(size.height))
}

fn at(position: Point<Pixels>) -> (f32, f32) {
    (f32::from(position.x), f32::from(position.y))
}

impl Viewer {
    fn content(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    fn zoom_by(&mut self, factor: f32, around: (f32, f32), window: &Window) {
        let content = self.content();
        self.canvas
            .zoom_by(factor, around, area(window), content, MARGIN);
    }
}

impl ReadingView {
    pub(super) fn link_page(&self, position: Point<Pixels>) -> Option<usize> {
        let built = self.built.as_ref()?;
        let source = built.source.as_ref()?;
        let hits = self.registry.0.borrow();
        let hit = hits.iter().find(|h| h.bounds.contains(&position))?;
        let index = hit.layout.index_for_position(position).ok()?;
        let link = hit.rich.links.iter().find(|l| l.range.contains(&index))?;
        let map = hit.map.as_ref()?;
        let item = built.items.get((hit.key.0 as usize).checked_sub(1)?)?;
        let origin = source
            .match_indices('\n')
            .nth((item.lines.start as usize).checked_sub(1)?)
            .map_or(0, |(i, _)| i + 1);
        let at = map.to_source(link.range.start)?;
        let element = map
            .holding(at)
            .map(|m| m.element.clone())
            .min_by_key(|e| e.len())?;
        let markup = source.get(origin + element.start..origin + element.end)?;
        let anchor = markup.split_once('#')?.1;
        let anchor = anchor.split(['|', ']', ')']).next()?;
        focus_view_media::pdf::page_anchor(anchor)
    }

    pub fn viewer_shown(&self) -> bool {
        self.viewer.is_some()
    }

    pub(super) fn viewer_key(&mut self, key: &str, window: &Window) -> bool {
        let Some(viewer) = self.viewer.as_mut() else {
            return false;
        };
        let (w, h) = area(window);
        match key {
            "+" | "=" => viewer.zoom_by(STEP, (w / 2., h / 2.), window),
            "-" => viewer.zoom_by(1. / STEP, (w / 2., h / 2.), window),
            "0" => viewer.canvas.fit(),
            _ => return false,
        }
        true
    }

    pub(super) fn open_viewer(
        &mut self,
        image: Arc<RenderImage>,
        width: f32,
        height: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus);
        self.viewer = Some(Viewer {
            image,
            width,
            height,
            canvas: Canvas::default(),
            grab: None,
            press: None,
        });
        cx.notify();
    }

    pub(super) fn render_viewer(
        &self,
        t: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let viewer = self.viewer.as_ref()?;
        let place = viewer
            .canvas
            .placement(area(window), viewer.content(), MARGIN);
        let percent =
            (viewer.canvas.zoom(area(window), viewer.content(), MARGIN) * 100.).round() as i64;
        let full = window.viewport_size();
        let button = |id: &'static str, glyph: Icon, key: &str| {
            IconButton::new(id, glyph, focus_i18n::t(key)).tab_stop(false)
        };
        Some(
            gpui::deferred(
                gpui::anchored().position(point(px(0.), px(0.))).child(
                    div()
                        .id("reading-viewer")
                        .occlude()
                        .relative()
                        .w(full.width)
                        .h(full.height)
                        .bg(t.bg)
                        .overflow_hidden()
                        .when(viewer.grab.is_some(), |this| {
                            this.cursor(gpui::CursorStyle::ClosedHand)
                        })
                        .when(viewer.grab.is_none(), |this| {
                            this.cursor(gpui::CursorStyle::OpenHand)
                        })
                        .on_scroll_wheel(cx.listener(
                            |this, event: &ScrollWheelEvent, window, cx| {
                                let delta = f32::from(event.delta.pixel_delta(px(20.)).y);
                                if let Some(viewer) = this.viewer.as_mut() {
                                    viewer.zoom_by(
                                        (delta / 200.).exp(),
                                        at(event.position),
                                        window,
                                    );
                                }
                                cx.stop_propagation();
                                cx.notify();
                            },
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                if let Some(viewer) = this.viewer.as_mut() {
                                    if event.click_count >= 2 {
                                        let content = viewer.content();
                                        if viewer.canvas.is_fitted() {
                                            viewer.canvas.zoom_to(
                                                1.,
                                                at(event.position),
                                                area(window),
                                                content,
                                                MARGIN,
                                            );
                                        } else {
                                            viewer.canvas.fit();
                                        }
                                    } else {
                                        viewer.grab = Some(event.position);
                                        viewer.press = Some(event.position);
                                    }
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                            if let Some(viewer) = this.viewer.as_mut()
                                && let Some(grab) = viewer.grab
                            {
                                if event.pressed_button != Some(MouseButton::Left) {
                                    viewer.grab = None;
                                } else {
                                    let delta = event.position - grab;
                                    let content = viewer.content();
                                    viewer.canvas.pan(
                                        (f32::from(delta.x), f32::from(delta.y)),
                                        area(window),
                                        content,
                                        MARGIN,
                                    );
                                    viewer.grab = Some(event.position);
                                    if viewer.press.is_some_and(|p| {
                                        (event.position.x - p.x).abs() > px(3.)
                                            || (event.position.y - p.y).abs() > px(3.)
                                    }) {
                                        viewer.press = None;
                                    }
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| {
                                let outside = this.viewer.as_ref().is_some_and(|v| {
                                    v.press.is_some()
                                        && !v
                                            .canvas
                                            .placement(area(window), v.content(), MARGIN)
                                            .contains(at(event.position))
                                });
                                if outside {
                                    this.viewer = None;
                                } else if let Some(viewer) = this.viewer.as_mut() {
                                    viewer.grab = None;
                                    viewer.press = None;
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .child(
                            img(viewer.image.clone())
                                .absolute()
                                .left(px(place.x))
                                .top(px(place.y))
                                .w(px(place.width))
                                .h(px(place.height)),
                        )
                        .child(
                            focus_kit::Dir::current()
                                .end(div().absolute(), size::S3)
                                .top(size::S3)
                                .flex()
                                .items_center()
                                .gap(size::S2)
                                .child(ZoomControls::new(
                                    focus_view_media::zoom_label(percent),
                                    button("viewer-zoom-out", Icon::MINUS, "reading-zoom-out")
                                        .shortcut("-")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            if let Some(viewer) = this.viewer.as_mut() {
                                                let (w, h) = area(window);
                                                viewer.zoom_by(1. / STEP, (w / 2., h / 2.), window);
                                            }
                                            cx.notify();
                                        })),
                                    button("viewer-zoom-in", Icon::PLUS, "reading-zoom-in")
                                        .shortcut("+")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            if let Some(viewer) = this.viewer.as_mut() {
                                                let (w, h) = area(window);
                                                viewer.zoom_by(STEP, (w / 2., h / 2.), window);
                                            }
                                            cx.notify();
                                        })),
                                    button("viewer-fit", Icon::MAXIMIZE, "reading-zoom-fit")
                                        .shortcut("0")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(viewer) = this.viewer.as_mut() {
                                                viewer.canvas.fit();
                                            }
                                            cx.notify();
                                        })),
                                ))
                                .child(
                                    focus_kit::floating(t).p(size::S1).child(
                                        button("viewer-close", Icon::CLOSE, "reading-viewer-close")
                                            .shortcut(focus_i18n::keys_text("escape"))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.viewer = None;
                                                cx.notify();
                                            })),
                                    ),
                                ),
                        ),
                ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
