use std::ops::Range;
use std::path::{Path, PathBuf};

use focus_md::{Block, BlockKind, Document, Inline, InlineKind, ParseOptions, parse};

fn doc(src: &str) -> Document {
    parse(src, &ParseOptions::default())
}

fn walk_inlines<'d>(inlines: &'d [Inline], out: &mut Vec<&'d Inline>) {
    for inline in inlines {
        out.push(inline);
        match &inline.kind {
            InlineKind::Strong(c)
            | InlineKind::Emphasis(c)
            | InlineKind::Strike(c)
            | InlineKind::Highlight(c) => walk_inlines(c, out),
            InlineKind::Link(link) => walk_inlines(&link.children, out),
            _ => {}
        }
    }
}

fn own_inlines(block: &Block) -> Vec<&[Inline]> {
    match &block.kind {
        BlockKind::Heading { inlines, .. } | BlockKind::Paragraph(inlines) => vec![inlines],
        BlockKind::Callout { title, .. } => vec![title],
        BlockKind::Details { summary, .. } => vec![summary],
        BlockKind::Table { head, rows, .. } => head
            .iter()
            .chain(rows.iter().flatten())
            .map(Vec::as_slice)
            .collect(),
        _ => Vec::new(),
    }
}

fn children(block: &Block) -> Vec<&Block> {
    match &block.kind {
        BlockKind::List { items, .. } => items.iter().flat_map(|i| &i.blocks).collect(),
        BlockKind::Quote(inner)
        | BlockKind::Callout { blocks: inner, .. }
        | BlockKind::Details { blocks: inner, .. } => inner.iter().collect(),
        BlockKind::Tabs { tabs, .. } => tabs.iter().flat_map(|t| &t.blocks).collect(),
        _ => Vec::new(),
    }
}

fn all_blocks(document: &Document) -> Vec<&Block> {
    fn visit<'d>(block: &'d Block, out: &mut Vec<&'d Block>) {
        out.push(block);
        for child in children(block) {
            visit(child, out);
        }
    }
    let mut out = Vec::new();
    for block in document
        .blocks
        .iter()
        .chain(document.footnotes.iter().flat_map(|f| &f.blocks))
    {
        visit(block, &mut out);
    }
    out
}

fn all_inlines(document: &Document) -> Vec<&Inline> {
    let mut out = Vec::new();
    for block in all_blocks(document) {
        for list in own_inlines(block) {
            walk_inlines(list, &mut out);
        }
    }
    out
}

fn texts(document: &Document) -> Vec<(&str, Range<usize>, bool)> {
    all_inlines(document)
        .into_iter()
        .filter_map(|i| match &i.kind {
            InlineKind::Text(t) => Some((t.as_str(), i.source.clone(), i.exact())),
            _ => None,
        })
        .collect()
}

