use gpui::{Animation, AnimationExt as _, AnyElement, App, ElementId, Global, IntoElement, Styled};

use crate::tokens::motion;

struct Motion {
    animate: bool,
}

impl Global for Motion {}

pub fn set_animate(animate: bool, cx: &mut App) {
    cx.set_global(Motion { animate });
}

pub fn animates(cx: &App) -> bool {
    cx.try_global::<Motion>().is_some_and(|m| m.animate)
}

pub fn fade_in<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    element: E,
    cx: &App,
) -> AnyElement {
    if animates(cx) {
        element
            .with_animation(id, Animation::new(motion::FADE), |element, delta| {
                element.opacity(delta)
            })
            .into_any_element()
    } else {
        element.into_any_element()
    }
}
