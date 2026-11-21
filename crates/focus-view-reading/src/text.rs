use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use focus_kit::Tokens;
use focus_md::highlight::Token;
use gpui::{
    App, Bounds, Element, ElementId, Font, FontStyle, FontWeight, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, SharedString, StrikethroughStyle,
    StyledText, TextLayout, TextRun, UnderlineStyle, Window, point, px,
};

use super::model::{LinkGoal, Marks, Rich};
use crate::srcmap::SourceMap;

pub type TextKey = (u32, u32);

pub struct Hit {
    pub key: TextKey,
    pub bounds: Bounds<Pixels>,
    pub layout: TextLayout,
    pub rich: Arc<Rich>,
    pub map: Option<Arc<SourceMap>>,
    pub caret: Option<usize>,
    pub selection: Option<Range<usize>>,
}

#[derive(Clone, Default)]
pub struct Registry(pub Rc<RefCell<Vec<Hit>>>);

impl Registry {
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    pub fn locate(&self, position: Point<Pixels>) -> Option<(TextKey, usize, Arc<Rich>)> {
        let hits = self.0.borrow();
        let hit = nearest(hits.iter(), position)?;
        let index = match hit.layout.index_for_position(position) {
            Ok(i) | Err(i) => i.min(hit.rich.text.len()),
        };
        Some((hit.key, index, hit.rich.clone()))
    }

    pub fn locate_caret(
        &self,
        position: Point<Pixels>,
    ) -> Option<(TextKey, usize, Arc<SourceMap>)> {
        let hits = self.0.borrow();
        if hits
            .iter()
            .any(|h| h.map.is_none() && h.bounds.contains(&position))
        {
            return None;
        }
        let hit = nearest(hits.iter().filter(|h| h.map.is_some()), position)?;
        let index = hit
            .layout
            .closest_index_for_position(position)
            .min(hit.rich.text.len());
        Some((hit.key, index, hit.map.clone()?))
    }
}

fn nearest<'a>(
    hits: impl Iterator<Item = &'a Hit> + Clone,
    position: Point<Pixels>,
) -> Option<&'a Hit> {
    let inside = hits.clone().find(|h| h.bounds.contains(&position));
    inside.or_else(|| {
        hits.min_by_key(|h| {
            let dy = if position.y < h.bounds.top() {
                h.bounds.top() - position.y
            } else if position.y > h.bounds.bottom() {
                position.y - h.bounds.bottom()
            } else {
                px(0.)
            };
            let dx = if position.x < h.bounds.left() {
                h.bounds.left() - position.x
            } else if position.x > h.bounds.right() {
                position.x - h.bounds.right()
            } else {
                px(0.)
            };
            ((dy / px(1.) * 4. + dx / px(1.)) * 16.) as i64
        })
    })
}

#[derive(Clone)]
pub struct Base {
    pub family: SharedString,
    pub mono: SharedString,
    pub size: Pixels,
    pub line_height: Pixels,
    pub color: Hsla,
    pub weight: FontWeight,
    pub italic: bool,
    pub tinted: bool,
}

fn token_style(token: Token, t: &Tokens, tinted: bool) -> (Hsla, FontWeight, bool) {
    let dark = t.mode.is_dark();
    let tint = |dark_hex: u32, light_hex: u32, fallback: Hsla| -> Hsla {
        if tinted {
            gpui::rgb(if dark { dark_hex } else { light_hex }).into()
        } else {
            fallback
        }
    };
    match token {
        Token::Keyword | Token::Heading => (t.ink, FontWeight::SEMIBOLD, false),
        Token::Function => (tint(0x9ecbff, 0x0550ae, t.ink), FontWeight::NORMAL, false),
        Token::Type | Token::Tag | Token::Attribute => (
            tint(0xc9b3ff, 0x6639ba, t.ink),
            if tinted {
                FontWeight::NORMAL
            } else {
                FontWeight::MEDIUM
            },
            false,
        ),
        Token::String => (tint(0xa5d6a7, 0x1a7f37, t.ink_2), FontWeight::NORMAL, false),
        Token::Number | Token::Constant => {
            (tint(0xe6c384, 0x953800, t.ink), FontWeight::NORMAL, false)
        }
        Token::Variable | Token::Property | Token::Flag => (t.ink_body, FontWeight::NORMAL, false),
        Token::Operator | Token::Punctuation => (t.ink_3, FontWeight::NORMAL, false),
        Token::Comment => (t.ink_3, FontWeight::NORMAL, true),
        Token::Prompt => (t.ink_4, FontWeight::NORMAL, false),
        Token::Inserted => (t.ok, FontWeight::NORMAL, false),
        Token::Deleted => (t.danger, FontWeight::NORMAL, false),
    }
}

