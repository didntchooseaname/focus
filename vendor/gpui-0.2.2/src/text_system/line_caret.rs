//! Focus: caret positions and hit testing that hold for right-to-left text and for aligned
//! lines (see FOCUS-PATCHES.md).
//!
//! The glyphs of a shaped line come in visual order. In a right-to-left run their byte
//! indices decrease from left to right, so the caret before a character sits on its
//! right edge, and the glyph of a line's first character is not its leftmost one. A
//! wrapped right-to-left line is laid out line after line, end to end
//! (`LineLayoutCache::wrap_logically`): each visual line holds a range of bytes in logical
//! order, which its glyph indices give.

use std::ops::Range;

use super::{LineLayout, WrappedLineLayout};
use crate::{Pixels, Point, TextAlign, point, px};

/// A glyph as the caret sees it: the byte index of its character and its left edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretGlyph {
    /// Byte index of the glyph's character.
    pub index: usize,
    /// Left edge of the glyph.
    pub x: Pixels,
}

/// One visual line: its glyphs in visual order, its left and right edges in the unwrapped
/// layout, and its bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct CaretLine {
    /// The glyphs, in visual order.
    pub glyphs: Vec<CaretGlyph>,
    /// Left edge of the line.
    pub start_x: Pixels,
    /// Right edge of the line.
    pub end_x: Pixels,
    /// Bytes of the line, in logical order.
    pub bytes: Range<usize>,
}

impl CaretLine {
    /// Width of the line.
    pub fn width(&self) -> Pixels {
        self.end_x - self.start_x
    }

    /// Byte indices of the line's characters, sorted, each once.
    fn indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = self.glyphs.iter().map(|g| g.index).collect();
        indices.sort_unstable();
        indices.dedup();
        indices
    }

    /// The glyphs of the character at visual position `p`: from its first to its last
    /// glyph in visual order (a base and its marks share an index).
    fn cluster(&self, p: usize) -> Range<usize> {
        let index = self.glyphs[p].index;
        let mut start = p;
        while start > 0 && self.glyphs[start - 1].index == index {
            start -= 1;
        }
        let mut end = p + 1;
        while end < self.glyphs.len() && self.glyphs[end].index == index {
            end += 1;
        }
        start..end
    }

    fn left(&self, cluster: &Range<usize>) -> Pixels {
        self.glyphs[cluster.start].x
    }

    fn right(&self, cluster: &Range<usize>) -> Pixels {
        self.glyphs.get(cluster.end).map_or(self.end_x, |g| g.x)
    }

    /// The character drawn at `cluster` reads right to left: a visual neighbour is its
    /// logical neighbour on the other side. A character alone between two of the other
    /// direction cannot tell, and counts as left to right.
    fn rtl(&self, cluster: &Range<usize>, indices: &[usize]) -> bool {
        let index = self.glyphs[cluster.start].index;
        let at = indices.partition_point(|&i| i < index);
        let previous = at.checked_sub(1).map(|i| indices[i]);
        let next = indices.get(at + 1).copied();
        let right = self.glyphs.get(cluster.end).map(|g| g.index);
        let left = cluster.start.checked_sub(1).map(|p| self.glyphs[p].index);
        if right.is_some() && right == next {
            return false;
        }
        if right.is_some() && right == previous {
            return true;
        }
        if left.is_some() && left == previous {
            return false;
        }
        left.is_some() && left == next
    }

    /// x of the caret before byte `index`, in the unwrapped layout.
    pub fn caret_x(&self, index: usize) -> Pixels {
        if self.glyphs.is_empty() {
            return self.start_x;
        }
        let indices = self.indices();
        if index < self.bytes.end
            && let Some(p) = self.glyphs.iter().position(|g| g.index == index)
        {
            let cluster = self.cluster(p);
            return if self.rtl(&cluster, &indices) {
                self.right(&cluster)
            } else {
                self.left(&cluster)
            };
        }
        // After the character before `index`: the end of the line, or inside a cluster.
        let before = self
            .glyphs
            .iter()
            .enumerate()
            .filter(|(_, g)| g.index < index)
            .max_by_key(|(p, g)| (g.index, usize::MAX - p));
        match before {
            Some((p, _)) => {
                let cluster = self.cluster(p);
                if self.rtl(&cluster, &indices) {
                    self.left(&cluster)
                } else {
                    self.right(&cluster)
                }
            }
            None => self.start_x,
        }
    }

    /// The character boundary closest to `x`, in the unwrapped layout.
    pub fn caret_index(&self, x: Pixels) -> usize {
        if self.glyphs.is_empty() {
            return self.bytes.start;
        }
        let indices = self.indices();
        let p = self.glyphs.iter().rposition(|g| g.x <= x).unwrap_or(0);
        let cluster = self.cluster(p);
        let index = self.glyphs[p].index;
        let after = indices
            .get(indices.partition_point(|&i| i <= index))
            .copied()
            .unwrap_or(self.bytes.end);
        let middle = (self.left(&cluster) + self.right(&cluster)) / 2.;
        let left_half = x < middle;
        if self.rtl(&cluster, &indices) == left_half {
            after
        } else {
            index
        }
    }

    /// Left edge of the line on screen, from the origin of the text, as `paint_line`
    /// aligns it in `align_width`.
    pub fn aligned_left(&self, align: TextAlign, align_width: Pixels) -> Pixels {
        match align {
            TextAlign::Left => px(0.),
            TextAlign::Center => (align_width - self.width()) / 2.,
            TextAlign::Right => align_width - self.width(),
        }
    }
}

