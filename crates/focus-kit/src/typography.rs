use gpui::{FontWeight, Styled, px};

pub const SANS: &str = "Inter";
pub const DISPLAY: &str = "Sora";
pub const MONO: &str = "JetBrains Mono";

pub trait Type: Styled + Sized {
    fn ui_xs(self) -> Self {
        self.font_family(SANS)
            .text_size(px(11.5))
            .line_height(px(16.))
            .font_weight(FontWeight::MEDIUM)
    }
    fn ui_sm(self) -> Self {
        self.font_family(SANS)
            .text_size(px(12.5))
            .line_height(px(18.))
            .font_weight(FontWeight::NORMAL)
    }
    fn ui(self) -> Self {
        self.font_family(SANS)
            .text_size(px(13.))
            .line_height(px(20.))
            .font_weight(FontWeight::NORMAL)
    }
    fn ui_strong(self) -> Self {
        self.font_family(SANS)
            .text_size(px(13.))
            .line_height(px(20.))
            .font_weight(FontWeight::SEMIBOLD)
    }
    fn ui_lg(self) -> Self {
        self.font_family(SANS)
            .text_size(px(14.))
            .line_height(px(22.))
            .font_weight(FontWeight::MEDIUM)
    }
    fn mono_sm(self) -> Self {
        self.font_family(MONO)
            .text_size(px(11.5))
            .line_height(px(16.))
            .font_weight(FontWeight::NORMAL)
    }
    fn mono(self) -> Self {
        self.font_family(MONO)
            .text_size(px(12.5))
            .line_height(px(20.))
            .font_weight(FontWeight::NORMAL)
    }
    fn display_sm(self) -> Self {
        self.font_family(DISPLAY)
            .text_size(px(18.))
            .line_height(px(24.))
            .font_weight(FontWeight::SEMIBOLD)
    }
    fn display(self) -> Self {
        self.font_family(DISPLAY)
            .text_size(px(28.))
            .line_height(px(32.))
            .font_weight(FontWeight::SEMIBOLD)
    }
    fn stat(self) -> Self {
        self.font_family(DISPLAY)
            .text_size(px(30.))
            .line_height(px(32.))
            .font_weight(FontWeight::SEMIBOLD)
    }
    fn body(self, base: f32) -> Self {
        self.font_family(SANS)
            .text_size(px(base))
            .line_height(px((base * 1.65).round()))
            .font_weight(FontWeight::NORMAL)
    }
}

impl<T: Styled> Type for T {}
