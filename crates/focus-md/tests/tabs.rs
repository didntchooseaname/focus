use focus_md::{
    Block, BlockKind, Directive, Document, InlineKind, ParseOptions, Tab, TabsForm, parse,
    plain_text,
};

fn doc(source: &str) -> Document {
    parse(source, &ParseOptions::default())
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn tabs_of(block: &Block) -> &[Tab] {
    match &block.kind {
        BlockKind::Tabs { tabs, .. } => tabs,
        other => panic!("not tabs: {other:?}"),
    }
}

fn titles(block: &Block) -> Vec<&str> {
    tabs_of(block).iter().map(|t| t.title.as_str()).collect()
}

fn texts(blocks: &[Block], out: &mut String) {
    for block in blocks {
        match &block.kind {
            BlockKind::Paragraph(i) | BlockKind::Heading { inlines: i, .. } => {
                out.push_str(&plain_text(i));
                out.push('\n');
            }
            BlockKind::CodeBlock { code, lang, .. } => {
                out.push_str(&format!("[{lang}]{code}\n"));
            }
            BlockKind::Quote(b)
            | BlockKind::Callout { blocks: b, .. }
            | BlockKind::Details { blocks: b, .. } => texts(b, out),
            BlockKind::List { items, .. } => items.iter().for_each(|i| texts(&i.blocks, out)),
            BlockKind::Tabs { tabs, .. } => {
                for t in tabs {
                    out.push_str(&format!("<{}>\n", t.title));
                    texts(&t.blocks, out);
                }
            }
            BlockKind::Directive(d) => out.push_str(&format!("{d:?}\n")),
            _ => {}
        }
    }
}

fn all_text(source: &str) -> String {
    let mut out = String::new();
    texts(&doc(source).blocks, &mut out);
    out
}

fn check(blocks: &[Block], parent: Option<&std::ops::Range<u32>>, lines: &[&str]) {
    let mut previous = parent.map_or(0, |p| p.start);
    for block in blocks {
        let r = &block.lines;
        assert!(
            r.start < r.end && r.end as usize <= lines.len(),
            "{r:?} out of the note"
        );
        assert!(
            r.start >= previous,
            "{r:?} overlaps what comes before ({previous})"
        );
        if let Some(p) = parent {
            assert!(r.start >= p.start && r.end <= p.end, "{r:?} leaves {p:?}");
        }
        assert!(
            !lines[r.start as usize].trim().is_empty()
                && !lines[r.end as usize - 1].trim().is_empty(),
            "{r:?} starts or ends on a blank line"
        );
        previous = r.end;
        match &block.kind {
            BlockKind::Quote(b)
            | BlockKind::Callout { blocks: b, .. }
            | BlockKind::Details { blocks: b, .. } => check(b, Some(r), lines),
            BlockKind::List { items, .. } => items
                .iter()
                .for_each(|i| check(&i.blocks, Some(&i.lines), lines)),
            BlockKind::Tabs { tabs, .. } => {
                let mut tab_end = r.start;
                for tab in tabs {
                    assert!(
                        tab.lines.start >= tab_end && tab.lines.end <= r.end,
                        "tab {:?} leaves {r:?}",
                        tab.lines
                    );
                    assert!(tab.title_line >= tab.lines.start && tab.title_line < tab.lines.end);
                    tab_end = tab.lines.end;
                    check(&tab.blocks, Some(&tab.lines), lines);
                }
            }
            _ => {}
        }
    }
}

fn line_of(lines: &[&str], text: &str) -> u32 {
    lines
        .iter()
        .position(|l| *l == text)
        .unwrap_or_else(|| panic!("{text:?} is not in the fixture")) as u32
}

#[test]
fn the_fenced_form_in_a_note_gives_groups_with_their_content() {
    let source = fixture("tabs-fenced.md");
    let lines: Vec<&str> = source.lines().collect();
    let d = doc(&source);
    let groups: Vec<&Block> = d
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Tabs { .. }))
        .collect();
    assert_eq!(groups.len(), 3);
    assert!(groups.iter().all(|g| matches!(
        &g.kind,
        BlockKind::Tabs {
            form: TabsForm::Fenced,
            ..
        }
    )));
    assert_eq!(titles(groups[0]), ["Linux", "Windows"]);
    assert_eq!(
        titles(groups[1]),
        ["Linux", "Windows"],
        "the `== Title` form, in a fence indented by a space"
    );
    assert_eq!(
        titles(groups[2]),
        ["", "Server", "Client"],
        "text before the first tab line is a tab with no title"
    );

    let open = line_of(&lines, "````tabs");
    let close = lines
        .iter()
        .enumerate()
        .skip(open as usize + 1)
        .find(|(_, l)| **l == "````")
        .map(|(i, _)| i as u32)
        .unwrap();
    assert_eq!(groups[0].lines, open..close + 1, "fences included");
    let first = tabs_of(groups[0]);
    let windows = line_of(&lines, "tab: Windows");
    assert_eq!(
        first[0].lines,
        line_of(&lines, "tab: Linux")..windows,
        "from the title line to the next title"
    );
    assert_eq!(
        first[1].lines,
        windows..close,
        "up to the line before the closing fence"
    );
    assert_eq!(first[0].title_line, first[0].lines.start);
    let kinds: Vec<&str> = first[0]
        .blocks
        .iter()
        .map(|b| match &b.kind {
            BlockKind::Paragraph(_) => "paragraph",
            BlockKind::CodeBlock { .. } => "code",
            BlockKind::Callout { .. } => "callout",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "paragraph",
            "code",
            "callout",
            "paragraph",
            "code",
            "paragraph"
        ]
    );
    assert!(matches!(&first[0].blocks[2].kind, BlockKind::Callout { kind, .. } if kind == "tip"));
    assert!(
        matches!(&first[0].blocks[1].kind, BlockKind::CodeBlock { lang, .. } if lang == "bash")
    );
    check(&d.blocks, None, &lines);
    let block = &first[0].blocks[1];
    assert!(
        lines[block.lines.start as usize].starts_with("```"),
        "a block in a tab is found by its line in the note"
    );

    let outer = tabs_of(groups[2]);
    assert_eq!(titles(&outer[1].blocks[0]), ["Inner A", "Inner B"]);
    assert!(
        matches!(&outer[2].blocks[0].kind, BlockKind::CodeBlock { code, .. } if code.contains("tab: this line is code"))
    );
}

