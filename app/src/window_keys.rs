use gpui::{Context, Entity, Focusable, Window};

use crate::editor::{NoteEditor, ViewMode};
use crate::settings::schema::{self, Control, Value};
use crate::shell::{BottomTab, LeftTab, RightTab, Shell};

const INTERFACE_SIZE: &str = "appearance.ui_scale";
const ZOOM_STEP: f64 = 10.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Note,
    Left,
    Right,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zoom {
    In,
    Out,
    Reset,
}

pub fn next_region(open: &[Region], current: Region, forward: bool) -> Region {
    let Some(at) = open.iter().position(|r| *r == current) else {
        return open.first().copied().unwrap_or(Region::Note);
    };
    let count = open.len() as isize;
    let step = if forward { 1 } else { -1 };
    open[(at as isize + step).rem_euclid(count) as usize]
}

fn interface_size(size: f64) -> String {
    focus_i18n::t_args(
        "ui-zoom-interface-size",
        &[("size", focus_i18n::Arg::Int(size.round() as i64))],
    )
}

pub fn zoomed(size: f64, zoom: Zoom, range: (f64, f64), default: f64) -> f64 {
    match zoom {
        Zoom::In => (size + ZOOM_STEP).min(range.1),
        Zoom::Out => (size - ZOOM_STEP).max(range.0),
        Zoom::Reset => default,
    }
}

