use focus_kit::Tokens;
use gpui::{Context, Div, Styled, Window, div};
use gpui_component::RopeExt as _;

use super::{FoldsChanged, ReadingEvent, ReadingView, Zone};
use crate::model::{EmbedBody, LinkGoal, Node, Rich};
use crate::srcmap;
use crate::text::TextKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Link {
        item: usize,
        key: TextKey,
        link: u16,
    },
    Task {
        item: usize,
        line: u32,
    },
    Fold(usize),
    Callout {
        item: usize,
        key: u32,
    },
    Details {
        item: usize,
        key: u32,
    },
    Copy {
        item: usize,
        ord: u32,
    },
    Diagram(usize),
    Image(usize),
    Tabs {
        item: usize,
        key: u32,
    },
}

impl Target {
    fn item(self) -> usize {
        match self {
            Target::Link { item, .. }
            | Target::Task { item, .. }
            | Target::Callout { item, .. }
            | Target::Details { item, .. }
            | Target::Copy { item, .. }
            | Target::Tabs { item, .. } => item,
            Target::Fold(item) | Target::Diagram(item) | Target::Image(item) => item,
        }
    }
}

pub(super) fn focus_ring(t: &Tokens) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .border_1()
        .border_color(t.ink)
}

pub(super) fn find_rich(node: &Node, ord: u32) -> Option<&Rich> {
    fn hit(rich: &Rich, ord: u32) -> Option<&Rich> {
        (rich.ord == ord).then_some(rich)
    }
    fn many(nodes: &[Node], ord: u32) -> Option<&Rich> {
        nodes.iter().find_map(|n| find_rich(n, ord))
    }
    match node {
        Node::Heading { rich, .. } | Node::Paragraph(rich) | Node::Ref(rich) | Node::Math(rich) => {
            hit(rich, ord)
        }
        Node::Code { code, .. } => hit(code, ord),
        Node::List { entries, .. } => entries.iter().find_map(|e| many(&e.children, ord)),
        Node::Quote(children) => many(children, ord),
        Node::Callout {
            title, children, ..
        } => hit(title, ord).or_else(|| many(children, ord)),
        Node::Details {
            summary, children, ..
        } => hit(summary, ord).or_else(|| many(children, ord)),
        Node::Table { head, rows, .. } => head
            .iter()
            .chain(rows.iter().flatten())
            .find_map(|rich| hit(rich, ord)),
        Node::Embed {
            body: EmbedBody::Content(nodes),
            ..
        } => many(nodes, ord),
        Node::Footnotes(notes) => notes.iter().find_map(|n| many(&n.blocks, ord)),
        _ => None,
    }
}

impl ReadingView {
    fn block_open(&self, item: usize, key: u32, default: bool) -> bool {
        self.callouts.get(&(item, key)).copied().unwrap_or(default)
    }

    fn collect(&self, node: &Node, item: usize, out: &mut Vec<Target>) {
        let key_item = item as u32 + 1;
        let links = |rich: &Rich, out: &mut Vec<Target>| {
            out.extend((0..rich.links.len()).map(|link| Target::Link {
                item,
                key: (key_item, rich.ord),
                link: link as u16,
            }))
        };
        match node {
            Node::Heading { rich, .. } | Node::Paragraph(rich) | Node::Ref(rich) => {
                links(rich, out)
            }
            Node::List { entries, .. } => {
                for entry in entries {
                    if entry.task.is_some() {
                        out.push(Target::Task {
                            item,
                            line: entry.line,
                        });
                    }
                    for child in &entry.children {
                        self.collect(child, item, out);
                    }
                }
            }
            Node::Code { code, .. } => out.push(Target::Copy {
                item,
                ord: code.ord,
            }),
            Node::Mermaid { .. } => out.push(Target::Diagram(item)),
            Node::Image(_) => out.push(Target::Image(item)),
            Node::Quote(children) => {
                for child in children {
                    self.collect(child, item, out);
                }
            }
            Node::Callout {
                key,
                title,
                fold,
                children,
                ..
            } => {
                if fold.is_some() {
                    out.push(Target::Callout { item, key: *key });
                }
                links(title, out);
                if self.block_open(item, *key, fold.unwrap_or(true)) {
                    for child in children {
                        self.collect(child, item, out);
                    }
                }
            }
            Node::Details {
                key,
                summary,
                open,
                children,
                ..
            } => {
                out.push(Target::Details { item, key: *key });
                links(summary, out);
                if self.block_open(item, *key, *open) {
                    for child in children {
                        self.collect(child, item, out);
                    }
                }
            }
            Node::Tabs { key, .. } => out.push(Target::Tabs { item, key: *key }),
            Node::Table { head, rows, .. } => {
                for rich in head.iter().chain(rows.iter().flatten()) {
                    links(rich, out);
                }
            }
            Node::Embed {
                body: EmbedBody::Content(nodes),
                ..
            } => {
                for child in nodes {
                    self.collect(child, item, out);
                }
            }
            Node::Footnotes(notes) => {
                for child in notes.iter().flat_map(|n| n.blocks.iter()) {
                    self.collect(child, item, out);
                }
            }
            _ => {}
        }
    }

