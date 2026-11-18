use std::ops::Range;

use super::{Spans, Token, char_at, generic, is_ident_char, is_ident_start, line_end};

pub(super) fn json(code: &str, range: Range<usize>, out: &mut Spans) {
    let end = range.end;
    let mut i = range.start;
    while i < end {
        let Some(c) = char_at(code, i) else { break };
        let rest = &code[i..end];
        match c {
            '"' => {
                let mut j = i + 1;
                while j < end {
                    match char_at(code, j) {
                        Some('\\') => j += 1 + char_at(code, j + 1).map_or(0, char::len_utf8),
                        Some('"') => {
                            j += 1;
                            break;
                        }
                        Some('\n') | None => break,
                        Some(ch) => j += ch.len_utf8(),
                    }
                }
                let is_key = code[j.min(end)..end]
                    .trim_start_matches([' ', '\t'])
                    .starts_with(':');
                out.push(
                    i..j,
                    if is_key {
                        Token::Property
                    } else {
                        Token::String
                    },
                );
                i = j;
            }
            '/' if rest.starts_with("//") => {
                let stop = line_end(code, i, end);
                out.push(i..stop, Token::Comment);
                i = stop;
            }
            '/' if rest.starts_with("/*") => {
                let stop = rest.find("*/").map_or(end, |o| i + o + 2);
                out.push(i..stop, Token::Comment);
                i = stop;
            }
            '{' | '}' | '[' | ']' | ',' | ':' => {
                out.push(i..i + 1, Token::Punctuation);
                i += 1;
            }
            '-' | '0'..='9' => {
                let len = rest
                    .find(|ch: char| !(ch.is_ascii_alphanumeric() || ".+-".contains(ch)))
                    .unwrap_or(rest.len())
                    .max(1);
                out.push(i..i + len, Token::Number);
                i += len;
            }
            _ if is_ident_start(c) => {
                let len = rest
                    .find(|ch: char| !is_ident_char(ch))
                    .unwrap_or(rest.len());
                if matches!(&rest[..len], "true" | "false" | "null") {
                    out.push(i..i + len, Token::Constant);
                }
                i += len;
            }
            _ => i += c.len_utf8(),
        }
    }
}

fn scalar(code: &str, range: Range<usize>, out: &mut Spans, constants: &[&str]) {
    let text = &code[range.clone()];
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let offset = range.start + (text.len() - text.trim_start().len());
    let range = offset..offset + trimmed.len();
    let first = trimmed.chars().next().unwrap_or(' ');
    if first == '"' || first == '\'' {
        out.push(range, Token::String);
    } else if constants.iter().any(|c| c.eq_ignore_ascii_case(trimmed)) {
        out.push(range, Token::Constant);
    } else if trimmed
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || c == '-' || c == '+')
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".:-+_".contains(c))
        && trimmed.chars().any(|c| c.is_ascii_digit())
    {
        out.push(range, Token::Number);
    } else if first == '&' || first == '*' {
        out.push(range, Token::Variable);
    } else if matches!(first, '[' | '{') {
        inline_collection(code, range, out, constants);
    }
}

fn inline_collection(code: &str, range: Range<usize>, out: &mut Spans, constants: &[&str]) {
    let end = range.end;
    let mut i = range.start;
    let mut word_start = i;
    let flush = |from: usize, to: usize, out: &mut Spans| {
        if from < to {
            scalar(code, from..to, out, constants);
        }
    };
    while i < end {
        let Some(c) = char_at(code, i) else { break };
        if c == '"' || c == '\'' {
            flush(word_start, i, out);
            let stop = code[i + 1..end].find(c).map_or(end, |o| i + o + 2);
            out.push(i..stop, Token::String);
            i = stop;
            word_start = i;
        } else if "[]{},:".contains(c) {
            flush(word_start, i, out);
            out.push(i..i + 1, Token::Punctuation);
            i += 1;
            word_start = i;
        } else {
            i += c.len_utf8();
        }
    }
    flush(word_start, end, out);
}

