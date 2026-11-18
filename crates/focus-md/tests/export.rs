use std::path::{Path, PathBuf};

use focus_md::export::{
    CodeColors, HtmlOptions, Labels, Resolver, to_html, to_html_fragment, to_standard_markdown, zip,
};

struct Notes {
    image: Option<PathBuf>,
}

impl Resolver for Notes {
    fn image(&self, src: &str) -> Option<PathBuf> {
        (src == "pic.png").then(|| self.image.clone()).flatten()
    }

    fn link(&self, target: &str, anchor: Option<&str>) -> Option<String> {
        (target == "Known").then(|| match anchor {
            Some(a) => format!("known.md#{a}"),
            None => "known.md".to_string(),
        })
    }

    fn include(&self, path: &str) -> Option<String> {
        (path == "banner.md")
            .then(|| "# Banner\n\nInclude body\n\n{{#include banner.md}}\n".to_string())
    }

    fn embed(&self, target: &str) -> Option<String> {
        match target {
            "WithInclude" => Some("embedded text\n\n{{#include banner.md}}\n".to_string()),
            "Known" => Some(
                "# Known\n\nIntro text\n\n## Part\n\nPart body ^blk1\n\n## Other\n\nOther body\n"
                    .to_string(),
            ),
            "Loop" => Some("![[Loop]]\n".to_string()),
            _ => None,
        }
    }
}

fn page(source: &str) -> String {
    to_html(
        source,
        &HtmlOptions {
            title: "T".into(),
            embed_fonts: false,
            ..HtmlOptions::default()
        },
        &Notes { image: None },
    )
}

#[test]
fn page_is_standalone_with_language_and_direction() {
    let html = page("text");
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("<html lang=\"en\" dir=\"ltr\" data-theme=\"dark\">"));
    assert!(html.contains("<style>"));
    assert!(!html.contains("http://") || html.contains("w3.org"));
    let arabic = to_html(
        "نص",
        &HtmlOptions {
            lang: "ar".into(),
            embed_fonts: false,
            ..HtmlOptions::default()
        },
        &Notes { image: None },
    );
    assert!(arabic.contains("lang=\"ar\" dir=\"rtl\""));
    assert!(arabic.contains("<p dir=\"auto\">نص</p>"));
    let light = to_html(
        "x",
        &HtmlOptions {
            dark: false,
            embed_fonts: false,
            ..HtmlOptions::default()
        },
        &Notes { image: None },
    );
    assert!(light.contains("data-theme=\"light\"") && light.contains("--bg:#ffffff"));
}

#[test]
fn fonts_are_embedded_only_on_request() {
    let with = to_html(
        "x",
        &HtmlOptions {
            embed_fonts: true,
            ..HtmlOptions::default()
        },
        &Notes { image: None },
    );
    assert!(with.contains("@font-face") && with.contains("base64,"));
    assert!(!page("x").contains("@font-face"));
}

#[test]
fn callouts_fold_with_native_details() {
    let html = page(
        "> [!warning]+ Careful\n> body\n\n> [!note]- Closed\n> body\n\n> [!tip]\n> plain\n\n> [!danger]\n> > [!note] Nested\n> > inner\n",
    );
    assert!(html.contains(
        "<details class=\"callout\" data-family=\"warning\" data-kind=\"warning\" open>"
    ));
    assert!(
        html.contains(
            "<details class=\"callout\" data-family=\"note\" data-kind=\"note\"><summary"
        )
    );
    assert!(html.contains("<div class=\"callout\" data-family=\"tip\" data-kind=\"tip\">"));
    assert!(html.contains("<span class=\"label\">Tip</span>"));
    assert!(html.contains("data-family=\"danger\"") && html.contains("Nested"));
    assert_eq!(html.matches("class=\"callout\"").count(), 5);
}

#[test]
fn callout_default_titles_come_from_labels() {
    let labels = Labels {
        callout_titles: vec![("tip".into(), "Astuce".into())],
        copy: "Copier".into(),
        ..Labels::default()
    };
    let html = to_html(
        "> [!tip]\n> a\n\n```bash\nid\n```\n",
        &HtmlOptions {
            labels,
            embed_fonts: false,
            ..HtmlOptions::default()
        },
        &Notes { image: None },
    );
    assert!(html.contains("<span class=\"label\">Astuce</span>"));
    assert!(html.contains(">Copier</button>"));
}

#[test]
fn details_block_becomes_native_details() {
    let html = page("<details open>\n<summary>More</summary>\n\nHidden **text**\n\n</details>\n");
    assert!(html.contains("<details class=\"fold\" open>"));
    assert!(html.contains("<span class=\"label\">More</span>"));
    assert!(html.contains("<strong>text</strong>"));
}

