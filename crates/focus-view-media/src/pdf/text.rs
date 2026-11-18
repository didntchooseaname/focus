use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::kurbo::{Affine, BezPath, Point, Rect};
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, TransformExt as _, interpret_page,
};
use hayro::hayro_syntax::page::Page;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageText {
    pub chars: Vec<char>,
    pub boxes: Vec<[f32; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub page: usize,
    pub start: usize,
    pub end: usize,
}

struct Collector {
    text: PageText,
    last: Option<(Point, Point, f64)>,
}

impl Collector {
    fn push_glyph(&mut self, unicode: String, transform: Affine, advance: f64) {
        let size =
            (transform.as_coeffs()[2].powi(2) + transform.as_coeffs()[3].powi(2)).sqrt() * 1000.;
        let origin = transform * Point::new(0., 0.);
        let end = transform * Point::new(advance, 0.);
        if let Some((_, last_end, last_size)) = self.last {
            let em = last_size.max(size).max(0.1);
            let line_change = (origin.y - last_end.y).abs() > em * 0.5;
            if line_change {
                self.separator('\n');
            } else if (origin.x - last_end.x).abs() > em * 0.15 {
                self.separator(' ');
            }
        }
        let corners = [
            transform * Point::new(0., -200.),
            transform * Point::new(advance, -200.),
            transform * Point::new(0., 800.),
            transform * Point::new(advance, 800.),
        ];
        let rect = corners
            .iter()
            .fold(Rect::from_points(corners[0], corners[0]), |r, p| {
                r.union_pt(*p)
            });
        let count = unicode.chars().count().max(1) as f64;
        for (i, ch) in unicode.chars().enumerate() {
            let x0 = rect.x0 + rect.width() * i as f64 / count;
            let x1 = rect.x0 + rect.width() * (i + 1) as f64 / count;
            self.text.chars.push(ch);
            self.text
                .boxes
                .push([x0 as f32, rect.y0 as f32, x1 as f32, rect.y1 as f32]);
        }
        self.last = Some((origin, end, size));
    }

    fn separator(&mut self, ch: char) {
        if self.text.chars.last().is_some_and(|c| c.is_whitespace()) {
            return;
        }
        self.text.chars.push(ch);
        self.text.boxes.push([0.; 4]);
    }
}

impl<'a> Device<'a> for Collector {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        for glyph in run.glyphs() {
            let Some(unicode) = glyph.as_unicode() else {
                continue;
            };
            let unicode = match unicode {
                hayro::hayro_interpret::hayro_cmap::BfString::Char(c) => c.to_string(),
                hayro::hayro_interpret::hayro_cmap::BfString::String(s) => s,
            };
            let advance = match &**glyph {
                Glyph::Outline(outline) => outline.advance_width(),
                Glyph::Type3(type3) => type3.advance_width(),
            }
            .unwrap_or(500.);
            self.push_glyph(
                unicode,
                props.transform * glyph.transform(),
                f64::from(advance),
            );
        }
    }
}

pub fn extract<'a>(page: &Page<'a>, settings: &InterpreterSettings) -> PageText {
    let (width, height) = page.render_dimensions();
    let cache = InterpreterCache::new();
    let mut context = Context::new(
        page.initial_transform(true).to_kurbo(),
        Rect::new(0., 0., f64::from(width), f64::from(height)),
        &cache,
        page.xref(),
        settings.clone(),
    );
    let mut collector = Collector {
        text: PageText::default(),
        last: None,
    };
    interpret_page(page, &mut context, &mut collector);
    collector.text
}

fn folded(c: char) -> char {
    match c {
        '\n' | '\t' | '\u{a0}' => ' ',
        c => c.to_lowercase().next().unwrap_or(c),
    }
}

impl PageText {
    pub fn find(&self, query: &str) -> Vec<(usize, usize)> {
        let needle: Vec<char> = query.chars().map(folded).collect();
        if needle.is_empty() || needle.len() > self.chars.len() {
            return Vec::new();
        }
        let hay: Vec<char> = self.chars.iter().map(|c| folded(*c)).collect();
        let mut found = Vec::new();
        let mut i = 0;
        while i + needle.len() <= hay.len() {
            if hay[i..i + needle.len()] == needle[..] {
                found.push((i, i + needle.len()));
                i += needle.len();
            } else {
                i += 1;
            }
        }
        found
    }