#[test]
fn the_mdbook_form_in_a_note_gives_tabs_directives_and_nesting() {
    let source = fixture("tabs-mdbook.md");
    let lines: Vec<&str> = source.lines().collect();
    let d = doc(&source);
    let includes: Vec<_> = d
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Directive(Directive::Include { .. })))
        .collect();
    assert_eq!(includes.len(), 2);
    assert!(
        matches!(&includes[0].kind, BlockKind::Directive(Directive::Include { path }) if path == "../shared/banner.md")
    );
    assert_eq!(includes[0].lines, 2..3);
    let groups: Vec<&Block> = d
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Tabs { .. }))
        .collect();
    assert_eq!(groups.len(), 2);
    assert!(groups.iter().all(|g| matches!(
        &g.kind,
        BlockKind::Tabs {
            form: TabsForm::Mdbook,
            ..
        }
    )));
    assert_eq!(
        titles(groups[0]),
        ["Rust", "C"],
        "a space before }} and single quotes are fine"
    );
    let open = line_of(&lines, "{{#tabs }}");
    let close = line_of(&lines, "{{#endtabs}}");
    assert_eq!(
        groups[0].lines,
        open..close + 1,
        "from {{#tabs}} to {{#endtabs}}"
    );
    let tabs = tabs_of(groups[0]);
    assert_eq!(
        tabs[0].lines,
        open + 1..line_of(&lines, "{{#endtab }}") + 1,
        "{{#endtab}} is part of its tab"
    );
    assert!(matches!(&tabs[0].blocks[0].kind, BlockKind::CodeBlock { lang, .. } if lang == "rust"));
    assert_eq!(titles(groups[1]), ["Outer"]);
    assert_eq!(
        titles(&tabs_of(groups[1])[0].blocks[0]),
        ["Inner"],
        "five spaces before the closing directives change nothing"
    );
    check(&d.blocks, None, &lines);

    let refs: Vec<&Directive> = d
        .blocks
        .iter()
        .filter_map(|b| {
            if let BlockKind::Directive(d) = &b.kind {
                Some(d)
            } else {
                None
            }
        })
        .collect();
    assert!(refs.contains(&&Directive::Ref {
        path: "../topics/overflow.md".into()
    }));
    assert!(refs.contains(&&Directive::Other {
        name: "file".into(),
        text: "sample.bin".into()
    }));
    let BlockKind::List { items, .. } = &d
        .blocks
        .iter()
        .find(|b| matches!(b.kind, BlockKind::List { .. }))
        .unwrap()
        .kind
    else {
        panic!()
    };
    assert!(items[0].blocks.iter().any(|b| matches!(&b.kind, BlockKind::Directive(Directive::Ref { path }) if path == "../topics/lists.md")), "a directive indented in a list item");
    let inline = d.blocks.iter().find_map(|b| match &b.kind {
        BlockKind::Paragraph(i) if plain_text(i).contains("inside it") => Some(i),
        _ => None,
    });
    assert!(
        inline.is_some_and(|i| i.iter().any(
            |x| matches!(&x.kind, InlineKind::Link(l) if l.target == "../topics/wrapping.md")
        )),
        "a ref in a sentence is a link"
    );
}

