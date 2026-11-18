use std::collections::HashMap;
use std::net::Ipv6Addr;
use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, Clone)]
pub struct Options {
    pub secrets: bool,
    pub ip_addresses: bool,
    pub hostnames: bool,
}

#[derive(Debug, Clone)]
pub struct Replacement {
    pub token: String,
    pub original: String,
    pub kind: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct Redacted {
    pub text: String,
    pub replacements: Vec<Replacement>,
}

const OPEN: char = '‹';
const CLOSE: char = '›';
const MIN_KNOWN_LEN: usize = 4;

type Filter = fn(&str, &regex::Match) -> bool;

struct Rule {
    kind: &'static str,
    regex: Regex,
    groups: &'static [usize],
    filter: Option<Filter>,
}

fn rule(
    kind: &'static str,
    pattern: &str,
    groups: &'static [usize],
    filter: Option<Filter>,
) -> Rule {
    Rule {
        kind,
        regex: Regex::new(pattern).expect("valid redaction pattern"),
        groups,
        filter,
    }
}

fn hex_hash(text: &str, m: &regex::Match) -> bool {
    let value = m.as_str();
    if !(value.bytes().any(|b| b.is_ascii_digit())
        && value.bytes().any(|b| b.is_ascii_alphabetic()))
    {
        return false;
    }
    let after = &text[m.end()..];
    let ssh_fingerprint = after.starts_with(" (")
        && ["(RSA)", "(ED25519)", "(ECDSA)", "(DSA)"]
            .iter()
            .any(|k| after[1..].starts_with(k));
    let path_component = text[..m.start()].ends_with(['/', '\\']);
    !ssh_fingerprint && !path_component
}

fn not_port_list(_: &str, m: &regex::Match) -> bool {
    let value = m.as_str();
    !value.starts_with('-')
        && !value
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, ',' | '-' | ':' | 'T' | 'U' | 'S'))
}

fn not_flag(_: &str, m: &regex::Match) -> bool {
    !m.as_str().starts_with('/') && !m.as_str().starts_with('-')
}

fn real_value(_: &str, m: &regex::Match) -> bool {
    let value = m.as_str().trim();
    let lower = value.to_ascii_lowercase();
    value.chars().count() >= 3
        && value.chars().any(char::is_alphanumeric)
        && !value.contains(OPEN)
        && !value.starts_with([
            '$', '<', '%', '^', '(', '[', '{', '/', ':', '`', '\\', '.', '*', '+',
        ])
        && !value.starts_with("{{")
        && !matches!(
            lower.as_str(),
            "null"
                | "none"
                | "(null)"
                | "required"
                | "true"
                | "false"
                | "empty"
                | "hidden"
                | "redacted"
        )
        && !matches!(
            lower.as_str(),
            "password"
                | "passwd"
                | "pass"
                | "pwd"
                | "secret"
                | "token"
                | "motdepasse"
                | "mdp"
                | "changeme"
                | "yourpassword"
        )
        && !is_cmdlet(value)
}

fn is_cmdlet(value: &str) -> bool {
    let mut parts = value.splitn(2, '-');
    match (parts.next(), parts.next()) {
        (Some(verb), Some(noun)) => {
            let capitalised = |w: &str| {
                w.chars().next().is_some_and(char::is_uppercase)
                    && w.chars().all(char::is_alphabetic)
            };
            capitalised(verb) && capitalised(noun)
        }
        _ => false,
    }
}

fn token_value(text: &str, m: &regex::Match) -> bool {
    real_value(text, m)
        && !m.as_str().starts_with("0x")
        && m.as_str().len() >= 8
        && m.as_str().bytes().any(|b| b.is_ascii_digit())
        && !m.as_str().contains(['(', ')'])
}

