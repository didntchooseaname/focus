use std::path::PathBuf;

use super::{HtmlOptions, Resolver, to_html, to_html_fragment};
use crate::html::{self, Token};

struct Vault;

impl Resolver for Vault {
    fn image(&self, _: &str) -> Option<PathBuf> {
        None
    }
    fn link(&self, target: &str, _: Option<&str>) -> Option<String> {
        Some(format!("{target}.html"))
    }
    fn embed(&self, target: &str) -> Option<String> {
        (target == "Hostile").then(|| {
            "<script>alert('embed')</script>\n\n<img src=x onerror=alert(1)>\n".to_string()
        })
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
        &Vault,
    )
}

fn browser_url(url: &str) -> String {
    url.chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn scheme(url: &str) -> Option<String> {
    let clean = browser_url(url);
    let end = clean.find([':', '/', '?', '#'])?;
    (clean.as_bytes()[end] == b':').then(|| clean[..end].to_string())
}

fn problems(html: &str) -> Vec<String> {
    let mut found = Vec::new();
    let (mut scripts, mut styles) = (0, 0);
    for (_, token) in html::tokens(html) {
        let Token::Open { name, attrs, .. } = token else {
            continue;
        };
        match name.as_str() {
            "script" => {
                scripts += 1;
                if scripts > 1 {
                    found.push("second script".into());
                }
            }
            "style" => {
                styles += 1;
                if styles > 1 {
                    found.push("second style".into());
                }
            }
            "iframe" | "frame" | "frameset" | "object" | "embed" | "applet" | "form" | "button"
            | "textarea" | "select" | "base" | "link" | "video" | "audio" | "source" | "track"
            | "foreignobject" | "animate" | "set" | "math" | "template" | "noscript" => {
                if name != "button" {
                    found.push(format!("element <{name}>"));
                }
            }
            "input" if !attrs.iter().any(|(k, v)| k == "type" && v == "radio") => {
                found.push("input that is not a tab radio button".into())
            }
            "meta"
                if !attrs.iter().any(|(k, v)| {
                    (k == "charset")
                        || (k == "name" && (v == "viewport" || v == "referrer"))
                        || (k == "http-equiv" && v.eq_ignore_ascii_case("content-security-policy"))
                }) =>
            {
                found.push("foreign <meta>".into())
            }
            _ => {}
        }
        for (key, value) in &attrs {
            if key.starts_with("on") {
                found.push(format!("handler {key} on <{name}>"));
            }
            if matches!(
                key.as_str(),
                "href"
                    | "src"
                    | "xlink:href"
                    | "action"
                    | "formaction"
                    | "data"
                    | "poster"
                    | "srcset"
            ) {
                match scheme(value).as_deref() {
                    None | Some("http" | "https" | "mailto" | "tel") => {}
                    Some("data")
                        if name == "img" && browser_url(value).starts_with("data:image/") => {}
                    Some(other) => found.push(format!("{key}={other}: on <{name}>")),
                }
            }
            if key == "style" {
                let lower = value.to_ascii_lowercase();
                if lower.contains("url(")
                    || lower.contains("@import")
                    || lower.contains("expression")
                    || lower.contains("javascript")
                    || lower.contains("http")
                {
                    found.push(format!("style attribute: {value}"));
                }
            }
        }
    }
    found
}

#[test]
fn raw_html_blocks_run_nothing() {
    let hostile = [
        "<script>alert(1)</script>",
        "<SCRIPT SRC=//evil/x.js></SCRIPT>",
        "<iframe src=\"https://evil/\"></iframe>",
        "<object data=\"x.swf\"></object>",
        "<embed src=\"x.swf\">",
        "<form action=\"https://evil/\"><input name=a><button>go</button></form>",
        "<base href=\"https://evil/\">",
        "<link rel=\"stylesheet\" href=\"https://evil/x.css\">",
        "<meta http-equiv=\"refresh\" content=\"0;url=https://evil/\">",
        "<style>@import url(https://evil/x.css); body{background:url(https://evil/b.png)}</style>",
        "<svg onload=\"alert(1)\"><script>alert(2)</script></svg>",
        "<math><mi xlink:href=\"javascript:alert(1)\">x</mi></math>",
        "<video src=\"x.mp4\" onerror=\"alert(1)\"></video>",
        "<div onclick=\"alert(1)\" onmouseover=alert(2)>text</div>",
        "<div style=\"background:url(https://evil/b.png)\">text</div>",
        "<p style=\"width:expression(alert(1))\">text</p>",
        "<details open ontoggle=\"alert(1)\"><summary>s</summary>x</details>",
        "<a href=\"javascript:alert(1)\">x</a>",
        "<a href=\"JaVaScRiPt:alert(1)\">x</a>",
        "<a href=\" javascript:alert(1)\">x</a>",
        "<a href=\"java&#9;script:alert(1)\">x</a>",
        "<a href=\"java&#x0A;script:alert(1)\">x</a>",
        "<a href=\"&#1;javascript:alert(1)\">x</a>",
        "<a href=\"&#106;avascript:alert(1)\">x</a>",
        "<a href=\"vbscript:msgbox(1)\">x</a>",
        "<a href=\"data:text/html,<script>alert(1)</script>\">x</a>",
        "<a href=\"data:image/svg+xml;base64,PHN2ZyBvbmxvYWQ9YWxlcnQoMSk+\">x</a>",
        "<img src=\"x\" onerror=\"alert(1)\">",
        "<img src=\"javascript:alert(1)\">",
        "<img src=\"data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==\">",
        "<img srcset=\"https://evil/x.png 1x\" src=\"x.png\">",
        "<img src=\"x.png\"\nonerror=\"alert(1)\">",
        "<img/src=x/onerror=alert(1)>",
        "<a href=\"x\" target=\"_blank\">x</a>",
        "<!--><script>alert(1)</script>-->",
        "<![CDATA[<script>alert(1)</script>]]>",
        "<noscript><p title=\"</noscript><script>alert(1)</script>\">",
        "<template><script>alert(1)</script></template>",
        "<table background=\"https://evil/b.png\"><tr><td background=\"https://evil/c.png\">x</td></tr></table>",
    ];
    for raw in hostile {
        for source in [
            raw.to_string(),
            format!("text {raw} text"),
            format!("> {raw}"),
            format!("- {raw}"),
            format!("| a |\n|---|\n| {raw} |"),
            format!("> [!note] {raw}\n> {raw}"),
        ] {
            let out = page(&source);
            let found = problems(&out);
            assert!(found.is_empty(), "{source:?} gives {found:?}");
            let fragment = to_html_fragment(&source, &Vault);
            let found = problems(&fragment);
            assert!(
                found.iter().all(|p| p != "second script"),
                "{source:?} fragment gives {found:?}"
            );
        }
    }
}

#[test]
fn markdown_constructs_cannot_carry_markup() {
    let hostile = [
        "[x](javascript:alert(1))",
        "[x](JAVASCRIPT:alert(1))",
        "[x](<javascript:alert(1)>)",
        "[x](javascript&#58;alert(1))",
        "[x](java\tscript:alert(1))",
        "[x](vbscript:msgbox(1))",
        "[x](data:text/html,<script>alert(1)</script>)",
        "[x](\"onmouseover=alert(1) x=\")",
        "[x](a\" onmouseover=\"alert(1))",
        "<javascript:alert(1)>",
        "<https://evil/\"onmouseover=\"alert(1)>",
        "![a\"onerror=\"alert(1)](p\"onerror=\"alert(2).png)",
        "![x](javascript:alert(1))",
        "![x](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==)",
        "![x](file:///etc/passwd)",
        "![[a\"onerror=\"alert(1).png]]",
        "[[a\"onmouseover=\"alert(1)|<script>alert(2)</script>]]",
        "[[Note#\"onmouseover=\"alert(1)]]",
        "![[Hostile]]",
        "![[Note\"><script>alert(1)</script>]]",
        "# <script>alert(1)</script> \"><img src=x onerror=alert(1)>",
        "## [x](javascript:alert(1))",
        "text[^\"><script>alert(1)</script>]\n\n[^\"><script>alert(1)</script>]: note <img src=x onerror=alert(1)>",
        "#\"onmouseover=\"alert(1) #tag\"><script>alert(1)</script>",
        "---\ntitle: </title><script>alert(1)</script>\nkey: \"><img src=x onerror=alert(1)>\nlist: [\"><script>alert(1)</script>]\nlink: [[a\"onmouseover=\"alert(1)]]\n---\n\ntext",
        "```\"><script>alert(1)</script>\ncode\n```",
        "```js onmouseover=alert(1)\ncode\n```",
        "```mermaid\nflowchart LR\n  A[\"<script>alert(1)</script>\"] --> B[\"<img src=x onerror=alert(2)>\"]\n  click A \"javascript:alert(3)\"\n  click B href \"javascript:alert(4)\"\n```",
        "```mermaid\nsequenceDiagram\n  A->>B: <script>alert(1)</script>\n  Note over A: </text><script>alert(2)</script>\n```",
        "```mermaid\nflowchart LR\n  A[[<script>alert(1)</script>]] --> B\n  link A \"javascript:alert(2)\"\n```",
        "> [!\"><script>alert(1)</script>] <img src=x onerror=alert(2)>\n> body",
        "$$<script>alert(1)</script>$$",
        "$x\"onmouseover=\"alert(1)$",
        "- [ ] <script>alert(1)</script>",
        "| <script>alert(1)</script> |\n|---|\n| <img src=x onerror=alert(1)> |",
        "<details><summary><script>alert(1)</script></summary></details>",
        "term\n: <script>alert(1)</script>",
        "[ref]: javascript:alert(1)\n\n[x][ref]",
        "[x]: <javascript:alert(1)>\n\n[x]",
        "%% <script>alert(1)</script> %%",
        "````tabs\ntab: <script>alert(1)</script>\"><img src=x onerror=alert(2)>\n\n<script>alert(3)</script>\n\n[x](javascript:alert(4))\n\ntab: \"onmouseover=\"alert(5)\n\n<input type=text onfocus=alert(6)>\n````",
        "{{#tabs}}\n{{#tab name=\"<img src=x onerror=alert(1)>\"}}\n<iframe src=x></iframe>\n{{#endtab}}\n{{#endtabs}}",
        "{{#ref}}\njavascript:alert(1)\n{{#endref}}\n\n{{#include javascript:alert(2)}}\n\n{{#<script>alert(3)</script> x}}",
        "^block <script>alert(1)</script> ^\"><script>",
    ];
    for source in hostile {
        let out = page(source);
        let found = problems(&out);
        assert!(found.is_empty(), "{source:?} gives {found:?}");
    }
    let titled = to_html(
        "x",
        &HtmlOptions {
            title: "</title><script>alert(1)</script>".into(),
            embed_fonts: false,
            ..HtmlOptions::default()
        },
        &Vault,
    );
    assert!(problems(&titled).is_empty(), "{:?}", problems(&titled));
}

#[test]
fn the_page_carries_a_policy_that_allows_only_its_own_script() {
    let plain = page("text");
    assert!(plain.contains("http-equiv=\"Content-Security-Policy\""));
    assert!(
        plain.contains("script-src 'none'"),
        "no script on the page, none allowed"
    );
    assert!(plain.contains("name=\"referrer\" content=\"no-referrer\""));
    let with_code = page("```bash\necho 1\n```");
    let script = with_code
        .split("<script>")
        .nth(1)
        .and_then(|rest| rest.split("</script>").next())
        .expect("the copy script");
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    let hash = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(script.as_bytes()));
    assert!(
        with_code.contains(&format!("script-src 'sha256-{hash}'")),
        "{hash}"
    );
    let hostile = page("<script>alert(1)</script>\n\n```bash\necho 1\n```");
    assert_eq!(
        hostile.matches("<script>").count(),
        1,
        "only the copy script is a script"
    );
}