#[test]
fn code_blocks_are_highlighted_and_detected_languages_are_marked() {
    let html = page(
        "```bash\nsudo nmap -sV $HOST # scan\n```\n\n```\nGet-ChildItem -Path C:\\ | Where-Object { $_.Length -gt 1 }\n```\n\n```text\nplain <b>\n```\n",
    );
    assert!(html.contains("<span class=\"t-fn\">nmap</span>"));
    assert!(html.contains("<span class=\"t-flag\">-sV</span>"));
    assert!(html.contains("<span class=\"t-cmt\"># scan</span>"));
    assert!(html.contains("PowerShell<span class=\"detected\">detected</span>"));
    assert!(html.contains("plain &lt;b&gt;"));
    let mono = to_html(
        "```bash\nls\n```\n",
        &HtmlOptions {
            code_colors: CodeColors::Mono,
            embed_fonts: false,
            ..HtmlOptions::default()
        },
        &Notes { image: None },
    );
    assert!(mono.contains("--c-fn:#f5f5f6"));
}

#[test]
fn active_html_is_neutralised() {
    let html = page(
        "<script>alert(1)</script>\n\n<div onclick=\"x()\">ok <a href=\"javascript:alert(1)\">bad</a></div>\n\n[x](javascript:alert(1))\n\ninline <img src=x onerror=alert(1)>\n",
    );
    assert!(!html.contains("<script>alert"));
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!html.contains("onclick") && !html.contains("onerror"));
    assert!(!html.contains("href=\"javascript:"));
}

#[test]
fn mermaid_renders_inline_svg_or_shows_the_error() {
    let html = page(
        "```mermaid\nflowchart LR\n A --> B\n```\n\n```mermaid\nflowchart LR\n A --> \n --> B[\n```\n",
    );
    assert!(
        html.contains(
            "<figure class=\"mermaid\"><header><span>Diagram</span><code>flowchart</code>"
        )
    );
    assert!(html.contains("<svg"));
    assert!(html.contains("class=\"mermaid-error\"") && html.contains("invalid Mermaid syntax"));
    let off = to_html(
        "```mermaid\nflowchart LR\n A --> B\n```\n",
        &HtmlOptions {
            render_mermaid: false,
            embed_fonts: false,
            ..HtmlOptions::default()
        },
        &Notes { image: None },
    );
    assert!(!off.contains("<svg") && off.contains("flowchart LR"));
}

#[test]
fn images_are_embedded_as_data_uris() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let file = dir.join("export_pic.png");
    std::fs::write(&file, [0x89, b'P', b'N', b'G']).unwrap();
    let source = "![alt](pic.png)\n\n<figure><img src=\"pic.png\" alt=\"\"><figcaption>Cap</figcaption></figure>\n\n![remote](https://e.org/a.png)\n";
    let html = to_html(
        source,
        &HtmlOptions {
            embed_fonts: false,
            ..HtmlOptions::default()
        },
        &Notes {
            image: Some(file.clone()),
        },
    );
    assert_eq!(
        html.matches("src=\"data:image/png;base64,iVBORw==\"")
            .count(),
        2
    );
    assert!(html.contains("<figcaption dir=\"auto\">Cap</figcaption>"));
    assert!(html.contains("src=\"https://e.org/a.png\""));
    let linked = to_html(
        "![alt](pic.png)",
        &HtmlOptions {
            embed_images: false,
            embed_fonts: false,
            ..HtmlOptions::default()
        },
        &Notes { image: Some(file) },
    );
    assert!(linked.contains("src=\"pic.png\""));
}

#[test]
fn embeds_expand_sections_and_stop_on_cycles() {
    let html = page("![[Known#Part]]\n\n![[Known^blk1]]\n\n![[Missing]]\n\n![[Loop]]\n");
    assert!(html.contains("<div class=\"embed\"><div class=\"embed-title\" dir=\"auto\"><a href=\"known.md#Part\">Known</a></div>"));
    assert!(html.contains("Part body"));
    assert!(!html.contains("Other body"));
    assert!(html.contains("class=\"embed missing\""));
    assert!(html.contains("embed-title"));
}

