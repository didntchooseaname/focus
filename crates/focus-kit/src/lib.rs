pub mod button;
pub mod cards;
pub mod checkbox;
pub mod chip;
pub mod clipboard;
pub mod dir;
pub mod disclosure;
pub mod ellipsis;
pub mod fade;
pub mod field;
pub mod field_menu;
pub mod file_list;
pub mod fonts;
pub mod hatch;
pub mod icons;
pub mod interaction;
pub mod kbd;
pub mod keyboard;
pub mod menu;
pub mod note_tab;
pub mod scrollbar;
pub mod segments;
pub mod theme;
pub mod toast;
pub mod tokens;
pub mod tooltip;
pub mod tree_row;
pub mod typography;
#[cfg(all(unix, not(target_os = "macos")))]
mod x11_selection;
pub mod zoom;

pub use button::{Button, ButtonKind, IconButton};
pub use cards::{DiffCard, DiffMark, ErrorCard, diff_marker, diff_row};
pub use checkbox::Checkbox;
pub use chip::{Chip, ContextChip};
pub use dir::{Dir, Flow};
pub use disclosure::Disclosure;
pub use ellipsis::ellipsis;
pub use fade::fade_in;
pub use field::{Field, TextArea};
pub use hatch::hatch;
pub use icons::{Icon, icon};
pub use interaction::Forced;
pub use kbd::Kbd;
pub use keyboard::{contain_tab, cycle_focus};
pub use menu::{
    KeyboardMenu, MENU_CONTEXT, MenuConfirm, MenuCursor, MenuDismiss, MenuEntry, MenuFirst,
    MenuItem, MenuLast, MenuMove, MenuNext, MenuPrevious, MenuRow, MenuScroll, drawn_menu,
    floating, focus_menu, keyboard_menu, menu, menu_rows, menu_separator,
};
pub use note_tab::NoteTab;
pub use scrollbar::{ScrollSource, Scrollbar, horizontal_scrollbar, scrollbar};
pub use segments::{Segment, Segments};
pub use toast::{Toast, Tone, dot};
pub use tokens::{ActiveTokens, Mode, Tokens, contrast, motion, size};
pub use tooltip::{Tooltip, tooltip_body, truncated};
pub use tree_row::TreeRow;
pub use typography::Type;
pub use zoom::ZoomControls;

pub fn init(mode: Mode, cx: &mut gpui::App) -> anyhow::Result<()> {
    fonts::install(cx)?;
    keyboard::bind(cx);
    menu::bind(cx);
    field_menu::install(cx);
    theme::register_markdown();
    theme::apply(mode, None, cx)
}
