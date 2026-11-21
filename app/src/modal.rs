use gpui::{
    AnyElement, App, Context, FocusHandle, InteractiveElement, IntoElement, KeystrokeEvent,
    ParentElement, Styled, Window, div,
};

use crate::shell::{Overlay, Shell};

pub struct ModalFocus {
    pub scope: FocusHandle,
    pub primary: FocusHandle,
    giver: Option<FocusHandle>,
}

impl ModalFocus {
    pub fn new(cx: &mut App) -> Self {
        ModalFocus {
            scope: cx.focus_handle(),
            primary: cx.focus_handle(),
            giver: None,
        }
    }
}

impl Shell {
    pub(crate) fn remember_focus_giver(&mut self, window: &Window, cx: &App) {
        if matches!(self.overlay, Overlay::None) {
            self.modal.giver = window.focused(cx);
        }
    }

    pub(crate) fn give_focus_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self
            .modal
            .giver
            .clone()
            .filter(|giver| self.focus.contains(giver, window))
        {
            Some(giver) => window.focus(&giver),
            None => self.focus_content(window, cx),
        }
    }

    pub(crate) fn keys_outside_overlay(
        &mut self,
        event: &KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let keystroke = &event.keystroke;
        let m = keystroke.modifiers;
        if m.control
            || m.alt
            || m.platform
            || matches!(self.overlay, Overlay::None)
            || self.modal.scope.contains_focused(window, cx)
        {
            return false;
        }
        match keystroke.key.as_str() {
            "escape" if !m.shift => self.close_overlay(window, cx),
            "tab" => focus_kit::cycle_focus(&self.modal.scope, !m.shift, window, cx),
            _ => return false,
        }
        true
    }

    pub(crate) fn scope_overlay(&self, overlay: AnyElement) -> AnyElement {
        focus_kit::contain_tab(
            div().absolute().inset_0().track_focus(&self.modal.scope),
            &self.modal.scope,
        )
        .child(overlay)
        .into_any_element()
    }
}
