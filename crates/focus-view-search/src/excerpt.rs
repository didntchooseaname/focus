use std::ops::Range;

use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, HighlightStyle, InspectorElementId,
    IntoElement, LayoutId, Pixels, ShapedLine, SharedString, Style, TextRun, TextStyle, Window, px,
    relative,
};

const MARK: &str = "…";

pub(crate) fn window_start(
    text: &str,
    first_match: usize,
    budget: f32,
    x_of: impl Fn(usize) -> f32,
) -> usize {
    let target = x_of(first_match);
    if target <= budget {
        return 0;
    }
    let mut start = first_match.min(text.len());
    for (offset, _) in text[..start].char_indices().rev() {
        if target - x_of(offset) > budget {
            break;
        }
        start = offset;
    }
    start
}

pub(crate) fn shift_ranges(
    ranges: &[Range<usize>],
    cut: usize,
    prefix: usize,
) -> Vec<Range<usize>> {
    ranges
        .iter()
        .filter(|r| r.end > cut)
        .map(|r| r.start.max(cut) - cut + prefix..r.end - cut + prefix)
        .collect()
}

pub struct Excerpt {
    text: SharedString,
    ranges: Vec<Range<usize>>,
    mark: HighlightStyle,
    keep_start: bool,
}

pub fn excerpt(
    text: impl Into<SharedString>,
    ranges: Vec<Range<usize>>,
    mark: HighlightStyle,
) -> Excerpt {
    Excerpt {
        text: text.into(),
        ranges,
        mark,
        keep_start: false,
    }
}

impl Excerpt {
    pub fn keep_start(mut self) -> Self {
        self.keep_start = true;
        self
    }

    fn runs(&self, text: &str, ranges: &[Range<usize>], style: &TextStyle) -> Vec<TextRun> {
        let mark = self.mark;
        let mut runs = Vec::new();
        let mut at = 0;
        for range in ranges {
            let range = range.start.max(at).min(text.len())..range.end.min(text.len());
            if range.start >= range.end
                || !text.is_char_boundary(range.start)
                || !text.is_char_boundary(range.end)
            {
                continue;
            }
            if range.start > at {
                runs.push(style.to_run(range.start - at));
            }
            runs.push(style.clone().highlight(mark).to_run(range.len()));
            at = range.end;
        }
        if at < text.len() || runs.is_empty() {
            runs.push(style.to_run(text.len() - at));
        }
        runs
    }
}

pub struct ExcerptLayout {
    full: ShapedLine,
    font_size: Pixels,
    line_height: Pixels,
}

impl Element for Excerpt {
    type RequestLayoutState = ExcerptLayout;
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
    ) -> (LayoutId, ExcerptLayout) {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style
            .line_height
            .to_pixels(font_size.into(), window.rem_size());
        let runs = self.runs(&self.text, &self.ranges, &style);
        let full = window
            .text_system()
            .shape_line(self.text.clone(), font_size, &runs, None);
        let mut layout = Style::default();
        layout.size.width = full.width.ceil().into();
        layout.size.height = line_height.into();
        layout.min_size.width = px(0.).into();
        layout.max_size.width = relative(1.).into();
        layout.flex_shrink = 1.;
        (
            window.request_layout(layout, [], cx),
            ExcerptLayout {
                full,
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
        _: &mut ExcerptLayout,
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut ExcerptLayout,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let width = bounds.size.width;
        let line = if layout.full.width <= width + px(0.5) {
            layout.full.clone()
        } else {
            let style = window.text_style();
            let first = self.ranges.first().map_or(0, |r| r.start);
            let start = if self.keep_start {
                0
            } else {
                window_start(&self.text, first, f32::from(width) * 0.3, |i| {
                    f32::from(layout.full.x_for_index(i))
                })
            };
            let (text, ranges): (SharedString, Vec<Range<usize>>) = if start == 0 {
                (self.text.clone(), self.ranges.clone())
            } else {
                (
                    format!("{MARK}{}", &self.text[start..]).into(),
                    shift_ranges(&self.ranges, start, MARK.len()),
                )
            };
            let mut runs = self.runs(&text, &ranges, &style);
            let mut wrapper = window
                .text_system()
                .line_wrapper(style.font(), layout.font_size);
            let cut = wrapper.truncate_line(text, width, MARK, &mut runs);
            window
                .text_system()
                .shape_line(cut, layout.font_size, &runs, None)
        };
        let origin = bounds.origin;
        if let Err(error) = line
            .paint_background(origin, layout.line_height, window, cx)
            .and_then(|()| line.paint(origin, layout.line_height, window, cx))
        {
            eprintln!("focus-view-search: text not painted: {error:#}");
        }
    }
}

impl IntoElement for Excerpt {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    fn mono(i: usize) -> f32 {
        i as f32 * 7.0
    }

    #[test]
    fn short_lines_start_at_zero() {
        assert_eq!(window_start("abcdefghij", 3, 60.0, mono), 0);
    }

    #[test]
    fn long_prefix_is_cut_to_the_budget() {
        let text = "0123456789".repeat(5);
        let start = window_start(&text, 40, 28.0, mono);
        assert_eq!(start, 36);
        assert!(mono(40) - mono(start) <= 28.0);
    }

    #[test]
    fn cuts_land_on_character_boundaries() {
        let text = "é".repeat(30);
        let start = window_start(&text, 40, 20.0, |i| i as f32 * 5.0);
        assert!(text.is_char_boundary(start));
    }

    #[test]
    fn ranges_follow_the_cut() {
        assert_eq!(shift_ranges(&[2..5, 10..14, 20..22], 12, 3), [3..5, 11..13]);
        assert_eq!(shift_ranges(&[0..4], 0, 0), [0..4]);
    }
}
