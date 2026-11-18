use std::ops::Range;

use super::{Spans, Token, char_at, generic, is_ident_char, is_ident_start, line_end};

const BASH_KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac",
    "in", "function", "select", "time", "coproc", "!", "{", "}",
];
const COMMAND_WRAPPERS: [&str; 10] = [
    "sudo", "doas", "env", "nohup", "time", "exec", "xargs", "watch", "nice", "command",
];
const COMMAND_AFTER: &[&str] = &[
    "then", "else", "elif", "do", "while", "until", "if", "!", "{",
];

fn is_word_break(c: char) -> bool {
    c.is_whitespace() || "|&;<>()$\"'`".contains(c)
}

pub(super) fn bash(code: &str, range: Range<usize>, out: &mut Spans) {
    let end = range.end;
    let mut i = range.start;
    let mut command = true;
    let mut wrapper = false;
    let mut heredoc: Option<(String, bool)> = None;
    while i < end {
        let Some(c) = char_at(code, i) else { break };
        let rest = &code[i..end];
        if c == '\n' {
            i += 1;
            if let Some((word, strip)) = heredoc.take() {
                i = heredoc_body(code, i, end, &word, strip, out);
            }
            command = true;
            wrapper = false;
            continue;
        }
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        if c == '\\' && rest[1..].starts_with('\n') {
            i += 2;
            continue;
        }
        if c == '#'
            && (i == range.start
                || code[..i].ends_with(char::is_whitespace)
                || code[..i].ends_with([';', '(', '{']))
        {
            let stop = line_end(code, i, end);
            out.push(i..stop, Token::Comment);
            i = stop;
            continue;
        }
        match c {
            '\'' => {
                let stop = rest[1..].find('\'').map_or(end, |o| i + o + 2);
                out.push(i..stop, Token::String);
                i = stop;
                command = false;
                continue;
            }
            '"' => {
                i = bash_double_quoted(code, i, end, out);
                command = false;
                continue;
            }
            '$' => {
                i = bash_dollar(code, i, end, out);
                command = false;
                continue;
            }
            '`' => {
                let stop = rest[1..].find('`').map_or(end, |o| i + 1 + o);
                out.push(i..i + 1, Token::Punctuation);
                bash(code, i + 1..stop, out);
                if stop < end {
                    out.push(stop..stop + 1, Token::Punctuation);
                }
                i = (stop + 1).min(end);
                command = false;
                continue;
            }
            '<' if rest.starts_with("<<") && !rest.starts_with("<<<") => {
                let mut j = i + 2;
                let strip = code[j..end].starts_with('-');
                if strip {
                    j += 1;
                }
                let quoted = code[j..end].starts_with(['\'', '"']);
                let word_start = j + usize::from(quoted);
                let word_len = code[word_start..end]
                    .find(|ch: char| !(is_ident_char(ch) || ch == '-' || ch == '.'))
                    .unwrap_or(end - word_start);
                if word_len > 0 {
                    out.push(i..j, Token::Operator);
                    out.push(word_start..word_start + word_len, Token::Constant);
                    heredoc = Some((code[word_start..word_start + word_len].to_string(), strip));
                    i = word_start + word_len + usize::from(quoted);
                    continue;
                }
            }
            _ => {}
        }
        if let Some(len) = operator_len(rest) {
            out.push(
                i..i + len,
                if rest.starts_with(['(', ')']) {
                    Token::Punctuation
                } else {
                    Token::Operator
                },
            );
            let op = &rest[..len];
            if matches!(op, "|" | "||" | "&&" | "&" | ";" | ";;" | "(" | "|&") {
                command = true;
                wrapper = false;
            }
            i += len;
            continue;
        }
        let len = rest.find(is_word_break).unwrap_or(rest.len());
        let word = &rest[..len];
        i = bash_word(code, i, i + len, word, &mut command, &mut wrapper, out);
    }
}

