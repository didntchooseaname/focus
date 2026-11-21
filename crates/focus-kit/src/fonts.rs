use std::borrow::Cow;

use gpui::App;

const FILES: [&[u8]; 18] = [
    include_bytes!("../../../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../../../assets/fonts/Inter-Italic.ttf"),
    include_bytes!("../../../assets/fonts/Inter-SemiBoldItalic.ttf"),
    include_bytes!("../../../assets/fonts/Sora-Regular.ttf"),
    include_bytes!("../../../assets/fonts/Sora-SemiBold.ttf"),
    include_bytes!("../../../assets/fonts/Sora-Bold.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Medium.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-SemiBold.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../../../assets/fonts/NotoSansArabic-Regular.ttf"),
    include_bytes!("../../../assets/fonts/NotoSansArabic-Medium.ttf"),
    include_bytes!("../../../assets/fonts/NotoSansArabic-SemiBold.ttf"),
    include_bytes!("../../../assets/fonts/NotoEmoji-Regular.ttf"),
    include_bytes!("../../../assets/icons/lucide.ttf"),
];

pub fn text_faces() -> [&'static [u8]; 3] {
    [FILES[0], FILES[1], FILES[2]]
}

pub fn install(cx: &mut App) -> anyhow::Result<()> {
    cx.text_system()
        .add_fonts(FILES.iter().map(|bytes| Cow::Borrowed(*bytes)).collect())
}

#[cfg(test)]
#[path = "../../../vendor/gpui-0.2.2/src/platform/linux/font_weight.rs"]
mod font_weight;

#[cfg(test)]
mod fallback_tests {
    use super::font_weight::nearest_weight;

    const CJK: [u16; 7] = [100, 300, 350, 400, 500, 700, 900];
    const ARABIC: [u16; 3] = [400, 500, 600];

    #[test]
    fn a_semibold_title_falls_back_to_the_bold_cjk_face() {
        assert_eq!(nearest_weight(600, &CJK), Some(700));
        assert_eq!(nearest_weight(800, &CJK), Some(900));
        assert_eq!(nearest_weight(400, &CJK), Some(400));
        assert_eq!(nearest_weight(450, &CJK), Some(500));
        assert_eq!(nearest_weight(200, &CJK), Some(100));
    }

    #[test]
    fn arabic_beyond_its_faces_takes_the_nearest() {
        assert_eq!(nearest_weight(700, &ARABIC), Some(600));
        assert_eq!(nearest_weight(900, &ARABIC), Some(600));
        assert_eq!(nearest_weight(300, &ARABIC), Some(400));
        assert_eq!(nearest_weight(100, &ARABIC), Some(400));
        assert_eq!(nearest_weight(600, &[]), None);
    }
}
