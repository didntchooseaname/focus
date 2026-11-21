use focus_kit::size;
use gpui::{
    App, Context, Div, InteractiveElement, KeyBinding, ScrollStrategy, Stateful,
    UniformListScrollHandle, actions,
};

use crate::tree::{OpenSelected, SelectNext, SelectPrevious};

pub(crate) trait Keyed: Sized + 'static {
    fn selection(&mut self) -> (usize, &mut Option<usize>, &UniformListScrollHandle);
    fn activate(&mut self, row: usize, cx: &mut Context<Self>);
}

pub(crate) fn keyed<V: Keyed>(element: Stateful<Div>, cx: &mut Context<V>) -> Stateful<Div> {
    element
        .key_context(CONTEXT)
        .on_action(
            cx.listener(|this: &mut V, _: &SelectPrevious, _, cx| select(this, Step::Previous, cx)),
        )
        .on_action(cx.listener(|this: &mut V, _: &SelectNext, _, cx| select(this, Step::Next, cx)))
        .on_action(
            cx.listener(|this: &mut V, _: &PagePrevious, _, cx| {
                select(this, Step::PagePrevious, cx)
            }),
        )
        .on_action(
            cx.listener(|this: &mut V, _: &PageNext, _, cx| select(this, Step::PageNext, cx)),
        )
        .on_action(
            cx.listener(|this: &mut V, _: &SelectFirst, _, cx| select(this, Step::First, cx)),
        )
        .on_action(cx.listener(|this: &mut V, _: &SelectLast, _, cx| select(this, Step::Last, cx)))
        .on_action(cx.listener(|this: &mut V, _: &OpenSelected, _, cx| {
            if let Some(row) = *this.selection().1 {
                this.activate(row, cx);
            }
        }))
}

fn select<V: Keyed>(view: &mut V, by: Step, cx: &mut Context<V>) {
    let (len, selected, scroll) = view.selection();
    let rows: Vec<usize> = (0..len).collect();
    if let Some(row) = step(&rows, selected.filter(|s| *s < len), page(scroll), by) {
        *selected = Some(row);
        scroll.scroll_to_item(row, ScrollStrategy::Center);
        cx.notify();
    }
}

pub(crate) const CONTEXT: &str = "FocusVaultList";

actions!(
    focus_vault_list,
    [PagePrevious, PageNext, SelectFirst, SelectLast]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Previous,
    Next,
    PagePrevious,
    PageNext,
    First,
    Last,
}

pub(crate) fn bind(cx: &mut App) {
    let list = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", SelectPrevious, list),
        KeyBinding::new("down", SelectNext, list),
        KeyBinding::new("enter", OpenSelected, list),
    ]);
    for context in [CONTEXT, crate::tree::CONTEXT, crate::bookmarks::CONTEXT] {
        let c = Some(context);
        cx.bind_keys([
            KeyBinding::new("pageup", PagePrevious, c),
            KeyBinding::new("pagedown", PageNext, c),
            KeyBinding::new("home", SelectFirst, c),
            KeyBinding::new("end", SelectLast, c),
        ]);
    }
}

pub(crate) fn page(scroll: &UniformListScrollHandle) -> usize {
    let height = scroll.0.borrow().base_handle.bounds().size.height;
    ((height / size::list_row()).floor() as usize)
        .saturating_sub(1)
        .max(1)
}

pub(crate) fn step(
    rows: &[usize],
    selected: Option<usize>,
    page: usize,
    step: Step,
) -> Option<usize> {
    let last = rows.len().checked_sub(1)?;
    let at = selected.and_then(|s| rows.iter().position(|r| *r == s));
    let next = match (step, at) {
        (Step::First, _) | (Step::Next | Step::PageNext, None) => 0,
        (Step::Last, _) | (Step::Previous | Step::PagePrevious, None) => last,
        (Step::Previous, Some(at)) => at.saturating_sub(1),
        (Step::Next, Some(at)) => (at + 1).min(last),
        (Step::PagePrevious, Some(at)) => at.saturating_sub(page),
        (Step::PageNext, Some(at)) => (at + page).min(last),
    };
    Some(rows[next])
}

#[cfg(test)]
mod tests {
    use super::{Step, step};

    #[test]
    fn page_keys_move_by_a_page_and_stop_at_the_ends() {
        let rows: Vec<usize> = (0..30).collect();
        assert_eq!(step(&rows, Some(3), 10, Step::PageNext), Some(13));
        assert_eq!(step(&rows, Some(25), 10, Step::PageNext), Some(29));
        assert_eq!(step(&rows, Some(5), 10, Step::PagePrevious), Some(0));
        assert_eq!(step(&rows, Some(5), 10, Step::Last), Some(29));
        assert_eq!(step(&rows, None, 10, Step::Next), Some(0));
        assert_eq!(step(&rows, None, 10, Step::Previous), Some(29));
        assert_eq!(step(&[], None, 10, Step::First), None);
        assert_eq!(step(&[1, 2, 4, 5], Some(2), 1, Step::Next), Some(4));
    }
}