fn operator_len(rest: &str) -> Option<usize> {
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let tail = &rest[digits..];
    for op in [
        "&>>", "&>", ">>", ">&", ">|", "<&", "<<<", "&&", "||", "|&", ";;", ">", "<", "|", "&",
        ";", "(", ")",
    ] {
        if tail.starts_with(op) {
            let digits_ok = digits > 0 && matches!(op, ">>" | ">&" | ">|" | "<&" | ">" | "<");
            return Some(if digits_ok {
                digits + op.len()
            } else if digits == 0 {
                op.len()
            } else {
                return None;
            });
        }
    }
    None
}

fn bash_word(
    code: &str,
    start: usize,
    stop: usize,
    word: &str,
    command: &mut bool,
    wrapper: &mut bool,
    out: &mut Spans,
) -> usize {
    if word.is_empty() {
        return start + char_at(code, start).map_or(1, char::len_utf8);
    }
    if *command
        && let Some((name, _)) = word.split_once('=')
        && !name.is_empty()
        && name.chars().all(is_ident_char)
        && !name.starts_with(|c: char| c.is_ascii_digit())
    {
        out.push(start..start + name.len(), Token::Variable);
        out.push(start + name.len()..start + name.len() + 1, Token::Operator);
        return start + name.len() + 1;
    }
    if matches!(word, "[" | "[[" | "]" | "]]") {
        out.push(start..stop, Token::Punctuation);
        *command = false;
        return stop;
    }
    if *command && code[stop..].starts_with("()") {
        out.push(start..stop, Token::Function);
        return stop;
    }
    if BASH_KEYWORDS.contains(&word) {
        out.push(start..stop, Token::Keyword);
        *command = COMMAND_AFTER.contains(&word);
        return stop;
    }
    if *command {
        out.push(start..stop, Token::Function);
        if COMMAND_WRAPPERS.contains(&word) {
            *wrapper = true;
        } else {
            *command = false;
            *wrapper = false;
        }
        return stop;
    }
    if word.starts_with('-')
        && word.len() > 1
        && !word[1..].chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        let flag_len = word.find('=').unwrap_or(word.len());
        out.push(start..start + flag_len, Token::Flag);
        if *wrapper {
            return start + flag_len;
        }
        return start + flag_len;
    }
    if *wrapper {
        out.push(start..stop, Token::Function);
        *wrapper = false;
        return stop;
    }
    if word.chars().all(|c| c.is_ascii_digit())
        || word
            .strip_prefix('-')
            .is_some_and(|w| w.chars().all(|c| c.is_ascii_digit()))
    {
        out.push(start..stop, Token::Number);
    }
    stop
}

fn heredoc_body(
    code: &str,
    from: usize,
    end: usize,
    word: &str,
    strip: bool,
    out: &mut Spans,
) -> usize {
    let mut i = from;
    while i < end {
        let stop = line_end(code, i, end);
        let line = &code[i..stop];
        let candidate = if strip {
            line.trim_start_matches('\t')
        } else {
            line
        };
        if candidate == word {
            out.push(from..i, Token::String);
            out.push(i..stop, Token::Constant);
            return stop;
        }
        i = (stop + 1).min(end);
        if stop == end {
            break;
        }
    }
    out.push(from..end, Token::String);
    end
}

fn bash_double_quoted(code: &str, start: usize, end: usize, out: &mut Spans) -> usize {
    let mut i = start + 1;
    let mut seg = start;
    while i < end {
        let Some(c) = char_at(code, i) else { break };
        match c {
            '\\' => i += c.len_utf8() + char_at(code, i + 1).map_or(0, char::len_utf8),
            '"' => {
                out.push(seg..i + 1, Token::String);
                return i + 1;
            }
            '$' => {
                out.push(seg..i, Token::String);
                i = bash_dollar(code, i, end, out);
                seg = i;
            }
            _ => i += c.len_utf8(),
        }
    }
    out.push(seg..end, Token::String);
    end
}

