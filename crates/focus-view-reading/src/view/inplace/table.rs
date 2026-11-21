use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use focus_kit::{Icon, IconButton, MenuRow, Tokens};
use focus_md::{Align, Block, BlockKind};
use gpui::{
    AnyElement, App, Bounds, Context, Corner, ElementId, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Pixels, Point,
    RenderOnce, StatefulInteractiveElement, Styled, WeakEntity, Window, div, point,
    prelude::FluentBuilder, px,
};
use gpui_component::{Rope, RopeExt as _};

use super::super::ReadingView;
use super::{At, Edit, Key};
use crate::model::{Item, Node};

pub(crate) fn key(at: &At, key: &Key) -> Option<Edit> {
    let table = at
        .path
        .iter()
        .rev()
        .find(|node| matches!(node, Node::Table { .. }))?;
    let plain = |text: &str| !text.contains(['|', '\n', '\r']);
    match key {
        Key::ShiftEnter => return Some(Grid::only_move(at.caret, at.caret)),
        Key::Text(text) if at.selection.is_empty() && plain(text) => return None,
        Key::Backspace | Key::Delete
            if at.selection.is_empty() && !near_escaped_pipe(at.text, at.caret) =>
        {
            return None;
        }
        _ => {}
    }
    let cells = cells(at.text, at.item?, at.start, table)?;
    Grid::read(at.text, &cells)?.key(at.caret, at.selection.clone(), key)
}

fn near_escaped_pipe(text: &Rope, caret: usize) -> bool {
    let row = text.offset_to_point(caret).row;
    let line = text.slice_line(row).to_string();
    let at = caret - text.line_start_offset(row);
    [at.wrapping_sub(2), at.wrapping_sub(1), at]
        .into_iter()
        .any(|start| line.get(start..start.wrapping_add(2)) == Some("\\|"))
}

fn table_nodes<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    match node {
        Node::Table { .. } => out.push(node),
        Node::List { entries, .. } => entries
            .iter()
            .flat_map(|e| e.children.iter())
            .for_each(|child| table_nodes(child, out)),
        Node::Quote(children) | Node::Callout { children, .. } | Node::Details { children, .. } => {
            children.iter().for_each(|child| table_nodes(child, out))
        }
        Node::Tabs { tabs, .. } => tabs
            .iter()
            .flat_map(|t| t.items.iter())
            .for_each(|item| table_nodes(&item.node, out)),
        _ => {}
    }
}

fn table_cells<'a>(blocks: &'a [Block], out: &mut Vec<&'a Vec<Vec<Range<usize>>>>) {
    for block in blocks {
        match &block.kind {
            BlockKind::Table { cells, .. } => out.push(cells),
            BlockKind::List { items, .. } => items.iter().for_each(|i| table_cells(&i.blocks, out)),
            BlockKind::Quote(inner)
            | BlockKind::Callout { blocks: inner, .. }
            | BlockKind::Details { blocks: inner, .. } => table_cells(inner, out),
            BlockKind::Tabs { tabs, .. } => tabs.iter().for_each(|t| table_cells(&t.blocks, out)),
            _ => {}
        }
    }
}

fn cells(text: &Rope, item: &Item, origin: usize, table: &Node) -> Option<Vec<Vec<Range<usize>>>> {
    let mut nodes = Vec::new();
    table_nodes(&item.node, &mut nodes);
    let index = nodes.iter().position(|node| std::ptr::eq(*node, table))?;
    let last = item.lines.end.checked_sub(1)? as usize;
    let end = text.line_start_offset(last) + text.slice_line(last).len();
    let source = text.slice(origin..end.max(origin)).to_string();
    let document = focus_md::parse(&source, &focus_md::ParseOptions::default());
    let mut found = Vec::new();
    table_cells(&document.blocks, &mut found);
    Some(
        found
            .get(index)?
            .iter()
            .map(|row| {
                row.iter()
                    .map(|c| origin + c.start..origin + c.end)
                    .collect()
            })
            .collect(),
    )
}

#[derive(Debug)]
struct Grid {
    source: String,
    base: usize,
    newline: &'static str,
    rows: Vec<Row>,
    delimiter: Row,
}

#[derive(Clone, Debug)]
struct Row {
    line: Range<usize>,
    cells: Vec<Range<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    RowAbove,
    RowBelow,
    ColumnBefore,
    ColumnAfter,
    DeleteRow,
    DeleteColumn,
    Align(Align),
    AlignTable,
}

fn cell_text(typed: &str) -> String {
    let mut out = String::with_capacity(typed.len());
    let mut chars = typed.chars().peekable();
    let mut escaped = false;
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(' ');
            }
            '\n' => out.push(' '),
            '|' if !escaped => out.push_str("\\|"),
            c => out.push(c),
        }
        escaped = c == '\\' && !escaped;
    }
    out
}

fn is_dashes(cell: &str) -> bool {
    let inner = cell.strip_prefix(':').unwrap_or(cell);
    let inner = inner.strip_suffix(':').unwrap_or(inner);
    !inner.is_empty() && inner.bytes().all(|b| b == b'-')
}

fn align_of(dashes: &str) -> Align {
    match (dashes.starts_with(':'), dashes.ends_with(':')) {
        (true, true) => Align::Center,
        (true, false) => Align::Left,
        (false, true) => Align::Right,
        (false, false) => Align::None,
    }
}

fn dashes(align: Align, width: usize) -> String {
    let width = width.max(3);
    match align {
        Align::None => "-".repeat(width),
        Align::Left => format!(":{}", "-".repeat(width - 1)),
        Align::Right => format!("{}:", "-".repeat(width - 1)),
        Align::Center => format!(":{}:", "-".repeat(width - 2)),
    }
}

#[derive(Default)]
struct Splices(Vec<(Range<usize>, String)>);

impl Splices {
    fn put(&mut self, range: Range<usize>, text: impl Into<String>) {
        self.0.push((range, text.into()));
    }

    fn apply(mut self, source: &str) -> (String, Self) {
        self.0.sort_by_key(|(range, _)| (range.start, range.end));
        let mut out = String::with_capacity(source.len() + 64);
        let mut at = 0;
        for (range, text) in &self.0 {
            out.push_str(&source[at..range.start]);
            out.push_str(text);
            at = range.end;
        }
        out.push_str(&source[at..]);
        (out, self)
    }

    fn start_of(&self, at: usize) -> usize {
        let shift: isize = self
            .0
            .iter()
            .filter(|(range, _)| range.start < at)
            .map(|(range, text)| text.len() as isize - range.len() as isize)
            .sum();
        (at as isize + shift) as usize
    }

    fn map(&self, old: usize) -> usize {
        let mut shift: isize = 0;
        for (range, text) in &self.0 {
            if old < range.start || (old == range.start && !range.is_empty()) {
                break;
            }
            if old < range.end {
                return (range.start as isize + shift) as usize;
            }
            shift += text.len() as isize - range.len() as isize;
        }
        (old as isize + shift) as usize
    }
}

