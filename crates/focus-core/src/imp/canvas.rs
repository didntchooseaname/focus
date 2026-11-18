use super::parse::Parsed;
use crate::{LinkKind, OutLink, ParsedMeta};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileRef {
    pub range: Range<usize>,
    pub file: String,
    pub subpath: Option<String>,
    pub line: u32,
}

impl FileRef {
    pub fn written(&self) -> String {
        match &self.subpath {
            Some(s) => format!("{}#{s}", self.file),
            None => self.file.clone(),
        }
    }
}

pub(crate) fn file_refs(text: &str) -> Option<Vec<FileRef>> {
    serde_json::from_str::<serde::de::IgnoredAny>(text).ok()?;
    let mut sc = Scanner {
        b: text.as_bytes(),
        text,
        pos: 0,
    };
    sc.ws();
    sc.eat(b'{')?;
    let mut refs = Vec::new();
    sc.members(|sc, key| {
        if key == "nodes" {
            refs = sc.nodes()?;
        } else {
            sc.skip_value()?;
        }
        Some(())
    })?;
    let mut line = 0u32;
    let mut at = 0usize;
    let mut sorted: Vec<&mut FileRef> = refs.iter_mut().collect();
    sorted.sort_by_key(|r| r.range.start);
    for r in sorted {
        line += text.as_bytes()[at..r.range.start]
            .iter()
            .filter(|&&c| c == b'\n')
            .count() as u32;
        at = r.range.start;
        r.line = line;
    }
    Some(refs)
}

pub(crate) fn parse(text: &str) -> Parsed {
    let links = file_refs(text)
        .unwrap_or_default()
        .into_iter()
        .map(|r| OutLink {
            kind: LinkKind::Embed,
            target: r.file,
            anchor: r.subpath,
            line: r.line,
        })
        .collect();
    Parsed {
        meta: ParsedMeta {
            links,
            ..ParsedMeta::default()
        },
        tasks: Vec::new(),
    }
}

pub(crate) fn rewrite(
    text: &str,
    mut new_path: impl FnMut(&FileRef) -> Option<String>,
) -> Option<(String, usize)> {
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for r in file_refs(text)? {
        if let Some(p) = new_path(&r).filter(|p| *p != r.file) {
            let quoted = serde_json::to_string(&p).ok()?;
            edits.push((r.range, quoted));
        }
    }
    edits.sort_by_key(|(range, _)| range.start);
    let count = edits.len();
    let mut out = text.to_string();
    for (range, new) in edits.into_iter().rev() {
        out.replace_range(range, &new);
    }
    Some((out, count))
}

struct Scanner<'a> {
    b: &'a [u8],
    text: &'a str,
    pos: usize,
}