pub fn runs(rich: &Rich, base: &Base, t: &Tokens, hovered_link: Option<u16>) -> Vec<TextRun> {
    let mut runs = Vec::with_capacity(rich.marks.len() + 1);
    let font = |family: &SharedString, weight: FontWeight, italic: bool| Font {
        family: family.clone(),
        weight,
        style: if italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        },
        ..gpui::font(family.clone())
    };
    let plain = |len: usize| TextRun {
        len,
        font: font(&base.family, base.weight, base.italic),
        color: base.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut at = 0;
    for (range, marks) in &rich.marks {
        if range.start > at {
            runs.push(plain(range.start - at));
        }
        runs.push(styled(
            range.len(),
            *marks,
            base,
            t,
            hovered_link,
            &font,
            rich,
        ));
        at = range.end;
    }
    if at < rich.text.len() || runs.is_empty() {
        runs.push(plain(rich.text.len() - at));
    }
    runs
}

fn styled(
    len: usize,
    marks: Marks,
    base: &Base,
    t: &Tokens,
    hovered_link: Option<u16>,
    font: &dyn Fn(&SharedString, FontWeight, bool) -> Font,
    rich: &Rich,
) -> TextRun {
    let mut color = base.color;
    let mut weight = base.weight;
    let mut italic = base.italic || marks.emphasis;
    let mut family = &base.family;
    let mut background = None;
    let mut underline = None;
    if marks.strong {
        weight = FontWeight::SEMIBOLD;
        color = t.ink;
    }
    if marks.mono {
        family = &base.mono;
    }
    if marks.code {
        family = &base.mono;
    }
    if let Some(token) = marks.token {
        let (c, w, i) = token_style(token, t, base.tinted);
        color = c;
        weight = w;
        italic = i;
    }
    if marks.tag {
        family = &base.mono;
        color = t.ink_2;
        background = Some(t.card_2);
    }
    if marks.muted {
        color = t.ink_3;
    }
    if marks.highlight {
        background = Some(t.ink_2);
        color = t.btn_ink;
    }
    match marks.decor {
        Some(crate::Decor::Resolved) => {
            background = Some(t.ink_2);
            color = t.btn_ink;
        }
        Some(crate::Decor::Unresolved) => {}
        None => {}
    }
    if let Some(index) = marks.link {
        let goal = rich.links.get(index as usize).map(|l| &l.goal);
        let hovered = hovered_link == Some(index);
        match goal {
            Some(LinkGoal::Unresolved(_)) => color = t.ink_3,
            Some(LinkGoal::Footnote(_)) => color = gpui::transparent_black(),
            _ => {
                color = t.ink;
                underline = Some(UnderlineStyle {
                    thickness: px(1.),
                    color: Some(if hovered { t.ink } else { t.line_2 }),
                    wavy: false,
                });
            }
        }
    }
    TextRun {
        len,
        font: font(family, weight, italic),
        color,
        background_color: background,
        underline,
        strikethrough: marks.strike.then_some(StrikethroughStyle {
            thickness: px(1.),
            color: Some(color),
        }),
    }
}

pub struct RichText {
    key: TextKey,
    rich: Arc<Rich>,
    styled: StyledText,
    selection: Option<Range<usize>>,
    selection_color: Hsla,
    registry: Registry,
    map: Option<Arc<SourceMap>>,
    caret: Option<(usize, Hsla)>,
    marked: Option<Range<usize>>,
    found: Vec<(Range<usize>, Hsla)>,
    paints: Paints,
}

#[derive(Clone, Default)]
pub struct Paints {
    size: Pixels,
    text: Option<Font>,
    boxes: Vec<(Range<usize>, Font, Hsla)>,
    dotted: Vec<(Range<usize>, Hsla)>,
    icons: Vec<(usize, Hsla)>,
    raised: Vec<(Range<usize>, Font, Hsla)>,
    outline: Option<(Range<usize>, Hsla)>,
}

