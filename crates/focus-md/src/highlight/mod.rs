mod commands;
mod data;
mod detect;
mod generic;
mod shell;

use std::ops::Range;

pub use detect::{Detected, Hints, detect, detect_in_note};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Token {
    Keyword,
    Function,
    Type,
    String,
    Number,
    Constant,
    Variable,
    Property,
    Flag,
    Operator,
    Punctuation,
    Comment,
    Prompt,
    Tag,
    Attribute,
    Inserted,
    Deleted,
    Heading,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub token: Token,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    pub id: &'static str,
    pub label: &'static str,
}

const LANGUAGES: &[Language] = &[
    Language {
        id: "bash",
        label: "Bash",
    },
    Language {
        id: "powershell",
        label: "PowerShell",
    },
    Language {
        id: "cmd",
        label: "Command prompt",
    },
    Language {
        id: "console",
        label: "Console",
    },
    Language {
        id: "python",
        label: "Python",
    },
    Language {
        id: "c",
        label: "C",
    },
    Language {
        id: "cpp",
        label: "C++",
    },
    Language {
        id: "csharp",
        label: "C#",
    },
    Language {
        id: "java",
        label: "Java",
    },
    Language {
        id: "javascript",
        label: "JavaScript",
    },
    Language {
        id: "typescript",
        label: "TypeScript",
    },
    Language {
        id: "go",
        label: "Go",
    },
    Language {
        id: "rust",
        label: "Rust",
    },
    Language {
        id: "php",
        label: "PHP",
    },
    Language {
        id: "ruby",
        label: "Ruby",
    },
    Language {
        id: "perl",
        label: "Perl",
    },
    Language {
        id: "lua",
        label: "Lua",
    },
    Language {
        id: "sql",
        label: "SQL",
    },
    Language {
        id: "json",
        label: "JSON",
    },
    Language {
        id: "yaml",
        label: "YAML",
    },
    Language {
        id: "toml",
        label: "TOML",
    },
    Language {
        id: "xml",
        label: "XML",
    },
    Language {
        id: "html",
        label: "HTML",
    },
    Language {
        id: "css",
        label: "CSS",
    },
    Language {
        id: "ini",
        label: "INI",
    },
    Language {
        id: "dockerfile",
        label: "Dockerfile",
    },
    Language {
        id: "diff",
        label: "Diff",
    },
    Language {
        id: "http",
        label: "HTTP request",
    },
    Language {
        id: "asm",
        label: "Assembly",
    },
    Language {
        id: "text",
        label: "Plain text",
    },
];

const ALIASES: &[(&str, &str)] = &[
    ("sh", "bash"),
    ("shell", "bash"),
    ("zsh", "bash"),
    ("ksh", "bash"),
    ("fish", "bash"),
    ("shellscript", "bash"),
    ("ps1", "powershell"),
    ("ps", "powershell"),
    ("pwsh", "powershell"),
    ("psm1", "powershell"),
    ("bat", "cmd"),
    ("batch", "cmd"),
    ("dos", "cmd"),
    ("shell-session", "console"),
    ("shellsession", "console"),
    ("terminal", "console"),
    ("sh-session", "console"),
    ("py", "python"),
    ("python3", "python"),
    ("h", "c"),
    ("c++", "cpp"),
    ("cxx", "cpp"),
    ("cc", "cpp"),
    ("hpp", "cpp"),
    ("cs", "csharp"),
    ("c#", "csharp"),
    ("js", "javascript"),
    ("jsx", "javascript"),
    ("mjs", "javascript"),
    ("node", "javascript"),
    ("ts", "typescript"),
    ("tsx", "typescript"),
    ("golang", "go"),
    ("rs", "rust"),
    ("rb", "ruby"),
    ("pl", "perl"),
    ("mysql", "sql"),
    ("postgresql", "sql"),
    ("psql", "sql"),
    ("sqlite", "sql"),
    ("jsonc", "json"),
    ("json5", "json"),
    ("yml", "yaml"),
    ("htm", "html"),
    ("xhtml", "html"),
    ("svg", "xml"),
    ("xaml", "xml"),
    ("plist", "xml"),
    ("scss", "css"),
    ("less", "css"),
    ("conf", "ini"),
    ("cfg", "ini"),
    ("properties", "ini"),
    ("docker", "dockerfile"),
    ("patch", "diff"),
    ("request", "http"),
    ("nasm", "asm"),
    ("armasm", "asm"),
    ("x86asm", "asm"),
    ("masm", "asm"),
    ("arm", "asm"),
    ("assembly", "asm"),
    ("s", "asm"),
    ("txt", "text"),
    ("plaintext", "text"),
    ("plain", "text"),
    ("none", "text"),
    ("output", "text"),
    ("log", "text"),
    ("ansi", "text"),
    ("nohighlight", "text"),
    ("raw", "text"),
];

pub fn canonical(lang: &str) -> Option<&'static str> {
    let lower = lang.trim().to_lowercase();
    if let Some(found) = LANGUAGES.iter().find(|l| l.id == lower) {
        return Some(found.id);
    }
    ALIASES
        .iter()
        .find(|(alias, _)| *alias == lower)
        .map(|&(_, id)| id)
}

pub fn label(lang: &str) -> Option<&'static str> {
    let id = canonical(lang)?;
    LANGUAGES.iter().find(|l| l.id == id).map(|l| l.label)
}

pub fn languages() -> &'static [Language] {
    LANGUAGES
}

pub fn highlight(code: &str, lang: &str) -> Vec<Span> {
    let Some(id) = canonical(lang) else {
        return Vec::new();
    };
    let mut out = Spans::new(code.len());
    let whole = 0..code.len();
    match id {
        "bash" => shell::bash(code, whole, &mut out),
        "powershell" => shell::powershell(code, whole, &mut out),
        "cmd" => shell::cmd(code, whole, &mut out),
        "console" => shell::console(code, &mut out),
        "dockerfile" => shell::dockerfile(code, &mut out),
        "json" => data::json(code, whole, &mut out),
        "yaml" => data::yaml(code, &mut out),
        "toml" => data::toml(code, &mut out),
        "ini" => data::ini(code, &mut out),
        "xml" | "html" => data::markup(code, whole, &mut out),
        "css" => data::css(code, whole, &mut out),
        "diff" => data::diff(code, &mut out),
        "http" => data::http(code, &mut out),
        "asm" => data::asm(code, &mut out),
        "text" => {}
        other => {
            if let Some(syntax) = generic::syntax(other) {
                generic::lex(code, whole, syntax, &mut out);
            }
        }
    }
    out.finish()
}

pub(crate) struct Spans {
    spans: Vec<Span>,
    len: usize,
}

impl Spans {
    fn new(len: usize) -> Self {
        Spans {
            spans: Vec::new(),
            len,
        }
    }

    pub(crate) fn push(&mut self, range: Range<usize>, token: Token) {
        let start = range
            .start
            .max(self.spans.last().map_or(0, |s| s.range.end));
        let end = range.end.min(self.len);
        if start >= end {
            return;
        }
        if let Some(last) = self.spans.last_mut()
            && last.token == token
            && last.range.end == start
        {
            last.range.end = end;
            return;
        }
        self.spans.push(Span {
            range: start..end,
            token,
        });
    }

    fn finish(self) -> Vec<Span> {
        self.spans
    }
}

pub(crate) fn char_at(code: &str, i: usize) -> Option<char> {
    code.get(i..)?.chars().next()
}

pub(crate) fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

pub(crate) fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub(crate) fn line_end(code: &str, i: usize, end: usize) -> usize {
    code[i..end].find('\n').map_or(end, |off| i + off)
}
