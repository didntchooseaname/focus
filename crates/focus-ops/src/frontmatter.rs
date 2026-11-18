use std::collections::BTreeMap;

#[derive(Debug, Clone)]
struct Entry {
    key: String,
    lines: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    entries: Vec<Entry>,
    body: String,
    crlf: bool,
}

fn is_key_line(line: &str) -> Option<&str> {
    let first = line.chars().next()?;
    if !(first.is_alphabetic() || first == '_') {
        return None;
    }
    let (key, rest) = line.split_once(':')?;
    if key.trim_end().contains(char::is_whitespace) {
        return None;
    }
    (rest.is_empty() || rest.starts_with([' ', '\t'])).then(|| key.trim_end())
}

impl Document {
    pub fn parse(text: &str) -> Document {
        let crlf = text.contains("\r\n");
        let (head, rest) = match split_front(text) {
            Some(parts) => parts,
            None => {
                return Document {
                    entries: Vec::new(),
                    body: text.to_string(),
                    crlf,
                };
            }
        };
        let mut entries: Vec<Entry> = Vec::new();
        for line in head.lines() {
            match is_key_line(line) {
                Some(key) => entries.push(Entry {
                    key: key.to_string(),
                    lines: vec![line.to_string()],
                }),
                None => match entries.last_mut() {
                    Some(last)
                        if !last.key.is_empty()
                            && (line.starts_with([' ', '\t'])
                                || line.starts_with("- ")
                                || line.trim().is_empty()) =>
                    {
                        last.lines.push(line.to_string())
                    }
                    _ => entries.push(Entry {
                        key: String::new(),
                        lines: vec![line.to_string()],
                    }),
                },
            }
        }
        Document {
            entries,
            body: rest.to_string(),
            crlf,
        }
    }

    pub fn has_frontmatter_keys(&self) -> bool {
        self.entries.iter().any(|e| !e.key.is_empty())
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn set_body(&mut self, body: String) {
        self.body = body;
    }

    fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.key == key)
    }

    fn inline_value(&self, key: &str) -> Option<String> {
        let first = &self.entry(key)?.lines[0];
        let value = first.split_once(':')?.1;
        Some(strip_comment(value).trim().to_string())
    }

    pub fn scalar(&self, key: &str) -> Option<String> {
        let value = self.inline_value(key)?;
        if value.is_empty() || value == "~" || value == "null" || value.starts_with(['[', '{']) {
            return None;
        }
        Some(unquote(&value))
    }

    pub fn list(&self, key: &str) -> Vec<String> {
        let Some(entry) = self.entry(key) else {
            return Vec::new();
        };
        let inline = self.inline_value(key).unwrap_or_default();
        if let Some(inner) = inline.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            return split_flow(inner)
                .into_iter()
                .map(|v| unquote(v.trim()))
                .filter(|v| !v.is_empty())
                .collect();
        }
        if !inline.is_empty() && inline != "~" && inline != "null" {
            return vec![unquote(&inline)];
        }
        entry.lines[1..]
            .iter()
            .filter_map(|l| {
                l.trim_start()
                    .strip_prefix("- ")
                    .or_else(|| (l.trim() == "-").then_some(""))
            })
            .map(|v| unquote(strip_comment(v).trim()))
            .filter(|v| !v.is_empty())
            .collect()
    }

    pub fn map(&self, key: &str) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        let Some(entry) = self.entry(key) else {
            return out;
        };
        let inline = self.inline_value(key).unwrap_or_default();
        if let Some(inner) = inline.strip_prefix('{').and_then(|v| v.strip_suffix('}')) {
            for part in split_flow(inner) {
                if let Some((k, v)) = part.split_once(':') {
                    out.insert(unquote(k.trim()), unquote(v.trim()));
                }
            }
            return out;
        }
        for line in &entry.lines[1..] {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = split_map_line(line) {
                out.insert(k, v);
            }
        }
        out
    }

    fn put(&mut self, key: &str, lines: Vec<String>) {
        match self.entries.iter_mut().find(|e| e.key == key) {
            Some(entry) => entry.lines = lines,
            None => self.entries.push(Entry {
                key: key.to_string(),
                lines,
            }),
        }
    }

    pub fn remove(&mut self, key: &str) {
        self.entries.retain(|e| e.key != key);
    }

    pub fn set_scalar(&mut self, key: &str, value: Option<&str>) {
        match value {
            Some(v) => self.put(key, vec![format!("{key}: {}", quote(v))]),
            None => self.remove(key),
        }
    }

    pub fn set_list(&mut self, key: &str, values: &[String]) {
        if self.entry(key).is_some() && self.list(key) == values {
            return;
        }
        let lines = if values.is_empty() {
            vec![format!("{key}: []")]
        } else {
            std::iter::once(format!("{key}:"))
                .chain(values.iter().map(|v| format!("  - {}", quote(v))))
                .collect()
        };
        self.put(key, lines);
    }

    pub fn set_map(&mut self, key: &str, values: &BTreeMap<String, String>) {
        if self.entry(key).is_some() && self.map(key) == *values {
            return;
        }
        let lines = if values.is_empty() {
            vec![format!("{key}: {{}}")]
        } else {
            std::iter::once(format!("{key}:"))
                .chain(
                    values
                        .iter()
                        .map(|(k, v)| format!("  {}: {}", quote(k), quote(v))),
                )
                .collect()
        };
        self.put(key, lines);
    }

    pub fn render(&self) -> String {
        let eol = if self.crlf { "\r\n" } else { "\n" };
        let mut out = String::new();
        if self.has_frontmatter_keys() || !self.entries.is_empty() {
            out.push_str("---");
            out.push_str(eol);
            for entry in &self.entries {
                for line in &entry.lines {
                    out.push_str(line);
                    out.push_str(eol);
                }
            }
            out.push_str("---");
            out.push_str(eol);
        }
        out.push_str(&self.body);
        out
    }
}

