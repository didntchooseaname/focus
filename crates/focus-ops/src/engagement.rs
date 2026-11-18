use std::path::Path;

use crate::frontmatter::Document;
use crate::fsutil;
use crate::{Engagement, EngagementKind, EngagementStatus, NewEngagement, OpsError, Result};

const README_PENTEST: &str = include_str!("../templates/readme-pentest.md");
const README_LAB: &str = include_str!("../templates/readme-lab.md");
const README_EXAM: &str = include_str!("../templates/readme-exam.md");

pub(crate) fn kind_name(kind: EngagementKind) -> &'static str {
    match kind {
        EngagementKind::ExternalPentest => "external-pentest",
        EngagementKind::InternalPentest => "internal-pentest",
        EngagementKind::WebPentest => "web-pentest",
        EngagementKind::RedTeam => "red-team",
        EngagementKind::Lab => "lab",
        EngagementKind::Exam => "exam",
    }
}

fn kind_from(name: &str) -> Option<EngagementKind> {
    [
        EngagementKind::ExternalPentest,
        EngagementKind::InternalPentest,
        EngagementKind::WebPentest,
        EngagementKind::RedTeam,
        EngagementKind::Lab,
        EngagementKind::Exam,
    ]
    .into_iter()
    .find(|k| kind_name(*k) == name)
}

pub(crate) fn status_name(status: EngagementStatus) -> &'static str {
    match status {
        EngagementStatus::Planning => "planning",
        EngagementStatus::InProgress => "in-progress",
        EngagementStatus::Reporting => "reporting",
        EngagementStatus::Done => "done",
    }
}

fn status_from(name: &str) -> Option<EngagementStatus> {
    [
        EngagementStatus::Planning,
        EngagementStatus::InProgress,
        EngagementStatus::Reporting,
        EngagementStatus::Done,
    ]
    .into_iter()
    .find(|s| status_name(*s) == name)
}

pub(crate) fn default_checklist(kind: EngagementKind) -> &'static str {
    match kind {
        EngagementKind::Exam => "exam",
        _ => "mission",
    }
}

fn parse_error(file: &Path, message: impl Into<String>) -> OpsError {
    OpsError::Parse {
        file: file.to_path_buf(),
        message: message.into(),
    }
}

pub fn load(engagement_dir: &Path) -> Result<Engagement> {
    let dir = fsutil::absolute(engagement_dir);
    let readme = dir.join("README.md");
    let text = fsutil::read(&readme)?;
    let doc = Document::parse(&text);
    let kind_text = doc
        .scalar("type")
        .ok_or_else(|| parse_error(&readme, "missing `type` key in the frontmatter"))?;
    let kind = kind_from(&kind_text)
        .ok_or_else(|| parse_error(&readme, format!("unknown engagement type: {kind_text}")))?;
    let status = match doc.scalar("status") {
        None => EngagementStatus::Planning,
        Some(s) => {
            status_from(&s).ok_or_else(|| parse_error(&readme, format!("unknown status: {s}")))?
        }
    };
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut engagement = Engagement {
        dir,
        name,
        kind,
        status,
        client: doc.scalar("client"),
        started: doc.scalar("started"),
        deadline: doc.scalar("deadline"),
        scope: doc.list("scope"),
        out_of_scope: doc.list("out_of_scope"),
        vars: doc.map("vars"),
        var_links: doc.map("var_links"),
        pauses: doc.list("pauses"),
        stopped: doc.scalar("stopped"),
        exam: doc.scalar("exam"),
        exam_done: doc.list("exam_done"),
        focus: doc.scalar("focus"),
    };
    let credentials = crate::loot::list(&engagement.dir);
    crate::links::follow(&mut engagement, &credentials);
    Ok(engagement)
}

pub fn list(ops_dir: &Path) -> Vec<Engagement> {
    let Ok(entries) = std::fs::read_dir(ops_dir) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, Engagement)> = entries
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let engagement = load(&e.path()).ok()?;
            let modified = ["README.md", "Journal.md"]
                .iter()
                .filter_map(|f| {
                    std::fs::metadata(e.path().join(f))
                        .and_then(|m| m.modified())
                        .ok()
                })
                .max()
                .unwrap_or(std::time::UNIX_EPOCH);
            Some((modified, engagement))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    found.into_iter().map(|(_, e)| e).collect()
}

