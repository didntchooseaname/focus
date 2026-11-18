use std::path::Path;
use std::time::SystemTime;

use crate::frontmatter::Document;
use crate::fsutil;
use crate::{Finding, FindingStatus, OpsError, Result, Severity};

const TEMPLATE: &str = include_str!("../templates/finding.md");

pub(crate) fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Low => "low",
        Severity::Medium => "medium",
        Severity::High => "high",
        Severity::Critical => "critical",
    }
}

pub(crate) fn severity_from(name: &str) -> Option<Severity> {
    [
        Severity::Info,
        Severity::Low,
        Severity::Medium,
        Severity::High,
        Severity::Critical,
    ]
    .into_iter()
    .find(|s| severity_name(*s) == name)
}

pub(crate) fn status_label(status: FindingStatus) -> &'static str {
    match status {
        FindingStatus::Draft => "Draft",
        FindingStatus::Confirmed => "Confirmed",
        FindingStatus::Reported => "Reported",
        FindingStatus::Fixed => "Fixed",
        FindingStatus::Accepted => "Accepted",
    }
}

fn status_name(status: FindingStatus) -> &'static str {
    match status {
        FindingStatus::Draft => "draft",
        FindingStatus::Confirmed => "confirmed",
        FindingStatus::Reported => "reported",
        FindingStatus::Fixed => "fixed",
        FindingStatus::Accepted => "accepted",
    }
}

fn status_from(name: &str) -> Option<FindingStatus> {
    [
        FindingStatus::Draft,
        FindingStatus::Confirmed,
        FindingStatus::Reported,
        FindingStatus::Fixed,
        FindingStatus::Accepted,
    ]
    .into_iter()
    .find(|s| status_name(*s) == name)
}

fn number(id: &str) -> Option<u32> {
    id.strip_prefix("F-")?.parse().ok()
}

fn id_from_file_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    let candidate: String = stem
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let mut parts = candidate.splitn(3, '-');
    match (parts.next(), parts.next()) {
        (Some("F"), Some(n)) if n.parse::<u32>().is_ok() => Some(format!("F-{n}")),
        _ => None,
    }
}

fn read_finding(path: &Path) -> Result<Finding> {
    let doc = Document::parse(&fsutil::read(path)?);
    let parse_error = |message: String| OpsError::Parse {
        file: path.to_path_buf(),
        message,
    };
    let id = doc
        .scalar("id")
        .or_else(|| id_from_file_name(path))
        .ok_or_else(|| {
            parse_error("missing identifier (`id` key or `F-001` file name prefix)".into())
        })?;
    let severity = match doc.scalar("severity") {
        None => Severity::Info,
        Some(s) => {
            severity_from(&s).ok_or_else(|| parse_error(format!("unknown severity: {s}")))?
        }
    };
    let status = match doc.scalar("status") {
        None => FindingStatus::Draft,
        Some(s) => {
            status_from(&s).ok_or_else(|| parse_error(format!("unknown finding status: {s}")))?
        }
    };
    let title = doc.scalar("title").unwrap_or_else(|| {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        stem.splitn(3, '-')
            .nth(2)
            .map(str::to_string)
            .unwrap_or(stem)
    });
    Ok(Finding {
        path: fsutil::absolute(path),
        id,
        title,
        severity,
        cvss_vector: doc.scalar("cvss"),
        cvss_score: doc.scalar("cvss_score").and_then(|s| s.parse().ok()),
        status,
        assets: doc.list("assets"),
        cwe: doc
            .list("cwe")
            .into_iter()
            .map(|v| crate::cwe::normalize(&v).unwrap_or(v))
            .collect(),
        attack: doc.list("attack"),
    })
}

fn read_all(engagement_dir: &Path) -> Vec<Finding> {
    let Ok(entries) = std::fs::read_dir(engagement_dir.join("Findings")) else {
        return Vec::new();
    };
    entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "md")
                && !p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .filter_map(|p| read_finding(&p).ok())
        .collect()
}

pub fn list(engagement_dir: &Path) -> Vec<Finding> {
    let mut findings = read_all(engagement_dir);
    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| number(&a.id).cmp(&number(&b.id)))
            .then_with(|| a.id.cmp(&b.id))
    });
    findings
}

