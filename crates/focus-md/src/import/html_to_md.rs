use std::fmt::Write;

use crate::html::{Token, attr, decode_entities, tokens};
use crate::url::is_safe_url;

enum Node {
    Text(String),
    Element {
        name: String,
        attrs: Vec<(String, String)>,
        children: Vec<Node>,
    },
}

type OpenElement = (String, Vec<(String, String)>, Vec<Node>);

const VOID: &[&str] = &[
    "br", "img", "hr", "meta", "link", "input", "col", "wbr", "source", "area", "base",
];
const SKIPPED: &[&str] = &["script", "style", "head", "template", "noscript", "svg"];
const BLOCKS: &[&str] = &[
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "p",
    "div",
    "ul",
    "ol",
    "li",
    "pre",
    "blockquote",
    "table",
    "hr",
    "section",
    "article",
    "header",
    "footer",
    "nav",
    "main",
    "aside",
    "figure",
    "figcaption",
    "details",
    "summary",
    "dl",
    "dt",
    "dd",
    "body",
    "html",
    "center",
];

fn build(html: &str) -> Vec<Node> {
    let mut stack: Vec<OpenElement> = vec![(String::new(), Vec::new(), Vec::new())];
    let mut skip: Option<String> = None;
    for (_, token) in tokens(html) {
        if let Some(name) = &skip {
            if matches!(&token, Token::Close { name: n } if n == name) {
                skip = None;
            }
            continue;
        }
        match token {
            Token::Text(t) => {
                let text = decode_entities(t);
                stack
                    .last_mut()
                    .expect("root stays")
                    .2
                    .push(Node::Text(text));
            }
            Token::Comment => {}
            Token::Open {
                name,
                attrs,
                self_closing,
                ..
            } => {
                if SKIPPED.contains(&name.as_str()) {
                    if !self_closing {
                        skip = Some(name);
                    }
                    continue;
                }
                let implicit_close = |top: &str| match name.as_str() {
                    "li" => top == "li",
                    "p" => top == "p",
                    "td" | "th" => matches!(top, "td" | "th"),
                    "tr" => matches!(top, "tr" | "td" | "th"),
                    "dt" | "dd" => matches!(top, "dt" | "dd"),
                    _ => false,
                };
                while stack.len() > 1 && implicit_close(&stack.last().expect("root stays").0) {
                    let (n, a, c) = stack.pop().expect("length checked");
                    stack.last_mut().expect("root stays").2.push(Node::Element {
                        name: n,
                        attrs: a,
                        children: c,
                    });
                }
                if VOID.contains(&name.as_str()) || self_closing {
                    stack.last_mut().expect("root stays").2.push(Node::Element {
                        name,
                        attrs,
                        children: Vec::new(),
                    });
                } else {
                    stack.push((name, attrs, Vec::new()));
                }
            }
            Token::Close { name } => {
                if let Some(depth) = stack
                    .iter()
                    .rposition(|(n, _, _)| *n == name)
                    .filter(|d| *d > 0)
                {
                    while stack.len() > depth {
                        let (n, a, c) = stack.pop().expect("length checked");
                        stack.last_mut().expect("root stays").2.push(Node::Element {
                            name: n,
                            attrs: a,
                            children: c,
                        });
                    }
                }
            }
        }
    }
    while stack.len() > 1 {
        let (n, a, c) = stack.pop().expect("length checked");
        stack.last_mut().expect("root stays").2.push(Node::Element {
            name: n,
            attrs: a,
            children: c,
        });
    }
    stack.pop().expect("root stays").2
}

pub(super) fn convert(html: &str) -> String {
    let nodes = build(html);
    let text = blocks(&nodes);
    let mut out = String::new();
    let mut blank = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(
            line.trim_end_matches(' ')
                .trim_end_matches('\t')
                .trim_end_matches(' '),
        );
        out.push('\n');
    }
    format!("{}\n", out.trim())
}

fn is_block(node: &Node) -> bool {
    matches!(node, Node::Element { name, .. } if BLOCKS.contains(&name.as_str()))
}

