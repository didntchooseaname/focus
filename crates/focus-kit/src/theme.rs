use std::sync::Arc;

use gpui::{App, Hsla, Rgba, Window, px};
use gpui_component::highlighter::{HighlightTheme, LanguageConfig, LanguageRegistry};
use gpui_component::{Theme, ThemeMode};

use crate::tokens::{FocusTheme, Mode, Tokens};
use crate::typography::{MONO, SANS};

pub const MARKDOWN: &str = "markdown";
const MARKDOWN_INLINE: &str = "markdown_inline";

pub(crate) fn register_markdown() {
    let registry = LanguageRegistry::singleton();
    registry.register(
        MARKDOWN,
        &LanguageConfig::new(
            MARKDOWN,
            tree_sitter_md::LANGUAGE.into(),
            vec![MARKDOWN_INLINE.into()],
            include_str!("queries/markdown.scm"),
            include_str!("queries/markdown_injections.scm"),
            "",
        ),
    );
    registry.register(
        MARKDOWN_INLINE,
        &LanguageConfig::new(
            MARKDOWN_INLINE,
            tree_sitter_md::INLINE_LANGUAGE.into(),
            vec![],
            include_str!("queries/markdown_inline.scm"),
            "",
            "",
        ),
    );
}

fn css(color: Hsla) -> String {
    let Rgba { r, g, b, a } = color.to_rgb();
    let byte = |v: f32| (v * 255.).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        byte(r),
        byte(g),
        byte(b),
        byte(a)
    )
}

fn highlight_theme(t: &Tokens) -> anyhow::Result<HighlightTheme> {
    let style = |color: Hsla, weight: Option<u16>, italic: bool| {
        let mut entry = serde_json::json!({ "color": css(color) });
        if let Some(weight) = weight {
            entry["font_weight"] = weight.into();
        }
        if italic {
            entry["font_style"] = "italic".into();
        }
        entry
    };
    let value = serde_json::json!({
        "name": if t.mode.is_dark() { "Focus dark" } else { "Focus light" },
        "appearance": if t.mode.is_dark() { "dark" } else { "light" },
        "style": {
            "editor.background": css(t.bg),
            "editor.foreground": css(t.ink_body),
            "editor.active_line.background": css(t.bg),
            "editor.line_number": css(t.ink_4),
            "editor.active_line_number": css(t.ink_3),
            "syntax": {
                "primary": style(t.ink_body, None, false),
                "title": style(t.ink, Some(600), false),
                "punctuation.special": style(t.ink_3, None, false),
                "punctuation.delimiter": style(t.ink_3, None, false),
                "punctuation.list_marker": style(t.ink_3, None, false),
                "text.literal": style(t.ink_2, None, false),
                "label": style(t.ink_3, None, false),
                "link_uri": style(t.ink_3, None, false),
                "link_text": style(t.ink, None, false),
                "string.escape": style(t.ink_3, None, false),
                "property": style(t.ink_2, None, false),
                "emphasis": style(t.ink_body, None, true),
                "emphasis.strong": style(t.ink, Some(600), false),
            }
        }
    });
    Ok(serde_json::from_value(value)?)
}

pub fn apply(mode: Mode, window: Option<&mut Window>, cx: &mut App) -> anyhow::Result<()> {
    let t = Tokens::of(mode);
    let highlight = Arc::new(highlight_theme(&t)?);
    cx.set_global(FocusTheme(t));
    let library_mode = if mode.is_dark() {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    Theme::change(library_mode, None, cx);

    let theme = Theme::global_mut(cx);
    theme.font_family = SANS.into();
    theme.mono_font_family = MONO.into();
    theme.font_size = px(13.);
    theme.mono_font_size = px(12.5);
    theme.radius = px(2.);
    theme.radius_lg = px(0.);
    theme.shadow = false;
    theme.highlight_theme = highlight;

    theme.background = t.bg;
    theme.foreground = t.ink;
    theme.muted = t.card_2;
    theme.muted_foreground = t.ink_3;
    theme.border = t.line;
    theme.input = t.line_2;
    theme.ring = t.ink;
    theme.caret = t.ink;
    theme.selection = t.sel;
    theme.accent = t.card_3;
    theme.accent_foreground = t.ink;
    theme.popover = t.card;
    theme.popover_foreground = t.ink;
    theme.overlay = t.veil;
    theme.list = t.bg;
    theme.list_hover = t.card_2;
    theme.list_active = t.card_3;
    theme.list_active_border = t.ink;
    theme.scrollbar = t.bg.opacity(0.);
    theme.scrollbar_thumb = t.line_2;
    theme.scrollbar_thumb_hover = t.ink_4;
    theme.primary = t.btn;
    theme.primary_hover = t.btn;
    theme.primary_active = t.btn_press;
    theme.primary_foreground = t.btn_ink;
    theme.secondary = t.card_2;
    theme.secondary_hover = t.card_3;
    theme.secondary_active = t.card_3;
    theme.secondary_foreground = t.ink;
    theme.danger = t.danger;
    theme.danger_foreground = t.btn_ink;

    if let Some(window) = window {
        window.refresh();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn highlight_themes_deserialize() {
        for mode in [crate::Mode::Dark, crate::Mode::Light] {
            super::highlight_theme(&crate::Tokens::of(mode)).expect("highlight theme");
        }
    }

    #[test]
    fn markdown_queries_compile() {
        let block: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
        let inline: tree_sitter::Language = tree_sitter_md::INLINE_LANGUAGE.into();
        tree_sitter::Query::new(&block, include_str!("queries/markdown.scm"))
            .expect("markdown.scm");
        tree_sitter::Query::new(&block, include_str!("queries/markdown_injections.scm"))
            .expect("injections");
        tree_sitter::Query::new(&inline, include_str!("queries/markdown_inline.scm"))
            .expect("markdown_inline.scm");
    }
}
