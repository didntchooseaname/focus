use focus_md::highlight::{Span, Token, canonical, detect, highlight, label, languages};

fn tokens<'a>(code: &'a str, lang: &str) -> Vec<(Token, &'a str)> {
    highlight(code, lang)
        .into_iter()
        .map(|Span { range, token }| (token, &code[range]))
        .collect()
}

fn has(code: &str, lang: &str, token: Token, text: &str) -> bool {
    tokens(code, lang).contains(&(token, text))
}

#[test]
fn aliases_and_labels() {
    assert_eq!(canonical("sh"), Some("bash"));
    assert_eq!(canonical("ZSH"), Some("bash"));
    assert_eq!(canonical("shell"), Some("bash"));
    assert_eq!(canonical("ps1"), Some("powershell"));
    assert_eq!(canonical("c++"), Some("cpp"));
    assert_eq!(canonical("shell-session"), Some("console"));
    assert_eq!(canonical("plaintext"), Some("text"));
    assert_eq!(canonical("brainfuck"), None);
    assert_eq!(label("ps1"), Some("PowerShell"));
    assert_eq!(label("nope"), None);
    assert!(languages().iter().all(|l| canonical(l.id) == Some(l.id)));
    assert_eq!(languages().len(), 30);
}

#[test]
fn bash_tokens() {
    let code = "sudo nmap -sV --min-rate=1000 \"$HOST\" | tee out.log # fin\nexport A=1\nif [ -f x ]; then echo ok; fi\n";
    assert!(has(code, "bash", Token::Function, "sudo"));
    assert!(has(code, "bash", Token::Function, "nmap"));
    assert!(has(code, "bash", Token::Flag, "-sV"));
    assert!(has(code, "bash", Token::Flag, "--min-rate"));
    assert!(has(code, "bash", Token::Variable, "$HOST"));
    assert!(has(code, "bash", Token::Operator, "|"));
    assert!(has(code, "bash", Token::Function, "tee"));
    assert!(has(code, "bash", Token::Comment, "# fin"));
    assert!(has(code, "bash", Token::Keyword, "if"));
    assert!(has(code, "bash", Token::Keyword, "fi"));
    assert!(has(code, "bash", Token::Punctuation, "["));
}

#[test]
fn bash_heredoc_and_substitution() {
    let code = "cat <<EOF\nligne $X\nEOF\necho $(id -u)\n";
    assert!(has(code, "bash", Token::Constant, "EOF"));
    assert!(
        tokens(code, "bash")
            .iter()
            .any(|(t, s)| *t == Token::String && s.contains("ligne"))
    );
    assert!(has(code, "bash", Token::Function, "id"));
    assert!(has(code, "bash", Token::Flag, "-u"));
}

#[test]
fn powershell_tokens() {
    let code = "Get-ChildItem -Path C:\\ | Where-Object { $_.Length -gt 1MB }\n$x = [string]$env:USERNAME\n<# note #>\n";
    assert!(has(code, "powershell", Token::Function, "Get-ChildItem"));
    assert!(has(code, "powershell", Token::Flag, "-Path"));
    assert!(has(code, "powershell", Token::Operator, "-gt"));
    assert!(has(code, "powershell", Token::Variable, "$_"));
    assert!(has(code, "powershell", Token::Type, "string"));
    assert!(has(code, "powershell", Token::Variable, "$env:USERNAME"));
    assert!(has(code, "powershell", Token::Comment, "<# note #>"));
}

#[test]
fn cmd_tokens() {
    let code =
        "@echo off\nrem commentaire\nset X=1\nif exist %TEMP%\\a.txt del /q %TEMP%\\a.txt\n:fin\n";
    assert!(has(code, "cmd", Token::Comment, "rem commentaire"));
    assert!(has(code, "cmd", Token::Keyword, "set"));
    assert!(has(code, "cmd", Token::Keyword, "exist"));
    assert!(has(code, "cmd", Token::Variable, "%TEMP%"));
    assert!(has(code, "cmd", Token::Flag, "/q"));
    assert!(has(code, "cmd", Token::Constant, ":fin"));
}