fn blocks(nodes: &[Node]) -> String {
    let mut out = String::new();
    let mut run: Vec<&Node> = Vec::new();
    let flush = |run: &mut Vec<&Node>, out: &mut String| {
        let text = inline_nodes(run.iter().copied());
        let text = text.trim();
        if !text.is_empty() {
            let _ = write!(out, "{text}\n\n");
        }
        run.clear();
    };
    for node in nodes {
        if is_block(node) {
            flush(&mut run, &mut out);
            out.push_str(&block(node));
        } else {
            run.push(node);
        }
    }
    flush(&mut run, &mut out);
    out
}

fn text_of(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(t),
            Node::Element { name, children, .. } => {
                if name == "br" {
                    out.push('\n');
                }
                out.push_str(&text_of(children));
            }
        }
    }
    out
}

fn quote(text: &str) -> String {
    text.trim()
        .lines()
        .map(|l| {
            if l.is_empty() {
                ">".to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn block(node: &Node) -> String {
    let Node::Element {
        name,
        attrs,
        children,
    } = node
    else {
        return String::new();
    };
    match name.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = usize::from(name.as_bytes()[1] - b'0');
            let text = inline_nodes(children.iter());
            format!(
                "{} {}\n\n",
                "#".repeat(level),
                text.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        }
        "pre" => {
            let lang = children
                .iter()
                .find_map(|c| match c {
                    Node::Element { name, attrs, .. } if name == "code" => attr(attrs, "class")
                        .and_then(|c| {
                            c.split_whitespace()
                                .find_map(|w| w.strip_prefix("language-"))
                        })
                        .map(str::to_string),
                    _ => None,
                })
                .unwrap_or_default();
            let code = text_of(children);
            let code = code
                .strip_prefix('\n')
                .unwrap_or(&code)
                .trim_end_matches('\n');
            let fence = "`".repeat(
                code.split(|c| c != '`')
                    .map(str::len)
                    .max()
                    .unwrap_or(0)
                    .max(2)
                    + 1,
            );
            format!("{fence}{lang}\n{code}\n{fence}\n\n")
        }
        "blockquote" => format!("{}\n\n", quote(&blocks(children))),
        "ul" | "ol" => format!(
            "{}\n",
            list(
                name == "ol",
                attr(attrs, "start")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1),
                children
            )
        ),
        "table" => table(children),
        "hr" => "---\n\n".to_string(),
        "li" => format!("- {}\n", blocks(children).trim()),
        "details" => blocks(children),
        "summary" => format!("**{}**\n\n", inline_nodes(children.iter()).trim()),
        "dt" => format!("**{}**\n\n", inline_nodes(children.iter()).trim()),
        _ => blocks(children),
    }
}

fn list(ordered: bool, start: u64, items: &[Node]) -> String {
    let mut out = String::new();
    let mut number = start;
    for item in items {
        let Node::Element { name, children, .. } = item else {
            continue;
        };
        if name != "li" {
            continue;
        }
        let marker = if ordered {
            format!("{number}. ")
        } else {
            "- ".to_string()
        };
        number += 1;
        let body = tighten(blocks(children).trim());
        let body = body.as_str();
        let indent = " ".repeat(marker.len());
        let mut lines = body.lines();
        let _ = write!(out, "{marker}{}", lines.next().unwrap_or(""));
        for line in lines {
            out.push('\n');
            if !line.is_empty() {
                out.push_str(&indent);
                out.push_str(line);
            }
        }
        out.push('\n');
    }
    out
}

fn tighten(body: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let starts_list = |line: &str| {
        line.starts_with("- ")
            || line
                .split_once(". ")
                .is_some_and(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let drop = line.is_empty()
            && i > 0
            && !lines[i - 1].is_empty()
            && lines.get(i + 1).is_some_and(|next| starts_list(next));
        if !drop {
            out.push(*line);
        }
    }
    out.join("\n")
}

fn rows(nodes: &[Node], out: &mut Vec<(bool, Vec<String>)>) {
    for node in nodes {
        let Node::Element { name, children, .. } = node else {
            continue;
        };
        match name.as_str() {
            "thead" | "tbody" | "tfoot" => rows(children, out),
            "tr" => {
                let mut header = false;
                let mut cells = Vec::new();
                for cell in children {
                    if let Node::Element { name, children, .. } = cell
                        && (name == "th" || name == "td")
                    {
                        header |= name == "th";
                        let text = inline_nodes(children.iter());
                        cells.push(
                            text.split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ")
                                .replace('|', "\\|"),
                        );
                    }
                }
                out.push((header, cells));
            }
            _ => {}
        }
    }
}

fn table(children: &[Node]) -> String {
    let mut all = Vec::new();
    rows(children, &mut all);
    if all.is_empty() {
        return String::new();
    }
    let columns = all.iter().map(|r| r.1.len()).max().unwrap_or(0).max(1);
    let line = |cells: &[String]| {
        let mut cells = cells.to_vec();
        cells.resize(columns, String::new());
        format!("| {} |", cells.join(" | "))
    };
    let mut out = String::new();
    let _ = writeln!(out, "{}", line(&all[0].1));
    let _ = write!(out, "|{}", " --- |".repeat(columns));
    for row in &all[1..] {
        let _ = write!(out, "\n{}", line(&row.1));
    }
    out.push_str("\n\n");
    out
}

fn escape(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let alnum = |j: Option<usize>| {
            j.and_then(|j| chars.get(j))
                .is_some_and(|c| c.is_alphanumeric())
        };
        match c {
            '\\' | '*' | '`' | '[' | ']' | '<' => {
                out.push('\\');
                out.push(c);
            }
            '_' if !(alnum(i.checked_sub(1)) && alnum(Some(i + 1))) => {
                out.push('\\');
                out.push(c);
            }
            other => out.push(other),
        }
    }
    out
}

fn inline_nodes<'a>(nodes: impl Iterator<Item = &'a Node>) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(t) => {
                let collapsed = t
                    .split_inclusive(char::is_whitespace)
                    .map(|p| p.to_string())
                    .collect::<String>();
                let mut spaced = String::new();
                let mut last_space = out.ends_with(' ') || out.ends_with('\n');
                for c in collapsed.chars() {
                    if c.is_whitespace() {
                        if !last_space {
                            spaced.push(' ');
                        }
                        last_space = true;
                    } else {
                        spaced.push(c);
                        last_space = false;
                    }
                }
                out.push_str(&escape(&spaced));
            }
            Node::Element {
                name,
                attrs,
                children,
            } => {
                let inner = || inline_nodes(children.iter());
                match name.as_str() {
                    "br" => out.push_str("  \n"),
                    "strong" | "b" => wrap(&mut out, "**", &inner()),
                    "em" | "i" => wrap(&mut out, "*", &inner()),
                    "del" | "s" | "strike" => wrap(&mut out, "~~", &inner()),
                    "mark" => wrap(&mut out, "==", &inner()),
                    "code" | "kbd" | "samp" => {
                        let text = text_of(children);
                        let fence = "`"
                            .repeat(text.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1);
                        let pad = if text.starts_with('`') || text.ends_with('`') {
                            " "
                        } else {
                            ""
                        };
                        let _ = write!(out, "{fence}{pad}{text}{pad}{fence}");
                    }
                    "a" => {
                        let label = inner();
                        match attr(attrs, "href").filter(|h| !h.is_empty() && is_safe_url(h, false))
                        {
                            Some(href) => {
                                let label = if label.trim().is_empty() {
                                    href.to_string()
                                } else {
                                    label.trim().to_string()
                                };
                                let _ = write!(out, "[{label}]({})", href.replace(' ', "%20"));
                            }
                            None => out.push_str(&label),
                        }
                    }
                    "img" => {
                        if let Some(src) =
                            attr(attrs, "src").filter(|s| !s.is_empty() && is_safe_url(s, true))
                        {
                            let _ = write!(
                                out,
                                "![{}]({})",
                                attr(attrs, "alt").unwrap_or("").replace(']', "\\]"),
                                src.replace(' ', "%20")
                            );
                        }
                    }
                    _ => out.push_str(&inner()),
                }
            }
        }
    }
    out
}

fn wrap(out: &mut String, mark: &str, inner: &str) {
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        out.push_str(inner);
        return;
    }
    if inner.starts_with(' ') {
        out.push(' ');
    }
    out.push_str(mark);
    out.push_str(trimmed);
    out.push_str(mark);
    if inner.ends_with(' ') {
        out.push(' ');
    }
}