impl Grid {
    fn read(text: &Rope, cells: &[Vec<Range<usize>>]) -> Option<Grid> {
        let first = cells.first()?.first()?.start;
        let last = cells.last()?.first()?.start;
        let start = text.line_start_offset(text.offset_to_point(first).row);
        let last_row = text.offset_to_point(last).row;
        let end = text.line_start_offset(last_row) + text.slice_line(last_row).len();
        let source = text.slice(start..end).to_string();
        Grid::new(source, start, cells)
    }

    fn new(source: String, base: usize, cells: &[Vec<Range<usize>>]) -> Option<Grid> {
        let line_of = |at: usize| {
            let start = source[..at].rfind('\n').map_or(0, |i| i + 1);
            let end = source[at..].find('\n').map_or(source.len(), |i| at + i);
            let end = if source[start..end].ends_with('\r') {
                end - 1
            } else {
                end
            };
            start..end
        };
        let mut rows = Vec::with_capacity(cells.len());
        for row in cells {
            let cells: Vec<Range<usize>> = row
                .iter()
                .map(|c| Some(c.start.checked_sub(base)?..c.end.checked_sub(base)?))
                .collect::<Option<_>>()?;
            if cells.iter().any(|c| c.end > source.len()) {
                return None;
            }
            let line = line_of(cells.first()?.start);
            rows.push(Row { line, cells });
        }
        let head_end = rows.first()?.line.end;
        let after = source[head_end..].find('\n')? + head_end + 1;
        let delimiter_line = line_of(after);
        let mut delimiter_cells = Vec::new();
        let mut at = delimiter_line.start;
        for part in source[delimiter_line.clone()].split('|') {
            let trimmed = part.trim();
            if is_dashes(trimmed) {
                let start = at + (part.len() - part.trim_start().len());
                delimiter_cells.push(start..start + trimmed.len());
            }
            at += part.len() + 1;
        }
        if delimiter_cells.is_empty() {
            return None;
        }
        let newline = if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        Some(Grid {
            source,
            base,
            newline,
            rows,
            delimiter: Row {
                line: delimiter_line,
                cells: delimiter_cells,
            },
        })
    }

    fn columns(&self) -> usize {
        self.rows[0].cells.len()
    }

    fn cell_of(&self, byte: usize) -> Option<(usize, usize)> {
        let at = byte.checked_sub(self.base)?;
        self.rows.iter().enumerate().find_map(|(r, row)| {
            row.cells
                .iter()
                .position(|cell| cell.start <= at && at <= cell.end)
                .map(|c| (r, c))
        })
    }

    fn cell_end(&self, row: usize, column: usize) -> usize {
        let cells = &self.rows[row].cells;
        self.base + cells[column.min(cells.len() - 1)].end
    }

    fn only_move(caret: usize, to: usize) -> Edit {
        Edit {
            range: caret..caret,
            text: String::new(),
            caret: to,
        }
    }

    fn key(&self, caret: usize, selection: Range<usize>, key: &Key) -> Option<Edit> {
        let (row, column) = self.cell_of(caret)?;
        let last_row = self.rows.len() - 1;
        let within = self.cell_of(selection.start) == Some((row, column))
            && self.cell_of(selection.end) == Some((row, column));
        match key {
            Key::Tab if column + 1 < self.rows[row].cells.len() => {
                Some(Self::only_move(caret, self.cell_end(row, column + 1)))
            }
            Key::Tab if row < last_row => Some(Self::only_move(caret, self.cell_end(row + 1, 0))),
            Key::Tab => self.op(last_row, 0, Op::RowBelow),
            Key::ShiftTab if column > 0 => {
                Some(Self::only_move(caret, self.cell_end(row, column - 1)))
            }
            Key::ShiftTab if row > 0 => {
                let previous = self.rows[row - 1].cells.len() - 1;
                Some(Self::only_move(caret, self.cell_end(row - 1, previous)))
            }
            Key::ShiftTab | Key::ShiftEnter => Some(Self::only_move(caret, caret)),
            Key::Enter if row < last_row => {
                Some(Self::only_move(caret, self.cell_end(row + 1, column)))
            }
            Key::Enter if row > 0 && selection.is_empty() && self.blank(row) => {
                Some(self.leave(row))
            }
            Key::Enter => self.op(last_row, column, Op::RowBelow),
            Key::Text(typed) => {
                let text = cell_text(typed);
                if within {
                    let caret = selection.start + text.len();
                    return Some(Edit {
                        range: selection,
                        text,
                        caret,
                    });
                }
                self.clear(selection, &text)
            }
            Key::Backspace | Key::Delete if !selection.is_empty() => {
                if within {
                    return None;
                }
                self.clear(selection, "")
            }
            Key::Backspace => self.escaped_pipe(caret, false),
            Key::Delete => self.escaped_pipe(caret, true),
        }
    }

    fn blank(&self, row: usize) -> bool {
        self.rows[row]
            .cells
            .iter()
            .all(|cell| self.source[cell.clone()].trim().is_empty())
    }

    fn leave(&self, row: usize) -> Edit {
        let row = &self.rows[row];
        let lead = &self.source[row.line.start..row.cells[0].start];
        let container = lead.trim_end().strip_suffix('|').unwrap_or(lead);
        let text = format!("{}{}{container}", container.trim_end(), self.newline);
        Edit {
            range: self.base + row.line.start..self.base + row.line.end,
            caret: self.base + row.line.start + text.len(),
            text,
        }
    }

    fn escaped_pipe(&self, caret: usize, forward: bool) -> Option<Edit> {
        let at = caret.checked_sub(self.base)?;
        let pair = |start: usize| self.source.get(start..start + 2) == Some("\\|");
        let range = if forward {
            if pair(at) {
                at..at + 2
            } else if at >= 1 && pair(at - 1) {
                at - 1..at + 1
            } else {
                return None;
            }
        } else if at >= 2 && pair(at - 2) {
            at - 2..at
        } else if at >= 1 && pair(at - 1) {
            let (row, column) = self.cell_of(caret)?;
            if self.rows[row].cells[column].start == at - 1 {
                return None;
            }
            let before = self.source[..at - 1].chars().next_back()?;
            at - 1 - before.len_utf8()..at - 1
        } else {
            return None;
        };
        Some(Edit {
            range: self.base + range.start..self.base + range.end,
            text: String::new(),
            caret: self.base + range.start,
        })
    }

    fn clear(&self, selection: Range<usize>, text: &str) -> Option<Edit> {
        let start = selection.start.checked_sub(self.base)?;
        let end = selection.end.checked_sub(self.base)?;
        if end > self.source.len() {
            return None;
        }
        let mut out = String::new();
        let mut at = start;
        let mut insert = None;
        for cell in self.rows.iter().flat_map(|row| row.cells.iter()) {
            let (from, to) = (cell.start.max(start), cell.end.min(end));
            if from >= to {
                continue;
            }
            out.push_str(&self.source[at..from]);
            insert.get_or_insert(out.len());
            at = to;
        }
        out.push_str(&self.source[at..end]);
        let insert = insert.unwrap_or(0);
        out.insert_str(insert, text);
        Some(Edit {
            range: selection.clone(),
            text: out,
            caret: selection.start + insert + text.len(),
        })
    }