#[test]
fn console_prompts_commands_options() {
    let code = "$ ls -la /tmp\nfichier.txt\nroot@kali:~# whoami\nPS C:\\Users\\a> Get-Process -Name x\nC:\\Windows>dir /w\nmeterpreter > upload a.exe\nmimikatz # privilege::debug\n";
    let t = tokens(code, "console");
    assert!(t.contains(&(Token::Prompt, "$")));
    assert!(t.contains(&(Token::Function, "ls")));
    assert!(t.contains(&(Token::Flag, "-la")));
    assert!(t.contains(&(Token::Prompt, "root@kali:~#")));
    assert!(t.contains(&(Token::Function, "whoami")));
    assert!(t.contains(&(Token::Prompt, "PS C:\\Users\\a>")));
    assert!(t.contains(&(Token::Function, "Get-Process")));
    assert!(t.contains(&(Token::Prompt, "C:\\Windows>")));
    assert!(t.contains(&(Token::Prompt, "meterpreter >")));
    assert!(t.contains(&(Token::Prompt, "mimikatz #")));
    assert!(
        !t.iter().any(|(_, s)| s.contains("fichier.txt")),
        "la sortie n'est pas colorée"
    );
}

#[test]
fn python_tokens() {
    let code = "def scan(host: str):\n    \"\"\"doc\"\"\"\n    return f\"{host}\" if host is None else 0  # fin\nclass A(B): pass\n";
    assert!(has(code, "python", Token::Keyword, "def"));
    assert!(has(code, "python", Token::Function, "scan"));
    assert!(has(code, "python", Token::Type, "str"));
    assert!(has(code, "python", Token::String, "\"\"\"doc\"\"\""));
    assert!(has(code, "python", Token::String, "f\"{host}\""));
    assert!(has(code, "python", Token::Constant, "None"));
    assert!(has(code, "python", Token::Comment, "# fin"));
    assert!(has(code, "python", Token::Type, "A"));
}

#[test]
fn c_family_tokens() {
    let c = "#include <stdio.h>\nint main(void) { printf(\"%d\\n\", 0x1F); return 0; } // fin\n";
    assert!(has(c, "c", Token::Keyword, "#include"));
    assert!(has(c, "c", Token::String, "<stdio.h>"));
    assert!(has(c, "c", Token::Type, "int"));
    assert!(has(c, "c", Token::Function, "main"));
    assert!(has(c, "c", Token::Function, "printf"));
    assert!(has(c, "c", Token::Number, "0x1F"));
    assert!(has(c, "c", Token::Comment, "// fin"));
    let rust = "fn main() { let x: &'a str = \"a\"; println!(\"{}\", x); }";
    assert!(has(rust, "rust", Token::Keyword, "fn"));
    assert!(has(rust, "rust", Token::Function, "println"));
    assert!(has(rust, "rust", Token::Type, "'a"));
    assert!(has(rust, "rust", Token::Type, "str"));
    let js = "const f = (a) => `x${a}`; // c\nfunction g() { return null; }";
    assert!(has(js, "javascript", Token::Keyword, "const"));
    assert!(has(js, "javascript", Token::String, "`x${a}`"));
    assert!(has(js, "javascript", Token::Function, "g"));
    assert!(has(js, "javascript", Token::Constant, "null"));
}

#[test]
fn sql_php_ruby_go() {
    assert!(has(
        "SELECT name FROM users WHERE id = 1 -- fin",
        "sql",
        Token::Keyword,
        "SELECT"
    ));
    assert!(has(
        "select 'it''s' from t",
        "sql",
        Token::String,
        "'it''s'"
    ));
    assert!(has(
        "<?php echo $_GET['a']; ?>",
        "php",
        Token::Variable,
        "$_GET"
    ));
    assert!(has("<?php echo 1; ?>", "php", Token::Keyword, "<?php"));
    assert!(has(
        "def run\n  puts @x\nend",
        "ruby",
        Token::Variable,
        "@x"
    ));
    assert!(has(
        "func main() {\n fmt.Println(\"a\")\n}",
        "go",
        Token::Function,
        "Println"
    ));
}

#[test]
fn data_formats() {
    let json = "{\"a\": [1, true, null], \"b\": \"x\"}";
    assert!(has(json, "json", Token::Property, "\"a\""));
    assert!(has(json, "json", Token::String, "\"x\""));
    assert!(has(json, "json", Token::Number, "1"));
    assert!(has(json, "json", Token::Constant, "true"));
    let yaml = "# c\nname: web\nports:\n  - 80\nok: true\ntext: |\n  bloc\n";
    assert!(has(yaml, "yaml", Token::Property, "name"));
    assert!(has(yaml, "yaml", Token::Number, "80"));
    assert!(has(yaml, "yaml", Token::Constant, "true"));
    assert!(has(yaml, "yaml", Token::Comment, "# c"));
    let toml = "[package]\nname = \"focus\"\nn = 3 # fin\n";
    assert!(has(toml, "toml", Token::Heading, "[package]"));
    assert!(has(toml, "toml", Token::Property, "name"));
    assert!(has(toml, "toml", Token::String, "\"focus\""));
    let ini = "; c\n[s]\nk=v\n";
    assert!(has(ini, "ini", Token::Heading, "[s]"));
    assert!(has(ini, "ini", Token::Property, "k"));
}