#[test]
fn nested_fences_and_closing_rules_follow_commonmark() {
    let d =
        doc("````tabs\ntab: A\n\n```bash\ntab: not a tab\n```\n\ntab: B\n\ntext\n````\n\nafter\n");
    assert_eq!(d.blocks.len(), 2);
    assert_eq!(titles(&d.blocks[0]), ["A", "B"]);
    let a = &tabs_of(&d.blocks[0])[0];
    assert!(
        matches!(&a.blocks[0].kind, BlockKind::CodeBlock { code, .. } if code.contains("tab: not a tab"))
    );
    let d = doc("~~~tabs\ntab: T\n\nx\n~~~\n");
    assert_eq!(titles(&d.blocks[0]), ["T"]);
    let d = doc("`````tabs\ntab: Outer\n\n````tabs\ntab: Inner\n\nleaf\n````\n\n`````\n");
    let outer = tabs_of(&d.blocks[0]);
    assert_eq!(outer.len(), 1);
    assert_eq!(titles(&outer[0].blocks[0]), ["Inner"]);
}

#[test]
fn a_tabs_fence_without_a_tab_line_stays_code_and_text_before_a_tab_is_kept() {
    let d = doc("```tabs\njust text\n```\n");
    assert!(matches!(&d.blocks[0].kind, BlockKind::CodeBlock { lang, .. } if lang == "tabs"));
    let d = doc("```tabs\nintro line\n\ntab: One\n\nbody\n```\n");
    assert_eq!(titles(&d.blocks[0]), ["", "One"]);
    assert!(all_text("```tabs\nintro line\n\ntab: One\n\nbody\n```\n").contains("intro line"));
}

#[test]
fn an_unclosed_fence_keeps_every_line() {
    let text = all_text("````tabs\ntab: One\n\nfirst words\n\ntab: Two\n\nlast words\n");
    assert!(
        text.contains("<One>")
            && text.contains("first words")
            && text.contains("<Two>")
            && text.contains("last words"),
        "{text}"
    );
}

#[test]
fn mdbook_directives_accept_spaces_and_indentation_and_nest() {
    let source = "{{#tabs }}\n{{#tab name=\"One\" }}\ntext one\n{{#endtab }}\n\n{{#tab name='Two'}}\n{{#tabs}}\n{{#tab name=\"Inner\"}}\ndeep\n{{#endtab}}\n{{#endtabs}}\n     {{#endtab }}\n     {{#endtabs }}\n";
    let d = doc(source);
    assert_eq!(
        d.blocks.len(),
        1,
        "{:?}",
        d.blocks.iter().map(|b| &b.kind).collect::<Vec<_>>()
    );
    assert_eq!(titles(&d.blocks[0]), ["One", "Two"]);
    let two = &tabs_of(&d.blocks[0])[1];
    assert_eq!(titles(&two.blocks[0]), ["Inner"]);
    assert!(all_text(source).contains("deep"));
    let lines: Vec<&str> = source.lines().collect();
    check(&d.blocks, None, &lines);
}