    fn allows(&self, row: usize, op: Op) -> bool {
        match op {
            Op::RowAbove | Op::DeleteRow => row > 0,
            Op::DeleteColumn => self.columns() > 1,
            _ => true,
        }
    }

    fn align(&self, column: usize) -> Align {
        self.delimiter
            .cells
            .get(column)
            .map_or(Align::None, |c| align_of(&self.source[c.clone()]))
    }

    fn empty_row(&self, template: &Row) -> (String, Vec<usize>) {
        let mut out = String::new();
        let mut starts = Vec::with_capacity(template.cells.len());
        let mut at = template.line.start;
        for cell in &template.cells {
            out.push_str(&self.source[at..cell.start]);
            starts.push(out.len());
            at = cell.end;
        }
        out.push_str(&self.source[at..template.line.end]);
        (out, starts)
    }

    fn leading_pipe(&self, row: &Row) -> bool {
        self.source[row.line.start..row.cells[0].start]
            .trim_end()
            .ends_with('|')
    }

    fn trailing_pipe(&self, row: &Row) -> bool {
        row.cells.last().is_some_and(|last| {
            self.source[last.end..row.line.end]
                .trim_start()
                .starts_with('|')
        })
    }

    fn op(&self, row: usize, column: usize, op: Op) -> Option<Edit> {
        if !self.allows(row, op) || row >= self.rows.len() {
            return None;
        }
        let column = column.min(self.rows[row].cells.len().saturating_sub(1));
        let mut splices = Splices::default();
        let mut caret_old = Some(self.rows[row].cells[column].end);
        let mut caret_new = None;
        match op {
            Op::RowAbove | Op::RowBelow => {
                let (text, starts) = self.empty_row(&self.rows[row]);
                let start = starts.get(column).copied().unwrap_or(0);
                caret_old = None;
                if op == Op::RowAbove {
                    let at = self.rows[row].line.start;
                    splices.put(at..at, format!("{text}{}", self.newline));
                    caret_new = Some((at, start));
                } else {
                    let line = if row == 0 {
                        &self.delimiter.line
                    } else {
                        &self.rows[row].line
                    };
                    splices.put(line.end..line.end, format!("{}{text}", self.newline));
                    caret_new = Some((line.end, self.newline.len() + start));
                }
            }
            Op::ColumnBefore | Op::ColumnAfter => {
                let before = op == Op::ColumnBefore;
                for (r, line) in self
                    .rows
                    .iter()
                    .enumerate()
                    .map(|(r, l)| (Some(r), l))
                    .chain([(None, &self.delimiter)])
                {
                    let Some(cell) = line.cells.get(column) else {
                        continue;
                    };
                    let new = if r.is_none() { "---" } else { "" };
                    let (at, text, offset) = if before {
                        if column == 0 && !self.leading_pipe(line) {
                            (cell.start, format!("| {new} | "), 2)
                        } else {
                            (cell.start, format!("{new} | "), 0)
                        }
                    } else if column + 1 == line.cells.len() && !self.trailing_pipe(line) {
                        (cell.end, format!(" | {new} |"), 3)
                    } else {
                        (cell.end, format!(" | {new}"), 3)
                    };
                    splices.put(at..at, text);
                    if r == Some(row) {
                        caret_old = None;
                        caret_new = Some((at, offset));
                    }
                }
            }
            Op::DeleteColumn => {
                for line in self.rows.iter().chain([&self.delimiter]) {
                    if column >= line.cells.len() || line.cells.len() < 2 {
                        continue;
                    }
                    let range = if column == 0 {
                        line.cells[0].start..line.cells[1].start
                    } else {
                        line.cells[column - 1].end..line.cells[column].end
                    };
                    splices.put(range, "");
                }
                let kept = if column == 0 { 1 } else { column - 1 };
                caret_old = self.rows[row].cells.get(kept).map(|c| c.end);
            }
            Op::DeleteRow => {
                let line = &self.rows[row].line;
                let previous_end = if row == 1 {
                    self.delimiter.line.end
                } else {
                    self.rows[row - 1].line.end
                };
                let range = if row + 1 < self.rows.len() {
                    line.start..self.rows[row + 1].line.start
                } else {
                    previous_end..line.end
                };
                splices.put(range, "");
                let next = if row + 1 < self.rows.len() {
                    row + 1
                } else {
                    row - 1
                };
                let cells = &self.rows[next].cells;
                caret_old = Some(cells[column.min(cells.len() - 1)].end);
            }
            Op::Align(align) => {
                let cell = self.delimiter.cells.get(column)?.clone();
                splices.put(cell.clone(), dashes(align, cell.len()));
            }
            Op::AlignTable => return Some(self.padded(row, column)),
        }
        let (text, splices) = splices.apply(&self.source);
        let caret = match (caret_old, caret_new) {
            (_, Some((at, offset))) => splices.start_of(at) + offset,
            (Some(old), None) => splices.map(old),
            (None, None) => 0,
        };
        Some(Edit {
            range: self.base..self.base + self.source.len(),
            text,
            caret: self.base + caret,
        })
    }

    fn padded(&self, row: usize, column: usize) -> Edit {
        let count = self.columns();
        let content =
            |r: &Row, c: usize| r.cells.get(c).map_or("", |cell| &self.source[cell.clone()]);
        let widths: Vec<usize> = (0..count)
            .map(|c| {
                self.rows
                    .iter()
                    .map(|r| content(r, c).chars().count())
                    .max()
                    .unwrap_or(0)
                    .max(3)
            })
            .collect();
        let aligns: Vec<Align> = (0..count).map(|c| self.align(c)).collect();
        let prefix = |r: &Row| {
            let before =
                &self.source[r.line.start..r.cells.first().map_or(r.line.end, |c| c.start)];
            match before.rfind('|') {
                Some(pipe) => &before[..pipe],
                None => before.trim_end_matches(' '),
            }
            .to_string()
        };
        let caret_offset = self.rows[row]
            .cells
            .get(column)
            .map_or(0, |cell| cell.len());
        let mut out = String::with_capacity(self.source.len() + 64);
        let mut caret = 0;
        let lines: Vec<(Option<usize>, &Row)> = std::iter::once((Some(0), &self.rows[0]))
            .chain([(None, &self.delimiter)])
            .chain(
                self.rows
                    .iter()
                    .enumerate()
                    .skip(1)
                    .map(|(r, l)| (Some(r), l)),
            )
            .collect();
        for (n, (r, line)) in lines.into_iter().enumerate() {
            if n > 0 {
                out.push_str(self.newline);
            }
            out.push_str(&prefix(line));
            out.push('|');
            for c in 0..count {
                out.push(' ');
                let width = widths[c];
                match r {
                    None => out.push_str(&dashes(aligns[c], width)),
                    Some(r) => {
                        let text = content(line, c);
                        let room = width - text.chars().count();
                        let left = match aligns[c] {
                            Align::Right => room,
                            Align::Center => room / 2,
                            Align::Left | Align::None => 0,
                        };
                        out.push_str(&" ".repeat(left));
                        if r == row && c == column {
                            caret = out.len() + caret_offset;
                        }
                        out.push_str(text);
                        out.push_str(&" ".repeat(room - left));
                    }
                }
                out.push_str(" |");
            }
        }
        Edit {
            range: self.base..self.base + self.source.len(),
            text: out,
            caret: self.base + caret,
        }
    }
}