static SECRET_RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    vec![
        rule(
            "private_key",
            r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----[\s\S]*?-----END (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----",
            &[0],
            None,
        ),
        rule(
            "private_key",
            r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----\r?\n(?:[A-Za-z-]+:[^\n]*\n)*(?:\r?\n)?(?:[A-Za-z0-9+/=]{16,}\r?\n)*[A-Za-z0-9+/=]{4,}",
            &[0],
            None,
        ),
        rule(
            "jwt",
            r"\beyJ[A-Za-z0-9_-]{5,}\.eyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]*",
            &[0],
            None,
        ),
        rule(
            "aws_key",
            r"\b(?:AKIA|ASIA|AGPA|AIDA|AROA|ANPA|ANVA|AIPA)[A-Z0-9]{16}\b",
            &[0],
            None,
        ),
        rule(
            "aws_secret",
            r#"(?i)aws_?secret_?access_?key["']?[ \t]*[=:][ \t]*["']?([A-Za-z0-9/+=]{40})\b"#,
            &[1],
            None,
        ),
        rule(
            "github_token",
            r"\b(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{22,})\b",
            &[0],
            None,
        ),
        rule("gitlab_token", r"\bglpat-[A-Za-z0-9_-]{20,}", &[0], None),
        rule(
            "api_key",
            r"\b(?:xox[abprs]-[A-Za-z0-9-]{10,}|sk_live_[A-Za-z0-9]{16,}|rk_live_[A-Za-z0-9]{16,}|AIza[0-9A-Za-z_-]{35}|sk-[A-Za-z0-9_-]{20,})",
            &[0],
            None,
        ),
        rule(
            "api_key",
            r"\b(?:discord(?:app)?\.com/api/webhooks/\d+|hooks\.slack\.com/services/[A-Z0-9]+/[A-Z0-9]+)/([A-Za-z0-9_-]{20,})",
            &[1],
            None,
        ),
        rule(
            "kerberos",
            r"\$krb5(?:tgs|asrep|pa|asreq)\$[^\s]+",
            &[0],
            None,
        ),
        rule(
            "ntlm",
            r"\b[^\s:]+::[^\s:]+:[A-Fa-f0-9]{16}:[A-Fa-f0-9]{32}:[A-Fa-f0-9]+",
            &[0],
            None,
        ),
        rule("ntlm", r"\b[A-Fa-f0-9]{32}:[A-Fa-f0-9]{32}\b", &[0], None),
        rule(
            "hash",
            r"\$(?:1|2[abxy]?|5|6|y|7|apr1|sha1|P|H|argon2(?:id|i|d))\$[./A-Za-z0-9$,=+-]{8,}",
            &[0],
            None,
        ),
        rule(
            "password",
            r#"(?i)\bauthorization[ \t]*:[ \t]*(?:bearer|basic|token)[ \t]+([A-Za-z0-9._~+/=-]{8,})"#,
            &[1],
            Some(real_value),
        ),
        rule(
            "password",
            r"\b[a-z][a-z0-9+.-]*://[^\s:/@]+:([^\s@/]+)@",
            &[1],
            Some(real_value),
        ),
        rule(
            "password",
            r#"(?i)\bConvertTo-SecureString[ \t]+(?:-String[ \t]+)?(?:"([^"\n]+)"|'([^'\n]+)')"#,
            &[1, 2],
            None,
        ),
        rule(
            "password",
            r#"(?i)\b(?:[a-z0-9_]*(?:password|passwd|passphrase|pwd)|pass|motdepasse|mdp)\b["']?[ \t]*[=:][ \t]*(?:"([^"\n]*)"|'([^'\n]*)')"#,
            &[1, 2],
            Some(real_value),
        ),
        rule(
            "password",
            r#"(?i)\b(?:[a-z0-9_]*(?:password|passwd|passphrase|pwd)|pass|motdepasse|mdp)\b["']?[ \t]*[=:][ \t]*([^\s"'&;,]+)"#,
            &[1],
            Some(real_value),
        ),
        rule(
            "api_key",
            r#"(?i)\b[a-z0-9_]*(?:secret|token|api[_-]?key|apikey|access[_-]?key|private[_-]?key|client[_-]?secret)\b["']?[ \t]*[=:][ \t]*(?:"([^"\n]*)"|'([^'\n]*)')"#,
            &[1, 2],
            Some(real_value),
        ),
        rule(
            "api_key",
            r#"(?i)\b[a-z0-9_]*(?:secret|token|api[_-]?key|apikey|access[_-]?key|private[_-]?key|client[_-]?secret)\b["']?[ \t]*[=:][ \t]*([^\s"'&;,]+)"#,
            &[1],
            Some(token_value),
        ),
        rule(
            "password",
            r#"(?i)(?:^|\s)--?(?:password|passwd|pass|pwd)(?:=|[ \t]+)(?:"([^"\n]+)"|'([^'\n]+)'|([^\s"']+))"#,
            &[1, 2, 3],
            Some(|t, m| not_flag(t, m) && real_value(t, m)),
        ),
        rule(
            "password",
            r#"(?i:\b(?:sshpass|hydra|medusa|ncrack|smbmap|crackmapexec|cme|netexec|nxc|evil-winrm|rdesktop|bloodhound-python|rusthound|ldapdomaindump|kerbrute|windapsearch|enum4linux(?:-ng)?|rpcclient)\b)[^\n|;&]*?[ \t]-p[ \t]*(?:"([^"\n]+)"|'([^'\n]+)'|([^\s"']+))"#,
            &[1, 2, 3],
            Some(|t, m| not_port_list(t, m) && real_value(t, m)),
        ),
        rule(
            "password",
            r#"(?i:\bmysql(?:dump|admin)?\b)[^\n|;&]*?[ \t]-p(?:"([^"\n]+)"|'([^'\n]+)'|([^\s"'-]\S*))"#,
            &[1, 2, 3],
            Some(real_value),
        ),
        rule(
            "password",
            r"(?i)(?:^|\s)/(?:p|pass|password):(\S+)",
            &[1],
            Some(real_value),
        ),
        rule(
            "password",
            r"(?m)^[ \t]*\*[ \t]*Password[ \t]*:[ \t]+(\S[^\n]*?)[ \t]*$",
            &[1],
            Some(|_, m| m.as_str() != "(null)" && !m.as_str().contains(OPEN)),
        ),
        rule("password", r"(?:^|\s)-p:(\S+)", &[1], Some(real_value)),
        rule(
            "password",
            r#"\b[\w.-]+/[\w.$-]+:([^\s@'"/]+)@[\w.-]+"#,
            &[1],
            Some(real_value),
        ),
        rule(
            "password",
            r#"\b[\w.$-]+:([^\s@:'"/]{3,})@(?:\d{1,3}\.){3}\d{1,3}\b"#,
            &[1],
            Some(real_value),
        ),
        rule(
            "password",
            r"(?i)\bnet[ \t]+user[ \t]+[^\s/]+[ \t]+([^\s/]+)",
            &[1],
            Some(not_flag),
        ),
        rule(
            "hash",
            r"\b(?:[A-Fa-f0-9]{128}|[A-Fa-f0-9]{64}|[A-Fa-f0-9]{40}|[A-Fa-f0-9]{32})\b",
            &[0],
            Some(hex_hash),
        ),
    ]
});

