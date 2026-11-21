use focus_i18n::Direction;
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

    pub fn align_ui<T: Styled>(self, element: T) -> T {
        if self.rtl {
            element.text_right()
        } else {
            element.text_left()
        }
    }
}
