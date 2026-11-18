use std::path::Path;

use focus_ops::{
    Credential, Engagement, EngagementKind, EngagementStatus, Finding, FindingStatus, Host,
    HostStatus, Severity,
};
use serde_json::{Map, Value, json};

use crate::design::Fields;
use crate::finding::{self, Sections};
use crate::ids::uuid;
use crate::images::{Image, Images, Outcome};
use crate::mask::{MASK, Masker};
use crate::summary::{Kind, Summary};
use crate::text;
use crate::{Error, Options, Result, Secrets, cwe};

pub(crate) struct Prepared {
    pub(crate) engagement: Engagement,
    pub(crate) findings: Vec<PreparedFinding>,
    pub(crate) hosts: Vec<Host>,
    pub(crate) proofs: Vec<HostProofs>,
    pub(crate) credentials: Vec<Credential>,
    pub(crate) images: Vec<Image>,
    pub(crate) summary: Summary,
}

pub(crate) struct HostProofs {
    pub(crate) host: String,
    pub(crate) rows: Vec<ProofRow>,
}

pub(crate) struct ProofRow {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) taken: String,
    pub(crate) image: Option<String>,
}

pub(crate) struct PreparedFinding {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) severity: Severity,
    pub(crate) status: FindingStatus,
    pub(crate) cvss: Option<String>,
    pub(crate) cwe: Option<String>,
    pub(crate) assets: Vec<String>,
    pub(crate) attack: Vec<String>,
    pub(crate) sections: Sections,
    pub(crate) references: Vec<String>,
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

fn status_name(status: FindingStatus) -> &'static str {
    match status {
        FindingStatus::Draft => "draft",
        FindingStatus::Confirmed => "confirmed",
        FindingStatus::Reported => "reported",
        FindingStatus::Fixed => "fixed",
        FindingStatus::Accepted => "accepted",
    }
}

pub(crate) fn review_status(status: FindingStatus) -> &'static str {
    if status == FindingStatus::Draft {
        "in-progress"
    } else {
        "finished"
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

fn status_label(status: EngagementStatus) -> &'static str {
    match status {
        EngagementStatus::Planning => "Planning",
        EngagementStatus::InProgress => "In progress",
        EngagementStatus::Reporting => "Reporting",
        EngagementStatus::Done => "Done",
    }
}

fn host_status(status: HostStatus) -> &'static str {
    match status {
        HostStatus::New => "new",
        HostStatus::Enum => "enum",
        HostStatus::Foothold => "foothold",
        HostStatus::User => "user",
        HostStatus::Owned => "owned",
    }
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

fn date_part(text: &str) -> Option<String> {
    let date = text.trim().get(..10)?;
    let bytes = date.as_bytes();
    let shaped = bytes.iter().enumerate().all(|(i, b)| {
        if i == 4 || i == 7 {
            *b == b'-'
        } else {
            b.is_ascii_digit()
        }
    });
    shaped.then(|| date.to_string())
}

fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86_400) as i64)
        .unwrap_or(0);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn read_finding(
    engagement: &Engagement,
    finding: &Finding,
    masker: &mut Masker,
    images: &mut Images,
    summary: &mut Summary,
) -> Result<PreparedFinding> {
    let raw = std::fs::read_to_string(&finding.path).map_err(|source| Error::Io {
        path: finding.path.clone(),
        source,
    })?;
    let note_dir = finding
        .path
        .parent()
        .unwrap_or(&engagement.dir)
        .to_path_buf();
    let sections = finding::split_sections(&finding::body_of(&raw));
    let subject = finding.id.as_str();
    let mut report = text::Report::default();
    let mut convert = |section: &str, masker: &mut Masker| {
        let masked = masker.text(section);
        finding::converted(&masked, images, &note_dir, &mut report)
    };
    let converted = Sections {
        description: convert(&sections.description, masker),
        evidence: convert(&sections.evidence, masker),
        impact: convert(&sections.impact, masker),
        remediation: convert(&sections.remediation, masker),
        references: String::new(),
        others: sections
            .others
            .iter()
            .map(|(heading, body)| (masker.text(heading), convert(body, masker)))
            .collect(),
    };
    let references = finding::references(&masker.text(&sections.references));
    for name in &report.missing_images {
        summary.note(Kind::ImageMissing, Some(subject), name.clone());
    }
    for (name, reason) in &report.rejected_images {
        summary.note(
            Kind::ImageRefused,
            Some(subject),
            format!("{name}: {reason}"),
        );
    }
    for name in &report.note_embeds {
        summary.note(Kind::NoteEmbed, Some(subject), name.clone());
    }
    if report.remote_images > 0 {
        summary.note_count(Kind::ImageRemote, Some(subject), report.remote_images);
    }
    if report.comments > 0 {
        summary.note_count(Kind::CommentsRemoved, Some(subject), report.comments);
    }

    let cvss = match finding
        .cvss_vector
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        None => None,
        Some(vector) => match focus_ops::cvss::score(vector) {
            Ok(_) => Some(vector.to_string()),
            Err(reason) => {
                summary.note(
                    Kind::CvssNotUsable,
                    Some(subject),
                    format!("{vector}: {reason}"),
                );
                None
            }
        },
    };
    let mut cwe_value = None;
    for written in finding
        .cwe
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
    {
        match cwe::number(written) {
            Some(number) if cwe::is_listed(number) && cwe_value.is_none() => {
                cwe_value = Some(cwe::value(number))
            }
            Some(number) if cwe::is_listed(number) => {
                summary.note(Kind::CweExtra, Some(subject), cwe::value(number))
            }
            _ => summary.note(Kind::CweNotListed, Some(subject), written.to_string()),
        }
    }
    Ok(PreparedFinding {
        id: finding.id.clone(),
        title: masker.text(&finding.title),
        severity: finding.severity,
        status: finding.status,
        cvss,
        cwe: cwe_value,
        assets: finding.assets.iter().map(|a| masker.text(a)).collect(),
        attack: finding.attack.clone(),
        sections: converted,
        references,
    })
}