fn table_node(node: &Node, first: u32) -> Option<&Node> {
    match node {
        Node::Table { head, rows, .. } => {
            let ord = head
                .first()
                .or_else(|| rows.first().and_then(|r| r.first()))
                .map_or(0, |r| r.ord);
            (ord == first).then_some(node)
        }
        Node::List { entries, .. } => entries
            .iter()
            .flat_map(|e| e.children.iter())
            .find_map(|child| table_node(child, first)),
        Node::Quote(children) | Node::Callout { children, .. } | Node::Details { children, .. } => {
            children.iter().find_map(|child| table_node(child, first))
        }
        Node::Tabs { tabs, .. } => tabs
            .iter()
            .flat_map(|t| t.items.iter())
            .find_map(|item| table_node(&item.node, first)),
        _ => None,
    }
}

impl ReadingView {
    fn table_grid(&self, table: (usize, u32), cx: &App) -> Option<Grid> {
        let text = self.in_place.as_ref()?.read(cx).text().clone();
        let item = self.built.as_ref()?.items.get(table.0)?;
        let node = table_node(&item.node, table.1)?;
        let origin = self.origin(table.0 as u32 + 1, &text)?;
        Grid::read(&text, &cells(&text, item, origin, node)?)
    }

    fn table_at(&self, text: &Rope, byte: usize) -> Option<((usize, u32), Grid)> {
        let index = (self.key_item_at(byte, text) as usize).checked_sub(1)?;
        let item = self.built.as_ref()?.items.get(index)?;
        let origin = self.origin(index as u32 + 1, text)?;
        let mut nodes = Vec::new();
        table_nodes(&item.node, &mut nodes);
        nodes.into_iter().find_map(|node| {
            let grid = Grid::read(text, &cells(text, item, origin, node)?)?;
            grid.cell_of(byte)?;
            Some(((index, first_ord(node)), grid))
        })
    }

    fn table_editable(&self, table: (usize, u32)) -> bool {
        let Some(built) = self.built.as_ref() else {
            return false;
        };
        let Some(item) = built.items.get(table.0) else {
            return false;
        };
        let Some(node @ Node::Table { head, rows, .. }) = table_node(&item.node, table.1) else {
            return false;
        };
        if !head
            .iter()
            .chain(rows.iter().flatten())
            .any(|r| crate::srcmap::of(r).is_some())
        {
            return false;
        }
        let html = std::ptr::eq(node, &item.node)
            && built.source.as_deref().is_some_and(|text| {
                text.lines()
                    .nth(item.lines.start as usize)
                    .is_some_and(|line| line.trim_start().starts_with('<'))
            });
        !html
    }

    fn table_rtl(&self, table: (usize, u32)) -> bool {
        self.built
            .as_ref()
            .and_then(|b| b.items.get(table.0))
            .and_then(|item| table_node(&item.node, table.1))
            .and_then(|node| match node {
                Node::Table { head, rows, .. } => head
                    .iter()
                    .chain(rows.iter().flatten())
                    .find(|r| r.text.chars().any(char::is_alphabetic))
                    .map(|r| focus_kit::Dir::current().of_text(&r.text).rtl),
                _ => None,
            })
            .unwrap_or(focus_kit::Dir::current().rtl)
    }

    fn table_op(
        &mut self,
        table: (usize, u32),
        row: usize,
        column: usize,
        op: Op,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(edit) = self
            .table_grid(table, cx)
            .and_then(|grid| grid.op(row, column, op))
        {
            self.start_editing(window, cx);
            self.apply(edit, window, cx);
        }
    }

    fn open_table_menu(
        &mut self,
        table: (usize, u32),
        (at, corner): (Point<Pixels>, Corner),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(grid) = self.table_grid(table, cx) else {
            return;
        };
        let caret = self
            .caret_shown()
            .then(|| self.field_state(window, cx))
            .flatten()
            .and_then(|(_, _, head)| grid.cell_of(head));
        let last = grid.rows.len() - 1;
        let (row, column) = caret.unwrap_or((last, grid.rows[last].cells.len().saturating_sub(1)));
        let focus = self
            .table_menu
            .focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        self.table_menu.opened = Some(Opened {
            table,
            at,
            corner,
            row,
            column,
        });
        window.focus(&focus);
        cx.notify();
    }

    fn close_table_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.table_menu.opened.take().is_some() {
            if self.caret_shown() {
                self.start_editing(window, cx);
            }
            cx.notify();
        }
    }

    fn table_menu_anchor(&self, table: (usize, u32)) -> Option<(Point<Pixels>, Corner)> {
        let visible = self.table_menu.visible.borrow().get(&table).copied()?;
        let top = visible.top() + px(BUTTON + 6.);
        Some(if self.table_rtl(table) {
            (point(visible.left() + px(4.), top), Corner::TopLeft)
        } else {
            (point(visible.right() - px(4.), top), Corner::TopRight)
        })
    }
}

fn first_ord(node: &Node) -> u32 {
    match node {
        Node::Table { head, rows, .. } => head
            .first()
            .or_else(|| rows.first().and_then(|r| r.first()))
            .map_or(0, |r| r.ord),
        _ => 0,
    }
}

const BUTTON: f32 = 28.;

type Drawn = Rc<RefCell<HashMap<(usize, u32), Bounds<Pixels>>>>;

#[derive(Default)]
pub(in crate::view) struct Menus {
    hovered: Option<(usize, u32)>,
    opened: Option<Opened>,
    visible: Drawn,
    focus: Option<FocusHandle>,
}

#[derive(Clone, Copy)]
struct Opened {
    table: (usize, u32),
    at: Point<Pixels>,
    corner: Corner,
    row: usize,
    column: usize,
}

struct MenuItem {
    op: Op,
    enabled: bool,
    checked: bool,
}

const ITEMS: [(&str, Op); 11] = [
    ("row-above", Op::RowAbove),
    ("row-below", Op::RowBelow),
    ("column-left", Op::ColumnBefore),
    ("column-right", Op::ColumnAfter),
    ("delete-row", Op::DeleteRow),
    ("delete-column", Op::DeleteColumn),
    ("align-left", Op::Align(Align::Left)),
    ("align-center", Op::Align(Align::Center)),
    ("align-right", Op::Align(Align::Right)),
    ("align-none", Op::Align(Align::None)),
    ("align-table", Op::AlignTable),
];

