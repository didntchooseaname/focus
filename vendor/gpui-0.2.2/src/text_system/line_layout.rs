use crate::{FontId, GlyphId, Pixels, PlatformTextSystem, Point, SharedString, Size, point, px};
use collections::FxHashMap;
use parking_lot::{Mutex, RwLock, RwLockUpgradableReadGuard};
use smallvec::SmallVec;
use std::{
    borrow::Borrow,
    hash::{Hash, Hasher},
    ops::Range,
    sync::Arc,
};

use super::LineWrapper;

/// A laid out and styled line of text
#[derive(Default, Debug)]
pub struct LineLayout {
    /// The font size for this line
    pub font_size: Pixels,
    /// The width of the line
    pub width: Pixels,
    /// The ascent of the line
    pub ascent: Pixels,
    /// The descent of the line
    pub descent: Pixels,
    /// The shaped runs that make up this line
    pub runs: Vec<ShapedRun>,
    /// The length of the line in utf-8 bytes
    pub len: usize,
}

/// A run of text that has been shaped .
#[derive(Debug, Clone)]
pub struct ShapedRun {
    /// The font id for this run
    pub font_id: FontId,
    /// The glyphs that make up this run
    pub glyphs: Vec<ShapedGlyph>,
}

/// A single glyph, ready to paint.
#[derive(Clone, Debug)]
pub struct ShapedGlyph {
    /// The ID for this glyph, as determined by the text system.
    pub id: GlyphId,

    /// The position of this glyph in its containing line.
    pub position: Point<Pixels>,

    /// The index of this glyph in the original text.
    pub index: usize,

    /// Whether this glyph is an emoji
    pub is_emoji: bool,
}

impl LineLayout {
    /// The index for the character at the given x coordinate
    pub fn index_for_x(&self, x: Pixels) -> Option<usize> {
        if x >= self.width {
            None
        } else {
            for run in self.runs.iter().rev() {
                for glyph in run.glyphs.iter().rev() {
                    if glyph.position.x <= x {
                        return Some(glyph.index);
                    }
                }
            }
            Some(0)
        }
    }

    /// closest_index_for_x returns the character boundary closest to the given x coordinate
    /// (e.g. to handle aligning up/down arrow keys)
    pub fn closest_index_for_x(&self, x: Pixels) -> usize {
        // Focus: right-to-left runs (line_caret.rs).
        self.caret_line().caret_index(x)
    }

    /// The x position of the character at the given index
    pub fn x_for_index(&self, index: usize) -> Pixels {
        // Focus: right-to-left runs (line_caret.rs).
        self.caret_line().caret_x(index)
    }

    /// The corresponding Font at the given index
    pub fn font_id_for_index(&self, index: usize) -> Option<FontId> {
        for run in &self.runs {
            for glyph in &run.glyphs {
                if glyph.index >= index {
                    return Some(run.font_id);
                }
            }
        }
        None
    }

