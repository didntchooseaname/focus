use std::time::{Duration, Instant};

use focus_i18n::t;
use focus_kit::{Tokens, Tooltip, Type, hatch, size};
use focus_ops::{FindingStatus, HostStatus, Severity};
use gpui::{
    AnyElement, App, Div, FocusHandle, FontWeight, Hsla, InteractiveElement, MouseDownEvent,
    ParentElement, ScrollHandle, Styled, WeakFocusHandle, Window, div, prelude::FluentBuilder, px,
};

pub(crate) const EYE: focus_kit::Icon = focus_kit::Icon("\u{E0BA}");
pub(crate) const EYE_OFF: focus_kit::Icon = focus_kit::Icon("\u{E0BB}");

pub(crate) fn row() -> Div {
    crate::dir::Dir::current().row()
}

pub(crate) fn t_(key: &str) -> String {
    t(key)
}

pub(crate) fn tip(label: String) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    move |_, cx| Tooltip::view(label.clone().into(), None, cx)
}

pub(crate) fn severity_key(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "ops-severity-critical",
        Severity::High => "ops-severity-high",
        Severity::Medium => "ops-severity-medium",
        Severity::Low => "ops-severity-low",
        Severity::Info => "ops-severity-info",
    }
}

pub(crate) fn finding_status_key(status: FindingStatus) -> &'static str {
    match status {
        FindingStatus::Draft => "ops-finding-status-draft",
        FindingStatus::Confirmed => "ops-finding-status-confirmed",
        FindingStatus::Reported => "ops-finding-status-reported",
        FindingStatus::Fixed => "ops-finding-status-fixed",
        FindingStatus::Accepted => "ops-finding-status-accepted",
    }
}

pub(crate) fn host_status_key(status: HostStatus) -> &'static str {
    match status {
        HostStatus::New => "ops-host-status-new",
        HostStatus::Enum => "ops-host-status-enum",
        HostStatus::Foothold => "ops-host-status-foothold",
        HostStatus::User => "ops-host-status-user",
        HostStatus::Owned => "ops-host-status-owned",
    }
}

pub(crate) fn severity_color(t: &Tokens, severity: Severity) -> Hsla {
    match severity {
        Severity::Critical | Severity::High => t.danger,
        Severity::Medium => t.warn,
        Severity::Low => t.ink_2,
        Severity::Info => t.ink_3,
    }
}

pub(crate) fn severity_weight(severity: Severity) -> FontWeight {
    if severity == Severity::Critical {
        FontWeight::SEMIBOLD
    } else {
        FontWeight::MEDIUM
    }
}

pub(crate) fn severity_tag(t: &Tokens, severity: Severity) -> Div {
    let color = severity_color(t, severity);
    row()
        .items_center()
        .gap(size::S2)
        .child(div().flex_none().w(px(2.)).h(px(16.)).bg(color))
        .child(
            div()
                .ui_sm()
                .font_weight(severity_weight(severity))
                .text_color(color)
                .child(t_(severity_key(severity))),
        )
}

pub(crate) fn mono_chip(t: &Tokens, text: impl Into<gpui::SharedString>) -> Div {
    div()
        .flex_none()
        .px(px(6.))
        .py(px(1.))
        .rounded(size::RADIUS)
        .bg(t.card_2)
        .mono_sm()
        .text_color(t.ink)
        .child(text.into())
}

pub(crate) fn tile(t: &Tokens, figure: String, label: String, marker: Option<Hsla>) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(size::S1)
        .px(size::S4)
        .child(div().stat().text_color(t.ink).child(figure))
        .child(
            row()
                .items_center()
                .gap(size::S2)
                .children(marker.map(|color| div().flex_none().w(px(2.)).h(px(14.)).bg(color)))
                .child(div().ui_sm().text_color(t.ink_3).child(label)),
        )
}

pub(crate) fn rule_v(t: &Tokens) -> Div {
    div().flex_none().w(px(1.)).bg(t.line)
}

