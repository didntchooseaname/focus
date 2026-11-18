use super::{commands, shell};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detected {
    pub lang: &'static str,
    pub confidence: f32,
}

const THRESHOLD: f32 = 0.55;

#[derive(Debug, Clone, Copy, Default)]
pub struct Hints<'a> {
    pub languages: &'a [&'a str],
    pub path: Option<&'a str>,
}

pub fn detect(code: &str) -> Option<Detected> {
    detect_in_note(code, &Hints::default())
}

pub fn detect_in_note(code: &str, hints: &Hints) -> Option<Detected> {
    let trimmed = code.trim();
    if trimmed.len() < 3 {
        return None;
    }
    let ctx = Ctx::new(code);
    if let Some(found) = structured(&ctx) {
        return Some(found);
    }
    let mut scores: Vec<(&'static str, f32)> = vec![
        ("bash", bash(&ctx)),
        ("powershell", powershell(&ctx)),
        ("cmd", cmd(&ctx)),
        ("python", python(&ctx)),
        ("c", c(&ctx)),
        ("cpp", cpp(&ctx)),
        ("csharp", csharp(&ctx)),
        ("java", java(&ctx)),
        ("javascript", javascript(&ctx)),
        ("typescript", typescript(&ctx)),
        ("go", go(&ctx)),
        ("rust", rust(&ctx)),
        ("php", php(&ctx)),
        ("ruby", ruby(&ctx)),
        ("perl", perl(&ctx)),
        ("lua", lua(&ctx)),
        ("sql", sql(&ctx)),
        ("asm", asm(&ctx)),
        ("yaml", yaml(&ctx)),
        ("css", css(&ctx)),
    ];
    let share = language_shares(hints);
    let dominant = share
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|&(lang, _)| lang);
    for (lang, score) in &mut scores {
        if *score <= 0.0 {
            continue;
        }
        if let Some(&(_, fraction)) = share.iter().find(|(l, _)| l == lang) {
            *score += CONTEXT_WEIGHT * fraction;
        }
        *score += path_bonus(lang, hints.path);
    }
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (lang, best) = scores[0];
    let second = scores[1].1;
    let confidence = confidence(best, second);
    if confidence >= THRESHOLD {
        return Some(Detected { lang, confidence });
    }
    (dominant == Some(lang) && confidence >= THRESHOLD - 0.2).then_some(Detected {
        lang,
        confidence: THRESHOLD,
    })
}

const CONTEXT_WEIGHT: f32 = 3.5;

fn language_shares(hints: &Hints) -> Vec<(&'static str, f32)> {
    let mut counts: Vec<(&'static str, usize)> = Vec::new();
    for lang in hints.languages {
        let Some(id) = super::canonical(lang).filter(|id| *id != "text") else {
            continue;
        };
        match counts.iter_mut().find(|(l, _)| *l == id) {
            Some(entry) => entry.1 += 1,
            None => counts.push((id, 1)),
        }
    }
    let total: usize = counts.iter().map(|(_, n)| n).sum();
    counts
        .into_iter()
        .map(|(l, n)| (l, n as f32 / total as f32))
        .collect()
}

fn path_bonus(lang: &str, path: Option<&str>) -> f32 {
    let Some(path) = path else { return 0.0 };
    let lower = path.to_lowercase();
    let hit = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    match lang {
        "powershell" if hit(&["powershell", "windows", "active directory", "/ad/", "_ad/"]) => 0.8,
        "cmd" if hit(&["windows", "cmd"]) => 0.5,
        "bash" if hit(&["linux", "unix", "bash", "shell"]) => 0.8,
        "python" if hit(&["python"]) => 0.8,
        "php" if hit(&["php"]) => 0.8,
        "javascript" if hit(&["javascript", "/js/", "xss"]) => 0.8,
        "sql" if hit(&["sql"]) => 0.8,
        "c" | "cpp" if hit(&["buffer", "exploit_dev", "binary"]) => 0.3,
        _ => 0.0,
    }
}

fn confidence(best: f32, second: f32) -> f32 {
    if best <= 0.0 {
        return 0.0;
    }
    let strength = 1.0 - (-best / 2.2).exp();
    let margin = 1.0 - 0.75 * (second / best).clamp(0.0, 1.0);
    (strength * margin).min(0.99)
}

struct Ctx<'a> {
    code: &'a str,
    body: Vec<&'a str>,
    lower: String,
}

impl<'a> Ctx<'a> {
    fn new(code: &'a str) -> Self {
        let lines: Vec<&str> = code.lines().collect();
        let body = lines
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        Ctx {
            code,
            body,
            lower: code.to_lowercase(),
        }
    }

    fn has(&self, needle: &str) -> bool {
        self.code.contains(needle)
    }

    fn starts(&self, prefix: &str) -> usize {
        self.body.iter().filter(|l| l.starts_with(prefix)).count()
    }

    fn lines_where(&self, f: impl Fn(&str) -> bool) -> usize {
        self.body.iter().filter(|l| f(l)).count()
    }

    fn word(&self, word: &str) -> bool {
        self.word_count(word) > 0
    }

    fn word_count(&self, word: &str) -> usize {
        let mut n = 0;
        let mut from = 0;
        while let Some(at) = self.code[from..].find(word) {
            let start = from + at;
            let end = start + word.len();
            let before = self.code[..start].chars().next_back();
            let after = self.code[end..].chars().next();
            let ident = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
            if !ident(before) && !ident(after) {
                n += 1;
            }
            from = end;
        }
        n
    }

    fn lower_word(&self, word: &str) -> bool {
        let mut from = 0;
        while let Some(at) = self.lower[from..].find(word) {
            let start = from + at;
            let end = start + word.len();
            let ident = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
            if !ident(self.lower[..start].chars().next_back())
                && !ident(self.lower[end..].chars().next())
            {
                return true;
            }
            from = end;
        }
        false
    }