    fn compute_wrap_boundaries(
        &self,
        text: &str,
        wrap_width: Pixels,
        max_lines: Option<usize>,
    ) -> SmallVec<[WrapBoundary; 1]> {
        let mut boundaries = SmallVec::new();
        let mut first_non_whitespace_ix = None;
        let mut last_candidate_ix = None;
        let mut last_candidate_x = px(0.);
        let mut last_boundary = WrapBoundary {
            run_ix: 0,
            glyph_ix: 0,
        };
        let mut last_boundary_x = px(0.);
        let mut prev_ch = '\0';
        let mut glyphs = self
            .runs
            .iter()
            .enumerate()
            .flat_map(move |(run_ix, run)| {
                run.glyphs.iter().enumerate().map(move |(glyph_ix, glyph)| {
                    let character = text[glyph.index..].chars().next().unwrap();
                    (
                        WrapBoundary { run_ix, glyph_ix },
                        character,
                        glyph.position.x,
                    )
                })
            })
            .peekable();

        while let Some((boundary, ch, x)) = glyphs.next() {
            if ch == '\n' {
                continue;
            }

            // Here is very similar to `LineWrapper::wrap_line` to determine text wrapping,
            // but there are some differences, so we have to duplicate the code here.
            if LineWrapper::is_word_char(ch) {
                if prev_ch == ' ' && ch != ' ' && first_non_whitespace_ix.is_some() {
                    last_candidate_ix = Some(boundary);
                    last_candidate_x = x;
                }
            } else {
                if ch != ' ' && first_non_whitespace_ix.is_some() {
                    last_candidate_ix = Some(boundary);
                    last_candidate_x = x;
                }
            }

            if ch != ' ' && first_non_whitespace_ix.is_none() {
                first_non_whitespace_ix = Some(boundary);
            }

            let next_x = glyphs.peek().map_or(self.width, |(_, _, x)| *x);
            let width = next_x - last_boundary_x;

            if width > wrap_width && boundary > last_boundary {
                // When used line_clamp, we should limit the number of lines.
                if let Some(max_lines) = max_lines
                    && boundaries.len() >= max_lines - 1
                {
                    break;
                }

                if let Some(last_candidate_ix) = last_candidate_ix.take() {
                    last_boundary = last_candidate_ix;
                    last_boundary_x = last_candidate_x;
                } else {
                    last_boundary = boundary;
                    last_boundary_x = x;
                }
                boundaries.push(last_boundary);
            }
            prev_ch = ch;
        }

        boundaries
    }
}

/// A line of text that has been wrapped to fit a given width
#[derive(Default, Debug)]
pub struct WrappedLineLayout {
    /// The line layout, pre-wrapping.
    pub unwrapped_layout: Arc<LineLayout>,

    /// The boundaries at which the line was wrapped
    pub wrap_boundaries: SmallVec<[WrapBoundary; 1]>,

    /// The width of the line, if it was wrapped
    pub wrap_width: Option<Pixels>,
}

/// A boundary at which a line was wrapped
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WrapBoundary {
    /// The index in the run just before the line was wrapped
    pub run_ix: usize,
    /// The index of the glyph just before the line was wrapped
    pub glyph_ix: usize,
}

impl WrappedLineLayout {
    /// The length of the underlying text, in utf8 bytes.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.unwrapped_layout.len
    }

    /// The width of this line, in pixels, whether or not it was wrapped.
    pub fn width(&self) -> Pixels {
        self.wrap_width
            .unwrap_or(Pixels::MAX)
            .min(self.unwrapped_layout.width)
    }

    /// The size of the whole wrapped text, for the given line_height.
    /// can span multiple lines if there are multiple wrap boundaries.
    pub fn size(&self, line_height: Pixels) -> Size<Pixels> {
        Size {
            width: self.width(),
            height: line_height * (self.wrap_boundaries.len() + 1),
        }
    }

    /// The ascent of a line in this layout
    pub fn ascent(&self) -> Pixels {
        self.unwrapped_layout.ascent
    }

    /// The descent of a line in this layout
    pub fn descent(&self) -> Pixels {
        self.unwrapped_layout.descent
    }

    /// The wrap boundaries in this layout
    pub fn wrap_boundaries(&self) -> &[WrapBoundary] {
        &self.wrap_boundaries
    }

    /// The font size of this layout
    pub fn font_size(&self) -> Pixels {
        self.unwrapped_layout.font_size
    }

    /// The runs in this layout, sans wrapping
    pub fn runs(&self) -> &[ShapedRun] {
        &self.unwrapped_layout.runs
    }

    /// The index corresponding to a given position in this layout for the given line height.
    ///
    /// See also [`Self::closest_index_for_position`].
    pub fn index_for_position(
        &self,
        position: Point<Pixels>,
        line_height: Pixels,
    ) -> Result<usize, usize> {
        self._index_for_position(position, line_height, false)
    }

    /// The closest index to a given position in this layout for the given line height.
    ///
    /// Closest means the character boundary closest to the given position.
    ///
    /// See also [`LineLayout::closest_index_for_x`].
    pub fn closest_index_for_position(
        &self,
        position: Point<Pixels>,
        line_height: Pixels,
    ) -> Result<usize, usize> {
        self._index_for_position(position, line_height, true)
    }

    fn _index_for_position(
        &self,
        position: Point<Pixels>,
        line_height: Pixels,
        closest: bool,
    ) -> Result<usize, usize> {
        // Focus: visual lines by their glyphs, right-to-left ones included (line_caret.rs).
        let lines = self.caret_lines();
        let Some(line) = lines.get((position.y / line_height).max(0.) as usize) else {
            return Err(0);
        };
        let x = position.x + line.start_x;
        let inside = x >= line.start_x && x < line.end_x;
        let index = match self.unwrapped_layout.index_for_x(x) {
            Some(index) if inside && !closest => index,
            _ => line.caret_index(x),
        };
        if inside { Ok(index) } else { Err(index) }
    }

    /// Returns the pixel position for the given byte index.
    pub fn position_for_index(&self, index: usize, line_height: Pixels) -> Option<Point<Pixels>> {
        // Focus: right-to-left runs and lines (line_caret.rs).
        (index <= self.len()).then(|| self.caret_position(index, line_height))
    }
}