pub fn create(ops_dir: &Path, spec: &NewEngagement) -> Result<Engagement> {
    let name = fsutil::file_name(&spec.name);
    if name.is_empty() {
        return Err(OpsError::Invalid("the engagement name is empty".into()));
    }
    let dir = fsutil::absolute(&ops_dir.join(&name));
    if dir.exists() {
        return Err(OpsError::AlreadyExists(dir));
    }
    let checklist_id = spec
        .checklist
        .as_deref()
        .unwrap_or_else(|| default_checklist(spec.kind));
    let template = crate::checklist::templates()
        .iter()
        .find(|t| t.id == checklist_id)
        .ok_or_else(|| OpsError::Invalid(format!("unknown checklist template: {checklist_id}")))?;
    if let Some(exam) = &spec.exam
        && !crate::exam::profiles().iter().any(|p| p.id == exam)
    {
        return Err(OpsError::Invalid(format!("unknown exam profile: {exam}")));
    }

    for sub in ["Hosts", "Findings", "Loot", "Evidence"] {
        fsutil::create_dir(&dir.join(sub))?;
    }
    let body = match spec.kind {
        EngagementKind::Lab => README_LAB,
        EngagementKind::Exam => README_EXAM,
        _ => README_PENTEST,
    }
    .replace("{name}", &name);
    let mut doc = Document::parse(&body);
    doc.set_scalar("type", Some(kind_name(spec.kind)));
    doc.set_scalar("status", Some(status_name(EngagementStatus::Planning)));
    doc.set_scalar(
        "client",
        spec.client.as_deref().filter(|c| !c.trim().is_empty()),
    );
    doc.set_scalar("started", None);
    doc.set_scalar("deadline", None);
    doc.set_list("scope", &spec.scope);
    doc.set_list("out_of_scope", &[]);
    doc.set_map("vars", &Default::default());
    doc.set_scalar("exam", spec.exam.as_deref());
    fsutil::write_atomic(&dir.join("README.md"), &doc.render())?;
    fsutil::write_atomic(&dir.join("Journal.md"), "# Journal\n\n")?;
    fsutil::write_atomic(&dir.join("Checklist.md"), template.markdown)?;
    fsutil::write_private(
        &dir.join("Loot").join("Credentials.md"),
        &crate::loot::empty_file(),
    )?;
    load(&dir)
}