pub fn create(engagement_dir: &Path, title: &str, severity: Severity) -> Result<Finding> {
    let title = title.trim();
    if title.is_empty() {
        return Err(OpsError::Invalid("the finding title is empty".into()));
    }
    let used = std::fs::read_dir(engagement_dir.join("Findings"))
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .filter_map(|e| id_from_file_name(&e.path()).and_then(|id| number(&id)))
        .chain(
            read_all(engagement_dir)
                .iter()
                .filter_map(|f| number(&f.id)),
        )
        .max()
        .unwrap_or(0);
    let id = format!("F-{:03}", used + 1);
    let stem: String = fsutil::file_name(title).chars().take(80).collect();
    let path = engagement_dir
        .join("Findings")
        .join(format!("{id}-{}.md", stem.trim()));
    let mut doc = Document::parse(&TEMPLATE.replace("{title}", title));
    doc.set_scalar("id", Some(&id));
    doc.set_scalar("title", Some(title));
    doc.set_scalar("severity", Some(severity_name(severity)));
    doc.set_scalar("status", Some(status_name(FindingStatus::Draft)));
    doc.set_list("assets", &[]);
    doc.set_list("attack", &[]);
    fsutil::write_atomic(&path, &doc.render())?;
    read_finding(&path)
}

pub struct Body {
    pub text: String,
    pub modified: SystemTime,
}

fn modified(path: &Path) -> Result<SystemTime> {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map_err(|source| OpsError::Io {
            path: path.to_path_buf(),
            source,
        })
}

pub fn read_body(path: &Path) -> Result<Body> {
    let modified = modified(path)?;
    let text = fsutil::read(path)?;
    Ok(Body {
        text: Document::parse(&text).body().replace("\r\n", "\n"),
        modified,
    })
}

pub fn save_body(path: &Path, body: &str, expected: SystemTime) -> Result<SystemTime> {
    if modified(path)? != expected {
        return Err(OpsError::Conflict(path.to_path_buf()));
    }
    let current = fsutil::read(path)?;
    let mut doc = Document::parse(&current);
    let body = body.replace("\r\n", "\n");
    doc.set_body(if current.contains("\r\n") {
        body.replace('\n', "\r\n")
    } else {
        body
    });
    fsutil::write_atomic(path, &doc.render())?;
    modified(path)
}

