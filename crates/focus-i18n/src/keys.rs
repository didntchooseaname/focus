use crate::state::t;

fn key_name(part: &str) -> String {
    let key = match part {
        "ctrl" => "ui-key-ctrl",
        "alt" => "ui-key-alt",
        "shift" => "ui-key-shift",
        "cmd" | "super" => "ui-key-super",
        "escape" => "ui-key-escape",
        "enter" => "ui-key-enter",
        "tab" => "ui-key-tab",
        "left" => "ui-key-left",
        "right" => "ui-key-right",
        "up" => "ui-key-up",
        "down" => "ui-key-down",
        "space" => "ui-key-space",
        "delete" => "ui-key-delete",
        "backspace" => "ui-key-backspace",
        "home" => "ui-key-home",
        "end" => "ui-key-end",
        "pageup" => "ui-key-page-up",
        "pagedown" => "ui-key-page-down",
        "insert" => "ui-key-insert",
        "menu" => "ui-key-menu",
        "add" => "ui-key-add",
        "subtract" => "ui-key-subtract",
        other => return other.to_uppercase(),
    };
    t(key)
}

fn stroke_text(stroke: &str) -> String {
    let parts: Vec<&str> = if stroke.ends_with("--") || stroke == "-" {
        vec![stroke.trim_end_matches('-'), "-"]
    } else {
        stroke.split('-').collect()
    };
    parts
        .iter()
        .filter(|p| !p.is_empty())
        .map(|part| key_name(part))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn keys_text(keys: &str) -> String {
    keys.split(' ')
        .map(stroke_text)
        .collect::<Vec<_>>()
        .join(" ")
}