fn split_front(text: &str) -> Option<(&str, &str)> {
    let rest = text
        .strip_prefix("---\r\n")
        .or_else(|| text.strip_prefix("---\n"))?;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
            return Some((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    None
}

fn strip_comment(value: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut previous = ' ';
    for (i, c) in value.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' && previous.is_whitespace() => return &value[..i],
            None => {}
        }
        previous = c;
    }
    value
}

fn split_flow(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut quote: Option<char> = None;
    let mut start = 0;
    for (i, c) in inner.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == ',' => {
                parts.push(&inner[start..i]);
                start = i + 1;
            }
            None => {}
        }
    }
    parts.push(&inner[start..]);
    parts
}

fn split_map_line(line: &str) -> Option<(String, String)> {
    let (key, value) = if let Some(rest) = line.strip_prefix('"') {
        let end = rest.find("\":")?;
        (unquote(&format!("\"{}\"", &rest[..end])), &rest[end + 2..])
    } else {
        let (k, v) = line.split_once(':')?;
        (k.trim().to_string(), v)
    };
    Some((key, unquote(strip_comment(value).trim())))
}

pub fn unquote(value: &str) -> String {
    let value = value.trim();
    if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(other) => out.push(other),
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        return out;
    }
    if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        return inner.replace("''", "'");
    }
    value.to_string()
}

