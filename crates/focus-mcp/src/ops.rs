use std::path::{Path, PathBuf};

use focus_ops::{
    Credential, Engagement, Severity, checklist, cvss, findings, hosts, journal, loot, report,
};
use serde_json::Value;

use crate::fault::Fault;
use crate::rules::{WriteOptions, etag};
use crate::tools::{Page, Refusal, VaultTools, bounded, dry_run_text, text_argument};

const MASKED: &str = "[masked]";
const MAX_NMAP_BYTES: usize = 5_000_000;
const MAX_JOURNAL_CHARS: usize = 10_000;
const MAX_FIELD_CHARS: usize = 1_000;
const MAX_LIST_ITEMS: usize = 100;
const VARS_REREAD: &str = "Call ops_vars_get with the same `engagement` to read the variables and their Etag again, then send it as `if_match`.";

#[derive(Debug, Clone)]
pub struct OpsConfig {
    pub dir: PathBuf,
    pub reveal_secrets: bool,
}

fn optional_bool(arguments: &Value, name: &str) -> Option<bool> {
    arguments.get(name).and_then(Value::as_bool)
}

fn optional_text<'a>(arguments: &'a Value, name: &str) -> Option<&'a str> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Low => "low",
        Severity::Medium => "medium",
        Severity::High => "high",
        Severity::Critical => "critical",
    }
}

fn parse_severity(text: &str) -> Result<Severity, String> {
    [
        Severity::Info,
        Severity::Low,
        Severity::Medium,
        Severity::High,
        Severity::Critical,
    ]
    .into_iter()
    .find(|s| severity_name(*s) == text.to_ascii_lowercase())
    .ok_or_else(|| format!("unknown severity: {text} (info, low, medium, high or critical)"))
}

pub(crate) fn sensitive_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "PASS", "PWD", "SECRET", "TOKEN", "HASH", "NTLM", "KEY", "CRED", "AUTH", "COOKIE",
        "SESSION", "BEARER", "JWT", "PRIVATE", "SEED", "PIN",
    ]
    .iter()
    .any(|k| upper.contains(k))
}

fn mask(value: &str, reveal: bool) -> String {
    if reveal || value.is_empty() {
        value.to_string()
    } else {
        MASKED.to_string()
    }
}

