#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Token<'a> {
    Text(&'a str),
    Open {
        name: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
        raw: &'a str,
    },
    Close {
        name: String,
    },
    Comment,
}

pub(crate) fn tokens(html: &str) -> Vec<(std::ops::Range<usize>, Token<'_>)> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut text_from = 0;
    while i < html.len() {
        let rest = &html[i..];
        if !rest.starts_with('<') {
            i += rest.chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let token_end = if rest.starts_with("<!--") {
            rest.find("-->").map(|o| (o + 3, Token::Comment))
        } else if rest.starts_with("<!") || rest.starts_with("<?") {
            rest.find('>').map(|o| (o + 1, Token::Comment))
        } else {
            parse_tag(rest)
        };
        match token_end {
            Some((len, token)) => {
                if text_from < i {
                    out.push((text_from..i, Token::Text(&html[text_from..i])));
                }
                let token = match token {
                    Token::Open {
                        name,
                        attrs,
                        self_closing,
                        ..
                    } => Token::Open {
                        name,
                        attrs,
                        self_closing,
                        raw: &html[i..i + len],
                    },
                    other => other,
                };
                out.push((i..i + len, token));
                i += len;
                text_from = i;
            }
            None => i += 1,
        }
    }
    if text_from < html.len() {
        out.push((text_from..html.len(), Token::Text(&html[text_from..])));
    }
    out
}

fn parse_tag(rest: &str) -> Option<(usize, Token<'_>)> {
    let bytes = rest.as_bytes();
    let closing = bytes.get(1) == Some(&b'/');
    let name_start = 1 + usize::from(closing);
    let name_len = rest[name_start..]
        .find(|c: char| {
            !(c.is_ascii_alphanumeric() || c == '-' || c == ':' || c == '_' || c == '.')
        })
        .unwrap_or(rest.len() - name_start);
    if name_len == 0 || !bytes[name_start].is_ascii_alphabetic() {
        return None;
    }
    let name = rest[name_start..name_start + name_len].to_ascii_lowercase();
    let mut i = name_start + name_len;
    let mut attrs = Vec::new();
    loop {
        while i < rest.len() && rest[i..].starts_with(|c: char| c.is_whitespace()) {
            i += rest[i..].chars().next().map_or(1, char::len_utf8);
        }
        let tail = rest.get(i..)?;
        if tail.is_empty() {
            return None;
        }
        if tail.starts_with('>') {
            return Some((
                i + 1,
                if closing {
                    Token::Close { name }
                } else {
                    Token::Open {
                        name,
                        attrs,
                        self_closing: false,
                        raw: "",
                    }
                },
            ));
        }
        if tail.starts_with("/>") {
            return Some((
                i + 2,
                if closing {
                    Token::Close { name }
                } else {
                    Token::Open {
                        name,
                        attrs,
                        self_closing: true,
                        raw: "",
                    }
                },
            ));
        }
        if tail.starts_with('/') {
            i += 1;
            continue;
        }
        let key_len = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '=' | '>' | '/'))
            .unwrap_or(tail.len())
            .max(tail.chars().next().map_or(1, char::len_utf8));
        let key = tail[..key_len].to_ascii_lowercase();
        i += key_len;
        let after_key = rest[i..].trim_start();
        let skipped = rest[i..].len() - after_key.len();
        if let Some(value_part) = after_key.strip_prefix('=') {
            let value_part_trimmed = value_part.trim_start();
            let skipped_value = value_part.len() - value_part_trimmed.len();
            let (value, consumed) = match value_part_trimmed.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let close = value_part_trimmed[1..].find(q)?;
                    (&value_part_trimmed[1..1 + close], close + 2)
                }
                _ => {
                    let len = value_part_trimmed
                        .find(|c: char| c.is_whitespace() || c == '>')
                        .unwrap_or(value_part_trimmed.len());
                    (&value_part_trimmed[..len], len)
                }
            };
            attrs.push((key, decode_entities(value)));
            i += skipped + 1 + skipped_value + consumed;
        } else {
            attrs.push((key, String::new()));
        }
    }
}

pub(crate) fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

pub(crate) fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest.find(';').filter(|&semi| semi <= 10).and_then(|semi| {
            let name = &rest[1..semi];
            let ch = if let Some(num) = name.strip_prefix('#') {
                let code = match num.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                    None => num.parse().ok()?,
                };
                char::from_u32(code)?
            } else {
                match name {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    "nbsp" => '\u{a0}',
                    "copy" => '©',
                    "reg" => '®',
                    "hellip" => '…',
                    "mdash" => '\u{2014}',
                    "ndash" => '\u{2013}',
                    "laquo" => '«',
                    "raquo" => '»',
                    "rarr" => '\u{2192}',
                    "larr" => '\u{2190}',
                    "times" => '×',
                    "middot" => '·',
                    _ => return None,
                }
            };
            Some((ch, semi + 1))
        });
        match decoded {
            Some((ch, len)) => {
                out.push(ch);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub(crate) fn plain(html: &str) -> String {
    let mut text = String::new();
    for (_, token) in tokens(html) {
        match token {
            Token::Text(t) => text.push_str(&decode_entities(t)),
            Token::Open { ref name, .. } if name == "br" => text.push(' '),
            _ => {}
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_attributes_and_entities() {
        let t = tokens(
            "a <img src=\"x.png\" alt='A &amp; B' width=10 hidden/> b &lt;c&gt; <!-- c --></p>",
        );
        assert!(matches!(&t[0].1, Token::Text("a ")));
        let Token::Open {
            name,
            attrs,
            self_closing,
            ..
        } = &t[1].1
        else {
            panic!()
        };
        assert_eq!(name, "img");
        assert!(*self_closing);
        assert_eq!(attr(attrs, "alt"), Some("A & B"));
        assert_eq!(attr(attrs, "width"), Some("10"));
        assert_eq!(attr(attrs, "hidden"), Some(""));
        assert_eq!(
            decode_entities("&lt;c&gt; &#65;&#x42; &unknown; &"),
            "<c> AB &unknown; &"
        );
        assert!(matches!(t.last(), Some((_, Token::Close { name })) if name == "p"));
        assert_eq!(plain("<p>un <b>deux</b>&nbsp;trois</p>"), "un deux trois");
    }

    #[test]
    fn lone_angle_bracket_is_text() {
        let t = tokens("1 < 2 and <3 <a href=x>");
        assert!(matches!(t.last(), Some((_, Token::Open { name, .. })) if name == "a"));
        assert!(
            t.iter()
                .any(|(_, x)| matches!(x, Token::Text(s) if s.contains("1 < 2")))
        );
    }
}
