use std::time::Duration;

use gpui::{App, Global, Hsla, Pixels, Rgba, px};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

impl Mode {
    pub fn is_dark(self) -> bool {
        self == Mode::Dark
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub mode: Mode,
    pub bg: Hsla,
    pub bg_2: Hsla,
    pub card: Hsla,
    pub card_2: Hsla,
    pub card_3: Hsla,
    pub line: Hsla,
    pub line_2: Hsla,
    pub hatch: Hsla,
    pub ink: Hsla,
    pub ink_body: Hsla,
    pub ink_2: Hsla,
    pub ink_3: Hsla,
    pub ink_4: Hsla,
    pub btn: Hsla,
    pub btn_ink: Hsla,
    pub btn_hover: Hsla,
    pub btn_press: Hsla,
    pub sel: Hsla,
    pub ok: Hsla,
    pub warn: Hsla,
    pub danger: Hsla,
    pub veil: Hsla,
    pub float_border: Hsla,
}

fn hex(packed: u32) -> Hsla {
    Rgba {
        r: ((packed >> 16) & 0xff) as f32 / 255.,
        g: ((packed >> 8) & 0xff) as f32 / 255.,
        b: (packed & 0xff) as f32 / 255.,
        a: 1.,
    }
    .into()
}

impl Tokens {
    pub fn of(mode: Mode) -> Tokens {
        match mode {
            Mode::Dark => Tokens {
                mode,
                bg: hex(0x070707),
                bg_2: hex(0x0c0c0d),
                card: hex(0x121213),
                card_2: hex(0x18181a),
                card_3: hex(0x202022),
                line: hex(0x232325),
                line_2: hex(0x303033),
                hatch: hex(0x1c1c1e),
                ink: hex(0xf5f5f6),
                ink_body: hex(0xd7d7db),
                ink_2: hex(0xb4b4ba),
                ink_3: hex(0x7d7d85),
                ink_4: hex(0x55555c),
                btn: hex(0xf5f5f6),
                btn_ink: hex(0x0a0a0a),
                btn_hover: hex(0xe4e4e7),
                btn_press: hex(0xd4d4d8),
                sel: hex(0xf5f5f6).opacity(0.16),
                ok: hex(0x4ade80),
                warn: hex(0xfbbf24),
                danger: hex(0xf87171),
                veil: hex(0x000000).opacity(0.55),
                float_border: hex(0x303033),
            },
            Mode::Light => Tokens {
                mode,
                bg: hex(0xffffff),
                bg_2: hex(0xf6f6f7),
                card: hex(0xffffff),
                card_2: hex(0xf4f4f5),
                card_3: hex(0xebebed),
                line: hex(0xe6e6e8),
                line_2: hex(0xd6d6da),
                hatch: hex(0xe4e4e7),
                ink: hex(0x0b0b0c),
                ink_body: hex(0x1f1f23),
                ink_2: hex(0x4b4b52),
                ink_3: hex(0x6b6b73),
                ink_4: hex(0xa1a1aa),
                btn: hex(0x0b0b0c),
                btn_ink: hex(0xffffff),
                btn_hover: hex(0x2a2a2e),
                btn_press: hex(0x3f3f46),
                sel: hex(0x0b0b0c).opacity(0.12),
                ok: hex(0x16a34a),
                warn: hex(0xb45309),
                danger: hex(0xdc2626),
                veil: hex(0x000000).opacity(0.55),
                float_border: hex(0xd6d6da),
            },
        }
    }
}

pub struct FocusTheme(pub Tokens);

impl Global for FocusTheme {}

pub trait ActiveTokens {
    fn tokens(&self) -> &Tokens;
}

impl ActiveTokens for App {
    fn tokens(&self) -> &Tokens {
        &self.global::<FocusTheme>().0
    }
}

pub mod size {
    use super::*;

    pub const S1: Pixels = px(4.);
    pub const S2: Pixels = px(8.);
    pub const S3: Pixels = px(12.);
    pub const S4: Pixels = px(16.);
    pub const S6: Pixels = px(24.);
    pub const S8: Pixels = px(32.);
    pub const S12: Pixels = px(48.);

    pub const RADIUS: Pixels = px(2.);

    pub const TITLE_BAR: Pixels = px(40.);
    pub const TAB_BAR: Pixels = px(36.);
    pub const SIDEBAR_HEADER: Pixels = px(36.);
    pub const ROW: Pixels = px(26.);
    pub const ROW_COMPACT: Pixels = px(22.);