pub(in crate::view) fn menu(
    view: &ReadingView,
    table: (usize, u32),
    t: &Tokens,
    cx: &mut Context<ReadingView>,
) -> Option<AnyElement> {
    if !view.live || view.in_place.is_none() {
        return None;
    }
    let width = view.tables.borrow().get(&table).map(|columns| {
        let room = view
            .table_room
            .borrow()
            .get(&table)
            .copied()
            .unwrap_or_else(|| view.room());
        columns.widths(room).iter().sum::<f32>().max(room)
    });
    let opened = view.table_menu.opened.filter(|o| o.table == table);
    let items = opened.and_then(|opened| {
        let grid = view.table_grid(table, cx)?;
        let align = grid.align(opened.column);
        Some(
            ITEMS
                .iter()
                .map(|(_, op)| MenuItem {
                    op: *op,
                    enabled: grid.allows(opened.row, *op),
                    checked: matches!(op, Op::Align(a) if *a == align),
                })
                .collect(),
        )
    });
    Some(
        TableMenu {
            view: cx.entity().downgrade(),
            table,
            t: *t,
            width,
            rtl: view.table_rtl(table),
            editable: view.table_editable(table),
            hovered: view.table_menu.hovered == Some(table),
            visible: view.table_menu.visible.borrow().get(&table).copied(),
            record: view.table_menu.visible.clone(),
            focus: view.table_menu.focus.clone(),
            opened,
            items: items.unwrap_or_default(),
        }
        .into_any_element(),
    )
}

#[derive(IntoElement)]
struct TableMenu {
    view: WeakEntity<ReadingView>,
    table: (usize, u32),
    t: Tokens,
    width: Option<f32>,
    rtl: bool,
    editable: bool,
    hovered: bool,
    visible: Option<Bounds<Pixels>>,
    record: Drawn,
    focus: Option<FocusHandle>,
    opened: Option<Opened>,
    items: Vec<MenuItem>,
}

