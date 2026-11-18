use std::path::{Path, PathBuf};

use focus_kit::{ActiveTokens, Button, ButtonKind, Flow as _, Type, size};
use gpui::{
    Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled, Window, div, px,
};

use crate::shell::{Overlay, Shell};

const DIRECT: [&str; 4] = ["http", "https", "mailto", "tel"];

const MEDIA: [&str; 11] = [
    "png", "jpg", "jpeg", "gif", "webp", "svg", "pdf", "mp3", "mp4", "webm", "txt",
];

#[derive(Debug, PartialEq)]
enum Address {
    Direct,
    Ask,
    Refused,
}

fn scheme(target: &str) -> Option<String> {
    let (scheme, _) = target.split_once(':')?;
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then(|| scheme.to_ascii_lowercase())
}

fn classify(target: &str) -> Address {
    if target.starts_with('-') || target.trim().is_empty() {
        return Address::Refused;
    }
    match scheme(target) {
        Some(scheme) if DIRECT.contains(&scheme.as_str()) => Address::Direct,
        _ => Address::Ask,
    }
}

fn attachment_target(path: &Path) -> PathBuf {
    let media = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA.contains(&e.to_ascii_lowercase().as_str()));
    if media || path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map_or_else(|| path.to_path_buf(), Path::to_path_buf)
    }
}

#[cfg(not(windows))]
fn launch(target: &str) -> std::io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map(drop)
}

#[cfg(windows)]
fn launch(target: &str) -> std::io::Result<()> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(target));
    // SAFETY: both strings are NUL terminated and outlive the call; no window is given.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as usize > 32 {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "ShellExecuteW returned {}",
            result as usize
        )))
    }
}

impl Shell {
    pub fn open_address(&mut self, target: &str, window: &mut Window, cx: &mut Context<Self>) {
        if crate::obsidian::is_obsidian(target) {
            return self.follow_obsidian(target, window, cx);
        }
        match classify(target) {
            Address::Direct => self.hand_over(target, cx),
            Address::Ask => {
                self.overlay = Overlay::OpenAddress(target.to_string());
                cx.notify();
            }
            Address::Refused => self.notify_user(
                focus_kit::Tone::Danger,
                focus_i18n::t_args("ui-address-refused", &[("target", target.into())]),
                cx,
            ),
        }
    }

    pub fn open_file(&mut self, path: &Path, cx: &mut Context<Self>) {
        let target = attachment_target(path);
        self.hand_over(&target.display().to_string(), cx);
    }

    fn hand_over(&mut self, target: &str, cx: &mut Context<Self>) {
        if classify(target) == Address::Refused && !Path::new(target).is_absolute() {
            return;
        }
        if let Err(error) = launch(target) {
            self.notify_user(
                focus_kit::Tone::Danger,
                focus_i18n::t_args(
                    "reading-open-failed",
                    &[
                        ("target", target.into()),
                        ("error", error.to_string().as_str().into()),
                    ],
                ),
                cx,
            );
        }
    }

    pub fn confirm_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Overlay::OpenAddress(target) = std::mem::replace(&mut self.overlay, Overlay::None) {
            self.hand_over(&target, cx);
        }
        self.close_overlay(window, cx);
    }

    pub fn render_open_address(&self, target: &str, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = *cx.tokens();
        div()
            .id("address-veil")
            .absolute()
            .inset_0()
            .flow_row()
            .items_center()
            .justify_center()
            .bg(t.veil)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_overlay(window, cx)),
            )
            .child(
                focus_kit::floating(&t)
                    .id("address-dialog")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .flex()
                    .flex_col()
                    .gap(size::S3)
                    .p(size::S4)
                    .w(px(480.))
                    .child(
                        div()
                            .ui()
                            .text_color(t.ink)
                            .child(focus_i18n::t("ui-address-title")),
                    )
                    .child(
                        div()
                            .ui_sm()
                            .text_color(t.ink_3)
                            .child(focus_i18n::t("ui-address-why")),
                    )
                    .child(
                        div()
                            .mono_sm()
                            .text_color(t.ink)
                            .bg(t.card_2)
                            .p(size::S2)
                            .child(target.to_string()),
                    )
                    .child(
                        div()
                            .flow_row()
                            .justify_end()
                            .gap(size::S2)
                            .child(
                                Button::new("address-cancel", focus_i18n::t("ui-action-cancel"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_overlay(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("address-open", focus_i18n::t("ui-address-open"))
                                    .kind(ButtonKind::Primary)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.confirm_address(window, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_mail_and_telephone_addresses_open_without_a_question() {
        for direct in [
            "https://example.org/a?b=c",
            "http://10.10.10.5:8080/",
            "mailto:a@b.c",
            "tel:+33123456789",
            "HTTPS://EXAMPLE.ORG",
        ] {
            assert_eq!(classify(direct), Address::Direct, "{direct}");
        }
        for asked in [
            "smb://host/share",
            "file:///home/me/.config/autostart/a.desktop",
            "javascript:alert(1)",
            "vscode://file/x",
            "ssh://root@host",
            "obsidian://open?vault=x",
            "notes.md",
            "C:\\Windows",
        ] {
            assert_eq!(classify(asked), Address::Ask, "{asked}");
        }
        for refused in ["-x", "--help", "--manual", "   "] {
            assert_eq!(classify(refused), Address::Refused, "{refused}");
        }
    }

    #[test]
    fn an_attachment_that_is_not_a_media_file_opens_its_folder() {
        assert_eq!(
            attachment_target(Path::new("/v/assets/shot.PNG")),
            PathBuf::from("/v/assets/shot.PNG")
        );
        assert_eq!(
            attachment_target(Path::new("/v/assets/report.pdf")),
            PathBuf::from("/v/assets/report.pdf")
        );
        for hostile in [
            "/v/assets/evil.sh",
            "/v/assets/run.desktop",
            "/v/assets/tool.AppImage",
            "/v/assets/noext",
        ] {
            assert_eq!(
                attachment_target(Path::new(hostile)),
                PathBuf::from("/v/assets"),
                "{hostile}"
            );
        }
    }
}
