use focus_kit::{Tokens, size};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, Window, div};
use gpui_component::RopeExt as _;

use super::super::ReadingView;
use crate::model::Node;

pub(in crate::view) fn render(
    view: &ReadingView,
    item: usize,
    t: &Tokens,
    _window: &mut Window,
    cx: &mut Context<ReadingView>,
) -> Option<AnyElement> {
    let field = view.in_place.as_ref()?;
    let open = view.open.as_ref()?;
    let entry = view.built.as_ref()?.items.get(item)?;
    if open.lines.start >= entry.lines.end || entry.lines.start >= open.lines.end {
        return None;
    }
    let first = field
        .read(cx)
        .text()
        .slice_line(entry.lines.start as usize)
        .to_string();
    if !opens_above(&entry.node, &first) {
        return None;
    }
    Some(
        div()
            .pb(size::S2)
            .child(view.open_editor(t))
            .into_any_element(),
    )
}

pub(in crate::view) fn opens_above(node: &Node, first: &str) -> bool {
    match node {
        Node::Mermaid { .. }
        | Node::Math(_)
        | Node::Image(_)
        | Node::Embed { .. }
        | Node::Html(_)
        | Node::Base { .. }
        | Node::Query(_)
        | Node::Directive(_)
        | Node::Footnotes(_) => true,
        Node::Table { .. } | Node::Code { .. } => first.trim_start().starts_with('<'),
        _ => false,
    }
}
