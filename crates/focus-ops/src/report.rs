use crate::frontmatter::Document;
use crate::{
    Engagement, EngagementKind, Finding, Result, Severity, checklist, findings, fsutil, hosts,
    journal, loot,
};

#[derive(Debug, Clone)]
pub struct Options {
    pub include_journal: bool,
    pub include_credentials: bool,
    pub mask_secrets: bool,
    pub author: String,
}

fn cell(text: &str) -> String {
    let text = text.replace('|', "\\|").replace(['\n', '\r'], " ");
    let text = text.trim();
    if text.is_empty() {
        "-".to_string()
    } else {
        text.to_string()
    }
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "Critical",
        Severity::High => "High",
        Severity::Medium => "Medium",
        Severity::Low => "Low",
        Severity::Info => "Info",
    }
}

fn kind_label(kind: EngagementKind) -> &'static str {
    match kind {
        EngagementKind::ExternalPentest => "External penetration test",
        EngagementKind::InternalPentest => "Internal penetration test",
        EngagementKind::WebPentest => "Web penetration test",
        EngagementKind::RedTeam => "Red team",
        EngagementKind::Lab => "Lab",
        EngagementKind::Exam => "Exam",
    }
}

fn status_label(status: crate::EngagementStatus) -> &'static str {
    match status {
        crate::EngagementStatus::Planning => "Planning",
        crate::EngagementStatus::InProgress => "In progress",
        crate::EngagementStatus::Reporting => "Reporting",
        crate::EngagementStatus::Done => "Done",
    }
}

fn finding_body(finding: &Finding) -> Result<String> {
    let doc = Document::parse(&fsutil::read(&finding.path)?);
    let mut out = String::new();
    let mut in_fence = false;
    let mut skipped_title = false;
    for line in doc.body().lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            in_fence = !in_fence;
        }
        if !in_fence {
            if !skipped_title && line.starts_with("# ") {
                skipped_title = true;
                continue;
            }
            let level = line.chars().take_while(|c| *c == '#').count();
            if (1..=6).contains(&level) && line[level..].starts_with(' ') {
                out.push_str(&"#".repeat((level + 2).min(6)));
                out.push_str(&line[level..]);
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    Ok(out.trim().to_string())
}

pub fn masked(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        out = out.replace(secret.as_str(), MASK);
    }
    let options = focus_ai::redact::Options {
        secrets: true,
        ip_addresses: false,
        hostnames: false,
    };
    let redacted = focus_ai::redact::redact(&out, &options, &[]);
    let mut out = redacted.text;
    for replacement in &redacted.replacements {
        out = out.replace(&replacement.token, MASK);
    }
    out
}

const MASK: &str = "[masked]";

pub fn secrets_of(engagement_dir: &std::path::Path) -> Vec<String> {
    let credentials = loot::list(engagement_dir).into_iter().map(|c| c.secret);
    let proofs = hosts::list(engagement_dir)
        .into_iter()
        .flat_map(|h| crate::proofs::read(&h.path).unwrap_or_default())
        .map(|p| p.value);
    credentials
        .chain(proofs)
        .filter(|s| s.chars().count() >= 4)
        .collect()
}