const YAML_CONSTANTS: &[&str] = &["true", "false", "yes", "no", "null", "~", "on", "off"];

pub(super) fn yaml(code: &str, out: &mut Spans) {
    let mut i = 0;
    let mut in_block_scalar: Option<usize> = None;
    while i < code.len() {
        let stop = line_end(code, i, code.len());
        let line = &code[i..stop];
        let indent = line.len() - line.trim_start().len();
        let content = &line[indent..];
        if let Some(parent) = in_block_scalar {
            if content.is_empty() || indent > parent {
                out.push(i..stop, Token::String);
                i = stop + 1;
                continue;
            }
            in_block_scalar = None;
        }
        if content.starts_with('#') {
            out.push(i + indent..stop, Token::Comment);
        } else if content == "---" || content == "..." || content.starts_with("--- ") {
            out.push(i + indent..i + indent + 3, Token::Punctuation);
        } else {
            let mut pos = i + indent;
            let mut body = content;
            while let Some(rest) = body.strip_prefix("- ") {
                out.push(pos..pos + 1, Token::Punctuation);
                let skip = 2 + (rest.len() - rest.trim_start().len());
                body = &body[skip..];
                pos = i + (line.len() - body.len());
            }
            if body == "-" {
                out.push(pos..pos + 1, Token::Punctuation);
            } else if let Some(colon) = key_end(body) {
                let key_start = pos;
                out.push(key_start..key_start + colon, Token::Property);
                out.push(key_start + colon..key_start + colon + 1, Token::Punctuation);
                let value_start = key_start + colon + 1;
                let (value, comment) = split_comment(code, value_start, stop);
                let value_text = code[value.clone()].trim();
                if matches!(value_text, "|" | ">" | "|-" | ">-" | "|+" | ">+") {
                    out.push(
                        value.start
                            + (code[value.clone()].len() - code[value.clone()].trim_start().len())
                            ..value.end,
                        Token::Operator,
                    );
                    in_block_scalar = Some(indent);
                } else {
                    scalar(code, value, out, YAML_CONSTANTS);
                }
                if let Some(c) = comment {
                    out.push(c, Token::Comment);
                }
            } else {
                let (value, comment) = split_comment(code, pos, stop);
                scalar(code, value, out, YAML_CONSTANTS);
                if let Some(c) = comment {
                    out.push(c, Token::Comment);
                }
            }
        }
        i = stop + 1;
    }
}

fn key_end(body: &str) -> Option<usize> {
    if let Some(quote) = body.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let close = body[1..].find(quote)? + 2;
        return body[close..].starts_with(':').then_some(close);
    }
    let mut prev = ' ';
    for (idx, c) in body.char_indices() {
        if c == ':' && (body[idx + 1..].is_empty() || body[idx + 1..].starts_with(' ')) && idx > 0 {
            return Some(idx);
        }
        if (c == '#' && prev == ' ') || c == '{' || c == '[' && idx == 0 {
            return None;
        }
        prev = c;
    }
    None
}

fn split_comment(code: &str, from: usize, to: usize) -> (Range<usize>, Option<Range<usize>>) {
    let text = &code[from..to];
    let mut quote: Option<char> = None;
    let mut prev = ' ';
    for (idx, c) in text.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' && prev.is_whitespace() => {
                return (from..from + idx, Some(from + idx..to));
            }
            None => {}
        }
        prev = c;
    }
    (from..to, None)
}

pub(super) fn toml(code: &str, out: &mut Spans) {
    let mut i = 0;
    while i < code.len() {
        let stop = line_end(code, i, code.len());
        let line = &code[i..stop];
        let indent = line.len() - line.trim_start().len();
        let content = line.trim();
        if content.starts_with('#') {
            out.push(i + indent..stop, Token::Comment);
        } else if content.starts_with('[') {
            out.push(i + indent..i + indent + content.len(), Token::Heading);
        } else if let Some(eq) = line.find('=') {
            let key = line[..eq].trim_end();
            out.push(i + indent..i + key.len(), Token::Property);
            out.push(i + eq..i + eq + 1, Token::Operator);
            let (value, comment) = split_comment(code, i + eq + 1, stop);
            scalar(code, value, out, &["true", "false"]);
            if let Some(c) = comment {
                out.push(c, Token::Comment);
            }
        }
        i = stop + 1;
    }
}