pub(crate) fn prepare(engagement: &Engagement, options: &Options) -> Result<(Prepared, Masker)> {
    let credentials = focus_ops::loot::list(&engagement.dir);
    let mut masker = Masker::new(options.secrets == Secrets::Masked, &engagement.dir);
    let mut summary = Summary::default();
    let mut images = Images::new(&engagement.dir);
    let list = focus_ops::findings::list(&engagement.dir);
    let mut findings = Vec::with_capacity(list.len());
    for finding in &list {
        findings.push(read_finding(
            engagement,
            finding,
            &mut masker,
            &mut images,
            &mut summary,
        )?);
    }
    let hosts = focus_ops::hosts::list(&engagement.dir);
    let mut proofs = Vec::new();
    for host in &hosts {
        let label = host
            .hostname
            .as_deref()
            .map_or_else(|| host.ip.clone(), |n| format!("{n} ({})", host.ip));
        let rows = read_proofs(
            engagement,
            host,
            &label,
            &mut masker,
            &mut images,
            &mut summary,
        )?;
        if !rows.is_empty() {
            summary.proofs += rows.len();
            proofs.push(HostProofs {
                host: masker.text(&label),
                rows,
            });
        }
    }
    let mut hosts_masked = Vec::with_capacity(hosts.len());
    for host in hosts {
        hosts_masked.push(Host {
            hostname: host.hostname.as_deref().map(|h| masker.text(h)),
            os: host.os.as_deref().map(|o| masker.text(o)),
            ..host
        });
    }
    let attached = images.into_attached();
    summary.findings = findings.len();
    summary.images = attached.len();

    let journal = focus_ops::journal::read(&engagement.dir).len();
    if journal > 0 {
        summary.note_count(Kind::Journal, None, journal);
    }
    let checklist = std::fs::read_to_string(engagement.dir.join("Checklist.md"))
        .map(|t| {
            t.lines()
                .filter(|l| l.trim_start().starts_with("- ["))
                .count()
        })
        .unwrap_or(0);
    if checklist > 0 {
        summary.note_count(Kind::Checklist, None, checklist);
    }
    if !engagement.exam_done.is_empty() {
        summary.note_count(Kind::ExamMilestones, None, engagement.exam_done.len());
    }
    if !engagement.vars.is_empty() {
        summary.note_count(Kind::Variables, None, engagement.vars.len());
    }
    let notes = hosts_masked
        .iter()
        .filter(|h| !h.path.as_os_str().is_empty() && host_has_note_text(&h.path))
        .count();
    if notes > 0 {
        summary.note_count(Kind::HostNotes, None, notes);
    }
    if !options.credentials && !credentials.is_empty() {
        summary.note_count(Kind::Credentials, None, credentials.len());
    }
    Ok((
        Prepared {
            engagement: engagement.clone(),
            findings,
            hosts: hosts_masked,
            proofs,
            credentials,
            images: attached,
            summary,
        },
        masker,
    ))
}