pub(crate) struct LineLayoutCache {
    previous_frame: Mutex<FrameCache>,
    current_frame: RwLock<FrameCache>,
    platform_text_system: Arc<dyn PlatformTextSystem>,
}

#[derive(Default)]
struct FrameCache {
    lines: FxHashMap<Arc<CacheKey>, Arc<LineLayout>>,
    wrapped_lines: FxHashMap<Arc<CacheKey>, Arc<WrappedLineLayout>>,
    used_lines: Vec<Arc<CacheKey>>,
    used_wrapped_lines: Vec<Arc<CacheKey>>,
}

#[derive(Clone, Default)]
pub(crate) struct LineLayoutIndex {
    lines_index: usize,
    wrapped_lines_index: usize,
}

impl LineLayoutCache {
    pub fn new(platform_text_system: Arc<dyn PlatformTextSystem>) -> Self {
        Self {
            previous_frame: Mutex::default(),
            current_frame: RwLock::default(),
            platform_text_system,
        }
    }

    pub fn layout_index(&self) -> LineLayoutIndex {
        let frame = self.current_frame.read();
        LineLayoutIndex {
            lines_index: frame.used_lines.len(),
            wrapped_lines_index: frame.used_wrapped_lines.len(),
        }
    }

    pub fn reuse_layouts(&self, range: Range<LineLayoutIndex>) {
        let mut previous_frame = &mut *self.previous_frame.lock();
        let mut current_frame = &mut *self.current_frame.write();

        for key in &previous_frame.used_lines[range.start.lines_index..range.end.lines_index] {
            if let Some((key, line)) = previous_frame.lines.remove_entry(key) {
                current_frame.lines.insert(key, line);
            }
            current_frame.used_lines.push(key.clone());
        }

        for key in &previous_frame.used_wrapped_lines
            [range.start.wrapped_lines_index..range.end.wrapped_lines_index]
        {
            if let Some((key, line)) = previous_frame.wrapped_lines.remove_entry(key) {
                current_frame.wrapped_lines.insert(key, line);
            }
            current_frame.used_wrapped_lines.push(key.clone());
        }
    }

    pub fn truncate_layouts(&self, index: LineLayoutIndex) {
        let mut current_frame = &mut *self.current_frame.write();
        current_frame.used_lines.truncate(index.lines_index);
        current_frame
            .used_wrapped_lines
            .truncate(index.wrapped_lines_index);
    }

    pub fn finish_frame(&self) {
        let mut prev_frame = self.previous_frame.lock();
        let mut curr_frame = self.current_frame.write();
        std::mem::swap(&mut *prev_frame, &mut *curr_frame);
        curr_frame.lines.clear();
        curr_frame.wrapped_lines.clear();
        curr_frame.used_lines.clear();
        curr_frame.used_wrapped_lines.clear();
    }

