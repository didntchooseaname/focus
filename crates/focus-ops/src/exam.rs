#[derive(Debug, Clone)]
pub struct Machine {
    pub label_key: &'static str,
    pub points: u32,
    pub milestones: &'static [(&'static str, u32)],
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub id: &'static str,
    pub name: &'static str,
    pub vendor: &'static str,
    pub exam_minutes: u32,
    pub report_minutes: Option<u32>,
    pub pass_points: Option<u32>,
    pub total_points: Option<u32>,
    pub machines: &'static [Machine],
    pub rule_keys: &'static [&'static str],
}

const OSCP_MACHINES: &[Machine] = &[
    Machine {
        label_key: "ops-exam-oscp-machine-1",
        points: 20,
        milestones: &[
            ("ops-exam-milestone-initial-access", 10),
            ("ops-exam-milestone-privilege-escalation", 10),
        ],
    },
    Machine {
        label_key: "ops-exam-oscp-machine-2",
        points: 20,
        milestones: &[
            ("ops-exam-milestone-initial-access", 10),
            ("ops-exam-milestone-privilege-escalation", 10),
        ],
    },
    Machine {
        label_key: "ops-exam-oscp-machine-3",
        points: 20,
        milestones: &[
            ("ops-exam-milestone-initial-access", 10),
            ("ops-exam-milestone-privilege-escalation", 10),
        ],
    },
    Machine {
        label_key: "ops-exam-oscp-machine-4",
        points: 10,
        milestones: &[],
    },
    Machine {
        label_key: "ops-exam-oscp-machine-5",
        points: 10,
        milestones: &[],
    },
    Machine {
        label_key: "ops-exam-oscp-machine-6",
        points: 20,
        milestones: &[],
    },
];

pub fn profiles() -> &'static [Profile] {
    PROFILES
}

const PROFILES: &[Profile] = &[
    Profile {
        id: "oscp",
        name: "OSCP+",
        vendor: "OffSec",
        exam_minutes: 23 * 60 + 45,
        report_minutes: Some(24 * 60),
        pass_points: None,
        total_points: Some(100),
        machines: OSCP_MACHINES,
        rule_keys: &[
            "ops-exam-oscp-rule-1",
            "ops-exam-oscp-rule-2",
            "ops-exam-oscp-rule-3",
            "ops-exam-oscp-rule-4",
            "ops-exam-oscp-rule-5",
            "ops-exam-oscp-rule-6",
        ],
    },
    Profile {
        id: "osep",
        name: "OSEP",
        vendor: "OffSec",
        exam_minutes: 47 * 60 + 45,
        report_minutes: Some(24 * 60),
        pass_points: Some(100),
        total_points: None,
        machines: &[],
        rule_keys: &[
            "ops-exam-osep-rule-1",
            "ops-exam-osep-rule-2",
            "ops-exam-osep-rule-3",
            "ops-exam-osep-rule-4",
        ],
    },
    Profile {
        id: "oswe",
        name: "OSWE",
        vendor: "OffSec",
        exam_minutes: 47 * 60 + 45,
        report_minutes: Some(24 * 60),
        pass_points: Some(85),
        total_points: Some(100),
        machines: &[],
        rule_keys: &[
            "ops-exam-oswe-rule-1",
            "ops-exam-oswe-rule-2",
            "ops-exam-oswe-rule-3",
        ],
    },
    Profile {
        id: "osed",
        name: "OSED",
        vendor: "OffSec",
        exam_minutes: 47 * 60 + 45,
        report_minutes: Some(24 * 60),
        pass_points: Some(60),
        total_points: Some(100),
        machines: &[],
        rule_keys: &[
            "ops-exam-osed-rule-1",
            "ops-exam-osed-rule-2",
            "ops-exam-osed-rule-3",
            "ops-exam-osed-rule-4",
        ],
    },
    Profile {
        id: "crtp",
        name: "CRTP",
        vendor: "Altered Security",
        exam_minutes: 24 * 60,
        report_minutes: None,
        pass_points: None,
        total_points: None,
        machines: &[],
        rule_keys: &["ops-exam-crtp-rule-1", "ops-exam-crtp-rule-2"],
    },
    Profile {
        id: "crte",
        name: "CRTE",
        vendor: "Altered Security",
        exam_minutes: 48 * 60,
        report_minutes: None,
        pass_points: None,
        total_points: None,
        machines: &[],
        rule_keys: &["ops-exam-crte-rule-1", "ops-exam-crte-rule-2"],
    },
    Profile {
        id: "crto",
        name: "CRTO",
        vendor: "Zero-Point Security",
        exam_minutes: 48 * 60,
        report_minutes: None,
        pass_points: Some(6),
        total_points: Some(8),
        machines: &[],
        rule_keys: &["ops-exam-crto-rule-1", "ops-exam-crto-rule-2"],
    },
    Profile {
        id: "pnpt",
        name: "PNPT",
        vendor: "TCM Security",
        exam_minutes: 5 * 24 * 60,
        report_minutes: Some(2 * 24 * 60),
        pass_points: None,
        total_points: None,
        machines: &[],
        rule_keys: &["ops-exam-pnpt-rule-1", "ops-exam-pnpt-rule-2"],
    },
    Profile {
        id: "cpts",
        name: "CPTS",
        vendor: "Hack The Box",
        exam_minutes: 10 * 24 * 60,
        report_minutes: Some(0),
        pass_points: None,
        total_points: None,
        machines: &[],
        rule_keys: &[
            "ops-exam-cpts-rule-1",
            "ops-exam-cpts-rule-2",
            "ops-exam-cpts-rule-3",
        ],
    },
    Profile {
        id: "bscp",
        name: "BSCP",
        vendor: "PortSwigger",
        exam_minutes: 4 * 60,
        report_minutes: None,
        pass_points: None,
        total_points: None,
        machines: &[],
        rule_keys: &[
            "ops-exam-bscp-rule-1",
            "ops-exam-bscp-rule-2",
            "ops-exam-bscp-rule-3",
        ],
    },
    Profile {
        id: "ejpt",
        name: "eJPT",
        vendor: "INE",
        exam_minutes: 48 * 60,
        report_minutes: None,
        pass_points: None,
        total_points: None,
        machines: &[],
        rule_keys: &["ops-exam-ejpt-rule-1", "ops-exam-ejpt-rule-2"],
    },
    Profile {
        id: "ecppt",
        name: "eCPPT",
        vendor: "INE",
        exam_minutes: 24 * 60,
        report_minutes: None,
        pass_points: None,
        total_points: None,
        machines: &[],
        rule_keys: &["ops-exam-ecppt-rule-1", "ops-exam-ecppt-rule-2"],
    },
];