    pub(super) fn targets(&self) -> Vec<Target> {
        let Some(built) = &self.built else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &item in &self.visible {
            let Some(entry) = built.items.get(item) else {
                continue;
            };
            if matches!(entry.node, Node::Heading { .. })
                && built.headings.iter().any(|h| !h.inner && h.item == item)
            {
                out.push(Target::Fold(item));
            }
            self.collect(&entry.node, item, &mut out);
        }
        out
    }

    pub(super) fn focus_step(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let targets = self.targets();
        if targets.is_empty() {
            return cx.propagate();
        }
        let at = self
            .focus_target
            .and_then(|target| targets.iter().position(|t| *t == target));
        let next = match (at, forward) {
            (None, true) => Some(0),
            (None, false) => Some(targets.len() - 1),
            (Some(at), true) => (at + 1 < targets.len()).then_some(at + 1),
            (Some(at), false) => at.checked_sub(1),
        };
        let Some(next) = next else {
            self.focus_target = None;
            window.focus(&self.focus);
            cx.notify();
            return cx.propagate();
        };
        let target = targets[next];
        self.focus_target = Some(target);
        if let Some(row) = self.row_of(target.item()) {
            self.list.scroll_to_reveal_item(row);
        }
        match target {
            Target::Tabs { item, key } => {
                if let Some(handle) = self.tab_focus.borrow().get(&(item, key)) {
                    window.focus(handle);
                }
            }
            _ => window.focus(&self.focus),
        }
        cx.notify();
    }

