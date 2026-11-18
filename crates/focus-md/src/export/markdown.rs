use std::fmt::Write;

use super::Resolver;
use super::render::select_anchor;
use crate::{
    Align, Block, BlockKind, Directive, Image, Inline, InlineKind, LinkKind, ListItem,
    ParseOptions, Tab, TabsForm, callout_kind, parse,
};

const MAX_EMBED_DEPTH: usize = 3;

pub(super) fn to_standard(source: &str, resolver: &dyn Resolver) -> String {
    let doc = parse(source, &ParseOptions::default());
    let mut writer = Writer {
        resolver,
        embed_stack: Vec::new(),
    };
    let mut out = String::new();
    if let Some(front) = &doc.frontmatter {
        let _ = write!(out, "---\n{}\n---\n\n", front.raw);
    }
    out.push_str(&writer.blocks(&doc.blocks));
    for note in &doc.footnotes {
        let body = writer.blocks(&note.blocks);
        let mut lines = body.lines();
        let first = lines.next().unwrap_or("");
        let _ = write!(out, "\n[^{}]: {first}", note.label);
        for line in lines {
            if line.is_empty() {
                out.push('\n');
            } else {
                let _ = write!(out, "\n    {line}");
            }
        }
        out.push('\n');
    }
    let trimmed = out.trim_end();
    format!("{trimmed}\n")
}

struct Writer<'a> {
    resolver: &'a dyn Resolver,
    embed_stack: Vec<String>,
}

