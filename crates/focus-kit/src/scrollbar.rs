use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    App, Bounds, CursorStyle, DispatchPhase, Element, ElementId, GlobalElementId, Hitbox,
    HitboxBehavior, InspectorElementId, IntoElement, LayoutId, ListState, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Position, ScrollHandle, Style,
    UniformListScrollHandle, Window, fill, point, px, relative, size,
};

use crate::tokens::ActiveTokens;

const WIDTH: f32 = 6.;
const MIN_THUMB: f32 = 24.;

pub trait ScrollSource: 'static {
    fn top(&self) -> Pixels;
    fn max(&self) -> Pixels;
    fn set_top(&self, top: Pixels);
    fn drag_started(&self) {}
    fn drag_ended(&self) {}
}

impl ScrollSource for ScrollHandle {
    fn top(&self) -> Pixels {
        -self.offset().y
    }
    fn max(&self) -> Pixels {
        self.max_offset().height
    }
    fn set_top(&self, top: Pixels) {
        self.set_offset(point(self.offset().x, -top));
    }
}

impl ScrollSource for UniformListScrollHandle {
    fn top(&self) -> Pixels {
        self.0.borrow().base_handle.top()
    }
    fn max(&self) -> Pixels {
        self.0.borrow().base_handle.max()
    }
    fn set_top(&self, top: Pixels) {
        self.0.borrow().base_handle.set_top(top);
    }
}

impl ScrollSource for ListState {
    fn top(&self) -> Pixels {
        -self.scroll_px_offset_for_scrollbar().y
    }
    fn max(&self) -> Pixels {
        self.max_offset_for_scrollbar().height
    }
    fn set_top(&self, top: Pixels) {
        self.set_offset_from_scrollbar(point(px(0.), top));
    }
    fn drag_started(&self) {
        self.scrollbar_drag_started();
    }
    fn drag_ended(&self) {
        self.scrollbar_drag_ended();
    }
}

struct Sideways(ScrollHandle);

impl ScrollSource for Sideways {
    fn top(&self) -> Pixels {
        -self.0.offset().x
    }
    fn max(&self) -> Pixels {
        self.0.max_offset().width
    }
    fn set_top(&self, top: Pixels) {
        self.0.set_offset(point(-top, self.0.offset().y));
    }
}

pub struct Scrollbar {
    id: ElementId,
    source: Rc<dyn ScrollSource>,
    horizontal: bool,
}

pub fn scrollbar(id: impl Into<ElementId>, source: impl ScrollSource) -> Scrollbar {
    Scrollbar {
        id: id.into(),
        source: Rc::new(source),
        horizontal: false,
    }
}

pub fn horizontal_scrollbar(id: impl Into<ElementId>, handle: ScrollHandle) -> Scrollbar {
    Scrollbar {
        id: id.into(),
        source: Rc::new(Sideways(handle)),
        horizontal: true,
    }
}

#[derive(Default, Clone)]
struct Drag(Rc<Cell<Option<Pixels>>>);

pub struct Prepaint {
    hitbox: Hitbox,
    thumb: Option<Bounds<Pixels>>,
    drag: Drag,
}

impl Element for Scrollbar {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style {
            position: Position::Absolute,
            ..Style::default()
        };
        if self.horizontal {
            style.inset.left = px(0.).into();
            style.inset.right = px(0.).into();
            style.inset.bottom = px(0.).into();
            style.size.width = relative(1.).into();
            style.size.height = px(WIDTH).into();
        } else {
            style.inset.top = px(0.).into();
            style.inset.bottom = px(0.).into();
            if crate::Dir::current().rtl {
                style.inset.left = px(0.).into();
            } else {
                style.inset.right = px(0.).into();
            }
            style.size.width = px(WIDTH).into();
            style.size.height = relative(1.).into();
        }
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) -> Prepaint {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let max = self.source.max();
        let horizontal = self.horizontal;
        let track = if horizontal {
            bounds.size.width
        } else {
            bounds.size.height
        };
        let thumb = (max > px(0.5) && track > px(MIN_THUMB)).then(|| {
            let length = (track * (track / (track + max))).max(px(MIN_THUMB));
            let top = (self.source.top() / max).clamp(0., 1.) * (track - length);
            if horizontal {
                Bounds::new(
                    point(bounds.origin.x + top, bounds.origin.y),
                    size(length, px(WIDTH)),
                )
            } else {
                Bounds::new(
                    point(bounds.origin.x, bounds.origin.y + top),
                    size(px(WIDTH), length),
                )
            }
        });
        let drag = match id {
            Some(id) => window.with_element_state(id, |state: Option<Drag>, _| {
                let drag = state.unwrap_or_default();
                (drag.clone(), drag)
            }),
            None => Drag::default(),
        };
        Prepaint {
            hitbox,
            thumb,
            drag,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        state: &mut Prepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(thumb) = state.thumb else { return };
        let t = *cx.tokens();
        let dragging = state.drag.0.get().is_some();
        let hovered = state.hitbox.is_hovered(window);
        let color = if dragging || hovered {
            t.ink_3
        } else {
            t.ink_4
        };
        window.paint_quad(fill(thumb, color).corner_radii(crate::tokens::size::RADIUS));
        window.set_cursor_style(CursorStyle::Arrow, &state.hitbox);

        let horizontal = self.horizontal;
        let along = move |p: gpui::Point<Pixels>| if horizontal { p.x } else { p.y };
        let (track, length, start) = if horizontal {
            (bounds.size.width, thumb.size.width, bounds.origin.x)
        } else {
            (bounds.size.height, thumb.size.height, bounds.origin.y)
        };
        let source = self.source.clone();
        let to_top = move |y: Pixels, grab: Pixels| {
            let travel = (track - length).max(px(1.));
            ((y - start - grab) / travel).clamp(0., 1.) * source.max()
        };

        let hitbox = state.hitbox.clone();
        let drag = state.drag.clone();
        let source = self.source.clone();
        let place = to_top.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !hitbox.is_hovered(window)
            {
                return;
            }
            let grab = if thumb.contains(&event.position) {
                along(event.position) - along(thumb.origin)
            } else {
                length / 2.
            };
            drag.0.set(Some(grab));
            source.drag_started();
            source.set_top(place(along(event.position), grab));
            cx.stop_propagation();
            window.refresh();
        });

        let drag = state.drag.clone();
        let source = self.source.clone();
        let hitbox = state.hitbox.clone();
        let was_hovered = hovered;
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            if let Some(grab) = drag.0.get() {
                source.set_top(to_top(along(event.position), grab));
                window.refresh();
            } else if hitbox.is_hovered(window) != was_hovered {
                window.refresh();
            }
        });

        let drag = state.drag.clone();
        let source = self.source.clone();
        window.on_mouse_event(move |_: &MouseUpEvent, phase, window, _| {
            if phase == DispatchPhase::Bubble && drag.0.take().is_some() {
                source.drag_ended();
                window.refresh();
            }
        });
    }
}

impl IntoElement for Scrollbar {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