    fn ending_ratio(&self, end: &str) -> f32 {
        if self.body.is_empty() {
            return 0.0;
        }
        self.lines_where(|l| l.ends_with(end)) as f32 / self.body.len() as f32
    }
}

fn found(lang: &'static str, confidence: f32) -> Option<Detected> {
    Some(Detected { lang, confidence })
}

fn structured(ctx: &Ctx) -> Option<Detected> {
    let first = *ctx.body.first()?;
    let code = ctx.code.trim();
    let methods = [
        "GET ", "POST ", "PUT ", "DELETE ", "PATCH ", "HEAD ", "OPTIONS ",
    ];
    if methods.iter().any(|m| first.starts_with(m))
        && (first.contains(" HTTP/") || ctx.lines_where(|l| l.starts_with("Host:")) > 0)
    {
        return found("http", 0.97);
    }
    if first.starts_with("HTTP/")
        && first
            .split(' ')
            .nth(1)
            .is_some_and(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()))
    {
        return found("http", 0.97);
    }
    if first.starts_with(":method:")
        || first.starts_with(":path:")
        || first.starts_with(":authority:")
    {
        return found("http", 0.85);
    }
    if ctx.starts("@@ -") > 0
        || ctx.starts("diff --git") > 0
        || (ctx.starts("--- ") > 0 && ctx.starts("+++ ") > 0)
    {
        return found("diff", 0.96);
    }
    if code.starts_with(['{', '[']) && code.ends_with(['}', ']']) {
        if serde_json::from_str::<serde::de::IgnoredAny>(code).is_ok() {
            return found("json", 0.97);
        }
        let keys = ctx.lines_where(|l| l.starts_with('"') && l.contains("\":"));
        if keys >= 2 && !code.contains(';') {
            return found("json", 0.7);
        }
    }
    if ctx.lower.starts_with("<?xml") {
        return found("xml", 0.96);
    }
    let lower = &ctx.lower;
    if lower.starts_with("<!doctype html") || lower.starts_with("<html") {
        return found("html", 0.96);
    }
    if code.starts_with('<') && code.ends_with('>') && !code.starts_with("<?php") {
        let tags = code.matches('<').count();
        if tags >= 2 && !code.contains(" -> ") {
            let html = [
                "<div", "<span", "<a ", "<body", "<head", "<script", "<p>", "<br", "<table",
                "<img", "<input", "<form", "<ul", "<li", "<h1", "<style", "<link", "<meta",
                "<button", "<iframe",
            ];
            let is_html = html.iter().any(|t| lower.contains(t));
            return found(
                if is_html { "html" } else { "xml" },
                if tags >= 4 { 0.9 } else { 0.75 },
            );
        }
    }
    let docker = [
        "FROM ",
        "RUN ",
        "COPY ",
        "CMD ",
        "ENTRYPOINT ",
        "WORKDIR ",
        "EXPOSE ",
        "ENV ",
        "ADD ",
        "ARG ",
        "USER ",
        "LABEL ",
    ];
    let docker_lines = ctx.lines_where(|l| docker.iter().any(|d| l.starts_with(d)));
    let from_first = ctx
        .body
        .iter()
        .find(|l| !l.starts_with('#'))
        .is_some_and(|l| l.starts_with("FROM "));
    if from_first || docker_lines >= 3 && docker_lines * 2 >= ctx.body.len() {
        return found("dockerfile", if from_first { 0.95 } else { 0.8 });
    }
    let prompts = ctx
        .body
        .iter()
        .filter(|l| shell::prompt(l).is_some())
        .count();
    let strong = ctx.body.iter().filter(|l| strong_prompt(l)).count();
    let first_is_prompt = strong_prompt(first) || first.starts_with('┌');
    if strong >= 1 && (first_is_prompt || strong * 3 >= ctx.body.len()) && prompts >= 1 {
        return found("console", if strong >= 2 { 0.93 } else { 0.85 });
    }
    let sections = ctx.lines_where(|l| l.starts_with('[') && l.ends_with(']') && l.len() > 2);
    let pairs = ctx.lines_where(|l| {
        l.split_once('=').is_some_and(|(k, _)| {
            let k = k.trim();
            !k.is_empty()
                && k.chars()
                    .all(|c| c.is_alphanumeric() || "_-. \"".contains(c))
                && !k.contains("  ")
        })
    });
    let comments = ctx.lines_where(|l| l.starts_with('#') || l.starts_with(';'));
    if sections >= 1 && pairs >= 1 && sections + pairs + comments >= ctx.body.len() * 7 / 10 {
        let toml_cues = ctx.lines_where(|l| {
            l.contains(" = \"")
                || l.contains(" = [")
                || l.contains(" = true")
                || l.contains(" = false")
                || l.starts_with("[[")
        }) as f32;
        let ini_cues = ctx
            .lines_where(|l| l.starts_with(';') || (l.contains('=') && !l.contains(" = ")))
            as f32;
        return if toml_cues > ini_cues {
            found("toml", 0.85)
        } else {
            found("ini", 0.8)
        };
    }
    if sections == 0
        && pairs >= 3
        && pairs == ctx.body.len() - comments
        && ctx.lines_where(|l| l.contains(" = \"")) * 2 >= pairs
    {
        return found("toml", 0.7);
    }
    None
}

fn strong_prompt(line: &str) -> bool {
    if line.starts_with("# ") || line.starts_with("% ") {
        return false;
    }
    if line.starts_with("$ ") {
        return line.len() > 2;
    }
    shell::prompt(line).is_some()
}

fn first_word(line: &str) -> &str {
    let mut line = line.trim();
    for prompt in ["$ ", "# ", "% "] {
        if let Some(rest) = line.strip_prefix(prompt) {
            line = rest.trim_start();
            break;
        }
    }
    loop {
        let word = line.split_whitespace().next().unwrap_or("");
        let is_assignment = word.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit())
        });
        if matches!(word, "sudo" | "doas") || is_assignment {
            line = line[word.len()..].trim_start();
            while line.starts_with('-') && matches!(word, "sudo" | "doas") {
                let flag = line.split_whitespace().next().unwrap_or("");
                line = line[flag.len()..].trim_start();
            }
            continue;
        }
        return line
            .split(|c: char| c.is_whitespace() || c == ';')
            .next()
            .unwrap_or("");
    }
}