pub(super) fn ini(code: &str, out: &mut Spans) {
    let mut i = 0;
    while i < code.len() {
        let stop = line_end(code, i, code.len());
        let line = &code[i..stop];
        let indent = line.len() - line.trim_start().len();
        let content = line.trim();
        if content.starts_with([';', '#']) {
            out.push(i + indent..stop, Token::Comment);
        } else if content.starts_with('[') {
            out.push(i + indent..i + indent + content.len(), Token::Heading);
        } else if let Some(eq) = line.find(['=', ':']) {
            let key = line[..eq].trim_end();
            out.push(i + indent..i + key.len(), Token::Property);
            out.push(i + eq..i + eq + 1, Token::Operator);
            scalar(
                code,
                i + eq + 1..stop,
                out,
                &["true", "false", "yes", "no", "on", "off"],
            );
        }
        i = stop + 1;
    }
}

pub(super) fn markup(code: &str, range: Range<usize>, out: &mut Spans) {
    let end = range.end;
    let mut i = range.start;
    while i < end {
        let Some(open) = code[i..end].find('<').map(|o| i + o) else {
            break;
        };
        entities(code, i..open, out);
        let rest = &code[open..end];
        if rest.starts_with("<!--") {
            let stop = rest.find("-->").map_or(end, |o| open + o + 3);
            out.push(open..stop, Token::Comment);
            i = stop;
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            let stop = rest.find("]]>").map_or(end, |o| open + o + 3);
            out.push(open..stop, Token::String);
            i = stop;
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            let stop = rest.find('>').map_or(end, |o| open + o + 1);
            out.push(open..stop, Token::Keyword);
            i = stop;
            continue;
        }
        let name_start = open + 1 + usize::from(rest[1..].starts_with('/'));
        let name_len = code[name_start..end]
            .find(|c: char| !(is_ident_char(c) || ":-.".contains(c)))
            .unwrap_or(end - name_start);
        if name_len == 0 {
            i = open + 1;
            continue;
        }
        out.push(open..name_start, Token::Punctuation);
        out.push(name_start..name_start + name_len, Token::Tag);
        i = markup_attributes(code, name_start + name_len, end, out);
        let tag_name = code[name_start..name_start + name_len].to_lowercase();
        if !rest[1..].starts_with('/')
            && (tag_name == "script" || tag_name == "style")
            && !code[open..i].ends_with("/>")
        {
            let close = format!("</{tag_name}");
            let stop = code[i..end]
                .to_lowercase()
                .find(&close)
                .map_or(end, |o| i + o);
            if tag_name == "script" {
                generic::lex(
                    code,
                    i..stop,
                    generic::syntax("javascript").expect("javascript existe"),
                    out,
                );
            } else {
                css(code, i..stop, out);
            }
            i = stop;
        }
    }
    entities(code, i..end, out);
}

fn entities(code: &str, range: Range<usize>, out: &mut Spans) {
    let text = &code[range.clone()];
    let mut from = 0;
    while let Some(amp) = text[from..].find('&') {
        let at = from + amp;
        match text[at..].find(';') {
            Some(semi)
                if semi > 1
                    && semi < 10
                    && text[at + 1..at + semi]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '#') =>
            {
                out.push(
                    range.start + at..range.start + at + semi + 1,
                    Token::Constant,
                );
                from = at + semi + 1;
            }
            _ => from = at + 1,
        }
    }
}

