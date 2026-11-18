use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

use super::{Spans, Token, char_at, is_ident_char, is_ident_start, line_end};

pub(super) struct Str {
    open: &'static str,
    close: &'static str,
    escape: bool,
    multiline: bool,
    doubled: bool,
}

const fn string(open: &'static str, close: &'static str, escape: bool, multiline: bool) -> Str {
    Str {
        open,
        close,
        escape,
        multiline,
        doubled: false,
    }
}

pub(super) struct Syntax {
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    strings: &'static [Str],
    string_prefixes: &'static [&'static str],
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    constants: &'static [&'static str],
    declarations: &'static [(&'static str, Token)],
    sigils: &'static str,
    case_insensitive: bool,
    capitalized_types: bool,
    caps_constants: bool,
    attributes: bool,
    preprocessor: bool,
    macros: bool,
    index: usize,
}

static WORDS: [OnceLock<HashMap<&'static str, Token>>; 14] = [const { OnceLock::new() }; 14];

impl Syntax {
    fn words(&self) -> &'static HashMap<&'static str, Token> {
        WORDS[self.index].get_or_init(|| {
            let mut map = HashMap::new();
            for w in self.keywords {
                map.insert(*w, Token::Keyword);
            }
            for w in self.types {
                map.insert(*w, Token::Type);
            }
            for w in self.constants {
                map.insert(*w, Token::Constant);
            }
            map
        })
    }
}

const NONE: &[&str] = &[];
const QUOTES: &[Str] = &[
    string("\"", "\"", true, false),
    string("'", "'", true, false),
];

const fn base() -> Syntax {
    Syntax {
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        strings: QUOTES,
        string_prefixes: NONE,
        keywords: NONE,
        types: NONE,
        constants: NONE,
        declarations: &[],
        sigils: "",
        case_insensitive: false,
        capitalized_types: false,
        caps_constants: false,
        attributes: false,
        preprocessor: false,
        macros: false,
        index: 0,
    }
}

static PYTHON: Syntax = Syntax {
    index: 0,
    line_comments: &["#"],
    block_comment: None,
    strings: &[
        string("\"\"\"", "\"\"\"", true, true),
        string("'''", "'''", true, true),
        string("\"", "\"", true, false),
        string("'", "'", true, false),
    ],
    string_prefixes: &["r", "f", "b", "u", "rb", "br", "fr", "rf", "R", "F", "B"],
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
        "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with",
        "yield", "match", "case",
    ],
    types: &[
        "int",
        "str",
        "float",
        "bool",
        "list",
        "dict",
        "set",
        "tuple",
        "bytes",
        "object",
        "bytearray",
    ],
    constants: &["True", "False", "None", "self", "cls"],
    declarations: &[("def", Token::Function), ("class", Token::Type)],
    capitalized_types: true,
    caps_constants: true,
    ..base()
};

static C: Syntax = Syntax {
    index: 1,
    keywords: &[
        "auto", "break", "case", "const", "continue", "default", "do", "else", "enum", "extern",
        "for", "goto", "if", "inline", "register", "restrict", "return", "sizeof", "static",
        "struct", "switch", "typedef", "union", "volatile", "while",
    ],
    types: &[
        "void", "char", "short", "int", "long", "float", "double", "signed", "unsigned", "size_t",
        "ssize_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t", "int8_t", "int16_t", "int32_t",
        "int64_t", "bool", "FILE", "wchar_t", "DWORD", "HANDLE", "LPVOID", "BYTE", "WORD", "BOOL",
        "PVOID", "LPCSTR", "HMODULE", "NTSTATUS", "SOCKET", "ULONG", "USHORT", "UCHAR", "PBYTE",
    ],
    constants: &["NULL", "true", "false", "EOF"],
    caps_constants: true,
    preprocessor: true,
    ..base()
};