const NOT_COMMANDS: &[&str] = &[
    "def", "if", "for", "while", "return", "import", "from", "print", "class", "function", "var",
    "let", "const", "select", "insert", "update", "delete", "create", "use", "show", "else",
    "elif", "end", "do", "then", "package", "public", "private", "static", "void", "int", "the",
    "a", "an", "to", "and", "new", "this",
];

fn command_like(line: &str) -> bool {
    let line = line.strip_prefix("sudo ").unwrap_or(line);
    let line = line.strip_prefix("./").map_or(line, |rest| rest);
    let word = line.split_whitespace().next().unwrap_or("");
    let ok_word = !word.is_empty()
        && word.len() <= 28
        && word.starts_with(|c: char| c.is_ascii_lowercase())
        && word
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-/".contains(c))
        && !NOT_COMMANDS.contains(&word)
        && !word.ends_with(".exe");
    ok_word
        && line.len() > word.len()
        && !line[word.len()..].starts_with(['(', '=', '.', ':', ',', ';'])
        && !line.ends_with(['{', ';', ':', ','])
        && !line.contains(" = ")
        && !line.contains('\\')
}

fn bash(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    if ctx
        .body
        .first()
        .is_some_and(|l| l.starts_with("#!") && (l.contains("sh") || l.contains("bash")))
    {
        s += 6.0;
    }
    let known = ctx
        .body
        .iter()
        .filter(|l| !l.starts_with('#'))
        .filter(|l| commands::is_unix_command(first_word(l)))
        .count();
    s += 1.6 * (known.min(4) as f32);
    let piped = ctx
        .body
        .iter()
        .filter(|l| {
            l.contains(" | ")
                && !l.contains("||")
                && commands::is_unix_command(first_word(l.split(" | ").nth(1).unwrap_or("").trim()))
        })
        .count();
    s += 0.8 * (piped.min(2) as f32);
    let flags = ctx
        .body
        .iter()
        .map(|l| {
            l.split_whitespace()
                .filter(|w| {
                    w.starts_with('-')
                        && w.len() > 1
                        && w[1..].starts_with(|c: char| c.is_alphabetic() || c == '-')
                })
                .count()
        })
        .sum::<usize>();
    s += 0.5 * (flags.min(4) as f32);
    if ctx.has("$(") || ctx.has("${") {
        s += 1.0;
    }
    if ctx.body.iter().any(|l| {
        l.contains('$')
            && l.split('$')
                .skip(1)
                .any(|r| r.starts_with(|c: char| c.is_ascii_uppercase()))
    }) {
        s += 0.8;
    }
    if ctx.has("2>&1") || ctx.has("/dev/null") || ctx.has("/dev/tcp") {
        s += 2.0;
    }
    if ctx.has(" && ") {
        s += 0.6;
    }
    if ctx.lines_where(|l| {
        l == "fi"
            || l == "done"
            || l == "esac"
            || l.starts_with("fi ")
            || l.ends_with("; then")
            || l.ends_with("; do")
    }) > 0
    {
        s += 2.5;
    }
    if ctx.has("if [") || ctx.has("[[ ") {
        s += 2.0;
    }
    if ctx.starts("export ") > 0 {
        s += 1.5;
    }
    if ctx.starts("sudo ") > 0 {
        s += 1.5;
    }
    for path in [
        "/etc/", "/usr/", "/tmp/", "/var/", "~/", "/opt/", "/home/", "/bin/",
    ] {
        if ctx.has(path) {
            s += 0.6;
        }
    }
    if ctx.lines_where(|l| {
        l.starts_with("./") || l.starts_with("python3 ") || l.starts_with("chmod +x")
    }) > 0
    {
        s += 1.0;
    }
    if ctx.has("<<EOF") || ctx.has("<< EOF") || ctx.has("<<'EOF'") {
        s += 2.0;
    }
    let command_like = ctx
        .body
        .iter()
        .filter(|l| !l.starts_with('#') && command_like(l))
        .count();
    let non_comment = ctx.body.iter().filter(|l| !l.starts_with('#')).count();
    s += 0.7 * (command_like.min(3) as f32);
    if command_like > 0 && command_like == non_comment {
        s += 1.5;
    }
    let windows_first = ctx
        .body
        .iter()
        .filter(|l| !l.starts_with('#'))
        .filter(|l| {
            let word = first_word(l);
            word.to_lowercase().ends_with(".exe")
                || (commands::is_windows_command(&word.to_lowercase())
                    && !commands::is_unix_command(word))
        })
        .count();
    s -= 2.5 * (windows_first.min(2) as f32);
    if ctx.ending_ratio(";") > 0.4 && ctx.body.len() >= 2 {
        s -= 3.0;
    }
    if ctx.has("Get-") || ctx.has("$_.") || ctx.has("-eq ") && ctx.has("$") {
        s -= 1.5;
    }
    if ctx.has("C:\\") || ctx.has("%") && ctx.has("%PATH%") {
        s -= 1.5;
    }
    if ctx.has("def ") && ctx.has("):") {
        s -= 3.0;
    }
    s.max(0.0)
}

const CMDLET_VERBS: &[&str] = &[
    "Get",
    "Set",
    "New",
    "Remove",
    "Add",
    "Invoke",
    "Import",
    "Export",
    "Out",
    "Write",
    "Read",
    "Select",
    "Where",
    "ForEach",
    "Start",
    "Stop",
    "Test",
    "Enable",
    "Disable",
    "Install",
    "Copy",
    "Move",
    "Clear",
    "ConvertTo",
    "ConvertFrom",
    "Format",
    "Measure",
    "Sort",
    "Group",
    "Compare",
    "Register",
    "Unregister",
    "Wait",
    "Find",
    "Show",
    "Update",
    "Restart",
    "Resolve",
    "Send",
    "Split",
    "Join",
    "Rename",
    "Enter",
    "Exit",
];

