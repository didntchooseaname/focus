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
