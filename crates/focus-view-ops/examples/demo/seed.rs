use std::path::Path;

use focus_ops::{
    Credential, EngagementKind, EngagementStatus, Host, HostStatus, NewEngagement, Service,
    Severity, findings, hosts, journal, loot,
};

pub const NMAP: &str = include_str!("../../../focus-ops/tests/data/scan.xml");

fn credential(user: &str, kind: &str, secret: &str, domain: &str, source: &str) -> Credential {
    Credential {
        user: user.into(),
        kind: kind.into(),
        secret: secret.into(),
        domain: domain.into(),
        source: source.into(),
        valid_on: String::new(),
        notes: String::new(),
    }
}

pub fn exam_started_iso(hours_ago: i64) -> String {
    let when =
        std::time::SystemTime::now() - std::time::Duration::from_secs((hours_ago * 3600) as u64);
    let secs = when
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as libc_time;
    // SAFETY: `localtime_r` writes into the zeroed `tm` it is given.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

#[allow(non_camel_case_types)]
type libc_time = libc::time_t;

pub fn seed_pentest(ops_dir: &Path) -> anyhow::Result<()> {
    let spec = NewEngagement {
        name: "ACME internal 2026".into(),
        kind: EngagementKind::InternalPentest,
        client: Some("ACME Corp".into()),
        scope: vec!["10.10.10.0/24".into(), "acme.local".into()],
        exam: None,
        checklist: None,
    };
    let mut engagement = focus_ops::create(ops_dir, &spec)?;
    engagement.status = EngagementStatus::InProgress;
    engagement.deadline = Some("2026-12-04".into());
    engagement.vars.insert("DOMAIN".into(), "acme.local".into());
    engagement.vars.insert("USER".into(), "svc-backup".into());
    engagement.vars.insert("LHOST".into(), "10.10.14.7".into());
    engagement.vars.insert("DC".into(), "10.10.10.2".into());
    let service = |port: u16, name: &str| Service {
        port,
        proto: "tcp".into(),
        state: "open".into(),
        name: name.into(),
        product: String::new(),
        version: String::new(),
        notes: String::new(),
    };
    type Row = (
        &'static str,
        &'static str,
        &'static str,
        HostStatus,
        &'static [(u16, &'static str)],
    );
    let rows: [Row; 6] = [
        (
            "10.10.10.2",
            "dc01.acme.local",
            "Windows Server 2019",
            HostStatus::Owned,
            &[
                (53, "domain"),
                (88, "kerberos-sec"),
                (389, "ldap"),
                (445, "microsoft-ds"),
            ],
        ),
        (
            "10.10.10.5",
            "files01",
            "Windows Server 2016",
            HostStatus::Owned,
            &[
                (135, "msrpc"),
                (445, "microsoft-ds"),
                (3389, "ms-wbt-server"),
            ],
        ),
        (
            "10.10.10.12",
            "build01",
            "Ubuntu Linux 22.04",
            HostStatus::Foothold,
            &[(22, "ssh"), (8080, "http-proxy")],
        ),
        (
            "10.10.10.20",
            "intranet",
            "Debian Linux 12",
            HostStatus::Enum,
            &[(80, "http"), (443, "https")],
        ),
        ("10.10.10.31", "", "", HostStatus::New, &[]),
        (
            "10.10.10.44",
            "print01",
            "Windows 10",
            HostStatus::Enum,
            &[(631, "ipp"), (9100, "jetdirect")],
        ),
    ];
    for (ip, name, os, status, ports) in rows {
        let host = Host {
            path: Default::default(),
            ip: ip.into(),
            hostname: (!name.is_empty()).then(|| name.into()),
            os: (!os.is_empty()).then(|| os.into()),
            status,
            services: ports.iter().map(|(p, n)| service(*p, n)).collect(),
            tags: vec![],
            flags: vec![],
            points: None,
        };
        hosts::upsert(&engagement.dir, &host)?;
    }
    engagement.focus = hosts::list(&engagement.dir)
        .first()
        .and_then(|h| h.path.file_stem())
        .map(|s| s.to_string_lossy().into_owned());
    focus_ops::save(&engagement)?;

    let make = |title: &str,
                severity: Severity,
                vector: Option<&str>,
                status: focus_ops::FindingStatus,
                assets: &[&str]|
     -> anyhow::Result<()> {
        let mut finding = findings::create(&engagement.dir, title, severity)?;
        if let Some(vector) = vector {
            let score = focus_ops::cvss::score(vector).map_err(anyhow::Error::msg)?;
            finding.cvss_vector = Some(score.vector);
            finding.cvss_score = Some(score.base);
        }
        finding.status = status;
        finding.assets = assets.iter().map(|s| s.to_string()).collect();
        findings::save(&finding)?;
        Ok(())
    };
    use focus_ops::FindingStatus::*;
    make(
        "Domain admin password reused on a file server",
        Severity::Critical,
        Some("CVSS:3.1/AV:N/AC:L/PR:L/UI:N/S:C/C:H/I:H/A:H"),
        Confirmed,
        &["10.10.10.5"],
    )?;
    make(
        "SMB signing not required on workstations",
        Severity::Medium,
        Some("CVSS:3.1/AV:A/AC:H/PR:N/UI:N/S:U/C:H/I:H/A:N"),
        Reported,
        &["10.10.10.0/24"],
    )?;
    make(
        "Backup service account with a weak password",
        Severity::High,
        Some("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:L/A:N"),
        Draft,
        &["10.10.10.12"],
    )?;
    make(
        "Verbose error pages on the intranet",
        Severity::Low,
        None,
        Draft,
        &["intranet.acme.local"],
    )?;
    make(
        "Server banner discloses the product version",
        Severity::Info,
        None,
        Accepted,
        &["10.10.10.5"],
    )?;

    loot::add(
        &engagement.dir,
        &Credential {
            valid_on: "dc01.acme.local, 10.10.10.5".into(),
            ..credential(
                "svc-backup",
                "password",
                "Winter2026!backup",
                "ACME",
                "config file on files01",
            )
        },
    )?;
    loot::add(
        &engagement.dir,
        &credential(
            "administrator",
            "ntlm",
            "aad3b435b51404eeaad3b435b51404ee",
            "ACME",
            "domain controller",
        ),
    )?;
    loot::add(
        &engagement.dir,
        &credential(
            "deploy",
            "ssh-key",
            "id_ed25519 (passphrase protected)",
            "",
            "build server",
        ),
    )?;

    for (target, text) in [
        (None, "Kick-off call, scope confirmed with the client"),
        (Some("10.10.10.5"), "Service scan done, SMB and RDP open"),
        (
            Some("10.10.10.5"),
            "Shared folder readable without a password",
        ),
        (None, "Break"),
        (
            Some("10.10.10.12"),
            "Found a service account in a deployment script on build01 #creds",
        ),
        (
            Some("10.10.10.2"),
            "dc01 reached with the backup account, see [[Kerberoasting]]",
        ),
    ] {
        journal::append(&engagement.dir, target, text)?;
    }
    Ok(())
}

pub fn seed_exam(ops_dir: &Path) -> anyhow::Result<()> {
    let spec = NewEngagement {
        name: "OSCP exam attempt".into(),
        kind: EngagementKind::Exam,
        client: None,
        scope: vec!["192.168.50.0/24".into()],
        exam: Some("oscp".into()),
        checklist: Some("exam".into()),
    };
    let mut engagement = focus_ops::create(ops_dir, &spec)?;
    engagement.status = EngagementStatus::InProgress;
    engagement.started = Some(exam_started_iso(23));
    focus_ops::save(&engagement)?;
    let parsed = hosts::parse_nmap(NMAP)?;
    hosts::import(&engagement.dir, &parsed)?;
    for (index, host) in hosts::list(&engagement.dir).into_iter().enumerate() {
        let updated = Host {
            status: if index == 0 {
                HostStatus::Owned
            } else {
                HostStatus::Enum
            },
            points: (index == 0).then_some(20),
            ..host
        };
        hosts::upsert(&engagement.dir, &updated)?;
    }
    journal::append(&engagement.dir, None, "Exam started, VPN connected")?;
    Ok(())
}