#[test]
fn links_tags_footnotes_properties_and_ids() {
    let html = page(
        "---\ntags: [a/b, c]\nstatus: draft\n---\n\n# Title\n\n## Same\n\n## Same\n\nSee [[Known#Part|alias]], [[Nowhere]], [[#Same]], https://e.org and #tag/x with note[^1].\n\n[^1]: The note.\n",
    );
    assert!(html.contains("<a href=\"known.md#Part\">alias</a>"));
    assert!(html.contains("<span class=\"unresolved\">Nowhere</span>"));
    assert!(html.contains("<a href=\"#same\">"));
    assert!(html.contains("id=\"same\"") && html.contains("id=\"same-1\""));
    assert!(html.contains("class=\"external\" target=\"_blank\" rel=\"noopener noreferrer\""));
    assert!(html.contains("<span class=\"tag\" dir=\"auto\">#tag/x</span>"));
    assert!(
        html.contains("<span class=\"tag\" dir=\"auto\">#a/b</span>")
            && html.contains("<span class=\"k\">status</span>")
    );
    assert!(
        html.contains("<section class=\"footnotes\"")
            && html.contains("The note.")
            && html.contains("Back to text")
    );
}

#[test]
fn print_stylesheet_avoids_breaking_blocks() {
    let html = page("x");
    assert!(html.contains("@media print"));
    assert!(html.contains("break-inside:avoid"));
    assert!(html.contains("size:A4"));
}

#[test]
fn fragment_has_no_page_wrapper() {
    let html = to_html_fragment(
        "> [!note] Title\n> body\n\n```rust\nfn x() {}\n```\n\n[[Known]] and [[Nope]]\n",
        &Notes { image: None },
    );
    assert!(!html.contains("<html") && !html.contains("<style"));
    assert!(html.contains("<blockquote>\n<p><strong>Title</strong></p>"));
    assert!(html.contains("<pre><code class=\"language-rust\">fn x() {}</code></pre>"));
    assert!(
        html.contains("<a href=\"known.md\">Known</a>") && html.contains("class=\"unresolved\"")
    );
}

#[test]
fn standard_markdown_replaces_the_dialect() {
    let source = "---\ntype: x\n---\n\n# T\n\nSee [[Known#Part|alias]] and [[Nowhere]] ==mark== #tag %%hidden%% end ^id\n\n> [!warning]+ Careful\n> body\n\n![[Known#Part]]\n\n- [x] done\n- [ ] todo\n\n| a | b |\n|:--|--:|\n| 1 | `x\\|y` |\n\n```mermaid\nflowchart LR\n A-->B\n```\n\n<details>\n<summary>S</summary>\n\nIn.\n\n</details>\n";
    let md = to_standard_markdown(source, &Notes { image: None });
    assert!(md.starts_with("---\ntype: x\n---\n\n# T"));
    assert!(md.contains("[alias](known.md#Part)") && md.contains("Nowhere") && !md.contains("[["));
    assert!(md.contains("<mark>mark</mark>") && !md.contains("hidden") && !md.contains("^id"));
    assert!(md.contains("> **Careful**\n>\n> body"));
    assert!(md.contains("## Part\n\nPart body"));
    assert!(md.contains("- [x] done\n- [ ] todo"));
    assert!(md.contains("| :--- | ---: |") || md.contains("| :--- | ---: |"));
    assert!(md.contains("```mermaid\nflowchart LR"));
    assert!(md.contains("<details>\n<summary>S</summary>\n\nIn.\n\n</details>"));
    let again = to_standard_markdown(&md, &Notes { image: None });
    assert_eq!(
        again, md,
        "converting standard Markdown again changes nothing"
    );
}

#[test]
fn zip_archives_files_under_their_archive_paths() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("zip_test");
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.md");
    std::fs::write(&a, "# A").unwrap();
    let archive = dir.join("out.zip");
    let size = zip(&[(a, PathBuf::from("notes/a.md"))], &archive).unwrap();
    assert!(size > 0);
    let mut reader = ::zip::ZipArchive::new(std::fs::File::open(&archive).unwrap()).unwrap();
    assert_eq!(reader.len(), 1);
    assert_eq!(reader.by_index(0).unwrap().name().unwrap(), "notes/a.md");
}

#[test]
fn right_to_left_containers_get_their_own_direction() {
    let source = "- عنصر\n\n| أ | ب |\n|---|---|\n| 1 | 2 |\n\n> [!tip] نصيحة\n> نص\n\n- item\n";
    let html = page(source);
    assert!(html.contains("<ul dir=\"rtl\">"));
    assert!(html.contains("<table dir=\"rtl\">"));
    assert!(html.contains("data-kind=\"tip\" dir=\"rtl\""));
    assert!(html.contains("<ul>\n<li dir=\"auto\">item"));
    let arabic_page = to_html(
        "- item\n",
        &HtmlOptions {
            lang: "ar".into(),
            embed_fonts: false,
            ..HtmlOptions::default()
        },
        &Notes { image: None },
    );
    assert!(arabic_page.contains("<ul dir=\"ltr\">"));
}

