use std::time::{Duration, Instant};

use focus_kit::{Toast, Tone, size};
use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div,
};

use crate::shell::Shell;

const VISIBLE: usize = 3;

#[derive(Clone, PartialEq)]
pub enum NoticeText {
    Plain(SharedString),
    Vault(focus_view_vault::Message),
}

impl NoticeText {
    fn text(&self) -> SharedString {
        match self {
            NoticeText::Plain(text) => text.clone(),
            NoticeText::Vault(message) => message.text().into(),
        }
    }
}

impl<T: Into<SharedString>> From<T> for NoticeText {
    fn from(text: T) -> NoticeText {
        NoticeText::Plain(text.into())
    }
}

struct Notice {
    id: u64,
    tone: Tone,
    text: NoticeText,
    left: Option<Duration>,
    countdown: Option<(Instant, gpui::Task<()>)>,
}

#[derive(Default)]
pub struct Notices {
    shown: Vec<Notice>,
    next: u64,
}

impl Shell {
    pub fn notify_user(
        &mut self,
        tone: Tone,
        text: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.notify(tone, NoticeText::Plain(text.into()), cx);
    }

    pub fn notify(&mut self, tone: Tone, text: NoticeText, cx: &mut Context<Self>) {
        let notices = &mut self.notices;
        notices
            .shown
            .retain(|notice| !(notice.tone == tone && notice.text == text));
        let id = notices.next;
        notices.next += 1;
        let left = (tone != Tone::Danger).then_some(focus_kit::motion::TOAST);
        notices.shown.push(Notice {
            id,
            tone,
            text,
            left,
            countdown: None,
        });
        if notices.shown.len() > VISIBLE {
            notices.shown.remove(0);
        }
        self.run_countdown(id, cx);
        cx.notify();
    }

    fn run_countdown(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(notice) = self.notices.shown.iter_mut().find(|notice| notice.id == id) else {
            return;
        };
        let Some(left) = notice.left else { return };
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(left).await;
            let _ = this.update(cx, |this, cx| this.close_notice(id, cx));
        });
        notice.countdown = Some((Instant::now(), task));
    }

    fn hover_notice(&mut self, id: u64, hovered: bool, cx: &mut Context<Self>) {
        let Some(notice) = self.notices.shown.iter_mut().find(|notice| notice.id == id) else {
            return;
        };
        if hovered {
            if let (Some((started, _)), Some(left)) = (notice.countdown.take(), notice.left) {
                notice.left = Some(left.saturating_sub(started.elapsed()));
            }
        } else if notice.countdown.is_none() {
            self.run_countdown(id, cx);
        }
    }

    pub fn hover_newest_notice(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.notices.shown.last().map(|notice| notice.id) else {
            return false;
        };
        self.hover_notice(id, true, cx);
        true
    }

    fn close_notice(&mut self, id: u64, cx: &mut Context<Self>) {
        self.notices.shown.retain(|notice| notice.id != id);
        cx.notify();
    }

    pub fn render_notices(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if self.notices.shown.is_empty() {
            return None;
        }
        let dir = focus_kit::Dir::current();
        let label = focus_i18n::t("ui-toast-close");
        let stack = self.notices.shown.iter().map(|notice| {
            let id = notice.id;
            div()
                .id(("notice", id as usize))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    this.hover_notice(id, *hovered, cx)
                }))
                .child(Toast::new(notice.tone, notice.text.text()).on_close(
                    ("notice-close", id as usize),
                    label.clone(),
                    cx.listener(move |this, _, _, cx| this.close_notice(id, cx)),
                ))
        });
        let column = div()
            .absolute()
            .bottom(size::STATUS_BAR + size::S2)
            .flex()
            .flex_col()
            .gap(size::S2);
        let column = if dir.rtl {
            column.items_start()
        } else {
            column.items_end()
        };
        Some(dir.end(column, size::S3).children(stack).into_any_element())
    }
}