fn powershell(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    let mut cmdlets: Vec<&str> = Vec::new();
    for word in ctx
        .code
        .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
    {
        if let Some((verb, noun)) = word.split_once('-')
            && CMDLET_VERBS.contains(&verb)
            && noun.starts_with(|c: char| c.is_ascii_uppercase())
            && !cmdlets.contains(&word)
        {
            cmdlets.push(word);
        }
    }
    s += 2.5 * (cmdlets.len().min(3) as f32);
    for marker in [
        "$_",
        "$env:",
        "$PSVersionTable",
        "[System.",
        "[string]",
        "[int]",
        "[byte[]]",
        "[Convert]::",
        "[Net.",
        "[Reflection.",
        "$true",
        "$false",
        "$null",
        "$PSScriptRoot",
        "$args",
    ] {
        if ctx.has(marker) {
            s += 1.5;
        }
    }
    let operators = [
        " -eq ",
        " -ne ",
        " -gt ",
        " -lt ",
        " -ge ",
        " -le ",
        " -match ",
        " -like ",
        " -and ",
        " -or ",
        " -contains ",
        " -replace ",
        " -notmatch ",
        " -split ",
        " -join ",
    ]
    .iter()
    .filter(|o| ctx.lower.contains(*o))
    .count();
    s += 1.5 * (operators.min(2) as f32);
    if ctx.has("param(") || ctx.has("param (") {
        s += 2.5;
    }
    if ctx.has("| %") || ctx.has("| ?") || ctx.has("| ForEach") || ctx.has("| Where") {
        s += 2.0;
    }
    if ctx.has("@{") || ctx.has("@(") {
        s += 1.0;
    }
    if ctx.has("`\n") {
        s += 1.5;
    }
    for marker in [
        "powershell ",
        "powershell.exe",
        "pwsh ",
        "-ExecutionPolicy",
        "-NoProfile",
        "-EncodedCommand",
        "-WindowStyle",
        "-nop ",
        "IEX",
        "iex ",
        "Invoke-Expression",
        "DownloadString",
        ".ps1",
        "-Enc ",
    ] {
        if ctx.has(marker) {
            s += 1.5;
        }
    }
    let assignments =
        ctx.lines_where(|l| l.starts_with('$') && l.contains(" = ") && !l.ends_with(';'));
    s += 1.6 * (assignments.min(2) as f32);
    if ctx
        .lines_where(|l| l.starts_with(".\\") || l.starts_with(". .\\") || l.starts_with(". C:\\"))
        > 0
    {
        s += 1.2;
    }
    if ctx.has("Write-Host") || ctx.has("Write-Output") {
        s += 1.0;
    }
    if ctx.has("<?php") {
        s -= 4.0;
    }
    s.max(0.0)
}

fn cmd(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    let lower_first_words = |l: &str| {
        l.trim_start_matches('@')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase()
    };
    if ctx.lower.contains("@echo off") {
        s += 6.0;
    }
    let commands = ctx
        .body
        .iter()
        .filter(|l| {
            let word = lower_first_words(l);
            let has_arguments = l.trim().len() > word.len();
            commands::is_windows_command(&word)
                || (has_arguments && (word.ends_with(".exe") || word.contains(":\\")))
        })
        .count();
    s += 1.4 * (commands.min(4) as f32);
    let variables = ctx.code.matches('%').count() / 2;
    if ctx.code.split('%').skip(1).step_by(2).any(|v| {
        v.starts_with(|c: char| c.is_alphabetic() || c == '~')
            && v.len() < 24
            && v.chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '~')
    }) {
        s += 1.0 + 0.5 * (variables.min(3) as f32);
    }
    if ctx.lines_where(|l| l.to_lowercase().starts_with("set ") && l.contains('=')) > 0 {
        s += 1.5;
    }
    for marker in [
        "if exist",
        "if not exist",
        "if errorlevel",
        "if /i",
        "goto ",
        "goto:",
        "setlocal",
        "%~dp0",
        "%errorlevel%",
        " /c ",
        "cmd /c",
        "cmd.exe",
    ] {
        if ctx.lower.contains(marker) {
            s += 1.5;
        }
    }
    if ctx.lines_where(|l| l.starts_with(':') && l.len() > 1 && !l.starts_with("::")) > 0
        || ctx.lines_where(|l| l.to_lowercase().starts_with("rem ") || l.starts_with("::")) > 0
    {
        s += 1.5;
    }
    const SHARED: &[&str] = &[
        "echo", "set", "cd", "start", "find", "sort", "more", "hostname", "ping", "mkdir", "tree",
        "ver", "path",
    ];
    if ctx
        .body
        .iter()
        .any(|l| SHARED.contains(&lower_first_words(l).as_str()))
    {
        s += 0.8;
    }
    let slashes = ctx
        .body
        .iter()
        .map(|l| {
            l.split_whitespace()
                .filter(|w| {
                    w.starts_with('/')
                        && w.len() > 1
                        && w.len() < 20
                        && w[1..].starts_with(|c: char| c.is_alphabetic())
                        && !w[1..].contains('/')
                })
                .count()
        })
        .sum::<usize>();
    s += 0.8 * (slashes.min(3) as f32);
    if ctx.has(":\\") || ctx.has("\\\\") {
        s += 0.8;
    }
    if ctx.has("Get-") || ctx.has("$_") || ctx.has("-eq ") {
        s -= 2.5;
    }
    if ctx.has("$(") || ctx.has("sudo ") || ctx.has("#!/") {
        s -= 2.0;
    }
    s.max(0.0)
}

