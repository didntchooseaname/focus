use std::path::Path;

use crate::{HostStatus, Overview, hosts, journal, loot};

pub(crate) fn compute(engagement_dir: &Path) -> Overview {
    let hosts = hosts::list(engagement_dir);
    let mut overview = Overview {
        hosts: hosts.len(),
        hosts_owned: hosts
            .iter()
            .filter(|h| h.status == HostStatus::Owned)
            .count(),
        services: hosts.iter().map(|h| h.services.len()).sum(),
        credentials: loot::list(engagement_dir).len(),
        journal_entries: journal::read(engagement_dir).len(),
        points: hosts.iter().filter_map(|h| h.points).sum(),
        ..Overview::default()
    };
    for finding in crate::findings::list(engagement_dir) {
        overview.findings[finding.severity as usize] += 1;
    }
    if let Ok(text) = std::fs::read_to_string(engagement_dir.join("Checklist.md")) {
        for section in crate::checklist::progress(&text) {
            overview.checklist_done += section.done;
            overview.checklist_total += section.total;
        }
    }
    overview
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngagementKind, NewEngagement, Severity, fsutil};

    #[test]
    fn empty_and_missing_directories_give_zeros() {
        let dir = fsutil::test_dir("overview-empty");
        let overview = compute(&dir.join("absent"));
        assert_eq!(
            (overview.hosts, overview.findings, overview.checklist_total),
            (0, [0; 5], 0)
        );
        let created = crate::create(
            &dir,
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
        let overview = compute(&created.dir);
        assert_eq!(
            (
                overview.hosts,
                overview.credentials,
                overview.journal_entries,
                overview.points
            ),
            (0, 0, 0, 0)
        );
        assert!(overview.checklist_total > 20 && overview.checklist_done == 0);
    }

    #[test]
    fn counts_follow_the_notes() {
        let dir = fsutil::test_dir("overview-full");
        let e = crate::create(
            &dir,
            &NewEngagement {
                name: "Full".into(),
                kind: EngagementKind::Exam,
                client: None,
                scope: vec![],
                exam: Some("oscp".into()),
                checklist: None,
            },
        )
        .unwrap();
        let mut host = crate::Host {
            path: Default::default(),
            ip: "192.0.2.5".into(),
            hostname: None,
            os: None,
            status: HostStatus::Owned,
            services: vec![],
            tags: vec![],
            flags: vec![],
            points: Some(20),
        };
        host.services = (1..=3)
            .map(|p| crate::Service {
                port: p,
                proto: "tcp".into(),
                state: "open".into(),
                name: String::new(),
                product: String::new(),
                version: String::new(),
                notes: String::new(),
            })
            .collect();
        hosts::upsert(&e.dir, &host).unwrap();
        host.ip = "192.0.2.6".into();
        host.status = HostStatus::New;
        host.points = None;
        host.services.truncate(1);
        hosts::upsert(&e.dir, &host).unwrap();
        crate::findings::create(&e.dir, "A", Severity::High).unwrap();
        crate::findings::create(&e.dir, "B", Severity::High).unwrap();
        crate::findings::create(&e.dir, "C", Severity::Info).unwrap();
        journal::append(&e.dir, None, "one line").unwrap();
        let checklist = std::fs::read_to_string(e.dir.join("Checklist.md"))
            .unwrap()
            .replacen("- [ ]", "- [x]", 3);
        std::fs::write(e.dir.join("Checklist.md"), checklist).unwrap();
        let o = compute(&e.dir);
        assert_eq!(
            (
                o.hosts,
                o.hosts_owned,
                o.services,
                o.points,
                o.journal_entries
            ),
            (2, 1, 4, 20, 1)
        );
        assert_eq!(o.findings, [1, 0, 0, 2, 0]);
        assert_eq!(o.checklist_done, 3);
    }
}
