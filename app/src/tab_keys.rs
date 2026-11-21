use std::path::PathBuf;

use gpui::{Context, Window};

use crate::shell::{Pane, Shell};

const REOPEN_DEPTH: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Closed {
    Note(PathBuf),
    Loose(PathBuf),
    File(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabRef {
    Note(usize),
    Pane(usize),
}

pub fn remember(stack: &mut Vec<Closed>, closed: Closed) {
    stack.retain(|c| *c != closed);
    stack.push(closed);
    let excess = stack.len().saturating_sub(REOPEN_DEPTH);
    stack.drain(..excess);
}

fn rank_of(count: usize, front: Option<usize>, delta: isize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let from = front.unwrap_or(0) as isize;
    Some((from + delta).rem_euclid(count as isize) as usize)
}

impl Shell {
    fn tab_order(&self) -> Vec<TabRef> {
        (0..self.tabs.len())
            .map(TabRef::Note)
            .chain((0..self.panes.len()).map(TabRef::Pane))
            .collect()
    }

    fn front_tab(&self) -> Option<TabRef> {
        self.active_pane
            .map(TabRef::Pane)
            .or(self.active.map(TabRef::Note))
    }

    fn show_tab(&mut self, tab: TabRef, window: &mut Window, cx: &mut Context<Self>) {
        match tab {
            TabRef::Note(index) => self.activate(index, window, cx),
            TabRef::Pane(index) => {
                if let Some(pane) = self.panes.get(index).copied() {
                    self.show_pane(pane, window, cx);
                }
            }
        }
    }

    pub fn step_tab(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let order = self.tab_order();
        let front = self
            .front_tab()
            .and_then(|tab| order.iter().position(|t| *t == tab));
        if let Some(rank) = rank_of(order.len(), front, delta) {
            self.show_tab(order[rank], window, cx);
        }
    }

    pub fn go_to_tab(&mut self, rank: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let order = self.tab_order();
        let tab = match rank {
            Some(rank) => order.get(rank),
            None => order.last(),
        };
        if let Some(tab) = tab.copied() {
            self.show_tab(tab, window, cx);
        }
    }

    pub fn close_front_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.front_tab() {
            Some(TabRef::Note(index)) => self.close_tab(index, window, cx),
            Some(TabRef::Pane(index)) => self.close_pane(index, window, cx),
            None => {}
        }
    }

    pub fn reopen_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Some(closed) = self.closed.pop() {
            match closed {
                Closed::Note(relative) => {
                    let Some(library) = self.library.clone() else {
                        continue;
                    };
                    if !library.root().join(&relative).is_file() {
                        continue;
                    }
                    if let Some(id) = library.note_by_path(&relative) {
                        self.open_note(id, true, window, cx);
                        return;
                    }
                }
                Closed::Loose(path) => {
                    if path.is_file() {
                        self.open_loose(path, window, cx);
                        return;
                    }
                }
                Closed::File(path) => {
                    if path.is_file() {
                        self.open_media(&path, None, window, cx);
                        return;
                    }
                }
            }
        }
    }

    pub fn closed_tab(&self, index: usize, cx: &gpui::App) -> Option<Closed> {
        let editor = self.tabs.get(index)?.read(cx);
        Some(match &editor.loose {
            Some(path) => Closed::Loose(path.clone()),
            None => Closed::Note(editor.meta.path.to_path_buf()),
        })
    }

    pub fn closed_pane(&self, index: usize) -> Option<Closed> {
        match self.panes.get(index)? {
            Pane::File(id) => Some(Closed::File(self.files.get(*id)?.path.clone())),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_wrap_around_in_both_directions() {
        assert_eq!(rank_of(3, Some(2), 1), Some(0));
        assert_eq!(rank_of(3, Some(0), -1), Some(2));
        assert_eq!(rank_of(3, None, 1), Some(1));
        assert_eq!(rank_of(0, None, 1), None);
    }

    #[test]
    fn the_reopen_stack_is_bounded_and_keeps_one_entry_per_tab() {
        let mut stack = Vec::new();
        for n in 0..30 {
            remember(&mut stack, Closed::Note(PathBuf::from(format!("{n}.md"))));
        }
        assert_eq!(stack.len(), REOPEN_DEPTH);
        assert_eq!(stack.last(), Some(&Closed::Note(PathBuf::from("29.md"))));
        remember(&mut stack, Closed::Note(PathBuf::from("15.md")));
        assert_eq!(stack.len(), REOPEN_DEPTH);
        assert_eq!(stack.last(), Some(&Closed::Note(PathBuf::from("15.md"))));
        assert_eq!(
            stack
                .iter()
                .filter(|c| **c == Closed::Note(PathBuf::from("15.md")))
                .count(),
            1
        );
    }
}
