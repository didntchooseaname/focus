use gpui::{FontWeight, Pixels, Styled, px};

pub const SANS: &str = "Inter";
pub const DISPLAY: &str = "Sora";
pub const MONO: &str = "JetBrains Mono";

pub const TRACKING_SNUG: f32 = -0.02;
pub const TRACKING_TIGHT: f32 = -0.03;
const TIGHT_FROM: Pixels = px(24.);

pub fn tracking(family: &str, size: Pixels) -> Pixels {
    if family != DISPLAY {
        return Pixels::ZERO;
    }
    size * if size >= TIGHT_FROM {
        TRACKING_TIGHT
    } else {
        TRACKING_SNUG
    }
}

pub trait Type: Styled + Sized {
    fn ui_xs(self) -> Self {
        self.font_family(SANS)
            .text_size(px(11.5))
            .line_height(px(16.))
            .font_weight(FontWeight::MEDIUM)
    }
    fn ui_sm(self) -> Self {
        self.font_family(SANS)
            .text_size(px(12.5))
            .line_height(px(18.))
            .font_weight(FontWeight::NORMAL)
    }
    fn ui(self) -> Self {
        self.font_family(SANS)
            .text_size(px(13.))
            .line_height(px(20.))
            .font_weight(FontWeight::NORMAL)
    }
    fn ui_strong(self) -> Self {
        self.font_family(SANS)
            .text_size(px(13.))
            .line_height(px(20.))
            .font_weight(FontWeight::SEMIBOLD)
    }
    fn ui_lg(self) -> Self {
        self.font_family(SANS)
            .text_size(px(14.))
            .line_height(px(22.))
            .font_weight(FontWeight::MEDIUM)
    }
    fn mono_sm(self) -> Self {
        self.font_family(MONO)
            .text_size(px(11.5))
            .line_height(px(16.))
            .font_weight(FontWeight::NORMAL)
    }
    fn mono(self) -> Self {
        self.font_family(MONO)
            .text_size(px(12.5))
            .line_height(px(20.))
            .font_weight(FontWeight::NORMAL)
    }
    fn display_sm(self) -> Self {
        self.font_family(DISPLAY)
            .text_size(px(18.))
            .letter_spacing(tracking(DISPLAY, px(18.)))
            .line_height(px(24.))
            .font_weight(FontWeight::SEMIBOLD)
    }
    fn display(self) -> Self {
        self.font_family(DISPLAY)
            .text_size(px(28.))
            .letter_spacing(tracking(DISPLAY, px(28.)))
            .line_height(px(32.))
            .font_weight(FontWeight::SEMIBOLD)
    }
    fn stat(self) -> Self {
        self.font_family(DISPLAY)
            .text_size(px(30.))
            .letter_spacing(tracking(DISPLAY, px(30.)))
            .line_height(px(32.))
            .font_weight(FontWeight::SEMIBOLD)
    }
    fn body(self, base: f32) -> Self {
        self.font_family(SANS)
            .text_size(px(base))
            .line_height(px((base * 1.65).round()))
            .font_weight(FontWeight::NORMAL)
    }
}

impl<T: Styled> Type for T {}

#[cfg(test)]
mod text_layout_tests {
    use super::*;
    use gpui::{FontId, GlyphId, LineLayout, ShapedGlyph, ShapedRun, WrappedLineLayout, point};
    use std::sync::Arc;

    fn shaped(text: &str, glyphs: &[(usize, f32)], width: f32) -> LineLayout {
        LineLayout {
            font_size: px(16.),
            width: px(width),
            ascent: px(12.),
            descent: px(4.),
            runs: vec![ShapedRun {
                font_id: FontId(0),
                glyphs: glyphs
                    .iter()
                    .map(|&(index, x)| ShapedGlyph {
                        id: GlyphId::from(1),
                        position: point(px(x), px(0.)),
                        index,
                        is_emoji: false,
                    })
                    .collect(),
            }],
            len: text.len(),
        }
    }