#[test]
fn markup_css_diff_http_asm_docker() {
    let html = "<div class=\"a\" id=x>t &amp; u</div><!-- c -->";
    assert!(has(html, "html", Token::Tag, "div"));
    assert!(has(html, "html", Token::Attribute, "class"));
    assert!(has(html, "html", Token::String, "\"a\""));
    assert!(has(html, "html", Token::Constant, "&amp;"));
    assert!(has(html, "html", Token::Comment, "<!-- c -->"));
    let embedded = "<script>var a = 1;</script>";
    assert!(has(embedded, "html", Token::Keyword, "var"));
    let css = "a.b { color: #fff; margin: 0 10px !important }";
    assert!(has(css, "css", Token::Property, "color"));
    assert!(has(css, "css", Token::Number, "#fff"));
    assert!(has(css, "css", Token::Keyword, "!important"));
    let diff = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-ancien\n+nouveau\n contexte\n";
    assert!(has(diff, "diff", Token::Heading, "@@ -1 +1 @@"));
    assert!(has(diff, "diff", Token::Deleted, "-ancien"));
    assert!(has(diff, "diff", Token::Inserted, "+nouveau"));
    let http = "POST /login HTTP/1.1\nHost: a.b\n\nuser=a&pass=b";
    assert!(has(http, "http", Token::Keyword, "POST"));
    assert!(has(http, "http", Token::Property, "Host"));
    assert!(has(http, "http", Token::Property, "user"));
    let asm = "_start:\n  mov eax, 0x1 ; fin\n  int 0x80\n";
    assert!(has(asm, "asm", Token::Function, "_start"));
    assert!(has(asm, "asm", Token::Keyword, "mov"));
    assert!(has(asm, "asm", Token::Variable, "eax"));
    assert!(has(asm, "asm", Token::Comment, "; fin"));
    let docker = "FROM rust:1 AS build\nRUN apt-get update && apt-get install -y gcc\nCOPY --from=build /a /b\n";
    assert!(has(docker, "dockerfile", Token::Keyword, "FROM"));
    assert!(has(docker, "dockerfile", Token::Keyword, "AS"));
    assert!(has(docker, "dockerfile", Token::Function, "apt-get"));
    assert!(has(docker, "dockerfile", Token::Flag, "--from"));
}

#[test]
fn unknown_and_text_have_no_spans() {
    assert!(highlight("fn main() {}", "brainfuck").is_empty());
    assert!(highlight("fn main() {}", "text").is_empty());
    assert!(highlight("", "bash").is_empty());
}

#[test]
fn spans_are_sorted_disjoint_and_on_char_boundaries() {
    let samples = [
        "é$(écho \"à\")\n# ü",
        "日本語 = \"値\" // 注釈\n",
        "<p>é&eacute;</p>",
        "\u{1F600} 'x",
        "\"",
        "'",
        "`",
        "$(",
        "${",
        "<<",
        "<<EOF\n",
        "@\"",
        "<#",
        "/*",
        "--[[",
        "'''",
        "r#\"",
        "[[",
        "<!--",
        "{",
        "\r\n\r\n$ a\r\n",
    ];
    for lang in languages() {
        for sample in samples {
            let spans = highlight(sample, lang.id);
            let mut end = 0;
            for span in &spans {
                assert!(span.range.start >= end, "{} {sample:?}", lang.id);
                assert!(span.range.start < span.range.end && span.range.end <= sample.len());
                assert!(
                    sample.is_char_boundary(span.range.start)
                        && sample.is_char_boundary(span.range.end),
                    "{} {sample:?}",
                    lang.id
                );
                end = span.range.end;
            }
        }
    }
}