#[test]
fn a_form_that_is_not_closed_is_not_tabs_and_loses_nothing() {
    let source = "{{#tabs}}\n{{#tab name=\"One\"}}\nbody text\n{{#endtab}}\n\nafter the group\n";
    let d = doc(source);
    assert!(
        d.blocks
            .iter()
            .all(|b| !matches!(b.kind, BlockKind::Tabs { .. }))
    );
    let text = all_text(source);
    assert!(
        text.contains("body text") && text.contains("after the group"),
        "{text}"
    );
    assert!(
        text.contains("\"tabs\"") && text.contains("\"endtab\""),
        "the directive lines are still there: {text}"
    );
}

#[test]
fn directives_inside_code_are_code() {
    let d = doc(
        "```md\n{{#tabs}}\n{{#tab name=\"A\"}}\nx\n{{#endtab}}\n{{#endtabs}}\n{{#include a.md}}\n```\n",
    );
    assert_eq!(d.blocks.len(), 1);
    assert!(matches!(&d.blocks[0].kind, BlockKind::CodeBlock { .. }));
    let d = doc("````tabs\ntab: A\n\n```\n{{#tabs}}\n```\n````\n");
    assert!(matches!(
        &tabs_of(&d.blocks[0])[0].blocks[0].kind,
        BlockKind::CodeBlock { .. }
    ));
}

#[test]
fn the_content_of_a_tab_is_a_note_of_its_own_without_frontmatter_and_with_footnotes() {
    let source = "intro[^a]\n\n````tabs\ntab: One\n\n---\nnot: frontmatter\n---\n\nin a tab[^b]\n\n[^b]: defined in the tab\n\ntab: Two\n\n- [ ] task\n````\n\n[^a]: outer\n";
    let d = doc(source);
    assert!(d.frontmatter.is_none());
    assert_eq!(
        d.footnotes.len(),
        2,
        "the definition inside a tab joins the note's"
    );
    assert_eq!(d.footnote_refs.len(), 2);
    let labels: Vec<_> = d
        .footnotes
        .iter()
        .map(|f| (f.number, f.label.as_str()))
        .collect();
    assert_eq!(
        labels,
        [(1, "a"), (2, "b")],
        "numbered in order of the references, tabs included"
    );
    let one = &tabs_of(&d.blocks[1])[0];
    assert!(
        matches!(one.blocks[0].kind, BlockKind::Rule),
        "{:?}",
        one.blocks[0].kind
    );
}

#[test]
fn tabs_inside_containers() {
    let d = doc(
        "> [!note]\n> ````tabs\n> tab: In a callout\n>\n> body\n> ````\n\n- item\n\n  ````tabs\n  tab: In a list\n\n  body\n  ````\n",
    );
    let BlockKind::Callout { blocks, .. } = &d.blocks[0].kind else {
        panic!()
    };
    assert_eq!(titles(&blocks[0]), ["In a callout"]);
    let BlockKind::List { items, .. } = &d.blocks[1].kind else {
        panic!()
    };
    assert_eq!(titles(&items[0].blocks[1]), ["In a list"]);
}

fn directive(block: &Block) -> &Directive {
    match &block.kind {
        BlockKind::Directive(d) => d,
        other => panic!("not a directive: {other:?}"),
    }
}

