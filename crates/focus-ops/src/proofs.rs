use std::path::Path;

use crate::frontmatter::Document;
use crate::hosts::{replace_table, split_row};
use crate::{Engagement, OpsError, Result, exam, fsutil};

pub const NAMES: [&str; 4] = ["local.txt", "proof.txt", "user.txt", "root.txt"];

const HEADER: &str = "| Name | Value | Taken | Capture |";
const RULE: &str = "|---|---|---|---|";
const MAX_FIELD: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Proof {
    pub name: String,
    pub value: String,
    pub taken: String,
    pub capture: String,
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|")
        .replace(['\n', '\r'], " ")
        .trim()
        .to_string()
}

pub fn read(host_path: &Path) -> Result<Vec<Proof>> {
    let doc = Document::parse(&fsutil::read(host_path)?);
    let mut in_section = false;
    let mut proofs = Vec::new();
    for line in doc.body().lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("## ") {
            in_section = title.trim().eq_ignore_ascii_case("proofs");
            continue;
        }
        if !in_section || !trimmed.starts_with('|') {
            continue;
        }
        let cells = split_row(trimmed);
        let is_rule = cells
            .iter()
            .all(|c| c.chars().all(|ch| matches!(ch, '-' | ':' | ' ')));
        if is_rule
            || cells
                .first()
                .is_some_and(|c| c.eq_ignore_ascii_case("name"))
            || cells.first().is_none_or(String::is_empty)
        {
            continue;
        }
        let get = |i: usize| cells.get(i).cloned().unwrap_or_default();
        proofs.push(Proof {
            name: get(0),
            value: get(1),
            taken: get(2),
            capture: get(3),
        });
    }
    for flag in doc.list("flags") {
        if !proofs.iter().any(|p| p.name.eq_ignore_ascii_case(&flag)) {
            proofs.push(Proof {
                name: flag,
                ..Proof::default()
            });
        }
    }
    Ok(proofs)
}

fn item_for(profile: &exam::Profile, machine: usize, name: &str) -> Option<String> {
    let machine = profile.machines.get(machine)?;
    if machine.milestones.is_empty() {
        return Some(exam::item_key(machine, None));
    }
    let lower = name.to_lowercase();
    let wanted = if lower.starts_with("local") || lower.starts_with("user") {
        "initial-access"
    } else if lower.starts_with("proof") || lower.starts_with("root") {
        "privilege-escalation"
    } else {
        return None;
    };
    machine
        .milestones
        .iter()
        .find(|(key, _)| key.ends_with(wanted))
        .map(|(key, _)| exam::item_key(machine, Some(key)))
}