pub(crate) fn start(block: Div) -> Div {
    crate::dir::Dir::current().align_ui(block.w_full())
}

pub(crate) fn section_label(t: &Tokens, text: String) -> Div {
    start(div().ui_xs().text_color(t.ink_3)).child(text)
}

pub(crate) fn hatched_empty(t: &Tokens, message: String, action: Option<AnyElement>) -> Div {
    div()
        .relative()
        .flex_1()
        .min_h(px(96.))
        .child(hatch())
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(size::S3)
                .child(
                    div()
                        .px(size::S3)
                        .py(size::S2)
                        .bg(t.bg)
                        .ui()
                        .text_color(t.ink_3)
                        .child(message),
                )
                .children(action),
        )
}

pub(crate) fn clip_to(text: &str, chars: usize) -> String {
    let mut out: String = text.chars().take(chars).collect();
    if text.chars().count() > chars {
        out.push('…');
    }
    out
}

pub(crate) fn screen_header(t: &Tokens, title: String, count: Option<String>) -> Div {
    div()
        .flex()
        .items_center()
        .gap(size::S3)
        .child(div().display_sm().text_color(t.ink).child(title))
        .when_some(count, |this, count| {
            this.child(div().ui_sm().text_color(t.ink_3).child(count))
        })
}

pub(crate) fn table_filler() -> Div {
    div().flex_1().min_h(px(0.)).child(hatch())
}

pub(crate) fn single_line(
    text: &str,
    _: &mut gpui::Context<gpui_component::input::InputState>,
) -> bool {
    !text.contains(['\n', '\r'])
}

#[derive(Default)]
pub(crate) struct CopyConfirm {
    which: Option<String>,
    token: u64,
}

impl CopyConfirm {
    pub(crate) fn is(&self, which: &str) -> bool {
        self.which.as_deref() == Some(which)
    }

