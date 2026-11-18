use crate::cwe_ids::IDS;

pub(crate) fn number(text: &str) -> Option<u32> {
    let text = text.trim();
    let digits = text
        .strip_prefix("CWE-")
        .or_else(|| text.strip_prefix("cwe-"))
        .or_else(|| text.strip_prefix("Cwe-"))
        .unwrap_or(text);
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    let rest = &digits[end..];
    if end == 0 || !(rest.is_empty() || rest.starts_with(|c: char| c == ':' || c.is_whitespace())) {
        return None;
    }
    digits[..end].parse().ok()
}

pub(crate) fn is_listed(number: u32) -> bool {
    IDS.binary_search(&number).is_ok()
}

pub(crate) fn value(number: u32) -> String {
    format!("CWE-{number}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_usual_spellings_give_the_same_number() {
        for text in [
            "CWE-79",
            "cwe-79",
            " 79 ",
            "CWE-79: Cross-site scripting",
            "79 XSS",
        ] {
            assert_eq!(number(text), Some(79), "{text}");
        }
        for text in ["", "CWE", "CWE-", "xss", "79x", "CWE-7a"] {
            assert_eq!(number(text), None, "{text}");
        }
    }

    #[test]
    fn the_list_is_sorted_and_knows_common_weaknesses() {
        assert!(IDS.windows(2).all(|w| w[0] < w[1]));
        assert!(is_listed(79) && is_listed(89) && is_listed(22));
        assert!(!is_listed(0) && !is_listed(99_999));
    }
}