    fn xs(layout: &LineLayout) -> Vec<f32> {
        layout.runs[0]
            .glyphs
            .iter()
            .map(|g| f32::from(g.position.x))
            .collect()
    }

    fn sora() -> LineLayout {
        shaped("Sora", &[(0, 0.), (1, 10.), (2, 20.), (3, 30.)], 40.)
    }

    #[test]
    fn display_styles_take_the_tracking_of_design() {
        assert_eq!(tracking(DISPLAY, px(18.)), px(18. * -0.02));
        assert_eq!(tracking(DISPLAY, px(20.)), px(20. * -0.02));
        assert_eq!(tracking(DISPLAY, px(26.)), px(26. * -0.03));
        assert_eq!(tracking(DISPLAY, px(28.)), px(28. * -0.03));
        assert_eq!(tracking(DISPLAY, px(30.)), px(30. * -0.03));
        assert_eq!(tracking(DISPLAY, px(34.)), px(34. * -0.03));
        assert_eq!(tracking(SANS, px(34.)), Pixels::ZERO);
        assert_eq!(tracking(MONO, px(18.)), Pixels::ZERO);
    }

    #[test]
    fn without_tracking_the_line_is_unchanged() {
        let mut line = sora();
        line.apply_letter_spacing("Sora", Pixels::ZERO);
        assert_eq!(xs(&line), vec![0., 10., 20., 30.]);
        assert_eq!(line.width, px(40.));
        assert_eq!(line.x_for_index(2), px(20.));
        assert_eq!(line.x_for_index(4), px(40.));
        assert_eq!(line.closest_index_for_x(px(14.)), 1);
        assert_eq!(line.closest_index_for_x(px(16.)), 2);
        assert_eq!(line.index_for_x(px(29.)), Some(2));
    }

    #[test]
    fn tracking_moves_each_character_and_narrows_the_line() {
        let mut line = sora();
        line.apply_letter_spacing("Sora", px(-1.));
        assert_eq!(xs(&line), vec![0., 9., 18., 27.]);
        assert_eq!(
            line.width,
            px(37.),
            "three gaps, none after the last character"
        );
        assert_eq!(line.x_for_index(2), px(18.));
        assert_eq!(line.x_for_index(4), px(37.));
        assert_eq!(line.closest_index_for_x(px(13.)), 1);
        assert_eq!(line.closest_index_for_x(px(14.)), 2);
        assert_eq!(line.closest_index_for_x(px(36.)), 4);
        assert_eq!(line.index_for_x(px(26.5)), Some(2));
        assert_eq!(line.index_for_x(px(27.)), Some(3));
        assert_eq!(line.index_for_x(px(37.)), None);
    }

    #[test]
    fn tracking_moves_the_wrap_boundary_with_the_glyphs() {
        let text = "ab cd";
        let glyphs = [(0, 0.), (1, 10.), (2, 20.), (3, 30.), (4, 40.)];
        let plain = shaped(text, &glyphs, 50.);
        assert_eq!(plain.compute_wrap_boundaries(text, px(45.), None).len(), 1);
        let mut tight = shaped(text, &glyphs, 50.);
        tight.apply_letter_spacing(text, px(-1.5));
        assert_eq!(tight.width, px(44.));
        assert!(
            tight
                .compute_wrap_boundaries(text, px(45.), None)
                .is_empty(),
            "the tracked line fits in 45"
        );
        let boundaries = tight.compute_wrap_boundaries(text, px(30.), None);
        assert_eq!(boundaries.len(), 1);
        assert_eq!((boundaries[0].run_ix, boundaries[0].glyph_ix), (0, 3));

        let wrapped = WrappedLineLayout {
            unwrapped_layout: Arc::new(tight),
            wrap_boundaries: boundaries,
            wrap_width: Some(px(30.)),
        };
        let line_height = px(20.);
        assert_eq!(
            wrapped.position_for_index(3, line_height),
            Some(point(px(25.5), px(0.))),
            "after the space that ends the first line"
        );
        assert_eq!(
            wrapped.position_for_index(4, line_height),
            Some(point(px(34. - 25.5), px(20.)))
        );
        assert_eq!(
            wrapped.position_for_index(5, line_height),
            Some(point(px(44. - 25.5), px(20.)))
        );
        assert_eq!(
            wrapped.index_for_position(point(px(9.), px(25.)), line_height),
            Ok(4)
        );
        assert_eq!(
            wrapped.closest_index_for_position(point(px(3.), px(5.)), line_height),
            Ok(0)
        );
    }