fn python(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    let def = ctx.lines_where(|l| l.starts_with("def ") && l.ends_with(':'));
    s += 3.0 * (def.min(2) as f32);
    if ctx.starts("class ") > 0
        && ctx.lines_where(|l| l.starts_with("class ") && l.ends_with(':')) > 0
    {
        s += 3.0;
    }
    let imports = ctx.lines_where(|l| {
        (l.starts_with("import ") && !l.ends_with(';') && !l.contains(" from ") && !l.contains('{'))
            || (l.starts_with("from ") && l.contains(" import "))
    });
    s += 2.0 * (imports.min(2) as f32);
    if ctx.has("self.") {
        s += 1.5;
    }
    if ctx.has("print(") {
        s += 0.8;
    }
    if ctx.starts("elif ") > 0 {
        s += 3.0;
    }
    if ctx.has("__name__") || ctx.has("__init__") {
        s += 3.0;
    }
    let blocks = ctx.lines_where(|l| {
        [
            "if ", "for ", "while ", "with ", "try", "except", "else", "finally",
        ]
        .iter()
        .any(|k| l.starts_with(k))
            && l.ends_with(':')
    });
    s += 1.0 * (blocks.min(3) as f32);
    for marker in [
        " None",
        "True",
        "False",
        "lambda ",
        " in range(",
        "isinstance(",
        ".append(",
        "len(",
        "raise ",
        "f\"",
        "f'",
        " is not ",
        "import requests",
        "import os",
        "import sys",
        "import socket",
        "subprocess",
        "argparse",
        "def __",
    ] {
        if ctx.has(marker) {
            s += 0.8;
        }
    }
    if ctx.has("#!/usr/bin/env python") || ctx.has("#!/usr/bin/python") {
        s += 5.0;
    }
    if ctx.ending_ratio(";") > 0.3 {
        s -= 3.0;
    }
    if ctx.has("{\n") && ctx.has("\n}") && !ctx.has("= {") {
        s -= 2.0;
    }
    if ctx.has("$(") || ctx.has("<?php") || ctx.has("function ") {
        s -= 2.0;
    }
    s.max(0.0)
}

fn c(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    if ctx.starts("#include <") > 0 || ctx.starts("#include \"") > 0 {
        s += 4.0;
        if ctx.has("<stdio.h>")
            || ctx.has("<stdlib.h>")
            || ctx.has("<string.h>")
            || ctx.has("<unistd.h>")
            || ctx.has("<windows.h>")
        {
            s += 1.5;
        }
    }
    if ctx.has("int main(") || ctx.has("int main (") || ctx.has("void main(") {
        s += 4.0;
    }
    for marker in [
        "printf(",
        "scanf(",
        "malloc(",
        "free(",
        "sizeof(",
        "strcpy(",
        "memcpy(",
        "fprintf(",
        "NULL",
        "unsigned ",
        "char *",
        "char*",
        "uint8_t",
        "DWORD",
        "HANDLE",
        "LPVOID",
        "->",
        "#define ",
        "WINAPI",
        "size_t",
        "(void)",
    ] {
        if ctx.has(marker) {
            s += 0.9;
        }
    }
    if ctx.lines_where(|l| l.starts_with("struct ") && l.ends_with('{')) > 0
        || ctx.lines_where(|l| l.starts_with("typedef ")) > 0
    {
        s += 1.5;
    }
    if ctx.lines_where(|l| {
        (l.starts_with("void ")
            || l.starts_with("int ")
            || l.starts_with("char ")
            || l.starts_with("static "))
            && l.contains('(')
            && (l.ends_with('{') || l.ends_with(')') || l.ends_with(");"))
    }) > 0
    {
        s += 1.5;
    }
    if ctx.ending_ratio(";") >= 0.3 {
        s += 1.0;
    }
    if ctx.has("std::")
        || ctx.has("cout")
        || ctx.has("class ") && ctx.has("public:")
        || ctx.has("namespace ")
        || ctx.has("template")
    {
        s -= 3.0;
    }
    s.max(0.0)
}

fn cpp(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    if ctx.has("std::") {
        s += 3.5;
    }
    for header in [
        "<iostream>",
        "<vector>",
        "<string>",
        "<map>",
        "<memory>",
        "<algorithm>",
        "<fstream>",
        "<thread>",
    ] {
        if ctx.has(header) {
            s += 4.0;
            break;
        }
    }
    for marker in [
        "cout",
        "cin >>",
        "endl",
        "nullptr",
        "template<",
        "template <",
        "namespace ",
        "using namespace",
        "public:",
        "private:",
        "protected:",
        "constexpr",
        "static_cast<",
        "reinterpret_cast<",
        "::",
        "auto ",
        "virtual ",
        "override",
        "delete ",
        "new ",
    ] {
        if ctx.has(marker) {
            s += 0.9;
        }
    }
    if ctx.lines_where(|l| l.starts_with("class ") && (l.ends_with('{') || l.contains(" : public")))
        > 0
    {
        s += 2.0;
    }
    if ctx.starts("#include") > 0 {
        s += 1.5;
    }
    if ctx.has("int main(") {
        s += 1.0;
    }
    if ctx.has("using System") || ctx.has("Console.") {
        s -= 3.0;
    }
    s.max(0.0)
}

fn csharp(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    if ctx.starts("using System") > 0 {
        s += 4.5;
    }
    for marker in [
        "Console.Write",
        "{ get; set; }",
        "{ get; }",
        "[DllImport",
        "Marshal.",
        "IntPtr",
        "foreach (var",
        "async Task",
        "namespace ",
        "static void Main",
        "string[] args",
        "var ",
        " new ",
        "public class",
        "public static",
        "using (",
        "Assembly.",
        "typeof(",
        "$\"",
        "@\"",
        "List<",
        "Dictionary<",
        "out ",
        "ref ",
        "byte[]",
        "string ",
        "null;",
        ".Length",
        "System.",
    ] {
        if ctx.has(marker) {
            s += 0.9;
        }
    }
    if ctx.has("static void Main") || ctx.has("static int Main") {
        s += 3.0;
    }
    if ctx.has("System.out") || ctx.has("import java") {
        s -= 4.0;
    }
    if ctx.has("#include") || ctx.has("std::") {
        s -= 3.0;
    }
    s.max(0.0)
}