fn markup_attributes(code: &str, from: usize, end: usize, out: &mut Spans) -> usize {
    let mut i = from;
    while i < end {
        let Some(c) = char_at(code, i) else { break };
        let rest = &code[i..end];
        if c.is_whitespace() {
            i += c.len_utf8();
        } else if rest.starts_with("/>") {
            out.push(i..i + 2, Token::Punctuation);
            return i + 2;
        } else if c == '>' {
            out.push(i..i + 1, Token::Punctuation);
            return i + 1;
        } else if c == '"' || c == '\'' {
            let stop = rest[1..].find(c).map_or(end, |o| i + o + 2);
            out.push(i..stop, Token::String);
            i = stop;
        } else if c == '=' {
            out.push(i..i + 1, Token::Operator);
            i += 1;
        } else if c == '<' {
            return i;
        } else {
            let len = rest
                .find(|ch: char| ch.is_whitespace() || "=>/\"'<".contains(ch))
                .unwrap_or(rest.len())
                .max(c.len_utf8());
            let after_equals = code[..i].trim_end().ends_with('=');
            out.push(
                i..i + len,
                if after_equals {
                    Token::String
                } else {
                    Token::Attribute
                },
            );
            i += len;
        }
    }
    end
}

pub(super) fn css(code: &str, range: Range<usize>, out: &mut Spans) {
    let end = range.end;
    let mut i = range.start;
    let mut depth = 0usize;
    while i < end {
        let Some(c) = char_at(code, i) else { break };
        let rest = &code[i..end];
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        if rest.starts_with("/*") {
            let stop = rest.find("*/").map_or(end, |o| i + o + 2);
            out.push(i..stop, Token::Comment);
            i = stop;
            continue;
        }
        if rest.starts_with("//") {
            let stop = line_end(code, i, end);
            out.push(i..stop, Token::Comment);
            i = stop;
            continue;
        }
        match c {
            '"' | '\'' => {
                let stop = rest[1..].find(c).map_or(end, |o| i + o + 2);
                out.push(i..stop, Token::String);
                i = stop;
            }
            '{' => {
                out.push(i..i + 1, Token::Punctuation);
                depth += 1;
                i += 1;
            }
            '}' => {
                out.push(i..i + 1, Token::Punctuation);
                depth = depth.saturating_sub(1);
                i += 1;
            }
            ';' | ',' | '(' | ')' | '>' | '+' | '~' | '*' | '=' => {
                out.push(
                    i..i + 1,
                    if "(),;".contains(c) {
                        Token::Punctuation
                    } else {
                        Token::Operator
                    },
                );
                i += 1;
            }
            '@' => {
                let len = 1 + rest[1..]
                    .find(|ch: char| !(is_ident_char(ch) || ch == '-'))
                    .unwrap_or(rest.len() - 1);
                out.push(i..i + len, Token::Keyword);
                i += len;
            }
            '!' => {
                let len = rest
                    .find(|ch: char| !(ch.is_alphabetic() || ch == '!'))
                    .unwrap_or(rest.len());
                out.push(i..i + len, Token::Keyword);
                i += len;
            }
            '#' | '.' | ':' | '[' | ']' if depth == 0 || c == '[' || c == ']' => {
                let len = c.len_utf8()
                    + rest[1..]
                        .find(|ch: char| !(is_ident_char(ch) || ch == '-'))
                        .unwrap_or(rest.len() - 1);
                out.push(
                    i..i + len,
                    if c == '[' || c == ']' {
                        Token::Punctuation
                    } else {
                        Token::Type
                    },
                );
                i += len;
            }
            '#' => {
                let len = 1 + rest[1..]
                    .find(|ch: char| !ch.is_ascii_hexdigit())
                    .unwrap_or(rest.len() - 1);
                out.push(i..i + len, Token::Number);
                i += len;
            }
            _ if c.is_ascii_digit()
                || (c == '-' && rest[1..].starts_with(|n: char| n.is_ascii_digit()))
                || (c == '.' && depth > 0) =>
            {
                let len = 1 + rest[1..]
                    .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '%'))
                    .unwrap_or(rest.len() - 1);
                out.push(i..i + len, Token::Number);
                i += len;
            }
            _ if is_ident_start(c) || c == '-' => {
                let len = rest
                    .find(|ch: char| !(is_ident_char(ch) || ch == '-'))
                    .unwrap_or(rest.len())
                    .max(c.len_utf8());
                let word = &rest[..len];
                let after = rest[len..].trim_start();
                let token = if word.starts_with("--") {
                    Some(Token::Variable)
                } else if depth == 0 {
                    Some(Token::Tag)
                } else if after.starts_with(':') && !after.starts_with("::") {
                    Some(Token::Property)
                } else if after.starts_with('(') {
                    Some(Token::Function)
                } else {
                    None
                };
                if let Some(t) = token {
                    out.push(i..i + len, t);
                }
                i += len;
            }
            _ => i += c.len_utf8(),
        }
    }
}

