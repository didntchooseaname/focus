use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub struct Snippet {
    pub command: String,
    pub lang: String,
    pub tool: String,
    pub heading: Option<String>,
    pub line: u32,
    pub comment: Option<String>,
    pub placeholders: Vec<String>,
}

const SHELL_LANGS: &[&str] = &[
    "bash",
    "sh",
    "zsh",
    "shell",
    "fish",
    "console",
    "terminal",
    "powershell",
    "pwsh",
    "ps1",
    "ps",
    "cmd",
    "bat",
    "batch",
];
const NOT_COMMANDS: &[&str] = &[
    "import", "from", "def", "class", "function", "var", "let", "const", "package", "public",
    "private", "use", "fn", "select", "insert", "update", "delete", "create", "return", "if",
    "while", "begin", "end", "total", "usage",
];
const WRAPPERS: &[&str] = &[
    "sudo", "doas", "time", "nohup", "exec", "command", "env", "nice", "stdbuf", "builtin",
];
const SUDO_ARGUMENT_OPTIONS: &[&str] =
    &["-u", "-g", "-h", "-p", "-C", "-r", "-t", "-U", "-D", "-R"];

static PROMPTS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"^\s*(?:\([^)]*\)\s*)?[\w.-]+@[\w.-]+(?::[^\s$#]*)?\s*[$#]\s+",
        r"^\s*\[[^\]\n]+\][$#]\s+",
        r"^\s*PS\s+[^>\n]*>\s*",
        r"^\s*PS>\s*",
        r"^\s*[A-Za-z]:\\[^>\n]*>\s*",
        r"^\s*└─[$#]\s*",
        r"^\s*[$%]\s+",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("prompt pattern"))
    .collect()
});
static HEREDOC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<<-?\s*['"]?([A-Za-z_][A-Za-z0-9_]*)['"]?"#).expect("here-document pattern")
});
static OUTPUT_LINES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"^\*",
        r"^\+0x",
        r"^(?:-{3,}|={3,})",
        r"^\[0{4,}[0-9a-fA-F]*\]",
        r"^\d+/(?:tcp|udp)\s",
        r"^\d+\s+\d+\s",
        r"^[A-Za-z][\w /.\-]*?\s{2,}:\s",
        r"^[A-Za-z][\w /.\-]*?\s:\s*$",
        r"^[\w/.\-]+\s+:\s+\S",
        r"^[A-Z][a-z]+(?: [A-Za-z]+)*\s:\s",
        r"^[A-Za-z][\w ]*:\s{2,}\S",
        r"^[A-Z][A-Za-z]*(?: [A-Za-z]+)*\s{2,}\S",
        r"^[A-Za-z0-9_]+:$",
        r"^[A-Z][A-Z0-9]*:\s+\S",
        r"^[A-Z][A-Z0-9]*:[0-9a-fA-F]{16,}",
        r"^[A-Z]{3,}\s+[0-9a-fA-F]{8,}\b",
        r"^(?:[0-9a-fA-F]{2}[ :]){7,}",
        r"^\S+\s+[0-9a-fA-F]{32,}\s*$",
        r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-",
        r#"^"[^"]+"\s*:"#,
        r"^[{}\[\]),;]+$",
        r"^\.{3}$",
        r"^[A-Z][a-z]+(?: \S+){2,}:$",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("output pattern"))
    .collect()
});
static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ {0,3}(#{1,6})\s+(.*?)\s*#*\s*$").expect("heading pattern"));

fn strip_quote(line: &str) -> &str {
    let mut rest = line.trim_start();
    while let Some(after) = rest.strip_prefix('>') {
        rest = after.trim_start();
    }
    rest
}

struct Fence {
    marker: char,
    length: usize,
    lang: String,
    indent: usize,
    quoted: bool,
}

fn open_fence(line: &str) -> Option<Fence> {
    let quoted = line.trim_start().starts_with('>');
    let stripped = strip_quote(line);
    let indent = line.len() - line.trim_start().len();
    let marker = stripped.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let length = stripped.chars().take_while(|c| *c == marker).count();
    if length < 3 {
        return None;
    }
    let info = stripped[length..].trim();
    if marker == '`' && info.contains('`') {
        return None;
    }
    let lang = info
        .split(|c: char| c.is_whitespace() || c == '{' || c == ',')
        .next()
        .unwrap_or("")
        .to_lowercase();
    Some(Fence {
        marker,
        length,
        lang,
        indent,
        quoted,
    })
}

fn closes(fence: &Fence, line: &str) -> bool {
    let stripped = if fence.quoted {
        strip_quote(line)
    } else {
        line.trim_start()
    };
    let count = stripped.chars().take_while(|c| *c == fence.marker).count();
    count >= fence.length && stripped[count..].trim().is_empty()
}

fn content_line<'a>(fence: &Fence, line: &'a str) -> &'a str {
    let line = if fence.quoted {
        strip_quote(line)
    } else {
        line
    };
    let spaces = line.len() - line.trim_start_matches(' ').len();
    &line[spaces.min(fence.indent)..]
}