    pub fn layout_wrapped_line<Text>(
        &self,
        text: Text,
        font_size: Pixels,
        runs: &[FontRun],
        wrap_width: Option<Pixels>,
        max_lines: Option<usize>,
    ) -> Arc<WrappedLineLayout>
    where
        Text: AsRef<str>,
        SharedString: From<Text>,
    {
        let key = &CacheKeyRef {
            text: text.as_ref(),
            font_size,
            runs,
            wrap_width,
            force_width: None,
        } as &dyn AsCacheKeyRef;

        let current_frame = self.current_frame.upgradable_read();
        if let Some(layout) = current_frame.wrapped_lines.get(key) {
            return layout.clone();
        }

        let previous_frame_entry = self.previous_frame.lock().wrapped_lines.remove_entry(key);
        if let Some((key, layout)) = previous_frame_entry {
            let mut current_frame = RwLockUpgradableReadGuard::upgrade(current_frame);
            current_frame
                .wrapped_lines
                .insert(key.clone(), layout.clone());
            current_frame.used_wrapped_lines.push(key);
            layout
        } else {
            drop(current_frame);
            let text = SharedString::from(text);
            let mut unwrapped_layout =
                self.layout_line::<&SharedString>(&text, font_size, runs, None);
            let wrap_boundaries = match wrap_width {
                // Focus patch: right-to-left text is cut in logical order and laid out in
                // screen order, see `wrap_logically`.
                _ if has_right_to_left(&text) => {
                    let (layout, boundaries) = self.wrap_logically(
                        &text,
                        font_size,
                        runs,
                        &unwrapped_layout,
                        wrap_width.unwrap_or(Pixels::MAX),
                        max_lines,
                    );
                    unwrapped_layout = layout;
                    boundaries
                }
                Some(wrap_width) => {
                    unwrapped_layout.compute_wrap_boundaries(text.as_ref(), wrap_width, max_lines)
                }
                None => SmallVec::new(),
            };
            let layout = Arc::new(WrappedLineLayout {
                unwrapped_layout,
                wrap_boundaries,
                wrap_width,
            });
            let key = Arc::new(CacheKey {
                text,
                font_size,
                runs: SmallVec::from(runs),
                wrap_width,
                force_width: None,
            });

            let mut current_frame = self.current_frame.write();
            current_frame
                .wrapped_lines
                .insert(key.clone(), layout.clone());
            current_frame.used_wrapped_lines.push(key);

            layout
        }
    }

