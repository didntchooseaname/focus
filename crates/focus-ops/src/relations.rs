use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::{Host, findings, hosts, loot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostMention {
    pub range: Range<usize>,
    pub host: PathBuf,
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

fn bounded(text: &str, range: &Range<usize>, address: bool) -> bool {
    let bytes = text.as_bytes();
    if range.start > 0 {
        let before = bytes[range.start - 1];
        if is_word_byte(before) || (address && before == b'.') {
            return false;
        }
    }
    match bytes.get(range.end) {
        None => true,
        Some(&after) if is_word_byte(after) => false,
        Some(b'.') => !bytes
            .get(range.end + 1)
            .is_some_and(|n| n.is_ascii_alphanumeric()),
        Some(_) => true,
    }
}

pub fn host_mentions(text: &str, hosts: &[Host]) -> Vec<HostMention> {
    let lower = text.to_lowercase();
    let same_length = lower.len() == text.len();
    let mut found: Vec<HostMention> = Vec::new();
    for host in hosts.iter().filter(|h| !h.path.as_os_str().is_empty()) {
        let names = [
            (Some(host.ip.as_str()), true),
            (host.hostname.as_deref(), false),
        ];
        for (name, address) in names {
            let Some(name) = name.filter(|n| !n.is_empty()) else {
                continue;
            };
            let (haystack, needle) = if same_length {
                (lower.as_str(), name.to_lowercase())
            } else {
                (text, name.to_string())
            };
            for (start, _) in haystack.match_indices(needle.as_str()) {
                let range = start..start + needle.len();
                if text.is_char_boundary(range.start)
                    && text.is_char_boundary(range.end)
                    && bounded(text, &range, address)
                {
                    found.push(HostMention {
                        range,
                        host: host.path.clone(),
                    });
                }
            }
        }
    }
    found.sort_by(|a, b| {
        a.range
            .start
            .cmp(&b.range.start)
            .then(b.range.len().cmp(&a.range.len()))
    });
    let mut kept: Vec<HostMention> = Vec::new();
    for mention in found {
        if kept
            .last()
            .is_none_or(|last| mention.range.start >= last.range.end)
        {
            kept.push(mention);
        }
    }
    kept
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    FindingHost,
    CredentialHost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub kind: RelationKind,
    pub from: PathBuf,
    pub label: String,
    pub to: PathBuf,
}

pub fn relations(engagement_dir: &Path) -> Vec<Relation> {
    let all = hosts::list(engagement_dir);
    let mut out: Vec<Relation> = Vec::new();
    let mut push = |kind: RelationKind, from: &Path, label: &str, text: &str| {
        for mention in host_mentions(text, &all) {
            let relation = Relation {
                kind,
                from: from.to_path_buf(),
                label: label.to_string(),
                to: mention.host,
            };
            if !out.contains(&relation) {
                out.push(relation);
            }
        }
    };
    for finding in findings::list(engagement_dir) {
        for asset in &finding.assets {
            push(
                RelationKind::FindingHost,
                &finding.path,
                &finding.title,
                asset,
            );
        }
    }
    let loot_path = engagement_dir.join("Loot").join("Credentials.md");
    for credential in loot::list(engagement_dir) {
        let label = if credential.domain.is_empty() {
            credential.user.clone()
        } else {
            format!("{}\\{}", credential.domain, credential.user)
        };
        push(
            RelationKind::CredentialHost,
            &loot_path,
            &label,
            &credential.source,
        );
        push(
            RelationKind::CredentialHost,
            &loot_path,
            &label,
            &credential.valid_on,
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HostStatus, fsutil};

    fn host(ip: &str, name: Option<&str>, note: &str) -> Host {
        Host {
            path: PathBuf::from(note),
            ip: ip.into(),
            hostname: name.map(Into::into),
            os: None,
            status: HostStatus::New,
            services: vec![],
            tags: vec![],
            flags: vec![],
            points: None,
        }
    }

    #[test]
    fn addresses_and_names_are_found_with_the_note_of_their_host() {
        let hosts = [
            host("10.0.0.5", Some("dc01"), "Hosts/dc01.md"),
            host("10.0.0.55", None, "Hosts/10.0.0.55.md"),
        ];
        let text = "ssh to 10.0.0.5, then DC01. Not 10.0.0.555 and not 10.0.0.5.example.";
        let found = host_mentions(text, &hosts);
        let spans: Vec<&str> = found.iter().map(|m| &text[m.range.clone()]).collect();
        assert_eq!(spans, ["10.0.0.5", "DC01"]);
        assert!(found.iter().all(|m| m.host == Path::new("Hosts/dc01.md")));
        assert_eq!(
            host_mentions("host 10.0.0.55.", &hosts)[0].host,
            Path::new("Hosts/10.0.0.55.md")
        );
    }

    #[test]
    fn a_name_inside_a_longer_word_is_not_a_mention() {
        let hosts = [host("10.0.0.5", Some("web"), "Hosts/web.md")];
        assert!(host_mentions("webserver and cobweb", &hosts).is_empty());
        assert_eq!(host_mentions("the web host", &hosts).len(), 1);
    }

    #[test]
    fn hosts_without_a_note_are_skipped_and_mentions_do_not_overlap() {
        let hosts = [
            host("10.0.0.5", None, ""),
            host("10.0.0.5", Some("dc01.acme.local"), "Hosts/dc01.md"),
            host("10.0.0.6", Some("dc01"), "Hosts/other.md"),
        ];
        let text = "dc01.acme.local";
        let found = host_mentions(text, &hosts);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].range, 0..text.len());
    }

    #[test]
    fn relations_join_findings_and_credentials_to_hosts() {
        let dir = fsutil::test_dir("relations");
        let engagement = crate::create(
            &dir,
            &crate::NewEngagement {
                name: "R".into(),
                kind: crate::EngagementKind::Lab,
                client: None,
                scope: vec![],
                exam: None,
                checklist: None,
            },
        )
        .unwrap();
        let note = hosts::upsert(&engagement.dir, &host("10.0.0.5", Some("web01"), "")).unwrap();
        let mut finding =
            findings::create(&engagement.dir, "Weak password", crate::Severity::High).unwrap();
        finding.assets = vec!["web01".into(), "10.9.9.9".into()];
        findings::save(&finding).unwrap();
        loot::add(
            &engagement.dir,
            &crate::Credential {
                user: "admin".into(),
                kind: "password".into(),
                secret: "s3cret!".into(),
                domain: "ACME".into(),
                source: "config on 10.0.0.5".into(),
                valid_on: "web01".into(),
                notes: String::new(),
            },
        )
        .unwrap();
        let relations = relations(&engagement.dir);
        let kinds: Vec<RelationKind> = relations.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            [RelationKind::FindingHost, RelationKind::CredentialHost],
            "source and valid_on citing one host give one relation"
        );
        assert!(relations.iter().all(|r| r.to == note));
        assert_eq!(relations[1].label, "ACME\\admin");
    }
}