fn is_output_line(line: &str, lang: &str) -> bool {
    let t = line.trim();
    if t.starts_with('[')
        && !(matches!(lang, "powershell" | "pwsh" | "ps1" | "ps") && t.contains("]::"))
        && !t.starts_with("[ ")
    {
        return true;
    }
    OUTPUT_LINES.iter().any(|r| r.is_match(t))
}

fn remove_prompt(line: &str) -> (&str, bool) {
    for pattern in PROMPTS.iter() {
        if let Some(m) = pattern.find(line) {
            return (&line[m.end()..], true);
        }
    }
    (line, false)
}

fn is_comment(line: &str, lang: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('#')
        || (matches!(lang, "cmd" | "bat" | "batch")
            && (t.to_ascii_uppercase().starts_with("REM ") || t.starts_with("::")))
}

fn comment_text(line: &str) -> String {
    let t = line.trim_start();
    let t = if t.to_ascii_uppercase().starts_with("REM ") {
        &t[4..]
    } else {
        t.trim_start_matches(['#', ':'])
    };
    t.trim().to_string()
}

fn continues(line: &str, lang: &str) -> Option<String> {
    let trimmed = line.trim_end();
    let joiner = match lang {
        "powershell" | "pwsh" | "ps1" | "ps" => '`',
        "cmd" | "bat" | "batch" => '^',
        _ => '\\',
    };
    if let Some(body) = trimmed.strip_suffix(joiner) {
        return Some(body.trim_end().to_string());
    }
    for tail in ["&&", "||", "|"] {
        if trimmed.ends_with(tail) && !trimmed.ends_with("|||") {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn tool_of(command: &str) -> String {
    let mut tokens = command.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
        let is_assignment = token.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit())
        });
        if is_assignment {
            continue;
        }
        let bare = token.trim_matches(['"', '\'', '&', '(', '{']);
        if WRAPPERS.contains(&bare) {
            if bare == "sudo" || bare == "doas" {
                while let Some(next) = tokens.peek() {
                    if !next.starts_with('-') {
                        break;
                    }
                    let option = tokens.next().unwrap_or("");
                    if SUDO_ARGUMENT_OPTIONS.contains(&option) {
                        tokens.next();
                    }
                }
            }
            continue;
        }
        let name = bare.rsplit(['/', '\\']).next().unwrap_or(bare);
        return name.to_string();
    }
    String::new()
}

fn looks_like_command(first_line: &str) -> bool {
    let (rest, prompted) = remove_prompt(first_line);
    if prompted {
        return true;
    }
    let rest = rest.trim();
    let Some(word) = rest.split_whitespace().next() else {
        return false;
    };
    if rest.starts_with(['{', '[', '<', '-', '*', '|', '#', '@', '/'])
        && !word.starts_with("/usr")
        && !word.starts_with("/bin")
        && !word.starts_with("/opt")
    {
        return false;
    }
    if word.ends_with(':') || word.contains("://") && !word.starts_with("http") {
        return false;
    }
    let tool = tool_of(rest);
    let lower = tool.to_lowercase();
    if lower.is_empty() || NOT_COMMANDS.contains(&lower.as_str()) {
        return false;
    }
    let path_like =
        tool.starts_with(['.', '/', '~']) || rest.starts_with("./") || rest.starts_with(".\\");
    let plain = tool
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && tool
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    (plain
        || path_like
        || rest.contains("=")
            && rest
                .split_whitespace()
                .next()
                .is_some_and(|w| w.contains('=')))
        && !rest.ends_with('.')
        && !rest.ends_with(':')
}

struct Block {
    lang: String,
    heading: Option<String>,
    first_line: u32,
    lines: Vec<String>,
}