    /// Focus patch. The glyphs of a right-to-left run come in text order with decreasing x.
    /// `compute_wrap_boundaries` measured them by x differences, which go negative there:
    /// Arabic paragraphs came out as one letter, then the rest past the edge, and painting
    /// drew backgrounds and underlines with negative widths. Here the text is cut in
    /// logical order (advances measured on screen, breaks as in `LineWrapper`), each line
    /// is shaped on its own, its glyphs are put in screen order, left to right, and the
    /// lines are laid end to end, so each wrap boundary is the leftmost glyph of its line.
    fn wrap_logically(
        &self,
        text: &SharedString,
        font_size: Pixels,
        runs: &[FontRun],
        shaped: &LineLayout,
        wrap_width: Pixels,
        max_lines: Option<usize>,
    ) -> (Arc<LineLayout>, SmallVec<[WrapBoundary; 1]>) {
        let text: &str = text.as_ref();
        // Glyphs of a right-to-left run come in text order with decreasing x: a glyph's
        // advance is the distance to its right neighbour on screen.
        let mut advance = vec![px(0.); text.len() + 1];
        let mut glyphs: Vec<&ShapedGlyph> = shaped.runs.iter().flat_map(|run| &run.glyphs).collect();
        glyphs.sort_by(|a, b| f32::from(a.position.x).total_cmp(&f32::from(b.position.x)));
        for (ix, glyph) in glyphs.iter().enumerate() {
            let next_x = glyphs.get(ix + 1).map_or(shaped.width, |next| next.position.x);
            if let Some(slot) = advance.get_mut(glyph.index) {
                *slot += next_x - glyph.position.x;
            }
        }

        let mut starts = vec![0];
        let mut line_start = 0;
        let mut width = px(0.);
        let mut candidate: Option<(usize, Pixels)> = None;
        let mut seen_word = false;
        let mut prev = '\0';
        for (ix, ch) in text.char_indices() {
            if ch == '\n' {
                continue;
            }
            let breakable = if LineWrapper::is_word_char(ch) { prev == ' ' && ch != ' ' } else { ch != ' ' };
            if breakable && seen_word && ix > line_start {
                candidate = Some((ix, width));
            }
            seen_word |= ch != ' ';
            width += advance[ix];
            if width > wrap_width && ix > line_start {
                if max_lines.is_some_and(|max| starts.len() >= max) {
                    break;
                }
                let (at, before) = candidate
                    .take()
                    .filter(|(at, _)| *at > line_start)
                    .unwrap_or((ix, width - advance[ix]));
                starts.push(at);
                line_start = at;
                width -= before;
            }
            prev = ch;
        }

        let mut out = LineLayout {
            font_size,
            width: px(0.),
            ascent: shaped.ascent,
            descent: shaped.descent,
            runs: Vec::new(),
            len: text.len(),
        };
        let mut boundaries = SmallVec::new();
        for (line, &start) in starts.iter().enumerate() {
            let end = starts.get(line + 1).copied().unwrap_or(text.len());
            let mut line_runs = Vec::new();
            let mut offset = 0;
            for run in runs {
                let (run_start, run_end) = (offset, offset + run.len);
                offset = run_end;
                let len = run_end.min(end).saturating_sub(run_start.max(start));
                if len > 0 {
                    line_runs.push(FontRun { len, font_id: run.font_id });
                }
            }
            let piece = self.layout_line(text[start..end].to_string(), font_size, &line_runs, None);
            // Each line in screen order, left to right, as painting and wrap boundaries
            // expect: a run per font, in the order the fonts appear from the left.
            let mut placed: Vec<(FontId, ShapedGlyph)> = piece
                .runs
                .iter()
                .flat_map(|run| run.glyphs.iter().map(move |glyph| (run.font_id, glyph)))
                .map(|(font_id, glyph)| {
                    let glyph = ShapedGlyph {
                        position: point(glyph.position.x + out.width, glyph.position.y),
                        index: glyph.index + start,
                        ..glyph.clone()
                    };
                    (font_id, glyph)
                })
                .collect();
            placed.sort_by(|a, b| f32::from(a.1.position.x).total_cmp(&f32::from(b.1.position.x)));
            let line_first_run = out.runs.len();
            if line > 0 && !placed.is_empty() {
                boundaries.push(WrapBoundary { run_ix: line_first_run, glyph_ix: 0 });
            }
            for (font_id, glyph) in placed {
                let in_line = out.runs.len() > line_first_run;
                match out.runs.last_mut() {
                    Some(run) if in_line && run.font_id == font_id => run.glyphs.push(glyph),
                    _ => out.runs.push(ShapedRun { font_id, glyphs: vec![glyph] }),
                }
            }
            out.width += piece.width;
            out.ascent = out.ascent.max(piece.ascent);
            out.descent = out.descent.max(piece.descent);
        }
        (Arc::new(out), boundaries)
    }

