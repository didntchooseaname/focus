use std::collections::BTreeMap;

use crate::Severity;
use crate::cvss4_table::LOOKUP;

#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub version: &'static str,
    pub base: f32,
    pub severity: Severity,
    pub vector: String,
}

#[derive(Debug, Clone)]
pub struct Metric {
    pub code: &'static str,
    pub label: &'static str,
    pub values: &'static [(&'static str, &'static str)],
}

const METRICS_31: &[Metric] = &[
    Metric {
        code: "AV",
        label: "Attack vector",
        values: &[
            ("N", "Network"),
            ("A", "Adjacent"),
            ("L", "Local"),
            ("P", "Physical"),
        ],
    },
    Metric {
        code: "AC",
        label: "Attack complexity",
        values: &[("L", "Low"), ("H", "High")],
    },
    Metric {
        code: "PR",
        label: "Privileges required",
        values: &[("N", "None"), ("L", "Low"), ("H", "High")],
    },
    Metric {
        code: "UI",
        label: "User interaction",
        values: &[("N", "None"), ("R", "Required")],
    },
    Metric {
        code: "S",
        label: "Scope",
        values: &[("U", "Unchanged"), ("C", "Changed")],
    },
    Metric {
        code: "C",
        label: "Confidentiality",
        values: &[("H", "High"), ("L", "Low"), ("N", "None")],
    },
    Metric {
        code: "I",
        label: "Integrity",
        values: &[("H", "High"), ("L", "Low"), ("N", "None")],
    },
    Metric {
        code: "A",
        label: "Availability",
        values: &[("H", "High"), ("L", "Low"), ("N", "None")],
    },
];

const METRICS_40: &[Metric] = &[
    Metric {
        code: "AV",
        label: "Attack vector",
        values: &[
            ("N", "Network"),
            ("A", "Adjacent"),
            ("L", "Local"),
            ("P", "Physical"),
        ],
    },
    Metric {
        code: "AC",
        label: "Attack complexity",
        values: &[("L", "Low"), ("H", "High")],
    },
    Metric {
        code: "AT",
        label: "Attack requirements",
        values: &[("N", "None"), ("P", "Present")],
    },
    Metric {
        code: "PR",
        label: "Privileges required",
        values: &[("N", "None"), ("L", "Low"), ("H", "High")],
    },
    Metric {
        code: "UI",
        label: "User interaction",
        values: &[("N", "None"), ("P", "Passive"), ("A", "Active")],
    },
    Metric {
        code: "VC",
        label: "Vulnerable system confidentiality",
        values: &[("H", "High"), ("L", "Low"), ("N", "None")],
    },
    Metric {
        code: "VI",
        label: "Vulnerable system integrity",
        values: &[("H", "High"), ("L", "Low"), ("N", "None")],
    },
    Metric {
        code: "VA",
        label: "Vulnerable system availability",
        values: &[("H", "High"), ("L", "Low"), ("N", "None")],
    },
    Metric {
        code: "SC",
        label: "Subsequent system confidentiality",
        values: &[("H", "High"), ("L", "Low"), ("N", "None")],
    },
    Metric {
        code: "SI",
        label: "Subsequent system integrity",
        values: &[("H", "High"), ("L", "Low"), ("N", "None")],
    },
    Metric {
        code: "SA",
        label: "Subsequent system availability",
        values: &[("H", "High"), ("L", "Low"), ("N", "None")],
    },
];

pub fn base_metrics(version: &str) -> &'static [Metric] {
    match version {
        "3.1" => METRICS_31,
        "4.0" => METRICS_40,
        _ => &[],
    }
}

pub fn score(vector: &str) -> Result<Score, String> {
    let vector = vector.trim();
    let (prefix, rest) = vector
        .split_once('/')
        .ok_or("empty vector or missing `CVSS:3.1/` or `CVSS:4.0/` prefix")?;
    match prefix {
        "CVSS:3.1" => score_31(rest),
        "CVSS:4.0" => score_40(rest),
        "CVSS:3.0" => Err("CVSS 3.0 is not supported: recompute the vector as 3.1".into()),
        other => Err(format!("unknown vector prefix: {other}")),
    }
}