pub(super) fn diff(code: &str, out: &mut Spans) {
    let mut i = 0;
    while i < code.len() {
        let stop = line_end(code, i, code.len());
        let line = &code[i..stop];
        let token = if line.starts_with("+++")
            || line.starts_with("---")
            || line.starts_with("diff ")
            || line.starts_with("index ")
            || line.starts_with("@@")
        {
            Some(Token::Heading)
        } else if line.starts_with('+') || line.starts_with('>') {
            Some(Token::Inserted)
        } else if line.starts_with('-') || line.starts_with('<') {
            Some(Token::Deleted)
        } else {
            None
        };
        if let Some(t) = token {
            out.push(i..stop, t);
        }
        i = stop + 1;
    }
}

const HTTP_METHODS: &[&str] = &[
    "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "CONNECT", "TRACE",
];

pub(super) fn http(code: &str, out: &mut Spans) {
    let mut i = 0;
    let mut in_body = false;
    let mut first = true;
    while i < code.len() {
        let stop = line_end(code, i, code.len());
        let line = &code[i..stop];
        if in_body {
            let body = code[i..].trim_start();
            let at = code.len() - body.len();
            if body.starts_with(['{', '[']) {
                json(code, at..code.len(), out);
            } else if body.starts_with('<') {
                markup(code, at..code.len(), out);
            } else {
                form_body(code, at..code.len(), out);
            }
            return;
        }
        if first {
            first = false;
            let trimmed = line.trim_end_matches('\r');
            let mut parts = trimmed.splitn(3, ' ');
            let (a, b, c) = (parts.next().unwrap_or(""), parts.next(), parts.next());
            if HTTP_METHODS.contains(&a) {
                out.push(i..i + a.len(), Token::Keyword);
                if let Some(path) = b {
                    out.push(i + a.len() + 1..i + a.len() + 1 + path.len(), Token::String);
                }
                if let Some(version) = c {
                    let at = i + trimmed.len() - version.len();
                    out.push(at..at + version.len(), Token::Constant);
                }
            } else if a.starts_with("HTTP/") {
                out.push(i..i + a.len(), Token::Constant);
                if let Some(status) = b {
                    out.push(
                        i + a.len() + 1..i + a.len() + 1 + status.len(),
                        Token::Number,
                    );
                }
            }
        } else if line.trim().is_empty() {
            in_body = true;
        } else if let Some(colon) = line.find(':') {
            out.push(i..i + colon, Token::Property);
            out.push(i + colon..i + colon + 1, Token::Punctuation);
        }
        i = stop + 1;
    }
}

fn form_body(code: &str, range: Range<usize>, out: &mut Spans) {
    let mut i = range.start;
    for pair in code[range.clone()].split('&') {
        if let Some(eq) = pair.find('=') {
            out.push(i..i + eq, Token::Property);
            out.push(i + eq..i + eq + 1, Token::Operator);
        }
        i += pair.len() + 1;
    }
}