impl LineLayout {
    /// The whole line as one visual line.
    pub fn caret_line(&self) -> CaretLine {
        CaretLine {
            glyphs: self
                .runs
                .iter()
                .flat_map(|run| run.glyphs.iter())
                .map(|g| CaretGlyph {
                    index: g.index,
                    x: g.position.x,
                })
                .collect(),
            start_x: px(0.),
            end_x: self.width,
            bytes: 0..self.len,
        }
    }
}

impl WrappedLineLayout {
    /// The visual lines, top to bottom. Their bytes are consecutive: each starts at the
    /// smallest index of its glyphs.
    pub fn caret_lines(&self) -> Vec<CaretLine> {
        let layout = &self.unwrapped_layout;
        let mut lines: Vec<CaretLine> = Vec::with_capacity(self.wrap_boundaries.len() + 1);
        let mut current = Vec::new();
        let mut boundaries = self.wrap_boundaries.iter().peekable();
        for (run_ix, run) in layout.runs.iter().enumerate() {
            for (glyph_ix, glyph) in run.glyphs.iter().enumerate() {
                if boundaries
                    .peek()
                    .is_some_and(|b| b.run_ix == run_ix && b.glyph_ix == glyph_ix)
                {
                    boundaries.next();
                    lines.push(CaretLine {
                        glyphs: std::mem::take(&mut current),
                        start_x: px(0.),
                        end_x: glyph.position.x,
                        bytes: 0..0,
                    });
                }
                current.push(CaretGlyph {
                    index: glyph.index,
                    x: glyph.position.x,
                });
            }
        }
        lines.push(CaretLine {
            glyphs: current,
            start_x: px(0.),
            end_x: layout.width,
            bytes: 0..0,
        });
        let mut start_x = px(0.);
        for line in &mut lines {
            line.start_x = line.glyphs.first().map_or(start_x, |g| g.x).min(line.end_x);
            start_x = line.end_x;
        }
        let starts: Vec<Option<usize>> = lines
            .iter()
            .map(|line| line.glyphs.iter().map(|g| g.index).min())
            .collect();
        let mut end = layout.len;
        for k in (0..lines.len()).rev() {
            let start = if k == 0 { 0 } else { starts[k].unwrap_or(end) };
            lines[k].bytes = start.min(end)..end;
            end = start.min(end);
        }
        lines
    }

    /// The visual line that draws byte `index`; at the end of a line, that line rather than
    /// the start of the next.
    fn caret_line_of(lines: &[CaretLine], index: usize) -> usize {
        lines
            .iter()
            .position(|line| {
                index < line.bytes.end || (index == line.bytes.end && index > line.bytes.start)
            })
            .unwrap_or(lines.len().saturating_sub(1))
    }

    /// The caret before byte `index`: x from the left edge of its visual line, y the top
    /// of that line.
    pub fn caret_position(&self, index: usize, line_height: Pixels) -> Point<Pixels> {
        self.caret_position_aligned(index, line_height, TextAlign::Left, px(0.))
    }

    /// [`Self::caret_position`] with each visual line aligned in `align_width` as
    /// `paint_line` aligns it: x from the origin of the text.
    pub fn caret_position_aligned(
        &self,
        index: usize,
        line_height: Pixels,
        align: TextAlign,
        align_width: Pixels,
    ) -> Point<Pixels> {
        let lines = self.caret_lines();
        let k = Self::caret_line_of(&lines, index);
        let line = &lines[k];
        point(
            line.aligned_left(align, align_width) + line.caret_x(index) - line.start_x,
            line_height * k as f32,
        )
    }

    /// The character boundary closest to `position` (from the origin of the text), each
    /// visual line aligned in `align_width` as `paint_line` aligns it.
    pub fn caret_index_aligned(
        &self,
        position: Point<Pixels>,
        line_height: Pixels,
        align: TextAlign,
        align_width: Pixels,
    ) -> usize {
        let lines = self.caret_lines();
        let k = ((position.y / line_height).floor().max(0.) as usize).min(lines.len() - 1);
        let line = &lines[k];
        line.caret_index(position.x - line.aligned_left(align, align_width) + line.start_x)
    }
}

impl WrappedLineLayout {
    /// [`Self::index_for_position`] with each visual line aligned in `align_width` as
    /// `paint_line` aligns it.
    pub fn index_for_position_aligned(
        &self,
        position: Point<Pixels>,
        line_height: Pixels,
        align: TextAlign,
        align_width: Pixels,
    ) -> Result<usize, usize> {
        let lines = self.caret_lines();
        let k = ((position.y / line_height).floor().max(0.) as usize).min(lines.len() - 1);
        let shift = lines[k].aligned_left(align, align_width);
        self.index_for_position(point(position.x - shift, position.y), line_height)
    }
}