fn parse_base(
    rest: &str,
    metrics: &'static [Metric],
) -> Result<BTreeMap<&'static str, char>, String> {
    let mut values: BTreeMap<&'static str, char> = BTreeMap::new();
    let mut seen: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        let (code, value) = part
            .split_once(':')
            .ok_or_else(|| format!("malformed metric: {part:?}"))?;
        if code.is_empty() || value.is_empty() {
            return Err(format!("malformed metric: {part:?}"));
        }
        if seen.contains(&code) {
            return Err(format!("repeated metric: {code}"));
        }
        seen.push(code);
        if let Some(metric) = metrics.iter().find(|m| m.code == code) {
            let known = metric
                .values
                .iter()
                .find(|(v, _)| *v == value)
                .ok_or_else(|| format!("unknown value for {code}: {value}"))?;
            values.insert(metric.code, known.0.chars().next().unwrap_or('?'));
        }
    }
    for metric in metrics {
        if !values.contains_key(metric.code) {
            return Err(format!("missing base metric: {}", metric.code));
        }
    }
    Ok(values)
}

fn normalized(
    version: &str,
    metrics: &'static [Metric],
    values: &BTreeMap<&'static str, char>,
) -> String {
    let parts: Vec<String> = metrics
        .iter()
        .map(|m| format!("{}:{}", m.code, values[m.code]))
        .collect();
    format!("CVSS:{version}/{}", parts.join("/"))
}

fn severity(score: f64) -> Severity {
    match score {
        s if s <= 0.0 => Severity::Info,
        s if s < 4.0 => Severity::Low,
        s if s < 7.0 => Severity::Medium,
        s if s < 9.0 => Severity::High,
        _ => Severity::Critical,
    }
}

fn roundup(input: f64) -> f64 {
    let int_input = (input * 100_000.0).round() as i64;
    if int_input % 10_000 == 0 {
        int_input as f64 / 100_000.0
    } else {
        ((int_input / 10_000) + 1) as f64 / 10.0
    }
}

fn score_31(rest: &str) -> Result<Score, String> {
    let v = parse_base(rest, METRICS_31)?;
    let changed = v["S"] == 'C';
    let av: f64 = match v["AV"] {
        'N' => 0.85,
        'A' => 0.62,
        'L' => 0.55,
        _ => 0.2,
    };
    let ac = if v["AC"] == 'L' { 0.77 } else { 0.44 };
    let pr = match (v["PR"], changed) {
        ('N', _) => 0.85,
        ('L', false) => 0.62,
        ('L', true) => 0.68,
        (_, false) => 0.27,
        (_, true) => 0.5,
    };
    let ui = if v["UI"] == 'N' { 0.85 } else { 0.62 };
    let cia = |c: char| -> f64 {
        match c {
            'H' => 0.56,
            'L' => 0.22,
            _ => 0.0,
        }
    };
    let iss = 1.0 - (1.0 - cia(v["C"])) * (1.0 - cia(v["I"])) * (1.0 - cia(v["A"]));
    let impact = if changed {
        7.52 * (iss - 0.029) - 3.25 * (iss - 0.02).powi(15)
    } else {
        6.42 * iss
    };
    let exploitability = 8.22 * av * ac * pr * ui;
    let base = if impact <= 0.0 {
        0.0
    } else if changed {
        roundup((1.08 * (impact + exploitability)).min(10.0))
    } else {
        roundup((impact + exploitability).min(10.0))
    };
    Ok(Score {
        version: "3.1",
        base: base as f32,
        severity: severity(base),
        vector: normalized("3.1", METRICS_31, &v),
    })
}

type Levels = &'static [(char, f64)];

fn levels(code: &str) -> Levels {
    match code {
        "AV" => &[('N', 0.0), ('A', 0.1), ('L', 0.2), ('P', 0.3)],
        "PR" => &[('N', 0.0), ('L', 0.1), ('H', 0.2)],
        "UI" => &[('N', 0.0), ('P', 0.1), ('A', 0.2)],
        "AC" => &[('L', 0.0), ('H', 0.1)],
        "AT" => &[('N', 0.0), ('P', 0.1)],
        "VC" | "VI" | "VA" => &[('H', 0.0), ('L', 0.1), ('N', 0.2)],
        "SC" => &[('H', 0.1), ('L', 0.2), ('N', 0.3)],
        "SI" | "SA" => &[('S', 0.0), ('H', 0.1), ('L', 0.2), ('N', 0.3)],
        "CR" | "IR" | "AR" => &[('H', 0.0), ('M', 0.1), ('L', 0.2)],
        _ => &[],
    }
}

