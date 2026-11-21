use gpui::{Div, ParentElement, Pixels, Styled, div, relative};

pub const FAMILY: &str = "lucide";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Icon(pub &'static str);

impl Icon {
    pub const ALERT: Icon = Icon("\u{E193}");
    pub const ALERT_CIRCLE: Icon = Icon("\u{E077}");
    pub const ARROW_LEFT: Icon = Icon("\u{E048}");
    pub const ARROW_RIGHT: Icon = Icon("\u{E049}");
    pub const ARROW_UP_DOWN: Icon = Icon("\u{E37D}");
    pub const BOOK_OPEN: Icon = Icon("\u{E05F}");
    pub const BOOKMARK: Icon = Icon("\u{E060}");
    pub const CHECK: Icon = Icon("\u{E06C}");
    pub const CHEVRON_DOWN: Icon = Icon("\u{E06D}");
    pub const CHEVRON_LEFT: Icon = Icon("\u{E06E}");
    pub const CHEVRON_RIGHT: Icon = Icon("\u{E06F}");
    pub const CLOCK: Icon = Icon("\u{E251}");
    pub const CLOSE: Icon = Icon("\u{E1B2}");
    pub const COLUMNS: Icon = Icon("\u{E098}");
    pub const COMMAND: Icon = Icon("\u{E09A}");
    pub const COPY: Icon = Icon("\u{E09E}");
    pub const CORNER_DOWN_LEFT: Icon = Icon("\u{E0A1}");
    pub const CROSSHAIR: Icon = Icon("\u{E0AC}");
    pub const EXTERNAL: Icon = Icon("\u{E0B9}");
    pub const FILE: Icon = Icon("\u{E0C0}");
    pub const FILE_PLUS: Icon = Icon("\u{E0C9}");
    pub const FILE_SEARCH: Icon = Icon("\u{E0CB}");
    pub const FILE_TEXT: Icon = Icon("\u{E0CC}");
    pub const FOLDER: Icon = Icon("\u{E0D7}");
    pub const FOLDER_OPEN: Icon = Icon("\u{E247}");
    pub const FOLDER_PLUS: Icon = Icon("\u{E0D9}");
    pub const HASH: Icon = Icon("\u{E0EF}");
    pub const HISTORY: Icon = Icon("\u{E1F5}");
    pub const INFO: Icon = Icon("\u{E0F9}");
    pub const KEYBOARD: Icon = Icon("\u{E284}");
    pub const LINK: Icon = Icon("\u{E102}");
    pub const LIST_CHECKS: Icon = Icon("\u{E1D0}");
    pub const LIST_TREE: Icon = Icon("\u{E408}");
    pub const MORE: Icon = Icon("\u{E0B6}");
    pub const PANEL_BOTTOM: Icon = Icon("\u{E42C}");
    pub const PANEL_LEFT: Icon = Icon("\u{E12A}");
    pub const PANEL_RIGHT: Icon = Icon("\u{E431}");
    pub const PENCIL: Icon = Icon("\u{E1F9}");
    pub const PLUS: Icon = Icon("\u{E13D}");
    pub const RESET: Icon = Icon("\u{E148}");
    pub const SCROLL_TEXT: Icon = Icon("\u{E45F}");
    pub const SEARCH: Icon = Icon("\u{E151}");
    pub const SETTINGS: Icon = Icon("\u{E154}");
    pub const SLIDERS: Icon = Icon("\u{E29A}");
    pub const SPARKLES: Icon = Icon("\u{E412}");
    pub const STAR: Icon = Icon("\u{E176}");
    pub const TAG: Icon = Icon("\u{E17F}");
    pub const TERMINAL: Icon = Icon("\u{E181}");
    pub const TRASH: Icon = Icon("\u{E18E}");
    pub const UNLINK: Icon = Icon("\u{E19C}");
    pub const WAYPOINTS: Icon = Icon("\u{E542}");
    pub const AT_SIGN: Icon = Icon("\u{E04E}");
    pub const BOLD: Icon = Icon("\u{E05D}");
    pub const BRACES: Icon = Icon("\u{E36A}");
    pub const BRAIN: Icon = Icon("\u{E3C6}");
    pub const BUG: Icon = Icon("\u{E20C}");
    pub const CALENDAR: Icon = Icon("\u{E063}");
    pub const CHECK_CIRCLE: Icon = Icon("\u{E07C}");
    pub const CLIPBOARD_LIST: Icon = Icon("\u{E086}");
    pub const CODE: Icon = Icon("\u{E093}");
    pub const FILE_INPUT: Icon = Icon("\u{E0C5}");
    pub const FLAME: Icon = Icon("\u{E0D2}");
    pub const GIT_BRANCH: Icon = Icon("\u{E0E2}");
    pub const HEADING: Icon = Icon("\u{E384}");
    pub const HELP: Icon = Icon("\u{E082}");
    pub const HIGHLIGHTER: Icon = Icon("\u{E0F4}");
    pub const IMAGE: Icon = Icon("\u{E0F6}");
    pub const ITALIC: Icon = Icon("\u{E0FB}");
    pub const LIST: Icon = Icon("\u{E106}");
    pub const LIST_ORDERED: Icon = Icon("\u{E1D1}");
    pub const LIST_TODO: Icon = Icon("\u{E4C3}");
    pub const FIT_WIDTH: Icon = Icon("\u{E1C6}");
    pub const MAXIMIZE: Icon = Icon("\u{E113}");
    pub const MESSAGE: Icon = Icon("\u{E117}");
    pub const MILESTONE: Icon = Icon("\u{E298}");
    pub const MINUS: Icon = Icon("\u{E11C}");
    pub const NETWORK: Icon = Icon("\u{E125}");
    pub const PAPERCLIP: Icon = Icon("\u{E12D}");
    pub const QUOTE: Icon = Icon("\u{E239}");
    pub const SIGMA: Icon = Icon("\u{E201}");
    pub const STRIKETHROUGH: Icon = Icon("\u{E177}");
    pub const TABLE: Icon = Icon("\u{E17D}");
    pub const TEXT_QUOTE: Icon = Icon("\u{E49E}");
    pub const WORKFLOW: Icon = Icon("\u{E425}");
    pub const ZAP: Icon = Icon("\u{E1B4}");
    pub const WRENCH: Icon = Icon("\u{E1B1}");
    pub const SQUARE: Icon = Icon("\u{E167}");
    pub const ARROW_UP: Icon = Icon("\u{E04A}");
    pub const ROTATE_CW: Icon = Icon("\u{E149}");
    pub const SHIELD_CHECK: Icon = Icon("\u{E1FF}");
    pub const LOADER: Icon = Icon("\u{E109}");
    pub const GRIP_VERTICAL: Icon = Icon("\u{E0EB}");
    pub const FILE_OUTPUT: Icon = Icon("\u{E0C8}");
    pub const PILCROW: Icon = Icon("\u{E3A3}");
    pub const PIN: Icon = Icon("\u{E259}");
}

pub fn icon(icon: Icon, size: Pixels) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(size)
        .font_family(FAMILY)
        .text_size(size)
        .line_height(relative(1.))
        .child(icon.0)
}
