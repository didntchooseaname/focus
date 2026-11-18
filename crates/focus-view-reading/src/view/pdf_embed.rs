use std::collections::HashMap;
use std::path::{Path, PathBuf};

use focus_core::NoteId;
use focus_kit::{Dir, Flow as _, Icon, Tokens, Type, size};
use focus_view_media::document::{Load, PdfDocument, Wanted, whole_tile};
use focus_view_media::pdf::{MAX_BITMAP_SIDE, PdfError};
use gpui::{
    AnyElement, Context, Entity, InteractiveElement, IntoElement, ParentElement, Styled,
    Subscription, Window, div, img, prelude::FluentBuilder, px,
};

use super::{LinkPage, ReadingEvent, ReadingView, Zone};
use crate::model::{LinkGoal, PdfEmbed};

const A4: (f32, f32) = (595., 842.);

#[derive(Default)]
pub(super) struct Documents {
    note: Option<NoteId>,
    open: HashMap<PathBuf, (Entity<PdfDocument>, Subscription)>,
    pub(super) drawn: Vec<String>,
}

impl ReadingView {
    pub fn script_pdf_embeds(&self) -> Vec<String> {
        self.pdf_documents.borrow().drawn.clone()
    }

    fn pdf_document(&self, path: &Path, cx: &mut Context<Self>) -> Entity<PdfDocument> {
        let note = self.built.as_ref().and_then(|b| b.note);
        let mut documents = self.pdf_documents.borrow_mut();
        if documents.note != note {
            documents.open.clear();
            documents.note = note;
        }
        if let Some((document, _)) = documents.open.get(path) {
            return document.clone();
        }
        let document = focus_view_media::document::open(path, cx);
        let observe = cx.observe(&document, |_, _, cx| cx.notify());
        documents
            .open
            .insert(path.to_path_buf(), (document.clone(), observe));
        document
    }

    pub(super) fn render_pdf_embed(
        &self,
        embed: &PdfEmbed,
        width: Option<u32>,
        item: usize,
        t: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((path, relative)) = &embed.file else {
            return self.missing(&embed.target, focus_i18n::t("media-pdf-missing"), t);
        };
        let document = self.pdf_document(path, cx);
        let (size, count) = {
            let document = document.read(cx);
            match document.load() {
                Load::Failed(error) => {
                    return self.missing(
                        &embed.target,
                        focus_view_media::pdf_view::pdf_error_text(error),
                        t,
                    );
                }
                Load::Opening => (A4, None),
                Load::Ready => match document.sizes().get(embed.page) {
                    Some(size) if !document.failed(embed.page) => {
                        (*size, Some(document.sizes().len()))
                    }
                    Some(_) => {
                        return self.missing(
                            &embed.target,
                            focus_view_media::pdf_view::pdf_error_text(PdfError::Unreadable),
                            t,
                        );
                    }
                    None => {
                        return self.missing(
                            &embed.target,
                            focus_i18n::t_args(
                                "media-pdf-no-page",
                                &[
                                    ("page", (embed.page + 1).into()),
                                    ("count", document.sizes().len().into()),
                                ],
                            ),
                            t,
                        );
                    }
                },
            }
        };
        let room = self.room().min(self.typesetting.measure) - 2.;
        let shown = width.map_or(room, |w| w as f32).min(room).max(1.);
        let page = embed.page;
        let tile = count.and_then(|_| {
            let side = MAX_BITMAP_SIDE as f32 / size.0.max(size.1).max(1.);
            let scale = (shown * window.scale_factor() / size.0.max(1.)).min(side);
            let (key, region) = whole_tile(page, size, scale)?;
            document.update(cx, |document, cx| {
                document.tile(&key).or_else(|| {
                    document.ask(Wanted::Render(key, region), cx);
                    document
                        .tiles(|k| k.0 == page && k.2[0] == 0 && k.2[1] == 0)
                        .into_iter()
                        .next()
                        .map(|(_, image)| image)
                })
            })
        });
        self.pdf_documents.borrow_mut().drawn.push(format!(
            "{} | page {} of {} | {}",
            embed.target,
            page + 1,
            count.map_or_else(|| "?".to_string(), |c| c.to_string()),
            if tile.is_some() { "drawn" } else { "sheet" }
        ));
        let file = Path::new(&embed.target).file_name().map_or_else(
            || embed.target.clone(),
            |n| n.to_string_lossy().into_owned(),
        );
        let open = {
            let relative = relative.clone();
            move |cx: &mut Context<Self>| {
                cx.emit(LinkPage(page));
                cx.emit(ReadingEvent::OpenLink {
                    goal: LinkGoal::Attachment(relative.clone()),
                    new_tab: false,
                });
            }
        };
        let group: gpui::SharedString = format!("pdf-embed-{item}-{page}").into();
        let button = self.live.then(|| {
            Dir::current()
                .end(div().absolute(), px(4.))
                .top(px(4.))
                .invisible()
                .group_hover(group.clone(), |s| s.visible())
                .child(
                    div()
                        .bg(t.card)
                        .border_1()
                        .border_color(t.float_border)
                        .child(
                            focus_kit::IconButton::new(
                                ("pdf-embed-open", item),
                                Icon::EXTERNAL,
                                focus_i18n::t("media-pdf-embed-open"),
                            )
                            .dense(true)
                            .on_click(cx.listener(move |_, _, _, cx| open(cx))),
                        ),
                )
        });
        let sheet = div()
            .id(group.clone())
            .group(group.clone())
            .relative()
            .w(px(shown + 2.))
            .max_w_full()
            .flex_shrink()
            .border_1()
            .border_color(t.line)
            .child({
                let mut room = div().w_full();
                room.style().aspect_ratio = Some(size.0 / size.1.max(1.));
                room.when_some(tile, |this, tile| this.child(img(tile).size_full()))
            })
            .children(button);
        let zone = Zone::PdfPage {
            relative: relative.clone(),
            page,
        };
        div()
            .flex()
            .flex_col()
            .gap(size::S2)
            .child(
                div()
                    .id(("pdf-embed", item))
                    .flow_row()
                    .when(!self.live, |this| {
                        this.cursor(gpui::CursorStyle::PointingHand)
                    })
                    .child(self.zone(sheet, zone)),
            )
            .child(
                Dir::current()
                    .row()
                    .gap(size::S2)
                    .ui_sm()
                    .text_color(t.ink_3)
                    .child(div().min_w_0().child(focus_kit::ellipsis(file)))
                    .when_some(count, |this, count| {
                        this.child(div().flex_none().mono_sm().child(focus_i18n::t_args(
                            "media-pdf-embed-page",
                            &[("page", (page + 1).into()), ("count", count.into())],
                        )))
                    }),
            )
            .into_any_element()
    }
}

impl ReadingView {
    fn missing(&self, target: &str, why: String, t: &Tokens) -> AnyElement {
        self.pdf_documents
            .borrow_mut()
            .drawn
            .push(format!("{target} | missing: {why}"));
        missing_line(target, why, t)
    }
}

fn missing_line(target: &str, why: String, t: &Tokens) -> AnyElement {
    let dir = Dir::current();
    dir.bar_start(
        dir.pad_start(div().flex().flex_col().gap(size::S2).py(size::S2), size::S4),
        t.line_2,
    )
    .child(
        div()
            .ui_strong()
            .text_color(t.ink)
            .child(target.to_string()),
    )
    .child(div().ui_sm().text_color(t.ink_3).child(why))
    .into_any_element()
}