    pub(crate) fn mark<V: 'static>(
        &mut self,
        which: String,
        field: fn(&mut V) -> &mut CopyConfirm,
        cx: &mut gpui::Context<V>,
    ) {
        self.token += 1;
        self.which = Some(which);
        let token = self.token;
        cx.notify();
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;
            view.update(cx, |view, cx| {
                let confirm = field(view);
                if confirm.token == token {
                    confirm.which = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn face(
        &self,
        which: &str,
        icon: focus_kit::Icon,
        label: &str,
    ) -> (focus_kit::Icon, String) {
        if self.is(which) {
            (focus_kit::Icon::CHECK, t_("ops-copied"))
        } else {
            (icon, t_(label))
        }
    }
}

gpui::actions!(focus_ops_list, [PageUp, PageDown, FirstItem, LastItem]);
gpui::actions!(focus_ops_menu, [CloseMenu]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Jump {
    PageUp,
    PageDown,
    First,
    Last,
}

struct ListKeysBound;
impl gpui::Global for ListKeysBound {}

pub(crate) fn list_focus(cx: &mut App) -> FocusHandle {
    if !cx.has_global::<ListKeysBound>() {
        use gpui::KeyBinding;
        const IN_LIST: Option<&str> = Some("OpsList");
        cx.bind_keys([
            KeyBinding::new("pageup", PageUp, IN_LIST),
            KeyBinding::new("pagedown", PageDown, IN_LIST),
            KeyBinding::new("home", FirstItem, IN_LIST),
            KeyBinding::new("end", LastItem, IN_LIST),
        ]);
        cx.set_global(ListKeysBound);
    }
    cx.focus_handle()
}

pub(crate) fn pageable<V: 'static, E: InteractiveElement>(
    element: E,
    focus: &FocusHandle,
    cx: &mut gpui::Context<V>,
    apply: fn(&mut V, Jump, &mut Window, &mut gpui::Context<V>),
) -> E {
    fn key<V: 'static, A: gpui::Action>(
        jump: Jump,
        focus: &FocusHandle,
        apply: fn(&mut V, Jump, &mut Window, &mut gpui::Context<V>),
    ) -> impl Fn(&mut V, &A, &mut Window, &mut gpui::Context<V>) + 'static {
        let focus = focus.clone();
        move |this, _, window, cx| {
            if focus.is_focused(window) {
                apply(this, jump, window, cx);
            } else {
                cx.propagate();
            }
        }
    }
    element
        .key_context("OpsList")
        .track_focus(focus)
        .on_action(cx.listener(key::<V, PageUp>(Jump::PageUp, focus, apply)))
        .on_action(cx.listener(key::<V, PageDown>(Jump::PageDown, focus, apply)))
        .on_action(cx.listener(key::<V, FirstItem>(Jump::First, focus, apply)))
        .on_action(cx.listener(key::<V, LastItem>(Jump::Last, focus, apply)))
}

pub(crate) fn landing(
    current: Option<usize>,
    len: usize,
    page: usize,
    jump: Jump,
) -> Option<usize> {
    let last = len.checked_sub(1)?;
    Some(match jump {
        Jump::First => 0,
        Jump::Last => last,
        Jump::PageDown => current
            .map_or(page.saturating_sub(1), |c| c + page)
            .min(last),
        Jump::PageUp => current.map_or(0, |c| c.saturating_sub(page)),
    })
}

pub(crate) fn rows_per_page(scroll: &ScrollHandle, height: f32) -> usize {
    ((scroll.bounds().size.height / px(height)) as usize).max(1)
}

pub(crate) fn scroll_jump(scroll: &ScrollHandle, jump: Jump) {
    let (offset, max, page) = (
        scroll.offset(),
        scroll.max_offset(),
        scroll.bounds().size.height * 0.9,
    );
    let y = match jump {
        Jump::PageUp => offset.y + page,
        Jump::PageDown => offset.y - page,
        Jump::First => px(0.),
        Jump::Last => -max.height,
    };
    scroll.set_offset(gpui::point(offset.x, y.clamp(-max.height, px(0.))));
}

struct MenuKeysBound;
impl gpui::Global for MenuKeysBound {}

#[derive(Default)]
pub(crate) struct Ticker {
    task: Option<gpui::Task<()>>,
}

impl Ticker {
    pub(crate) fn running(state: &crate::state::OpsState) -> bool {
        state.active.as_ref().is_some_and(|a| {
            let timer = focus_ops::timer::Timer::of(&a.engagement);
            if timer.state() != focus_ops::timer::State::Running {
                return false;
            }
            let now = crate::time::now_unix();
            let over = a
                .engagement
                .exam
                .as_deref()
                .and_then(|id| focus_ops::exam::profiles().iter().find(|p| p.id == id))
                .zip(timer.started)
                .is_some_and(|(profile, started)| {
                    focus_ops::exam::clock(profile, started, timer.clock_now(now)).expired
                });
            !over
        })
    }

    pub(crate) fn sync<V: 'static>(&mut self, running: bool, cx: &mut gpui::Context<V>) {
        match (running, self.task.is_some()) {
            (true, false) => {
                self.task = Some(cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor().timer(Duration::from_secs(1)).await;
                        if this.update(cx, |_, cx| cx.notify()).is_err() {
                            break;
                        }
                    }
                }));
            }
            (false, true) => self.task = None,
            _ => {}
        }
    }
}

pub(crate) struct MenuFocus {
    handle: FocusHandle,
    previous: Option<WeakFocusHandle>,
    active: bool,
    closed_at: Option<Instant>,
}

impl MenuFocus {
    pub(crate) fn new(cx: &mut App) -> MenuFocus {
        if !cx.has_global::<MenuKeysBound>() {
            cx.bind_keys([gpui::KeyBinding::new("escape", CloseMenu, Some("OpsMenu"))]);
            cx.set_global(MenuKeysBound);
        }
        MenuFocus {
            handle: cx.focus_handle(),
            previous: None,
            active: false,
            closed_at: None,
        }
    }

