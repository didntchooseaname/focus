use std::sync::Arc;

use focus_core::NoteId;
use focus_kit::{ActiveTokens, Type, size};
use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, div, prelude::FluentBuilder, px,
};

use crate::Resolver;
use crate::model::{build, embedded_blocks};
use crate::view::{ReadingView, Typesetting};

pub const PREVIEW_WIDTH: f32 = 420.;
pub const PREVIEW_HEIGHT: f32 = 320.;
const BLOCKS: usize = 12;

pub struct LinkPreview {
    body: Entity<ReadingView>,
    title: SharedString,
    shown: Option<(NoteId, Option<String>)>,
    failure: Option<String>,
    task: Option<gpui::Task<()>>,
}

impl LinkPreview {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let body = cx.new(|cx| ReadingView::compact(Typesetting::compact(PREVIEW_WIDTH - 32.), cx));
        cx.observe(&body, |_, _, cx| cx.notify()).detach();
        LinkPreview {
            body,
            title: SharedString::default(),
            shown: None,
            failure: None,
            task: None,
        }
    }

    pub fn show(
        &mut self,
        note: NoteId,
        anchor: Option<String>,
        resolver: Arc<dyn Resolver>,
        cx: &mut Context<Self>,
    ) {
        if self.shown.as_ref() == Some(&(note, anchor.clone())) {
            return;
        }
        self.shown = Some((note, anchor.clone()));
        let task = cx.background_spawn(async move {
            let (text, meta) = match resolver.note_text(note)? {
                Ok(found) => found,
                Err(reason) => return Some(Err(reason)),
            };
            let document = focus_md::parse(&text, &focus_md::ParseOptions::default());
            let title = match &anchor {
                Some(anchor) => format!("{} › {}", meta.title, anchor.trim_start_matches('#')),
                None => meta.title.to_string(),
            };
            let Some(blocks) = embedded_blocks(&document, &meta, anchor.as_deref()) else {
                return Some(Err(focus_i18n::t_args(
                    "reading-embed-anchor-missing",
                    &[("anchor", anchor.unwrap_or_default().as_str().into())],
                )));
            };
            let excerpt = focus_md::Document {
                frontmatter: None,
                blocks: blocks.iter().take(BLOCKS).cloned().collect(),
                footnotes: document.footnotes.clone(),
                footnote_refs: document.footnote_refs.clone(),
                line_count: document.line_count,
            };
            Some(Ok((
                title,
                Arc::new(build(&excerpt, Some(&meta), resolver.as_ref())),
            )))
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.task = None;
                match result {
                    Some(Ok((title, built))) => {
                        this.title = title.into();
                        this.failure = None;
                        this.body.update(cx, |body, cx| body.set_built(built, cx));
                    }
                    Some(Err(reason)) => this.failure = Some(reason),
                    None => this.failure = Some(focus_i18n::t("reading-embed-missing")),
                }
                cx.notify();
            });
        }));
    }

    pub fn set_code_decoration(
        &mut self,
        decoration: Option<Arc<dyn crate::CodeDecoration>>,
        cx: &mut Context<Self>,
    ) {
        self.body
            .update(cx, |body, cx| body.set_code_decoration(decoration, cx));
    }

    pub fn script_code(&self, cx: &gpui::App) -> Vec<(u32, String, String)> {
        self.body.read(cx).script_code()
    }

    pub fn busy(&self) -> bool {
        self.task.is_some()
    }
}

impl Render for LinkPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *cx.tokens();
        div()
            .w(px(PREVIEW_WIDTH))
            .max_h(px(PREVIEW_HEIGHT))
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap(size::S2)
            .px(size::S4)
            .py(size::S3)
            .bg(t.card)
            .border_1()
            .border_color(t.line_2)
            .child(
                div()
                    .ui_strong()
                    .text_color(t.ink)
                    .truncate()
                    .child(self.title.clone()),
            )
            .when_some(self.failure.clone(), |this, reason| {
                this.child(div().ui_sm().text_color(t.ink_3).child(reason))
            })
            .when(self.failure.is_none(), |this| this.child(self.body.clone()))
    }
}