fn java(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    if ctx.starts("import java") > 0
        || ctx.starts("import javax") > 0
        || ctx.starts("import org.") > 0
    {
        s += 4.5;
    }
    if ctx.has("System.out.print") {
        s += 4.0;
    }
    if ctx.has("public static void main(String") {
        s += 5.0;
    }
    if ctx.starts("package ") > 0
        && ctx.lines_where(|l| l.starts_with("package ") && l.ends_with(';')) > 0
    {
        s += 3.0;
    }
    for marker in [
        "@Override",
        "public class",
        "private ",
        "final ",
        "throws ",
        "new ArrayList<",
        "String[]",
        "extends ",
        "implements ",
        "@Autowired",
        "@RequestMapping",
        "Exception e",
        "catch (",
        "System.getProperty",
        "Runtime.getRuntime",
        "ProcessBuilder",
        "ObjectInputStream",
        "void ",
        "this.",
    ] {
        if ctx.has(marker) {
            s += 0.9;
        }
    }
    if ctx.has("using System") || ctx.has("Console.") {
        s -= 4.0;
    }
    if ctx.has("#include") || ctx.has("function ") || ctx.has("def ") {
        s -= 3.0;
    }
    s.max(0.0)
}

fn javascript(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    if ctx.has("console.log(") {
        s += 3.0;
    }
    if ctx.has("require(") {
        s += 2.0;
    }
    if ctx.has("module.exports") {
        s += 3.0;
    }
    if ctx.has("document.") || ctx.has("window.") || ctx.has("document[") {
        s += 2.0;
    }
    if ctx.has("===") || ctx.has("!==") {
        s += 2.0;
    }
    if ctx
        .lines_where(|l| l.starts_with("const ") || l.starts_with("let ") || l.starts_with("var "))
        > 0
        || ctx.has("const ") && ctx.has(" = ")
    {
        s += 1.8;
    }
    if ctx.has("=>") {
        s += 1.5;
    }
    if ctx.word("function") {
        s += 1.5;
    }
    for marker in [
        "async ",
        "await ",
        "export default",
        "export const",
        "export function",
        ".then(",
        "fetch(",
        "new Promise",
        "JSON.",
        "undefined",
        "XMLHttpRequest",
        "addEventListener",
        "innerHTML",
        "localStorage",
        "$.ajax",
        "alert(",
        "eval(",
        "atob(",
        "btoa(",
        "Object.",
        "Array.",
        "setTimeout(",
        "String.fromCharCode",
    ] {
        if ctx.has(marker) {
            s += 1.0;
        }
    }
    if ctx.lines_where(|l| l.starts_with("import ") && l.contains(" from ")) > 0 {
        s += 3.0;
    }
    if ctx.has("<?php") || ctx.has("def ") && ctx.has("):") || ctx.has("#include") {
        s -= 4.0;
    }
    if ctx.has("interface ") && ctx.has(": string") {
        s -= 2.0;
    }
    s.max(0.0)
}

fn typescript(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    let annotations = [
        ": string",
        ": number",
        ": boolean",
        ": any",
        ": void",
        ": unknown",
        ": string[]",
        ": Promise<",
        "<T>",
        " as string",
        " as const",
        "readonly ",
        "private ",
        "enum ",
    ]
    .iter()
    .filter(|m| ctx.has(m))
    .count();
    if annotations == 0
        && !ctx.has("interface ")
        && !ctx.lines_where(|l| l.starts_with("type ") && l.contains(" = ")) > 0
    {
        return 0.0;
    }
    s += 2.0 * (annotations.min(3) as f32);
    if ctx.lines_where(|l| l.starts_with("interface ") || l.starts_with("export interface ")) > 0 {
        s += 3.5;
    }
    if ctx.lines_where(|l| l.starts_with("type ") && l.contains(" = ")) > 0 {
        s += 3.0;
    }
    s + javascript(ctx) * 0.8
}

fn go(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    if ctx.lines_where(|l| l.starts_with("package ") && !l.ends_with(';')) > 0 {
        s += 4.0;
    }
    if ctx.has("func ")
        && (ctx.has("func (") || ctx.lines_where(|l| l.starts_with("func ") && l.contains('(')) > 0)
    {
        s += 3.0;
    }
    if ctx.has(" := ") {
        s += 2.0;
    }
    for marker in [
        "fmt.",
        "err != nil",
        "defer ",
        "map[string]",
        "[]byte",
        "[]string",
        "import (",
        "go func",
        "chan ",
        "struct {",
        "interface{}",
        "make(",
        "range ",
    ] {
        if ctx.has(marker) {
            s += 1.4;
        }
    }
    if ctx.has("err != nil") {
        s += 2.0;
    }
    if ctx.has("function ") || ctx.has("let ") {
        s -= 3.0;
    }
    s.max(0.0)
}

fn rust(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    if ctx.has("fn main()") {
        s += 4.0;
    }
    if ctx.lines_where(|l| {
        (l.starts_with("fn ")
            || l.starts_with("pub fn ")
            || l.starts_with("async fn ")
            || l.starts_with("pub async fn "))
            && l.contains('(')
    }) > 0
    {
        s += 3.5;
    }
    if ctx.has("let mut ") {
        s += 3.0;
    }
    for marker in [
        "println!(",
        "format!(",
        "vec![",
        "use std::",
        "impl ",
        "&str",
        "&mut ",
        "::new(",
        "#[derive",
        "Result<",
        "Option<",
        ".unwrap()",
        "-> ",
        "match ",
        "mod ",
        "pub struct",
        "&self",
        "'static",
        "Box<",
        "unsafe {",
        "extern crate",
        "use crate::",
        "None",
        "Some(",
    ] {
        if ctx.has(marker) {
            s += 1.1;
        }
    }
    if ctx.has("use std::") || ctx.has("#[derive") {
        s += 2.0;
    }
    if ctx.has("function ") || ctx.has("var ") || ctx.has("def ") {
        s -= 3.0;
    }
    s.max(0.0)
}