fn read_proofs(
    engagement: &Engagement,
    host: &Host,
    label: &str,
    masker: &mut Masker,
    images: &mut Images,
    summary: &mut Summary,
) -> Result<Vec<ProofRow>> {
    if host.path.as_os_str().is_empty() {
        return Ok(Vec::new());
    }
    let mut rows = Vec::new();
    for proof in focus_ops::proofs::read(&host.path)? {
        let image = if proof.capture.is_empty() {
            None
        } else {
            match images.resolve(&proof.capture, &engagement.dir, false) {
                Outcome::Attached(name) => Some(name),
                Outcome::NotFound => {
                    summary.note(Kind::ImageMissing, Some(label), proof.capture.clone());
                    None
                }
                Outcome::Rejected(reason) => {
                    summary.note(
                        Kind::ImageRefused,
                        Some(label),
                        format!("{}: {reason}", proof.capture),
                    );
                    None
                }
                Outcome::Remote | Outcome::NoteEmbed => None,
            }
        };
        rows.push(ProofRow {
            name: proof.name,
            value: masker.proof_value(&proof.value),
            taken: proof.taken,
            image,
        });
    }
    Ok(rows)
}

fn host_has_note_text(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let body = finding::body_of(&text);
    body.lines()
        .any(|l| !l.trim().is_empty() && !l.trim_start().starts_with(['#', '|']))
}

fn list_markdown(title: &str, items: &[String], masker_text: impl Fn(&str) -> String) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut out = format!("**{title}**\n\n");
    for item in items {
        out.push_str(&format!("- {}\n", masker_text(item)));
    }
    out
}

fn scope_markdown(prepared: &Prepared, masker: &mut Masker) -> String {
    let engagement = &prepared.engagement;
    let included: Vec<String> = engagement.scope.iter().map(|s| masker.text(s)).collect();
    let excluded: Vec<String> = engagement
        .out_of_scope
        .iter()
        .map(|s| masker.text(s))
        .collect();
    let parts = [
        list_markdown("In scope", &included, str::to_string),
        list_markdown("Out of scope", &excluded, str::to_string),
    ];
    parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn hosts_markdown(hosts: &[Host], proofs: &[HostProofs]) -> String {
    if hosts.is_empty() {
        return String::new();
    }
    let mut out =
        String::from("| Address | Name | OS | Status | Services |\n|---|---|---|---|---|\n");
    for h in hosts {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            cell(&h.ip),
            cell(h.hostname.as_deref().unwrap_or("")),
            cell(h.os.as_deref().unwrap_or("")),
            host_status(h.status),
            h.services.len()
        ));
    }
    for h in hosts.iter().filter(|h| !h.services.is_empty()) {
        let title = h
            .hostname
            .as_deref()
            .map_or_else(|| h.ip.clone(), |n| format!("{n} ({})", h.ip));
        out.push_str(&format!("\n**{}**\n\n| Port | Proto | State | Service | Product | Version |\n|---|---|---|---|---|---|\n", cell(&title)));
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
    }
    for host in proofs {
        out.push_str(&format!(
            "\n**Proofs of {}**\n\n| Name | Value | Taken | Capture |\n|---|---|---|---|\n",
            cell(&host.host)
        ));
        for row in &host.rows {
            let capture = row.image.as_ref().map_or_else(
                || "-".to_string(),
                |name| {
                    format!(
                        "![{}](/images/name/{name})",
                        row.name.replace(['[', ']'], "")
                    )
                },
            );
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                cell(&row.name),
                cell(&row.value),
                cell(&row.taken),
                capture
            ));
        }
    }
    out
}

fn credentials_markdown(credentials: &[Credential], secrets: Secrets) -> String {
    if credentials.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "| User | Type | Secret | Domain | Source | Valid on |\n|---|---|---|---|---|---|\n",
    );
    for c in credentials {
        let secret = if secrets == Secrets::Masked {
            MASK.to_string()
        } else {
            cell(&c.secret)
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            cell(&c.user),
            cell(&c.kind),
            secret,
            cell(&c.domain),
            cell(&c.source),
            cell(&c.valid_on)
        ));
    }
    out
}

pub(crate) fn report_data(
    prepared: &Prepared,
    options: &Options,
    masker: &mut Masker,
) -> Map<String, Value> {
    let engagement = &prepared.engagement;
    let mut data = Map::new();
    data.insert("title".into(), json!(masker.text(&engagement.name)));
    if let Some(client) = engagement
        .client
        .as_deref()
        .filter(|c| !c.trim().is_empty())
    {
        data.insert("customer".into(), json!(masker.text(client)));
    }
    let scope = scope_markdown(prepared, masker);
    if !scope.is_empty() {
        data.insert("scope".into(), json!(scope));
    }
    if let Some(start) = engagement.started.as_deref().and_then(date_part) {
        data.insert("start_date".into(), json!(start));
    }
    if let Some(end) = engagement.deadline.as_deref().and_then(date_part) {
        data.insert("end_date".into(), json!(end));
    }
    let hosts = hosts_markdown(&prepared.hosts, &prepared.proofs);
    if !hosts.is_empty() {
        data.insert("hosts".into(), json!(hosts));
    }
    if options.credentials {
        let credentials = credentials_markdown(&prepared.credentials, options.secrets);
        if !credentials.is_empty() {
            data.insert("credentials".into(), json!(credentials));
        }
    }
    data.insert("report_date".into(), json!(today()));
    if !options.author.trim().is_empty() {
        data.insert("author".into(), json!(options.author.trim()));
    }
    data.insert("engagement_type".into(), json!(kind_label(engagement.kind)));
    data.insert(
        "engagement_status".into(),
        json!(status_label(engagement.status)),
    );
    data
}