fn level(code: &str, value: char) -> f64 {
    levels(code)
        .iter()
        .find(|(c, _)| *c == value)
        .map_or(0.0, |(_, l)| *l)
}

fn max_eq1(eq: u8) -> &'static [&'static str] {
    match eq {
        0 => &["AV:N/PR:N/UI:N/"],
        1 => &["AV:A/PR:N/UI:N/", "AV:N/PR:L/UI:N/", "AV:N/PR:N/UI:P/"],
        _ => &["AV:P/PR:N/UI:N/", "AV:A/PR:L/UI:P/"],
    }
}

fn max_eq2(eq: u8) -> &'static [&'static str] {
    match eq {
        0 => &["AC:L/AT:N/"],
        _ => &["AC:H/AT:N/", "AC:L/AT:P/"],
    }
}

fn max_eq3_eq6(eq3: u8, eq6: u8) -> &'static [&'static str] {
    match (eq3, eq6) {
        (0, 0) => &["VC:H/VI:H/VA:H/CR:H/IR:H/AR:H/"],
        (0, _) => &[
            "VC:H/VI:H/VA:L/CR:M/IR:M/AR:H/",
            "VC:H/VI:H/VA:H/CR:M/IR:M/AR:M/",
        ],
        (1, 0) => &[
            "VC:L/VI:H/VA:H/CR:H/IR:H/AR:H/",
            "VC:H/VI:L/VA:H/CR:H/IR:H/AR:H/",
        ],
        (1, _) => &[
            "VC:L/VI:H/VA:L/CR:H/IR:M/AR:H/",
            "VC:L/VI:H/VA:H/CR:H/IR:M/AR:M/",
            "VC:H/VI:L/VA:H/CR:M/IR:H/AR:M/",
            "VC:H/VI:L/VA:L/CR:M/IR:H/AR:H/",
            "VC:L/VI:L/VA:H/CR:H/IR:H/AR:M/",
        ],
        _ => &["VC:L/VI:L/VA:L/CR:H/IR:H/AR:H/"],
    }
}

fn max_eq4(eq: u8) -> &'static [&'static str] {
    match eq {
        0 => &["SC:H/SI:S/SA:S/"],
        1 => &["SC:H/SI:H/SA:H/"],
        _ => &["SC:L/SI:L/SA:L/"],
    }
}

fn max_severity_eq1(eq: u8) -> f64 {
    [1.0, 4.0, 5.0][usize::from(eq)]
}
fn max_severity_eq2(eq: u8) -> f64 {
    [1.0, 2.0][usize::from(eq)]
}
fn max_severity_eq3_eq6(eq3: u8, eq6: u8) -> f64 {
    match (eq3, eq6) {
        (0, 0) => 7.0,
        (0, _) => 6.0,
        (1, _) => 8.0,
        _ => 10.0,
    }
}
fn max_severity_eq4(eq: u8) -> f64 {
    [6.0, 5.0, 4.0][usize::from(eq)]
}

fn lookup(key: &str) -> Option<f64> {
    LOOKUP
        .binary_search_by(|(k, _)| (*k).cmp(key))
        .ok()
        .map(|i| LOOKUP[i].1)
}

fn score_40(rest: &str) -> Result<Score, String> {
    let v = parse_base(rest, METRICS_40)?;
    let base = base_40(&v);
    Ok(Score {
        version: "4.0",
        base: base as f32,
        severity: severity(base),
        vector: normalized("4.0", METRICS_40, &v),
    })
}