static IPV4: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\b",
    )
    .expect("IPv4 pattern")
});
static IPV6_CANDIDATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[0-9A-Fa-f]{0,4}(?::[0-9A-Fa-f]{0,4}){2,7}").expect("IPv6 pattern")
});
static FQDN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+([a-z]{2,24})\b")
        .expect("hostname pattern")
});
static UNC_HOST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\\\([A-Za-z0-9][A-Za-z0-9._-]*)\\").expect("UNC pattern"));

const HOST_TLDS: &[&str] = &[
    "local",
    "lan",
    "internal",
    "corp",
    "home",
    "htb",
    "thm",
    "vl",
    "intranet",
    "localdomain",
    "test",
    "lab",
    "priv",
    "private",
    "ad",
    "domain",
    "ctf",
    "box",
    "dev",
    "int",
    "com",
    "net",
    "org",
    "io",
    "fr",
    "de",
    "uk",
    "eu",
    "us",
    "info",
    "biz",
    "xyz",
    "co",
    "me",
    "tv",
    "app",
    "cloud",
    "online",
    "site",
    "tech",
    "pro",
    "edu",
    "gov",
    "mil",
    "nl",
    "be",
    "ch",
    "es",
    "it",
    "ru",
    "cn",
    "jp",
    "ca",
    "au",
    "pw",
    "cc",
    "to",
    "sh",
    "ai",
    "gg",
];

const PUBLIC_DOMAINS: &[&str] = &[
    "github.com",
    "githubusercontent.com",
    "gitlab.com",
    "microsoft.com",
    "google.com",
    "mitre.org",
    "nist.gov",
    "exploit-db.com",
    "hackthebox.com",
    "tryhackme.com",
    "wikipedia.org",
    "kernel.org",
    "nmap.org",
    "gmail.com",
    "youtube.com",
    "twitter.com",
    "linkedin.com",
    "stackoverflow.com",
    "medium.com",
    "php.net",
    "example.com",
    "example.org",
    "offsec.com",
    "root-me.org",
    "hacktricks.xyz",
    "revshells.com",
    "gentilkiwi.com",
    "pingcastle.com",
    "reddit.com",
    "amazon.com",
    "amazonaws.com",
    "azure.com",
    "windows.net",
    "cloudflare.com",
    "python.org",
    "mozilla.org",
    "ubuntu.com",
    "apache.org",
    "wordpress.org",
    "wordpress.com",
    "docker.com",
    "npmjs.com",
    "pypi.org",
    "sans.org",
    "portswigger.net",
    "gtfobins.github.io",
    "debian.org",
    "archlinux.org",
    "crates.io",
    "rust-lang.org",
    "deepseek.com",
    "owasp.org",
    "ietf.org",
    "w3.org",
    "docs.rs",
];

