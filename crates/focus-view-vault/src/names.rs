use crate::Message;

const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
const UNLINKABLE: &[char] = &['#', '^', '[', ']'];
const MAX_BYTES: usize = 255;

pub fn check_name(typed: &str, note: bool) -> Result<String, Message> {
    let name = typed.trim();
    let stem = if note {
        name.strip_suffix(".md")
            .or_else(|| name.strip_suffix(".MD"))
            .unwrap_or(name)
            .trim_end()
    } else {
        name
    };
    if stem.is_empty() {
        return Err(Message::new("vault-name-empty"));
    }
    if let Some(c) = stem
        .chars()
        .find(|c| FORBIDDEN.contains(c) || c.is_control())
    {
        return Err(Message::new("vault-name-forbidden").text_arg("character", shown(c)));
    }
    if let Some(c) = stem.chars().find(|c| UNLINKABLE.contains(c)) {
        return Err(Message::new("vault-name-unlinkable").text_arg("character", c.to_string()));
    }
    if stem.starts_with('.') {
        return Err(Message::new("vault-name-hidden"));
    }
    if stem.ends_with('.') {
        return Err(Message::new("vault-name-dot"));
    }
    let device = stem.split('.').next().unwrap_or(stem).to_ascii_uppercase();
    let reserved = matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (device.len() == 4
            && (device.starts_with("COM") || device.starts_with("LPT"))
            && device.as_bytes()[3].is_ascii_digit());
    if reserved {
        return Err(Message::new("vault-name-reserved").text_arg("name", stem));
    }
    let file = if note {
        format!("{stem}.md")
    } else {
        stem.to_string()
    };
    if file.len() > MAX_BYTES {
        return Err(Message::new("vault-name-long").count_arg("max", MAX_BYTES));
    }
    Ok(file)
}

fn shown(c: char) -> String {
    if c.is_control() {
        format!("U+{:04X}", c as u32)
    } else {
        c.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(typed: &str) -> &'static str {
        match check_name(typed, true) {
            Ok(_) => "ok",
            Err(message) => message.key,
        }
    }

    #[test]
    fn names_windows_obsidian_or_a_link_cannot_use_are_refused() {
        assert_eq!(
            check_name("  Plan de test ", true),
            Ok("Plan de test.md".to_string())
        );
        assert_eq!(check_name("Plan.md", true), Ok("Plan.md".to_string()));
        assert_eq!(check_name("Rendu", false), Ok("Rendu".to_string()));
        for typed in [
            "a/b", "a\\b", "a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b", "a\tb",
        ] {
            assert_eq!(key(typed), "vault-name-forbidden", "{typed}");
        }
        for typed in ["a#b", "a^b", "[a]"] {
            assert_eq!(key(typed), "vault-name-unlinkable", "{typed}");
        }
        assert_eq!(key("   "), "vault-name-empty");
        assert_eq!(key(".md"), "vault-name-empty");
        assert_eq!(key(".cache"), "vault-name-hidden");
        assert_eq!(key(".."), "vault-name-hidden");
        assert_eq!(key("fin."), "vault-name-dot");
        assert_eq!(key("con"), "vault-name-reserved");
        assert_eq!(key("LPT1.txt"), "vault-name-reserved");
        assert_eq!(key("Console"), "ok");
        assert_eq!(key(&"é".repeat(130)), "vault-name-long");
        assert_eq!(key(&"a".repeat(252)), "ok");
    }
}