static CPP: Syntax = Syntax {
    index: 2,
    keywords: &[
        "alignas",
        "auto",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "constexpr",
        "continue",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "explicit",
        "extern",
        "final",
        "for",
        "friend",
        "goto",
        "if",
        "inline",
        "namespace",
        "new",
        "noexcept",
        "operator",
        "override",
        "private",
        "protected",
        "public",
        "return",
        "sizeof",
        "static",
        "static_cast",
        "dynamic_cast",
        "reinterpret_cast",
        "const_cast",
        "struct",
        "switch",
        "template",
        "this",
        "throw",
        "try",
        "typedef",
        "typename",
        "union",
        "using",
        "virtual",
        "volatile",
        "while",
    ],
    types: &[
        "void", "char", "short", "int", "long", "float", "double", "signed", "unsigned", "size_t",
        "bool", "string", "vector", "map", "set", "wstring", "uint8_t", "uint16_t", "uint32_t",
        "uint64_t", "int8_t", "int16_t", "int32_t", "int64_t", "DWORD", "HANDLE", "LPVOID", "BYTE",
    ],
    constants: &["nullptr", "NULL", "true", "false"],
    declarations: &[
        ("class", Token::Type),
        ("struct", Token::Type),
        ("namespace", Token::Type),
    ],
    capitalized_types: true,
    caps_constants: true,
    preprocessor: true,
    ..base()
};

static CSHARP: Syntax = Syntax {
    index: 3,
    strings: &[
        string("@\"", "\"", false, true),
        string("$\"", "\"", true, false),
        string("\"", "\"", true, false),
        string("'", "'", true, false),
    ],
    keywords: &[
        "abstract",
        "as",
        "async",
        "await",
        "base",
        "break",
        "case",
        "catch",
        "checked",
        "class",
        "const",
        "continue",
        "default",
        "delegate",
        "do",
        "else",
        "enum",
        "event",
        "explicit",
        "extern",
        "finally",
        "fixed",
        "for",
        "foreach",
        "get",
        "goto",
        "if",
        "implicit",
        "in",
        "interface",
        "internal",
        "is",
        "lock",
        "namespace",
        "new",
        "operator",
        "out",
        "override",
        "params",
        "partial",
        "private",
        "protected",
        "public",
        "readonly",
        "ref",
        "return",
        "sealed",
        "set",
        "static",
        "struct",
        "switch",
        "this",
        "throw",
        "try",
        "typeof",
        "unchecked",
        "unsafe",
        "using",
        "var",
        "virtual",
        "volatile",
        "while",
        "yield",
    ],
    types: &[
        "int", "string", "bool", "double", "float", "long", "short", "byte", "char", "object",
        "decimal", "void", "uint", "ulong", "ushort", "sbyte", "dynamic", "IntPtr", "Task", "List",
    ],
    constants: &["true", "false", "null"],
    declarations: &[
        ("class", Token::Type),
        ("struct", Token::Type),
        ("interface", Token::Type),
        ("namespace", Token::Type),
    ],
    capitalized_types: true,
    attributes: true,
    preprocessor: true,
    ..base()
};

static JAVA: Syntax = Syntax {
    index: 4,
    keywords: &[
        "abstract",
        "assert",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "default",
        "do",
        "else",
        "enum",
        "extends",
        "final",
        "finally",
        "for",
        "goto",
        "if",
        "implements",
        "import",
        "instanceof",
        "interface",
        "native",
        "new",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "static",
        "strictfp",
        "super",
        "switch",
        "synchronized",
        "this",
        "throw",
        "throws",
        "transient",
        "try",
        "volatile",
        "while",
        "var",
        "record",
    ],
    types: &[
        "boolean", "byte", "char", "double", "float", "int", "long", "short", "void", "String",
        "Object", "List", "Map",
    ],
    constants: &["true", "false", "null"],
    declarations: &[
        ("class", Token::Type),
        ("interface", Token::Type),
        ("enum", Token::Type),
    ],
    capitalized_types: true,
    caps_constants: true,
    attributes: true,
    ..base()
};