fn complex(value: &str) -> bool {
    let classes = [
        value.chars().any(char::is_lowercase),
        value.chars().any(char::is_uppercase),
        value.chars().any(|c| c.is_ascii_digit()),
        value.chars().any(|c| !c.is_alphanumeric()),
    ];
    value.chars().count() >= 6 && classes.iter().filter(|&&c| c).count() >= 2
}

struct Candidate {
    start: usize,
    end: usize,
    kind: &'static str,
    prefix: &'static str,
    priority: usize,
}

fn resolve_overlaps(candidates: &mut [Candidate]) -> Vec<Candidate> {
    candidates.sort_by_key(|c| (c.start, std::cmp::Reverse(c.end - c.start), c.priority));
    let mut selected: Vec<Candidate> = Vec::new();
    let mut covered_to = 0usize;
    for c in candidates.iter() {
        if c.start >= covered_to {
            covered_to = c.end;
            selected.push(Candidate {
                start: c.start,
                end: c.end,
                kind: c.kind,
                prefix: c.prefix,
                priority: c.priority,
            });
        }
    }
    selected
}

pub fn redact(text: &str, options: &Options, known: &[Replacement]) -> Redacted {
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut priority = 0usize;
    if options.secrets {
        for rule in SECRET_RULES.iter() {
            priority += 1;
            for captures in rule.regex.captures_iter(text) {
                for &group in rule.groups {
                    let Some(m) = captures.get(group) else {
                        continue;
                    };
                    if m.as_str().is_empty() || rule.filter.is_some_and(|f| !f(text, &m)) {
                        continue;
                    }
                    candidates.push(Candidate {
                        start: m.start(),
                        end: m.end(),
                        kind: rule.kind,
                        prefix: "SECRET",
                        priority,
                    });
                }
            }
        }
    }
    if options.ip_addresses {
        priority += 1;
        for m in IPV4.find_iter(text) {
            if !matches!(m.as_str(), "0.0.0.0" | "255.255.255.255")
                && !m.as_str().starts_with("127.")
            {
                candidates.push(Candidate {
                    start: m.start(),
                    end: m.end(),
                    kind: "ip",
                    prefix: "IP",
                    priority,
                });
            }
        }
        for m in IPV6_CANDIDATE.find_iter(text) {
            let before = text[..m.start()].chars().next_back();
            let after = text[m.end()..].chars().next();
            let bounded = !before
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == ':' || c == '.')
                && !after.is_some_and(|c| c.is_ascii_alphanumeric() || c == ':');
            if bounded
                && m.as_str()
                    .parse::<Ipv6Addr>()
                    .is_ok_and(|a| !a.is_loopback() && !a.is_unspecified())
            {
                candidates.push(Candidate {
                    start: m.start(),
                    end: m.end(),
                    kind: "ip",
                    prefix: "IP",
                    priority,
                });
            }
        }
    }
    if options.hostnames {
        priority += 1;
        for captures in FQDN.captures_iter(text) {
            let whole = captures.get(0).expect("group 0");
            let tld = captures
                .get(1)
                .expect("group 1")
                .as_str()
                .to_ascii_lowercase();
            let lower = whole.as_str().to_ascii_lowercase();
            let public = PUBLIC_DOMAINS
                .iter()
                .any(|d| lower == *d || lower.ends_with(&format!(".{d}")));
            if !HOST_TLDS.contains(&tld.as_str()) || public || IPV4.is_match(whole.as_str()) {
                continue;
            }
            candidates.push(Candidate {
                start: whole.start(),
                end: whole.end(),
                kind: "hostname",
                prefix: "HOST",
                priority,
            });
        }
        for captures in UNC_HOST.captures_iter(text) {
            let host = captures.get(1).expect("group 1");
            let before = text[..captures.get(0).expect("group 0").start()]
                .chars()
                .next_back();
            let directory = [
                "users",
                "windows",
                "temp",
                "programdata",
                "inetpub",
                "public",
                "program files",
                "system32",
            ]
            .contains(&host.as_str().to_ascii_lowercase().as_str());
            if directory || before.is_some_and(|c| c.is_alphanumeric() || c == ':' || c == '\\') {
                continue;
            }
            candidates.push(Candidate {
                start: host.start(),
                end: host.end(),
                kind: "hostname",
                prefix: "HOST",
                priority,
            });
        }
    }
    for r in known {
        let allowed = match r.kind {
            "ip" => options.ip_addresses,
            "hostname" => options.hostnames,
            _ => options.secrets,
        };
        if !allowed || r.original.len() < MIN_KNOWN_LEN {
            continue;
        }
        let prefix = if r.token.starts_with("‹IP_") {
            "IP"
        } else if r.token.starts_with("‹HOST_") {
            "HOST"
        } else {
            "SECRET"
        };
        for (start, _) in text.match_indices(&r.original) {
            candidates.push(Candidate {
                start,
                end: start + r.original.len(),
                kind: r.kind,
                prefix,
                priority: 0,
            });
        }
    }

    let mut selected = resolve_overlaps(&mut candidates);
    let mut found: Vec<(&str, &'static str, &'static str)> = Vec::new();
    for c in &selected {
        let original = &text[c.start..c.end];
        if original.len() >= MIN_KNOWN_LEN
            && !found.iter().any(|f| f.0 == original)
            && (c.kind != "password" || complex(original))
        {
            found.push((original, c.kind, c.prefix));
        }
    }
    let before = candidates.len();
    for (original, kind, prefix) in found {
        for (start, _) in text.match_indices(original) {
            candidates.push(Candidate {
                start,
                end: start + original.len(),
                kind,
                prefix,
                priority: 0,
            });
        }
    }
    if candidates.len() > before {
        selected = resolve_overlaps(&mut candidates);
    }

    let by_original: HashMap<&str, &Replacement> =
        known.iter().map(|r| (r.original.as_str(), r)).collect();
    let mut counters: HashMap<&'static str, u32> = HashMap::new();
    for r in known {
        for prefix in ["SECRET", "IP", "HOST"] {
            if let Some(n) = r
                .token
                .strip_prefix(&format!("{OPEN}{prefix}_"))
                .and_then(|t| t.strip_suffix(CLOSE))
                .and_then(|t| t.parse::<u32>().ok())
            {
                let counter = counters.entry(prefix).or_insert(0);
                *counter = (*counter).max(n);
            }
        }
    }

    let mut out = String::with_capacity(text.len());
    let mut replacements: Vec<Replacement> = Vec::new();
    let mut created: Vec<Replacement> = Vec::new();
    let mut cursor = 0usize;
    for c in &selected {
        let original = &text[c.start..c.end];
        out.push_str(&text[cursor..c.start]);
        cursor = c.end;
        let token = if let Some(existing) = by_original.get(original) {
            existing.token.clone()
        } else if let Some(existing) = created.iter().find(|r| r.original == original) {
            existing.token.clone()
        } else {
            let counter = counters.entry(c.prefix).or_insert(0);
            *counter += 1;
            let token = format!("{OPEN}{}_{}{CLOSE}", c.prefix, counter);
            created.push(Replacement {
                token: token.clone(),
                original: original.to_string(),
                kind: c.kind,
            });
            token
        };
        out.push_str(&token);
        if !replacements.iter().any(|r| r.token == token) {
            let kept = by_original
                .get(original)
                .map(|r| (*r).clone())
                .or_else(|| created.iter().find(|r| r.token == token).cloned());
            replacements.extend(kept);
        }
    }
    out.push_str(&text[cursor..]);
    Redacted {
        text: out,
        replacements,
    }
}

pub fn restore(text: &str, replacements: &[Replacement]) -> String {
    let lookup: HashMap<&str, &str> = replacements
        .iter()
        .map(|r| (r.token.as_str(), r.original.as_str()))
        .collect();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find(CLOSE) else {
            out.push_str(tail);
            return out;
        };
        let token = &tail[..end + CLOSE.len_utf8()];
        match lookup.get(token) {
            Some(original) => out.push_str(original),
            None => out.push_str(token),
        }
        rest = &tail[end + CLOSE.len_utf8()..];
    }
    out.push_str(rest);
    out
}