#[test]
fn refs_includes_and_other_directives() {
    let source = "before\n{{#ref}}\n../generic/phishing.md\n{{#endref}}\nafter\n\n{{#ref}}one-line.md#Part{{#endref}}\n\n{{#include ../banners/x.md}}\n\n{{#file}}\nreverse.tar.gz\n{{#endfile}}\n\n{{#with name}}\n\n{{#include a.rs:2:10 }}\n";
    let d = doc(source);
    let kinds: Vec<String> = d
        .blocks
        .iter()
        .map(|b| format!("{:?}", b.kind).chars().take(24).collect())
        .collect();
    assert_eq!(d.blocks.len(), 8, "{kinds:?}");
    assert!(matches!(&d.blocks[0].kind, BlockKind::Paragraph(_)));
    assert_eq!(d.blocks[0].lines, 0..1);
    assert_eq!(
        directive(&d.blocks[1]),
        &Directive::Ref {
            path: "../generic/phishing.md".into()
        }
    );
    assert_eq!(d.blocks[1].lines, 1..4, "the three lines of the directive");
    assert!(matches!(&d.blocks[2].kind, BlockKind::Paragraph(_)));
    assert_eq!(d.blocks[2].lines, 4..5);
    assert_eq!(
        directive(&d.blocks[3]),
        &Directive::Ref {
            path: "one-line.md#Part".into()
        }
    );
    assert_eq!(
        directive(&d.blocks[4]),
        &Directive::Include {
            path: "../banners/x.md".into()
        }
    );
    assert_eq!(
        directive(&d.blocks[5]),
        &Directive::Other {
            name: "file".into(),
            text: "reverse.tar.gz".into()
        }
    );
    assert_eq!(d.blocks[5].lines, 10..13);
    assert_eq!(
        directive(&d.blocks[6]),
        &Directive::Other {
            name: "with".into(),
            text: "name".into()
        }
    );
    assert_eq!(
        directive(&d.blocks[7]),
        &Directive::Include {
            path: "a.rs:2:10".into()
        }
    );
    let lines: Vec<&str> = source.lines().collect();
    check(&d.blocks, None, &lines);
}

#[test]
fn a_ref_inside_a_sentence_is_a_link_and_directives_work_in_containers() {
    let d = doc("see {{#ref}}../persistence/accountpersistence.md{{#endref}} for details\n");
    let BlockKind::Paragraph(inlines) = &d.blocks[0].kind else {
        panic!()
    };
    let link = inlines
        .iter()
        .find_map(|i| {
            if let InlineKind::Link(l) = &i.kind {
                Some(l)
            } else {
                None
            }
        })
        .expect("a link");
    assert_eq!(link.target, "../persistence/accountpersistence.md");
    assert_eq!(plain_text(&link.children), "accountpersistence");
    let d = doc("> [!tip]\n> {{#ref}}\n> a/b.md\n> {{#endref}}\n\n- item\n  {{#include c.md}}\n");
    let BlockKind::Callout { blocks, .. } = &d.blocks[0].kind else {
        panic!()
    };
    assert_eq!(
        directive(&blocks[0]),
        &Directive::Ref {
            path: "a/b.md".into()
        }
    );
    assert_eq!(blocks[0].lines, 1..4);
    let BlockKind::List { items, .. } = &d.blocks[1].kind else {
        panic!()
    };
    assert!(items[0].blocks.iter().any(
        |b| matches!(&b.kind, BlockKind::Directive(Directive::Include { path }) if path == "c.md")
    ));
}

#[test]
fn details_around_and_inside_tabs_keep_their_lines() {
    let source = "<details>\n<summary>Fold</summary>\n\nbefore\n\n````tabs\ntab: In a fold\n\nbody\n````\n\n</details>\n";
    let d = doc(source);
    let BlockKind::Details { blocks, .. } = &d.blocks[0].kind else {
        panic!("{:?}", d.blocks[0].kind)
    };
    assert_eq!(titles(&blocks[1]), ["In a fold"]);
    let lines: Vec<&str> = source.lines().collect();
    check(&d.blocks, None, &lines);
}

#[test]
fn the_older_equals_form_of_a_tab_line_is_read_too() {
    let d = doc("```tabs\n\n== UNIX-like\n\nfirst\n\n== Windows \u{1F5A5}\n\nsecond\n```\n");
    assert_eq!(titles(&d.blocks[0]), ["UNIX-like", "Windows \u{1F5A5}"]);
}