const JS_KEYWORDS: &[&str] = &[
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "finally",
    "for",
    "from",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "let",
    "new",
    "of",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

static JAVASCRIPT: Syntax = Syntax {
    index: 5,
    strings: &[
        string("`", "`", true, true),
        string("\"", "\"", true, false),
        string("'", "'", true, false),
    ],
    keywords: JS_KEYWORDS,
    types: &[
        "Object", "Array", "Promise", "Map", "Set", "String", "Number", "Boolean",
    ],
    constants: &["true", "false", "null", "undefined", "NaN", "Infinity"],
    declarations: &[("function", Token::Function), ("class", Token::Type)],
    capitalized_types: true,
    caps_constants: true,
    ..base()
};

static TYPESCRIPT: Syntax = Syntax {
    index: 6,
    strings: &[
        string("`", "`", true, true),
        string("\"", "\"", true, false),
        string("'", "'", true, false),
    ],
    keywords: &[
        "abstract",
        "as",
        "async",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "declare",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "finally",
        "for",
        "from",
        "function",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "interface",
        "is",
        "keyof",
        "let",
        "namespace",
        "new",
        "of",
        "private",
        "protected",
        "public",
        "readonly",
        "return",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "try",
        "type",
        "typeof",
        "var",
        "void",
        "while",
        "yield",
    ],
    types: &[
        "string", "number", "boolean", "any", "unknown", "never", "object", "symbol", "bigint",
        "Promise", "Array", "Record",
    ],
    constants: &["true", "false", "null", "undefined", "NaN", "Infinity"],
    declarations: &[
        ("function", Token::Function),
        ("class", Token::Type),
        ("interface", Token::Type),
        ("type", Token::Type),
    ],
    capitalized_types: true,
    caps_constants: true,
    attributes: true,
    ..base()
};

static GO: Syntax = Syntax {
    index: 7,
    strings: &[
        string("`", "`", false, true),
        string("\"", "\"", true, false),
        string("'", "'", true, false),
    ],
    keywords: &[
        "break",
        "case",
        "chan",
        "const",
        "continue",
        "default",
        "defer",
        "else",
        "fallthrough",
        "for",
        "func",
        "go",
        "goto",
        "if",
        "import",
        "interface",
        "map",
        "package",
        "range",
        "return",
        "select",
        "struct",
        "switch",
        "type",
        "var",
    ],
    types: &[
        "int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16", "uint32", "uint64",
        "uintptr", "float32", "float64", "string", "bool", "byte", "rune", "error", "any",
    ],
    constants: &["true", "false", "nil", "iota"],
    declarations: &[("func", Token::Function), ("type", Token::Type)],
    capitalized_types: true,
    ..base()
};

static RUST: Syntax = Syntax {
    index: 8,
    strings: &[
        string("r#\"", "\"#", false, true),
        string("\"", "\"", true, true),
        string("'", "'", true, false),
    ],
    string_prefixes: &["b", "r", "br"],
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
        "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "type",
        "unsafe", "use", "where", "while",
    ],
    types: &[
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64", "bool", "char", "str", "String", "Vec", "Option", "Result", "Box",
    ],
    constants: &["true", "false", "None", "Some", "Ok", "Err"],
    declarations: &[
        ("fn", Token::Function),
        ("struct", Token::Type),
        ("enum", Token::Type),
        ("trait", Token::Type),
    ],
    capitalized_types: true,
    caps_constants: true,
    attributes: true,
    macros: true,
    ..base()
};

static PHP: Syntax = Syntax {
    index: 9,
    line_comments: &["//", "#"],
    keywords: &[
        "abstract",
        "and",
        "array",
        "as",
        "break",
        "callable",
        "case",
        "catch",
        "class",
        "clone",
        "const",
        "continue",
        "declare",
        "default",
        "do",
        "echo",
        "else",
        "elseif",
        "empty",
        "extends",
        "final",
        "finally",
        "fn",
        "for",
        "foreach",
        "function",
        "global",
        "if",
        "implements",
        "include",
        "include_once",
        "instanceof",
        "interface",
        "isset",
        "list",
        "match",
        "namespace",
        "new",
        "or",
        "print",
        "private",
        "protected",
        "public",
        "readonly",
        "require",
        "require_once",
        "return",
        "static",
        "switch",
        "throw",
        "trait",
        "try",
        "unset",
        "use",
        "var",
        "while",
        "xor",
        "yield",
        "<?php",
        "?>",
        "<?=",
    ],
    types: &[
        "int", "string", "bool", "float", "array", "object", "void", "mixed",
    ],
    constants: &["true", "false", "null", "__FILE__", "__DIR__", "__LINE__"],
    declarations: &[("function", Token::Function), ("class", Token::Type)],
    sigils: "$",
    case_insensitive: true,
    capitalized_types: true,
    ..base()
};

static RUBY: Syntax = Syntax {
    index: 10,
    line_comments: &["#"],
    block_comment: None,
    keywords: &[
        "alias",
        "and",
        "begin",
        "break",
        "case",
        "class",
        "def",
        "defined?",
        "do",
        "else",
        "elsif",
        "end",
        "ensure",
        "for",
        "if",
        "in",
        "module",
        "next",
        "not",
        "or",
        "redo",
        "rescue",
        "retry",
        "return",
        "then",
        "undef",
        "unless",
        "until",
        "when",
        "while",
        "yield",
        "require",
        "require_relative",
        "include",
        "extend",
        "attr_accessor",
        "attr_reader",
        "attr_writer",
        "puts",
        "private",
        "public",
        "protected",
    ],
    constants: &["true", "false", "nil", "self", "super"],
    declarations: &[
        ("def", Token::Function),
        ("class", Token::Type),
        ("module", Token::Type),
    ],
    sigils: "@$",
    capitalized_types: true,
    ..base()
};