#[test]
fn diagram_links_become_anchors_when_they_resolve() {
    let source = "```mermaid\nflowchart LR\n  A[Start] --> B[\"See [[Known]]\"]\n  B --> C[Gone]\n  click A \"[[Known#Part]]\"\n  click C \"[[Missing]]\"\n  click B \"https://example.com/x\"\n```\n";
    let html = to_html(
        source,
        &HtmlOptions {
            title: "T".into(),
            embed_fonts: false,
            ..HtmlOptions::default()
        },
        &Notes { image: None },
    );
    assert!(
        html.contains("<a href=\"known.md#Part\"><rect"),
        "{}",
        &html[html.find("<svg").unwrap_or(0)..]
            .chars()
            .take(3000)
            .collect::<String>()
    );
    assert!(html.contains("<a href=\"https://example.com/x\">"));
    assert!(
        !html.contains("Missing\""),
        "an unresolved link is not an anchor"
    );
    assert_eq!(
        html.matches("<a href=\"known.md").count(),
        1,
        "one anchor per node: the click wins over the label link"
    );
}

#[test]
fn property_links_are_anchors_when_they_resolve() {
    let html =
        page("---\nrelated: \"[[Known#Part|the known one]]\"\nsee: \"[[Nowhere]]\"\n---\n\nBody\n");
    assert!(
        html.contains("<a href=\"known.md#Part\">the known one</a>"),
        "{html}"
    );
    assert!(html.contains(">Nowhere<") && !html.contains("[[Nowhere"));
}

#[test]
fn diagrams_shrink_to_the_page_down_to_three_quarters_and_always_fit_in_print() {
    let html = page("```mermaid\nflowchart LR\n  A[Start] --> B[End]\n```\n");
    let root = &html[html.find("<svg").unwrap()..];
    let root = &root[..root.find('>').unwrap()];
    let width: f32 = root
        .split(" width=\"")
        .nth(1)
        .and_then(|w| w.split('"').next())
        .and_then(|w| w.parse().ok())
        .expect("fixed width attribute");
    assert!(
        root.contains("max-width:100%") && root.contains("height:auto"),
        "{root}"
    );
    assert!(
        root.contains(&format!("width:{width:.0}px"))
            && root.contains(&format!("min-width:{:.0}px", width * 0.75)),
        "{root}"
    );
    assert!(
        html.contains(
            "figure.mermaid svg{width:auto!important;min-width:0!important;max-width:100%!important"
        ),
        "print rule"
    );
}

#[test]
fn inline_code_is_isolated_left_to_right_inside_right_to_left_text() {
    let html = to_html(
        "\u{0627}\u{0644}\u{0633}\u{062c}\u{0644} `- 2026-11-12 14:03:27 [10.10.10.5] text` \u{0646}\u{0647}\u{0627}\u{064a}\u{0629}\n",
        &HtmlOptions {
            lang: "ar".into(),
            embed_fonts: false,
            ..HtmlOptions::default()
        },
        &Notes { image: None },
    );
    assert!(
        html.contains("<code dir=\"ltr\">- 2026-11-12 14:03:27 [10.10.10.5] text</code>"),
        "{html}"
    );
    assert!(html.contains("unicode-bidi:isolate"));
}

const TABS: &str = "````tabs\n\ntab: First\n\nbody one\n\n```bash\ntab: not a title\n```\n\ntab: Second <b>\n\nbody two\n\n> [!tip]\n> inside\n\n````\n\n{{#tabs}}\n{{#tab name=\"Alpha\"}}\nalpha text\n{{#endtab}}\n{{#tab name=\"Beta\"}}\nbeta text\n{{#endtab}}\n{{#tab name=\"Gamma\"}}\ngamma text\n{{#endtab}}\n{{#endtabs}}\n";

