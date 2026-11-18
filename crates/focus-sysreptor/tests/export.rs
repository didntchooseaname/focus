use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use focus_ops::proofs::Proof;
use focus_ops::{
    Credential, EngagementKind, FindingStatus, Host, HostStatus, NewEngagement, Service, Severity,
};
use focus_sysreptor::{
    Fields, FileFormat, Kind, Options, Secrets, export_finding_files, export_project, write_output,
};
use serde_json::Value;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nbytes of a screenshot";
const SECRET: &str = "Zebra-Plain-77";
const PROOF: &str = "d41d8cd98f00b204e9800998ecf8427e";

fn engagement(name: &str) -> focus_ops::Engagement {
    let ops = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("sysreptor-{name}"));
    let _ = std::fs::remove_dir_all(&ops);
    std::fs::create_dir_all(&ops).unwrap();
    let created = focus_ops::create(
        &ops,
        &NewEngagement {
            name: "Orchard audit".into(),
            kind: EngagementKind::InternalPentest,
            client: Some("Example Corp".into()),
            scope: vec!["198.51.100.0/24".into(), "app.example.test".into()],
            exam: None,
            checklist: None,
        },
    )
    .unwrap();
    let mut e = focus_ops::load(&created.dir).unwrap();
    e.out_of_scope = vec!["198.51.100.250".into()];
    e.started = Some("2026-11-02T09:00:00".into());
    e.deadline = Some("2026-11-13".into());
    e.vars.insert("PASS".into(), SECRET.into());
    focus_ops::save(&e).unwrap();

    let mut web = Host {
        path: PathBuf::new(),
        ip: "198.51.100.5".into(),
        hostname: Some("web.example.test".into()),
        os: Some("Linux".into()),
        status: HostStatus::Foothold,
        services: Vec::new(),
        tags: Vec::new(),
        flags: Vec::new(),
        points: None,
    };
    web.services.push(Service {
        port: 443,
        proto: "tcp".into(),
        state: "open".into(),
        name: "https".into(),
        product: "nginx".into(),
        version: "1.24".into(),
        notes: String::new(),
    });
    let web_path = focus_ops::hosts::upsert(&e.dir, &web).unwrap();
    std::fs::write(e.dir.join("Evidence/proof.png"), PNG).unwrap();
    let mut e_mut = focus_ops::load(&e.dir).unwrap();
    focus_ops::proofs::record(
        &mut e_mut,
        &web_path,
        &Proof {
            name: "proof.txt".into(),
            value: PROOF.into(),
            taken: "2026-11-12 10:00:00".into(),
            capture: "Evidence/proof.png".into(),
        },
        None,
    )
    .unwrap();
    focus_ops::proofs::record(
        &mut e_mut,
        &web_path,
        &Proof {
            name: "user.txt".into(),
            value: "x7z".into(),
            taken: "2026-11-12 09:30:00".into(),
            capture: "Evidence/gone.png".into(),
        },
        None,
    )
    .unwrap();

    let mut f1 =
        focus_ops::findings::create(&e.dir, "SQL injection in search", Severity::Critical).unwrap();
    f1.cvss_vector = Some("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H".into());
    f1.cvss_score = Some(9.8);
    f1.cwe = vec!["CWE-89".into(), "CWE-564".into()];
    f1.assets = vec!["web.example.test".into(), "198.51.100.5".into()];
    f1.attack = vec!["T1190".into()];
    f1.status = FindingStatus::Confirmed;
    focus_ops::findings::save(&f1).unwrap();
    std::fs::write(e.dir.join("Evidence/dump.png"), PNG).unwrap();
    let note = std::fs::read_to_string(&f1.path)
        .unwrap()
        .replace("## Description\n", &format!("## Description\n\nThe `q` parameter reaches the query. See [[Host web]] %% private remark %%.\n\nPassword found: {SECRET}\n"))
        .replace("## Evidence\n", "## Evidence\n\n![[dump.png|400]]\n\n![missing](../Evidence/gone.png)\n")
        .replace("## Impact\n", "## Impact\n\nFull read access to the database.\n")
        .replace("## Remediation\n", "## Remediation\n\nUse prepared statements.\n")
        .replace("## References\n", "## References\n\n- https://owasp.org/www-community/attacks/SQL_Injection\n- Vendor bulletin 12\n")
        + "\n## Retest\n\nNot yet.\n";
    std::fs::write(&f1.path, note).unwrap();

    let mut f2 = focus_ops::findings::create(&e.dir, "Weak TLS", Severity::Low).unwrap();
    f2.cwe = vec!["CWE-99999".into()];
    f2.cvss_vector = Some("CVSS:3.0/AV:N/AC:L/PR:N/UI:N/S:U/C:L/I:N/A:N".into());
    focus_ops::findings::save(&f2).unwrap();

    focus_ops::loot::add(
        &e.dir,
        &Credential {
            user: "svc-app".into(),
            kind: "password".into(),
            secret: SECRET.into(),
            domain: "example.test".into(),
            source: "config".into(),
            valid_on: "198.51.100.5".into(),
            notes: String::new(),
        },
    )
    .unwrap();
    focus_ops::journal::append(&e.dir, Some("198.51.100.5"), "tried the credential").unwrap();
    focus_ops::load(&e.dir).unwrap()
}