static PERL: Syntax = Syntax {
    index: 11,
    line_comments: &["#"],
    block_comment: None,
    keywords: &[
        "my", "our", "local", "sub", "use", "no", "package", "require", "if", "elsif", "else",
        "unless", "while", "until", "for", "foreach", "do", "return", "last", "next", "redo",
        "and", "or", "not", "xor", "eq", "ne", "lt", "gt", "le", "ge", "cmp", "print", "printf",
        "say", "die", "warn", "open", "close", "chomp", "split", "join", "push", "pop", "shift",
        "unshift", "keys", "values", "defined", "ref", "bless", "BEGIN", "END",
    ],
    declarations: &[("sub", Token::Function), ("package", Token::Type)],
    sigils: "$@",
    ..base()
};

static LUA: Syntax = Syntax {
    index: 12,
    line_comments: &["--"],
    block_comment: Some(("--[[", "]]")),
    strings: &[
        string("[[", "]]", false, true),
        string("\"", "\"", true, false),
        string("'", "'", true, false),
    ],
    keywords: &[
        "and", "break", "do", "else", "elseif", "end", "for", "function", "goto", "if", "in",
        "local", "not", "or", "repeat", "return", "then", "until", "while",
    ],
    constants: &["true", "false", "nil"],
    declarations: &[("function", Token::Function)],
    ..base()
};

static SQL: Syntax = Syntax {
    index: 13,
    line_comments: &["--", "#"],
    strings: &[
        Str {
            open: "'",
            close: "'",
            escape: true,
            multiline: true,
            doubled: true,
        },
        Str {
            open: "\"",
            close: "\"",
            escape: false,
            multiline: false,
            doubled: true,
        },
        string("`", "`", false, false),
    ],
    keywords: &[
        "select",
        "from",
        "where",
        "and",
        "or",
        "not",
        "insert",
        "into",
        "values",
        "update",
        "set",
        "delete",
        "create",
        "table",
        "alter",
        "drop",
        "index",
        "view",
        "join",
        "inner",
        "left",
        "right",
        "outer",
        "full",
        "cross",
        "on",
        "as",
        "group",
        "by",
        "order",
        "having",
        "limit",
        "offset",
        "union",
        "all",
        "distinct",
        "in",
        "is",
        "like",
        "between",
        "exists",
        "case",
        "when",
        "then",
        "else",
        "end",
        "primary",
        "key",
        "foreign",
        "references",
        "default",
        "unique",
        "check",
        "constraint",
        "database",
        "use",
        "grant",
        "revoke",
        "exec",
        "execute",
        "declare",
        "begin",
        "commit",
        "rollback",
        "truncate",
        "with",
        "asc",
        "desc",
        "if",
        "show",
        "describe",
        "waitfor",
        "delay",
        "into",
        "outfile",
        "load_file",
        "information_schema",
    ],
    types: &[
        "int",
        "integer",
        "bigint",
        "smallint",
        "tinyint",
        "varchar",
        "nvarchar",
        "char",
        "text",
        "date",
        "datetime",
        "timestamp",
        "boolean",
        "bool",
        "float",
        "double",
        "decimal",
        "numeric",
        "blob",
        "varbinary",
        "uniqueidentifier",
    ],
    constants: &["null", "true", "false"],
    sigils: "@",
    case_insensitive: true,
    ..base()
};

pub(super) fn syntax(id: &str) -> Option<&'static Syntax> {
    Some(match id {
        "python" => &PYTHON,
        "c" => &C,
        "cpp" => &CPP,
        "csharp" => &CSHARP,
        "java" => &JAVA,
        "javascript" => &JAVASCRIPT,
        "typescript" => &TYPESCRIPT,
        "go" => &GO,
        "rust" => &RUST,
        "php" => &PHP,
        "ruby" => &RUBY,
        "perl" => &PERL,
        "lua" => &LUA,
        "sql" => &SQL,
        _ => return None,
    })
}