fn bash_dollar(code: &str, i: usize, end: usize, out: &mut Spans) -> usize {
    let rest = &code[i..end];
    let Some(next) = rest[1..].chars().next() else {
        out.push(i..i + 1, Token::Variable);
        return i + 1;
    };
    if rest.starts_with("$((") {
        let stop = matching_paren(code, i + 2, end).unwrap_or(end);
        out.push(i..stop.min(end), Token::Variable);
        return stop.min(end);
    }
    if next == '(' {
        let close = matching_paren(code, i + 1, end);
        out.push(i..i + 2, Token::Punctuation);
        let inner_end = close.map_or(end, |c| c - 1);
        bash(code, i + 2..inner_end, out);
        if close.is_some() {
            out.push(inner_end..inner_end + 1, Token::Punctuation);
        }
        return close.unwrap_or(end);
    }
    if next == '{' {
        let stop = rest.find('}').map_or(end, |o| i + o + 1);
        out.push(i..stop, Token::Variable);
        return stop;
    }
    if is_ident_start(next) {
        let len = 1 + rest[1..]
            .find(|ch: char| !is_ident_char(ch))
            .unwrap_or(rest.len() - 1);
        out.push(i..i + len, Token::Variable);
        return i + len;
    }
    if next.is_ascii_digit() || "?$!@*#-".contains(next) {
        out.push(i..i + 2, Token::Variable);
        return i + 2;
    }
    i + 1
}

fn matching_paren(code: &str, open: usize, end: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut i = open;
    while i < end {
        let c = char_at(code, i)?;
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some('"'), '\\') => i += 1,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += c.len_utf8();
    }
    None
}

const POWERSHELL_KEYWORDS: &[&str] = &[
    "if",
    "else",
    "elseif",
    "foreach",
    "for",
    "while",
    "do",
    "until",
    "switch",
    "function",
    "filter",
    "param",
    "return",
    "try",
    "catch",
    "finally",
    "throw",
    "in",
    "begin",
    "process",
    "end",
    "class",
    "using",
    "break",
    "continue",
    "exit",
    "trap",
    "data",
    "dynamicparam",
    "enum",
    "hidden",
    "static",
    "workflow",
    "parallel",
];
const POWERSHELL_OPERATORS: &[&str] = &[
    "eq",
    "ne",
    "gt",
    "ge",
    "lt",
    "le",
    "like",
    "notlike",
    "match",
    "notmatch",
    "contains",
    "notcontains",
    "in",
    "notin",
    "is",
    "isnot",
    "as",
    "and",
    "or",
    "not",
    "xor",
    "band",
    "bor",
    "bxor",
    "bnot",
    "shl",
    "shr",
    "replace",
    "split",
    "join",
    "f",
    "ceq",
    "cne",
    "cgt",
    "cge",
    "clt",
    "cle",
    "ieq",
    "ine",
    "igt",
    "ilt",
];