pub fn record(
    engagement: &mut Engagement,
    host_path: &Path,
    proof: &Proof,
    machine: Option<usize>,
) -> Result<Option<String>> {
    let name = proof.name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_FIELD
        || proof.value.trim().is_empty()
        || proof.value.chars().count() > MAX_FIELD * 10
    {
        return Err(OpsError::Invalid("a proof needs a name and a value".into()));
    }
    if !engagement.dir.join("Hosts").exists()
        || !host_path.starts_with(engagement.dir.join("Hosts"))
    {
        return Err(OpsError::Invalid(
            "the host is not in this engagement".into(),
        ));
    }
    let mut proofs = read(host_path)?;
    let entry = Proof {
        name: name.to_string(),
        value: proof.value.trim().to_string(),
        taken: proof.taken.clone(),
        capture: proof.capture.trim().to_string(),
    };
    match proofs
        .iter_mut()
        .find(|p| p.name.eq_ignore_ascii_case(name))
    {
        Some(existing) => *existing = entry,
        None => proofs.push(entry),
    }
    let mut doc = Document::parse(&fsutil::read(host_path)?);
    let mut table = vec![HEADER.to_string(), RULE.to_string()];
    table.extend(proofs.iter().map(|p| {
        format!(
            "| {} | {} | {} | {} |",
            cell(&p.name),
            cell(&p.value),
            cell(&p.taken),
            cell(&p.capture)
        )
    }));
    doc.set_body(replace_table(doc.body(), "Proofs", table));
    let names: Vec<String> = doc
        .list("flags")
        .into_iter()
        .filter(|f| !f.eq_ignore_ascii_case(name))
        .collect();
    doc.set_list("flags", &names);
    fsutil::write_private(host_path, &doc.render())?;
    let key = engagement
        .exam
        .as_deref()
        .and_then(|id| exam::profiles().iter().find(|p| p.id == id))
        .zip(machine)
        .and_then(|(profile, machine)| item_for(profile, machine, name));
    if let Some(key) = key
        .as_ref()
        .filter(|key| !engagement.exam_done.contains(key))
    {
        engagement.exam_done.push(key.clone());
        crate::save(engagement)?;
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngagementKind, Host, HostStatus, NewEngagement, hosts};

    fn lab(name: &str, exam: Option<&str>) -> (Engagement, std::path::PathBuf) {
        let ops = fsutil::test_dir(name);
        let kind = if exam.is_some() {
            EngagementKind::Exam
        } else {
            EngagementKind::Lab
        };
        let created = crate::create(
            &ops,
            &NewEngagement {
                name: "Proofs".into(),
                kind,
                client: None,
                scope: vec![],
                exam: exam.map(str::to_string),
                checklist: None,
            },
        )
        .unwrap();
        let host = Host {
            path: Default::default(),
            ip: "192.0.2.5".into(),
            hostname: Some("srv".into()),
            os: None,
            status: HostStatus::New,
            services: vec![],
            tags: vec![],
            flags: vec!["user.txt".into()],
            points: None,
        };
        let path = hosts::upsert(&created.dir, &host).unwrap();
        (crate::load(&created.dir).unwrap(), path)
    }

    #[test]
    fn a_note_with_only_flags_is_still_read() {
        let (_, path) = lab("proofs-flags", None);
        let proofs = read(&path).unwrap();
        assert_eq!(
            proofs,
            [Proof {
                name: "user.txt".into(),
                ..Proof::default()
            }]
        );
    }

    #[test]
    fn a_proof_is_written_in_its_own_section_and_replaced_by_name() {
        let (mut e, path) = lab("proofs-write", None);
        let proof = Proof {
            name: "local.txt".into(),
            value: "d41d8cd98f00b204e9800998ecf8427e".into(),
            taken: "2026-11-12T10:00:00".into(),
            capture: "Evidence/local.png".into(),
        };
        record(&mut e, &path, &proof, None).unwrap();
        record(
            &mut e,
            &path,
            &Proof {
                name: "user.txt".into(),
                value: "feedfacefeedface".into(),
                taken: "2026-11-12T10:05:00".into(),
                ..Proof::default()
            },
            None,
        )
        .unwrap();
        record(
            &mut e,
            &path,
            &Proof {
                value: "0123456789abcdef".into(),
                ..proof.clone()
            },
            None,
        )
        .unwrap();
        let proofs = read(&path).unwrap();
        assert_eq!(proofs.len(), 2);
        let by_name = |name: &str| {
            proofs
                .iter()
                .find(|p| p.name == name)
                .unwrap()
                .value
                .clone()
        };
        assert_eq!(
            (by_name("local.txt").as_str(), by_name("user.txt").as_str()),
            ("0123456789abcdef", "feedfacefeedface"),
            "the flag became a proof with its value"
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("## Proofs")
                && !text.contains("d41d8cd98f00b204e9800998ecf8427e")
                && text.contains("flags: []"),
            "{text}"
        );
        assert!(
            record(
                &mut e,
                &path,
                &Proof {
                    name: " ".into(),
                    value: "x".into(),
                    ..Proof::default()
                },
                None
            )
            .is_err()
        );
    }

    #[test]
    fn a_proof_ticks_the_milestone_of_the_machine() {
        let (mut e, path) = lab("proofs-exam", Some("oscp"));
        let user = Proof {
            name: "local.txt".into(),
            value: "aaaabbbbcccc".into(),
            ..Proof::default()
        };
        let root = Proof {
            name: "proof.txt".into(),
            value: "ddddeeeeffff".into(),
            ..Proof::default()
        };
        let profile = exam::profiles().iter().find(|p| p.id == "oscp").unwrap();
        let ticked = record(&mut e, &path, &user, Some(0)).unwrap().unwrap();
        assert!(ticked.ends_with("initial-access"));
        record(&mut e, &path, &root, Some(0)).unwrap();
        record(&mut e, &path, &root, Some(3)).unwrap();
        let again = crate::load(&e.dir).unwrap();
        assert_eq!(
            exam::score(profile, &again.exam_done),
            10 + 10 + 10,
            "two milestones of machine 1 and machine 4"
        );
        assert_eq!(record(&mut e, &path, &user, None).unwrap(), None);
    }

    #[test]
    fn the_report_masks_proof_values_unless_asked() {
        let (mut e, path) = lab("proofs-report", None);
        record(
            &mut e,
            &path,
            &Proof {
                name: "proof.txt".into(),
                value: "0123456789abcdef".into(),
                taken: "2026-11-12T10:00:00".into(),
                ..Proof::default()
            },
            None,
        )
        .unwrap();
        let masked = crate::report::build(
            &e,
            &crate::report::Options {
                include_journal: false,
                include_credentials: false,
                mask_secrets: true,
                author: String::new(),
            },
        )
        .unwrap();
        assert!(
            masked.contains("| proof.txt | [masked] |") && !masked.contains("0123456789abcdef"),
            "{masked}"
        );
        let clear = crate::report::build(
            &e,
            &crate::report::Options {
                include_journal: false,
                include_credentials: false,
                mask_secrets: false,
                author: String::new(),
            },
        )
        .unwrap();
        assert!(clear.contains("| proof.txt | 0123456789abcdef |"));
        assert!(crate::report::secrets_of(&e.dir).contains(&"0123456789abcdef".to_string()));
    }
}