impl Paints {
    pub fn outline(mut self, range: Option<Range<usize>>, color: Hsla) -> Self {
        self.outline = range.map(|r| (r, color));
        self
    }
}

pub fn paints(rich: &Rich, base: &Base, t: &Tokens, hovered_link: Option<u16>) -> Paints {
    let font = |family: &SharedString, weight: FontWeight| Font {
        family: family.clone(),
        weight,
        ..gpui::font(family.clone())
    };
    let mut out = Paints {
        size: base.size,
        text: Some(font(&base.family, base.weight)),
        ..Paints::default()
    };
    for (range, marks) in &rich.marks {
        if marks.code && marks.token.is_none() && base.family != base.mono {
            out.boxes
                .push((range.clone(), font(&base.mono, base.weight), t.card_2));
        }
        if marks.decor == Some(crate::Decor::Unresolved) {
            out.dotted.push((range.clone(), t.warn));
        }
        let Some(index) = marks.link else { continue };
        let hovered = hovered_link == Some(index);
        match rich.links.get(index as usize).map(|l| &l.goal) {
            Some(LinkGoal::Unresolved(_)) => out
                .dotted
                .push((range.clone(), if hovered { t.ink } else { t.ink_4 })),
            Some(LinkGoal::Footnote(_)) => out.raised.push((
                range.clone(),
                font(&base.family, base.weight),
                if hovered { t.ink } else { t.ink_2 },
            )),
            _ => {}
        }
    }
    for (index, link) in rich.links.iter().enumerate() {
        if matches!(link.goal, LinkGoal::External(_))
            && rich
                .text
                .get(link.range.end..)
                .is_some_and(|rest| rest.starts_with(crate::model::EXTERNAL_MARK))
        {
            let hovered = hovered_link == Some(index as u16);
            out.icons
                .push((link.range.end, if hovered { t.ink } else { t.ink_3 }));
        }
    }
    out
}

impl RichText {
    pub fn new(
        key: TextKey,
        rich: Arc<Rich>,
        runs: Vec<TextRun>,
        selection: Option<Range<usize>>,
        selection_color: Hsla,
        registry: Registry,
    ) -> Self {
        let styled = StyledText::new(rich.text.clone()).with_runs(runs);
        RichText {
            key,
            rich,
            styled,
            selection,
            selection_color,
            registry,
            map: None,
            caret: None,
            marked: None,
            found: Vec::new(),
            paints: Paints::default(),
        }
    }

    pub fn paints(mut self, paints: Paints) -> Self {
        self.paints = paints;
        self
    }

    pub fn editing(
        mut self,
        map: Option<Arc<SourceMap>>,
        caret: Option<(usize, Hsla)>,
        marked: Option<Range<usize>>,
        found: Vec<(Range<usize>, Hsla)>,
    ) -> Self {
        self.map = map;
        self.caret = caret;
        self.marked = marked;
        self.found = found;
        self
    }
}

const CARET_WIDTH: f32 = 1.;