fn base_40(v: &BTreeMap<&'static str, char>) -> f64 {
    let m = |code: &str| -> char {
        match code {
            "E" => 'A',
            "CR" | "IR" | "AR" => 'H',
            _ => v[code],
        }
    };
    if ["VC", "VI", "VA", "SC", "SI", "SA"]
        .iter()
        .all(|c| m(c) == 'N')
    {
        return 0.0;
    }
    let eq1: u8 = if m("AV") == 'N' && m("PR") == 'N' && m("UI") == 'N' {
        0
    } else if (m("AV") == 'N' || m("PR") == 'N' || m("UI") == 'N') && m("AV") != 'P' {
        1
    } else {
        2
    };
    let eq2: u8 = if m("AC") == 'L' && m("AT") == 'N' {
        0
    } else {
        1
    };
    let eq3: u8 = if m("VC") == 'H' && m("VI") == 'H' {
        0
    } else if m("VC") == 'H' || m("VI") == 'H' || m("VA") == 'H' {
        1
    } else {
        2
    };
    let eq4: u8 = if m("SC") == 'H' || m("SI") == 'H' || m("SA") == 'H' {
        1
    } else {
        2
    };
    let eq5: u8 = 0;
    let eq6: u8 = if (m("CR") == 'H' && m("VC") == 'H')
        || (m("IR") == 'H' && m("VI") == 'H')
        || (m("AR") == 'H' && m("VA") == 'H')
    {
        0
    } else {
        1
    };

    let key = |a: u8, b: u8, c: u8, d: u8, e: u8, f: u8| format!("{a}{b}{c}{d}{e}{f}");
    let macro_vector = key(eq1, eq2, eq3, eq4, eq5, eq6);
    let value = lookup(&macro_vector).expect("every computed macro vector is in the FIRST table");

    let lower_eq1 = lookup(&key(eq1 + 1, eq2, eq3, eq4, eq5, eq6));
    let lower_eq2 = lookup(&key(eq1, eq2 + 1, eq3, eq4, eq5, eq6));
    let lower_eq3_eq6 = match (eq3, eq6) {
        (1, 1) | (0, 1) => lookup(&key(eq1, eq2, eq3 + 1, eq4, eq5, eq6)),
        (1, 0) => lookup(&key(eq1, eq2, eq3, eq4, eq5, eq6 + 1)),
        (0, 0) => {
            let left = lookup(&key(eq1, eq2, eq3, eq4, eq5, eq6 + 1));
            let right = lookup(&key(eq1, eq2, eq3 + 1, eq4, eq5, eq6));
            match (left, right) {
                (Some(l), Some(r)) => Some(if l > r { l } else { r }),
                (l, r) => l.or(r),
            }
        }
        _ => lookup(&key(eq1, eq2, eq3 + 1, eq4, eq5, eq6 + 1)),
    };
    let lower_eq4 = lookup(&key(eq1, eq2, eq3, eq4 + 1, eq5, eq6));
    let lower_eq5 = lookup(&key(eq1, eq2, eq3, eq4, eq5 + 1, eq6));

    let mut distances: Option<BTreeMap<&str, f64>> = None;
    'search: for a in max_eq1(eq1) {
        for b in max_eq2(eq2) {
            for c in max_eq3_eq6(eq3, eq6) {
                for d in max_eq4(eq4) {
                    let composed = format!("{a}{b}{c}{d}");
                    let max: BTreeMap<&str, char> = composed
                        .split('/')
                        .filter_map(|p| p.split_once(':'))
                        .map(|(k, val)| (k, val.chars().next().unwrap_or('?')))
                        .collect();
                    let mut current: BTreeMap<&str, f64> = BTreeMap::new();
                    for code in [
                        "AV", "PR", "UI", "AC", "AT", "VC", "VI", "VA", "SC", "SI", "SA", "CR",
                        "IR", "AR",
                    ] {
                        current.insert(code, level(code, m(code)) - level(code, max[code]));
                    }
                    let acceptable = current.values().all(|d| *d >= 0.0);
                    distances = Some(current);
                    if acceptable {
                        break 'search;
                    }
                }
            }
        }
    }
    let d = distances.expect("at least one maximal vector per macro vector");
    let step = 0.1;
    let distance_eq1 = d["AV"] + d["PR"] + d["UI"];
    let distance_eq2 = d["AC"] + d["AT"];
    let distance_eq3_eq6 = d["VC"] + d["VI"] + d["VA"] + d["CR"] + d["IR"] + d["AR"];
    let distance_eq4 = d["SC"] + d["SI"] + d["SA"];

    let mut normalized = [0.0f64; 5];
    let mut existing = 0u32;
    let parts = [
        (lower_eq1, distance_eq1, max_severity_eq1(eq1) * step),
        (lower_eq2, distance_eq2, max_severity_eq2(eq2) * step),
        (
            lower_eq3_eq6,
            distance_eq3_eq6,
            max_severity_eq3_eq6(eq3, eq6) * step,
        ),
        (lower_eq4, distance_eq4, max_severity_eq4(eq4) * step),
        (lower_eq5, 0.0, 1.0),
    ];
    for (i, (lower, distance, max_severity)) in parts.into_iter().enumerate() {
        if let Some(lower) = lower {
            existing += 1;
            normalized[i] = (value - lower) * (distance / max_severity);
        }
    }
    let mean = if existing == 0 {
        0.0
    } else {
        (normalized[0] + normalized[1] + normalized[2] + normalized[3] + normalized[4])
            / f64::from(existing)
    };
    let result = (value - mean).clamp(0.0, 10.0);
    (result * 10.0 + 0.5).floor() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFFICIAL_31: &[(&str, f32)] = &[
        ("AV:N/AC:L/PR:L/UI:N/S:C/C:L/I:L/A:N", 6.4),
        ("AV:N/AC:H/PR:N/UI:R/S:U/C:L/I:N/A:N", 3.1),
        ("AV:N/AC:L/PR:L/UI:N/S:C/C:H/I:H/A:H", 9.9),
        ("AV:L/AC:L/PR:H/UI:N/S:U/C:L/I:L/A:L", 4.2),
        ("AV:N/AC:L/PR:H/UI:N/S:U/C:H/I:H/A:H", 7.2),
        ("AV:L/AC:L/PR:N/UI:R/S:U/C:H/I:H/A:H", 7.8),
        ("AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N", 7.5),
        ("AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H", 9.8),
        ("AV:N/AC:H/PR:N/UI:N/S:C/C:N/I:H/A:N", 6.8),
        ("AV:P/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H", 6.8),
        ("AV:N/AC:L/PR:N/UI:N/S:C/C:L/I:N/A:N", 5.8),
        ("AV:N/AC:L/PR:N/UI:N/S:C/C:N/I:L/A:N", 5.8),
        ("AV:A/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:H", 9.3),
        ("AV:N/AC:L/PR:L/UI:R/S:C/C:H/I:H/A:H", 9.0),
        ("AV:A/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H", 8.8),
        ("AV:P/AC:L/PR:N/UI:N/S:U/C:N/I:H/A:N", 4.6),
        ("AV:N/AC:L/PR:N/UI:R/S:U/C:H/I:H/A:H", 8.8),
        ("AV:N/AC:H/PR:N/UI:N/S:U/C:H/I:H/A:N", 7.4),
        ("AV:N/AC:L/PR:N/UI:R/S:C/C:H/I:H/A:H", 9.6),
        ("AV:N/AC:H/PR:N/UI:R/S:U/C:H/I:H/A:N", 6.8),
        ("AV:N/AC:H/PR:N/UI:R/S:U/C:H/I:H/A:H", 7.5),
        ("AV:N/AC:L/PR:N/UI:R/S:C/C:L/I:L/A:N", 6.1),
        ("AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:L/A:L", 8.6),
        ("AV:L/AC:L/PR:H/UI:N/S:C/C:H/I:H/A:H", 8.2),
        ("AV:L/AC:L/PR:H/UI:N/S:U/C:N/I:H/A:H", 6.0),
        ("AV:P/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H", 7.6),
        ("AV:N/AC:H/PR:N/UI:R/S:U/C:L/I:L/A:N", 4.2),
    ];

    const OFFICIAL_40: &[(&str, f32)] = &[
        (
            "AV:L/AC:L/AT:P/PR:L/UI:N/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N",
            7.3,
        ),
        (
            "AV:N/AC:L/AT:P/PR:N/UI:P/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N",
            7.7,
        ),
        (
            "AV:N/AC:L/AT:P/PR:N/UI:N/VC:H/VI:L/VA:L/SC:N/SI:N/SA:N",
            8.3,
        ),
        (
            "AV:L/AC:L/AT:N/PR:N/UI:A/VC:L/VI:N/VA:N/SC:N/SI:N/SA:N",
            4.6,
        ),
        (
            "AV:N/AC:L/AT:N/PR:N/UI:A/VC:N/VI:N/VA:N/SC:L/SI:L/SA:N",
            5.1,
        ),
        (
            "AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:N/VA:N/SC:L/SI:L/SA:N",
            6.9,
        ),
        (
            "AV:L/AC:L/AT:N/PR:H/UI:N/VC:N/VI:N/VA:N/SC:H/SI:N/SA:N",
            5.9,
        ),
        (
            "AV:L/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:H/SI:H/SA:H",
            9.4,
        ),
        (
            "AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:H/VA:N/SC:H/SI:H/SA:H",
            9.3,
        ),
        (
            "AV:P/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:N/SI:H/SA:N",
            8.3,
        ),
        (
            "AV:N/AC:L/AT:N/PR:H/UI:N/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N",
            8.6,
        ),
        (
            "AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:N/VA:N/SC:N/SI:N/SA:N",
            8.7,
        ),
        (
            "AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N",
            9.3,
        ),
        (
            "AV:A/AC:L/AT:N/PR:N/UI:N/VC:N/VI:L/VA:N/SC:H/SI:N/SA:H",
            6.4,
        ),
        (
            "AV:P/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N",
            7.0,
        ),
        (
            "AV:N/AC:L/AT:N/PR:L/UI:N/VC:H/VI:N/VA:N/SC:N/SI:N/SA:N",
            7.1,
        ),
        (
            "AV:N/AC:L/AT:P/PR:N/UI:N/VC:H/VI:N/VA:N/SC:N/SI:N/SA:N",
            8.2,
        ),
        (
            "AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:N/VA:H/SC:N/SI:N/SA:L",
            8.7,
        ),
        (
            "AV:N/AC:L/AT:N/PR:N/UI:A/VC:L/VI:L/VA:N/SC:L/SI:L/SA:N",
            5.1,
        ),
        (
            "AV:N/AC:L/AT:N/PR:L/UI:P/VC:N/VI:N/VA:N/SC:L/SI:L/SA:N",
            5.1,
        ),
        (
            "AV:N/AC:L/AT:N/PR:L/UI:A/VC:H/VI:H/VA:H/SC:L/SI:L/SA:N",
            8.5,
        ),
        (
            "AV:N/AC:L/AT:N/PR:N/UI:A/VC:N/VI:L/VA:N/SC:N/SI:N/SA:N",
            5.1,
        ),
        (
            "AV:N/AC:L/AT:P/PR:L/UI:N/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N",
            7.7,
        ),
    ];

    #[test]
    fn cvss_31_official_examples() {
        for (metrics, expected) in OFFICIAL_31 {
            let vector = format!("CVSS:3.1/{metrics}");
            let result = score(&vector).unwrap();
            assert_eq!(result.base, *expected, "{vector}");
            assert_eq!(result.vector, vector);
            assert_eq!(result.version, "3.1");
        }
    }

    #[test]
    fn cvss_40_official_examples() {
        for (metrics, expected) in OFFICIAL_40 {
            let vector = format!("CVSS:4.0/{metrics}");
            let result = score(&vector).unwrap();
            assert_eq!(result.base, *expected, "{vector}");
            assert_eq!(result.vector, vector);
            assert_eq!(result.version, "4.0");
        }
    }

    #[test]
    fn cvss_40_matches_the_reference_calculator_on_700_random_vectors() {
        let data = include_str!("../tests/data/cvss40-reference.txt");
        let mut count = 0;
        for line in data.lines() {
            let (metrics, expected) = line.split_once(' ').unwrap();
            let result = score(&format!("CVSS:4.0/{metrics}")).unwrap();
            assert_eq!(format!("{:.1}", result.base), expected, "{metrics}");
            count += 1;
        }
        assert_eq!(count, 700);
    }

    #[test]
    fn severities_follow_the_qualitative_scales() {
        assert_eq!(
            score("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:N")
                .unwrap()
                .severity,
            Severity::Info
        );
        assert_eq!(
            score("CVSS:3.1/AV:N/AC:H/PR:N/UI:R/S:U/C:L/I:N/A:N")
                .unwrap()
                .severity,
            Severity::Low
        );
        assert_eq!(
            score("CVSS:3.1/AV:L/AC:L/PR:H/UI:N/S:U/C:L/I:L/A:L")
                .unwrap()
                .severity,
            Severity::Medium
        );
        assert_eq!(
            score("CVSS:3.1/AV:A/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H")
                .unwrap()
                .severity,
            Severity::High
        );
        assert_eq!(
            score("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H")
                .unwrap()
                .severity,
            Severity::Critical
        );
        assert_eq!(
            score("CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:N/VA:N/SC:N/SI:N/SA:N")
                .unwrap()
                .base,
            0.0
        );
    }

    #[test]
    fn optional_metrics_and_order_are_tolerated_and_dropped() {
        let reference = score("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H").unwrap();
        let shuffled = score("CVSS:3.1/A:H/I:H/C:H/S:U/UI:N/PR:N/AC:L/AV:N/E:F/RL:O/RC:C").unwrap();
        assert_eq!(shuffled, reference);
        let v4 =
            score("CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N/E:A/CR:H/MAV:N")
                .unwrap();
        assert_eq!(
            (v4.base, v4.vector.as_str()),
            (
                9.3,
                "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N"
            )
        );
    }

    #[test]
    fn malformed_vectors_are_explained() {
        for (vector, fragment) in [
            ("", "prefix"),
            ("AV:N", "prefix"),
            ("CVSS:3.0/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H", "3.0"),
            ("CVSS:9.9/AV:N", "prefix"),
            (
                "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H",
                "missing base metric: A",
            ),
            (
                "CVSS:3.1/AV:X/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H",
                "unknown value",
            ),
            (
                "CVSS:3.1/AV:N/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H",
                "repeated",
            ),
            ("CVSS:3.1/AV/AC:L", "malformed"),
            (
                "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H",
                "missing base metric: SC",
            ),
        ] {
            let error = score(vector).unwrap_err();
            assert!(error.contains(fragment), "{vector:?}: {error}");
        }
    }

    #[test]
    fn metrics_tables_match_the_vector_order() {
        let codes = |v: &str| {
            base_metrics(v)
                .iter()
                .map(|m| m.code)
                .collect::<Vec<_>>()
                .join(",")
        };
        assert_eq!(codes("3.1"), "AV,AC,PR,UI,S,C,I,A");
        assert_eq!(codes("4.0"), "AV,AC,AT,PR,UI,VC,VI,VA,SC,SI,SA");
        assert!(base_metrics("2.0").is_empty());
    }

    #[test]
    fn every_macrovector_of_the_reference_table_is_reachable_or_valid() {
        let mut count = 0;
        for av in ["N", "A", "L", "P"] {
            for pr in ["N", "L", "H"] {
                for ui in ["N", "P", "A"] {
                    for ac in ["L", "H"] {
                        for at in ["N", "P"] {
                            for (vc, vi, va) in [
                                ("H", "H", "H"),
                                ("H", "L", "N"),
                                ("L", "L", "L"),
                                ("N", "N", "N"),
                                ("L", "H", "L"),
                                ("N", "N", "L"),
                            ] {
                                for (sc, si, sa) in [
                                    ("N", "N", "N"),
                                    ("H", "H", "H"),
                                    ("L", "L", "L"),
                                    ("H", "N", "L"),
                                ] {
                                    let vector = format!(
                                        "CVSS:4.0/AV:{av}/AC:{ac}/AT:{at}/PR:{pr}/UI:{ui}/VC:{vc}/VI:{vi}/VA:{va}/SC:{sc}/SI:{si}/SA:{sa}"
                                    );
                                    let result = score(&vector).unwrap();
                                    assert!(
                                        (0.0..=10.0).contains(&result.base),
                                        "{vector} : {}",
                                        result.base
                                    );
                                    count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(count, 4 * 3 * 3 * 2 * 2 * 6 * 4);
    }
}
