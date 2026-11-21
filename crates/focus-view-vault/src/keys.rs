use std::time::{Duration, Instant};

use focus_kit::size;
use gpui::{
    App, Context, Div, InteractiveElement, KeyBinding, KeyDownEvent, ScrollStrategy, Stateful,
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

const TYPE_AHEAD_PAUSE: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(crate) struct TypeAhead {
    typed: String,
    last: Option<Instant>,
}

impl TypeAhead {
    pub(crate) fn typed_char(event: &KeyDownEvent) -> Option<&str> {
        let m = event.keystroke.modifiers;
        if m.control || m.alt || m.platform || m.function {
            return None;
        }
        event
            .keystroke
            .key_char
            .as_deref()
            .filter(|typed| !typed.is_empty() && !typed.chars().any(char::is_control))
    }

    pub(crate) fn seek<'a>(
        &mut self,
        typed: &str,
        now: Instant,
        names: impl Iterator<Item = (usize, &'a str)>,
        current: Option<usize>,
    ) -> Option<usize> {
        if self
            .last
            .is_none_or(|last| now.duration_since(last) > TYPE_AHEAD_PAUSE)
        {
            self.typed.clear();
        }
        self.last = Some(now);
        if self.typed.is_empty() && typed.trim().is_empty() {
            return None;
        }
        self.typed
            .extend(typed.chars().flat_map(char::to_lowercase));
        let names: Vec<(usize, String)> = names
            .map(|(row, name)| (row, name.chars().flat_map(char::to_lowercase).collect()))
            .collect();
        find_prefix(&names, current, &self.typed)
    }
}

fn find_prefix(names: &[(usize, String)], current: Option<usize>, typed: &str) -> Option<usize> {
    let mut chars = typed.chars();
    let first = chars.next()?;
    let repeated = chars.all(|c| c == first);
    let (prefix, skip_current) = if repeated {
        (&typed[..first.len_utf8()], true)
    } else {
        (typed, false)
    };
    let at = current.and_then(|row| names.iter().position(|(r, _)| *r == row));
    let start = match at {
        Some(at) if skip_current => at + 1,
        Some(at) => at,
        None => 0,
    };
    (0..names.len())
        .map(|offset| &names[(start + offset) % names.len()])
        .find(|(_, name)| name.starts_with(prefix))
        .map(|(row, _)| *row)
}

#[cfg(test)]
mod tests {
    use super::{Step, TypeAhead, find_prefix, step};

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

    #[test]
    fn typing_selects_the_next_row_whose_name_starts_with_the_typed_text() {
        let names: Vec<(usize, String)> = ["daily", "method", "media", "maps", "notes"]
            .iter()
            .enumerate()
            .map(|(row, name)| (row * 2, name.to_string()))
            .collect();
        assert_eq!(find_prefix(&names, None, "m"), Some(2));
        assert_eq!(find_prefix(&names, Some(2), "m"), Some(4));
        assert_eq!(find_prefix(&names, Some(6), "m"), Some(2));
        assert_eq!(find_prefix(&names, Some(2), "me"), Some(2));
        assert_eq!(find_prefix(&names, Some(2), "mmm"), Some(4));
        assert_eq!(find_prefix(&names, Some(4), "ma"), Some(6));
        assert_eq!(find_prefix(&names, Some(0), "x"), None);
        assert_eq!(find_prefix(&[], None, "m"), None);
    }

    #[test]
    fn the_typed_prefix_is_forgotten_after_a_pause() {
        let names = [(0, "Method"), (1, "Media"), (2, "Notes")];
        let start = std::time::Instant::now();
        let mut ahead = TypeAhead::default();
        assert_eq!(ahead.seek("m", start, names.into_iter(), None), Some(0));
        let soon = start + std::time::Duration::from_millis(300);
        assert_eq!(ahead.seek("e", soon, names.into_iter(), Some(0)), Some(0));
        let still = soon + std::time::Duration::from_millis(300);
        assert_eq!(ahead.seek("d", still, names.into_iter(), Some(0)), Some(1));
        let later = still + std::time::Duration::from_millis(1500);
        assert_eq!(ahead.seek("n", later, names.into_iter(), Some(1)), Some(2));
        assert_eq!(ahead.seek(" ", later, names.into_iter(), Some(2)), None);
    }
}