pub fn build(engagement: &Engagement, options: &Options) -> Result<String> {
    let secrets: Vec<String> = if options.mask_secrets {
        secrets_of(&engagement.dir)
    } else {
        Vec::new()
    };
    let free = |text: &str| {
        if options.mask_secrets {
            masked(text, &secrets)
        } else {
            text.to_string()
        }
    };
    let findings = findings::list(&engagement.dir);
    let host_list = hosts::list(&engagement.dir);
    let mut out = String::new();

    out.push_str(&format!("# Report: {}\n\n", engagement.name));
    out.push_str(&format!("- Type: {}\n", kind_label(engagement.kind)));
    if let Some(client) = &engagement.client {
        out.push_str(&format!("- Client: {client}\n"));
    }
    if !options.author.trim().is_empty() {
        out.push_str(&format!("- Author: {}\n", options.author.trim()));
    }
    if let Some(started) = &engagement.started {
        out.push_str(&format!("- Start: {started}\n"));
    }
    if let Some(deadline) = &engagement.deadline {
        out.push_str(&format!("- Deadline: {deadline}\n"));
    }
    out.push_str(&format!(
        "- Status: {}\n- Report date: {}\n\n",
        status_label(engagement.status),
        fsutil::date_today()
    ));

    out.push_str("## Executive summary\n\n");
    if findings.is_empty() {
        out.push_str("No findings are recorded.\n\n");
    } else {
        out.push_str(&format!(
            "{} finding(s) recorded, broken down as follows:\n\n| Severity | Count |\n|---|---|\n",
            findings.len()
        ));
        for severity in [
            Severity::Critical,
            Severity::High,
            Severity::Medium,
            Severity::Low,
            Severity::Info,
        ] {
            out.push_str(&format!(
                "| {} | {} |\n",
                severity_label(severity),
                findings.iter().filter(|f| f.severity == severity).count()
            ));
        }
        out.push('\n');
        out.push_str("| ID | Title | Severity | Status |\n|---|---|---|---|\n");
        for f in &findings {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                f.id,
                cell(&free(&f.title)),
                severity_label(f.severity),
                findings::status_label(f.status)
            ));
        }
        out.push('\n');
    }

    out.push_str("## Scope\n\n### Included\n\n");
    if engagement.scope.is_empty() {
        out.push_str("Not provided.\n");
    }
    for item in &engagement.scope {
        out.push_str(&format!("- {item}\n"));
    }
    out.push_str("\n### Excluded\n\n");
    if engagement.out_of_scope.is_empty() {
        out.push_str("No exclusions are provided.\n");
    }
    for item in &engagement.out_of_scope {
        out.push_str(&format!("- {item}\n"));
    }
    out.push('\n');

    out.push_str("## Methodology\n\n");
    let sections = std::fs::read_to_string(engagement.dir.join("Checklist.md"))
        .map(|t| checklist::progress(&t))
        .unwrap_or_default();
    let sections: Vec<_> = sections.into_iter().filter(|s| s.total > 0).collect();
    if sections.is_empty() {
        out.push_str("No checklist is recorded.\n\n");
    } else {
        out.push_str(
            "Progress of the engagement checklist:\n\n| Phase | Done | Total |\n|---|---|---|\n",
        );
        for s in &sections {
            let title = if s.title.is_empty() {
                "General"
            } else {
                &s.title
            };
            out.push_str(&format!("| {} | {} | {} |\n", cell(title), s.done, s.total));
        }
        out.push('\n');
    }

    out.push_str("## Findings\n\n");
    if findings.is_empty() {
        out.push_str("No findings.\n\n");
    }
    for f in &findings {
        out.push_str(&format!("### {} {}\n\n", f.id, free(&f.title)));
        out.push_str("| Field | Value |\n|---|---|\n");
        out.push_str(&format!(
            "| Severity | {} |\n| Status | {} |\n",
            severity_label(f.severity),
            findings::status_label(f.status)
        ));
        if let Some(vector) = &f.cvss_vector {
            let score = f
                .cvss_score
                .map(|s| format!(" ({s:.1})"))
                .unwrap_or_default();
            out.push_str(&format!("| CVSS | {}{} |\n", cell(vector), score));
        }
        if !f.assets.is_empty() {
            out.push_str(&format!(
                "| Affected assets | {} |\n",
                cell(&f.assets.join(", "))
            ));
        }
        if !f.cwe.is_empty() {
            let listed: Vec<String> = f
                .cwe
                .iter()
                .map(|id| {
                    crate::cwe::parse_id(id)
                        .and_then(crate::cwe::name)
                        .map_or_else(|| id.clone(), |name| format!("{id} {name}"))
                })
                .collect();
            out.push_str(&format!("| CWE | {} |\n", cell(&listed.join(", "))));
        }
        if !f.attack.is_empty() {
            out.push_str(&format!(
                "| MITRE ATT&CK | {} |\n",
                cell(&f.attack.join(", "))
            ));
        }
        out.push('\n');
        let body = free(&finding_body(f)?);
        if !body.is_empty() {
            out.push_str(&body);
            out.push_str("\n\n");
        }
    }

    out.push_str("## Hosts and services\n\n");
    if host_list.is_empty() {
        out.push_str("No hosts.\n\n");
    } else {
        out.push_str("| Address | Name | OS | Status | Open services |\n|---|---|---|---|---|\n");
        for h in &host_list {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                cell(&h.ip),
                cell(h.hostname.as_deref().unwrap_or("")),
                cell(h.os.as_deref().unwrap_or("")),
                hosts::status_name(h.status),
                h.services.len()
            ));
        }
        out.push('\n');
        for h in &host_list {
            let proofs = crate::proofs::read(&h.path).unwrap_or_default();
            if h.services.is_empty() && proofs.is_empty() {
                continue;
            }
            out.push_str(&format!(
                "### {}\n\n",
                h.hostname
                    .as_deref()
                    .map_or_else(|| h.ip.clone(), |n| format!("{n} ({})", h.ip))
            ));
            if !h.services.is_empty() {
                out.push_str("| Port | Proto | State | Service | Product | Version |\n|---|---|---|---|---|---|\n");
                for s in &h.services {
                    out.push_str(&format!(
                        "| {} | {} | {} | {} | {} | {} |\n",
                        s.port,
                        cell(&s.proto),
                        cell(&s.state),
                        cell(&s.name),
                        cell(&s.product),
                        cell(&s.version)
                    ));
                }
                out.push('\n');
            }
            if !proofs.is_empty() {
                out.push_str("| Proof | Value | Taken |\n|---|---|---|\n");
                for p in &proofs {
                    let value = if options.mask_secrets {
                        MASK.to_string()
                    } else {
                        cell(&p.value)
                    };
                    out.push_str(&format!(
                        "| {} | {} | {} |\n",
                        cell(&p.name),
                        value,
                        cell(&p.taken)
                    ));
                }
                out.push('\n');
            }
        }
    }

    let mut annexes = String::new();
    if options.include_credentials {
        let credentials = loot::list(&engagement.dir);
        annexes.push_str("### Credentials\n\n");
        if credentials.is_empty() {
            annexes.push_str("No credentials recorded.\n\n");
        } else {
            annexes.push_str("| User | Type | Secret | Domain | Source | Valid on |\n|---|---|---|---|---|---|\n");
            for c in &credentials {
                let secret = if options.mask_secrets {
                    MASK.to_string()
                } else {
                    cell(&c.secret)
                };
                annexes.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} |\n",
                    cell(&c.user),
                    cell(&c.kind),
                    secret,
                    cell(&c.domain),
                    cell(&c.source),
                    cell(&c.valid_on)
                ));
            }
            annexes.push('\n');
        }
    }
    if options.include_journal {
        let entries = journal::read(&engagement.dir);
        annexes.push_str("### Journal\n\n");
        if entries.is_empty() {
            annexes.push_str("Journal is empty.\n\n");
        } else {
            annexes.push_str("| Timestamp | Target | Entry |\n|---|---|---|\n");
            for e in &entries {
                annexes.push_str(&format!(
                    "| {} | {} | {} |\n",
                    e.timestamp,
                    cell(e.target.as_deref().unwrap_or("")),
                    cell(&free(&e.text))
                ));
            }
            annexes.push('\n');
        }
    }
    if !annexes.is_empty() {
        out.push_str("## Appendices\n\n");
        out.push_str(&annexes);
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Credential, FindingStatus, NewEngagement, load};

    fn options(mask: bool) -> Options {
        Options {
            include_journal: true,
            include_credentials: true,
            mask_secrets: mask,
            author: "Example Person".into(),
        }
    }

    fn engagement_with_content(name: &str) -> Engagement {
        let ops = fsutil::test_dir(name);
        let created = crate::create(
            &ops,
            &NewEngagement {
                name: "Mission".into(),
                kind: EngagementKind::InternalPentest,
                client: Some("Example Corp".into()),
                scope: vec!["198.51.100.0/24".into()],
                exam: None,
                checklist: None,
            },
        )
        .unwrap();
        let mut e = load(&created.dir).unwrap();
        e.out_of_scope = vec!["198.51.100.250".into()];
        crate::save(&e).unwrap();
        hosts::import(
            &e.dir,
            &hosts::parse_nmap(include_str!("../tests/data/scan.xml")).unwrap(),
        )
        .unwrap();
        let mut f1 = findings::create(&e.dir, "Minor finding", Severity::Low).unwrap();
        f1.assets = vec!["192.0.2.5".into()];
        findings::save(&f1).unwrap();
        let mut f2 = findings::create(&e.dir, "Major finding", Severity::Critical).unwrap();
        f2.cvss_vector = Some("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H".into());
        f2.cvss_score = Some(9.8);
        f2.status = FindingStatus::Confirmed;
        f2.attack = vec!["T1000".into()];
        f2.cwe = vec!["CWE-79".into(), "CWE-9999999".into()];
        findings::save(&f2).unwrap();
        let text = std::fs::read_to_string(&f2.path).unwrap().replace(
            "## Description\n",
            "## Description\n\nDescription text of the major finding.\n\n### Detail\n\nSub part.\n",
        );
        std::fs::write(&f2.path, text).unwrap();
        loot::add(
            &e.dir,
            &Credential {
                user: "alice".into(),
                kind: "password".into(),
                secret: "Secret-123".into(),
                domain: "example.test".into(),
                source: "note".into(),
                valid_on: "192.0.2.5".into(),
                notes: String::new(),
            },
        )
        .unwrap();
        journal::append(&e.dir, Some("192.0.2.5"), "first observation | with bar").unwrap();
        load(&e.dir).unwrap()
    }

    #[test]
    fn a_masked_report_hides_the_secrets_quoted_in_the_journal_and_the_findings() {
        let ops = fsutil::test_dir("report-free-text");
        let created = crate::create(
            &ops,
            &NewEngagement {
                name: "Quote".into(),
                kind: EngagementKind::Lab,
                client: None,
                scope: vec![],
                exam: None,
                checklist: None,
            },
        )
        .unwrap();
        loot::add(
            &created.dir,
            &crate::Credential {
                user: "svc".into(),
                kind: "password".into(),
                secret: "Zebra-Plain-77".into(),
                domain: String::new(),
                source: String::new(),
                valid_on: String::new(),
                notes: String::new(),
            },
        )
        .unwrap();
        journal::append(
            &created.dir,
            None,
            "tried Zebra-Plain-77 on the share, then password=Other-Secret-12345",
        )
        .unwrap();
        let engagement = crate::load(&created.dir).unwrap();
        let options = |mask| Options {
            include_journal: true,
            include_credentials: true,
            mask_secrets: mask,
            author: String::new(),
        };
        let masked = build(&engagement, &options(true)).unwrap();
        assert!(
            !masked.contains("Zebra-Plain-77") && !masked.contains("Other-Secret-12345"),
            "{masked}"
        );
        assert!(
            build(&engagement, &options(false))
                .unwrap()
                .contains("Zebra-Plain-77"),
            "an explicit request keeps the text"
        );
    }

    #[test]
    fn report_has_every_section_in_order_and_findings_by_severity() {
        let e = engagement_with_content("report-full");
        let report = build(&e, &options(true)).unwrap();
        let positions: Vec<usize> = [
            "# Report: Mission",
            "## Executive summary",
            "## Scope",
            "## Methodology",
            "## Findings",
            "## Hosts and services",
            "## Appendices",
        ]
        .iter()
        .map(|h| report.find(h).unwrap_or_else(|| panic!("{h}")))
        .collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{positions:?}");
        assert!(
            report.find("### F-002 Major finding").unwrap()
                < report.find("### F-001 Minor finding").unwrap()
        );
        assert!(report.contains("| Critical | 1 |") && report.contains("| Low | 1 |"));
        assert!(report.contains("| CVSS | CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H (9.8) |"));
        assert!(report.contains("| CWE | CWE-79 Improper Neutralization of Input During Web Page Generation, CWE-9999999 |"), "the name when the catalog lists it");
        assert!(report.contains("#### Description\n\nDescription text of the major finding.\n\n##### Detail\n\nSub part."), "headings lowered by two levels");
        assert!(
            !report.contains("# Major finding\n"),
            "the level 1 title of the note is not repeated"
        );
        assert!(report.contains("- 198.51.100.0/24\n") && report.contains("- 198.51.100.250\n"));
        assert!(
            report.contains("| 192.0.2.5 | srv-web.example.test | Linux 5.4 - 5.15 | new | 3 |")
        );
        assert!(
            report.contains("### srv-web.example.test (192.0.2.5)")
                && report.contains("| 22 | tcp | open | ssh | OpenSSH |")
        );
        assert!(report.contains("| Scoping and authorization | 0 | 5 |"));
        assert!(
            report.contains("- Client: Example Corp")
                && report.contains("- Author: Example Person")
        );
    }

    #[test]
    fn secrets_are_masked_unless_asked_and_annexes_are_optional() {
        let e = engagement_with_content("report-secrets");
        let masked = build(&e, &options(true)).unwrap();
        assert!(!masked.contains("Secret-123") && masked.contains("[masked]"));
        assert!(masked.contains("first observation \\| with bar"));
        let clear = build(&e, &options(false)).unwrap();
        assert!(clear.contains("Secret-123"));
        let none = build(
            &e,
            &Options {
                include_journal: false,
                include_credentials: false,
                mask_secrets: true,
                author: String::new(),
            },
        )
        .unwrap();
        assert!(
            !none.contains("## Appendices")
                && !none.contains("Secret-123")
                && !none.contains("- Author")
        );
    }

    #[test]
    fn empty_engagement_still_builds() {
        let ops = fsutil::test_dir("report-empty");
        let created = crate::create(
            &ops,
            &NewEngagement {
                name: "Empty".into(),
                kind: EngagementKind::Lab,
                client: None,
                scope: vec![],
                exam: None,
                checklist: None,
            },
        )
        .unwrap();
        let report = build(&created, &options(true)).unwrap();
        for text in [
            "No findings are recorded.",
            "Not provided.",
            "No hosts.",
            "No credentials recorded.",
            "Journal is empty.",
        ] {
            assert!(report.contains(text), "{text}");
        }
    }
}