    pub(crate) fn sync(&mut self, open: bool, window: &mut Window, cx: &mut App) {
        if open && !self.active {
            self.previous = window.focused(cx).map(|focused| focused.downgrade());
            window.focus(&self.handle);
            self.active = true;
        } else if !open && self.active {
            self.active = false;
            if let Some(previous) = self
                .previous
                .take()
                .and_then(|weak| weak.upgrade())
                .filter(|_| self.handle.is_focused(window))
            {
                window.focus(&previous);
            }
        }
    }

    pub(crate) fn dismissed(&mut self) {
        self.closed_at = Some(Instant::now());
    }

    pub(crate) fn click_opens(&self, open: bool) -> bool {
        !open
            && !self
                .closed_at
                .is_some_and(|at| at.elapsed() < Duration::from_millis(200))
    }

    pub(crate) fn attach(
        &self,
        menu: Div,
        dismiss: impl Fn(&mut Window, &mut App) + Clone + 'static,
    ) -> Div {
        let on_escape = dismiss.clone();
        menu.track_focus(&self.handle)
            .key_context("OpsMenu")
            .occlude()
            .on_mouse_down_out(move |_: &MouseDownEvent, window, cx| dismiss(window, cx))
            .on_action(move |_: &CloseMenu, window, cx| on_escape(window, cx))
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn dropdown(
    t: &Tokens,
    id: &'static str,
    label: String,
    rows: Vec<(String, bool)>,
    open: bool,
    menus: &MenuFocus,
    toggle: impl Fn(&mut Window, &mut App) + 'static,
    pick: impl Fn(usize, &mut Window, &mut App) + Clone + 'static,
    dismiss: impl Fn(&mut Window, &mut App) + Clone + 'static,
    cx: &App,
) -> AnyElement {
    use focus_kit::{Icon, MenuRow, menu};
    use gpui::StatefulInteractiveElement as _;
    use gpui::{IntoElement as _, anchored, deferred};
    div()
        .relative()
        .child(
            row()
                .id(id)
                .items_center()
                .gap(size::S2)
                .h(size::CONTROL)
                .px(size::S2)
                .rounded(size::RADIUS)
                .border_1()
                .border_color(t.line_2)
                .cursor_pointer()
                .hover({
                    let hover = t.ink_3;
                    move |s| s.border_color(hover)
                })
                .on_click(move |_, window, cx| toggle(window, cx))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .ui_strong()
                        .text_color(t.ink)
                        .child(focus_kit::ellipsis(label)),
                )
                .child(focus_kit::icon(Icon::CHEVRON_DOWN, size::ICON_SMALL).text_color(t.ink_3)),
        )
        .when(open, |this| {
            let list = focus_kit::fade_in(
                "fade-ops-dropdown",
                menus.attach(menu(t).mt(px(4.)), dismiss).children(
                    rows.into_iter()
                        .enumerate()
                        .map(|(index, (text, checked))| {
                            let pick = pick.clone();
                            MenuRow::new((id, index), text)
                                .checked(checked)
                                .on_click(move |_, window, cx| pick(index, window, cx))
                        }),
                ),
                cx,
            );
            this.child(deferred(anchored().snap_to_window().child(list)))
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Jump, landing};

    #[test]
    fn a_jump_lands_inside_the_list() {
        assert_eq!(landing(None, 0, 10, Jump::PageDown), None);
        assert_eq!(landing(Some(3), 50, 10, Jump::PageDown), Some(13));
        assert_eq!(landing(Some(45), 50, 10, Jump::PageDown), Some(49));
        assert_eq!(landing(Some(3), 50, 10, Jump::PageUp), Some(0));
        assert_eq!(landing(Some(30), 50, 10, Jump::PageUp), Some(20));
        assert_eq!(
            (
                landing(Some(7), 50, 10, Jump::First),
                landing(Some(7), 50, 10, Jump::Last)
            ),
            (Some(0), Some(49))
        );
        assert_eq!(landing(None, 50, 10, Jump::PageDown), Some(9));
        assert_eq!(landing(None, 4, 10, Jump::PageDown), Some(3));
    }
}