pub(super) fn lex(code: &str, range: Range<usize>, syn: &Syntax, out: &mut Spans) {
    let words = syn.words();
    let end = range.end;
    let mut i = range.start;
    let mut pending: Option<Token> = None;
    while i < end {
        let Some(c) = char_at(code, i) else { break };
        let rest = &code[i..end];
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        if syn.preprocessor && c == '#' && at_line_start(code, i) {
            let stop = line_end(code, i, end);
            let word_len = 1 + rest[1..]
                .find(|ch: char| !ch.is_alphabetic())
                .unwrap_or(rest.len() - 1);
            out.push(i..i + word_len, Token::Keyword);
            let argument = code[i + word_len..stop].trim_start_matches(' ');
            let argument_start = stop - argument.len();
            let close = match argument.chars().next() {
                Some('<') => Some('>'),
                Some('"') => Some('"'),
                _ => None,
            };
            if let Some(close) = close {
                let argument_end = argument[1..]
                    .find(close)
                    .map_or(stop, |o| argument_start + o + 2);
                out.push(argument_start..argument_end, Token::String);
                i = argument_end;
            } else {
                i += word_len;
            }
            continue;
        }
        if syn.line_comments.iter().any(|m| rest.starts_with(*m)) {
            let stop = line_end(code, i, end);
            out.push(i..stop, Token::Comment);
            i = stop;
            continue;
        }
        if let Some((open, close)) = syn.block_comment
            && rest.starts_with(open)
        {
            let stop = rest[open.len()..]
                .find(close)
                .map_or(end, |o| i + open.len() + o + close.len());
            out.push(i..stop, Token::Comment);
            i = stop;
            continue;
        }
        if is_ident_start(c) && !syn.string_prefixes.is_empty() {
            let word_len = rest
                .find(|ch: char| !is_ident_char(ch))
                .unwrap_or(rest.len());
            if syn.string_prefixes.contains(&&rest[..word_len])
                && matches!(rest[word_len..].chars().next(), Some('"' | '\'' | '#'))
                && let Some(stop) = lex_string(code, i + word_len, end, syn)
            {
                out.push(i..stop, Token::String);
                i = stop;
                continue;
            }
        }
        if syn.strings.iter().any(|s| rest.starts_with(s.open)) {
            if c == '\'' && syn.macros && is_lifetime(rest) {
                let len = 1 + rest[1..]
                    .find(|ch: char| !is_ident_char(ch))
                    .unwrap_or(rest.len() - 1);
                out.push(i..i + len, Token::Type);
                i += len;
                continue;
            }
            if let Some(stop) = lex_string(code, i, end, syn) {
                out.push(i..stop, Token::String);
                i = stop;
                continue;
            }
        }
        if syn.attributes && (c == '@' || (c == '#' && rest[1..].starts_with(['[', '!']))) {
            let stop = if c == '#' {
                rest.find(']').map_or(line_end(code, i, end), |o| i + o + 1)
            } else {
                i + 1
                    + rest[1..]
                        .find(|ch: char| !(is_ident_char(ch) || ch == '.'))
                        .unwrap_or(rest.len() - 1)
            };
            out.push(i..stop, Token::Attribute);
            i = stop;
            continue;
        }
        if c == '['
            && syn.attributes
            && at_line_start(code, i)
            && !syn.macros
            && let Some(close) = rest.find(']')
            && rest[1..close].chars().all(|ch| {
                is_ident_char(ch)
                    || ch == '('
                    || ch == ')'
                    || ch == '"'
                    || ch == ','
                    || ch == ' '
                    || ch == '.'
                    || ch == '='
            })
            && !rest[..close].contains('\n')
        {
            out.push(i..i + close + 1, Token::Attribute);
            i += close + 1;
            continue;
        }
        if syn.sigils.contains(c)
            && rest[c.len_utf8()..]
                .chars()
                .next()
                .is_some_and(|n| is_ident_start(n) || (c == '@' && n == '@') || n == '$')
        {
            let mut len = c.len_utf8();
            while let Some(n) = char_at(code, i + len) {
                if is_ident_char(n) || n == '@' || n == '$' {
                    len += n.len_utf8();
                } else {
                    break;
                }
            }
            out.push(i..i + len, Token::Variable);
            i += len;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && rest[1..].starts_with(|n: char| n.is_ascii_digit())) {
            let mut len = c.len_utf8();
            let bytes = rest.as_bytes();
            while len < bytes.len() {
                let b = bytes[len];
                let exponent_sign = matches!(b, b'+' | b'-')
                    && matches!(bytes[len - 1], b'e' | b'E')
                    && !rest.starts_with("0x")
                    && !rest.starts_with("0X");
                if b.is_ascii_alphanumeric()
                    || b == b'_'
                    || exponent_sign
                    || (b == b'.' && bytes.get(len + 1).is_some_and(u8::is_ascii_digit))
                {
                    len += 1;
                } else {
                    break;
                }
            }
            out.push(i..i + len, Token::Number);
            i += len;
            continue;
        }
        if is_ident_start(c)
            || (syn.keywords.contains(&"<?php")
                && (rest.starts_with("<?php") || rest.starts_with("<?=") || rest.starts_with("?>")))
        {
            if rest.starts_with("<?") || rest.starts_with("?>") {
                let len = if rest.starts_with("<?php") {
                    5
                } else {
                    2 + usize::from(rest.starts_with("<?="))
                };
                out.push(i..i + len.min(rest.len()), Token::Keyword);
                i += len.min(rest.len());
                continue;
            }
            let mut len = rest
                .find(|ch: char| !is_ident_char(ch))
                .unwrap_or(rest.len());
            if syn.keywords.contains(&"defined?") && rest[len..].starts_with(['?', '!']) {
                len += 1;
            }
            let word = &rest[..len];
            let lowered;
            let key = if syn.case_insensitive {
                lowered = word.to_lowercase();
                lowered.as_str()
            } else {
                word
            };
            let after = rest[len..].trim_start_matches([' ', '\t']);
            let after_char = after.chars().next();
            let previous = prev_significant(code, i);
            let token = if let Some(t) = pending.take() {
                Some(t)
            } else if let Some(&t) = words.get(key) {
                if t == Token::Keyword {
                    pending = syn
                        .declarations
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|&(_, t)| t);
                }
                Some(t)
            } else if after_char == Some('(')
                || (syn.macros && after_char == Some('!') && !after.starts_with("!="))
            {
                Some(Token::Function)
            } else if previous == Some('.') {
                Some(Token::Property)
            } else if syn.caps_constants
                && word.len() > 1
                && word
                    .chars()
                    .all(|ch| ch.is_uppercase() || ch.is_ascii_digit() || ch == '_')
            {
                Some(Token::Constant)
            } else if syn.capitalized_types && word.chars().next().is_some_and(char::is_uppercase) {
                Some(Token::Type)
            } else {
                None
            };
            if let Some(t) = token {
                out.push(i..i + len, t);
            }
            i += len;
            continue;
        }
        pending = None;
        let token = if "+-*/%=<>!&|^~?:".contains(c) {
            Token::Operator
        } else {
            Token::Punctuation
        };
        if !c.is_alphanumeric() && c != '\\' && c != '@' && c != '$' && c != '`' {
            out.push(i..i + c.len_utf8(), token);
        }
        i += c.len_utf8();
    }
}