impl Element for RichText {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        self.styled.request_layout(id, inspector, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.styled
            .prepaint(id, inspector, bounds, state, window, cx);
        self.registry.0.borrow_mut().push(Hit {
            key: self.key,
            bounds,
            layout: self.styled.layout().clone(),
            rich: self.rich.clone(),
            map: self.map.clone(),
            caret: self.caret.map(|(at, _)| at),
            selection: self.selection.clone(),
        });
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut (),
        prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let ranges = self
            .found
            .iter()
            .cloned()
            .chain(self.selection.clone().map(|r| (r, self.selection_color)));
        for (range, color) in ranges.filter(|(r, _)| r.start < r.end) {
            let layout = self.styled.layout();
            let line = layout.line_height();
            let start = layout
                .position_for_index(range.start)
                .unwrap_or(bounds.origin);
            let end = layout
                .position_for_index(range.end)
                .unwrap_or_else(|| point(bounds.right(), bounds.bottom() - line));
            let quad = |from: Point<Pixels>, to_x: Pixels| {
                gpui::fill(
                    Bounds::from_corners(
                        point(from.x.min(to_x), from.y),
                        point(from.x.max(to_x), from.y + line),
                    ),
                    color,
                )
            };
            if (end.y - start.y).abs() < px(1.) {
                window.paint_quad(quad(start, end.x));
            } else {
                window.paint_quad(quad(start, bounds.right()));
                let mut y = start.y + line;
                while y + px(0.5) < end.y {
                    window.paint_quad(quad(point(bounds.left(), y), bounds.right()));
                    y += line;
                }
                window.paint_quad(quad(point(bounds.left(), end.y), end.x));
            }
        }
        self.paint_boxes(bounds, window);
        self.styled
            .paint(id, inspector, bounds, state, prepaint, window, cx);
        self.paint_marks(bounds, window, cx);
        let layout = self.styled.layout();
        let line = layout.line_height();
        if let Some(range) = self.marked.clone().filter(|r| r.start < r.end)
            && let (Some(start), Some(end)) = (
                layout.position_for_index(range.start),
                layout.position_for_index(range.end),
            )
        {
            let y = start.y + line - px(2.);
            window.paint_quad(gpui::fill(
                Bounds::from_corners(
                    point(start.x.min(end.x), y),
                    point(start.x.max(end.x), y + px(1.)),
                ),
                self.caret.map_or(self.selection_color, |(_, color)| color),
            ));
        }
        if let Some((offset, color)) = self.caret
            && let Some(at) = layout.position_for_index(offset.min(self.rich.text.len()))
        {
            let scale = window.scale_factor();
            let x = (f32::from(at.x) * scale).round() / scale;
            let width = (CARET_WIDTH * scale).round().max(1.) / scale;
            window.paint_quad(gpui::fill(
                Bounds::new(point(px(x), at.y), gpui::size(px(width), line)),
                color,
            ));
        }
    }
}

impl RichText {
    fn line_spans(
        &self,
        range: &Range<usize>,
        bounds: Bounds<Pixels>,
    ) -> Vec<(Point<Pixels>, Pixels)> {
        let layout = self.styled.layout();
        let line = layout.line_height();
        let (Some(start), Some(end)) = (
            layout.position_for_index(range.start),
            layout.position_for_index(range.end),
        ) else {
            return Vec::new();
        };
        if (end.y - start.y).abs() < px(1.) {
            return vec![(point(start.x.min(end.x), start.y), start.x.max(end.x))];
        }
        let mut spans = vec![(start, bounds.right())];
        let mut y = start.y + line;
        while y + px(0.5) < end.y {
            spans.push((point(bounds.left(), y), bounds.right()));
            y += line;
        }
        spans.push((point(bounds.left(), end.y), end.x));
        spans
    }

    fn glyph_band(&self, font: &Font, window: &Window) -> (Pixels, Pixels) {
        let system = window.text_system();
        let id = system.resolve_font(font);
        let size = self.paints.size;
        let (ascent, descent) = (system.ascent(id, size), system.descent(id, size).abs());
        let line = self.styled.layout().line_height();
        ((line - ascent - descent) / 2., ascent + descent)
    }

    fn paint_boxes(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        for (range, font, color) in &self.paints.boxes {
            let (top, height) = self.glyph_band(font, window);
            for (from, to) in self.line_spans(range, bounds) {
                window.paint_quad(
                    gpui::fill(
                        Bounds::from_corners(
                            point(from.x - px(4.), from.y + top - px(1.)),
                            point(to + px(4.), from.y + top + height + px(1.)),
                        ),
                        *color,
                    )
                    .corner_radii(px(2.)),
                );
            }
        }
    }

