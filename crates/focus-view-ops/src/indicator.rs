use focus_kit::{ActiveTokens, Type, dot, size};
use gpui::{
    Context, Entity, EventEmitter, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};

use crate::event::OpsEvent;
use crate::sidebar::{ExamView, exam_view};
use crate::state::OpsState;
use crate::time::{clock_text, now_unix};

pub struct OpsIndicator {
    state: Entity<OpsState>,
    _subscription: Subscription,
    ticker: crate::ui::Ticker,
}

impl EventEmitter<OpsEvent> for OpsIndicator {}

impl OpsIndicator {
    pub fn new(state: Entity<OpsState>, cx: &mut Context<Self>) -> OpsIndicator {
        let subscription = cx.observe(&state, |this, state, cx| {
            let running = crate::ui::Ticker::running(state.read(cx));
            this.ticker.sync(running, cx);
            cx.notify();
        });
        OpsIndicator {
            state,
            _subscription: subscription,
            ticker: Default::default(),
        }
    }
}

impl Render for OpsIndicator {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        let alert_seconds = u64::from(self.state.read(cx).settings.alert_minutes) * 60;
        let Some(active) = self.state.read(cx).active.clone() else {
            return div().id("ops-indicator");
        };
        let now = now_unix();
        let paused = focus_ops::timer::Timer::of(&active.engagement).state()
            == focus_ops::timer::State::Paused;
        let timer = match exam_view(&active, now) {
            Some(ExamView {
                clock: Some(clock), ..
            }) if clock.expired => Some((0, t.danger)),
            Some(ExamView {
                clock: Some(clock), ..
            }) => Some((
                clock.remaining_secs,
                if clock.remaining_secs < alert_seconds {
                    t.danger
                } else {
                    t.ink
                },
            )),
            Some(_) => None,
            None => {
                let timer = focus_ops::timer::Timer::of(&active.engagement);
                timer.started.map(|_| (timer.worked(now), t.ink_2))
            }
        };
        crate::ui::row()
            .id("ops-indicator")
            .items_center()
            .gap(size::S2)
            .h(size::CONTROL)
            .px(size::S2)
            .rounded(size::RADIUS)
            .cursor_pointer()
            .hover(move |s| s.bg(t.card_2))
            .on_click(cx.listener(|_, _, _, cx| cx.emit(OpsEvent::DashboardRequested)))
            .child(dot(if paused { t.warn } else { t.ok }))
            .child(
                div()
                    .max_w(px(240.))
                    .min_w_0()
                    .ui_sm()
                    .text_color(t.ink_2)
                    .child(focus_kit::ellipsis(active.engagement.name.clone())),
            )
            .children(timer.map(|(seconds, color)| {
                div()
                    .flex_none()
                    .mono_sm()
                    .text_color(color)
                    .child(clock_text(seconds))
            }))
    }
}