fn php(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    let opens_line = ctx
        .body
        .iter()
        .any(|l| l.starts_with("<?php") || l.starts_with("<?="));
    if opens_line {
        s += 7.0;
    } else if ctx.has("<?php") || ctx.has("<?=") {
        s += 2.0;
    }
    for marker in [
        "$_GET",
        "$_POST",
        "$_REQUEST",
        "$_SERVER",
        "$_COOKIE",
        "$_FILES",
        "$_SESSION",
    ] {
        if ctx.has(marker) {
            s += 3.5;
        }
    }
    for marker in [
        "$this->",
        "array(",
        "isset(",
        "mysqli_",
        "mysql_",
        "echo ",
        "shell_exec(",
        "passthru(",
        "system(",
        "exec(",
        "eval(",
        "base64_decode(",
        "include(",
        "require_once",
        "preg_match(",
        "file_get_contents(",
        "->",
        "=>",
        "function ",
    ] {
        if ctx.has(marker) {
            s += 0.8;
        }
    }
    if ctx.code.matches('$').count() >= 2 && ctx.ending_ratio(";") > 0.4 && ctx.has("function ") {
        s += 1.5;
    }
    let shell_writer = ctx.body.iter().any(|l| {
        commands::is_unix_command(first_word(l)) && (l.contains("'<?php") || l.contains("\"<?php"))
    });
    if shell_writer && !opens_line {
        s -= 6.0;
    }
    if ctx.has("Get-") || ctx.has("$_.") {
        s -= 1.0;
    }
    s.max(0.0)
}

fn ruby(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    let ends = ctx.lines_where(|l| l == "end");
    if ends > 0 && (ctx.has("def ") || ctx.has(" do") || ctx.has("class ") || ctx.has("if ")) {
        s += 3.0 + (ends.min(2) as f32);
    }
    if ctx.lines_where(|l| l.starts_with("def ") && !l.ends_with(':') && !l.contains('{')) > 0 {
        s += 1.5;
    }
    for marker in [
        "puts ",
        "require '",
        "require \"",
        ".each do |",
        "attr_accessor",
        "attr_reader",
        "unless ",
        "elsif ",
        "#{",
        " do |",
        "nil",
        "@@",
        "Gem::",
        "module ",
        "=> e",
        "rescue",
        "msf",
        "Msf::",
        "class << self",
        ".map {",
        "include ",
    ] {
        if ctx.has(marker) {
            s += 1.1;
        }
    }
    if ctx.lines_where(|l| l.starts_with("class ") && l.contains(" < ")) > 0 {
        s += 3.0;
    }
    if ctx.has("#!/usr/bin/env ruby") || ctx.has("#!/usr/bin/ruby") {
        s += 5.0;
    }
    if ctx.has("def ") && ctx.has("):") {
        s -= 4.0;
    }
    if ctx.has("function ") || ctx.has("#include") {
        s -= 2.0;
    }
    s.max(0.0)
}

fn perl(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    if ctx.has("#!/usr/bin/perl") || ctx.has("#!/usr/bin/env perl") {
        s += 6.0;
    }
    for marker in [
        "use strict",
        "use warnings",
        "my $",
        "my @",
        "my %",
        "=~",
        "@ARGV",
        "%ENV",
        "$ENV{",
        "foreach my",
        "$_",
        "sub ",
        "print \"",
        "die ",
        "open(",
        "local $",
        "->{",
        "qw(",
        "chomp",
    ] {
        if ctx.has(marker) {
            s += 1.4;
        }
    }
    if ctx.has("use strict") || ctx.has("my $") {
        s += 2.0;
    }
    if ctx.has("<?php") || ctx.has("function ") {
        s -= 3.0;
    }
    s.max(0.0)
}

fn lua(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    let ends = ctx.lines_where(|l| l == "end" || l.starts_with("end)") || l.starts_with("end,"));
    if ends > 0 && ctx.lower_word("then")
        || ctx.has("function") && ctx.has("end") && ctx.has("local ")
    {
        s += 3.0;
    }
    if ctx.lines_where(|l| l.starts_with("local ") && l.contains('=')) > 0 {
        s += 2.5;
    }
    for marker in [
        "elseif",
        " then",
        "~=",
        "ipairs(",
        "pairs(",
        "nil",
        "require(\"",
        "require(\'",
        "--[[",
        "local function",
        "string.",
        "table.insert",
        "os.execute",
        "io.open",
        "..",
        "#!/usr/bin/lua",
    ] {
        if ctx.has(marker) {
            s += 1.1;
        }
    }
    if ctx.lines_where(|l| l.starts_with("--")) > 0 && ctx.has("-- ") {
        s += 0.5;
    }
    if ctx.has("def ") || ctx.has("{\n") || ctx.ending_ratio(";") > 0.3 {
        s -= 3.0;
    }
    s.max(0.0)
}

const SQL_WORDS: &[&str] = &[
    "select",
    "from",
    "where",
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
    "join",
    "group by",
    "order by",
    "union",
    "having",
    "limit",
    "like",
    "database",
    "inner join",
    "left join",
    "null",
    "count(",
    "distinct",
];