#[test]
fn detect_samples() {
    let cases: [(&str, &str); 24] = [
        (
            "#!/bin/bash\nfor i in $(seq 1 3); do\n  echo $i\ndone\n",
            "bash",
        ),
        (
            "Get-ChildItem -Path C:\\ | Where-Object { $_.Length -gt 1MB }\n",
            "powershell",
        ),
        ("@echo off\nset X=1\nif exist %X% goto fin\n:fin\n", "cmd"),
        (
            "def scan(host):\n    import socket\n    return socket.socket()\n",
            "python",
        ),
        (
            "#include <stdio.h>\nint main(void) {\n  printf(\"x\");\n  return 0;\n}\n",
            "c",
        ),
        (
            "#include <iostream>\nint main() { std::cout << 1; }\n",
            "cpp",
        ),
        (
            "using System;\nclass A { static void Main(string[] args) { Console.WriteLine(1); } }\n",
            "csharp",
        ),
        (
            "import java.util.List;\npublic class A { public static void main(String[] a) { System.out.println(1); } }\n",
            "java",
        ),
        (
            "const a = require('x');\nconsole.log(a === 1);\n",
            "javascript",
        ),
        (
            "interface A { x: string; }\nconst a: number = 1;\nconsole.log(a);\n",
            "typescript",
        ),
        (
            "package main\nimport \"fmt\"\nfunc main() {\n  x := 1\n  fmt.Println(x)\n}\n",
            "go",
        ),
        (
            "fn main() {\n    let mut x = 1;\n    println!(\"{}\", x);\n}\n",
            "rust",
        ),
        ("<?php\n$a = $_GET['x'];\necho $a;\n?>", "php"),
        ("require 'json'\ndef run\n  puts 1\nend\n", "ruby"),
        (
            "SELECT id, name FROM users WHERE id = 1 UNION SELECT 1, 2 FROM dual;",
            "sql",
        ),
        ("{\"a\": 1, \"b\": [true, null]}", "json"),
        ("---\nname: web\nports:\n  - 80\nimage: nginx\n", "yaml"),
        ("[package]\nname = \"focus\"\nversion = \"0.1.0\"\n", "toml"),
        ("<?xml version=\"1.0\"?>\n<a><b/></a>", "xml"),
        (
            "<!DOCTYPE html>\n<html><body><p>x</p></body></html>",
            "html",
        ),
        (
            "FROM ubuntu:22.04\nRUN apt-get update\nCMD [\"bash\"]\n",
            "dockerfile",
        ),
        ("GET /index.php HTTP/1.1\nHost: a.b\n\n", "http"),
        (
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n",
            "diff",
        ),
        (
            "$ nmap -sV 10.0.0.1\nStarting Nmap\nroot@kali:~# id\nuid=0\n",
            "console",
        ),
    ];
    for (code, expected) in cases {
        let found = detect(code).unwrap_or_else(|| panic!("aucune détection pour {expected}"));
        assert_eq!(found.lang, expected, "{code}");
        assert!(found.confidence >= 0.5);
    }
}

#[test]
fn detect_declines_ambiguous_text() {
    for code in [
        "",
        "  ",
        "ok",
        "Voici une phrase en français sans code.",
        "12 34",
        "a b",
    ] {
        assert!(detect(code).is_none(), "{code:?} : {:?}", detect(code));
    }
}

#[test]
fn detect_single_known_commands() {
    use focus_md::highlight::Detected;
    let lang = |code: &str| detect(code).map(|d: Detected| d.lang);
    assert_eq!(lang("nmap -sV 10.0.0.1"), Some("bash"));
    assert_eq!(lang("cd impacket"), Some("bash"));
    assert_eq!(lang("sudo apt-get install -y gcc"), Some("bash"));
    assert_eq!(lang("KEY=1 curl -s http://a.b"), Some("bash"));
    assert_eq!(lang("reg query HKLM\\Software\\X /v Y"), Some("cmd"));
    assert_eq!(lang("Rubeus.exe asktgt /user:x /ptt"), Some("cmd"));
    assert_eq!(lang("Get-Process | Select-Object Name"), Some("powershell"));
    assert_eq!(lang("$env:USERNAME -eq 'a'"), Some("powershell"));
    assert_eq!(
        lang("echo '<?php system($_GET[0]); ?>' > shell.php"),
        Some("bash")
    );
}

#[test]
fn detect_in_note_prefers_the_language_of_the_note() {
    use focus_md::highlight::{Hints, detect_in_note};
    let code = "echo hello world";
    let windows = Hints {
        languages: &["cmd", "bat", "cmd"],
        path: None,
    };
    let linux = Hints {
        languages: &["bash", "sh"],
        path: None,
    };
    assert_ne!(detect_in_note(code, &windows).map(|d| d.lang), Some("bash"));
    assert_eq!(detect_in_note(code, &linux).map(|d| d.lang), Some("bash"));
    let by_path = Hints {
        languages: &[],
        path: Some("Red Team/Windows/Notes.md"),
    };
    assert_eq!(
        detect_in_note("Get-Date", &by_path).map(|d| d.lang),
        Some("powershell")
    );
    assert_eq!(detect(code), detect_in_note(code, &Hints::default()));
}
