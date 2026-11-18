mod callout;
mod html;
mod html_model;
mod origin;
mod parse;
mod tabs;
mod url;

use std::ops::Range;

#[derive(Debug, Clone, Default)]
pub struct Document {
    pub frontmatter: Option<Frontmatter>,
    pub blocks: Vec<Block>,
    pub footnotes: Vec<Footnote>,
    pub footnote_refs: Vec<FootnoteReference>,
    pub line_count: u32,
}

#[derive(Debug, Clone)]
pub struct Frontmatter {
    pub raw: String,
    pub lines: Range<u32>,
    pub bytes: Range<usize>,
    pub links: Vec<PropertyLink>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyLink {
    pub key: String,
    pub line: u32,
    pub target: String,
    pub anchor: Option<String>,
    pub display: String,
}

#[derive(Debug, Clone)]
pub struct Footnote {
    pub label: String,
    pub blocks: Vec<Block>,
    pub number: u32,
    pub references: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteReference {
    pub label: String,
    pub number: u32,
    pub definition: Option<usize>,
    pub occurrence: u32,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub lines: Range<u32>,
    pub bytes: Range<usize>,
    pub kind: BlockKind,
}

#[derive(Debug, Clone)]
pub enum BlockKind {
    Heading {
        level: u8,
        id: String,
        inlines: Vec<Inline>,
        marker: Range<usize>,
    },
    Paragraph(Vec<Inline>),
    List {
        ordered: bool,
        start: u64,
        tight: bool,
        items: Vec<ListItem>,
    },
    CodeBlock {
        lang: String,
        code: String,
        content: Range<usize>,
    },
    Mermaid {
        source: String,
    },
    Math {
        tex: String,
    },
    Quote(Vec<Block>),
    Callout {
        kind: String,
        title: Vec<Inline>,
        fold: Option<bool>,
        blocks: Vec<Block>,
        marker: Range<usize>,
    },
    Table {
        align: Vec<Align>,
        head: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
        cells: Vec<Vec<Range<usize>>>,
    },
    Embed {
        target: String,
        anchor: Option<String>,
    },
    Image(Image),
    Rule,
    Html(String),
    Details {
        summary: Vec<Inline>,
        open: bool,
        blocks: Vec<Block>,
    },
    Tabs {
        form: TabsForm,
        tabs: Vec<Tab>,
    },
    Directive(Directive),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabsForm {
    Fenced,
    Mdbook,
}

#[derive(Debug, Clone)]
pub struct Tab {
    pub title: String,
    pub lines: Range<u32>,
    pub title_line: u32,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Directive {
    Ref { path: String },
    Include { path: String },
    Other { name: String, text: String },
}

#[derive(Debug, Clone)]
pub struct ListItem {
    pub lines: Range<u32>,
    pub marker: Range<usize>,
    pub task: Option<char>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub struct Image {
    pub src: String,
    pub alt: String,
    pub caption: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Inline {
    pub kind: InlineKind,
    pub source: Range<usize>,
    exact: bool,
}

impl Inline {
    pub(crate) fn new(kind: InlineKind, source: Range<usize>, exact: bool) -> Self {
        let exact = exact && matches!(kind, InlineKind::Text(_));
        Inline {
            kind,
            source,
            exact,
        }
    }

    pub fn exact(&self) -> bool {
        self.exact
    }
}

#[derive(Debug, Clone)]
pub enum InlineKind {
    Text(String),
    Code(String),
    Strong(Vec<Inline>),
    Emphasis(Vec<Inline>),
    Strike(Vec<Inline>),
    Highlight(Vec<Inline>),
    Link(Link),
    Image(Image),
    Tag(String),
    Math(String),
    FootnoteRef(String),
    BlockId(String),
    SoftBreak,
    HardBreak,
    Html(String),
}

#[derive(Debug, Clone)]
pub struct Link {
    pub kind: LinkKind,
    pub target: String,
    pub anchor: Option<String>,
    pub children: Vec<Inline>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Wiki,
    Markdown,
    Auto,
}

#[derive(Debug, Clone)]
pub struct ParseOptions {
    pub hard_breaks: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions { hard_breaks: true }
    }
}

pub use callout::callout_kind;
pub use parse::{parse, plain_text};
pub use url::is_safe_url;

pub mod mermaid;

pub mod export;
pub mod highlight;
pub mod import;