fn fence_for(code: &str) -> String {
    let longest = code.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

fn quote(text: &str) -> String {
    text.lines()
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

fn encode_target(target: &str) -> String {
    target.replace(' ', "%20")
}

impl Writer<'_> {
    fn blocks(&mut self, blocks: &[Block]) -> String {
        let parts: Vec<String> = blocks
            .iter()
            .map(|b| self.block(b))
            .filter(|s| !s.is_empty())
            .collect();
        parts.join("\n\n")
    }

    fn block(&mut self, block: &Block) -> String {
        match &block.kind {
            BlockKind::Heading { level, inlines, .. } => format!(
                "{} {}",
                "#".repeat(*level as usize),
                self.inlines(inlines, false)
            ),
            BlockKind::Paragraph(inlines) => {
                let text = self.inlines(inlines, false);
                escape_line_start(&text)
            }
            BlockKind::List {
                ordered,
                start,
                items,
                tight,
            } => self.list(*ordered, *start, items, *tight),
            BlockKind::CodeBlock { lang, code, .. } => {
                let fence = fence_for(code);
                format!("{fence}{lang}\n{code}\n{fence}")
            }
            BlockKind::Mermaid { source } => {
                let fence = fence_for(source);
                format!("{fence}mermaid\n{source}\n{fence}")
            }
            BlockKind::Math { tex } => format!("$$\n{tex}\n$$"),
            BlockKind::Quote(blocks) => quote(&self.blocks(blocks)),
            BlockKind::Callout {
                kind,
                title,
                blocks,
                ..
            } => {
                let title = if title.is_empty() {
                    callout_kind(kind).1
                } else {
                    self.inlines(title, false)
                };
                let body = self.blocks(blocks);
                if body.is_empty() {
                    quote(&format!("**{title}**"))
                } else {
                    quote(&format!("**{title}**\n\n{body}"))
                }
            }
            BlockKind::Table {
                align, head, rows, ..
            } => self.table(align, head, rows),
            BlockKind::Embed { target, anchor } => self.embed(target, anchor.as_deref()),
            BlockKind::Image(image) => self.image(image),
            BlockKind::Rule => "---".to_string(),
            BlockKind::Html(raw) => raw.clone(),
            BlockKind::Details {
                summary,
                open,
                blocks,
            } => {
                let summary = self.inlines(summary, false);
                let body = self.blocks(blocks);
                format!(
                    "<details{}>\n<summary>{summary}</summary>\n\n{body}\n\n</details>",
                    if *open { " open" } else { "" }
                )
            }
            BlockKind::Tabs { form, tabs } => self.tabs(*form, tabs),
            BlockKind::Directive(directive) => match directive {
                Directive::Ref { path } => format!("{{{{#ref}}}}\n{path}\n{{{{#endref}}}}"),
                Directive::Include { path } => {
                    let content = if self.embed_stack.is_empty() {
                        self.resolver.include(path)
                    } else {
                        None
                    };
                    match content {
                        Some(content) => {
                            let doc = parse(&content, &ParseOptions::default());
                            self.embed_stack.push(format!("include:{path}"));
                            let text = self.blocks(&doc.blocks);
                            self.embed_stack.pop();
                            text
                        }
                        None => format!("{{{{#include {path}}}}}"),
                    }
                }
                Directive::Other { name, text } if name == "file" => {
                    format!("{{{{#file}}}}\n{text}\n{{{{#endfile}}}}")
                }
                Directive::Other { name, text } => {
                    format!("{{{{#{name} {text}}}}}").replace(" }}", "}}")
                }
            },
        }
    }

    fn tabs(&mut self, form: TabsForm, tabs: &[Tab]) -> String {
        match form {
            TabsForm::Mdbook => {
                let mut out = vec!["{{#tabs}}".to_string()];
                for tab in tabs {
                    out.push(format!(
                        "{{{{#tab name=\"{}\"}}}}",
                        tab.title.replace('"', "'")
                    ));
                    let body = self.blocks(&tab.blocks);
                    if !body.is_empty() {
                        out.push(String::new());
                        out.push(body);
                        out.push(String::new());
                    }
                    out.push("{{#endtab}}".to_string());
                }
                out.push("{{#endtabs}}".to_string());
                out.join("\n")
            }
            TabsForm::Fenced => {
                let mut body = String::new();
                for tab in tabs {
                    let _ = write!(
                        body,
                        "tab: {}\n\n{}\n\n",
                        tab.title,
                        self.blocks(&tab.blocks)
                    );
                }
                let fence = "`".repeat(fence_for(&body).len().max(4));
                format!("{fence}tabs\n\n{}\n{fence}", body.trim_end())
            }
        }
    }

    fn list(&mut self, ordered: bool, start: u64, items: &[ListItem], tight: bool) -> String {
        let mut out = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let marker = if ordered {
                format!("{}. ", start + i as u64)
            } else {
                "- ".to_string()
            };
            let indent = " ".repeat(marker.len());
            let task = item
                .task
                .map(|state| format!("[{state}] "))
                .unwrap_or_default();
            let body = {
                let separator = if tight { "\n" } else { "\n\n" };
                let parts: Vec<String> = item
                    .blocks
                    .iter()
                    .map(|b| self.block(b))
                    .filter(|s| !s.is_empty())
                    .collect();
                parts.join(separator)
            };
            let mut lines = body.lines();
            let first = lines.next().unwrap_or("");
            let mut text = format!("{marker}{task}{first}");
            for line in lines {
                text.push('\n');
                if !line.is_empty() {
                    text.push_str(&indent);
                    text.push_str(line);
                }
            }
            out.push(text);
        }
        out.join(if tight { "\n" } else { "\n\n" })
    }

    fn table(
        &mut self,
        align: &[Align],
        head: &[Vec<Inline>],
        rows: &[Vec<Vec<Inline>>],
    ) -> String {
        let cell = |writer: &mut Self, inlines: &[Inline]| {
            writer.inlines(inlines, true).replace('\n', " ")
        };
        let mut out = String::new();
        let head_cells: Vec<String> = head.iter().map(|c| cell(self, c)).collect();
        let _ = writeln!(out, "| {} |", head_cells.join(" | "));
        let rule: Vec<&str> = (0..head.len())
            .map(|i| match align.get(i) {
                Some(Align::Left) => ":---",
                Some(Align::Center) => ":---:",
                Some(Align::Right) => "---:",
                _ => "---",
            })
            .collect();
        let _ = write!(out, "| {} |", rule.join(" | "));
        for row in rows {
            let cells: Vec<String> = row.iter().map(|c| cell(self, c)).collect();
            let _ = write!(out, "\n| {} |", cells.join(" | "));
        }
        out
    }

    fn embed(&mut self, target: &str, anchor: Option<&str>) -> String {
        let content = if self.embed_stack.len() < MAX_EMBED_DEPTH
            && !self.embed_stack.iter().any(|t| t == target)
        {
            self.resolver.embed(target)
        } else {
            None
        };
        let Some(content) = content else {
            return target.to_string();
        };
        let doc = parse(&content, &ParseOptions::default());
        let blocks = match anchor {
            Some(a) => select_anchor(&doc.blocks, a),
            None => doc.blocks,
        };
        self.embed_stack.push(target.to_string());
        let text = self.blocks(&blocks);
        self.embed_stack.pop();
        text
    }

    fn image(&self, image: &Image) -> String {
        let mut text = format!(
            "![{}]({})",
            image.alt.replace(']', "\\]"),
            encode_target(&image.src)
        );
        if let Some(caption) = image.caption.as_deref().filter(|c| *c != image.alt) {
            let _ = write!(text, "\n\n*{caption}*");
        }
        text
    }

    fn inlines(&mut self, inlines: &[Inline], in_table: bool) -> String {
        let mut out = String::new();
        for inline in inlines {
            match &inline.kind {
                InlineKind::Text(t) => out.push_str(&escape_text(t, in_table)),
                InlineKind::Code(c) => {
                    let fence =
                        "`".repeat(c.split(|ch| ch != '`').map(str::len).max().unwrap_or(0) + 1);
                    let pad = if c.starts_with('`') || c.ends_with('`') {
                        " "
                    } else {
                        ""
                    };
                    let c = if in_table {
                        c.replace('|', "\\|")
                    } else {
                        c.clone()
                    };
                    let _ = write!(out, "{fence}{pad}{c}{pad}{fence}");
                }
                InlineKind::Strong(c) => {
                    let _ = write!(out, "**{}**", self.inlines(c, in_table));
                }
                InlineKind::Emphasis(c) => {
                    let _ = write!(out, "*{}*", self.inlines(c, in_table));
                }
                InlineKind::Strike(c) => {
                    let _ = write!(out, "~~{}~~", self.inlines(c, in_table));
                }
                InlineKind::Highlight(c) => {
                    let _ = write!(out, "<mark>{}</mark>", self.inlines(c, in_table));
                }
                InlineKind::Link(link) => {
                    let label = self.inlines(&link.children, in_table);
                    match link.kind {
                        LinkKind::Auto => {
                            let _ = write!(out, "<{}>", link.target);
                        }
                        LinkKind::Wiki => {
                            match self.resolver.link(&link.target, link.anchor.as_deref()) {
                                Some(url) => {
                                    let _ = write!(out, "[{label}]({})", encode_target(&url));
                                }
                                None => out.push_str(&label),
                            }
                        }
                        LinkKind::Markdown => {
                            let mut url = encode_target(&link.target);
                            if let Some(anchor) = &link.anchor {
                                let _ = write!(url, "#{}", encode_target(anchor));
                            }
                            let _ = write!(out, "[{label}]({url})");
                        }
                    }
                }
                InlineKind::Image(image) => {
                    let _ = write!(
                        out,
                        "![{}]({})",
                        image.alt.replace(']', "\\]"),
                        encode_target(&image.src)
                    );
                }
                InlineKind::Tag(t) => {
                    let _ = write!(out, "#{t}");
                }
                InlineKind::Math(m) => {
                    let _ = write!(out, "${m}$");
                }
                InlineKind::FootnoteRef(label) => {
                    let _ = write!(out, "[^{label}]");
                }
                InlineKind::BlockId(_) => {}
                InlineKind::SoftBreak => out.push('\n'),
                InlineKind::HardBreak => out.push_str("  \n"),
                InlineKind::Html(raw) => out.push_str(raw),
            }
        }
        out.trim_end().to_string()
    }
}

fn escape_text(text: &str, in_table: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' | '*' | '_' | '[' | ']' | '`' | '<' | '~' => {
                out.push('\\');
                out.push(c);
            }
            '|' if in_table => out.push_str("\\|"),
            other => out.push(other),
        }
    }
    out
}

fn escape_line_start(text: &str) -> String {
    let first = text.chars().next();
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    let numbered = digits > 0 && text[digits..].starts_with(['.', ')']);
    match first {
        Some('#' | '>' | '-' | '+') => format!("\\{text}"),
        _ if numbered => format!("{}\\{}", &text[..digits], &text[digits..]),
        _ => text.to_string(),
    }
}