    static COMPACT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    pub fn set_compact(compact: bool) {
        COMPACT.store(compact, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn list_row() -> Pixels {
        if COMPACT.load(std::sync::atomic::Ordering::Relaxed) {
            ROW_COMPACT
        } else {
            ROW
        }
    }
    pub const PALETTE_ROW: Pixels = px(34.);
    pub const CONTROL: Pixels = px(28.);
    pub const CONTROL_LG: Pixels = px(34.);
    pub const STATUS_BAR: Pixels = px(24.);

    pub const LEFT: Pixels = px(272.);
    pub const LEFT_MIN: Pixels = px(200.);
    pub const LEFT_MAX: Pixels = px(480.);
    pub const RIGHT: Pixels = px(320.);
    pub const RIGHT_MIN: Pixels = px(240.);
    pub const RIGHT_MAX: Pixels = px(560.);
    pub const BOTTOM: Pixels = px(220.);
    pub const BOTTOM_MIN: Pixels = px(120.);
    pub const HANDLE: Pixels = px(6.);

    pub const MEASURE: Pixels = px(760.);
    pub const SHEET_PAD: Pixels = px(48.);
    pub const SHEET_PAD_NARROW: Pixels = px(24.);

    pub const ICON: Pixels = px(16.);
    pub const ICON_DENSE: Pixels = px(14.);
    pub const ICON_SMALL: Pixels = px(12.);
    pub const ICON_BUTTON: Pixels = px(26.);
    pub const ICON_HIT: Pixels = px(28.);
    pub const INDENT: Pixels = px(14.);
    pub const DOT: Pixels = px(6.);
    pub const CHECKBOX: Pixels = px(14.);

    pub const TAB_MIN: Pixels = px(120.);
    pub const TAB_MAX: Pixels = px(220.);

    pub const PALETTE_W: Pixels = px(640.);
    pub const PALETTE_INPUT: Pixels = px(44.);
    pub const PALETTE_FOOT: Pixels = px(28.);
    pub const PALETTE_VISIBLE: usize = 8;

    pub const HATCH_PITCH: u32 = 12;
}

pub mod motion {
    use super::*;

    pub const FADE: Duration = Duration::from_millis(100);
    pub const TOAST: Duration = Duration::from_secs(4);
}

pub fn contrast(a: Hsla, b: Hsla) -> f32 {
    fn channel(c: f32) -> f32 {
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    fn luminance(color: Hsla) -> f32 {
        let rgba = color.to_rgb();
        0.2126 * channel(rgba.r) + 0.7152 * channel(rgba.g) + 0.0722 * channel(rgba.b)
    }
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::{Mode, Tokens, contrast, size};

    #[test]
    fn text_inks_reach_four_and_a_half() {
        for mode in [Mode::Dark, Mode::Light] {
            let t = Tokens::of(mode);
            let surfaces = [
                ("bg", t.bg),
                ("bg_2", t.bg_2),
                ("card", t.card),
                ("card_2", t.card_2),
                ("card_3", t.card_3),
            ];
            for (name, surface) in surfaces {
                for (ink_name, ink) in
                    [("ink", t.ink), ("ink_body", t.ink_body), ("ink_2", t.ink_2)]
                {
                    let ratio = contrast(ink, surface);
                    assert!(
                        ratio >= 4.5,
                        "{ink_name} on {name} ({mode:?}): {ratio:.2}:1"
                    );
                }
            }
            for (name, surface) in &surfaces[..3] {
                let ratio = contrast(t.ink_3, *surface);
                assert!(ratio >= 4.5, "ink_3 on {name} ({mode:?}): {ratio:.2}:1");
            }
            let on_button = contrast(t.btn_ink, t.btn);
            assert!(
                on_button >= 4.5,
                "btn_ink on btn ({mode:?}): {on_button:.2}:1"
            );
            let pressed = contrast(t.btn_ink, t.btn_press);
            assert!(
                pressed >= 4.5,
                "btn_ink on btn_press ({mode:?}): {pressed:.2}:1"
            );
        }
    }

    #[test]
    fn status_inks_are_readable() {
        for mode in [Mode::Dark, Mode::Light] {
            let t = Tokens::of(mode);
            for (name, surface) in [("bg", t.bg), ("card", t.card)] {
                for (ink_name, ink) in [("ok", t.ok), ("warn", t.warn), ("danger", t.danger)] {
                    let ratio = contrast(ink, surface);
                    assert!(
                        ratio >= 3.0,
                        "{ink_name} on {name} ({mode:?}): {ratio:.2}:1"
                    );
                }
            }
        }
    }

    #[test]
    fn the_list_row_follows_the_density() {
        assert_eq!(size::list_row(), size::ROW);
        size::set_compact(true);
        assert_eq!(size::list_row(), size::ROW_COMPACT);
        size::set_compact(false);
        assert_eq!(size::list_row(), size::ROW);
    }
}
