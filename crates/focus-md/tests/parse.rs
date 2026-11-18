use focus_md::{Align, BlockKind, Inline, InlineKind, LinkKind, ParseOptions, parse, plain_text};

fn doc(src: &str) -> focus_md::Document {
    parse(src, &ParseOptions::default())
}

fn first_inlines(src: &str) -> Vec<Inline> {
    match doc(src).blocks.into_iter().next().map(|b| b.kind) {
        Some(BlockKind::Paragraph(i)) => i,
        other => panic!("paragraphe attendu, obtenu {other:?}"),
    }
}

#[test]
fn heading_lines_and_id() {
    let d = doc("intro\n\n## Mon **titre** ^ancre\n\nsuite\n");
    assert_eq!(d.line_count, 5);
    let h = &d.blocks[1];
    assert_eq!(h.lines, 2..3);
    let BlockKind::Heading {
        level, id, inlines, ..
    } = &h.kind
    else {
        panic!()
    };
    assert_eq!(*level, 2);
    assert_eq!(id, "Mon titre");
    assert!(
        matches!(inlines.last().map(|x| &x.kind), Some(InlineKind::BlockId(b)) if b == "ancre")
    );
    assert_eq!(d.blocks[2].lines, 4..5);
}

#[test]
fn wiki_links() {
    let i = first_inlines("[[Note#Section|alias]] [[Autre]] [[#loc]] [[n#^bloc]]");
    let links: Vec<_> = i
        .iter()
        .filter_map(|x| {
            if let InlineKind::Link(l) = &x.kind {
                Some(l)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(links.len(), 4);
    assert_eq!(
        (links[0].target.as_str(), links[0].anchor.as_deref()),
        ("Note", Some("Section"))
    );
    assert_eq!(plain_text(&links[0].children), "alias");
    assert_eq!(links[0].kind, LinkKind::Wiki);
    assert_eq!(plain_text(&links[1].children), "Autre");
    assert_eq!(
        (links[2].target.as_str(), links[2].anchor.as_deref()),
        ("", Some("loc"))
    );
    assert_eq!(links[3].anchor.as_deref(), Some("^bloc"));
}

#[test]
fn markdown_and_bare_links() {
    let i = first_inlines(
        "[a](x/y.md#s) <https://e.org> voir https://e.org/p?q=1#f, puis (https://z.fr).",
    );
    let links: Vec<_> = i
        .iter()
        .filter_map(|x| {
            if let InlineKind::Link(l) = &x.kind {
                Some(l)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(links.len(), 4);
    assert_eq!(
        (
            links[0].kind,
            links[0].target.as_str(),
            links[0].anchor.as_deref()
        ),
        (LinkKind::Markdown, "x/y.md", Some("s"))
    );
    assert_eq!(links[1].kind, LinkKind::Auto);
    assert_eq!(links[2].target, "https://e.org/p?q=1#f");
    assert_eq!(links[3].target, "https://z.fr");
    assert!(
        !i.iter().any(|x| matches!(&x.kind, InlineKind::Tag(_))),
        "le fragment d'URL n'est pas un tag"
    );
}

#[test]
fn embeds_and_images() {
    let d = doc(
        "![[note#sec]]\n![[img.png|300]]\n![[b^blk]]\n\n![alt|200x100](a/b.png)\n\ntexte ![[i.jpg]] fin\n",
    );
    let kinds: Vec<_> = d.blocks.iter().map(|b| &b.kind).collect();
    assert!(
        matches!(kinds[0], BlockKind::Embed { target, anchor } if target == "note" && anchor.as_deref() == Some("sec"))
    );
    assert!(matches!(kinds[1], BlockKind::Image(i) if i.src == "img.png" && i.width == Some(300)));
    assert!(
        matches!(kinds[2], BlockKind::Embed { target, anchor } if target == "b" && anchor.as_deref() == Some("^blk"))
    );
    let lines: Vec<_> = d.blocks.iter().take(3).map(|b| b.lines.clone()).collect();
    assert_eq!(lines, [0..1, 1..2, 2..3]);
    assert!(
        matches!(kinds[3], BlockKind::Image(i) if i.src == "a/b.png" && i.alt == "alt" && i.width == Some(200) && i.height == Some(100))
    );
    let BlockKind::Paragraph(p) = &d.blocks[4].kind else {
        panic!()
    };
    assert!(
        p.iter()
            .any(|x| matches!(&x.kind, InlineKind::Image(i) if i.src == "i.jpg"))
    );
}

#[test]
fn callouts() {
    let d = doc(
        "> [!warning]+ Titre **gras**\n> corps\n\n> [!note]\n> seul\n\n> [!Danger]- \n> x\n\n> citation\n",
    );
    let BlockKind::Callout {
        kind,
        title,
        fold,
        blocks,
        ..
    } = &d.blocks[0].kind
    else {
        panic!("{:?}", d.blocks[0])
    };
    assert_eq!((kind.as_str(), *fold), ("warning", Some(true)));
    assert_eq!(plain_text(title), "Titre gras");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].lines, 1..2);
    assert_eq!(d.blocks[0].lines, 0..2);
    let BlockKind::Callout {
        kind, title, fold, ..
    } = &d.blocks[1].kind
    else {
        panic!()
    };
    assert_eq!((kind.as_str(), title.len(), *fold), ("note", 0, None));
    let BlockKind::Callout { kind, fold, .. } = &d.blocks[2].kind else {
        panic!()
    };
    assert_eq!((kind.as_str(), *fold), ("danger", Some(false)));
    assert!(matches!(d.blocks[3].kind, BlockKind::Quote(_)));
}

#[test]
fn callout_with_nested_blocks() {
    let d = doc("> [!example] Ex\n> - a\n> - b\n>\n> ```bash\n> echo\n> ```\n");
    let BlockKind::Callout { blocks, .. } = &d.blocks[0].kind else {
        panic!()
    };
    assert!(matches!(blocks[0].kind, BlockKind::List { .. }));
    assert!(
        matches!(&blocks[1].kind, BlockKind::CodeBlock { lang, code, .. } if lang == "bash" && code == "echo")
    );
    assert_eq!(blocks[1].lines, 4..7);
}

#[test]
fn highlight_tags_math() {
    let i = first_inlines("un ==mot **gras**== et #a/b-c, #123 pas tag, \\#non, $x^2$, `#code` ==");
    assert!(
        i.iter()
            .any(|x| matches!(&x.kind, InlineKind::Highlight(c) if plain_text(c) == "mot gras"))
    );
    assert!(
        i.iter()
            .any(|x| matches!(&x.kind, InlineKind::Tag(t) if t == "a/b-c"))
    );
    assert!(
        !i.iter().any(
            |x| matches!(&x.kind, InlineKind::Tag(t) if t == "123" || t == "non" || t == "code")
        )
    );
    assert!(
        i.iter()
            .any(|x| matches!(&x.kind, InlineKind::Math(m) if m == "x^2"))
    );
    let plain = plain_text(&i);
    assert!(plain.contains("#non"), "{plain}");
}

#[test]
fn display_math_block() {
    let d = doc("avant\n\n$$\nE = mc^2\n$$\n\naprès");
    assert!(matches!(&d.blocks[1].kind, BlockKind::Math { tex } if tex == "E = mc^2"));
    assert_eq!(d.blocks[1].lines, 2..5);
}

#[test]
fn comments_removed_lines_kept() {
    let d =
        doc("a %% caché %% b\n\n%% multi\nligne %%\n\nc `%% code %%`\n\n```\n%% garde %%\n```\n");
    assert_eq!(plain_text(&first_inlines("a %% caché %% b")), "a  b");
    assert_eq!(d.blocks.len(), 3, "{:?}", d.blocks);
    assert_eq!(d.blocks[1].lines, 5..6);
    let BlockKind::Paragraph(p) = &d.blocks[1].kind else {
        panic!()
    };
    assert!(plain_text(p).contains("%% code %%"));
    assert!(
        matches!(&d.blocks[2].kind, BlockKind::CodeBlock { code, .. } if code == "%% garde %%")
    );
    assert_eq!(
        plain_text(&first_inlines("a %% jamais fermé")),
        "a %% jamais fermé"
    );
}

#[test]
fn block_id_trailing() {
    let i = first_inlines("fin de ligne. ^bloc-1");
    assert!(matches!(i.last().map(|x| &x.kind), Some(InlineKind::BlockId(b)) if b == "bloc-1"));
    assert_eq!(plain_text(&i), "fin de ligne.");
}

#[test]
fn tasks_and_lists() {
    let d = doc("- [ ] a\n- [x] b\n- [/] c\n- [-] d\n- [1] e\n\n1. un\n2. deux\n   - sub\n");
    let BlockKind::List {
        ordered,
        tight,
        items,
        ..
    } = &d.blocks[0].kind
    else {
        panic!()
    };
    assert!(!ordered && *tight);
    let tasks: Vec<_> = items.iter().map(|i| i.task).collect();
    assert_eq!(
        tasks,
        [Some(' '), Some('x'), Some('/'), Some('-'), Some('1')]
    );
    assert_eq!(items[2].lines, 2..3);
    let BlockKind::Paragraph(p) = &items[2].blocks[0].kind else {
        panic!()
    };
    assert_eq!(plain_text(p), "c");
    let BlockKind::List {
        ordered,
        start,
        items,
        ..
    } = &d.blocks[1].kind
    else {
        panic!()
    };
    assert!(*ordered && *start == 1);
    assert!(matches!(items[1].blocks[1].kind, BlockKind::List { .. }));
    assert_eq!(items[1].blocks[1].lines, 8..9);
}

#[test]
fn loose_list() {
    let d = doc("- a\n\n- b\n");
    let BlockKind::List { tight, .. } = &d.blocks[0].kind else {
        panic!()
    };
    assert!(!tight);
}

#[test]
fn table_alignment() {
    let d = doc("| a | b |\n|:--|--:|\n| 1 | `x` |\n| 2 | 3 |\n");
    let BlockKind::Table {
        align, head, rows, ..
    } = &d.blocks[0].kind
    else {
        panic!()
    };
    assert_eq!(align, &[Align::Left, Align::Right]);
    assert_eq!(head.len(), 2);
    assert_eq!(rows.len(), 2);
    assert!(matches!(rows[0][1][0].kind, InlineKind::Code(_)));
    assert_eq!(d.blocks[0].lines, 0..4);
}

#[test]
fn code_and_mermaid() {
    let d = doc("```Bash -x\nls\n```\n\n```mermaid\ngraph TD\nA-->B\n```\n\n    indenté\n");
    assert!(
        matches!(&d.blocks[0].kind, BlockKind::CodeBlock { lang, code, .. } if lang == "bash" && code == "ls")
    );
    assert!(
        matches!(&d.blocks[1].kind, BlockKind::Mermaid { source } if source == "graph TD\nA-->B")
    );
    assert_eq!(d.blocks[1].lines, 4..8);
    assert!(matches!(&d.blocks[2].kind, BlockKind::CodeBlock { lang, .. } if lang.is_empty()));
}

#[test]
fn frontmatter_and_footnotes() {
    let d = doc("---\ntags: [a]\n---\n\ntexte[^b] et[^a]\n\n[^a]: A\n[^b]: B\n");
    let fm = d.frontmatter.as_ref().unwrap();
    assert_eq!(fm.raw, "tags: [a]");
    assert_eq!(fm.lines, 0..3);
    let labels: Vec<_> = d.footnotes.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(labels, ["b", "a"]);
    assert_eq!(d.blocks.len(), 1);
}

#[test]
fn strike_rule_html_quote() {
    let d = doc("~~x~~\n\n---\n\n<div>h</div>\n\n> q\n");
    assert!(
        matches!(&d.blocks[0].kind, BlockKind::Paragraph(p) if matches!(p[0].kind, InlineKind::Strike(_)))
    );
    assert!(matches!(d.blocks[1].kind, BlockKind::Rule));
    assert!(matches!(&d.blocks[2].kind, BlockKind::Html(h) if h == "<div>h</div>"));
    assert!(matches!(d.blocks[3].kind, BlockKind::Quote(_)));
}

#[test]
fn break_options() {
    let hard = parse("a\nb", &ParseOptions { hard_breaks: true });
    let soft = parse("a\nb", &ParseOptions { hard_breaks: false });
    assert!(
        matches!(&hard.blocks[0].kind, BlockKind::Paragraph(p) if matches!(p[1].kind, InlineKind::HardBreak))
    );
    assert!(
        matches!(&soft.blocks[0].kind, BlockKind::Paragraph(p) if matches!(p[1].kind, InlineKind::SoftBreak))
    );
}

#[test]
fn never_panics_on_odd_input() {
    let samples = [
        "",
        "\n",
        "%%",
        "%% %% %%",
        "---\n",
        "---\n---\n",
        "[[",
        "![[",
        "[[]]",
        "![[]]",
        "==",
        "== ==",
        "====",
        "> [!",
        "> [!]",
        "> [!x",
        "#",
        "# ",
        "^",
        "^id",
        "$$",
        "$",
        "$$$$",
        "[^1]",
        "[^1]: ",
        "|a|\n|-|\n",
        "- [",
        "- [/",
        "- [/]",
        "é==é==é",
        "日本語 #標籤 ==強調==",
        "\u{E000}\u{E001}",
        "```",
        "~~~mermaid",
        "http://",
        "https://a",
        "(https://a))",
        "\r\n\r\n- a\r\n- b\r\n",
    ];
    for s in samples {
        let d = doc(s);
        let _ = d.line_count;
    }
}

#[test]
fn block_id_after_non_ascii_space() {
    let i = first_inlines("mot\u{a0}^id");
    assert!(matches!(i.last().map(|x| &x.kind), Some(InlineKind::BlockId(b)) if b == "id"));
}

#[test]
fn task_states_any_single_character() {
    let d = doc("- [?] a\n- [!] b\n- [>] c\n- [I] d\n- [ ] e\n- [x] f\n- [lien](x) g\n- [ab] h\n");
    let BlockKind::List { items, .. } = &d.blocks[0].kind else {
        panic!()
    };
    let tasks: Vec<_> = items.iter().map(|i| i.task).collect();
    assert_eq!(
        tasks,
        [
            Some('?'),
            Some('!'),
            Some('>'),
            Some('I'),
            Some(' '),
            Some('x'),
            None,
            None
        ]
    );
}

fn details_of(block: &focus_md::Block) -> (&Vec<Inline>, bool, &Vec<focus_md::Block>) {
    match &block.kind {
        BlockKind::Details {
            summary,
            open,
            blocks,
        } => (summary, *open, blocks),
        other => panic!("Details attendu, obtenu {other:?}"),
    }
}

#[test]
fn details_with_blank_lines() {
    let d = doc(
        "avant\n\n<details>\n<summary>Le **titre**</summary>\n\nContenu *markdown*\n\n```bash\nid\n```\n\n</details>\n\naprès\n",
    );
    assert_eq!(d.blocks.len(), 3);
    let (summary, open, blocks) = details_of(&d.blocks[1]);
    assert!(!open);
    assert_eq!(plain_text(summary), "Le titre");
    assert!(
        matches!(&blocks[0].kind, BlockKind::Paragraph(p) if p.iter().any(|i| matches!(&i.kind, InlineKind::Emphasis(_))))
    );
    assert!(matches!(&blocks[1].kind, BlockKind::CodeBlock { lang, .. } if lang == "bash"));
    assert_eq!(d.blocks[1].lines, 2..12);
}

#[test]
fn details_single_block_open_attribute() {
    let d = doc("<details open>\n<summary>S</summary>\nligne **a**\n</details>\n");
    let (summary, open, blocks) = details_of(&d.blocks[0]);
    assert!(open);
    assert_eq!(plain_text(summary), "S");
    assert!(matches!(&blocks[0].kind, BlockKind::Paragraph(_)));
    assert_eq!(blocks[0].lines, 2..3);
}

#[test]
fn details_nested_and_in_containers() {
    let src = "<details>\n<summary>Externe</summary>\n\n<details>\n<summary>Interne</summary>\n\ntexte\n\n</details>\n\n- a\n\n</details>\n\n> <details>\n> <summary>Dans citation</summary>\n> corps\n> </details>\n";
    let d = doc(src);
    let (_, _, outer) = details_of(&d.blocks[0]);
    let (inner_summary, _, inner) = details_of(&outer[0]);
    assert_eq!(plain_text(inner_summary), "Interne");
    assert!(matches!(inner[0].kind, BlockKind::Paragraph(_)));
    assert!(matches!(outer[1].kind, BlockKind::List { .. }));
}

#[test]
fn details_unclosed_stays_html() {
    let d = doc("<details>\n<summary>S</summary>\n\ntexte sans fermeture\n");
    assert!(matches!(d.blocks[0].kind, BlockKind::Html(_)));
    let d = doc("<details><summary>S</summary>x</details> trop\n");
    assert!(matches!(d.blocks[0].kind, BlockKind::Html(_)));
}

#[test]
fn callout_kind_families_and_labels() {
    use focus_md::callout_kind;
    let families = [
        ("note", "note"),
        ("abstract", "abstract"),
        ("summary", "abstract"),
        ("tldr", "abstract"),
        ("info", "info"),
        ("todo", "todo"),
        ("tip", "tip"),
        ("hint", "tip"),
        ("important", "tip"),
        ("success", "success"),
        ("check", "success"),
        ("done", "success"),
        ("question", "question"),
        ("help", "question"),
        ("faq", "question"),
        ("warning", "warning"),
        ("caution", "warning"),
        ("attention", "warning"),
        ("failure", "failure"),
        ("fail", "failure"),
        ("missing", "failure"),
        ("danger", "danger"),
        ("error", "danger"),
        ("bug", "bug"),
        ("example", "example"),
        ("quote", "quote"),
        ("cite", "quote"),
        ("WARNING", "warning"),
    ];
    for (kind, family) in families {
        let (found, label) = callout_kind(kind);
        assert_eq!(found, family, "{kind}");
        assert!(!label.is_empty());
    }
    assert_eq!(callout_kind("tip").1, "Tip");
    assert_eq!(callout_kind("todo").1, "To do");
    assert_eq!(callout_kind("Inconnu"), ("note", "Inconnu".to_string()));
}

#[test]
fn callout_nested_and_rich_content() {
    let src = "> [!note] Externe\n> Texte\n>\n> > [!warning]- Interne\n> > Corps interne\n> > - point\n>\n> | a | b |\n> |---|---|\n> | 1 | 2 |\n>\n> ```rust\n> fn x() {}\n> ```\n>\n> 1. un\n> 2. deux\n";
    let d = doc(src);
    let BlockKind::Callout { kind, blocks, .. } = &d.blocks[0].kind else {
        panic!("{:?}", d.blocks[0])
    };
    assert_eq!(kind, "note");
    assert!(matches!(&blocks[0].kind, BlockKind::Paragraph(_)));
    let BlockKind::Callout {
        kind,
        fold,
        blocks: inner,
        ..
    } = &blocks[1].kind
    else {
        panic!("{:?}", blocks[1])
    };
    assert_eq!((kind.as_str(), *fold), ("warning", Some(false)));
    assert!(matches!(inner[0].kind, BlockKind::Paragraph(_)));
    assert!(matches!(inner[1].kind, BlockKind::List { .. }));
    assert!(matches!(blocks[2].kind, BlockKind::Table { .. }));
    assert!(
        matches!(&blocks[3].kind, BlockKind::CodeBlock { lang, code, .. } if lang == "rust" && code == "fn x() {}")
    );
    assert!(matches!(
        &blocks[4].kind,
        BlockKind::List { ordered: true, .. }
    ));
}

#[test]
fn html_comments_hidden_and_markers_are_text() {
    let d = doc(
        "<!-- vault:summary:start -->\n\ntexte avec <ip> et <token> et <br> et <!-- c --> fin\n\n<!-- a -->\n<!-- b -->\n\n<figure>x</figure>\n",
    );
    assert_eq!(d.blocks.len(), 2, "{:?}", d.blocks);
    let BlockKind::Paragraph(p) = &d.blocks[0].kind else {
        panic!()
    };
    assert_eq!(
        plain_text(p)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        "texte avec <ip> et <token> et et fin"
    );
    assert!(p.iter().any(|i| matches!(&i.kind, InlineKind::HardBreak)));
    assert!(matches!(&d.blocks[1].kind, BlockKind::Html(h) if h.starts_with("<figure>")));
}

#[test]
fn html_figure_img_pre_table_become_model_blocks() {
    let src = "<figure><img src=\"a/b.png\" alt=\"\" width=\"300\"><figcaption><p>Une &amp; légende</p></figcaption></figure>\n\n<img src=\"c.png\" alt=\"c\" width=\"120px\" height=\"60\">\n\n<pre><code class=\"language-Bash\">echo &lt;a&gt;\nls\n</code></pre>\n\n<table>\n<tr><th>A</th><th>B</th></tr>\n<tr><td><b>1</b></td><td>x &amp; y</td></tr>\n</table>\n";
    let d = doc(src);
    assert_eq!(d.blocks.len(), 4, "{:?}", d.blocks);
    let BlockKind::Image(i) = &d.blocks[0].kind else {
        panic!("{:?}", d.blocks[0])
    };
    assert_eq!(
        (
            i.src.as_str(),
            i.alt.as_str(),
            i.caption.as_deref(),
            i.width
        ),
        ("a/b.png", "Une & légende", Some("Une & légende"), Some(300))
    );
    let BlockKind::Image(i) = &d.blocks[1].kind else {
        panic!()
    };
    assert_eq!(
        (i.width, i.height, i.caption.is_none()),
        (Some(120), Some(60), true)
    );
    assert!(
        matches!(&d.blocks[2].kind, BlockKind::CodeBlock { lang, code, .. } if lang == "bash" && code == "echo <a>\nls")
    );
    let BlockKind::Table { head, rows, .. } = &d.blocks[3].kind else {
        panic!()
    };
    assert_eq!(plain_text(&head[1]), "B");
    assert!(matches!(rows[0][0][0].kind, InlineKind::Strong(_)));
    assert_eq!(plain_text(&rows[0][1]), "x & y");
}

#[test]
fn html_figure_keeps_alt_and_adds_caption() {
    let d =
        doc("<figure><img src=\"a.png\" alt=\"Texte\"><figcaption>Légende</figcaption></figure>\n");
    let BlockKind::Image(i) = &d.blocks[0].kind else {
        panic!()
    };
    assert_eq!(
        (i.alt.as_str(), i.caption.as_deref()),
        ("Texte", Some("Légende"))
    );
}

#[test]
fn html_complex_blocks_stay_html() {
    for src in [
        "<table>\n<tr><td colspan=\"2\">x</td></tr>\n</table>\n",
        "<table>\n<tr><td>a</td></tr>\n",
        "<iframe src=\"x\"></iframe>\n",
        "<div>\ntexte\n</div>\n",
        "<script>var a = 1;</script>\n",
        "<figure>\n<video src=\"a.mp4\"></video>\n</figure>\n",
    ] {
        let d = doc(src);
        assert!(
            matches!(d.blocks[0].kind, BlockKind::Html(_)),
            "{src:?} : {:?}",
            d.blocks
        );
    }
}

#[test]
fn html_inline_formatting_becomes_model_inlines() {
    let i = first_inlines(
        "a<br>b <a href=\"x/y.md#s\">lien <b>gras</b></a> <em>e</em> <i>i</i> <code>c &lt;1&gt;</code> <mark>m</mark> <del>d</del> <s>s</s> <img src=\"p.png\" width=\"20\"> <kbd>K</kbd> <sup>2</sup>",
    );
    assert!(i.iter().any(|x| matches!(&x.kind, InlineKind::HardBreak)));
    let link = i
        .iter()
        .find_map(|x| {
            if let InlineKind::Link(l) = &x.kind {
                Some(l)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        (link.kind, link.target.as_str(), link.anchor.as_deref()),
        (LinkKind::Markdown, "x/y.md", Some("s"))
    );
    assert_eq!(plain_text(&link.children), "lien gras");
    assert_eq!(
        i.iter()
            .filter(|x| matches!(&x.kind, InlineKind::Emphasis(_)))
            .count(),
        2
    );
    assert!(
        i.iter()
            .any(|x| matches!(&x.kind, InlineKind::Code(c) if c == "c <1>"))
    );
    assert!(
        i.iter()
            .any(|x| matches!(&x.kind, InlineKind::Highlight(_)))
    );
    assert_eq!(
        i.iter()
            .filter(|x| matches!(&x.kind, InlineKind::Strike(_)))
            .count(),
        2
    );
    assert!(i.iter().any(
        |x| matches!(&x.kind, InlineKind::Image(im) if im.src == "p.png" && im.width == Some(20))
    ));
    assert!(
        i.iter()
            .any(|x| matches!(&x.kind, InlineKind::Html(h) if h == "<kbd>"))
    );
    assert!(
        i.iter()
            .any(|x| matches!(&x.kind, InlineKind::Html(h) if h == "<sup>"))
    );
}

#[test]
fn html_inline_unbalanced_tags_stay_html() {
    let i = first_inlines("un <b>gras sans fin et </i> puis <a>sans href</a>");
    assert!(
        i.iter()
            .any(|x| matches!(&x.kind, InlineKind::Html(h) if h == "<b>"))
    );
    assert!(
        i.iter()
            .any(|x| matches!(&x.kind, InlineKind::Html(h) if h == "</i>"))
    );
    assert!(
        !i.iter()
            .any(|x| matches!(&x.kind, InlineKind::Strong(_) | InlineKind::Link(_)))
    );
    assert!(plain_text(&i).contains("gras sans fin"));
}

#[test]
fn footnote_references_know_their_number_definition_and_rank() {
    let d =
        doc("Intro[^b] and again[^a] then[^b].\n\n[^a]: First defined.\n[^b]: Second defined.\n");
    let seen: Vec<_> = d
        .footnote_refs
        .iter()
        .map(|r| (r.label.as_str(), r.number, r.occurrence, r.definition))
        .collect();
    assert_eq!(
        seen,
        [
            ("b", 1, 0, Some(0)),
            ("a", 2, 0, Some(1)),
            ("b", 1, 1, Some(0))
        ]
    );
    assert_eq!(
        d.footnotes
            .iter()
            .map(|f| (f.label.as_str(), f.number))
            .collect::<Vec<_>>(),
        [("b", 1), ("a", 2)]
    );
    assert_eq!(d.footnotes[0].references, [0, 2]);
    assert_eq!(d.footnotes[1].references, [1]);
    assert!(d.footnote_refs.iter().all(|r| r.line == 0));
}

#[test]
fn footnotes_are_found_in_tables_callouts_headings_and_captions() {
    let src = "# Title[^h]\n\n| a | b |\n|---|---|\n| cell[^t] | x |\n\n> [!note] Callout[^c]\n> body[^b]\n\n<figure><img src=\"a.png\"><figcaption>Photo[^f]</figcaption></figure>\n\n[^h]: h\n[^t]: t\n[^c]: c\n[^b]: b\n[^f]: f\n[^z]: unused\n";
    let d = doc(src);
    let labels: Vec<_> = d.footnote_refs.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, ["h", "t", "c", "b", "f"]);
    let numbers: Vec<_> = d
        .footnotes
        .iter()
        .map(|f| (f.label.as_str(), f.number))
        .collect();
    assert_eq!(
        numbers,
        [("h", 1), ("t", 2), ("c", 3), ("b", 4), ("f", 5), ("z", 6)]
    );
    assert!(
        d.footnotes[5].references.is_empty(),
        "a definition nobody cites"
    );
    assert_eq!(
        d.footnote_refs[1].line, 2,
        "the table block starts on line 2"
    );
}

#[test]
fn a_footnote_cited_only_by_another_footnote_is_numbered_after_it() {
    let d = doc("Text[^x] and[^a].\n\n[^a]: See also[^n].\n[^n]: Nested.\n");
    assert_eq!(
        d.footnote_refs
            .iter()
            .map(|r| (r.label.as_str(), r.number, r.definition.is_some()))
            .collect::<Vec<_>>(),
        [("a", 1, true), ("n", 2, true)]
    );
    assert_eq!(
        d.footnotes
            .iter()
            .map(|f| f.label.as_str())
            .collect::<Vec<_>>(),
        ["a", "n"]
    );
}

#[test]
fn frontmatter_properties_list_their_wiki_links() {
    let d = doc(
        "---\nstatus: ready\nrelated: \"[[Note A]]\"\nsee:\n  - \"[[Note B#Part|the B note]]\"\n  - plain\ntags: [x, \"[[C]]\"]\n---\n\nBody [[Not a property]]\n",
    );
    let links = &d.frontmatter.as_ref().unwrap().links;
    let seen: Vec<_> = links
        .iter()
        .map(|l| {
            (
                l.key.as_str(),
                l.target.as_str(),
                l.anchor.as_deref(),
                l.display.as_str(),
                l.line,
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("related", "Note A", None, "Note A", 2),
            ("see", "Note B", Some("Part"), "the B note", 4),
            ("tags", "C", None, "C", 6)
        ]
    );
}

#[test]
fn wiki_links_with_an_escaped_alias_separator_in_a_table_cell() {
    let d = doc(
        "| a | b |\n|---|---|\n| [[Note\\|the alias]] | [[Other#Part\\|x]] ![[pic.png\\|300]] |\n",
    );
    let BlockKind::Table { rows, .. } = &d.blocks[0].kind else {
        panic!("table expected")
    };
    let links: Vec<(String, Option<String>, String)> = rows[0]
        .iter()
        .flatten()
        .filter_map(|i| {
            if let InlineKind::Link(l) = &i.kind {
                Some((l.target.clone(), l.anchor.clone(), plain_text(&l.children)))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        links,
        [
            ("Note".into(), None, "the alias".into()),
            ("Other".into(), Some("Part".into()), "x".into())
        ]
    );
    let image = rows[0]
        .iter()
        .flatten()
        .find_map(|i| {
            if let InlineKind::Image(img) = &i.kind {
                Some(img)
            } else {
                None
            }
        })
        .expect("image");
    assert_eq!((image.src.as_str(), image.width), ("pic.png", Some(300)));
}
