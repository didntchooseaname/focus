use std::rc::Rc;

use gpui::{
    App, ClickEvent, Div, ElementId, FocusHandle, Global, InteractiveElement, KeyBinding,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window, actions, div,
};

actions!(
    focus_kit,
    [Press, StepLeft, StepRight, FocusNext, FocusPrevious]
);

pub(crate) type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub(crate) fn with_press<E>(
    element: E,
    context: &'static str,
    focus: &FocusHandle,
    handler: ClickHandler,
) -> E
where
    E: StatefulInteractiveElement + ParentElement,
{
    let on_key = handler.clone();
    element
        .key_context(context)
        .on_action(move |_: &Press, window, cx| on_key(&ClickEvent::default(), window, cx))
        .on_click(move |event, window, cx| handler(event, window, cx))
        .child(focus_anchor(focus))
}

pub(crate) fn focus_anchor(focus: &FocusHandle) -> Div {
    div().absolute().size_0().track_focus(focus)
}

pub(crate) const BUTTON: &str = "KitButton";
pub(crate) const CHECKBOX: &str = "KitCheckbox";
pub(crate) const SEGMENTS: &str = "KitSegments";
const TRAP: &str = "KitFocusTrap";
const WALK_LIMIT: usize = 4096;

pub(crate) fn bind(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", Press, Some(BUTTON)),
        KeyBinding::new("space", Press, Some(BUTTON)),
        KeyBinding::new("space", Press, Some(CHECKBOX)),
        KeyBinding::new("left", StepLeft, Some(SEGMENTS)),
        KeyBinding::new("right", StepRight, Some(SEGMENTS)),
        KeyBinding::new("tab", FocusNext, Some(TRAP)),
        KeyBinding::new("shift-tab", FocusPrevious, Some(TRAP)),
    ]);
}

pub(crate) fn control_focus(
    id: &ElementId,
    given: Option<FocusHandle>,
    tab_stop: bool,
    window: &mut Window,
    cx: &mut App,
) -> FocusHandle {
    let handle = given
        .unwrap_or_else(|| {
            window
                .use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle())
                .read(cx)
                .clone()
        })
        .tab_stop(tab_stop);
    if cx.has_global::<FocusNames>() {
        name_focus(&handle, id.to_string(), cx);
    }
    handle
}

#[derive(Default)]
struct FocusNames(Vec<(FocusHandle, SharedString)>);

impl Global for FocusNames {}

pub fn record_focus_names(cx: &mut App) {
    cx.set_global(FocusNames::default());
}

pub fn name_focus(handle: &FocusHandle, name: impl Into<SharedString>, cx: &mut App) {
    if !cx.has_global::<FocusNames>() {
        return;
    }
    let name = name.into();
    let names = &mut cx.global_mut::<FocusNames>().0;
    match names.iter_mut().find(|(known, _)| known == handle) {
        Some(entry) => entry.1 = name,
        None => names.push((handle.clone(), name)),
    }
}

pub fn focus_name(window: &Window, cx: &App) -> Option<SharedString> {
    let focused = window.focused(cx)?;
    cx.try_global::<FocusNames>()?
        .0
        .iter()
        .find(|(known, _)| *known == focused)
        .map(|(_, name)| name.clone())
}

pub(crate) fn step_toward_end(left: bool) -> bool {
    left == crate::Dir::current().rtl
}

pub fn cycle_focus(scope: &FocusHandle, forward: bool, window: &mut Window, cx: &App) {
    let start = window.focused(cx);
    let mut first: Option<FocusHandle> = None;
    for _ in 0..WALK_LIMIT {
        if forward {
            window.focus_next();
        } else {
            window.focus_prev();
        }
        let Some(now) = window.focused(cx) else {
            break;
        };
        if now != *scope && scope.contains(&now, window) {
            return;
        }
        if first.as_ref() == Some(&now) {
            break;
        }
        first.get_or_insert(now);
    }
    match start {
        Some(handle) => window.focus(&handle),
        None => window.blur(),
    }
}

pub fn contain_tab<E: InteractiveElement>(element: E, scope: &FocusHandle) -> E {
    let forward = scope.clone();
    let backward = scope.clone();
    element
        .key_context(TRAP)
        .on_action(move |_: &FocusNext, window, cx| cycle_focus(&forward, true, window, cx))
        .on_action(move |_: &FocusPrevious, window, cx| cycle_focus(&backward, false, window, cx))
}