fn blocks(markdown: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut lines = markdown.lines().enumerate().peekable();
    let mut heading: Option<String> = None;
    if markdown.starts_with("---\n") || markdown.starts_with("---\r\n") {
        lines.next();
        for (_, l) in lines.by_ref() {
            if l.trim_end() == "---" || l.trim_end() == "..." {
                break;
            }
        }
    }
    while let Some((index, line)) = lines.next() {
        if let Some(fence) = open_fence(line) {
            let mut content = Vec::new();
            for (_, inner) in lines.by_ref() {
                if closes(&fence, inner) {
                    break;
                }
                content.push(content_line(&fence, inner).to_string());
            }
            out.push(Block {
                lang: fence.lang,
                heading: heading.clone(),
                first_line: index as u32 + 2,
                lines: content,
            });
        } else if let Some(captures) = HEADING.captures(line) {
            let text = captures[2].trim().to_string();
            heading = (!text.is_empty()).then_some(text);
        }
    }
    out
}

pub fn extract(markdown: &str) -> Vec<Snippet> {
    let mut snippets = Vec::new();
    for block in blocks(markdown) {
        let labelled = SHELL_LANGS.contains(&block.lang.as_str());
        if !labelled && !block.lang.is_empty() {
            continue;
        }
        if !labelled {
            let first = block
                .lines
                .iter()
                .find(|l| !l.trim().is_empty() && !is_comment(l, ""));
            if !first.is_some_and(|l| looks_like_command(l)) {
                continue;
            }
        }
        let transcript = block.lines.iter().any(|l| remove_prompt(l).1);
        let lang = block.lang.as_str();
        let mut comment: Vec<String> = Vec::new();
        let mut i = 0;
        while i < block.lines.len() {
            let raw = &block.lines[i];
            let number = block.first_line + i as u32;
            i += 1;
            if raw.trim().is_empty() {
                comment.clear();
                continue;
            }
            if is_comment(raw, lang) {
                comment.push(comment_text(raw));
                continue;
            }
            let (text, prompted) = remove_prompt(raw);
            if text.starts_with('┌') || text.starts_with('╭') {
                continue;
            }
            let unlabelled_prose = lang.is_empty() && !prompted && !looks_like_command(text);
            if (transcript && !prompted)
                || (!prompted && is_output_line(text, lang))
                || unlabelled_prose
            {
                comment.clear();
                continue;
            }
            let mut command = text.trim_end().to_string();
            while let Some(head) = continues(&command, lang) {
                let Some(next) = block.lines.get(i) else {
                    command = head;
                    break;
                };
                let next = remove_prompt(next).0.trim();
                i += 1;
                command = format!("{head} {next}").trim().to_string();
            }
            if let Some(captures) = HEREDOC.captures(&command) {
                let delimiter = captures[1].to_string();
                while let Some(next) = block.lines.get(i) {
                    i += 1;
                    command.push('\n');
                    command.push_str(next);
                    if next.trim() == delimiter {
                        break;
                    }
                }
            }
            let command = command.trim().to_string();
            if command.is_empty() {
                continue;
            }
            let mut placeholders: Vec<String> = Vec::new();
            for p in crate::vars::find(&command) {
                if !placeholders.contains(&p.name) {
                    placeholders.push(p.name);
                }
            }
            snippets.push(Snippet {
                tool: tool_of(&command),
                command,
                lang: block.lang.clone(),
                heading: block.heading.clone(),
                line: number,
                comment: (!comment.is_empty()).then(|| comment.join(" ")),
                placeholders,
            });
            comment.clear();
        }
    }
    snippets
}