pub fn quote(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let risky = value.is_empty()
        || value != value.trim()
        || value.starts_with([
            '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%',
            '@', '`',
        ])
        || value.contains(": ")
        || value.contains(" #")
        || value.ends_with(':')
        || value.contains(['\n', '\r', '\t'])
        || matches!(
            lower.as_str(),
            "true" | "false" | "yes" | "no" | "on" | "off" | "null" | "~"
        )
        || value.parse::<f64>().is_ok();
    if !risky {
        return value.to_string();
    }
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "---\ntype: lab\n# comment\nstatus: in-progress\nclient: \"ACME: subsidiary\"\nscope:\n  - 10.0.0.0/24\n  - corp.local\ntags: [a, \"b c\"]\nvars:\n  LHOST: 10.10.14.2\n  DOMAIN: corp.local\ncustom_key: {x: 1}\nlist_unknown:\n- one\n- two\n---\n# Title\n\nFree body.\n";

    #[test]
    fn reads_scalars_lists_and_maps() {
        let doc = Document::parse(NOTE);
        assert_eq!(doc.scalar("type").as_deref(), Some("lab"));
        assert_eq!(doc.scalar("client").as_deref(), Some("ACME: subsidiary"));
        assert_eq!(doc.list("scope"), ["10.0.0.0/24", "corp.local"]);
        assert_eq!(doc.list("tags"), ["a", "b c"]);
        assert_eq!(doc.list("list_unknown"), ["one", "two"]);
        assert_eq!(
            doc.map("vars").get("LHOST").map(String::as_str),
            Some("10.10.14.2")
        );
        assert_eq!(doc.scalar("missing"), None);
        assert_eq!(doc.body(), "# Title\n\nFree body.\n");
    }

    #[test]
    fn untouched_rewrite_is_byte_identical() {
        assert_eq!(Document::parse(NOTE).render(), NOTE);
    }

    #[test]
    fn rewrite_keeps_unknown_keys_comments_and_body() {
        let mut doc = Document::parse(NOTE);
        doc.set_scalar("status", Some("reporting"));
        doc.set_list("scope", &["10.0.0.0/24".into(), "corp.local".into()]);
        doc.set_list("tags", &["a".into(), "b c".into()]);
        doc.set_scalar("deadline", Some("2026-12-01"));
        doc.set_scalar("client", None);
        let out = doc.render();
        assert!(out.contains("status: reporting\n"));
        assert!(
            out.contains("# comment\n")
                && out.contains("custom_key: {x: 1}\n")
                && out.contains("list_unknown:\n- one\n- two\n")
        );
        assert!(out.contains("tags: [a, \"b c\"]\n"), "inline style kept");
        assert!(out.contains("deadline: 2026-12-01\n") && !out.contains("client"));
        assert!(out.ends_with("---\n# Title\n\nFree body.\n"));
    }

    #[test]
    fn quoting_roundtrips_awkward_values() {
        let mut doc = Document::default();
        let values = [
            "",
            "10",
            "true",
            "a: b",
            "# x",
            " space",
            "quote \" and \\",
            "line\nbreak",
            "10.10.11.5",
            "ok",
        ];
        for (i, v) in values.iter().enumerate() {
            doc.set_scalar(&format!("k{i}"), Some(v));
        }
        let parsed = Document::parse(&doc.render());
        for (i, v) in values.iter().enumerate() {
            assert_eq!(
                parsed.scalar(&format!("k{i}")).unwrap_or_default(),
                *v,
                "value {v:?}"
            );
        }
        assert_eq!(quote("10.10.11.5"), "10.10.11.5");
    }

    #[test]
    fn crlf_and_missing_frontmatter() {
        let doc = Document::parse("---\r\ntype: lab\r\n---\r\nbody\r\n");
        assert_eq!(doc.scalar("type").as_deref(), Some("lab"));
        assert_eq!(doc.render(), "---\r\ntype: lab\r\n---\r\nbody\r\n");
        let plain = Document::parse("# Title\n---\nno\n");
        assert!(!plain.has_frontmatter_keys());
        assert_eq!(plain.render(), "# Title\n---\nno\n");
        let mut fresh = Document::parse("text\n");
        fresh.set_scalar("a", Some("b"));
        assert_eq!(fresh.render(), "---\na: b\n---\ntext\n");
    }

    #[test]
    fn map_roundtrip_and_empty_forms() {
        let mut doc = Document::default();
        let mut vars = BTreeMap::new();
        vars.insert("PASS".to_string(), "p@ss: 1".to_string());
        vars.insert("LHOST".to_string(), "10.10.14.2".to_string());
        doc.set_map("vars", &vars);
        doc.set_list("scope", &[]);
        let parsed = Document::parse(&doc.render());
        assert_eq!(parsed.map("vars"), vars);
        assert!(parsed.list("scope").is_empty());
    }
}
