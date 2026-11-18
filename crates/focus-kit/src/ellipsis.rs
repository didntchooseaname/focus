use std::ops::Range;

use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, HighlightStyle, InspectorElementId,
    IntoElement, LayoutId, Pixels, ShapedLine, SharedString, Style, TextRun, Window, px, relative,
};

const SUFFIX: &str = "…";

pub struct Ellipsis {
    text: SharedString,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
}

pub fn ellipsis(text: impl Into<SharedString>) -> Ellipsis {
    let text: SharedString = text.into();
    let text = if text.contains('\n') {
        text.replace('\n', " ").into()
    } else {
        text
    };
    Ellipsis {
        text,
        highlights: Vec::new(),
    }
}

impl Ellipsis {
    pub fn highlights(
        mut self,
        highlights: impl IntoIterator<Item = (Range<usize>, HighlightStyle)>,
    ) -> Self {
        self.highlights = highlights.into_iter().collect();
        self.highlights.sort_by_key(|(range, _)| range.start);
        self
    }

    fn runs(&self, window: &Window) -> Vec<TextRun> {
        let style = window.text_style();
        let mut runs = Vec::new();
        let mut at = 0;
        for (range, highlight) in &self.highlights {
            let range = range.start.max(at).min(self.text.len())..range.end.min(self.text.len());
            if range.start >= range.end {
                continue;
            }
            if range.start > at {
                runs.push(style.to_run(range.start - at));
            }
            runs.push(style.clone().highlight(*highlight).to_run(range.len()));
            at = range.end;
        }
        if at < self.text.len() || runs.is_empty() {
            runs.push(style.to_run(self.text.len() - at));
        }
        runs
    }
}

pub struct EllipsisLayout {
    full: ShapedLine,
    runs: Vec<TextRun>,
    font_size: Pixels,
    line_height: Pixels,
}

impl Element for Ellipsis {
    type RequestLayoutState = EllipsisLayout;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
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
    ) -> (LayoutId, Self::RequestLayoutState) {
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line_height = text_style
            .line_height
            .to_pixels(font_size.into(), window.rem_size());
        let runs = self.runs(window);
        let full = window
            .text_system()
            .shape_line(self.text.clone(), font_size, &runs, None);
        let mut style = Style::default();
        style.size.width = full.width.ceil().into();
        style.size.height = line_height.into();
        style.min_size.width = px(0.).into();
        style.max_size.width = relative(1.).into();
        style.flex_shrink = 1.;
        let layout_id = window.request_layout(style, [], cx);
        (
            layout_id,
            EllipsisLayout {
                full,
                runs,
                font_size,
                line_height,
            },
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let line = if layout.full.width <= bounds.size.width + px(0.5) {
            layout.full.clone()
        } else {
            let font = layout
                .runs
                .first()
                .map(|run| run.font.clone())
                .unwrap_or_else(|| window.text_style().font());
            let mut runs = layout.runs.clone();
            let mut wrapper = window.text_system().line_wrapper(font, layout.font_size);
            let cut =
                wrapper.truncate_line(self.text.clone(), bounds.size.width, SUFFIX, &mut runs);
            window
                .text_system()
                .shape_line(cut, layout.font_size, &runs, None)
        };
        if let Err(error) = line.paint(bounds.origin, layout.line_height, window, cx) {
            eprintln!("focus-kit: text not painted: {error:#}");
        }
    }
}

impl IntoElement for Ellipsis {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
