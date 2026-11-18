use crate::Direction;

pub fn text_direction(text: &str) -> Direction {
    for ch in text.chars() {
        if is_rtl_letter(ch) {
            return Direction::Rtl;
        }
        if ch.is_alphabetic() {
            return Direction::Ltr;
        }
    }
    Direction::Ltr
}

fn is_rtl_letter(ch: char) -> bool {
    matches!(ch as u32,
        0x0590..=0x08FF
        | 0xFB1D..=0xFDFF
        | 0xFE70..=0xFEFF
        | 0x10800..=0x10FFF
        | 0x1E800..=0x1EFFF)
        && ch.is_alphabetic()
}