    pub(super) fn leave_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.focus_target.take().is_none() {
            return false;
        }
        window.focus(&self.focus);
        cx.notify();
        true
    }

    pub(super) fn toggle_block(
        &mut self,
        item: usize,
        key: u32,
        default: bool,
        cx: &mut Context<Self>,
    ) {
        let open = self.block_open(item, key, default);
        self.callouts.insert((item, key), !open);
        if let Some(row) = self.row_of(item) {
            self.list.splice(row..row + 1, 1);
        }
        cx.emit(FoldsChanged);
        cx.notify();
    }

    pub(super) fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.focus_target else {
            return cx.propagate();
        };
        let Some(built) = self.built.clone() else {
            return;
        };
        let Some(node) = built.items.get(target.item()).map(|i| &i.node) else {
            return;
        };
        match target {
            Target::Link { item, key, link } => {
                let goal = find_rich(node, key.1)
                    .and_then(|rich| rich.links.get(link as usize))
                    .map(|l| l.goal.clone());
                if let Some(goal) = goal {
                    self.follow(Some(item), goal, false, cx);
                }
            }
            Target::Task { line, .. } => cx.emit(ReadingEvent::ToggleTask(line)),
            Target::Fold(item) => self.toggle_section(item, cx),
            Target::Callout { item, key } => {
                let fold = callout_fold(node, key).unwrap_or(true);
                self.toggle_block(item, key, fold, cx);
            }
            Target::Details { item, key } => {
                let open = details_open(node, key).unwrap_or(false);
                self.toggle_block(item, key, open, cx);
            }
            Target::Copy { item, ord } => {
                if let Node::Code { lang, code, .. } = node
                    && code.ord == ord
                {
                    let text = self
                        .decorated(item, lang, code)
                        .map_or_else(|| code.text.clone(), |d| d.shown(true).text.clone());
                    let which = ((item << 16) | (ord as usize) << 8) << 2;
                    self.copy(which, text, cx);
                }
            }
            Target::Diagram(_) => {
                if let Node::Mermaid { source } = node {
                    cx.emit(ReadingEvent::Zoom(source.clone()));
                }
            }
            Target::Image(item) => {
                let row = self.row_of(item).and_then(|r| self.list.bounds_for_item(r));
                let zone = self
                    .zones
                    .borrow()
                    .iter()
                    .find_map(|(bounds, zone)| match zone {
                        Zone::Image {
                            image,
                            width,
                            height,
                        } if row.is_some_and(|r| r.intersects(bounds)) => {
                            Some((image.clone(), *width, *height))
                        }
                        _ => None,
                    });
                if let Some((image, width, height)) = zone {
                    self.open_viewer(image, width, height, window, cx);
                }
            }
            Target::Tabs { .. } => {}
        }
    }

    pub(super) fn caret_line(&self) -> Option<(u32, usize)> {
        let frame = self.in_place_frame.as_ref()?;
        if frame.anchor != frame.head {
            return None;
        }
        Some((
            frame.text.offset_to_point(frame.head).row as u32,
            frame.head,
        ))
    }

    pub(super) fn follow_at_caret(&mut self, cx: &mut Context<Self>) {
        let (Some((_, head)), Some(built)) = (self.caret_line(), self.built.clone()) else {
            return cx.propagate();
        };
        let Some(text) = self.in_place_frame.as_ref().map(|f| f.text.clone()) else {
            return;
        };
        let key_item = self.key_item_at(head, &text);
        let Some(item) = (key_item as usize).checked_sub(1) else {
            return cx.propagate();
        };
        let Some(origin) = self.origin(key_item, &text) else {
            return;
        };
        let relative = head.saturating_sub(origin);
        let mut targets = Vec::new();
        if let Some(entry) = built.items.get(item) {
            self.collect(&entry.node, item, &mut targets);
        }
        for target in targets {
            let Target::Link { key, link, .. } = target else {
                continue;
            };
            let Some(rich) = built
                .items
                .get(item)
                .and_then(|i| find_rich(&i.node, key.1))
            else {
                continue;
            };
            let (Some(span), Some(map)) = (rich.links.get(link as usize), srcmap::of(rich)) else {
                continue;
            };
            let shown = map.to_shown(relative);
            let inside = shown.is_some_and(|at| span.range.start <= at && at <= span.range.end)
                || map.holding(relative).any(|m| {
                    m.hides.as_ref() == Some(&span.range)
                        || m.inserts.iter().any(|(at, _)| *at == span.range.start)
                });
            if inside {
                let goal = span.goal.clone();
                return self.follow(Some(item), goal, false, cx);
            }
        }
        cx.propagate();
    }

    pub(super) fn toggle_at_caret(&mut self, cx: &mut Context<Self>) {
        let (Some((line, _)), Some(built)) = (self.caret_line(), self.built.clone()) else {
            return cx.propagate();
        };
        let Some((item, entry)) = built
            .items
            .iter()
            .enumerate()
            .find(|(_, i)| i.lines.contains(&line))
        else {
            return cx.propagate();
        };
        match &entry.node {
            Node::Heading { .. } => self.toggle_section(item, cx),
            Node::Callout { key, fold, .. } if line == entry.lines.start => {
                self.toggle_block(item, *key, fold.unwrap_or(true), cx)
            }
            Node::Details { key, open, .. } if line == entry.lines.start => {
                self.toggle_block(item, *key, *open, cx)
            }
            Node::List { entries, .. } => match entries.iter().find(|e| e.line == line) {
                Some(e) if e.task.is_some() => cx.emit(ReadingEvent::ToggleTask(line)),
                _ => cx.propagate(),
            },
            _ => cx.propagate(),
        }
    }

    pub(super) fn follow(
        &mut self,
        item: Option<usize>,
        goal: LinkGoal,
        new_tab: bool,
        cx: &mut Context<Self>,
    ) {
        match goal {
            LinkGoal::Footnote(_) => self.follow_footnote(item, cx),
            LinkGoal::Note {
                id,
                anchor: Some(anchor),
            } if !new_tab
                && let Some(line) = self
                    .built
                    .as_ref()
                    .filter(|b| b.note == Some(id))
                    .and_then(|b| {
                        b.headings
                            .iter()
                            .find(|h| crate::model::heading_matches(&h.text, &anchor))
                    })
                    .map(|h| h.line) =>
            {
                self.scroll_to_line(line)
            }
            goal => cx.emit(ReadingEvent::OpenLink { goal, new_tab }),
        }
    }
}

fn callout_fold(node: &Node, key: u32) -> Option<bool> {
    let many = |nodes: &[Node]| nodes.iter().find_map(|n| callout_fold(n, key));
    match node {
        Node::Callout {
            key: k,
            fold,
            children,
            ..
        } => (*k == key)
            .then(|| fold.unwrap_or(true))
            .or_else(|| many(children)),
        Node::Quote(children) | Node::Details { children, .. } => many(children),
        Node::List { entries, .. } => entries.iter().find_map(|e| many(&e.children)),
        _ => None,
    }
}

fn details_open(node: &Node, key: u32) -> Option<bool> {
    let many = |nodes: &[Node]| nodes.iter().find_map(|n| details_open(n, key));
    match node {
        Node::Details {
            key: k,
            open,
            children,
            ..
        } => (*k == key).then_some(*open).or_else(|| many(children)),
        Node::Quote(children) | Node::Callout { children, .. } => many(children),
        Node::List { entries, .. } => entries.iter().find_map(|e| many(&e.children)),
        _ => None,
    }
}