    pub fn layout_line<Text>(
        &self,
        text: Text,
        font_size: Pixels,
        runs: &[FontRun],
        force_width: Option<Pixels>,
    ) -> Arc<LineLayout>
    where
        Text: AsRef<str>,
        SharedString: From<Text>,
    {
        let key = &CacheKeyRef {
            text: text.as_ref(),
            font_size,
            runs,
            wrap_width: None,
            force_width,
        } as &dyn AsCacheKeyRef;

        if crate::window::ui_profile::armed() {
            crate::window::ui_profile::SHAPE_LOOKUPS
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        let current_frame = self.current_frame.upgradable_read();
        if let Some(layout) = current_frame.lines.get(key) {
            return layout.clone();
        }

        let mut current_frame = RwLockUpgradableReadGuard::upgrade(current_frame);
        if let Some((key, layout)) = self.previous_frame.lock().lines.remove_entry(key) {
            current_frame.lines.insert(key.clone(), layout.clone());
            current_frame.used_lines.push(key);
            layout
        } else {
            if crate::window::ui_profile::armed() {
                crate::window::ui_profile::SHAPE_MISSES
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            let text = SharedString::from(text);
            let mut layout =
                crate::window::ui_profile::charge(&crate::window::ui_profile::SHAPE_NANOS, || {
                    self.platform_text_system
                        .layout_line(&text, font_size, runs)
                });

            if let Some(force_width) = force_width {
                let mut glyph_pos = 0;
                for run in layout.runs.iter_mut() {
                    for glyph in run.glyphs.iter_mut() {
                        if (glyph.position.x - glyph_pos * force_width).abs() > px(1.) {
                            glyph.position.x = glyph_pos * force_width;
                        }
                        glyph_pos += 1;
                    }
                }
            }

            let key = Arc::new(CacheKey {
                text,
                font_size,
                runs: SmallVec::from(runs),
                wrap_width: None,
                force_width,
            });
            let layout = Arc::new(layout);
            current_frame.lines.insert(key.clone(), layout.clone());
            current_frame.used_lines.push(key);
            layout
        }
    }
}

/// A run of text with a single font.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct FontRun {
    pub(crate) len: usize,
    pub(crate) font_id: FontId,
}

trait AsCacheKeyRef {
    fn as_cache_key_ref(&self) -> CacheKeyRef<'_>;
}

#[derive(Clone, Debug, Eq)]
struct CacheKey {
    text: SharedString,
    font_size: Pixels,
    runs: SmallVec<[FontRun; 1]>,
    wrap_width: Option<Pixels>,
    force_width: Option<Pixels>,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
struct CacheKeyRef<'a> {
    text: &'a str,
    font_size: Pixels,
    runs: &'a [FontRun],
    wrap_width: Option<Pixels>,
    force_width: Option<Pixels>,
}

impl PartialEq for dyn AsCacheKeyRef + '_ {
    fn eq(&self, other: &dyn AsCacheKeyRef) -> bool {
        self.as_cache_key_ref() == other.as_cache_key_ref()
    }
}

impl Eq for dyn AsCacheKeyRef + '_ {}

impl Hash for dyn AsCacheKeyRef + '_ {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_cache_key_ref().hash(state)
    }
}

impl AsCacheKeyRef for CacheKey {
    fn as_cache_key_ref(&self) -> CacheKeyRef<'_> {
        CacheKeyRef {
            text: &self.text,
            font_size: self.font_size,
            runs: self.runs.as_slice(),
            wrap_width: self.wrap_width,
            force_width: self.force_width,
        }
    }
}

impl PartialEq for CacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.as_cache_key_ref().eq(&other.as_cache_key_ref())
    }
}

impl Hash for CacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_cache_key_ref().hash(state);
    }
}

impl<'a> Borrow<dyn AsCacheKeyRef + 'a> for Arc<CacheKey> {
    fn borrow(&self) -> &(dyn AsCacheKeyRef + 'a) {
        self.as_ref() as &dyn AsCacheKeyRef
    }
}

impl AsCacheKeyRef for CacheKeyRef<'_> {
    fn as_cache_key_ref(&self) -> CacheKeyRef<'_> {
        *self
    }
}

/// Focus patch: Hebrew, Arabic and the other right-to-left blocks, as in `LineWrapper`.
fn has_right_to_left(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}'))
}