fn at_line_start(code: &str, i: usize) -> bool {
    code[..i]
        .rsplit('\n')
        .next()
        .is_some_and(|before| before.trim().is_empty())
}

fn prev_significant(code: &str, i: usize) -> Option<char> {
    code[..i]
        .chars()
        .rev()
        .take_while(|&c| c != '\n')
        .find(|c| !c.is_whitespace())
}

fn is_lifetime(rest: &str) -> bool {
    let mut chars = rest[1..].chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_ident_start(first) {
        return false;
    }
    let after = rest[1 + first.len_utf8()..].chars().next();
    after != Some('\'')
}

fn lex_string(code: &str, i: usize, end: usize, syn: &Syntax) -> Option<usize> {
    let rest = &code[i..end];
    let rule = syn.strings.iter().find(|s| rest.starts_with(s.open))?;
    let mut j = i + rule.open.len();
    while j < end {
        let c = char_at(code, j)?;
        if rule.escape && c == '\\' {
            j += c.len_utf8() + char_at(code, j + 1).map_or(0, char::len_utf8);
            continue;
        }
        if code[j..end].starts_with(rule.close) {
            if rule.doubled && code[j + rule.close.len()..end].starts_with(rule.close) {
                j += rule.close.len() * 2;
                continue;
            }
            return Some(j + rule.close.len());
        }
        if c == '\n' && !rule.multiline {
            return Some(j);
        }
        j += c.len_utf8();
    }
    Some(end)
}