    pub fn rects(&self, start: usize, end: usize) -> Vec<[f32; 4]> {
        let mut out: Vec<[f32; 4]> = Vec::new();
        for b in self
            .boxes
            .get(start..end.min(self.boxes.len()))
            .unwrap_or(&[])
        {
            if b[2] <= b[0] {
                continue;
            }
            match out.last_mut() {
                Some(last) if (last[1] - b[1]).abs() < 0.5 * (b[3] - b[1]) && b[0] >= last[0] => {
                    last[2] = last[2].max(b[2]);
                    last[1] = last[1].min(b[1]);
                    last[3] = last[3].max(b[3]);
                }
                _ => out.push(*b),
            }
        }
        out
    }

    pub fn index_at(&self, (x, y): (f32, f32)) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, b) in self.boxes.iter().enumerate() {
            if b[2] <= b[0] {
                continue;
            }
            let dy = if y < b[1] {
                b[1] - y
            } else if y > b[3] {
                y - b[3]
            } else {
                0.
            };
            let dx = if x < b[0] {
                b[0] - x
            } else if x > b[2] {
                x - b[2]
            } else {
                0.
            };
            let cost = dy * 1000. + dx;
            if best.is_none_or(|(_, c)| cost < c) {
                best = Some((i, cost));
            }
        }
        let (i, _) = best?;
        let b = self.boxes[i];
        Some(if x > (b[0] + b[2]) / 2. { i + 1 } else { i })
    }

    pub fn slice(&self, start: usize, end: usize) -> String {
        self.chars
            .get(start..end.min(self.chars.len()))
            .unwrap_or(&[])
            .iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Renderer, open, sample};

    #[test]
    fn the_text_of_a_page_comes_with_the_box_of_each_character() {
        let pdf = open(sample::document()).unwrap();
        let renderer = Renderer::new();
        let pages = pdf.pages();
        let text = renderer.text(&pages[0]);
        assert_eq!(text.slice(0, text.chars.len()), "Findings summary");
        assert_eq!(text.chars.len(), text.boxes.len());
        let f = text.boxes[0];
        assert!((f[0] - 72.).abs() < 0.01, "{f:?}");
        assert!((f[1] - (142. - 9.6)).abs() < 0.01, "{f:?}");
        assert!((f[3] - (142. + 2.4)).abs() < 0.01, "{f:?}");
        assert!(f[2] > 72. + 6. && f[2] < 72. + 9., "{f:?}");
        assert!(
            text.boxes
                .windows(2)
                .all(|w| w[1][0] >= w[0][0] || w[1][2] == 0.)
        );
    }

    #[test]
    fn find_is_case_folded_and_returns_boxes_per_line() {
        let pdf = open(sample::document()).unwrap();
        let renderer = Renderer::new();
        let pages = pdf.pages();
        let second = renderer.text(&pages[1]);
        assert_eq!(
            second.slice(0, second.chars.len()),
            "Second page\nFindings summary"
        );
        assert_eq!(second.find("SUMMARY"), vec![(21, 28)]);
        assert_eq!(second.find("page findings"), vec![(7, 20)]);
        let rects = second.rects(7, 20);
        assert_eq!(rects.len(), 2, "{rects:?}");
        assert!(second.find("").is_empty());
        assert!(second.find("absent").is_empty());
    }

    #[test]
    fn a_point_selects_the_nearest_character_boundary() {
        let pdf = open(sample::document()).unwrap();
        let renderer = Renderer::new();
        let pages = pdf.pages();
        let text = renderer.text(&pages[0]);
        assert_eq!(text.index_at((60., 140.)), Some(0));
        assert_eq!(text.index_at((400., 140.)), Some(text.chars.len()));
        let at = text.boxes[9];
        assert_eq!(text.index_at((at[0] + 0.5, 300.)), Some(9));
    }
}