impl TableMenu {
    fn popup(&self, opened: Opened, cx: &App) -> Option<AnyElement> {
        let focus = self.focus.clone()?;
        let t = self.t;
        let mut rows = Vec::new();
        for (n, (item, (name, _))) in self.items.iter().zip(ITEMS).enumerate() {
            if matches!(n, 2 | 4 | 6 | 10) {
                rows.push(focus_kit::menu_separator(&t).into_any_element());
            }
            let (view, op) = (self.view.clone(), item.op);
            rows.push(
                MenuRow::new(
                    ElementId::from(("table-menu-item", n)),
                    focus_i18n::t(&format!("reading-table-{name}")),
                )
                .disabled(!item.enabled)
                .checked(item.checked)
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.close_table_menu(window, cx);
                        view.table_op(opened.table, opened.row, opened.column, op, window, cx);
                    });
                })
                .into_any_element(),
            );
        }
        let (out, key) = (self.view.clone(), self.view.clone());
        Some(
            gpui::deferred(
                gpui::anchored()
                    .anchor(opened.corner)
                    .position(opened.at)
                    .snap_to_window()
                    .child(focus_kit::fade_in(
                        "fade-table-menu",
                        focus_kit::menu(&t)
                            .id("table-menu")
                            .occlude()
                            .track_focus(&focus)
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_mouse_down_out(move |_, window, cx| {
                                let _ =
                                    out.update(cx, |view, cx| view.close_table_menu(window, cx));
                            })
                            .capture_key_down(move |event: &KeyDownEvent, window, cx| {
                                if event.keystroke.key == "escape" {
                                    cx.stop_propagation();
                                    let _ = key
                                        .update(cx, |view, cx| view.close_table_menu(window, cx));
                                }
                            })
                            .children(rows),
                        cx,
                    )),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

impl RenderOnce for TableMenu {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = self.t;
        let table = self.table;
        let button = (self.editable && (self.hovered || self.opened.is_some()))
            .then_some(self.visible)
            .flatten()
            .map(|visible| {
                let x = if self.rtl {
                    visible.left() + px(4.)
                } else {
                    visible.right() - px(BUTTON + 4.)
                };
                let view = self.view.clone();
                gpui::deferred(
                    gpui::anchored()
                        .position(point(x, visible.top() + px(4.)))
                        .child(
                            div().bg(t.bg).child(
                                IconButton::new(
                                    "table-menu-button",
                                    Icon::MORE,
                                    focus_i18n::t("reading-table-menu"),
                                )
                                .dense(true)
                                .active(self.opened.is_some())
                                .on_click(move |_, window, cx| {
                                    let _ = view.update(cx, |view, cx| {
                                        if let Some(at) = view.table_menu_anchor(table) {
                                            view.open_table_menu(table, at, window, cx);
                                        }
                                    });
                                }),
                            ),
                        ),
                )
                .with_priority(1)
            });
        let popup = self.opened.and_then(|opened| self.popup(opened, cx));
        let (hover, press, record) = (self.view.clone(), self.view.clone(), self.record.clone());
        div()
            .id(ElementId::from((
                "table-menu-area",
                (table.0 << 16) | table.1 as usize,
            )))
            .absolute()
            .top_0()
            .left_0()
            .h_full()
            .map(|this| match self.width {
                Some(width) => this.w(px(width)),
                None => this.w_full(),
            })
            .when(self.editable, |this| {
                this.on_hover(move |hovered, _, cx| {
                    let _ = hover.update(cx, |view, cx| {
                        let now = hovered.then_some(table);
                        if (*hovered || view.table_menu.hovered == Some(table))
                            && view.table_menu.hovered != now
                        {
                            view.table_menu.hovered = now;
                            cx.notify();
                        }
                    });
                })
                .on_mouse_down(
                    MouseButton::Right,
                    move |event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        let _ = press.update(cx, |view, cx| {
                            if view.caret_at_point(event.position, window, cx) {
                                let at = (event.position, Corner::TopLeft);
                                view.open_table_menu(table, at, window, cx);
                            }
                        });
                    },
                )
            })
            .child(
                gpui::canvas(
                    move |bounds, window, _| {
                        let visible = bounds.intersect(&window.content_mask().bounds);
                        record.borrow_mut().insert(table, visible);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .children(button)
            .children(popup)
    }
}

impl ReadingView {
    pub fn script_table(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let op = match ITEMS.iter().find(|(item, _)| *item == name) {
            Some((_, op)) => Some(*op),
            None if name == "menu" => None,
            None => return Err(format!("no table menu item {name}")),
        };
        let (text, _, head) = self
            .caret_shown()
            .then(|| self.field_state(window, cx))
            .flatten()
            .ok_or("no caret in place")?;
        let (table, grid) = self
            .table_at(&text, head)
            .ok_or("the caret is not in a cell of a Markdown table")?;
        let Some(op) = op else {
            let at = self
                .table_menu_anchor(table)
                .ok_or("the table was not drawn at the last frame")?;
            self.open_table_menu(table, at, window, cx);
            return Ok(());
        };
        let (row, column) = grid.cell_of(head).ok_or("the caret is not in a cell")?;
        if !grid.allows(row, op) {
            return Err(format!("{name}: disabled in this cell"));
        }
        self.table_op(table, row, column, op, window, cx);
        Ok(())
    }

    fn caret_at_point(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(text) = self.field_text(cx) else {
            return false;
        };
        let Some(byte) = self.byte_in(position, &text) else {
            return false;
        };
        self.start_editing(window, cx);
        self.select(byte, byte, cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use focus_md::Align;

    use gpui_component::{Rope, RopeExt as _};

    use super::{Edit, Grid, Key, Op, cell_text};

    fn grid(source: &str, base: usize, cells: &[&[Range<usize>]]) -> Grid {
        let cells: Vec<Vec<Range<usize>>> = cells
            .iter()
            .map(|row| row.iter().map(|c| base + c.start..base + c.end).collect())
            .collect();
        Grid::new(source.to_string(), base, &cells).expect("a Markdown table")
    }

    const SIMPLE: &str = "| a | b |\n|---|---|\n| c | d |\n| e | f |";
    fn simple() -> Grid {
        grid(
            SIMPLE,
            100,
            &[&[2..3, 6..7], &[22..23, 26..27], &[32..33, 36..37]],
        )
    }

    fn after(grid: &Grid, edit: &Edit) -> String {
        let mut out = grid.source.clone();
        out.replace_range(
            edit.range.start - grid.base..edit.range.end - grid.base,
            &edit.text,
        );
        out
    }

    fn caret_in(grid: &Grid, edit: &Edit) -> String {
        let text = after(grid, edit);
        let at = edit.caret - grid.base;
        format!("{}^{}", &text[..at], &text[at..])
    }

    #[test]
    fn typing_in_a_cell_writes_only_its_bytes() {
        let g = simple();
        let edit = g
            .key(123, 123..123, &Key::Text("x".into()))
            .expect("a rule");
        assert_eq!(
            edit,
            Edit {
                range: 123..123,
                text: "x".into(),
                caret: 124
            }
        );
        let edit = g
            .key(123, 123..123, &Key::Text("a|b".into()))
            .expect("a rule");
        assert_eq!(edit.text, "a\\|b");
        assert_eq!(edit.caret, 123 + 4);
        let edit = g
            .key(123, 122..123, &Key::Text("x\\|y\r\nz\nw".into()))
            .expect("a rule");
        assert_eq!(
            edit,
            Edit {
                range: 122..123,
                text: "x\\|y z w".into(),
                caret: 122 + 8
            }
        );
    }

    #[test]
    fn a_pasted_pipe_and_line_break_stay_in_the_cell() {
        assert_eq!(cell_text("|"), "\\|");
        assert_eq!(cell_text("a\\|b|c"), "a\\|b\\|c");
        assert_eq!(cell_text("a\\\\|b"), "a\\\\\\|b");
        assert_eq!(cell_text("one\r\ntwo\rthree\nfour"), "one two three four");
    }

    #[test]
    fn tab_and_shift_tab_go_from_cell_to_cell() {
        let g = simple();
        let tab = |caret| {
            g.key(caret, caret..caret, &Key::Tab)
                .map(|e| caret_in(&g, &e))
        };
        assert_eq!(
            tab(102).as_deref(),
            Some(&*SIMPLE.replacen("b |", "b^ |", 1))
        );
        assert_eq!(
            tab(107).as_deref(),
            Some(&*SIMPLE.replacen("c |", "c^ |", 1))
        );
        let back = |caret| {
            g.key(caret, caret..caret, &Key::ShiftTab)
                .map(|e| caret_in(&g, &e))
        };
        assert_eq!(
            back(122).as_deref(),
            Some(&*SIMPLE.replacen("b |", "b^ |", 1))
        );
        assert_eq!(
            back(126).as_deref(),
            Some(&*SIMPLE.replacen("c |", "c^ |", 1))
        );
        assert_eq!(
            back(102).as_deref(),
            Some(&*SIMPLE.replacen("| a", "| ^a", 1))
        );
        let moved = g.key(102, 102..102, &Key::Tab).expect("a rule");
        assert!(moved.range.is_empty() && moved.text.is_empty());
    }

    #[test]
    fn tab_in_the_last_cell_adds_a_row() {
        let g = simple();
        let edit = g.key(137, 137..137, &Key::Tab).expect("a rule");
        assert_eq!(edit.range, 100..100 + SIMPLE.len());
        assert_eq!(caret_in(&g, &edit), format!("{SIMPLE}\n| ^ |  |"));
    }

    #[test]
    fn enter_goes_down_and_adds_a_row_under_the_last() {
        let g = simple();
        let enter = |caret| {
            g.key(caret, caret..caret, &Key::Enter)
                .map(|e| caret_in(&g, &e))
        };
        assert_eq!(
            enter(106).as_deref(),
            Some(&*SIMPLE.replacen("d |", "d^ |", 1))
        );
        assert_eq!(
            enter(127).as_deref(),
            Some(&*SIMPLE.replacen("f |", "f^ |", 1))
        );
        assert_eq!(enter(137).as_deref(), Some(&*format!("{SIMPLE}\n|  | ^ |")));
        let shift = g.key(123, 123..123, &Key::ShiftEnter).expect("a rule");
        assert!(shift.text.is_empty() && shift.caret == 123);
    }

    #[test]
    fn enter_on_an_empty_last_row_leaves_the_table() {
        let note = format!("{SIMPLE}\n|  |  |\n\nAfter.\n");
        let g = parsed(&note, 0, 0).expect("a Markdown table");
        let caret = SIMPLE.len() + 3;
        let edit = g.key(caret, caret..caret, &Key::Enter).expect("a rule");
        assert_eq!(apply(&note, &edit), format!("{SIMPLE}\n\n\n\nAfter.\n"));
        assert_eq!(edit.caret, SIMPLE.len() + 2);
        assert_eq!(shape(&apply(&note, &edit)).len(), 3);
        let g = simple();
        assert!(
            g.key(137, 137..137, &Key::Enter)
                .is_some_and(|e| !e.text.is_empty())
        );
        let callout = "> [!note] T\r\n> | a | b |\r\n> |---|---|\r\n> |   |  |\r\n";
        let g = parsed(callout, 0, 0).expect("a Markdown table");
        let caret = callout.find("|   |").expect("the empty row") + 2;
        let edit = g.key(caret, caret..caret, &Key::Enter).expect("a rule");
        let out = apply(callout, &edit);
        assert_eq!(out, callout.replace("> |   |  |\r\n", ">\r\n> \r\n"));
        assert_eq!(
            &out[..edit.caret],
            callout.replace("> |   |  |\r\n", ">\r\n> ")
        );
    }

    #[test]
    fn a_selection_over_cells_keeps_the_pipes() {
        let g = simple();
        let edit = g.key(126, 102..127, &Key::Backspace).expect("a rule");
        assert_eq!(after(&g, &edit), "|  |  |\n|---|---|\n|  |  |\n| e | f |");
        let edit = g
            .key(126, 102..127, &Key::Text("z".into()))
            .expect("a rule");
        assert_eq!(
            caret_in(&g, &edit),
            "| z^ |  |\n|---|---|\n|  |  |\n| e | f |"
        );
        assert_eq!(g.key(123, 122..123, &Key::Backspace), None);
    }

    #[test]
    fn backspace_takes_an_escaped_pipe_whole() {
        let source = "| a\\|b | c |\n|---|---|";
        let g = grid(source, 0, &[&[2..6, 9..10]]);
        let gone = |range: Range<usize>| {
            Some(Edit {
                caret: range.start,
                range,
                text: String::new(),
            })
        };
        assert_eq!(g.key(5, 5..5, &Key::Backspace), gone(3..5));
        assert_eq!(g.key(4, 4..4, &Key::Backspace), gone(2..3));
        assert_eq!(
            g.key(3, 3..3, &Key::Delete),
            Some(Edit {
                range: 3..5,
                text: String::new(),
                caret: 3
            })
        );
        assert_eq!(
            g.key(4, 4..4, &Key::Delete),
            Some(Edit {
                range: 3..5,
                text: String::new(),
                caret: 3
            })
        );
        assert_eq!(g.key(3, 3..3, &Key::Backspace), None);
    }

    #[test]
    fn rows_are_inserted_and_deleted() {
        let g = simple();
        let op = |row, column, op| g.op(row, column, op).map(|e| caret_in(&g, &e));
        assert_eq!(
            op(1, 1, Op::RowAbove).as_deref(),
            Some("| a | b |\n|---|---|\n|  | ^ |\n| c | d |\n| e | f |")
        );
        assert_eq!(
            op(1, 0, Op::RowBelow).as_deref(),
            Some("| a | b |\n|---|---|\n| c | d |\n| ^ |  |\n| e | f |")
        );
        assert_eq!(
            op(0, 0, Op::RowBelow).as_deref(),
            Some("| a | b |\n|---|---|\n| ^ |  |\n| c | d |\n| e | f |")
        );
        assert_eq!(op(0, 0, Op::RowAbove), None);
        assert_eq!(op(0, 0, Op::DeleteRow), None);
        assert_eq!(
            op(1, 1, Op::DeleteRow).as_deref(),
            Some("| a | b |\n|---|---|\n| e | f^ |")
        );
        assert_eq!(
            op(2, 0, Op::DeleteRow).as_deref(),
            Some("| a | b |\n|---|---|\n| c^ | d |")
        );
    }

    #[test]
    fn columns_are_inserted_and_deleted() {
        let g = simple();
        let op = |row, column, op| g.op(row, column, op).map(|e| caret_in(&g, &e));
        assert_eq!(
            op(1, 1, Op::ColumnBefore).as_deref(),
            Some("| a |  | b |\n|---|--- | ---|\n| c | ^ | d |\n| e |  | f |")
        );
        assert_eq!(
            op(2, 1, Op::ColumnAfter).as_deref(),
            Some("| a | b |  |\n|---|--- | ---|\n| c | d |  |\n| e | f | ^ |")
        );
        assert_eq!(
            op(1, 0, Op::DeleteColumn).as_deref(),
            Some("| b |\n|---|\n| d^ |\n| f |")
        );
        assert_eq!(
            op(0, 1, Op::DeleteColumn).as_deref(),
            Some("| a^ |\n|---|\n| c |\n| e |")
        );
    }

    #[test]
    fn rows_without_outer_pipes_keep_their_columns() {
        let source = "a | b\n--|--\nc | d";
        let g = grid(source, 0, &[&[0..1, 4..5], &[12..13, 16..17]]);
        let edit = g.op(1, 0, Op::ColumnBefore).expect("an operation");
        assert_eq!(
            caret_in(&g, &edit),
            "|  | a | b\n| --- | --|--\n| ^ | c | d"
        );
        let edit = g.op(1, 1, Op::ColumnAfter).expect("an operation");
        assert_eq!(
            caret_in(&g, &edit),
            "a | b |  |\n--|-- | --- |\nc | d | ^ |"
        );
    }

    #[test]
    fn alignment_rewrites_the_delimiter_row_only() {
        let g = simple();
        let edit = g.op(2, 1, Op::Align(Align::Center)).expect("an operation");
        assert_eq!(
            after(&g, &edit),
            "| a | b |\n|---|:-:|\n| c | d |\n| e | f |"
        );
        assert_eq!(edit.caret, 137);
        let edit = g.op(2, 0, Op::Align(Align::Right)).expect("an operation");
        assert_eq!(
            after(&g, &edit),
            "| a | b |\n|--:|---|\n| c | d |\n| e | f |"
        );
        assert_eq!(g.align(1), Align::None);
    }

    #[test]
    fn align_table_pads_by_characters() {
        let source = "| Nom | Valeur |\n|:--|--:|\n| é | 10 |\n| longer | 2 |";
        let g = grid(
            source,
            0,
            &[&[2..5, 8..14], &[29..31, 34..36], &[41..47, 50..51]],
        );
        let edit = g.op(1, 1, Op::AlignTable).expect("an operation");
        assert_eq!(
            caret_in(&g, &edit),
            "| Nom    | Valeur |\n| :----- | -----: |\n| é      |     10^ |\n| longer |      2 |"
        );
    }

    #[test]
    fn a_table_inside_a_quote_keeps_its_markers_and_line_breaks() {
        let source = "> | a | b |\r\n> |---|---|\r\n> | c | d |";
        let g = grid(source, 10, &[&[4..5, 8..9], &[30..31, 34..35]]);
        let edit = g.key(45, 45..45, &Key::Enter).expect("a rule");
        assert_eq!(caret_in(&g, &edit), format!("{source}\r\n> |  | ^ |"));
        let edit = g.op(0, 0, Op::RowBelow).expect("an operation");
        assert_eq!(
            after(&g, &edit),
            "> | a | b |\r\n> |---|---|\r\n> |  |  |\r\n> | c | d |"
        );
        let edit = g.op(1, 1, Op::AlignTable).expect("an operation");
        assert_eq!(
            after(&g, &edit),
            "> | a   | b   |\r\n> | --- | --- |\r\n> | c   | d   |"
        );
    }

    #[test]
    fn empty_cells_take_the_caret() {
        let source = "| a |  |\n|---|---|\n|  | d |";
        let g = grid(source, 0, &[&[2..3, 6..6], &[21..21, 24..25]]);
        let tab = g.key(3, 3..3, &Key::Tab).expect("a rule");
        assert_eq!(tab.caret, 6);
        let edit = g.key(6, 6..6, &Key::Text("x".into())).expect("a rule");
        assert_eq!(after(&g, &edit), "| a | x |\n|---|---|\n|  | d |");
        let enter = g.key(6, 6..6, &Key::Enter).expect("a rule");
        assert_eq!(enter.caret, 25);
    }

    struct Unresolved;

    impl crate::Resolver for Unresolved {
        fn resolve_link(&self, _: focus_core::NoteId, _: &str) -> focus_core::LinkTarget {
            focus_core::LinkTarget::Unresolved
        }
        fn absolute(&self, relative: &std::path::Path) -> std::path::PathBuf {
            relative.to_path_buf()
        }
    }

    fn parsed(note: &str, item: usize, nth: usize) -> Option<Grid> {
        let built = crate::model::build_text(note, None, &Unresolved, None);
        let rope = Rope::from(note);
        let item = &built.items[item];
        let mut nodes = Vec::new();
        super::table_nodes(&item.node, &mut nodes);
        let origin = rope.line_start_offset(item.lines.start as usize);
        Grid::read(&rope, &super::cells(&rope, item, origin, nodes.get(nth)?)?)
    }

    fn apply(note: &str, edit: &Edit) -> String {
        let mut out = note.to_string();
        out.replace_range(edit.range.clone(), &edit.text);
        out
    }

    fn shape(note: &str) -> Vec<Vec<String>> {
        let document = focus_md::parse(note, &focus_md::ParseOptions::default());
        let mut found = Vec::new();
        super::table_cells(&document.blocks, &mut found);
        found[0]
            .iter()
            .map(|row| row.iter().map(|c| note[c.clone()].to_string()).collect())
            .collect()
    }

    #[test]
    fn the_parser_reads_back_what_the_rules_write() {
        let note = "Avant.\n\n| a\\|b | |\n|:--|--:|\n| c |\n| é | f |\n\nAprès.\n";
        let g = parsed(note, 1, 0).expect("a Markdown table");
        let tab = g.key(14, 14..14, &Key::Tab).expect("a rule");
        assert_eq!(tab.caret, 17);
        let typed = apply(
            note,
            &g.key(17, 17..17, &Key::Text("x|y".into())).expect("a rule"),
        );
        assert_eq!(shape(&typed)[0], ["a\\|b", "x\\|y"]);
        let tab = g.key(32, 32..32, &Key::Tab).expect("a rule");
        let typed = apply(
            note,
            &Edit {
                range: tab.caret..tab.caret,
                text: "z".into(),
                caret: 0,
            },
        );
        assert_eq!(shape(&typed)[1], ["c", "z"]);
        let enter = apply(note, &g.key(43, 43..43, &Key::Enter).expect("a rule"));
        assert_eq!(enter, note.replace("| é | f |\n", "| é | f |\n|  |  |\n"));
        assert_eq!(shape(&enter).len(), 4);
        let after = |row, column, op| apply(note, &g.op(row, column, op).expect("an operation"));
        assert_eq!(shape(&after(1, 0, Op::RowAbove)).len(), 4);
        assert_eq!(
            shape(&after(2, 1, Op::DeleteRow)),
            [["a\\|b", ""], ["c", ""]]
        );
        assert_eq!(shape(&after(0, 1, Op::ColumnAfter))[2], ["é", "f", ""]);
        assert_eq!(shape(&after(2, 0, Op::ColumnBefore))[0], ["", "a\\|b", ""]);
        assert_eq!(shape(&after(2, 1, Op::DeleteColumn))[2], ["é"]);
        assert_eq!(
            after(1, 1, Op::AlignTable),
            "Avant.\n\n| a\\|b |     |\n| :--- | --: |\n| c    |     |\n| é    |   f |\n\nAprès.\n"
        );
    }

    #[test]
    fn a_table_in_a_callout_or_a_list_keeps_its_markers() {
        let callout = "> [!note] Titre\r\n> | a | b |\r\n> |---|---|\r\n> | c | d |\r\n";
        let g = parsed(callout, 0, 0).expect("a Markdown table");
        let enter = apply(callout, &g.key(52, 52..52, &Key::Enter).expect("a rule"));
        assert_eq!(
            enter,
            callout.replace("> | c | d |\r\n", "> | c | d |\r\n> |  |  |\r\n")
        );
        assert_eq!(shape(&enter).len(), 3);
        let list = "- item\n\n  | a | b |\n  |---|---|\n  | c | d |\n";
        let g = parsed(list, 0, 0).expect("a Markdown table");
        let inserted = apply(list, &g.op(1, 0, Op::ColumnBefore).expect("an operation"));
        assert_eq!(shape(&inserted), [["", "a", "b"], ["", "c", "d"]]);
        let aligned = apply(list, &g.op(1, 0, Op::AlignTable).expect("an operation"));
        assert_eq!(
            aligned,
            "- item\n\n  | a   | b   |\n  | --- | --- |\n  | c   | d   |\n"
        );
    }

    #[test]
    fn rows_without_outer_pipes_read_back_with_the_new_column() {
        let note = "a | b\n--|--\nc | d\n";
        let g = parsed(note, 0, 0).expect("a Markdown table");
        let before = apply(note, &g.op(1, 0, Op::ColumnBefore).expect("an operation"));
        assert_eq!(shape(&before), [["", "a", "b"], ["", "c", "d"]]);
        let after = apply(note, &g.op(1, 1, Op::ColumnAfter).expect("an operation"));
        assert_eq!(shape(&after), [["a", "b", ""], ["c", "d", ""]]);
    }

    #[test]
    fn an_html_table_parsed_has_no_rules() {
        assert!(parsed("<table>\n<tr><td>a</td><td>b</td></tr>\n</table>\n", 0, 0).is_none());
    }

    #[test]
    #[ignore = "a measure, not a check"]
    fn long_table_keys() {
        let mut note = String::from("# Long\n\n| Nom | Valeur | Note |\n|---|---|---|\n");
        for i in 0..200 {
            note.push_str(&format!(
                "| ligne {i} | {} | texte de la cellule {i} |\n",
                i * 7
            ));
        }
        let built = crate::model::build_text(&note, None, &Unresolved, None);
        let rope = Rope::from(note.as_str());
        let (index, item) = built
            .items
            .iter()
            .enumerate()
            .find(|(_, i)| matches!(i.node, crate::model::Node::Table { .. }))
            .expect("the table");
        let origin = rope.line_start_offset(item.lines.start as usize);
        let caret = note.find("texte de la cellule 100").expect("a cell") + 5;
        let at = super::super::At {
            text: &rope,
            item: Some(item),
            path: vec![&item.node],
            rich: None,
            prop: None,
            map: None,
            start: origin,
            caret,
            selection: caret..caret,
        };
        for (name, key) in [
            ("letter", Key::Text("x".into())),
            ("pipe", Key::Text("|".into())),
            ("tab", Key::Tab),
            ("enter", Key::Enter),
        ] {
            let mut times: Vec<f64> = (0..200)
                .map(|_| {
                    let start = std::time::Instant::now();
                    std::hint::black_box(super::key(&at, &key));
                    start.elapsed().as_secs_f64() * 1000.
                })
                .collect();
            times.sort_by(f64::total_cmp);
            eprintln!(
                "item {index}, {name}: median {:.4} ms, p90 {:.4} ms",
                times[100], times[180]
            );
        }
    }

    #[test]
    fn an_html_table_has_no_rules() {
        let source = "<table>\n<tr><td>a</td></tr>\n</table>";
        let cells = vec![vec![16..17]];
        assert!(Grid::new(source.to_string(), 0, &cells).is_none());
    }
}