pub(super) fn powershell(code: &str, range: Range<usize>, out: &mut Spans) {
    let end = range.end;
    let mut i = range.start;
    let mut command = true;
    while i < end {
        let Some(c) = char_at(code, i) else { break };
        let rest = &code[i..end];
        if c == '\n' {
            command = true;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        if c == '`' {
            i += 1 + char_at(code, i + 1).map_or(0, char::len_utf8);
            continue;
        }
        if rest.starts_with("<#") {
            let stop = rest.find("#>").map_or(end, |o| i + o + 2);
            out.push(i..stop, Token::Comment);
            i = stop;
            continue;
        }
        if c == '#' {
            let stop = line_end(code, i, end);
            out.push(i..stop, Token::Comment);
            i = stop;
            continue;
        }
        if rest.starts_with("@\"") || rest.starts_with("@'") {
            let close = if rest.starts_with("@\"") { "\"@" } else { "'@" };
            let stop = rest[2..].find(close).map_or(end, |o| i + 2 + o + 2);
            out.push(i..stop, Token::String);
            i = stop;
            command = false;
            continue;
        }
        if c == '\'' {
            let mut j = i + 1;
            while j < end {
                if code[j..end].starts_with("''") {
                    j += 2;
                } else if code[j..end].starts_with('\'') {
                    j += 1;
                    break;
                } else {
                    j += char_at(code, j).map_or(1, char::len_utf8);
                }
            }
            out.push(i..j, Token::String);
            i = j;
            command = false;
            continue;
        }
        if c == '"' {
            i = powershell_double_quoted(code, i, end, out);
            command = false;
            continue;
        }
        if c == '$' {
            i = powershell_variable(code, i, end, out);
            command = false;
            continue;
        }
        if c == '@' && rest[1..].starts_with(|n: char| is_ident_start(n)) {
            let len = 1 + rest[1..]
                .find(|ch: char| !is_ident_char(ch))
                .unwrap_or(rest.len() - 1);
            out.push(i..i + len, Token::Variable);
            i += len;
            command = false;
            continue;
        }
        if c == '['
            && let Some(close) = rest.find(']')
        {
            let inner = &rest[1..close];
            if inner.starts_with(|n: char| n.is_alphabetic())
                && inner
                    .chars()
                    .all(|n| is_ident_char(n) || ".,[] `".contains(n))
            {
                out.push(i..i + 1, Token::Punctuation);
                out.push(i + 1..i + close, Token::Type);
                out.push(i + close..i + close + 1, Token::Punctuation);
                i += close + 1;
                continue;
            }
        }
        if c == '-'
            && rest[1..].starts_with(|n: char| n.is_alphabetic())
            && (i == range.start || code[..i].ends_with(|p: char| p.is_whitespace() || p == '('))
        {
            let len = 1 + rest[1..]
                .find(|ch: char| !is_ident_char(ch))
                .unwrap_or(rest.len() - 1);
            let name = rest[1..len].to_lowercase();
            let token = if POWERSHELL_OPERATORS.contains(&name.as_str()) {
                Token::Operator
            } else {
                Token::Flag
            };
            out.push(i..i + len, token);
            i += len;
            continue;
        }
        if c.is_ascii_digit() {
            let len = rest
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.'))
                .unwrap_or(rest.len());
            out.push(i..i + len, Token::Number);
            i += len;
            command = false;
            continue;
        }
        if is_ident_start(c) {
            let len = rest
                .find(|ch: char| !(is_ident_char(ch) || ch == '-'))
                .unwrap_or(rest.len());
            let mut word_end = i + len;
            while code[i..word_end].ends_with('-') {
                word_end -= 1;
            }
            let word = &code[i..word_end];
            let lowered = word.to_lowercase();
            let previous = code[..i].chars().next_back();
            let after = code[word_end..end].trim_start_matches([' ', '\t']);
            let token = if previous == Some('.') || code[..i].ends_with("::") {
                if after.starts_with('(') {
                    Some(Token::Function)
                } else {
                    Some(Token::Property)
                }
            } else if POWERSHELL_KEYWORDS.contains(&lowered.as_str()) {
                Some(Token::Keyword)
            } else if (word.contains('-')
                && word.split('-').count() == 2
                && word.split('-').all(|p| p.starts_with(char::is_alphabetic)))
                || (command && (after.is_empty() || !after.starts_with(['=', '.', '(', '['])))
                || after.starts_with('(')
            {
                Some(Token::Function)
            } else {
                None
            };
            if let Some(t) = token {
                out.push(i..word_end, t);
            }
            command = false;
            i = word_end;
            continue;
        }
        if !(c == '\\' || (c == ':' && !rest.starts_with("::"))) {
            let token = if "{}();,".contains(c) {
                Token::Punctuation
            } else {
                Token::Operator
            };
            out.push(i..i + c.len_utf8(), token);
        }
        command = matches!(c, '|' | ';' | '{' | '(');
        i += c.len_utf8();
    }
}

fn powershell_variable(code: &str, i: usize, end: usize, out: &mut Spans) -> usize {
    let rest = &code[i..end];
    if rest.starts_with("$(") {
        let close = matching_paren(code, i + 1, end);
        out.push(i..i + 2, Token::Punctuation);
        let inner_end = close.map_or(end, |c| c - 1);
        powershell(code, i + 2..inner_end, out);
        if close.is_some() {
            out.push(inner_end..inner_end + 1, Token::Punctuation);
        }
        return close.unwrap_or(end);
    }
    if rest.starts_with("${") {
        let stop = rest.find('}').map_or(end, |o| i + o + 1);
        out.push(i..stop, Token::Variable);
        return stop;
    }
    let len = 1 + rest[1..]
        .find(|ch: char| !(is_ident_char(ch) || ch == ':' || ch == '?' || ch == '^'))
        .unwrap_or(rest.len() - 1);
    if len == 1 {
        return i + 1;
    }
    let name = rest[1..len].to_lowercase();
    let token = if matches!(name.as_str(), "true" | "false" | "null") {
        Token::Constant
    } else {
        Token::Variable
    };
    out.push(i..i + len, token);
    i + len
}

fn powershell_double_quoted(code: &str, start: usize, end: usize, out: &mut Spans) -> usize {
    let mut i = start + 1;
    let mut seg = start;
    while i < end {
        let Some(c) = char_at(code, i) else { break };
        match c {
            '`' => i += 1 + char_at(code, i + 1).map_or(0, char::len_utf8),
            '"' if code[i..end].starts_with("\"\"") => i += 2,
            '"' => {
                out.push(seg..i + 1, Token::String);
                return i + 1;
            }
            '$' if code[i + 1..end]
                .starts_with(|n: char| is_ident_start(n) || n == '(' || n == '{') =>
            {
                out.push(seg..i, Token::String);
                i = powershell_variable(code, i, end, out);
                seg = i;
            }
            _ => i += c.len_utf8(),
        }
    }
    out.push(seg..end, Token::String);
    end
}

const CMD_BUILTINS: &[&str] = &[
    "echo", "set", "if", "for", "goto", "call", "exit", "setlocal", "endlocal", "shift", "pause",
    "cd", "dir", "copy", "del", "move", "ren", "md", "rd", "mkdir", "rmdir", "type", "cls",
    "title", "start", "else", "rem", "pushd", "popd", "color",
];
const CMD_CLAUSES: &[&str] = &[
    "not",
    "exist",
    "defined",
    "errorlevel",
    "in",
    "do",
    "equ",
    "neq",
    "lss",
    "leq",
    "gtr",
    "geq",
    "enabledelayedexpansion",
    "off",
];

pub(super) fn cmd(code: &str, range: Range<usize>, out: &mut Spans) {
    let end = range.end;
    let mut i = range.start;
    let mut command = true;
    while i < end {
        let Some(c) = char_at(code, i) else { break };
        let rest = &code[i..end];
        if c == '\n' {
            command = true;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        let starts_line = code[..i]
            .rsplit('\n')
            .next()
            .is_some_and(|b| b.trim().is_empty() || b.trim() == "@");
        if starts_line && (rest.starts_with("::") || word_is(rest, "rem")) {
            let stop = line_end(code, i, end);
            out.push(i..stop, Token::Comment);
            i = stop;
            continue;
        }
        if starts_line && c == ':' && rest[1..].starts_with(|n: char| is_ident_char(n)) {
            let stop = line_end(code, i, end);
            let label_end = rest
                .find(char::is_whitespace)
                .map_or(stop, |o| (i + o).min(stop));
            out.push(i..label_end, Token::Constant);
            i = label_end;
            continue;
        }
        match c {
            '"' => {
                let stop = rest[1..].find(['"', '\n']).map_or(end, |o| {
                    if rest[1 + o..].starts_with('"') {
                        i + o + 2
                    } else {
                        i + 1 + o
                    }
                });
                out.push(i..stop, Token::String);
                i = stop;
                command = false;
                continue;
            }
            '%' | '!' => {
                if let Some(len) = cmd_variable_len(rest) {
                    out.push(i..i + len, Token::Variable);
                    i += len;
                    command = false;
                    continue;
                }
            }
            _ => {}
        }
        if let Some(len) = ["&&", "||", ">>", "&", "|", ">", "<", "(", ")", "^", "=="]
            .iter()
            .find(|op| rest.starts_with(**op))
            .map(|op| op.len())
        {
            let token = if matches!(&rest[..len], "(" | ")") {
                Token::Punctuation
            } else {
                Token::Operator
            };
            out.push(i..i + len, token);
            if matches!(&rest[..len], "&&" | "||" | "&" | "|" | "(") {
                command = true;
            }
            i += len;
            continue;
        }
        if c == '@' {
            out.push(i..i + 1, Token::Operator);
            i += 1;
            continue;
        }
        let len = rest
            .find(|ch: char| ch.is_whitespace() || "&|<>()\"^%!".contains(ch))
            .unwrap_or(rest.len())
            .max(c.len_utf8());
        let word = &rest[..len];
        let lowered = word.to_lowercase();
        if (word.starts_with('/') || word.starts_with('-'))
            && word.len() > 1
            && word[1..].starts_with(|n: char| n.is_alphabetic() || n == '-')
            && !word[1..].contains('\\')
            && word.matches('/').count() <= 1
        {
            let flag_len = word.find([':', '=']).unwrap_or(word.len());
            out.push(i..i + flag_len, Token::Flag);
            i += flag_len;
            continue;
        }
        if CMD_CLAUSES.contains(&lowered.as_str())
            || (command && CMD_BUILTINS.contains(&lowered.as_str()))
        {
            out.push(i..i + len, Token::Keyword);
            command = matches!(lowered.as_str(), "else" | "do");
        } else if command {
            out.push(i..i + len, Token::Function);
            command = false;
        } else if word.chars().all(|ch| ch.is_ascii_digit()) {
            out.push(i..i + len, Token::Number);
        }
        i += len;
    }
}

fn word_is(rest: &str, word: &str) -> bool {
    rest.get(..word.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(word))
        && rest[word.len()..]
            .chars()
            .next()
            .is_none_or(|n| n.is_whitespace())
}

fn cmd_variable_len(rest: &str) -> Option<usize> {
    let mark = rest.chars().next()?;
    let tail = &rest[1..];
    if mark == '%' {
        if let Some(after_percent) = tail.strip_prefix('%') {
            let len =
                after_percent.find(|c: char| !(c.is_alphanumeric() || c == '~' || c == '_'))?;
            return (len > 0).then_some(2 + len);
        }
        if tail.starts_with(|c: char| c.is_ascii_digit() || c == '*') {
            return Some(2);
        }
        if tail.starts_with('~') {
            let len = tail.find(|c: char| !(c.is_alphanumeric() || c == '~'))?;
            return Some(1 + len);
        }
        let close = tail.find(|c: char| c == '%' || c.is_whitespace())?;
        return (tail[close..].starts_with('%') && close > 0).then_some(close + 2);
    }
    let close = tail.find(|c: char| c == '!' || c.is_whitespace())?;
    (tail[close..].starts_with('!') && close > 0).then_some(close + 2)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PromptKind {
    Shell,
    PowerShell,
    Cmd,
    Python,
    Tool,
}

pub(super) fn prompt(line: &str) -> Option<(usize, PromptKind)> {
    if line
        .strip_prefix(">>> ")
        .is_some_and(|rest| !rest.is_empty())
    {
        return Some((3, PromptKind::Python));
    }
    if line.starts_with("$ ") || line.starts_with("# ") || line.starts_with("% ") {
        return Some((1, PromptKind::Shell));
    }
    let head: String = line.chars().take(120).collect();
    if let Some(pos) = powershell_prompt(&head) {
        return Some((pos, PromptKind::PowerShell));
    }
    if let Some(pos) = cmd_prompt(&head) {
        return Some((pos, PromptKind::Cmd));
    }
    if let Some(pos) = shell_user_prompt(&head) {
        return Some((pos, PromptKind::Shell));
    }
    tool_prompt(&head).map(|pos| (pos, PromptKind::Tool))
}

fn powershell_prompt(head: &str) -> Option<usize> {
    let at = head.find("PS ")?;
    if at > 0 && !head[..at].ends_with("* ") {
        return None;
    }
    let after = &head[at + 3..];
    let close = after
        .find("> ")
        .or_else(|| after.ends_with('>').then(|| after.len() - 1))?;
    let path = &after[..close];
    (path.contains(['\\', '/', ':']) && !path.contains('\n')).then_some(at + 3 + close + 1)
}

fn cmd_prompt(head: &str) -> Option<usize> {
    let bytes = head.as_bytes();
    if bytes.len() < 4 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return None;
    }
    let close = head.find('>')?;
    let path = &head[3..close];
    (!path.contains(['<', '|', '"', '*', '?', ' ']) || path.contains(':')).then_some(close + 1)
}

fn shell_user_prompt(head: &str) -> Option<usize> {
    let (body, from) = if let Some(rest) = head.strip_prefix('└') {
        let at = rest.find(['$', '#'])?;
        (&rest[..at], '└'.len_utf8() + at)
    } else {
        let at = head.find("@")?;
        let after = &head[at..];
        let mark = after.find(['$', '#'])?;
        (&head[..at + mark], at + mark)
    };
    let valid_body = body
        .chars()
        .all(|c| is_ident_char(c) || ".-@:~/ []()\\㉿─╼╾".contains(c));
    let ends_ok = head[from + 1..].starts_with(' ') || head[from + 1..].is_empty();
    (valid_body && ends_ok && body.chars().count() < 80).then_some(from + 1)
}

fn tool_prompt(head: &str) -> Option<usize> {
    const TOOLS: &[&str] = &[
        "mimikatz",
        "meterpreter",
        "msf",
        "beacon",
        "sliver",
        "evil-winrm",
        "smb:",
        "ftp>",
        "mysql>",
        "sqlite>",
        "psql",
        "irb",
        "gdb",
        "(gdb)",
        "pwndbg>",
        "sqlmap",
        "Cmd>",
        "sh-",
    ];
    if !TOOLS.iter().any(|t| head.starts_with(t)) {
        return None;
    }
    let at = head.find(['>', '#'])?;
    let body = &head[..at];
    let ok_after = head[at + 1..].is_empty() || head[at + 1..].starts_with(' ');
    let valid_body = body
        .chars()
        .all(|c| is_ident_char(c) || " -.:()/\\~*[]=,".contains(c));
    (ok_after && valid_body && at < 90).then_some(at + 1)
}

pub(super) fn console(code: &str, out: &mut Spans) {
    let mut i = 0;
    while i < code.len() {
        let stop = line_end(code, i, code.len());
        let line = &code[i..stop];
        if line.starts_with('┌') {
            out.push(i..stop, Token::Prompt);
        } else if let Some((len, kind)) = prompt(line) {
            out.push(i..i + len, Token::Prompt);
            let command = i + len;
            match kind {
                PromptKind::Shell => bash(code, command..stop, out),
                PromptKind::PowerShell => powershell(code, command..stop, out),
                PromptKind::Cmd => cmd(code, command..stop, out),
                PromptKind::Python => generic::lex(
                    code,
                    command..stop,
                    generic::syntax("python").expect("python existe"),
                    out,
                ),
                PromptKind::Tool => tool_command(code, command..stop, out),
            }
        }
        i = (stop + 1).min(code.len());
        if stop == code.len() {
            break;
        }
    }
}

fn tool_command(code: &str, range: Range<usize>, out: &mut Spans) {
    let end = range.end;
    let mut i = range.start;
    let mut first = true;
    while i < end {
        let Some(c) = char_at(code, i) else { break };
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        let rest = &code[i..end];
        let len = if c == '"' || c == '\'' {
            1 + rest[1..].find(c).map_or(rest.len() - 1, |o| o + 1)
        } else {
            rest.find(char::is_whitespace).unwrap_or(rest.len())
        };
        let word = &rest[..len];
        let token = if first {
            Some(Token::Function)
        } else if c == '"' || c == '\'' {
            Some(Token::String)
        } else if word.starts_with('-') && word.len() > 1 {
            Some(Token::Flag)
        } else if word.chars().all(|ch| ch.is_ascii_digit()) {
            Some(Token::Number)
        } else {
            None
        };
        if let Some(t) = token {
            let flag_len = if t == Token::Flag {
                word.find('=').unwrap_or(len)
            } else {
                len
            };
            out.push(i..i + flag_len, t);
        }
        first = false;
        i += len;
    }
}

const DOCKER_INSTRUCTIONS: &[&str] = &[
    "from",
    "run",
    "cmd",
    "label",
    "maintainer",
    "expose",
    "env",
    "add",
    "copy",
    "entrypoint",
    "volume",
    "user",
    "workdir",
    "arg",
    "onbuild",
    "stopsignal",
    "healthcheck",
    "shell",
];

pub(super) fn dockerfile(code: &str, out: &mut Spans) {
    let mut i = 0;
    while i < code.len() {
        let mut stop = line_end(code, i, code.len());
        let line = &code[i..stop];
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if trimmed.starts_with('#') {
            out.push(i + indent..stop, Token::Comment);
        } else if let Some(word_len) = trimmed
            .find(char::is_whitespace)
            .or(Some(trimmed.len()))
            .filter(|&n| n > 0)
        {
            let word = &trimmed[..word_len];
            if DOCKER_INSTRUCTIONS.contains(&word.to_lowercase().as_str()) {
                while stop < code.len() && code[i..stop].trim_end().ends_with('\\') {
                    stop = line_end(code, stop + 1, code.len());
                }
                let begin = i + indent;
                out.push(begin..begin + word_len, Token::Keyword);
                let args = begin + word_len..stop;
                if matches!(word.to_lowercase().as_str(), "run" | "cmd" | "entrypoint")
                    && !code[args.clone()].trim_start().starts_with('[')
                {
                    bash(code, args, out);
                } else {
                    docker_arguments(code, args, out);
                }
            }
        }
        i = (stop + 1).min(code.len());
        if stop == code.len() {
            break;
        }
    }
}

fn docker_arguments(code: &str, range: Range<usize>, out: &mut Spans) {
    let end = range.end;
    let mut i = range.start;
    while i < end {
        let Some(c) = char_at(code, i) else { break };
        let rest = &code[i..end];
        if c.is_whitespace() || c == '\\' {
            i += c.len_utf8();
            continue;
        }
        if c == '"' || c == '\'' {
            let stop = rest[1..].find(c).map_or(end, |o| i + o + 2);
            out.push(i..stop, Token::String);
            i = stop;
            continue;
        }
        if c == '$' {
            i = bash_dollar(code, i, end, out);
            continue;
        }
        if c == '[' || c == ']' || c == ',' {
            out.push(i..i + 1, Token::Punctuation);
            i += 1;
            continue;
        }
        let len = rest
            .find(|ch: char| ch.is_whitespace() || "\"'$[],".contains(ch))
            .unwrap_or(rest.len())
            .max(c.len_utf8());
        let word = &rest[..len];
        if word.starts_with("--") && word.len() > 2 {
            let flag_len = word.find('=').unwrap_or(len);
            out.push(i..i + flag_len, Token::Flag);
        } else if word.eq_ignore_ascii_case("as") {
            out.push(i..i + len, Token::Keyword);
        } else if word.chars().all(|ch| ch.is_ascii_digit() || ch == '/')
            && word.chars().any(|ch| ch.is_ascii_digit())
        {
            out.push(i..i + len, Token::Number);
        }
        i += len;
    }
}