pub fn save(engagement: &Engagement) -> Result<()> {
    let readme = engagement.dir.join("README.md");
    let mut doc = Document::parse(&fsutil::read(&readme)?);
    doc.set_scalar("type", Some(kind_name(engagement.kind)));
    doc.set_scalar("status", Some(status_name(engagement.status)));
    doc.set_scalar("client", engagement.client.as_deref());
    doc.set_scalar("started", engagement.started.as_deref());
    doc.set_scalar("deadline", engagement.deadline.as_deref());
    doc.set_list("scope", &engagement.scope);
    doc.set_list("out_of_scope", &engagement.out_of_scope);
    doc.set_map("vars", &engagement.vars);
    if engagement.var_links.is_empty() {
        doc.set_scalar("var_links", None);
    } else {
        doc.set_map("var_links", &engagement.var_links);
    }
    if engagement.pauses.is_empty() {
        doc.set_scalar("pauses", None);
    } else {
        doc.set_list("pauses", &engagement.pauses);
    }
    doc.set_scalar("stopped", engagement.stopped.as_deref());
    doc.set_scalar("exam", engagement.exam.as_deref());
    doc.set_list("exam_done", &engagement.exam_done);
    doc.set_scalar("focus", engagement.focus.as_deref());
    fsutil::write_atomic(&readme, &doc.render())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, kind: EngagementKind) -> NewEngagement {
        NewEngagement {
            name: name.into(),
            kind,
            client: Some("Example Corp".into()),
            scope: vec!["198.51.100.0/24".into(), "example.test".into()],
            exam: None,
            checklist: None,
        }
    }

    #[test]
    fn create_load_save_roundtrip_keeps_body_and_unknown_keys() {
        let ops = fsutil::test_dir("engagement");
        let created = create(&ops, &spec("Mission A", EngagementKind::ExternalPentest)).unwrap();
        for path in [
            "README.md",
            "Journal.md",
            "Checklist.md",
            "Loot/Credentials.md",
            "Hosts",
            "Findings",
            "Evidence",
        ] {
            assert!(created.dir.join(path).exists(), "{path}");
        }
        assert_eq!(created.name, "Mission A");
        assert_eq!(created.status, EngagementStatus::Planning);
        assert_eq!(created.scope, ["198.51.100.0/24", "example.test"]);
        assert!(matches!(
            create(&ops, &spec("Mission A", EngagementKind::Lab)),
            Err(OpsError::AlreadyExists(_))
        ));

        let readme = created.dir.join("README.md");
        let text = std::fs::read_to_string(&readme).unwrap().replacen(
            "---\n",
            "---\nmy_field: value to keep\n",
            1,
        ) + "\nFree text added.\n";
        std::fs::write(&readme, text).unwrap();

        let mut engagement = load(&created.dir).unwrap();
        engagement.status = EngagementStatus::InProgress;
        engagement.started = Some("2026-11-12".into());
        engagement.vars.insert("LHOST".into(), "192.0.2.10".into());
        engagement.out_of_scope.push("198.51.100.99".into());
        engagement.focus = Some("srv-web".into());
        engagement.exam_done =
            vec!["ops-exam-oscp-machine-1/ops-exam-milestone-initial-access".into()];
        save(&engagement).unwrap();

        let after = std::fs::read_to_string(&readme).unwrap();
        assert!(
            after.contains("my_field: value to keep\n")
                && after.contains("Free text added.\n")
                && after.contains("## Scope\n")
        );
        let reloaded = load(&created.dir).unwrap();
        assert_eq!(reloaded.status, EngagementStatus::InProgress);
        assert_eq!(
            reloaded.vars.get("LHOST").map(String::as_str),
            Some("192.0.2.10")
        );
        assert_eq!(reloaded.out_of_scope, ["198.51.100.99"]);
        assert_eq!(reloaded.focus.as_deref(), Some("srv-web"));
        assert_eq!(
            reloaded.exam_done,
            ["ops-exam-oscp-machine-1/ops-exam-milestone-initial-access"]
        );
        assert_eq!(reloaded.client.as_deref(), Some("Example Corp"));
    }

    #[test]
    #[cfg(unix)]
    fn credentials_are_readable_by_their_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let ops = fsutil::test_dir("engagement-private");
        let created = create(&ops, &spec("Private", EngagementKind::Lab)).unwrap();
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&created.dir.join("Loot")), 0o700);
        assert_eq!(mode(&created.dir.join("Loot/Credentials.md")), 0o600);
        crate::loot::add(
            &created.dir,
            &crate::Credential {
                user: "u".into(),
                kind: "password".into(),
                secret: "s3cret-value".into(),
                domain: String::new(),
                source: String::new(),
                valid_on: String::new(),
                notes: String::new(),
            },
        )
        .unwrap();
        assert_eq!(
            mode(&created.dir.join("Loot/Credentials.md")),
            0o600,
            "a rewrite keeps the rights"
        );
    }

    #[test]
    fn list_orders_by_recent_modification_and_skips_foreign_folders() {
        let ops = fsutil::test_dir("engagement-list");
        create(&ops, &spec("Older", EngagementKind::Lab)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        create(&ops, &spec("Newer", EngagementKind::Exam)).unwrap();
        std::fs::create_dir_all(ops.join("Other folder")).unwrap();
        let names: Vec<String> = list(&ops).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Newer", "Older"]);
        assert!(load(&ops.join("Other folder")).is_err());
    }

    #[test]
    fn invalid_specs_are_refused_and_load_explains_why() {
        let ops = fsutil::test_dir("engagement-invalid");
        assert!(matches!(
            create(&ops, &spec("  ", EngagementKind::Lab)),
            Err(OpsError::Invalid(_))
        ));
        let mut bad = spec("X", EngagementKind::Exam);
        bad.exam = Some("missing".into());
        assert!(matches!(create(&ops, &bad), Err(OpsError::Invalid(_))));
        let mut bad = spec("Y", EngagementKind::Lab);
        bad.checklist = Some("missing".into());
        assert!(matches!(create(&ops, &bad), Err(OpsError::Invalid(_))));
        assert!(
            !ops.join("X").exists() && !ops.join("Y").exists(),
            "nothing is created when the request is refused"
        );
        std::fs::create_dir_all(ops.join("Z")).unwrap();
        std::fs::write(ops.join("Z/README.md"), "---\ntype: unknown\n---\n").unwrap();
        assert!(matches!(load(&ops.join("Z")), Err(OpsError::Parse { .. })));
        assert!(matches!(
            load(&ops.join("absent")),
            Err(OpsError::Io { .. })
        ));
    }

    #[test]
    fn names_are_made_safe() {
        let ops = fsutil::test_dir("engagement-name");
        let created = create(&ops, &spec("client/../evil: test?", EngagementKind::Lab)).unwrap();
        assert!(created.dir.starts_with(fsutil::absolute(&ops)));
        assert_eq!(created.name, "client-..-evil- test-");
    }
}