fn unpack(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut files = BTreeMap::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().to_string_lossy().into_owned();
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        files.insert(name, content);
    }
    files
}

fn project_of(files: &BTreeMap<String, Vec<u8>>) -> Value {
    let (name, bytes) = files
        .iter()
        .find(|(n, _)| n.ends_with(".json") && !n.contains('/'))
        .expect("one project file at the top of the archive");
    assert!(name.len() == 41, "{name}");
    serde_json::from_slice(bytes).unwrap()
}

fn all_text(files: &BTreeMap<String, Vec<u8>>) -> String {
    files
        .values()
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .collect()
}

#[test]
fn the_archive_has_the_layout_the_import_reads() {
    let e = engagement("layout");
    let out = export_project(&e, &Options::default()).unwrap();
    assert_eq!(out.file_name, "orchard-audit-sysreptor.tar.gz");
    let files = unpack(&out.bytes);
    let project = project_of(&files);
    let id = project["id"].as_str().unwrap();
    assert_eq!(project["format"], "projects/v2");
    assert_eq!(project["name"], "Orchard audit");
    assert_eq!(project["project_type"]["format"], "projecttypes/v2");
    assert_eq!(
        project["project_type"]["assets"].as_array().unwrap().len(),
        0
    );
    assert_eq!(
        files.keys().cloned().collect::<Vec<_>>(),
        [
            format!("{id}-images/dump.png"),
            format!("{id}-images/proof.png"),
            format!("{id}.json")
        ]
    );
    assert_eq!(files[&format!("{id}-images/dump.png")], PNG);
    assert_eq!(
        project["images"],
        serde_json::json!([{"name": "dump.png"}, {"name": "proof.png"}])
    );
    let sections: Vec<&str> = project["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        sections,
        ["customer", "scope", "hosts", "appendix", "other"]
    );

    let findings = project["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 2);
    let first = &findings[0];
    assert_eq!(
        (first["order"].as_u64(), first["status"].as_str()),
        (Some(1), Some("finished"))
    );
    let data = &first["data"];
    assert_eq!(data["title"], "SQL injection in search");
    assert_eq!(data["cvss"], "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H");
    assert_eq!(
        (
            data["severity"].as_str(),
            data["cwe"].as_str(),
            data["finding_status"].as_str()
        ),
        (Some("critical"), Some("CWE-89"), Some("confirmed"))
    );
    assert_eq!(
        data["affected_components"],
        serde_json::json!(["web.example.test", "198.51.100.5"])
    );
    assert_eq!(data["attack"], serde_json::json!(["T1190"]));
    assert_eq!(
        data["references"],
        serde_json::json!([
            "https://owasp.org/www-community/attacks/SQL_Injection",
            "Vendor bulletin 12"
        ])
    );
    assert_eq!(data["impact"], "Full read access to the database.");
    assert_eq!(data["recommendation"], "Use prepared statements.");
    let evidence = data["evidence"].as_str().unwrap();
    assert!(
        evidence.contains("![dump](/images/name/dump.png)") && !evidence.contains("gone.png"),
        "{evidence}"
    );
    let description = data["description"].as_str().unwrap();
    assert!(
        description.contains("See Host web .") && !description.contains("private remark"),
        "{description}"
    );
    assert!(
        description.ends_with("### Retest\n\nNot yet."),
        "a section the design has no field for is kept in the description: {description}"
    );
    assert_eq!(findings[1]["status"], "in-progress");
    assert_eq!(findings[1]["data"]["cvss"], "n/a");
    assert!(findings[1]["data"].get("cwe").is_none());

    let report = &project["report_data"];
    assert_eq!(
        (report["title"].as_str(), report["customer"].as_str()),
        (Some("Orchard audit"), Some("Example Corp"))
    );
    assert_eq!(
        (report["start_date"].as_str(), report["end_date"].as_str()),
        (Some("2026-11-02"), Some("2026-11-13"))
    );
    assert_eq!(
        report["scope"],
        "**In scope**\n\n- 198.51.100.0/24\n- app.example.test\n\n**Out of scope**\n\n- 198.51.100.250\n"
    );
    assert!(
        report["hosts"]
            .as_str()
            .unwrap()
            .contains("| 198.51.100.5 | web.example.test | Linux | foothold | 1 |")
    );
    assert!(report.get("credentials").is_none());
}

#[test]
fn the_summary_names_what_was_left_out() {
    let e = engagement("summary");
    let summary = export_project(&e, &Options::default()).unwrap().summary;
    assert_eq!((summary.findings, summary.images), (2, 2));
    let lost: Vec<(Kind, Option<&str>)> = summary
        .left_out
        .iter()
        .map(|x| (x.kind, x.subject.as_deref()))
        .collect();
    for expected in [
        (Kind::Journal, None),
        (Kind::Checklist, None),
        (Kind::Variables, None),
        (Kind::Credentials, None),
        (Kind::ImageMissing, Some("F-001")),
        (Kind::CweNotListed, Some("F-002")),
        (Kind::CweExtra, Some("F-001")),
        (Kind::ImageMissing, Some("web.example.test (198.51.100.5)")),
        (Kind::CvssNotUsable, Some("F-002")),
    ] {
        assert!(lost.contains(&expected), "{expected:?} in {lost:?}");
    }
    let adjusted: Vec<Kind> = summary.adjusted.iter().map(|x| x.kind).collect();
    assert!(
        adjusted.contains(&Kind::SectionMerged) && adjusted.contains(&Kind::CommentsRemoved),
        "{adjusted:?}"
    );
    assert!(
        summary.to_text().contains("Left out:")
            && summary.to_text().contains("Design: Focus engagement v1")
    );
}

#[test]
fn secrets_are_masked_unless_asked_for_and_the_clear_file_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let e = engagement("secrets");
    let masked = export_project(
        &e,
        &Options {
            credentials: true,
            ..Options::default()
        },
    )
    .unwrap();
    let text = all_text(&unpack(&masked.bytes));
    assert!(
        !text.contains(SECRET) && text.contains("[masked]"),
        "a masked export holds no secret"
    );
    assert!(masked.summary.masked >= 1);

    let clear = export_project(
        &e,
        &Options {
            secrets: Secrets::Clear,
            credentials: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(
        all_text(&unpack(&clear.bytes)).contains(SECRET),
        "an explicit request writes the text as it is"
    );
    assert_eq!(clear.summary.masked, 0);

    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sysreptor-secrets-out");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("clear.tar.gz");
    std::fs::write(&path, b"old").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    write_output(&path, &clear.bytes, Secrets::Clear).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600,
        "an existing file is made private too"
    );
    assert_eq!(std::fs::read(&path).unwrap(), clear.bytes);
    let fresh = dir.join("fresh.tar.gz");
    write_output(&fresh, &clear.bytes, Secrets::Clear).unwrap();
    assert_eq!(
        std::fs::metadata(&fresh).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn finding_files_follow_the_target_design() {
    let e = engagement("files");
    let focus = export_finding_files(
        &e,
        &Options::default(),
        &Fields::focus(),
        FileFormat::Toml,
        "Focus engagement v1",
    )
    .unwrap();
    assert_eq!(
        focus
            .files
            .iter()
            .map(|f| f.file_name.as_str())
            .collect::<Vec<_>>(),
        ["F-001-sql-injection-in-search.toml", "F-002-weak-tls.toml"]
    );
    let parsed: toml::Table =
        toml::from_str(std::str::from_utf8(&focus.files[0].bytes).unwrap()).unwrap();
    assert_eq!(parsed["status"].as_str(), Some("finished"));
    let data = parsed["data"].as_table().unwrap();
    assert_eq!(
        (
            data["title"].as_str(),
            data["cwe"].as_str(),
            data["severity"].as_str()
        ),
        (
            Some("SQL injection in search"),
            Some("CWE-89"),
            Some("critical")
        )
    );
    assert_eq!(data["affected_components"].as_array().unwrap().len(), 2);
    assert_eq!(focus.images.len(), 2);
    assert!(
        focus
            .summary
            .adjusted
            .iter()
            .any(|x| x.kind == Kind::ImagesWrittenAside)
    );
    assert!(!String::from_utf8_lossy(&focus.files[0].bytes).contains(SECRET));

    let default = export_finding_files(
        &e,
        &Options::default(),
        &Fields::sysreptor_default(),
        FileFormat::Json,
        "SysReptor default",
    )
    .unwrap();
    let value: Value = serde_json::from_slice(&default.files[0].bytes).unwrap();
    let data = &value["data"];
    assert!(
        data.get("cwe").is_none()
            && data.get("severity").is_none()
            && data.get("evidence").is_none()
    );
    let description = data["description"].as_str().unwrap();
    assert!(
        description.contains("### Evidence")
            && description.contains("![dump](/images/name/dump.png)"),
        "{description}"
    );
    assert_eq!(data["impact"], "Full read access to the database.");
    let missing: Vec<&str> = default
        .summary
        .left_out
        .iter()
        .filter(|x| x.kind == Kind::NoFieldInDesign)
        .map(|x| x.detail.as_str())
        .collect();
    for field in ["severity", "cwe", "attack", "status"] {
        assert!(missing.contains(&field), "{field} in {missing:?}");
    }
}

#[test]
fn the_embedded_design_satisfies_the_rules_of_a_sysreptor_design() {
    let e = engagement("design");
    let project = project_of(&unpack(
        &export_project(&e, &Options::default()).unwrap().bytes,
    ));
    let design = &project["project_type"];
    let id_ok = |id: &str| {
        let mut chars = id.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    fn walk<'a>(field: &'a Value, out: &mut Vec<&'a Value>) {
        out.push(field);
        for property in field["properties"].as_array().into_iter().flatten() {
            walk(property, out);
        }
        if field.get("items").is_some() {
            walk(&field["items"], out);
        }
    }
    let finding_fields = design["finding_fields"].as_array().unwrap();
    let mut ids = Vec::new();
    for field in finding_fields {
        let id = field["id"].as_str().unwrap();
        assert!(id_ok(id), "{id}");
        ids.push(id);
        let mut all = Vec::new();
        walk(field, &mut all);
        for f in all {
            assert!(
                ["string", "markdown", "cvss", "cwe", "enum", "list", "date"]
                    .contains(&f["type"].as_str().unwrap())
            );
            assert!(
                f["label"].is_string()
                    && ["core", "predefined", "custom"].contains(&f["origin"].as_str().unwrap())
            );
            for choice in f["choices"].as_array().into_iter().flatten() {
                let value = choice["value"].as_str().unwrap();
                assert!(
                    value.len() >= 2
                        && value
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                    "{value}"
                );
                assert!(choice["label"].is_string());
            }
        }
    }
    let unique: std::collections::BTreeSet<_> = ids.iter().collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "finding field identifiers are unique"
    );
    assert_eq!(
        finding_fields[0]["id"], "title",
        "the core field `title` is there"
    );
    let mut section_fields = Vec::new();
    for section in design["report_sections"].as_array().unwrap() {
        assert!(
            section["id"]
                .as_str()
                .unwrap()
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        );
        for field in section["fields"].as_array().unwrap() {
            section_fields.push(field["id"].as_str().unwrap());
        }
    }
    let unique: std::collections::BTreeSet<_> = section_fields.iter().collect();
    assert_eq!(
        unique.len(),
        section_fields.len(),
        "report field identifiers are unique across sections"
    );
    assert!(section_fields.contains(&"title"));
    for order in design["finding_ordering"].as_array().unwrap() {
        assert!(
            ids.contains(&order["field"].as_str().unwrap())
                && ["asc", "desc"].contains(&order["order"].as_str().unwrap())
        );
    }
    for key in project["report_data"].as_object().unwrap().keys() {
        assert!(
            section_fields.contains(&key.as_str()),
            "report_data key {key} has no field"
        );
    }
    for finding in project["findings"].as_array().unwrap() {
        for key in finding["data"].as_object().unwrap().keys() {
            assert!(
                ids.contains(&key.as_str()),
                "finding key {key} has no field"
            );
        }
    }
    let first = &project["findings"][0];
    assert_eq!(first["id"].as_str().unwrap().len(), 36);
}

#[test]
fn an_export_of_the_same_engagement_is_the_same_archive() {
    let e = engagement("stable");
    let one = export_project(&e, &Options::default()).unwrap().bytes;
    let two = export_project(&e, &Options::default()).unwrap().bytes;
    let (a, b) = (unpack(&one), unpack(&two));
    let strip = |mut p: Value| {
        p["report_data"]
            .as_object_mut()
            .unwrap()
            .remove("report_date");
        p
    };
    assert_eq!(strip(project_of(&a)), strip(project_of(&b)));
}

#[test]
fn a_second_cwe_is_noted_apart_from_one_that_is_not_listed() {
    let e = engagement("cwe");
    let summary = export_project(&e, &Options::default()).unwrap().summary;
    let of = |kind| {
        summary
            .left_out
            .iter()
            .filter(|x| x.kind == kind)
            .map(|x| (x.subject.clone().unwrap(), x.detail.clone(), x.count))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        of(Kind::CweExtra),
        [("F-001".to_string(), "CWE-564".to_string(), 1)],
        "the first listed identifier is kept, the second is extra"
    );
    assert_eq!(
        of(Kind::CweNotListed),
        [("F-002".to_string(), "CWE-99999".to_string(), 1)]
    );
    assert_eq!(
        (Kind::CweExtra.key(), Kind::Journal.key()),
        ("cwe-extra", "journal")
    );
    assert!(
        summary
            .left_out
            .iter()
            .any(|x| x.kind == Kind::Journal && x.count == 1 && x.detail.is_empty()),
        "counts are numbers, not text"
    );
}

#[test]
fn proofs_enter_the_hosts_section_and_are_hidden_when_masked() {
    let e = engagement("proofs");
    let masked = export_project(&e, &Options::default()).unwrap();
    let files = unpack(&masked.bytes);
    let hosts = project_of(&files)["report_data"]["hosts"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        hosts.contains("**Proofs of web.example.test (198.51.100.5)**"),
        "{hosts}"
    );
    assert!(
        hosts.contains(
            "| proof.txt | [masked] | 2026-11-12 10:00:00 | ![proof.txt](/images/name/proof.png) |"
        ),
        "{hosts}"
    );
    assert!(
        hosts.contains("| user.txt | [masked] | 2026-11-12 09:30:00 | - |"),
        "a short value is hidden too: {hosts}"
    );
    assert!(!all_text(&files).contains(PROOF) && !all_text(&files).contains("x7z"));
    assert_eq!(masked.summary.proofs, 2);
    assert!(files.keys().any(|n| n.ends_with("-images/proof.png")));

    let clear = export_project(
        &e,
        &Options {
            secrets: Secrets::Clear,
            ..Options::default()
        },
    )
    .unwrap();
    let text = all_text(&unpack(&clear.bytes));
    assert!(text.contains(PROOF) && text.contains("x7z"));
}