pub fn save(finding: &Finding) -> Result<()> {
    let mut doc = Document::parse(&fsutil::read(&finding.path)?);
    doc.set_scalar("id", Some(&finding.id));
    doc.set_scalar("title", Some(&finding.title));
    doc.set_scalar("severity", Some(severity_name(finding.severity)));
    doc.set_scalar("cvss", finding.cvss_vector.as_deref());
    doc.set_scalar(
        "cvss_score",
        finding.cvss_score.map(|s| format!("{s:.1}")).as_deref(),
    );
    doc.set_scalar("status", Some(status_name(finding.status)));
    doc.set_list("assets", &finding.assets);
    if finding.cwe.is_empty() {
        doc.set_scalar("cwe", None);
    } else {
        doc.set_list("cwe", &finding.cwe);
    }
    doc.set_list("attack", &finding.attack);
    fsutil::write_atomic(&finding.path, &doc.render())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_is_saved_under_its_frontmatter_unless_the_file_changed() {
        let dir = fsutil::test_dir("finding-body");
        let finding = create(&dir, "Editable", Severity::High).unwrap();
        let read = read_body(&finding.path).unwrap();
        assert!(read.text.contains("## Description"));
        let first = save_body(
            &finding.path,
            "# Editable\n\n## Description\n\nWritten in the sheet.\n",
            read.modified,
        )
        .unwrap();
        let text = std::fs::read_to_string(&finding.path).unwrap();
        assert!(
            text.starts_with("---\n")
                && text.contains("severity: high")
                && text.contains("Written in the sheet."),
            "{text}"
        );
        assert_eq!(
            list(&dir)[0].title,
            "Editable",
            "the frontmatter is untouched"
        );
        std::fs::write(&finding.path, format!("{text}Added elsewhere.\n")).unwrap();
        match save_body(&finding.path, "overwrite", first) {
            Err(OpsError::Conflict(path)) => assert_eq!(path, finding.path),
            other => panic!("{other:?}"),
        }
        assert!(
            std::fs::read_to_string(&finding.path)
                .unwrap()
                .contains("Added elsewhere."),
            "a conflict writes nothing"
        );
    }

    #[test]
    fn a_body_saved_into_a_crlf_note_keeps_its_line_ends() {
        let dir = fsutil::test_dir("finding-body-crlf");
        let finding = create(&dir, "Windows", Severity::Low).unwrap();
        let crlf = std::fs::read_to_string(&finding.path)
            .unwrap()
            .replace('\n', "\r\n");
        std::fs::write(&finding.path, &crlf).unwrap();
        let read = read_body(&finding.path).unwrap();
        assert!(!read.text.contains('\r'));
        save_body(&finding.path, "one\ntwo\n", read.modified).unwrap();
        let text = std::fs::read_to_string(&finding.path).unwrap();
        assert!(
            text.ends_with("one\r\ntwo\r\n") && !text.replace("\r\n", "").contains('\n'),
            "{text:?}"
        );
    }

    #[test]
    fn create_numbers_findings_and_list_sorts_by_severity() {
        let dir = fsutil::test_dir("findings");
        let a = create(&dir, "Exposed admin interface", Severity::Medium).unwrap();
        let b = create(&dir, "Default password", Severity::Critical).unwrap();
        let c = create(&dir, "Verbose banner", Severity::Info).unwrap();
        let d = create(&dir, "Another critical one", Severity::Critical).unwrap();
        assert_eq!(
            [a.id.as_str(), b.id.as_str(), c.id.as_str(), d.id.as_str()],
            ["F-001", "F-002", "F-003", "F-004"]
        );
        assert!(
            a.path
                .ends_with("Findings/F-001-Exposed admin interface.md")
        );
        let ids: Vec<String> = list(&dir).into_iter().map(|f| f.id).collect();
        assert_eq!(ids, ["F-002", "F-004", "F-001", "F-003"]);
        let text = std::fs::read_to_string(&b.path).unwrap();
        for section in [
            "## Description",
            "## Evidence",
            "## Impact",
            "## Remediation",
            "## References",
        ] {
            assert!(text.contains(section), "{section}");
        }
        assert!(matches!(
            create(&dir, "  ", Severity::Low),
            Err(OpsError::Invalid(_))
        ));
    }

    #[test]
    fn save_rewrites_frontmatter_only() {
        let dir = fsutil::test_dir("findings-save");
        let mut finding = create(&dir, "Finding", Severity::Low).unwrap();
        let mut text = std::fs::read_to_string(&finding.path).unwrap();
        text = text
            .replacen("---\n", "---\ncustom_key: keep me\n", 1)
            .replace("## Evidence\n", "## Evidence\n\nScreenshot attached.\n");
        std::fs::write(&finding.path, text).unwrap();
        finding.severity = Severity::High;
        finding.status = FindingStatus::Confirmed;
        finding.cvss_vector = Some("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N".into());
        finding.cvss_score = Some(7.5);
        finding.assets = vec!["192.0.2.5".into(), "srv-web".into()];
        finding.cwe = vec!["CWE-200".into(), "CWE-79".into()];
        finding.attack = vec!["T1000".into()];
        save(&finding).unwrap();
        let after = std::fs::read_to_string(&finding.path).unwrap();
        assert!(
            after.contains("custom_key: keep me\n") && after.contains("Screenshot attached.\n")
        );
        let reloaded = list(&dir).remove(0);
        assert_eq!(
            (reloaded.severity, reloaded.status, reloaded.cvss_score),
            (Severity::High, FindingStatus::Confirmed, Some(7.5))
        );
        assert_eq!(reloaded.assets, ["192.0.2.5", "srv-web"]);
        assert_eq!(
            (reloaded.cwe.clone(), reloaded.attack.clone()),
            (
                vec!["CWE-200".to_string(), "CWE-79".to_string()],
                vec!["T1000".to_string()]
            )
        );
        assert_eq!(
            reloaded.cvss_vector.as_deref(),
            Some("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N")
        );
    }

    #[test]
    fn hand_written_notes_are_read_and_ids_never_reused() {
        let dir = fsutil::test_dir("findings-manual");
        std::fs::create_dir_all(dir.join("Findings")).unwrap();
        std::fs::write(
            dir.join("Findings/F-007-Typed by hand.md"),
            "---\nseverity: high\n---\n# Body\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("Findings/F-009-Unreadable.md"),
            "---\nseverity: huge\n---\n",
        )
        .unwrap();
        let listed = list(&dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(
            (listed[0].id.as_str(), listed[0].title.as_str()),
            ("F-007", "Typed by hand")
        );
        assert_eq!(
            create(&dir, "Next", Severity::Info).unwrap().id,
            "F-010",
            "the number of an unreadable note stays reserved"
        );
    }
}