impl Shell {
    pub fn bind_window_keys(
        &self,
        root: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        use crate::actions::*;
        use gpui::InteractiveElement as _;
        root.on_action(cx.listener(|this, _: &FindInNote, window, cx| {
            if this.find_target() {
                this.open_find(false, window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &FindReplace, window, cx| {
            if this.find_target() {
                this.open_find(true, window, cx);
            }
        }))
        .on_action(
            cx.listener(|this, _: &CreateNote, window, cx| this.create_note_to_name(window, cx)),
        )
        .on_action(cx.listener(|this, _: &SaveNow, _, cx| this.save_now(cx)))
        .on_action(cx.listener(|this, _: &ReopenTab, window, cx| this.reopen_tab(window, cx)))
        .on_action(cx.listener(|this, _: &TogglePinTab, _, cx| this.toggle_front_pin(cx)))
        .on_action(
            cx.listener(|this, _: &GoToTab1, window, cx| this.go_to_tab(Some(0), window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &GoToTab2, window, cx| this.go_to_tab(Some(1), window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &GoToTab3, window, cx| this.go_to_tab(Some(2), window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &GoToTab4, window, cx| this.go_to_tab(Some(3), window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &GoToTab5, window, cx| this.go_to_tab(Some(4), window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &GoToTab6, window, cx| this.go_to_tab(Some(5), window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &GoToTab7, window, cx| this.go_to_tab(Some(6), window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &GoToTab8, window, cx| this.go_to_tab(Some(7), window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &GoToLastTab, window, cx| this.go_to_tab(None, window, cx)),
        )
        .on_action(cx.listener(|this, _: &Quit, _, cx| {
            this.flush_all(cx);
            cx.quit();
        }))
        .on_action(|_: &ToggleFullScreen, window, _| window.toggle_fullscreen())
        .on_action(cx.listener(|this, _: &ShowShortcuts, window, cx| {
            this.open_settings(crate::settings::schema::Section::Shortcuts, window, cx)
        }))
        .on_action(cx.listener(|this, _: &ZoomIn, window, cx| this.zoom(Zoom::In, window, cx)))
        .on_action(cx.listener(|this, _: &ZoomOut, window, cx| this.zoom(Zoom::Out, window, cx)))
        .on_action(
            cx.listener(|this, _: &ResetZoom, window, cx| this.zoom(Zoom::Reset, window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &NextRegion, window, cx| this.cycle_region(true, window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &PrevRegion, window, cx| this.cycle_region(false, window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &EditorMenu, window, cx| this.editor_menu_at_caret(window, cx)),
        )
        .on_action(cx.listener(|this, _: &PastePlain, window, cx| {
            if let Some(editor) = this.active_editor().cloned() {
                this.paste_plain(editor, window, cx);
            }
        }))
    }

    pub fn create_note_to_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.vault_views.as_ref().map(|views| views.tree.clone()) else {
            return;
        };
        self.layout.left_open = true;
        self.layout.left_tab = LeftTab::Files;
        tree.update(cx, |tree, cx| tree.create_note_to_name(window, cx));
        cx.notify();
    }

    pub fn write_new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor().cloned() else {
            return;
        };
        let in_place = {
            let e = editor.read(cx);
            e.view == ViewMode::Live && e.edit_in_place()
        };
        if in_place {
            let end = editor
                .read(cx)
                .input
                .read(cx)
                .value()
                .lines()
                .count()
                .saturating_sub(1);
            let reading = editor.read(cx).reading.clone();
            reading.update(cx, |reading, cx| {
                reading.place_caret_at(end as u32, 0, window, cx)
            });
        }
        self.focus_content(window, cx);
        cx.notify();
    }

    pub fn save_now(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor().cloned() else {
            return;
        };
        let ready = {
            let e = editor.read(cx);
            e.dirty && matches!(e.load, crate::editor::Load::Ready)
        };
        if ready {
            editor.update(cx, |editor, cx| drop(editor.save(false, cx)));
        }
    }

    pub fn zoom(&mut self, zoom: Zoom, window: &mut Window, cx: &mut Context<Self>) {
        if self.front_file().is_some() {
            self.zoom_file(
                match zoom {
                    Zoom::In => focus_view_media::ZoomAction::In,
                    Zoom::Out => focus_view_media::ZoomAction::Out,
                    Zoom::Reset => focus_view_media::ZoomAction::Fit,
                },
                cx,
            );
            return;
        }
        if self.viewer_focused(window, cx) {
            cx.propagate();
            return;
        }
        let Some(setting) = schema::setting(INTERFACE_SIZE) else {
            return;
        };
        let Control::Number { min, max, .. } = setting.control else {
            return;
        };
        let default = match &setting.default {
            Some(Value::Number(n)) => *n,
            _ => min,
        };
        let size = self.prefs.read(cx).number(INTERFACE_SIZE);
        let next = zoomed(size, zoom, (min, max), default);
        self.withdraw_notice(interface_size(size), cx);
        if next != size {
            self.prefs.update(cx, |prefs, cx| {
                prefs.set(INTERFACE_SIZE, Value::Number(next), cx)
            });
        }
        self.notify_user(focus_kit::Tone::Ok, interface_size(next), cx);
    }

    fn viewer_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        let graph = self
            .graph
            .as_ref()
            .is_some_and(|graph| graph.focus_handle(cx).contains_focused(window, cx));
        let diagram = self.active_editor().is_some_and(|editor| {
            let reading = editor.read(cx).reading.read(cx);
            reading.viewer_shown() && reading.focus_handle(cx).contains_focused(window, cx)
        });
        graph || diagram
    }

    fn open_regions(&self) -> Vec<Region> {
        let mut open = vec![Region::Note];
        if self.layout.left_open {
            open.push(Region::Left);
        }
        if self.layout.right_open {
            open.push(Region::Right);
        }
        if self.layout.bottom_open {
            open.push(Region::Bottom);
        }
        open
    }

    pub(crate) fn region_now(&self, window: &Window, cx: &gpui::App) -> Region {
        let inside = |handle: gpui::FocusHandle| handle.contains_focused(window, cx);
        let left = self.search.focus_handle(cx).contains_focused(window, cx)
            || self.vault_views.as_ref().is_some_and(|views| {
                inside(views.tree.focus_handle(cx))
                    || inside(views.tags.focus_handle(cx))
                    || inside(views.bookmarks.focus_handle(cx))
            });
        if left {
            return Region::Left;
        }
        let right = inside(self.right_strip.clone())
            || self
                .ai
                .as_ref()
                .is_some_and(|panel| inside(panel.focus_handle(cx)));
        if right {
            return Region::Right;
        }
        let bottom = inside(self.bottom_strip.clone())
            || self.vault_views.as_ref().is_some_and(|views| {
                inside(views.tasks.focus_handle(cx)) || inside(views.broken.focus_handle(cx))
            });
        if bottom {
            return Region::Bottom;
        }
        Region::Note
    }

    pub fn cycle_region(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.open_regions();
        let now = self.region_now(window, cx);
        let region = next_region(&open, now, forward);
        self.focus_region(region, window, cx);
    }

    fn focus_region(&mut self, region: Region, window: &mut Window, cx: &mut Context<Self>) {
        match region {
            Region::Note => self.focus_content(window, cx),
            Region::Left => match self.layout.left_tab {
                LeftTab::Search => self
                    .search
                    .update(cx, |panel, cx| panel.focus_field(window, cx)),
                tab => {
                    if let Some(views) = &self.vault_views {
                        match tab {
                            LeftTab::Files => {
                                let tree = views.tree.clone();
                                tree.update(cx, |tree, cx| tree.focus_on_active(window, cx));
                            }
                            LeftTab::Tags => window.focus(&views.tags.focus_handle(cx)),
                            _ => window.focus(&views.bookmarks.focus_handle(cx)),
                        }
                    }
                }
            },
            Region::Right => window.focus(&self.right_strip),
            Region::Bottom => window.focus(&self.bottom_strip),
        }
        cx.notify();
    }

    pub fn show_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.layout.left_open = true;
        self.layout.left_tab = LeftTab::Files;
        self.focus_region(Region::Left, window, cx);
    }

    pub fn enter_strip(&mut self, bottom: bool, window: &mut Window, cx: &mut Context<Self>) {
        if bottom {
            let Some(views) = &self.vault_views else {
                return;
            };
            match self.layout.bottom_tab {
                BottomTab::Tasks => window.focus(&views.tasks.focus_handle(cx)),
                BottomTab::Broken => window.focus(&views.broken.focus_handle(cx)),
                BottomTab::Journal => {}
            }
        } else if self.layout.right_tab == RightTab::Ai
            && self.prefs.read(cx).bool("ai.enabled")
            && let Some(panel) = self.ai.clone()
        {
            panel.update(cx, |panel, cx| panel.focus_input(window, cx));
        }
        cx.notify();
    }

    pub fn step_strip(&mut self, bottom: bool, forward: bool, cx: &mut Context<Self>) {
        if bottom {
            let tabs = [BottomTab::Journal, BottomTab::Tasks, BottomTab::Broken];
            let at = tabs
                .iter()
                .position(|t| *t == self.layout.bottom_tab)
                .unwrap_or(0);
            let next = (at as isize + if forward { 1 } else { -1 }).rem_euclid(tabs.len() as isize);
            self.layout.bottom_tab = tabs[next as usize];
        } else {
            let mut tabs = vec![RightTab::Outline, RightTab::Links, RightTab::Info];
            if self.prefs.read(cx).bool("ai.enabled") {
                tabs.push(RightTab::Ai);
            }
            if self.prefs.read(cx).bool("ops.enabled") {
                tabs.push(RightTab::Ops);
            }
            let at = tabs
                .iter()
                .position(|t| *t == self.layout.right_tab)
                .unwrap_or(0);
            let next = (at as isize + if forward { 1 } else { -1 }).rem_euclid(tabs.len() as isize);
            self.layout.right_tab = tabs[next as usize];
        }
        cx.notify();
    }

    pub fn editor_menu_at_caret(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor().cloned() else {
            return;
        };
        if !matches!(self.overlay, crate::shell::Overlay::None) {
            return;
        }
        let (view, block, note, reading) = {
            let e = editor.read(cx);
            let block = e.live.as_ref().map(|b| b.input.clone());
            (e.view, block, e.input.clone(), e.reading.clone())
        };
        let fallback = reading.read(cx).viewport().origin
            + gpui::point(focus_kit::size::S12, focus_kit::size::S12);
        let field = match (view, block) {
            (ViewMode::Source, _) => Some(note.clone()),
            (ViewMode::Live, Some(block)) => Some(block),
            _ => None,
        };
        let at = match &field {
            Some(field) => field
                .read(cx)
                .cursor_bounds()
                .map(|b| b.bottom_left())
                .unwrap_or(fallback),
            None => {
                let caret = note.read(cx).cursor();
                reading
                    .read(cx)
                    .bounds_for_byte(caret)
                    .map(|b| b.bottom_left())
                    .unwrap_or(fallback)
            }
        };
        let in_place = field.is_none() && editor.read(cx).caret_in_view(cx);
        if field.is_none() && !in_place {
            return;
        }
        let selected = editor
            .update(cx, |editor, cx| editor.compose_selection(window, cx))
            .is_some();
        if in_place && !selected {
            return;
        }
        self.show_editor_menu(at, field, selected, window, cx);
    }

    pub fn paste_plain(
        &mut self,
        editor: Entity<NoteEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let text = focus_kit::clipboard::read_text(cx).await;
            let _ = this.update_in(cx, |this, window, cx| match text {
                Ok(Some(text)) => {
                    editor.update(cx, |editor, cx| editor.insert_at_cursor(&text, window, cx))
                }
                Ok(None) => {}
                Err(error) => this.notify_user(
                    focus_kit::Tone::Danger,
                    crate::errors::clipboard(&error),
                    cx,
                ),
            });
        })
        .detach();
    }
}

const WHEEL_STEP: f32 = 40.;

pub fn zoom_wheel(acc: std::rc::Rc<std::cell::Cell<f32>>) -> impl gpui::IntoElement {
    use gpui::Styled as _;
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            window.on_mouse_event(move |event: &gpui::ScrollWheelEvent, phase, window, cx| {
                if phase != gpui::DispatchPhase::Capture
                    || !event.modifiers.control
                    || !bounds.contains(&event.position)
                {
                    return;
                }
                cx.stop_propagation();
                let delta = match event.delta {
                    gpui::ScrollDelta::Lines(lines) => lines.y * WHEEL_STEP,
                    gpui::ScrollDelta::Pixels(pixels) => f32::from(pixels.y),
                };
                let total = acc.get() + delta;
                if total.abs() < WHEEL_STEP {
                    acc.set(total);
                    return;
                }
                acc.set(0.);
                if total > 0. {
                    window.dispatch_action(Box::new(crate::actions::ZoomIn), cx);
                } else {
                    window.dispatch_action(Box::new(crate::actions::ZoomOut), cx);
                }
            });
        },
    )
    .absolute()
    .inset_0()
}

pub fn select_page(
    input: Entity<gpui_component::input::InputState>,
    down: bool,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    use gpui::EntityInputHandler as _;
    use gpui_component::RopeExt as _;
    let anchor = input.update(cx, |state, cx| {
        let selected = state.selected_text_range(true, window, cx)?;
        let at = if selected.reversed {
            selected.range.end
        } else {
            selected.range.start
        };
        Some(state.text().offset_utf16_to_offset(at))
    });
    let Some(anchor) = anchor else {
        return;
    };
    if down {
        window.dispatch_action(Box::new(gpui_component::input::MovePageDown), cx);
    } else {
        window.dispatch_action(Box::new(gpui_component::input::MovePageUp), cx);
    }
    window.defer(cx, move |_, cx| {
        input.update(cx, |state, cx| {
            let head = state.cursor();
            state.set_selection(anchor, head, cx);
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_cycle_through_the_open_ones() {
        let open = [Region::Note, Region::Left, Region::Bottom];
        assert_eq!(next_region(&open, Region::Note, true), Region::Left);
        assert_eq!(next_region(&open, Region::Bottom, true), Region::Note);
        assert_eq!(next_region(&open, Region::Note, false), Region::Bottom);
        assert_eq!(next_region(&open, Region::Right, true), Region::Note);
        assert_eq!(
            next_region(&[Region::Note], Region::Note, true),
            Region::Note
        );
    }

    #[test]
    fn the_interface_size_steps_by_ten_inside_its_range() {
        assert_eq!(zoomed(100., Zoom::In, (90., 120.), 100.), 110.);
        assert_eq!(zoomed(120., Zoom::In, (90., 120.), 100.), 120.);
        assert_eq!(zoomed(90., Zoom::Out, (90., 120.), 100.), 90.);
        assert_eq!(zoomed(115., Zoom::In, (90., 120.), 100.), 120.);
        assert_eq!(zoomed(110., Zoom::Reset, (90., 120.), 100.), 100.);
    }
}
