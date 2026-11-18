mod bookmarks;
mod broken;
mod errors;
mod groups;
mod keys;
mod names;
mod notes;
mod tags;
mod tasks;
mod tree;

#[cfg(test)]
#[path = "../../../vendor/gpui-0.2.2/src/platform/linux/wayland/drag_out.rs"]
mod gpui_drag_out;

use std::path::PathBuf;

use focus_core::NoteId;

pub use bookmarks::{Bookmarks, RECENT_LIMIT};
pub use broken::{BrokenIndex, BrokenLinks};
pub use errors::{Message, MessageArg, error_message};
pub use groups::{Group, Grouped, Rows};
pub use names::check_name;
pub use notes::{
    CaptureTo, Date, Time, append_line, capture, capture_line, daily_note, format_date,
    insert_template, merge_frontmatter, note_from_template, note_from_text, templates,
};
pub use tags::{TagIndex, TagTree};
pub use tasks::{TaskFilter, TaskIndex, TaskList};
pub use tree::{FileTree, ImportStatus, TreeSettings, visible_rows};

#[derive(Clone, Debug, PartialEq)]
pub enum VaultViewEvent {
    OpenNote {
        note: NoteId,
        line: Option<u32>,
        new_tab: bool,
    },
    OpenFile {
        path: PathBuf,
        new_tab: bool,
    },
    OpenWithSystem(PathBuf),
    ShowVersions(NoteId),
    SearchTag(String),
    RenameTag(String),
    MergeNote(NoteId),
    Notify {
        message: Message,
        tone: Tone,
    },
    BookmarksChanged(Vec<PathBuf>),
    RecentChanged(Vec<PathBuf>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Danger,
}

pub fn bind(cx: &mut gpui::App) {
    tree::bind(cx);
    bookmarks::bind(cx);
    keys::bind(cx);
}