#[test]
fn tabs_work_without_script_and_print_every_panel_under_its_title() {
    let html = page(TABS);
    assert_eq!(html.matches("<div class=\"tabs\">").count(), 2);
    assert_eq!(
        html.matches("type=\"radio\"").count(),
        5,
        "one radio button per tab"
    );
    assert_eq!(
        html.matches(" checked>").count(),
        2,
        "the first tab of each group is selected"
    );
    assert!(
        html.contains("name=\"tabs-1\"")
            && html.contains("name=\"tabs-2\"")
            && html.contains("id=\"tabs-2-2\"")
    );
    assert!(
        html.contains("<label for=\"tabs-1-1\" dir=\"auto\">Second &lt;b&gt;</label>"),
        "titles are text"
    );
    assert_eq!(html.matches("<section class=\"tab-panel\">").count(), 5);
    assert!(
        !page("````tabs\ntab: A\n\nx\n\ntab: B\n\ny\n````\n").contains("<script"),
        "a page with tabs and nothing to copy has no script: the radio buttons do the work"
    );
    let script = html
        .split("<script>")
        .nth(1)
        .and_then(|rest| rest.split("</script>").next())
        .unwrap_or("");
    assert!(
        !script.contains("tab") && !script.contains("radio"),
        "the only script is the copy button"
    );
    assert!(html.contains(
        ".tabs>input:nth-of-type(3):checked~.tab-panels>.tab-panel:nth-child(3){display:block}"
    ));
    assert!(!html.contains("nth-of-type(4)"));
    assert!(
        html.contains("@media print{.tab-bar{display:none}.tab-panel{display:block!important"),
        "printing shows every panel"
    );
    assert!(
        html.contains(".tab-title{display:none}"),
        "titles over the panels only when printed"
    );
    assert!(
        page("no tabs here").find(".tab-panel").is_none(),
        "a page without tabs carries no tab rules"
    );
}

#[test]
fn the_clipboard_fragment_stacks_the_tabs_under_their_titles() {
    let html = to_html_fragment(TABS, &Notes { image: None });
    assert!(!html.contains("<input") && !html.contains("class=\"tabs\""));
    assert!(html.contains("<h4 dir=\"auto\">First</h4>") && html.contains("beta text"));
}

#[test]
fn standard_markdown_keeps_tabs_and_directives_in_their_source_form() {
    let md = to_standard_markdown(TABS, &Notes { image: None });
    assert!(md.contains("````tabs\n\ntab: First\n"), "{md}");
    assert!(
        md.contains("{{#tabs}}\n{{#tab name=\"Alpha\"}}\n\nalpha text\n\n{{#endtab}}"),
        "{md}"
    );
    let again = focus_md::parse(&md, &focus_md::ParseOptions::default());
    let titles: Vec<Vec<String>> = again
        .blocks
        .iter()
        .filter_map(|b| {
            if let focus_md::BlockKind::Tabs { tabs, .. } = &b.kind {
                Some(tabs.iter().map(|t| t.title.clone()).collect())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        titles,
        [
            vec!["First".to_string(), "Second <b>".to_string()],
            vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()]
        ]
    );
    let directives = "{{#ref}}\nsome/note.md\n{{#endref}}\n\n{{#include a.md}}\n\n{{#file}}\nx.bin\n{{#endfile}}\n\n{{#with y}}\n";
    assert_eq!(
        to_standard_markdown(directives, &Notes { image: None }),
        directives
    );
}

#[test]
fn directives_are_a_link_or_a_muted_line() {
    let html = page(
        "{{#ref}}\nKnown.md#Part\n{{#endref}}\n\n{{#ref}}\nMissing.md\n{{#endref}}\n\n{{#include banner.md}}\n\n{{#whatever a b}}\n",
    );
    assert!(
        html.contains("<p class=\"directive\"><a href=\"known.md#Part\">Known</a></p>"),
        "{html}"
    );
    assert!(html.contains("<span class=\"unresolved\">Missing</span>"));
    assert!(html.contains("<p class=\"directive muted\">include banner.md</p>"));
    assert!(html.contains("<p class=\"directive muted\">whatever a b</p>"));
}

#[test]
fn an_include_the_resolver_knows_is_expanded_once_and_never_from_inside_another() {
    let html = page("before\n\n{{#include banner.md}}\n\n{{#include other.md}}\n\nafter\n");
    assert!(
        html.contains("<div class=\"include\">") && html.contains("Include body"),
        "{html}"
    );
    assert_eq!(
        html.matches("Include body").count(),
        1,
        "the include inside the included note is not expanded again"
    );
    assert!(
        html.contains("<p class=\"directive muted\">include banner.md</p>"),
        "it stays a muted line"
    );
    assert!(
        html.contains("<p class=\"directive muted\">include other.md</p>"),
        "an unknown file stays a muted line"
    );
    let embedded = page("![[WithInclude]]\n");
    assert!(
        embedded.contains("embedded text") && !embedded.contains("Include body"),
        "inside an embed the relative path would mean something else"
    );
    let md = to_standard_markdown(
        "{{#include banner.md}}\n\n{{#include other.md}}\n",
        &Notes { image: None },
    );
    assert_eq!(
        md,
        "# Banner\n\nInclude body\n\n{{#include banner.md}}\n\n{{#include other.md}}\n"
    );
}