impl Scanner<'_> {
    fn ws(&mut self) {
        while let Some(c) = self.b.get(self.pos)
            && matches!(c, b' ' | b'\t' | b'\n' | b'\r')
        {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }

    fn eat(&mut self, c: u8) -> Option<()> {
        (self.peek()? == c).then(|| self.pos += 1)
    }

    fn string(&mut self) -> Option<Range<usize>> {
        let start = self.pos;
        self.eat(b'"')?;
        loop {
            match self.peek()? {
                b'\\' => self.pos += 2,
                b'"' => {
                    self.pos += 1;
                    return Some(start..self.pos);
                }
                _ => self.pos += 1,
            }
        }
    }

    fn decode(&self, range: Range<usize>) -> Option<String> {
        let raw = self.text.get(range)?;
        if raw.contains('\\') {
            serde_json::from_str(raw).ok()
        } else {
            raw.get(1..raw.len() - 1).map(str::to_string)
        }
    }

    fn members(&mut self, mut each: impl FnMut(&mut Self, &str) -> Option<()>) -> Option<()> {
        self.ws();
        if self.eat(b'}').is_some() {
            return Some(());
        }
        loop {
            self.ws();
            let key = self.string()?;
            let key = self.decode(key)?;
            self.ws();
            self.eat(b':')?;
            self.ws();
            each(self, &key)?;
            self.ws();
            match self.peek()? {
                b',' => self.pos += 1,
                b'}' => {
                    self.pos += 1;
                    return Some(());
                }
                _ => return None,
            }
        }
    }

    fn nodes(&mut self) -> Option<Vec<FileRef>> {
        self.eat(b'[')?;
        let mut refs = Vec::new();
        self.ws();
        if self.eat(b']').is_some() {
            return Some(refs);
        }
        loop {
            self.ws();
            self.eat(b'{')?;
            let mut kind = None;
            let mut file = None;
            let mut subpath = None;
            self.members(|sc, key| {
                let is_string = sc.peek()? == b'"';
                match key {
                    "type" | "file" | "subpath" if is_string => {
                        let range = sc.string()?;
                        let value = sc.decode(range.clone())?;
                        match key {
                            "type" => kind = Some(value),
                            "file" => file = Some((range, value)),
                            _ => subpath = Some(value),
                        }
                    }
                    _ => sc.skip_value()?,
                }
                Some(())
            })?;
            if kind.as_deref() == Some("file")
                && let Some((range, file)) = file.filter(|(_, f)| !f.is_empty())
            {
                let subpath = subpath
                    .map(|s| s.strip_prefix('#').map(str::to_string).unwrap_or(s))
                    .filter(|s| !s.is_empty());
                refs.push(FileRef {
                    range,
                    file,
                    subpath,
                    line: 0,
                });
            }
            self.ws();
            match self.peek()? {
                b',' => self.pos += 1,
                b']' => {
                    self.pos += 1;
                    return Some(refs);
                }
                _ => return None,
            }
        }
    }

    fn skip_value(&mut self) -> Option<()> {
        match self.peek()? {
            b'"' => {
                self.string()?;
            }
            b'{' | b'[' => {
                let mut depth = 0usize;
                loop {
                    match self.peek()? {
                        b'"' => {
                            self.string()?;
                            continue;
                        }
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth = depth.checked_sub(1)?;
                            if depth == 0 {
                                self.pos += 1;
                                return Some(());
                            }
                        }
                        _ => {}
                    }
                    self.pos += 1;
                }
            }
            _ => {
                while !matches!(
                    self.peek()?,
                    b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r'
                ) {
                    self.pos += 1;
                }
            }
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_nodes_are_links_and_text_cards_are_not() {
        let text = "{\"nodes\":[\n{\"id\":\"1\",\"type\":\"text\",\"text\":\"[[A]] [b](B.md)\",\"x\":0,\"y\":0,\"width\":1,\"height\":1},\n{\"id\":\"2\",\"type\":\"file\",\"file\":\"dir/N.md\",\"subpath\":\"#Part\",\"x\":0,\"y\":0,\"width\":1,\"height\":1},\n{\"type\":\"file\",\"file\":\"img.png\"},\n{\"type\":\"link\",\"url\":\"https://x\"},\n{\"type\":\"group\",\"background\":\"bg.png\",\"label\":\"g\"}\n],\"edges\":[{\"id\":\"e\",\"fromNode\":\"1\",\"toNode\":\"2\"}]}";
        let refs = file_refs(text).unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].file, "dir/N.md");
        assert_eq!(refs[0].subpath.as_deref(), Some("Part"));
        assert_eq!(refs[0].line, 2);
        assert_eq!(&text[refs[0].range.clone()], "\"dir/N.md\"");
        assert_eq!((refs[1].file.as_str(), refs[1].line), ("img.png", 3));
        let links = parse(text).meta.links;
        assert_eq!(links[0].kind, LinkKind::Embed);
        assert_eq!(links[0].anchor.as_deref(), Some("Part"));
    }

    #[test]
    fn keys_in_any_order_and_escaped() {
        let text = r#"{"edges":[],"nodes":[{"file":"a\/b \"q\".md","x":-1.5e2,"t":[true,null,{"y":"}"}],"type":"file"}]}"#;
        let refs = file_refs(text).unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].file, "a/b \"q\".md");
    }

    #[test]
    fn not_json_or_not_a_canvas_has_no_links() {
        for text in [
            "",
            "not json",
            "{\"nodes\":[{\"type\":\"file\",\"file\":\"a.md\"}]",
            "{\"nodes\":[{\"type\":\"file\",\"file\":\"a.md\"},]}",
            "[{\"type\":\"file\",\"file\":\"a.md\"}]",
            "{\"nodes\":{\"type\":\"file\",\"file\":\"a.md\"}}",
            "{\"nodes\":[\"a.md\"]}",
            "\"text\"",
        ] {
            assert!(file_refs(text).is_none(), "{text}");
            assert!(parse(text).meta.links.is_empty());
            assert!(rewrite(text, |_| Some("x.md".into())).is_none());
        }
        assert_eq!(file_refs("{}").unwrap(), Vec::new());
        assert_eq!(file_refs(" {\"nodes\":[]} ").unwrap(), Vec::new());
        let odd =
            r#"{"nodes":[{"type":"file","file":3},{"type":"file"},{"type":"file","file":""}]}"#;
        assert_eq!(file_refs(odd).unwrap(), Vec::new());
    }

    #[test]
    fn rewrite_changes_only_the_strings() {
        let text = "{\n  \"nodes\" : [ {\"file\":\"a.md\", \"type\":\"file\", \"text\":\"caf\\u00e9 a.md\"},\n {\"type\":\"file\",\"file\":\"keep.md\"} ] ,\"x\":\"\\/\"}\n";
        let (out, n) = rewrite(text, |r| {
            (r.file == "a.md").then(|| "dir/\"é\"\\.md".into())
        })
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            out,
            text.replacen("\"a.md\"", "\"dir/\\\"é\\\"\\\\.md\"", 1)
        );
        assert_eq!(file_refs(&out).unwrap()[0].file, "dir/\"é\"\\.md");
        let (same, n) = rewrite(text, |r| Some(r.file.clone())).unwrap();
        assert_eq!((same.as_str(), n), (text, 0));
    }
}
