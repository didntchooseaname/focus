use focus_i18n::Direction;
use gpui::{Div, Hsla, Length, Styled, div};

use crate::icons::Icon;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dir {
    pub rtl: bool,
}

impl Dir {
    pub fn current() -> Dir {
        Dir {
            rtl: focus_i18n::direction() == Direction::Rtl,
        }
    }

    pub fn row(self) -> Div {
        self.flow(div())
    }

    pub fn flow<T: Styled>(self, element: T) -> T {
        if self.rtl {
            element.flex().flex_row_reverse()
        } else {
            element.flex().flex_row()
        }
    }

    pub fn start<T: Styled>(self, element: T, offset: impl Into<Length> + Clone) -> T {
        if self.rtl {
            element.right(offset)
        } else {
            element.left(offset)
        }
    }

    pub fn end<T: Styled>(self, element: T, offset: impl Into<Length> + Clone) -> T {
        if self.rtl {
            element.left(offset)
        } else {
            element.right(offset)
        }
    }

    pub fn pad_start<T: Styled>(
        self,
        element: T,
        amount: impl Into<gpui::DefiniteLength> + Clone,
    ) -> T {
        if self.rtl {
            element.pr(amount)
        } else {
            element.pl(amount)
        }
    }

    pub fn pad_end<T: Styled>(
        self,
        element: T,
        amount: impl Into<gpui::DefiniteLength> + Clone,
    ) -> T {
        if self.rtl {
            element.pl(amount)
        } else {
            element.pr(amount)
        }
    }

    pub fn margin_start<T: Styled>(self, element: T, amount: impl Into<Length> + Clone) -> T {
        if self.rtl {
            element.mr(amount)
        } else {
            element.ml(amount)
        }
    }

    pub fn margin_end<T: Styled>(self, element: T, amount: impl Into<Length> + Clone) -> T {
        if self.rtl {
            element.ml(amount)
        } else {
            element.mr(amount)
        }
    }

    pub fn border_start<T: Styled>(self, element: T, color: Hsla) -> T {
        let element = if self.rtl {
            element.border_r_1()
        } else {
            element.border_l_1()
        };
        element.border_color(color)
    }

    pub fn border_end<T: Styled>(self, element: T, color: Hsla) -> T {
        let element = if self.rtl {
            element.border_l_1()
        } else {
            element.border_r_1()
        };
        element.border_color(color)
    }

    pub fn bar_start<T: Styled>(self, element: T, color: Hsla) -> T {
        let element = if self.rtl {
            element.border_r_2()
        } else {
            element.border_l_2()
        };
        element.border_color(color)
    }

    pub fn align_ui<T: Styled>(self, element: T) -> T {
        if self.rtl {
            element.text_right()
        } else {
            element.text_left()
        }
    }

    pub fn align_end<T: Styled>(self, element: T) -> T {
        if self.rtl {
            element.text_left()
        } else {
            element.text_right()
        }
    }

    pub fn align_text<T: Styled>(self, element: T, text: &str) -> T {
        if self.of_text(text).rtl {
            element.text_right()
        } else {
            element.text_left()
        }
    }

    pub fn of_text(self, text: &str) -> Dir {
        let rtl = match focus_i18n::text_direction(text) {
            Direction::Rtl => true,
            Direction::Ltr => self.rtl && !text.chars().any(|c| c.is_alphabetic()),
        };
        Dir { rtl }
    }

    pub fn across<T: Styled>(element: T) -> T {
        element.left_0().right_0()
    }

    pub fn inset<T: Styled>(element: T, amount: gpui::Pixels) -> T {
        element
            .top(amount)
            .bottom(amount)
            .left(amount)
            .right(amount)
    }

    pub fn forward(self) -> Icon {
        if self.rtl {
            Icon::CHEVRON_LEFT
        } else {
            Icon::CHEVRON_RIGHT
        }
    }

    pub fn arrow_back(self) -> Icon {
        if self.rtl {
            Icon::ARROW_RIGHT
        } else {
            Icon::ARROW_LEFT
        }
    }

    pub fn panel_start(self) -> Icon {
        if self.rtl {
            Icon::PANEL_RIGHT
        } else {
            Icon::PANEL_LEFT
        }
    }

    pub fn panel_end(self) -> Icon {
        if self.rtl {
            Icon::PANEL_LEFT
        } else {
            Icon::PANEL_RIGHT
        }
    }

    pub fn arrow_forward(self) -> Icon {
        if self.rtl {
            Icon::ARROW_LEFT
        } else {
            Icon::ARROW_RIGHT
        }
    }

    pub fn toward_end(self, delta: gpui::Pixels) -> gpui::Pixels {
        if self.rtl { -delta } else { delta }
    }
}

pub trait Flow: Styled + Sized {
    fn flow_row(self) -> Self {
        Dir::current().flow(self)
    }
    fn pad_start(self, amount: impl Into<gpui::DefiniteLength> + Clone) -> Self {
        Dir::current().pad_start(self, amount)
    }
    fn pad_end(self, amount: impl Into<gpui::DefiniteLength> + Clone) -> Self {
        Dir::current().pad_end(self, amount)
    }
    fn margin_start(self, amount: impl Into<Length> + Clone) -> Self {
        Dir::current().margin_start(self, amount)
    }
    fn margin_end(self, amount: impl Into<Length> + Clone) -> Self {
        Dir::current().margin_end(self, amount)
    }
    fn start_at(self, offset: impl Into<Length> + Clone) -> Self {
        Dir::current().start(self, offset)
    }
    fn end_at(self, offset: impl Into<Length> + Clone) -> Self {
        Dir::current().end(self, offset)
    }
    fn border_start_1(self) -> Self {
        if Dir::current().rtl {
            self.border_r_1()
        } else {
            self.border_l_1()
        }
    }
    fn border_end_1(self) -> Self {
        if Dir::current().rtl {
            self.border_l_1()
        } else {
            self.border_r_1()
        }
    }
    fn border_start_2(self) -> Self {
        if Dir::current().rtl {
            self.border_r_2()
        } else {
            self.border_l_2()
        }
    }
    fn text_start(self) -> Self {
        Dir::current().align_ui(self)
    }
    fn text_end(self) -> Self {
        Dir::current().align_end(self)
    }
}

impl<T: Styled> Flow for T {}
