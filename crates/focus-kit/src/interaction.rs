use gpui::{Div, Hsla, Styled, div, px};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Forced {
    #[default]
    Live,
    Hover,
    Pressed,
    Focus,
}

impl Forced {
    pub fn hover(self) -> bool {
        self == Forced::Hover
    }
    pub fn pressed(self) -> bool {
        self == Forced::Pressed
    }
    pub fn focus(self) -> bool {
        self == Forced::Focus
    }
}

pub fn inset_ring(color: Hsla) -> Div {
    crate::Dir::inset(div().absolute(), px(2.))
        .border_1()
        .border_color(color)
}