    fn paint_marks(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let layout = self.styled.layout();
        let line = layout.line_height();
        let baseline = self.paints.text.as_ref().map(|font| {
            let (top, _) = self.glyph_band(font, window);
            let system = window.text_system();
            top + system.ascent(system.resolve_font(font), self.paints.size)
        });
        if let Some(baseline) = baseline {
            for (range, color) in &self.paints.dotted {
                for (from, to) in self.line_spans(range, bounds) {
                    let y = from.y + baseline + px(3.);
                    let mut x = from.x;
                    while x < to {
                        window.paint_quad(gpui::fill(
                            Bounds::new(point(x, y), gpui::size(px(1.), px(1.))),
                            *color,
                        ));
                        x += px(2.);
                    }
                }
            }
        }
        if let Some((range, color)) = &self.paints.outline {
            for (from, to) in self.line_spans(range, bounds) {
                window.paint_quad(gpui::outline(
                    Bounds::from_corners(
                        point(from.x - px(2.), from.y),
                        point(to + px(2.), from.y + line),
                    ),
                    *color,
                    gpui::BorderStyle::Solid,
                ));
            }
        }
        let icon_size = px(12.);
        for (at, color) in &self.paints.icons {
            let Some(origin) = layout.position_for_index(*at) else {
                continue;
            };
            let glyph = window.text_system().shape_line(
                focus_kit::Icon::EXTERNAL.0.into(),
                icon_size,
                &[TextRun {
                    len: focus_kit::Icon::EXTERNAL.0.len(),
                    font: gpui::font(focus_kit::icons::FAMILY),
                    color: *color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            );
            let room = self.paints.size - icon_size;
            let _ = glyph.paint(
                point(origin.x + room / 2., origin.y + (line - icon_size) / 2.),
                icon_size,
                window,
                cx,
            );
        }
        for (range, font, color) in &self.paints.raised {
            let Some(origin) = layout.position_for_index(range.start) else {
                continue;
            };
            let Some(text) = self.rich.text.get(range.clone()) else {
                continue;
            };
            let size = self.paints.size * 0.75;
            let shaped = window.text_system().shape_line(
                SharedString::from(text.to_string()),
                size,
                &[TextRun {
                    len: text.len(),
                    font: font.clone(),
                    color: *color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            );
            let _ = shaped.paint(
                point(origin.x, origin.y - self.paints.size * 0.3),
                line,
                window,
                cx,
            );
        }
    }
}

impl IntoElement for RichText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(test)]
mod caret_tests {
    use gpui::{CaretGlyph, CaretLine, TextAlign, px};

    fn line(glyphs: &[(usize, f32)], end: f32, bytes: std::ops::Range<usize>) -> CaretLine {
        CaretLine {
            glyphs: glyphs
                .iter()
                .map(|&(index, x)| CaretGlyph { index, x: px(x) })
                .collect(),
            start_x: px(0.),
            end_x: px(end),
            bytes,
        }
    }

    #[test]
    fn left_to_right_carets_sit_on_the_left_edge() {
        let l = line(&[(0, 0.), (1, 10.), (2, 20.)], 30., 0..3);
        assert_eq!(l.caret_x(0), px(0.));
        assert_eq!(l.caret_x(1), px(10.));
        assert_eq!(l.caret_x(3), px(30.));
        assert_eq!(l.caret_index(px(14.)), 1);
        assert_eq!(l.caret_index(px(16.)), 2);
        assert_eq!(l.caret_index(px(40.)), 3);
    }

    #[test]
    fn right_to_left_carets_sit_on_the_right_edge() {
        let l = line(&[(2, 0.), (1, 10.), (0, 20.)], 30., 0..3);
        assert_eq!(
            l.caret_x(0),
            px(30.),
            "before the first character, on its right"
        );
        assert_eq!(l.caret_x(1), px(20.));
        assert_eq!(l.caret_x(2), px(10.));
        assert_eq!(l.caret_x(3), px(0.), "the end of the text, on the left");
        assert_eq!(l.caret_index(px(29.)), 0);
        assert_eq!(l.caret_index(px(21.)), 1);
        assert_eq!(l.caret_index(px(1.)), 3);
    }

    #[test]
    fn a_latin_word_in_an_arabic_line_reads_left_to_right() {
        let l = line(
            &[
                (6, 0.),
                (5, 10.),
                (3, 20.),
                (4, 30.),
                (2, 40.),
                (1, 50.),
                (0, 60.),
            ],
            70.,
            0..7,
        );
        assert_eq!(l.caret_x(0), px(70.));
        assert_eq!(l.caret_x(1), px(60.));
        assert_eq!(l.caret_x(3), px(20.), "before x, on its left");
        assert_eq!(l.caret_x(4), px(30.));
        assert_eq!(l.caret_x(7), px(0.));
        assert_eq!(l.caret_index(px(24.)), 3);
        assert_eq!(l.caret_index(px(36.)), 5, "after y");
    }

    #[test]
    fn aligned_lines_start_where_paint_puts_them() {
        let l = line(&[(0, 0.), (1, 10.), (2, 20.)], 30., 0..3);
        assert_eq!(l.aligned_left(TextAlign::Left, px(100.)), px(0.));
        assert_eq!(l.aligned_left(TextAlign::Right, px(100.)), px(70.));
        assert_eq!(l.aligned_left(TextAlign::Center, px(100.)), px(35.));
    }
}