fn sql(ctx: &Ctx) -> f32 {
    let mut s: f32 = 0.0;
    let first = ctx.body.first().map_or(String::new(), |l| l.to_lowercase());
    for start in [
        "select ",
        "insert ",
        "update ",
        "delete ",
        "create ",
        "alter ",
        "drop ",
        "declare ",
        "exec ",
        "grant ",
        "truncate ",
    ] {
        if first.starts_with(start) {
            s += 3.0;
            break;
        }
    }
    let words = SQL_WORDS.iter().filter(|w| ctx.lower_word(w)).count();
    s += 1.1 * (words.min(6) as f32);
    for marker in [
        "union select",
        "information_schema",
        "xp_cmdshell",
        "@@version",
        "waitfor delay",
        "sleep(",
        "' or ",
        "or 1=1",
        "' --",
        "'--",
        "load_file(",
        "into outfile",
        "sys.databases",
        "sysobjects",
        "pg_sleep",
        "extractvalue(",
        "updatexml(",
        "concat(",
    ] {
        if ctx.lower.contains(marker) {
            s += 2.5;
        }
    }
    if ctx.has("SELECT") && ctx.has("FROM") {
        s += 1.5;
    }
    if ctx.has("{\n") || ctx.has("function ") || ctx.has("def ") || ctx.has("#include") {
        s -= 2.0;
    }
    s.max(0.0)
}

fn asm(ctx: &Ctx) -> f32 {
    let mnemonics = [
        "mov", "push", "pop", "call", "ret", "jmp", "je", "jne", "jz", "jnz", "cmp", "test", "add",
        "sub", "xor", "lea", "int", "nop", "inc", "dec", "and", "or", "shl", "shr", "leave",
        "syscall", "ldr", "str", "bl", "svc", "stp", "ldp", "movzx", "imul", "div",
    ];
    let hits = ctx
        .body
        .iter()
        .filter(|l| {
            let l = l.split([';', '#']).next().unwrap_or("").trim();
            let l = l.split_once(':').map_or(l, |(a, b)| {
                if !a.contains(' ') && a.len() < 24 && !b.starts_with(':') {
                    b.trim()
                } else {
                    l
                }
            });
            let word = l.split_whitespace().next().unwrap_or("").to_lowercase();
            mnemonics.contains(&word.as_str())
                && (l.len() == word.len() || l[word.len()..].starts_with(char::is_whitespace))
        })
        .count();
    let strong = [
        "mov", "push", "pop", "call", "ret", "jmp", "lea", "cmp", "xor", "syscall", "ldr", "str",
        "bl", "int",
    ];
    let strong_hits = ctx
        .body
        .iter()
        .filter(|l| {
            strong.contains(
                &l.split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_lowercase()
                    .as_str(),
            )
        })
        .count();
    let mut s: f32 = 0.0;
    if hits >= 2 && hits * 2 >= ctx.body.len() && strong_hits >= 1 {
        s += 3.0 + (hits.min(6) as f32);
    }
    for marker in [
        "\n.text",
        "section .text",
        ".globl",
        "global _start",
        "_start:",
        "[bits",
        "eax,",
        "rax,",
        "rsp",
        "ebp",
        "0x",
        "dword ptr",
        "qword ptr",
        "byte ptr",
    ] {
        if ctx.lower.contains(marker) || ctx.lower.starts_with(marker.trim_start_matches('\n')) {
            s += 1.2;
        }
    }
    if ctx.has("{") && ctx.has("}") {
        s -= 2.0;
    }
    s.max(0.0)
}

fn yaml(ctx: &Ctx) -> f32 {
    let key_lines = ctx.lines_where(|l| {
        let l = l.strip_prefix("- ").unwrap_or(l);
        l.split_once(':').is_some_and(|(k, v)| {
            !k.is_empty()
                && !k.contains(' ')
                && !k.contains([
                    '(', ')', '{', '"', '\'', '$', '/', '<', '>', '=', ';', '`', '[', '\\',
                ])
                && (v.is_empty() || v.starts_with(' '))
                && !matches!(
                    k,
                    "else"
                        | "try"
                        | "finally"
                        | "default"
                        | "case"
                        | "public"
                        | "private"
                        | "protected"
                        | "http"
                        | "https"
                        | "ftp"
                        | "BEGIN"
                        | "END"
                )
                && !v.trim_end().ends_with([';', '{'])
        })
    });
    let items = ctx.lines_where(|l| l.starts_with("- "));
    let starts_like_yaml = ctx.body.first().is_some_and(|l| {
        l.starts_with("---")
            || l.starts_with('#')
            || l.starts_with("- ")
            || l.split_once(':')
                .is_some_and(|(k, _)| !k.contains(' ') && !k.contains('/'))
    });
    if key_lines < 3
        || !starts_like_yaml
        || (key_lines + items + ctx.starts("#")) * 10 < ctx.body.len() * 7
    {
        return 0.0;
    }
    let mut s = 1.5 * (key_lines.min(4) as f32);
    if ctx.starts("---") > 0 {
        s += 3.0;
    }
    if items > 0 {
        s += 1.0;
    }
    if ctx.has("version:")
        || ctx.has("services:")
        || ctx.has("apiVersion:")
        || ctx.has("hosts:")
        || ctx.has("- name:")
        || ctx.has("steps:")
    {
        s += 2.5;
    }
    if ctx.ending_ratio(";") > 0.2 || ctx.has("def ") || ctx.has("{\n") {
        s -= 3.0;
    }
    s.max(0.0)
}

fn css(ctx: &Ctx) -> f32 {
    let props = ctx.lines_where(|l| {
        l.split_once(':').is_some_and(|(k, v)| {
            !k.is_empty()
                && k.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                && v.trim_end().ends_with(';')
        })
    });
    if props < 2 || !ctx.has('{'.to_string().as_str()) {
        return 0.0;
    }
    let mut s = 2.0 * (props.min(4) as f32);
    if ctx.lines_where(|l| {
        l.ends_with('{')
            && (l.starts_with('.')
                || l.starts_with('#')
                || l.starts_with("body")
                || l.starts_with("html")
                || l.chars().next().is_some_and(|c| c.is_ascii_lowercase()))
    }) > 0
    {
        s += 2.0;
    }
    if ctx.has("@media") || ctx.has("@import") || ctx.has("px;") || ctx.has("rem;") {
        s += 1.5;
    }
    s
}
