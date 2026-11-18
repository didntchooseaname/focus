use focus_i18n::Direction;
use focus_kit::Icon;
use gpui::{Div, Styled, div};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Dir {
    pub rtl: bool,
}

impl Dir {
    pub fn current() -> Dir {
        Dir {
            rtl: focus_i18n::direction() == Direction::Rtl,
        }
    }

    pub fn row(self) -> Div {
        if self.rtl {
            div().flex().flex_row_reverse()
        } else {
            div().flex().flex_row()
        }
    }

    pub fn align<T: Styled>(self, element: T, text: &str) -> T {
        let rtl = match focus_i18n::text_direction(text) {
            Direction::Rtl => true,
            Direction::Ltr => self.rtl && !text.chars().any(|c| c.is_alphabetic()),
        };
        if rtl {
            element.text_right()
        } else {
            element.text_left()
        }
    }

    pub fn at_start(self, element: Div, offset: gpui::Pixels) -> Div {
        if self.rtl {
            element.right(offset)
        } else {
            element.left(offset)
        }
    }

    pub fn border_start(self, element: Div, color: gpui::Hsla) -> Div {
        let element = if self.rtl {
            element.border_r_2()
        } else {
            element.border_l_2()
        };
        element.border_color(color)
    }

    pub fn pad_start(self, element: Div, amount: gpui::Pixels) -> Div {
        if self.rtl {
            element.pr(amount)
        } else {
            element.pl(amount)
        }
    }

    pub fn forward(self) -> Icon {
        if self.rtl {
            Icon::CHEVRON_LEFT
        } else {
            Icon::CHEVRON_RIGHT
        }
    }
}