fn file_content(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

fn etag_line(path: &Path) -> String {
    file_content(path)
        .map(|content| format!("\nEtag: {}", etag(&content)))
        .unwrap_or_default()
}

fn shown_variable(name: &str, value: &str, reveal: bool) -> String {
    if reveal || (!sensitive_name(name) && crate::exposure::mask_secrets(value) == value) {
        value.to_string()
    } else {
        MASKED.to_string()
    }
}

impl VaultTools {
    fn ops_config(&self) -> Result<&crate::ops::OpsConfig, String> {
        self.ops
            .as_deref()
            .ok_or_else(|| "Ops mode is not enabled: no engagements folder".to_string())
    }

    fn engagement(&self, arguments: &Value) -> Result<Engagement, String> {
        let config = self.ops_config()?;
        let name = text_argument(arguments, "engagement")?.trim();
        if name.is_empty()
            || name.contains(['/', '\\'])
            || name == "."
            || name == ".."
            || name.contains('\0')
        {
            return Err(format!(
                "invalid engagement name: {name:?}. Use a name returned by ops_list_engagements."
            ));
        }
        let dir = config.dir.join(name);
        let inside = dunce::canonicalize(&dir)
            .ok()
            .zip(dunce::canonicalize(&config.dir).ok())
            .is_some_and(|(found, ops)| found.parent() == Some(ops.as_path()));
        if dir.exists() && !inside {
            return Err(format!(
                "engagement not found: {name}. Use a name returned by ops_list_engagements."
            ));
        }
        focus_ops::load(&dir).map_err(|e| {
            let known: Vec<String> = focus_ops::list(&config.dir)
                .into_iter()
                .map(|e| e.name)
                .collect();
            format!(
                "engagement not found: {name} ({e}). Engagements: {}",
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            )
        })
    }

    fn check_etag(&self, options: &WriteOptions, file: &Path, reread: &str) -> Result<(), Fault> {
        options.check(file_content(file).as_deref(), "engagement", reread)
    }

    fn reveal(&self) -> bool {
        self.ops.as_ref().is_some_and(|o| o.reveal_secrets)
    }

    pub(crate) fn ops_tool(
        &self,
        name: &str,
        arguments: &Value,
        options: &WriteOptions,
        cancel: &focus_ai::Cancel,
    ) -> Result<String, Refusal> {
        self.ops_config()?;
        if focus_ai::agent::is_write_tool(name) && !self.writes_enabled() {
            return Err(Fault::read_only().into());
        }
        match name {
            "ops_list_engagements" => {
                let all = focus_ops::list(&self.ops_config()?.dir);
                if all.is_empty() {
                    return Ok("No engagements".into());
                }
                let mut out = format!("{} engagement(s)\n", all.len());
                for e in all {
                    out.push_str(&format!(
                        "{}: {:?}, {:?}{}\n",
                        e.name,
                        e.kind,
                        e.status,
                        e.focus.map(|f| format!(", target {f}")).unwrap_or_default()
                    ));
                }
                Ok(out)
            }
            "ops_overview" => {
                let e = self.engagement(arguments)?;
                let o = focus_ops::overview(&e.dir);
                Ok(format!(
                    "{} ({:?}, {:?})\nHosts: {}, {} owned, services: {}\nFindings: critical {}, high {}, medium {}, low {}, info {}\nCredentials: {}\nJournal: {} entries\nChecklist: {}/{}\nPoints: {}\n",
                    e.name,
                    e.kind,
                    e.status,
                    o.hosts,
                    o.hosts_owned,
                    o.services,
                    o.findings[4],
                    o.findings[3],
                    o.findings[2],
                    o.findings[1],
                    o.findings[0],
                    o.credentials,
                    o.journal_entries,
                    o.checklist_done,
                    o.checklist_total,
                    o.points
                ))
            }
            "ops_journal_read" => {
                let e = self.engagement(arguments)?;
                let page = Page::of(arguments, 50, usize::MAX)?;
                let target = optional_text(arguments, "target");
                let entries: Vec<_> = journal::read(&e.dir)
                    .into_iter()
                    .filter(|x| target.is_none_or(|t| x.target.as_deref() == Some(t)))
                    .collect();
                if entries.is_empty() {
                    return Ok("Journal is empty".into());
                }
                let end = entries.len().saturating_sub(page.offset);
                let start = end.saturating_sub(page.limit);
                let total = entries.len();
                let mut out = format!(
                    "{total} entries{}\n",
                    if page.offset == 0 && start > 0 {
                        format!(", the last {}", end - start)
                    } else if end - start < total {
                        format!(", entries {} to {end} shown", start + 1)
                    } else {
                        String::new()
                    }
                );
                for x in &entries[start..end] {
                    let line = if self.reveal() {
                        x.text.clone()
                    } else {
                        crate::exposure::mask_secrets(&x.text)
                    };
                    out.push_str(&format!(
                        "{}{} {}\n",
                        x.timestamp,
                        x.target
                            .as_ref()
                            .map(|t| format!(" [{t}]"))
                            .unwrap_or_default(),
                        line
                    ));
                }
                out.push_str(&crate::tools::more(
                    start,
                    "older entries",
                    page.offset + (end - start),
                    "send `target` to keep the entries of one host",
                ));
                Ok(out)
            }
            "ops_journal_append" => {
                let e = self.engagement(arguments)?;
                let text = text_argument(arguments, "text")?;
                bounded("text", text, MAX_JOURNAL_CHARS)?;
                let target = optional_text(arguments, "target")
                    .map(str::to_string)
                    .or_else(|| e.focus.clone());
                if let Some(target) = &target {
                    bounded("target", target, MAX_FIELD_CHARS)?;
                }
                if text.trim().is_empty() {
                    return Err(Refusal::Error(
                        "parameter `text`: the journal entry is empty".into(),
                    ));
                }
                if options.dry_run {
                    return Ok(dry_run_text(
                        &format!("{name} would append one line to the journal of {}", e.name),
                        &format!(
                            "{}{}\n",
                            target.map(|t| format!("[{t}] ")).unwrap_or_default(),
                            text.trim().replace(['\r', '\n'], " ")
                        ),
                    ));
                }
                self.authorize(name, &e.dir.join("Journal.md"), None, text, cancel)?;
                let entry = journal::append(&e.dir, target.as_deref(), text)
                    .map_err(|x| Refusal::Error(x.to_string()))?;
                Ok(format!(
                    "Journal: {}{} {}",
                    entry.timestamp,
                    entry
                        .target
                        .as_ref()
                        .map(|t| format!(" [{t}]"))
                        .unwrap_or_default(),
                    entry.text
                ))
            }
            "ops_hosts_list" => {
                let e = self.engagement(arguments)?;
                let page = Page::of(arguments, MAX_LIST_ITEMS, usize::MAX)?;
                let list = hosts::list(&e.dir);
                if list.is_empty() {
                    return Ok("No hosts".into());
                }
                let total = list.len();
                let (list, remainder) = page.cut(list, "hosts", "");
                let mut out = format!("{total} host(s)\n");
                for h in list {
                    let stem = h
                        .path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let ports: Vec<String> = h
                        .services
                        .iter()
                        .map(|s| format!("{}/{} {}", s.port, s.proto, s.name))
                        .collect();
                    let proofs: Vec<String> = focus_ops::proofs::read(&h.path)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|p| p.name)
                        .collect();
                    out.push_str(&format!(
                        "{} ({}): {}, {:?}, {}{}\n",
                        h.ip,
                        stem,
                        h.os.as_deref().unwrap_or("unknown operating system"),
                        h.status,
                        if ports.is_empty() {
                            "no services".to_string()
                        } else {
                            ports.join(", ")
                        },
                        if proofs.is_empty() {
                            String::new()
                        } else {
                            format!(", proofs: {}", proofs.join(" "))
                        }
                    ));
                }
                out.push_str(&remainder);
                Ok(out)
            }
            "ops_import_nmap" => {
                let e = self.engagement(arguments)?;
                let text = text_argument(arguments, "text")?;
                if text.len() > MAX_NMAP_BYTES {
                    return Err(Refusal::Error(format!(
                        "nmap output too long: {} bytes, at most {MAX_NMAP_BYTES}",
                        text.len()
                    )));
                }
                let parsed = hosts::parse_nmap(text).map_err(|x| Refusal::Error(x.to_string()))?;
                if options.dry_run {
                    let known: Vec<String> =
                        hosts::list(&e.dir).into_iter().map(|h| h.ip).collect();
                    let new = parsed.iter().filter(|h| !known.contains(&h.ip)).count();
                    let services: usize = parsed.iter().map(|h| h.services.len()).sum();
                    return Ok(dry_run_text(
                        &format!("{name} would import into {}", e.name),
                        &format!(
                            "{} host(s) read: {new} new, {} already known (they keep their notes); {services} service(s) in the output\n",
                            parsed.len(),
                            parsed.len() - new
                        ),
                    ));
                }
                self.authorize(
                    name,
                    &e.dir.join("Hosts"),
                    None,
                    &format!("import {} host(s)", parsed.len()),
                    cancel,
                )?;
                let report =
                    hosts::import(&e.dir, &parsed).map_err(|x| Refusal::Error(x.to_string()))?;
                Ok(format!(
                    "nmap import: {} host(s) read, {} created ({}), {} updated ({}), {} service(s) added or changed",
                    parsed.len(),
                    report.created.len(),
                    report.created.join(", "),
                    report.updated.len(),
                    report.updated.join(", "),
                    report.services
                ))
            }
            "ops_findings_list" => {
                let e = self.engagement(arguments)?;
                let page = Page::of(arguments, MAX_LIST_ITEMS, usize::MAX)?;
                let list = findings::list(&e.dir);
                if list.is_empty() {
                    return Ok("No findings".into());
                }
                let total = list.len();
                let (list, remainder) = page.cut(list, "findings", "");
                let mut out = format!("{total} finding(s)\n");
                for f in list {
                    out.push_str(&format!(
                        "{}: {} ({}, {:?}{}{})\n",
                        f.id,
                        f.title,
                        severity_name(f.severity),
                        f.status,
                        f.cvss_score
                            .map(|s| format!(", CVSS {s:.1}"))
                            .unwrap_or_default(),
                        if f.cwe.is_empty() {
                            String::new()
                        } else {
                            format!(", {}", f.cwe.join(" "))
                        }
                    ));
                }
                out.push_str(&remainder);
                Ok(out)
            }
            "ops_finding_create" => {
                let e = self.engagement(arguments)?;
                let title = text_argument(arguments, "title")?;
                bounded("title", title, MAX_FIELD_CHARS)?;
                let score = match optional_text(arguments, "cvss") {
                    Some(vector) => {
                        Some(cvss::score(vector).map_err(|m| format!("parameter `cvss`: {m}"))?)
                    }
                    None => None,
                };
                let severity = match optional_text(arguments, "severity") {
                    Some(s) => parse_severity(s)?,
                    None => score.as_ref().map_or(Severity::Info, |s| s.severity),
                };
                let list = |key: &str| -> Vec<String> {
                    arguments
                        .get(key)
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default()
                };
                let (assets, attack, weaknesses) = (list("assets"), list("attack"), list("cwe"));
                for id in &weaknesses {
                    bounded("cwe", id, MAX_FIELD_CHARS)?;
                }
                let cwe: Vec<String> = weaknesses
                    .iter()
                    .map(|id| {
                        focus_ops::cwe::normalize(id).ok_or_else(|| {
                            Refusal::Error(format!("parameter `cwe`: not a CWE identifier: {id}"))
                        })
                    })
                    .collect::<std::result::Result<_, _>>()?;
                if cwe.len() > MAX_LIST_ITEMS {
                    return Err(Refusal::Error(format!(
                        "parameter `cwe`: at most {MAX_LIST_ITEMS} weaknesses"
                    )));
                }
                if assets.len() > MAX_LIST_ITEMS
                    || attack.len() > MAX_LIST_ITEMS
                    || assets
                        .iter()
                        .chain(&attack)
                        .any(|item| item.chars().count() > MAX_FIELD_CHARS)
                {
                    return Err(Refusal::Error(format!(
                        "parameter `assets` or `attack`: at most {MAX_LIST_ITEMS} items of {MAX_FIELD_CHARS} characters"
                    )));
                }
                if title.trim().is_empty() {
                    return Err(Refusal::Error(
                        "parameter `title`: the finding title is empty".into(),
                    ));
                }
                if options.dry_run {
                    return Ok(dry_run_text(
                        &format!("{name} would create a finding in {}", e.name),
                        &format!(
                            "Title: {}\nSeverity: {}{}\n",
                            title.trim(),
                            severity_name(severity),
                            score
                                .as_ref()
                                .map(|s| format!(", CVSS {:.1}", s.base))
                                .unwrap_or_default()
                        ),
                    ));
                }
                self.authorize(
                    name,
                    &e.dir.join("Findings"),
                    None,
                    &format!("create the finding {title:?} ({})", severity_name(severity)),
                    cancel,
                )?;
                let mut finding = findings::create(&e.dir, title, severity)
                    .map_err(|x| Refusal::Error(x.to_string()))?;
                if let Some(score) = score {
                    finding.cvss_vector = Some(score.vector);
                    finding.cvss_score = Some(score.base);
                }
                finding.assets = assets;
                finding.attack = attack;
                finding.cwe = cwe;
                findings::save(&finding).map_err(|x| Refusal::Error(x.to_string()))?;
                Ok(format!(
                    "Finding created: {} {} ({}){}\nNote: {}",
                    finding.id,
                    finding.title,
                    severity_name(finding.severity),
                    finding
                        .cvss_score
                        .map(|s| format!(", CVSS {s:.1}"))
                        .unwrap_or_default(),
                    finding.path.display()
                ))
            }
            "ops_cvss_score" => {
                let score = cvss::score(text_argument(arguments, "vector")?)?;
                Ok(format!(
                    "CVSS {}: {:.1} ({})\nNormalized vector: {}",
                    score.version,
                    score.base,
                    severity_name(score.severity),
                    score.vector
                ))
            }
            "ops_credentials_list" => {
                let e = self.engagement(arguments)?;
                let page = Page::of(arguments, MAX_LIST_ITEMS, usize::MAX)?;
                let list = loot::list(&e.dir);
                if list.is_empty() {
                    return Ok("No credentials".into());
                }
                let total = list.len();
                let (list, remainder) = page.cut(list, "credentials", "");
                let mut out = format!(
                    "{total} credential(s){}\n",
                    if self.reveal() {
                        ""
                    } else {
                        ", secrets masked"
                    }
                );
                for c in list {
                    out.push_str(&format!(
                        "{}{}: {} {} (source: {}, valid on: {})\n",
                        if c.domain.is_empty() {
                            String::new()
                        } else {
                            format!("{}\\", c.domain)
                        },
                        c.user,
                        c.kind,
                        mask(&c.secret, self.reveal()),
                        c.source,
                        c.valid_on
                    ));
                }
                out.push_str(&remainder);
                Ok(out)
            }
            "ops_credential_add" => {
                let e = self.engagement(arguments)?;
                let get = |k: &str| optional_text(arguments, k).unwrap_or("").to_string();
                let credential = Credential {
                    user: text_argument(arguments, "user")?.trim().to_string(),
                    kind: text_argument(arguments, "kind")?.trim().to_string(),
                    secret: text_argument(arguments, "secret")?.trim().to_string(),
                    domain: get("domain"),
                    source: get("source"),
                    valid_on: get("valid_on"),
                    notes: get("notes"),
                };
                if credential.user.is_empty() || credential.secret.is_empty() {
                    return Err(Refusal::Error("`user` and `secret` cannot be empty".into()));
                }
                for (field, value) in [
                    ("user", &credential.user),
                    ("kind", &credential.kind),
                    ("secret", &credential.secret),
                    ("domain", &credential.domain),
                    ("source", &credential.source),
                    ("valid_on", &credential.valid_on),
                    ("notes", &credential.notes),
                ] {
                    bounded(field, value, MAX_FIELD_CHARS)?;
                }
                if options.dry_run {
                    let present = loot::list(&e.dir).iter().any(|c| {
                        c.user == credential.user
                            && c.kind == credential.kind
                            && c.secret == credential.secret
                            && c.domain == credential.domain
                    });
                    let what = format!("{} ({})", credential.user, credential.kind);
                    return Ok(dry_run_text(
                        &format!("{name} on {}", e.name),
                        &if present {
                            format!("{what} is already in the table: nothing would change\n")
                        } else {
                            format!("{what} would be added to the table\n")
                        },
                    ));
                }
                self.authorize(
                    name,
                    &e.dir.join("Loot").join("Credentials.md"),
                    None,
                    &format!(
                        "add the credential {} ({})",
                        credential.user, credential.kind
                    ),
                    cancel,
                )?;
                let added =
                    loot::add(&e.dir, &credential).map_err(|x| Refusal::Error(x.to_string()))?;
                Ok(if added {
                    format!(
                        "Credential added: {} ({})",
                        credential.user, credential.kind
                    )
                } else {
                    format!(
                        "Credential already present: {} ({})",
                        credential.user, credential.kind
                    )
                })
            }
            "ops_vars_get" => {
                let e = self.engagement(arguments)?;
                let version = etag_line(&e.dir.join("README.md"));
                if e.vars.is_empty() {
                    return Ok(format!("No variables{version}"));
                }
                let mut out = format!("{} variable(s)\n", e.vars.len());
                for (k, v) in &e.vars {
                    out.push_str(&format!("{k}={}\n", shown_variable(k, v, self.reveal())));
                }
                if let Some(focus) = &e.focus {
                    out.push_str(&format!("Targeted host: {focus}\n"));
                }
                if !version.is_empty() {
                    out.push_str(version.trim_start());
                    out.push('\n');
                }
                Ok(out)
            }
            "ops_vars_set" => {
                let mut e = self.engagement(arguments)?;
                let variable = text_argument(arguments, "name")?.trim().to_string();
                if variable.is_empty() || variable.contains(['\n', '\r', ':']) {
                    return Err(Refusal::Error("invalid variable name".into()));
                }
                bounded("name", &variable, MAX_FIELD_CHARS)?;
                let value = text_argument(arguments, "value")?.trim().to_string();
                bounded("value", &value, MAX_FIELD_CHARS)?;
                let readme = e.dir.join("README.md");
                self.check_etag(options, &readme, VARS_REREAD)?;
                if options.dry_run {
                    let old = focus_ops::vars::stored(&e.vars, &variable)
                        .map(|(_, v)| shown_variable(&variable, v, self.reveal()));
                    let new = if value.is_empty() {
                        None
                    } else {
                        Some(shown_variable(&variable, &value, self.reveal()))
                    };
                    return Ok(dry_run_text(
                        &format!("{name} would change a variable of {}", e.name),
                        &format!(
                            "{variable}: was {}, would be {}\nEtag now: {}\n",
                            old.as_deref().unwrap_or("(not set)"),
                            new.as_deref().unwrap_or("(deleted)"),
                            file_content(&readme).map(|c| etag(&c)).unwrap_or_default()
                        ),
                    ));
                }
                self.authorize(
                    name,
                    &e.dir.join("README.md"),
                    None,
                    &format!(
                        "{variable} = {}",
                        if sensitive_name(&variable) {
                            MASKED.to_string()
                        } else {
                            value.clone()
                        }
                    ),
                    cancel,
                )?;
                focus_ops::links::unlink(&mut e, &variable);
                focus_ops::vars::store(&mut e.vars, &variable, &value);
                focus_ops::save(&e).map_err(|x| Refusal::Error(x.to_string()))?;
                Ok(format!(
                    "Variable {variable} {}{}",
                    if focus_ops::vars::stored(&e.vars, &variable).is_some() {
                        "set"
                    } else {
                        "deleted"
                    },
                    etag_line(&readme)
                ))
            }
            "ops_set_focus" => {
                let mut e = self.engagement(arguments)?;
                let host = text_argument(arguments, "host")?.trim();
                let focus = if host.is_empty() {
                    None
                } else {
                    let found = hosts::list(&e.dir).into_iter().find(|h| {
                        h.ip == host
                            || h.path
                                .file_stem()
                                .is_some_and(|s| s.to_string_lossy() == host)
                    });
                    let found = found
                        .ok_or_else(|| format!("host not found: {host}. See ops_hosts_list."))?;
                    Some(
                        found
                            .path
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    )
                };
                let readme = e.dir.join("README.md");
                self.check_etag(options, &readme, VARS_REREAD)?;
                if options.dry_run {
                    return Ok(dry_run_text(
                        &format!("{name} would change the targeted host of {}", e.name),
                        &format!(
                            "was {}, would be {}\n",
                            e.focus.as_deref().unwrap_or("none"),
                            focus.as_deref().unwrap_or("none")
                        ),
                    ));
                }
                self.authorize(
                    name,
                    &readme,
                    None,
                    &format!("target: {}", focus.as_deref().unwrap_or("none")),
                    cancel,
                )?;
                e.focus.clone_from(&focus);
                focus_ops::save(&e).map_err(|x| Refusal::Error(x.to_string()))?;
                Ok(format!(
                    "Targeted host: {}{}",
                    focus.as_deref().unwrap_or("none"),
                    etag_line(&readme)
                ))
            }
            "ops_checklist_progress" => {
                let e = self.engagement(arguments)?;
                let text = std::fs::read_to_string(e.dir.join("Checklist.md"))
                    .map_err(|x| format!("Checklist.md: {x}"))?;
                let sections = checklist::progress(&text);
                let mut out = String::new();
                for s in sections.iter().filter(|s| s.total > 0) {
                    out.push_str(&format!(
                        "{}: {}/{}",
                        if s.title.is_empty() {
                            "General"
                        } else {
                            &s.title
                        },
                        s.done,
                        s.total
                    ));
                    if let Some((line, task)) = &s.next {
                        out.push_str(&format!(", next task (line {line}): {task}"));
                    }
                    out.push('\n');
                }
                Ok(if out.is_empty() {
                    "No checkboxes in the checklist".into()
                } else {
                    out
                })
            }
            "ops_report_build" => {
                let e = self.engagement(arguments)?;
                let build = report::Options {
                    include_journal: optional_bool(arguments, "include_journal").unwrap_or(true),
                    include_credentials: optional_bool(arguments, "include_credentials")
                        .unwrap_or(false),
                    mask_secrets: !self.reveal(),
                    author: optional_text(arguments, "author").unwrap_or("").to_string(),
                };
                bounded("author", &build.author, MAX_FIELD_CHARS)?;
                let text = report::build(&e, &build).map_err(|x| Refusal::Error(x.to_string()))?;
                if optional_bool(arguments, "save").unwrap_or(false) {
                    if !self.writes_enabled() {
                        return Err(Refusal::Error(
                            "writing Report.md is a write: the server is read-only (--write)"
                                .into(),
                        ));
                    }
                    let path = e.dir.join("Report.md");
                    if options.if_match.is_some() {
                        self.check_etag(
                            options,
                            &path,
                            "Call read_note on the Report.md of the engagement, or ops_report_build with `dry_run` set, to see what is there, then send its Etag as `if_match`.",
                        )?;
                    }
                    if options.dry_run {
                        return Ok(dry_run_text(
                            &format!("ops_report_build would write Report.md of {}", e.name),
                            &format!(
                                "{} byte(s), Report.md would be {}\n",
                                text.len(),
                                if path.exists() { "replaced" } else { "created" }
                            ),
                        ));
                    }
                    self.authorize("ops_report_build", Path::new(&path), None, &text, cancel)?;
                    #[cfg(unix)]
                    let private = build.include_credentials && !build.mask_secrets;
                    focus_ops::keep_version(&path).map_err(|x| Refusal::Error(x.to_string()))?;
                    let written = (|| -> std::io::Result<()> {
                        use std::io::Write as _;
                        let mut file = std::fs::OpenOptions::new();
                        file.write(true).create(true).truncate(true);
                        #[cfg(unix)]
                        std::os::unix::fs::OpenOptionsExt::mode(
                            &mut file,
                            if private { 0o600 } else { 0o644 },
                        );
                        file.open(&path)?.write_all(text.as_bytes())
                    })();
                    written.map_err(|x| Refusal::Error(format!("{}: {x}", path.display())))?;
                    return Ok(format!(
                        "Report written: {} ({} bytes){}",
                        path.display(),
                        text.len(),
                        etag_line(&path)
                    ));
                }
                Ok(text)
            }
            other => Err(Refusal::Error(format!("unknown tool: {other}"))),
        }
    }
}