fn within(inner: &Range<usize>, outer: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

fn check(source: &str, what: &str) -> usize {
    let document = doc(source);
    let comments = source.contains("%%");
    let within = |inner: &Range<usize>, outer: &Range<usize>| {
        within(inner, outer) || (comments && inner.is_empty())
    };
    let valid = |r: &Range<usize>| {
        r.start <= r.end
            && r.end <= source.len()
            && source.is_char_boundary(r.start)
            && source.is_char_boundary(r.end)
    };
    let mut exact = 0;
    if let Some(frontmatter) = &document.frontmatter {
        assert!(valid(&frontmatter.bytes), "{what}: frontmatter");
        assert!(
            source[frontmatter.bytes.clone()].starts_with("---"),
            "{what}"
        );
    }
    for block in all_blocks(&document) {
        assert!(valid(&block.bytes), "{what}: block {block:?}");
        for child in children(block) {
            assert!(
                within(&child.bytes, &block.bytes),
                "{what}: {:?} outside {:?}",
                child.bytes,
                block.bytes
            );
        }
        let markers: Vec<Range<usize>> = match &block.kind {
            BlockKind::Heading { marker, .. } | BlockKind::Callout { marker, .. } => {
                vec![marker.clone()]
            }
            BlockKind::CodeBlock { content, .. } => vec![content.clone()],
            BlockKind::List { items, .. } => items.iter().map(|i| i.marker.clone()).collect(),
            BlockKind::Table { cells, .. } => cells.iter().flatten().cloned().collect(),
            _ => Vec::new(),
        };
        for marker in &markers {
            assert!(valid(marker), "{what}: {marker:?} in {block:?}");
            assert!(
                within(marker, &block.bytes),
                "{what}: {marker:?} outside {:?}",
                block.bytes
            );
        }
        for list in own_inlines(block) {
            let mut inlines = Vec::new();
            walk_inlines(list, &mut inlines);
            for inline in inlines {
                assert!(valid(&inline.source), "{what}: {inline:?}");
                assert!(
                    within(&inline.source, &block.bytes),
                    "{what}: {inline:?} outside {:?} {:?}",
                    block.bytes,
                    &source[block.bytes.clone()]
                );
                let nested = match &inline.kind {
                    InlineKind::Strong(c)
                    | InlineKind::Emphasis(c)
                    | InlineKind::Strike(c)
                    | InlineKind::Highlight(c) => c.as_slice(),
                    InlineKind::Link(link) => &link.children,
                    _ => &[],
                };
                for child in nested {
                    assert!(
                        within(&child.source, &inline.source),
                        "{what}: {child:?} outside {inline:?}"
                    );
                }
                if let InlineKind::Text(text) = &inline.kind
                    && inline.exact()
                {
                    assert_eq!(&source[inline.source.clone()], text, "{what}");
                    exact += 1;
                }
            }
        }
    }
    exact
}

fn notes(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("fixture folder") {
        let path = entry.expect("fixture entry").path();
        if path.is_dir() {
            notes(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

#[test]
fn every_fixture_note_keeps_its_bytes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = Vec::new();
    notes(&root.join("../../fixtures"), &mut paths);
    notes(&root.join("tests/fixtures"), &mut paths);
    assert!(paths.len() > 50, "{} notes", paths.len());
    let (mut exact, mut all) = (0, 0);
    for path in &paths {
        let source = std::fs::read_to_string(path).expect("fixture note");
        exact += check(&source, &path.display().to_string());
        all += texts(&doc(&source)).len();
        let crlf = source.replace('\n', "\r\n");
        check(&crlf, &format!("{} with CRLF", path.display()));
        check(
            &format!("\u{feff}{source}"),
            &format!("{} with BOM", path.display()),
        );
    }
    assert!(exact * 100 > all * 97, "{exact} exact texts of {all}");
}

#[test]
fn comments_are_hidden_bytes_between_texts() {
    let source = "a %%hidden%% b %%x%% c\n";
    let document = doc(source);
    assert_eq!(
        texts(&document),
        [
            ("a ", 0..2, true),
            (" b ", 12..15, true),
            (" c", 20..22, true)
        ]
    );
    assert_eq!(document.blocks[0].bytes, 0..22);
    check(source, "comments");
    check(
        "a %%two\nlines%% b\n\n%%\nalone\n%%\n\nc %% open\n",
        "comments on lines",
    );
    let source = "`%%x%%` and\n\n```\n%% kept %%\n```\n";
    let document = doc(source);
    let BlockKind::CodeBlock { code, content, .. } = &document.blocks[1].kind else {
        panic!("{:?}", document.blocks[1]);
    };
    assert_eq!(
        (code.as_str(), &source[content.clone()]),
        ("%% kept %%", "%% kept %%")
    );
    check(source, "comments in code");
}

#[test]
fn escapes_and_entities_are_atomic() {
    let source = "a \\* b &amp; c \\#tag d\\_e\n";
    let document = doc(source);
    assert_eq!(
        texts(&document),
        [
            ("a ", 0..2, true),
            ("* b ", 3..7, true),
            ("&", 7..12, false),
            (" c ", 12..15, true),
            ("#tag d", 16..22, true),
            ("_e", 23..25, true),
        ]
    );
    let source = "\\==not== ==yes==\n";
    let document = doc(source);
    let inlines = all_inlines(&document)
        .into_iter()
        .map(|i| {
            (
                &source[i.source.clone()],
                matches!(i.kind, InlineKind::Highlight(_)),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        inlines,
        [("==not== ", false), ("==yes==", true), ("yes", false)]
    );
}

#[test]
fn line_endings_and_byte_order_mark() {
    let source = "\u{feff}---\r\ntitle: T\r\n---\r\n\r\n# Head\r\n\r\nline one  \r\nline two\r\n";
    let document = doc(source);
    let frontmatter = document
        .frontmatter
        .as_ref()
        .expect("frontmatter after a BOM");
    assert_eq!(&source[frontmatter.bytes.clone()], "---\r\ntitle: T\r\n---");
    let BlockKind::Heading { marker, .. } = &document.blocks[0].kind else {
        panic!("{:?}", document.blocks[0]);
    };
    assert_eq!(&source[marker.clone()], "# ");
    assert_eq!(&source[document.blocks[0].bytes.clone()], "# Head");
    assert_eq!(
        &source[document.blocks[1].bytes.clone()],
        "line one  \r\nline two"
    );
    let shown: Vec<&str> = texts(&document)
        .iter()
        .map(|t| &source[t.1.clone()])
        .collect();
    assert_eq!(shown, ["Head", "line one", "line two"]);
    check(source, "CRLF and BOM");
}

#[test]
fn links_images_and_references() {
    let source = "[[Note]] [[Note|alias]] [[N#S]] ![alt](a.png) ![[img.png|300]] [l](u) <https://x.y> https://bare.example/p $m$ [^1] #tag ==hi== ^id\n\n[^1]: foot\n";
    let document = doc(source);
    let BlockKind::Paragraph(inlines) = &document.blocks[0].kind else {
        panic!("{:?}", document.blocks[0]);
    };
    let elements: Vec<&str> = inlines
        .iter()
        .filter(|i| !matches!(i.kind, InlineKind::Text(_)))
        .map(|i| &source[i.source.clone()])
        .collect();
    assert_eq!(
        elements,
        [
            "[[Note]]",
            "[[Note|alias]]",
            "[[N#S]]",
            "![alt](a.png)",
            "![[img.png|300]]",
            "[l](u)",
            "<https://x.y>",
            "https://bare.example/p",
            "$m$",
            "[^1]",
            "#tag",
            "==hi==",
            "^id"
        ]
    );
    let link_texts: Vec<(String, &str, bool)> = inlines
        .iter()
        .filter_map(|i| match &i.kind {
            InlineKind::Link(link) => Some(&link.children[0]),
            _ => None,
        })
        .map(|child| {
            let InlineKind::Text(text) = &child.kind else {
                panic!("{child:?}");
            };
            (text.clone(), &source[child.source.clone()], child.exact())
        })
        .collect();
    assert_eq!(
        link_texts,
        [
            ("Note".to_string(), "Note", true),
            ("alias".to_string(), "alias", true),
            ("N > S".to_string(), "N#S", false),
            ("l".to_string(), "l", true),
            ("https://x.y".to_string(), "https://x.y", true),
            (
                "https://bare.example/p".to_string(),
                "https://bare.example/p",
                true
            ),
        ]
    );
    let foot = &document.footnotes[0].blocks[0];
    assert_eq!(&source[foot.bytes.clone()], "foot");
    check(source, "links");
}

#[test]
fn inline_code_with_several_backticks() {
    let source = "``a`b`` and ` x ` and ```c``d```\n";
    let document = doc(source);
    let codes: Vec<(&str, &str)> = all_inlines(&document)
        .into_iter()
        .filter_map(|i| match &i.kind {
            InlineKind::Code(code) => Some((code.as_str(), &source[i.source.clone()])),
            _ => None,
        })
        .collect();
    assert_eq!(
        codes,
        [("a`b", "``a`b``"), ("x", "` x `"), ("c``d", "```c``d```")]
    );
}

#[test]
fn table_cells_without_pipes_or_padding() {
    let source = "| a | b \\| c |  |\n|---|:-:|---|\n| [[N\\|al]] |  | x\n|  y |\n";
    let document = doc(source);
    let BlockKind::Table { cells, rows, .. } = &document.blocks[0].kind else {
        panic!("{:?}", document.blocks[0]);
    };
    let shown: Vec<Vec<(&str, usize)>> = cells
        .iter()
        .map(|row| row.iter().map(|c| (&source[c.clone()], c.start)).collect())
        .collect();
    assert_eq!(
        shown,
        [
            vec![("a", 2), ("b \\| c", 6), ("", 15)],
            vec![("[[N\\|al]]", 34), ("", 46), ("x", 49)],
            vec![("y", 54), ("", 57), ("", 57)],
        ]
    );
    let cell_texts: Vec<(&str, bool)> = rows[0][0]
        .iter()
        .map(|i| (&source[i.source.clone()], i.exact()))
        .collect();
    assert_eq!(cell_texts, [("[[N\\|al]]", false)]);
    check(source, "table");
}

#[test]
fn list_markers_and_task_boxes() {
    let source = "- a\n- [ ] task\n  - nested\n    1. deep\n-\n- [x]\n- [/] custom\n\n3) three\n";
    let document = doc(source);
    let mut markers = Vec::new();
    for block in all_blocks(&document) {
        if let BlockKind::List { items, .. } = &block.kind {
            markers.extend(items.iter().map(|i| &source[i.marker.clone()]));
        }
    }
    assert_eq!(
        markers,
        ["- ", "- [ ] ", "-", "- [x]", "- [/] ", "- ", "1. ", "3) "]
    );
    let shown: Vec<&str> = texts(&document).iter().map(|t| t.0).collect();
    assert_eq!(shown, ["a", "task", "nested", "deep", "custom", "three"]);
    check(source, "lists");
}

#[test]
fn headings_code_and_frontmatter() {
    let source = "---\ntitle: T\n---\n\n## Two ##\n\nSetext\n===\n\n> Quoted\n> ---\n\n```rust\nfn x() {}\n\n```\n\n```\n```\n\n    indented\n    two\n\n> - ```\n>   q code\n>   ```\n";
    let document = doc(source);
    let frontmatter = document.frontmatter.as_ref().expect("frontmatter");
    assert_eq!(&source[frontmatter.bytes.clone()], "---\ntitle: T\n---");
    let mut markers = Vec::new();
    let mut contents = Vec::new();
    for block in all_blocks(&document) {
        match &block.kind {
            BlockKind::Heading { marker, .. } => markers.push(&source[marker.clone()]),
            BlockKind::CodeBlock { content, code, .. } => {
                contents.push((&source[content.clone()], code.as_str(), content.start))
            }
            _ => {}
        }
    }
    assert_eq!(markers, ["## ", "===", "---"]);
    assert_eq!(
        contents,
        [
            ("fn x() {}\n", "fn x() {}\n", 65),
            ("", "", 85),
            ("indented\n    two", "indented\ntwo", 94),
            ("q code", "q code", 124),
        ]
    );
    check(source, "headings and code");
}

#[test]
fn nested_callouts() {
    let source = "> [!warning]- Out **b**\n> body\n> > [!tip]+ In\n> > deep\n> after\n";
    let document = doc(source);
    let mut found = Vec::new();
    for block in all_blocks(&document) {
        if let BlockKind::Callout { marker, title, .. } = &block.kind {
            let title: Vec<&str> = title.iter().map(|i| &source[i.source.clone()]).collect();
            found.push((&source[marker.clone()], title));
        }
    }
    assert_eq!(
        found,
        [
            ("[!warning]-", vec!["Out ", "**b**"]),
            ("[!tip]+", vec!["In"])
        ]
    );
    let shown: Vec<&str> = texts(&document)
        .iter()
        .map(|t| &source[t.1.clone()])
        .collect();
    assert_eq!(shown, ["Out ", "b", "body", "In", "deep", "after"]);
    check(source, "callouts");
}

#[test]
fn tabs_details_and_directives_map_back() {
    let fenced = "````tabs\ntab: One\nfirst *x*\ntab: Two\n- item\n````\n\n> ```tabs\n> tab: Q\n> quoted\n> ```\n";
    let mdbook = "{{#tabs}}\n{{#tab name=\"A\"}}\nalpha %%c%% beta\n{{#endtab}}\n{{#tab name=\"B\"}}\n| h |\n|---|\n| v |\n{{#endtab}}\n{{#endtabs}}\n\nafter\n";
    let details = "<details>\n<summary>Sum *s*</summary>\n\nInside **b**\n</details>\n\n> <details><summary>Q</summary>\n> in quote\n> </details>\n\n<details><summary>One</summary>Whole</details>\n";
    let directives = "text before\n{{#include file.md}}\ntext after\n";
    let html = "<table><tr><th> Head *x* </th><td>b</td></tr><tr><td>c &amp; d</td><td></td></tr></table>\n\n<pre><code>a &lt; b</code></pre>\n";
    for (name, source, expected) in [
        ("fenced", fenced, vec!["first ", "x", "item", "quoted"]),
        ("mdbook", mdbook, vec!["alpha ", " beta", "h", "v", "after"]),
        (
            "details",
            details,
            vec!["Sum ", "s", "Inside ", "b", "Q", "in quote", "One", "Whole"],
        ),
        ("directives", directives, vec!["text before", "text after"]),
        ("html", html, vec!["Head ", "x", "b", "c ", " d"]),
    ] {
        let document = doc(source);
        let shown: Vec<&str> = texts(&document)
            .iter()
            .filter(|t| t.2)
            .map(|t| &source[t.1.clone()])
            .collect();
        assert_eq!(shown, expected, "{name}");
        check(source, name);
    }
    let document = doc(html);
    let BlockKind::Table { cells, .. } = &document.blocks[0].kind else {
        panic!("{:?}", document.blocks[0]);
    };
    let cells: Vec<Vec<&str>> = cells
        .iter()
        .map(|row| row.iter().map(|c| &html[c.clone()]).collect())
        .collect();
    assert_eq!(cells, [vec!["Head *x*", "b"], vec!["c &amp; d", ""]]);
    let BlockKind::CodeBlock { content, .. } = &document.blocks[1].kind else {
        panic!("{:?}", document.blocks[1]);
    };
    assert_eq!(&html[content.clone()], "a &lt; b");
}

#[test]
fn random_notes_keep_their_bytes() {
    const PIECES: &[&str] = &[
        "plain words",
        "**bold _it_**",
        "`co`de`",
        "``a`b``",
        "[[Note]]",
        "[[Note#Part|alias]]",
        "![[img.png|30]]",
        "![[Other note]]",
        "[md](a.md)",
        "<https://x.y>",
        "see https://a.example/p.",
        "#tag/sub",
        "\\#not",
        "==high== ",
        "\\==no==",
        "~~gone~~",
        "$x^2$",
        "[^n]",
        "a \\* b",
        "AT&amp;T &#35;x",
        "%%comment%%",
        "%%multi\nline%%",
        "<b>html</b>",
        "<kbd>K</kbd>",
        "<ip>",
        "^block-1",
        "{{#ref}}topics/x.md{{#endref}}",
        "عربي **غامق** نص",
        "中文 [[链接]]",
        "emoji \u{1F600} text",
    ];
    const BLOCKS: &[&str] = &[
        "# ",
        "## ",
        "- ",
        "- [ ] ",
        "- [/] ",
        "1. ",
        "> ",
        "> [!note] ",
        "> [!tip]- ",
        "| ",
        "",
        "",
    ];
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    for round in 0..1500 {
        let mut source = String::new();
        if next(5) == 0 {
            source.push_str("---\ntags: [a]\n---\n");
        }
        for _ in 0..1 + next(6) {
            let prefix = BLOCKS[next(BLOCKS.len())];
            source.push_str(prefix);
            for _ in 0..1 + next(4) {
                source.push_str(PIECES[next(PIECES.len())]);
                source.push(' ');
            }
            if prefix == "| " {
                source.push_str("| x \\| y |\n|---|---|\n| c |  |");
            }
            source.push_str(["\n", "\n\n", "\n> ", "\n  "][next(4)]);
            if next(8) == 0 {
                source.push_str("```\ncode %%c%%\n```\n");
            }
            if next(10) == 0 {
                source.push_str("<details><summary>S</summary>\n\nin *d*\n</details>\n\n");
            }
            if next(12) == 0 {
                source.push_str("````tabs\ntab: T\nin `tab`\n````\n");
            }
            if next(12) == 0 {
                source.push_str(
                    "{{#tabs}}\n{{#tab name=\"A\"}}\nmd **b**\n{{#endtab}}\n{{#endtabs}}\n",
                );
            }
        }
        if next(3) == 0 {
            source = source.replace('\n', "\r\n");
        }
        if next(6) == 0 {
            source.insert(0, '\u{feff}');
        }
        check(&source, &format!("round {round}: {source:?}"));
    }
}