pub fn item_key(machine: &Machine, milestone: Option<&str>) -> String {
    match milestone {
        Some(key) => format!("{}/{key}", machine.label_key),
        None => machine.label_key.to_string(),
    }
}

pub fn score(profile: &Profile, done: &[String]) -> u32 {
    profile
        .machines
        .iter()
        .map(|machine| {
            if machine.milestones.is_empty() {
                if done.contains(&item_key(machine, None)) {
                    machine.points
                } else {
                    0
                }
            } else {
                machine
                    .milestones
                    .iter()
                    .filter(|(key, _)| done.contains(&item_key(machine, Some(key))))
                    .map(|(_, points)| points)
                    .sum()
            }
        })
        .sum()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clock {
    pub elapsed_secs: u64,
    pub remaining_secs: u64,
    pub report_phase: bool,
    pub expired: bool,
}

pub fn clock(profile: &Profile, started: u64, now: u64) -> Clock {
    let elapsed = now.saturating_sub(started);
    let exam = u64::from(profile.exam_minutes) * 60;
    let report = u64::from(profile.report_minutes.unwrap_or(0)) * 60;
    if elapsed < exam {
        Clock {
            elapsed_secs: elapsed,
            remaining_secs: exam - elapsed,
            report_phase: false,
            expired: false,
        }
    } else if elapsed < exam + report {
        Clock {
            elapsed_secs: elapsed,
            remaining_secs: exam + report - elapsed,
            report_phase: true,
            expired: false,
        }
    } else {
        Clock {
            elapsed_secs: elapsed,
            remaining_secs: 0,
            report_phase: false,
            expired: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str) -> &'static Profile {
        profiles().iter().find(|p| p.id == id).expect("profile")
    }

    #[test]
    fn the_score_adds_the_items_reached() {
        let oscp = profile("oscp");
        let first = &oscp.machines[0];
        let done = vec![
            item_key(first, Some("ops-exam-milestone-initial-access")),
            item_key(&oscp.machines[3], None),
            "not-an-item".to_string(),
        ];
        assert_eq!(score(oscp, &done), 10 + 10);
        assert_eq!(score(oscp, &[]), 0);
        let every: Vec<String> = oscp
            .machines
            .iter()
            .flat_map(|m| {
                if m.milestones.is_empty() {
                    vec![item_key(m, None)]
                } else {
                    m.milestones
                        .iter()
                        .map(|(k, _)| item_key(m, Some(k)))
                        .collect()
                }
            })
            .collect();
        assert_eq!(Some(score(oscp, &every)), oscp.total_points);
    }

    #[test]
    fn all_twelve_profiles_are_present_and_consistent() {
        let ids: Vec<&str> = profiles().iter().map(|p| p.id).collect();
        assert_eq!(
            ids,
            [
                "oscp", "osep", "oswe", "osed", "crtp", "crte", "crto", "pnpt", "cpts", "bscp",
                "ejpt", "ecppt"
            ]
        );
        for p in profiles() {
            assert!(
                p.exam_minutes > 0 && !p.rule_keys.is_empty() && !p.vendor.is_empty(),
                "{}",
                p.id
            );
            if let (Some(pass), Some(total)) = (p.pass_points, p.total_points) {
                assert!(pass <= total, "{}", p.id);
            }
            if !p.machines.is_empty() {
                assert_eq!(
                    p.machines.iter().map(|m| m.points).sum::<u32>(),
                    p.total_points.unwrap(),
                    "{}",
                    p.id
                );
                for m in p.machines {
                    assert!(
                        m.milestones.is_empty()
                            || m.milestones.iter().map(|(_, pts)| pts).sum::<u32>() == m.points,
                        "{} {}",
                        p.id,
                        m.label_key
                    );
                }
            }
        }
    }

    fn all_keys() -> Vec<&'static str> {
        let mut keys = Vec::new();
        for p in profiles() {
            keys.extend(p.rule_keys);
            for m in p.machines {
                keys.push(m.label_key);
                keys.extend(m.milestones.iter().map(|(k, _)| *k));
            }
        }
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    fn defined_keys(path: &std::path::Path) -> Vec<String> {
        let text =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        text.lines()
            .filter_map(|l| l.split_once(" =").map(|(k, _)| k.trim().to_string()))
            .filter(|k| k.starts_with("ops-exam-"))
            .collect()
    }

    #[test]
    fn keys_follow_the_scheme() {
        let keys = all_keys();
        assert_eq!(keys.len(), 43);
        for p in profiles() {
            for (i, key) in p.rule_keys.iter().enumerate() {
                assert_eq!(*key, format!("ops-exam-{}-rule-{}", p.id, i + 1));
            }
        }
    }

    #[test]
    fn keys_exist_in_the_locale_files() {
        let locales = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../locales");
        for language in ["en", "fr", "ar", "zh-CN"] {
            let defined = defined_keys(&locales.join(language).join("ops.ftl"));
            for key in all_keys() {
                assert!(
                    defined.iter().any(|k| k == key),
                    "{language}/ops.ftl lacks {key}"
                );
            }
        }
    }

    #[test]
    fn durations_match_the_sources() {
        assert_eq!(
            (profile("oscp").exam_minutes, profile("oscp").report_minutes),
            (1425, Some(1440))
        );
        for id in ["osep", "oswe", "osed"] {
            assert_eq!(
                (profile(id).exam_minutes, profile(id).report_minutes),
                (2865, Some(1440)),
                "{id}"
            );
        }
        assert_eq!(
            (
                profile("oswe").pass_points,
                profile("osed").pass_points,
                profile("osep").pass_points
            ),
            (Some(85), Some(60), Some(100))
        );
        assert_eq!(
            (profile("pnpt").exam_minutes, profile("pnpt").report_minutes),
            (7200, Some(2880))
        );
        assert_eq!(profile("bscp").exam_minutes, 240);
        assert_eq!(
            profile("oscp").pass_points,
            None,
            "threshold not read: stays None"
        );
        assert_eq!(profile("crtp").report_minutes, None);
    }

    #[test]
    fn clock_phases() {
        let oscp = profile("oscp");
        let start = 1_000_000;
        assert_eq!(
            clock(oscp, start, start - 5),
            Clock {
                elapsed_secs: 0,
                remaining_secs: 1425 * 60,
                report_phase: false,
                expired: false
            }
        );
        assert_eq!(
            clock(oscp, start, start + 3600),
            Clock {
                elapsed_secs: 3600,
                remaining_secs: 1425 * 60 - 3600,
                report_phase: false,
                expired: false
            }
        );
        let exam_end = start + 1425 * 60;
        assert_eq!(clock(oscp, start, exam_end - 1).remaining_secs, 1);
        let report = clock(oscp, start, exam_end);
        assert_eq!(
            (report.report_phase, report.expired, report.remaining_secs),
            (true, false, 1440 * 60)
        );
        let late = clock(oscp, start, exam_end + 1440 * 60);
        assert_eq!(
            (late.report_phase, late.expired, late.remaining_secs),
            (false, true, 0)
        );
        let no_report = clock(profile("bscp"), start, start + 4 * 3600);
        assert_eq!((no_report.report_phase, no_report.expired), (false, true));
        let same_window = clock(profile("cpts"), start, start + 10 * 86_400);
        assert!(same_window.expired);
    }
}