    fn breaks(text: &str) -> Vec<usize> {
        let mut breaks = gpui::LineBreaks::default();
        text.char_indices()
            .filter_map(|(ix, c)| breaks.next(ix, c))
            .collect()
    }

    #[test]
    fn a_line_never_starts_with_closing_punctuation() {
        assert_eq!(breaks("a (b), c"), vec![2, 7]);
        assert_eq!(breaks("mot : suite ; fin !"), vec![6, 14]);
        assert_eq!(breaks("say \"hi\" now"), vec![4, 9]);
    }

    #[test]
    fn inline_code_isolates_stay_with_what_follows() {
        let text = "list (\u{2066}x\u{2069}, \u{2066}tags\u{2069})";
        assert_eq!(breaks(text), vec![5, 15]);
    }

    #[test]
    fn chinese_keeps_its_closing_punctuation_at_the_end_of_a_line() {
        assert_eq!(breaks("你好，世界。"), vec![3, 9, 12]);
        assert_eq!(breaks("「引用」文字"), vec![6, 12, 15]);
    }

    #[test]
    fn the_wrap_of_a_shaped_line_does_not_start_with_closing_punctuation() {
        let text = "aa (bb), cc";
        let glyphs: Vec<(usize, f32)> = (0..text.len()).map(|i| (i, i as f32 * 10.)).collect();
        let line = shaped(text, &glyphs, 110.);
        let boundaries: Vec<(usize, usize)> = line
            .compute_wrap_boundaries(text, px(70.), None)
            .iter()
            .map(|b| (b.run_ix, b.glyph_ix))
            .collect();
        assert_eq!(
            boundaries,
            vec![(0, 3), (0, 9)],
            "before the opening bracket and before the last word, not before ')' or ','"
        );
    }

    #[test]
    fn arabic_takes_no_tracking() {
        let text = "مرحبا";
        let glyphs = [(8, 0.), (6, 10.), (4, 20.), (2, 30.), (0, 40.)];
        let mut line = shaped(text, &glyphs, 50.);
        line.apply_letter_spacing(text, px(-1.));
        assert_eq!(xs(&line), vec![0., 10., 20., 30., 40.]);
        assert_eq!(line.width, px(50.));
    }

    #[test]
    fn latin_beside_arabic_keeps_its_tracking_alone() {
        let text = "ab مر";
        let glyphs = [(0, 0.), (1, 10.), (2, 20.), (5, 30.), (3, 40.)];
        let mut line = shaped(text, &glyphs, 50.);
        line.apply_letter_spacing(text, px(-1.));
        assert_eq!(xs(&line), vec![0., 9., 18., 28., 38.]);
        assert_eq!(line.width, px(48.));
    }

    #[test]
    fn the_glyphs_of_one_character_stay_together() {
        let text = "e\u{301}x";
        let glyphs = [(0, 0.), (0, 2.), (3, 10.)];
        let mut line = shaped(text, &glyphs, 20.);
        line.apply_letter_spacing(text, px(-1.));
        assert_eq!(xs(&line), vec![0., 2., 9.]);
        assert_eq!(line.width, px(19.));
        assert_eq!(line.x_for_index(3), px(9.));
    }
}