const ASM_REGISTERS: &[&str] = &[
    "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rsp", "rbp", "eax", "ebx", "ecx", "edx", "esi",
    "edi", "esp", "ebp", "ax", "bx", "cx", "dx", "al", "bl", "cl", "dl", "ah", "rip", "eip", "sp",
    "lr", "pc",
];
const ASM_MNEMONICS: &[&str] = &[
    "mov", "movzx", "movsx", "lea", "push", "pop", "call", "ret", "jmp", "je", "jne", "jz", "jnz",
    "ja", "jb", "jg", "jl", "jge", "jle", "cmp", "test", "add", "sub", "mul", "imul", "div",
    "idiv", "inc", "dec", "and", "or", "xor", "not", "neg", "shl", "shr", "sar", "rol", "ror",
    "nop", "int", "syscall", "leave", "cdq", "xchg", "loop", "rep", "movs", "stos", "lods", "ldr",
    "str", "ldp", "stp", "bl", "bx", "blx", "b", "beq", "bne", "cbz", "cbnz", "svc", "adr", "adrp",
    "movk", "orr", "eor",
];

pub(super) fn asm(code: &str, out: &mut Spans) {
    let mut i = 0;
    while i < code.len() {
        let stop = line_end(code, i, code.len());
        asm_line(code, i, stop, out);
        i = stop + 1;
    }
}

fn asm_line(code: &str, start: usize, stop: usize, out: &mut Spans) {
    let line = &code[start..stop];
    let comment = line
        .find([';', '#'])
        .or_else(|| line.find("//"))
        .filter(|&at| !in_quotes(line, at));
    let body_end = start + comment.unwrap_or(line.len());
    let mut i = start;
    let mut first_word = true;
    while i < body_end {
        let Some(c) = char_at(code, i) else { break };
        let rest = &code[i..body_end];
        if c.is_whitespace() || c == ',' {
            i += c.len_utf8();
            continue;
        }
        if c == '"' || c == '\'' {
            let close = rest[1..].find(c).map_or(rest.len(), |o| o + 2);
            out.push(i..i + close, Token::String);
            i += close;
            continue;
        }
        if "[]+-*:()".contains(c) {
            out.push(i..i + 1, Token::Punctuation);
            i += 1;
            continue;
        }
        let len = rest
            .find(|ch: char| ch.is_whitespace() || ",[]+-*:()\"'".contains(ch))
            .unwrap_or(rest.len())
            .max(c.len_utf8());
        let word = &rest[..len];
        let lowered = word.to_lowercase();
        let label = rest[len..].starts_with(':') && first_word;
        let token = if label {
            Some(Token::Function)
        } else if word.starts_with('.') && first_word {
            Some(Token::Attribute)
        } else if first_word && ASM_MNEMONICS.contains(&lowered.as_str()) {
            Some(Token::Keyword)
        } else if ASM_REGISTERS.contains(&lowered.as_str()) || is_numbered_register(&lowered) {
            Some(Token::Variable)
        } else if word.starts_with(['#', '$'])
            && word[1..].starts_with(|n: char| n.is_ascii_digit() || n == '-')
            || word.starts_with(|n: char| n.is_ascii_digit())
        {
            Some(Token::Number)
        } else if matches!(
            lowered.as_str(),
            "byte" | "word" | "dword" | "qword" | "ptr" | "offset"
        ) {
            Some(Token::Type)
        } else if first_word {
            Some(Token::Keyword)
        } else {
            None
        };
        if let Some(t) = token {
            out.push(i..i + len, t);
        }
        first_word = label && first_word;
        i += len;
    }
    if let Some(at) = comment {
        out.push(start + at..stop, Token::Comment);
    }
}

fn is_numbered_register(word: &str) -> bool {
    let prefix_len = word
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(word.len());
    let (prefix, digits) = word.split_at(prefix_len);
    !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
        && matches!(
            prefix,
            "r" | "x" | "w" | "xmm" | "ymm" | "q" | "d" | "s" | "v"
        )
        && digits.len() <= 2
}

fn in_quotes(line: &str, at: usize) -> bool {
    line[..at].chars().filter(|&c| c == '"').count() % 2 == 1
}