fn push_section(description: &mut String, heading: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    if !description.is_empty() {
        description.push_str("\n\n");
    }
    description.push_str(&format!("### {heading}\n\n{}", text.trim()));
}

fn strings(values: &[String]) -> Value {
    Value::Array(values.iter().map(|v| json!(v)).collect())
}

pub(crate) fn finding_data(
    finding: &PreparedFinding,
    fields: &Fields,
    summary: &mut Summary,
    missing: &mut Vec<&'static str>,
) -> Map<String, Value> {
    let subject = Some(finding.id.as_str());
    let mut data = Map::new();
    data.insert(fields.title.clone(), json!(finding.title));
    data.insert(
        fields.cvss.clone(),
        json!(finding.cvss.as_deref().unwrap_or("n/a")),
    );
    let mut absent = |name: &'static str, present: bool| {
        if !present && !missing.contains(&name) {
            missing.push(name);
        }
    };
    match &fields.severity {
        Some(id) => {
            data.insert(id.clone(), json!(severity_name(finding.severity)));
        }
        None => absent("severity", false),
    }
    match (&fields.cwe, &finding.cwe) {
        (Some(id), Some(value)) => {
            data.insert(id.clone(), json!(value));
        }
        (None, Some(_)) => absent("cwe", false),
        _ => {}
    }
    match (&fields.attack, finding.attack.is_empty()) {
        (Some(id), false) => {
            data.insert(id.clone(), strings(&finding.attack));
        }
        (None, false) => absent("attack", false),
        _ => {}
    }
    match &fields.status {
        Some(id) => {
            data.insert(id.clone(), json!(status_name(finding.status)));
        }
        None => absent("status", false),
    }

    let mut description = finding.sections.description.clone();
    match &fields.affected_components {
        Some(id) => {
            data.insert(id.clone(), strings(&finding.assets));
        }
        None if !finding.assets.is_empty() => {
            let line = format!("**Affected:** {}", finding.assets.join(", "));
            description = if description.is_empty() {
                line
            } else {
                format!("{line}\n\n{description}")
            };
            summary.note(Kind::SectionMerged, subject, "affected assets");
        }
        None => {}
    }
    for (heading, id, text) in [
        ("Evidence", &fields.evidence, &finding.sections.evidence),
        ("Impact", &fields.impact, &finding.sections.impact),
        (
            "Remediation",
            &fields.recommendation,
            &finding.sections.remediation,
        ),
    ] {
        match id {
            Some(id) => {
                data.insert(id.clone(), json!(text));
            }
            None if !text.trim().is_empty() => {
                push_section(&mut description, heading, text);
                summary.note(Kind::SectionMerged, subject, heading.to_ascii_lowercase());
            }
            None => {}
        }
    }
    match &fields.references {
        Some(id) => {
            data.insert(id.clone(), strings(&finding.references));
        }
        None if !finding.references.is_empty() => {
            let list = finding
                .references
                .iter()
                .map(|r| format!("- {r}"))
                .collect::<Vec<_>>()
                .join("\n");
            push_section(&mut description, "References", &list);
            summary.note(Kind::SectionMerged, subject, "references");
        }
        None => {}
    }
    for (heading, text) in &finding.sections.others {
        push_section(&mut description, heading, text);
        summary.note(Kind::SectionMerged, subject, heading.to_ascii_lowercase());
    }
    data.insert(fields.description.clone(), json!(description));
    data
}

pub(crate) fn project_findings(
    prepared: &Prepared,
    fields: &Fields,
    summary: &mut Summary,
) -> Vec<Value> {
    let mut missing = Vec::new();
    let findings: Vec<Value> = prepared
        .findings
        .iter()
        .enumerate()
        .map(|(index, finding)| {
            json!({
                "id": uuid(&["focus-sysreptor-finding", &prepared.engagement.name, &finding.id]),
                "status": review_status(finding.status),
                "order": index + 1,
                "data": finding_data(finding, fields, summary, &mut missing),
            })
        })
        .collect();
    for name in missing {
        summary.note(Kind::NoFieldInDesign, None, name);
    }
    findings
}