pub fn score(snippet: &Snippet, words: &[&str]) -> Option<u32> {
    let tool = snippet.tool.to_lowercase();
    let command = snippet.command.to_lowercase();
    let comment = snippet.comment.as_deref().unwrap_or("").to_lowercase();
    let heading = snippet.heading.as_deref().unwrap_or("").to_lowercase();
    let mut total = 0;
    for word in words
        .iter()
        .map(|w| w.trim().to_lowercase())
        .filter(|w| !w.is_empty())
    {
        let mut points = 0;
        if tool == word {
            points += 50;
        } else if tool.starts_with(&word) {
            points += 30;
        } else if tool.contains(&word) {
            points += 15;
        }
        if let Some(position) = command.find(&word) {
            points += 10;
            let boundary = command[..position]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric());
            if boundary {
                points += 5;
            }
        }
        if comment.contains(&word) {
            points += 8;
        }
        if heading.contains(&word) {
            points += 5;
        }
        if points == 0 {
            return None;
        }
        total += points;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "---\n# frontmatter comment\ntype: note\n---\n# Main title\n\n## Files\n\nText before.\n\n```bash\n# list the files\nls -la /tmp\n\ngrep -r \"pattern\" . \\\n  --include=*.txt \\\n  | sort\nOUT=1 sudo -u admin curl -s https://example.test/$URL\n```\n\n## Network\n\n```console\nuser@host:~$ ssh -p 2222 $USER@$IP\nWelcome to host\nLast login: yesterday\nuser@host:~$ ls\nfile.txt\n```\n\n```\nls /var/log\n```\n\n```text\nthis is not a command\n```\n\n```json\n{\"a\": 1}\n```\n\n```\nExpected result: 3 lines\n```\n";

    fn commands(snippets: &[Snippet]) -> Vec<&str> {
        snippets.iter().map(|s| s.command.as_str()).collect()
    }

    #[test]
    fn extracts_commands_with_context() {
        let snippets = extract(NOTE);
        assert_eq!(
            commands(&snippets),
            [
                "ls -la /tmp",
                "grep -r \"pattern\" . --include=*.txt | sort",
                "OUT=1 sudo -u admin curl -s https://example.test/$URL",
                "ssh -p 2222 $USER@$IP",
                "ls",
                "ls /var/log"
            ]
        );
        let ls = &snippets[0];
        assert_eq!(
            (
                ls.tool.as_str(),
                ls.lang.as_str(),
                ls.heading.as_deref(),
                ls.line
            ),
            ("ls", "bash", Some("Files"), 13)
        );
        assert_eq!(ls.comment.as_deref(), Some("list the files"));
        let grep = &snippets[1];
        assert_eq!(
            (grep.tool.as_str(), grep.line, grep.comment.clone()),
            ("grep", 15, None)
        );
        assert_eq!(snippets[2].tool, "curl");
        assert_eq!(snippets[2].placeholders, ["URL"]);
        let ssh = &snippets[3];
        assert_eq!(
            (ssh.tool.as_str(), ssh.lang.as_str(), ssh.heading.as_deref()),
            ("ssh", "console", Some("Network"))
        );
        assert_eq!(ssh.placeholders, ["USER", "RHOST"]);
        assert_eq!(
            (snippets[5].lang.as_str(), snippets[5].tool.as_str()),
            ("", "ls")
        );
    }

    #[test]
    fn lines_point_at_the_command_in_the_note() {
        let lines: Vec<&str> = NOTE.lines().collect();
        for s in extract(NOTE) {
            let first_word = s.command.split_whitespace().next().unwrap();
            assert!(
                lines[s.line as usize - 1].contains(first_word),
                "{}: line {} = {:?}",
                s.command,
                s.line,
                lines[s.line as usize - 1]
            );
        }
    }

    #[test]
    fn prompts_of_all_shapes_are_removed_and_outputs_ignored() {
        let note = "```console\n$ whoami\nroot\n% date\nmonday\nPS C:\\Users\\me> Get-Date\ntuesday\nC:\\Users\\me> dir\nvolume\n[me@host folder]$ pwd\n/tmp\n┌──(me㉿host)-[~]\n└─$ id\nuid=1000\n```\n";
        let snippets = extract(note);
        assert_eq!(
            commands(&snippets),
            ["whoami", "date", "Get-Date", "dir", "pwd", "id"]
        );
    }

    #[test]
    fn tool_skips_assignments_wrappers_and_paths() {
        for (command, tool) in [
            ("FOO=1 BAR=2 ls", "ls"),
            ("sudo ls", "ls"),
            ("sudo -E -u admin grep x y", "grep"),
            ("time nohup ssh h", "ssh"),
            ("/usr/bin/curl -s x", "curl"),
            ("./run.sh", "run.sh"),
            (".\\outil.exe -a", "outil.exe"),
            ("env LANG=C sort f", "sort"),
            ("'ls' -l", "ls"),
        ] {
            assert_eq!(tool_of(command), tool, "{command}");
        }
    }

    #[test]
    fn continuations_here_documents_and_powershell() {
        let note = "```sh\ncat > f.txt <<EOF\nline one\nline two\nEOF\nls &&\n  pwd\n```\n\n```powershell\nGet-ChildItem `\n  -Recurse `\n  -Filter *.log\nWrite-Output \"ok\"\n```\n\n```cmd\nREM list\ndir ^\n  /b\n```\n";
        let snippets = extract(note);
        assert_eq!(
            commands(&snippets),
            [
                "cat > f.txt <<EOF\nline one\nline two\nEOF",
                "ls && pwd",
                "Get-ChildItem -Recurse -Filter *.log",
                "Write-Output \"ok\"",
                "dir /b"
            ]
        );
        assert_eq!(snippets[4].comment.as_deref(), Some("list"));
        assert_eq!(snippets[0].tool, "cat");
    }

    #[test]
    fn quoted_indented_and_tilde_fences() {
        let note = "- point\n\n  ```bash\n  ls -l\n  ```\n\n> ```bash\n> pwd\n> ```\n\n~~~sh\nuname -a\n~~~\n\n````bash\necho ```\n````\n";
        assert_eq!(
            commands(&extract(note)),
            ["ls -l", "pwd", "uname -a", "echo ```"]
        );
    }

    #[test]
    fn pasted_tool_output_in_shell_blocks_is_not_a_command() {
        let note = "```powershell\nGet-Date\nLogon Time        : 4/13/2024 2:46:35 AM\nmsv :\n * Password : (null)\n[00000003] Primary\n-----------------\n4586/tcp  filtered unknown\ndes_cbc_md4       9e0105c4c07740e3986f082ef94cb767\n[Net.ServicePointManager]::SecurityProtocol = 'Tls12'\nWrite-Output ok\n```\n";
        assert_eq!(
            commands(&extract(note)),
            [
                "Get-Date",
                "[Net.ServicePointManager]::SecurityProtocol = 'Tls12'",
                "Write-Output ok"
            ]
        );
    }

    #[test]
    fn unlabelled_blocks_need_a_command_looking_first_line() {
        for (block, expected) in [
            ("ls -la\n", true),
            ("$ ls\n", true),
            ("./run.sh x\n", true),
            ("curl -s http://example.test\n", true),
            ("# commentaire\ngrep a b\n", true),
            ("import os\nprint(os)\n", false),
            ("Expected result: ok\n", false),
            ("key: value\n", false),
            ("{ \"a\": 1 }\n", false),
            ("- list\n", false),
            ("select * from t\n", false),
            ("Just a sentence.\n", false),
        ] {
            let note = format!("```\n{block}```\n");
            assert_eq!(!extract(&note).is_empty(), expected, "{block:?}");
        }
    }

    #[test]
    fn score_requires_every_word_and_ranks_the_tool_first() {
        let snippets = extract(NOTE);
        let ls = &snippets[0];
        let grep = &snippets[1];
        assert_eq!(score(ls, &[]), Some(0));
        assert!(score(ls, &["ls"]) > score(ls, &["tmp"]));
        assert!(score(ls, &["LS", "tmp"]).is_some());
        assert_eq!(score(ls, &["absent"]), None);
        assert_eq!(score(ls, &["ls", "absent"]), None);
        assert!(score(ls, &["files"]).is_some(), "heading");
        assert!(score(ls, &["list"]).is_some(), "comment");
        assert!(score(grep, &["gre"]).unwrap() > score(grep, &["include"]).unwrap());
        let mut ranked = snippets
            .iter()
            .filter_map(|s| score(s, &["ls"]).map(|p| (p, s.line)))
            .collect::<Vec<_>>();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        assert_eq!(ranked.first().map(|r| r.1), Some(13));
    }

    #[test]
    fn real_notes_do_not_panic_and_yield_commands() {
        let Some(root) = std::env::var_os("FOCUS_REAL_VAULT").map(std::path::PathBuf::from) else {
            return;
        };
        if !root.is_dir() {
            return;
        }
        let mut stack = vec![root];
        let (mut notes, mut total) = (0usize, 0usize);
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                stack.extend(
                    std::fs::read_dir(&path)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(|e| e.path()),
                );
            } else if path.extension().is_some_and(|e| e == "md")
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                notes += 1;
                let found = extract(&text);
                assert!(
                    found
                        .iter()
                        .all(|s| !s.command.trim().is_empty() && s.line >= 1)
                );
                total += found.len();
            }
        }
        assert!(notes > 50 && total > 200, "notes {notes}, commands {total}");
        println!("{notes} notes, {total} commands extracted");
    }
}
