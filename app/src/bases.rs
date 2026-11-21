use std::path::{Path, PathBuf};
use std::sync::Arc;

use focus_view_reading::view::{ReadingEvent, ReadingView};
use gpui::{AppContext, Context, Entity, Subscription, Window};

use crate::shell::{Pane, Shell};

pub struct BasePage {
    pub id: u64,
    pub path: PathBuf,
    pub view: Entity<ReadingView>,
    _events: Subscription,
}

pub fn is_base(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("base"))
}

impl Shell {
    pub fn open_base(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let resolver: Arc<dyn focus_view_reading::Resolver> = library.vault().clone();
        if let Some(page) = self.bases.iter().find(|page| page.path == path) {
            let (id, view) = (page.id, page.view.clone());
            view.update(cx, |view, cx| {
                view.set_base_page(path.to_path_buf(), resolver, cx)
            });
            self.show_pane(Pane::Base(id), window, cx);
            return;
        }
        let (typesetting, sheet_width) = (self.typesetting(cx), self.sheet_width(cx));
        let view = cx.new(|cx| {
            let mut view = ReadingView::new(typesetting, cx);
            view.set_sheet_width(Some(sheet_width), cx);
            view.set_base_page(path.to_path_buf(), resolver, cx);
            view
        });
        let events = cx.subscribe_in(
            &view,
            window,
            |this, _, event: &ReadingEvent, window, cx| match event {
                ReadingEvent::OpenLink { goal, new_tab } => {
                    this.follow_link(goal.clone(), *new_tab, window, cx)
                }
                ReadingEvent::Zoom(source) => {
                    this.overlay = crate::shell::Overlay::Zoom(source.clone());
                    cx.notify();
                }
                ReadingEvent::Hover { goal, position } => {
                    let note = match goal {
                        Some(focus_view_reading::model::LinkGoal::Note { id, anchor }) => {
                            Some((*id, anchor.clone()))
                        }
                        _ => None,
                    };
                    this.show_link_preview(note, *position, cx);
                }
                ReadingEvent::ToggleTask(_) | ReadingEvent::StartWriting => {}
            },
        );
        self.next_base += 1;
        let id = self.next_base;
        self.bases.push(BasePage {
            id,
            path: path.to_path_buf(),
            view,
            _events: events,
        });
        self.show_pane(Pane::Base(id), window, cx);
    }

    pub fn base_page(&self, id: u64) -> Option<&BasePage> {
        self.bases.iter().find(|page| page.id == id)
    }

    pub fn active_base_view(&self) -> Option<Entity<ReadingView>> {
        match self.panes.get(self.active_pane?)? {
            Pane::Base(id) => self.base_page(*id).map(|page| page.view.clone()),
            _ => None,
        }
    }

    pub fn forget_base(&mut self, id: u64) {
        self.bases.retain(|page| page.id != id);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn base_files_are_known_by_their_extension() {
        assert!(super::is_base(std::path::Path::new("Suivi.base")));
        assert!(super::is_base(std::path::Path::new("Dossier/Projets.BASE")));
        assert!(!super::is_base(std::path::Path::new("Suivi.md")));
    }
}
