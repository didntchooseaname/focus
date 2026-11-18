use std::path::PathBuf;

use crate::hosts::{self, ImportReport};
use crate::{Host, HostStatus, OpsError, Result};

pub const MAX_BYTES: usize = 5_000_000;
pub const MAX_TARGETS: usize = 5000;
const MAX_FIELD: usize = 253;
const MAX_TAGS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub address: String,
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    NotAnAddress,
    NotAName,
    ExtraText,
    TooLong,
    Repeated,
    TooMany,
    NoAddress,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub line: usize,
    pub text: String,
    pub reason: Reason,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    pub targets: Vec<Target>,
    pub refused: Vec<Refused>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Preview {
    pub create: Vec<Target>,
    pub known: Vec<Target>,
    pub refused: Vec<Refused>,
}

fn is_name(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_FIELD
        && text.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

fn is_address(text: &str) -> bool {
    if text.len() > MAX_FIELD {
        return false;
    }
    if let Some((base, prefix)) = text.split_once('/') {
        let Ok(prefix) = prefix.parse::<u8>() else {
            return false;
        };
        return match base.parse::<std::net::IpAddr>() {
            Ok(std::net::IpAddr::V4(_)) => prefix <= 32,
            Ok(std::net::IpAddr::V6(_)) => prefix <= 128,
            Err(_) => false,
        };
    }
    text.parse::<std::net::IpAddr>().is_ok() || is_name(text)
}

fn tags_of(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in text
        .split([',', ';'])
        .map(|t| t.trim().trim_start_matches('#'))
        .filter(|t| !t.is_empty())
    {
        let tag: String = tag.chars().take(60).collect();
        if !tags.iter().any(|known| known.eq_ignore_ascii_case(&tag)) && tags.len() < MAX_TAGS {
            tags.push(tag);
        }
    }
    tags
}

fn push(parsed: &mut Parsed, line: usize, text: &str, target: Target) {
    if parsed.targets.len() >= MAX_TARGETS {
        parsed.refused.push(Refused {
            line,
            text: text.to_string(),
            reason: Reason::TooMany,
        });
    } else if parsed
        .targets
        .iter()
        .any(|t| t.address.eq_ignore_ascii_case(&target.address))
    {
        parsed.refused.push(Refused {
            line,
            text: text.to_string(),
            reason: Reason::Repeated,
        });
    } else {
        parsed.targets.push(target);
    }
}

fn clip(text: &str) -> String {
    text.chars().take(120).collect()
}

pub fn parse_text(text: &str) -> Parsed {
    let mut parsed = Parsed::default();
    for (index, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut words = line
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .filter(|w| !w.is_empty());
        let (Some(address), name, rest) = (words.next(), words.next(), words.next()) else {
            continue;
        };
        let refuse = |reason| Refused {
            line: index + 1,
            text: clip(raw),
            reason,
        };
        if address.len() > MAX_FIELD {
            parsed.refused.push(refuse(Reason::TooLong));
        } else if !is_address(address) {
            parsed.refused.push(refuse(Reason::NotAnAddress));
        } else if rest.is_some() {
            parsed.refused.push(refuse(Reason::ExtraText));
        } else if name.is_some_and(|n| !is_name(n)) {
            parsed.refused.push(refuse(Reason::NotAName));
        } else {
            push(
                &mut parsed,
                index + 1,
                raw,
                Target {
                    address: address.to_string(),
                    hostname: name.map(str::to_string),
                    os: None,
                    tags: Vec::new(),
                    notes: None,
                },
            );
        }
    }
    parsed
}

fn separator(header: &str) -> char {
    [',', ';', '\t']
        .into_iter()
        .max_by_key(|s| header.matches(*s).count())
        .unwrap_or(',')
}

fn rows(text: &str, separator: char) -> Vec<(usize, Vec<String>)> {
    let mut rows = Vec::new();
    let (mut row, mut field, mut quoted) = (Vec::new(), String::new(), false);
    let (mut line, mut start) = (1, 1);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' if quoted => quoted = false,
            '"' if field.is_empty() => quoted = true,
            c if c == separator && !quoted => row.push(std::mem::take(&mut field)),
            '\n' | '\r' if !quoted => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                if row.iter().any(|f| !f.trim().is_empty()) {
                    rows.push((start, std::mem::take(&mut row)));
                } else {
                    row.clear();
                }
                line += 1;
                start = line;
            }
            c => {
                if c == '\n' {
                    line += 1;
                }
                field.push(c);
            }
        }
    }
    row.push(field);
    if row.iter().any(|f| !f.trim().is_empty()) {
        rows.push((start, row));
    }
    rows
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Column {
    Address,
    Hostname,
    Os,
    Tags,
    Notes,
}

fn column_of(header: &str) -> Option<Column> {
    Some(
        match header
            .trim()
            .trim_start_matches('\u{feff}')
            .to_lowercase()
            .as_str()
        {
            "ip" | "address" | "ip_address" | "ip address" | "host" => Column::Address,
            "hostname" | "name" | "host name" | "fqdn" => Column::Hostname,
            "os" | "system" | "operating system" => Column::Os,
            "tags" | "tag" => Column::Tags,
            "notes" | "note" | "comment" | "comments" => Column::Notes,
            _ => return None,
        },
    )
}

pub fn parse_csv(text: &str) -> Parsed {
    let text = text.trim_start_matches('\u{feff}');
    let mut parsed = Parsed::default();
    let first_line = text
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .unwrap_or("");
    let separator = separator(first_line);
    let mut all = rows(text, separator)
        .into_iter()
        .filter(|(_, row)| !row.first().is_some_and(|f| f.trim_start().starts_with('#')));
    let Some((_, header)) = all.next() else {
        return parsed;
    };
    let columns: Vec<Option<Column>> = header.iter().map(|h| column_of(h)).collect();
    for (line, row) in all {
        let get = |wanted: Column| {
            columns
                .iter()
                .position(|c| *c == Some(wanted))
                .and_then(|i| row.get(i))
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
        };
        let shown = clip(&row.join(&separator.to_string()));
        let refuse = |reason| Refused {
            line,
            text: shown.clone(),
            reason,
        };
        let hostname = get(Column::Hostname);
        let Some(address) = get(Column::Address).or(hostname) else {
            parsed.refused.push(refuse(Reason::NoAddress));
            continue;
        };
        if address.len() > MAX_FIELD {
            parsed.refused.push(refuse(Reason::TooLong));
        } else if !is_address(address) {
            parsed.refused.push(refuse(Reason::NotAnAddress));
        } else if hostname.is_some_and(|n| !is_name(n)) {
            parsed.refused.push(refuse(Reason::NotAName));
        } else if [get(Column::Os), get(Column::Notes)]
            .into_iter()
            .flatten()
            .any(|v| v.chars().count() > 1000)
        {
            parsed.refused.push(refuse(Reason::TooLong));
        } else {
            let hostname = hostname
                .filter(|n| !n.eq_ignore_ascii_case(address))
                .map(str::to_string);
            push(
                &mut parsed,
                line,
                &shown,
                Target {
                    address: address.to_string(),
                    hostname,
                    os: get(Column::Os).map(str::to_string),
                    tags: get(Column::Tags).map(tags_of).unwrap_or_default(),
                    notes: get(Column::Notes).map(str::to_string),
                },
            );
        }
    }
    parsed
}

pub fn parse(text: &str) -> Result<Parsed> {
    if text.len() > MAX_BYTES {
        return Err(OpsError::Invalid(format!(
            "input too long: {} bytes, at most {MAX_BYTES}",
            text.len()
        )));
    }
    let first = text
        .trim_start_matches('\u{feff}')
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .unwrap_or("");
    let separator = separator(first);
    let headers: Vec<Option<Column>> = first
        .split(separator)
        .map(|h| column_of(h.trim().trim_matches('"')))
        .collect();
    let is_csv = headers.iter().all(Option::is_some)
        && headers
            .iter()
            .any(|c| matches!(c, Some(Column::Address | Column::Hostname)));
    Ok(if is_csv {
        parse_csv(text)
    } else {
        parse_text(text)
    })
}

pub fn manual(address: &str, hostname: &str, os: &str, tags: &str) -> Parsed {
    let (address, hostname, os) = (address.trim(), hostname.trim(), os.trim());
    let shown = clip(&format!("{address} {hostname}"));
    let refuse = |reason| Parsed {
        targets: Vec::new(),
        refused: vec![Refused {
            line: 1,
            text: shown.clone(),
            reason,
        }],
    };
    if address.is_empty() {
        return refuse(Reason::NoAddress);
    }
    if !is_address(address) {
        return refuse(Reason::NotAnAddress);
    }
    if !hostname.is_empty() && !is_name(hostname) {
        return refuse(Reason::NotAName);
    }
    if os.chars().count() > 1000 {
        return refuse(Reason::TooLong);
    }
    let target = Target {
        address: address.to_string(),
        hostname: Some(hostname.to_string()).filter(|h| !h.is_empty()),
        os: Some(os.to_string()).filter(|o| !o.is_empty()),
        tags: tags_of(tags),
        notes: None,
    };
    Parsed {
        targets: vec![target],
        refused: Vec::new(),
    }
}

fn same(host: &Host, target: &Target) -> bool {
    host.ip.eq_ignore_ascii_case(&target.address)
        || host
            .hostname
            .as_deref()
            .zip(Some(target.address.as_str()))
            .is_some_and(|(h, a)| h.eq_ignore_ascii_case(a))
}

pub fn preview(engagement_dir: &std::path::Path, parsed: Parsed) -> Preview {
    let present = hosts::list(engagement_dir);
    let mut out = Preview {
        refused: parsed.refused,
        ..Preview::default()
    };
    for target in parsed.targets {
        if present.iter().any(|host| same(host, &target)) {
            out.known.push(target);
        } else {
            out.create.push(target);
        }
    }
    out
}

pub fn create(engagement_dir: &std::path::Path, targets: &[Target]) -> Result<ImportReport> {
    let present = hosts::list(engagement_dir);
    let mut report = ImportReport::default();
    for target in targets {
        if present.iter().any(|host| same(host, target)) {
            continue;
        }
        let host = Host {
            path: PathBuf::new(),
            ip: target.address.clone(),
            hostname: target.hostname.clone(),
            os: target.os.clone(),
            status: HostStatus::New,
            services: Vec::new(),
            tags: target.tags.clone(),
            flags: Vec::new(),
            points: None,
        };
        let path = hosts::upsert(engagement_dir, &host)?;
        if let Some(notes) = &target.notes {
            let text = std::fs::read_to_string(&path).map_err(|source| OpsError::Io {
                path: path.clone(),
                source,
            })?;
            let written = text.replacen(
                "## Notes\n\n",
                &format!("## Notes\n\n{}\n\n", notes.trim()),
                1,
            );
            crate::fsutil::write_atomic(&path, &written)?;
        }
        report.created.push(
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addresses(parsed: &Parsed) -> Vec<&str> {
        parsed.targets.iter().map(|t| t.address.as_str()).collect()
    }

    #[test]
    fn a_text_list_keeps_addresses_names_and_ranges_and_drops_comments() {
        let parsed = parse_text(
            "# scope\n10.0.0.5\n\n10.0.1.0/24  # office\ndc01.corp.local\n2001:db8::1\n192.0.2.7 web01\n  \n",
        );
        assert_eq!(
            addresses(&parsed),
            [
                "10.0.0.5",
                "10.0.1.0/24",
                "dc01.corp.local",
                "2001:db8::1",
                "192.0.2.7"
            ]
        );
        assert_eq!(parsed.targets[4].hostname.as_deref(), Some("web01"));
        assert!(parsed.refused.is_empty());
    }

    #[test]
    fn a_text_line_is_refused_with_its_line_and_its_reason() {
        let parsed = parse_text(
            "10.0.0.5\n!!bad\n10.0.0.0/33\n10.0.0.6 web01 extra\n10.0.0.5\n999.1.1.1/8\n-bad.example\n10.0.0.7 bad_name!",
        );
        assert_eq!(addresses(&parsed), ["10.0.0.5"]);
        let reasons: Vec<(usize, Reason)> =
            parsed.refused.iter().map(|r| (r.line, r.reason)).collect();
        assert_eq!(
            reasons,
            [
                (2, Reason::NotAnAddress),
                (3, Reason::NotAnAddress),
                (4, Reason::ExtraText),
                (5, Reason::Repeated),
                (6, Reason::NotAnAddress),
                (7, Reason::NotAnAddress),
                (8, Reason::NotAName)
            ]
        );
    }

    #[test]
    fn a_csv_is_read_by_its_headers_whatever_their_case_and_separator() {
        let comma = "IP,Hostname,OS,Tags,Notes\n10.0.0.5,dc01,Windows Server 2019,\"ad, dc\",\"first, with a comma\"\n10.0.0.6,,Linux,,\n";
        let parsed = parse(comma).unwrap();
        assert_eq!(addresses(&parsed), ["10.0.0.5", "10.0.0.6"]);
        let first = &parsed.targets[0];
        assert_eq!(
            (
                first.hostname.as_deref(),
                first.os.as_deref(),
                first.notes.as_deref()
            ),
            (
                Some("dc01"),
                Some("Windows Server 2019"),
                Some("first, with a comma")
            )
        );
        assert_eq!(first.tags, ["ad", "dc"]);
        let semicolon = parse("address;name;tags\n10.0.0.5;web01;a;b\n").unwrap();
        assert_eq!(semicolon.targets[0].hostname.as_deref(), Some("web01"));
        let tab = parse("ip\thostname\n10.0.0.5\tweb01\n").unwrap();
        assert_eq!(addresses(&tab), ["10.0.0.5"]);
    }

    #[test]
    fn a_csv_row_without_address_takes_its_name_or_is_refused() {
        let parsed = parse("hostname,os\ndc01.corp.local,Windows\n,Linux\n").unwrap();
        assert_eq!(addresses(&parsed), ["dc01.corp.local"]);
        assert_eq!(parsed.targets[0].hostname, None);
        assert_eq!(
            parsed
                .refused
                .iter()
                .map(|r| (r.line, r.reason))
                .collect::<Vec<_>>(),
            [(3, Reason::NoAddress)]
        );
    }

    #[test]
    fn a_quoted_field_may_hold_a_line_break_and_the_next_line_number_follows() {
        let parsed = parse_csv("ip,notes\n10.0.0.5,\"two\nlines\"\nbad value,x\n");
        assert_eq!(parsed.targets[0].notes.as_deref(), Some("two\nlines"));
        assert_eq!(parsed.refused[0].line, 4);
    }

    #[test]
    fn a_list_that_only_looks_like_a_header_stays_a_list() {
        assert_eq!(parse("host01\nhost02\n").unwrap().targets.len(), 2);
        assert_eq!(
            parse("# comment\nname\n10.0.0.5\n").unwrap().targets.len(),
            1
        );
    }

    #[test]
    fn input_over_the_bounds_is_refused_or_cut() {
        assert!(parse(&"a".repeat(MAX_BYTES + 1)).is_err());
        let many: String = (0..MAX_TARGETS + 5)
            .map(|i| format!("h{i}.example\n"))
            .collect();
        let parsed = parse(&many).unwrap();
        assert_eq!(
            (parsed.targets.len(), parsed.refused.len()),
            (MAX_TARGETS, 5)
        );
        assert!(parsed.refused.iter().all(|r| r.reason == Reason::TooMany));
        assert_eq!(
            parse_text(&"a".repeat(300)).refused[0].reason,
            Reason::TooLong
        );
    }

    #[test]
    fn the_manual_form_checks_the_same_rules() {
        let ok = manual(" 10.0.0.5 ", "web01", "Linux", "web, dmz");
        assert_eq!(
            (
                ok.targets[0].address.as_str(),
                ok.targets[0].hostname.as_deref(),
                ok.targets[0].os.as_deref(),
                ok.targets[0].tags.clone()
            ),
            (
                "10.0.0.5",
                Some("web01"),
                Some("Linux"),
                vec!["web".to_string(), "dmz".to_string()]
            )
        );
        assert_eq!(manual("", "", "", "").refused[0].reason, Reason::NoAddress);
        assert_eq!(
            manual("a b", "", "", "").refused[0].reason,
            Reason::NotAnAddress
        );
        assert_eq!(
            manual("10.0.0.5", "bad name", "", "").refused[0].reason,
            Reason::NotAName
        );
    }

    #[test]
    fn creating_never_overwrites_a_host_the_engagement_has() {
        let dir = crate::fsutil::test_dir("targets-create");
        std::fs::create_dir_all(dir.join("Hosts")).unwrap();
        let known = Host {
            path: PathBuf::new(),
            ip: "10.0.0.5".into(),
            hostname: Some("dc01".into()),
            os: Some("Windows".into()),
            status: HostStatus::Owned,
            services: vec![],
            tags: vec!["keep".into()],
            flags: vec![],
            points: None,
        };
        hosts::upsert(&dir, &known).unwrap();
        let parsed = parse("ip,hostname,os,tags,notes\n10.0.0.5,other,Linux,new,x\nDC01,,,,\n10.0.0.9,web09,Linux,web,Seen on the portal\n").unwrap();
        let shown = preview(&dir, parsed);
        assert_eq!((shown.known.len(), shown.create.len()), (2, 1));
        let report = create(&dir, &shown.create).unwrap();
        assert_eq!(report.created, ["web09"]);
        let report = create(&dir, &[shown.known[0].clone()]).unwrap();
        assert!(report.created.is_empty());
        let all = hosts::list(&dir);
        assert_eq!(all.len(), 2);
        let first = all.iter().find(|h| h.ip == "10.0.0.5").unwrap();
        assert_eq!(
            (
                first.hostname.as_deref(),
                first.os.as_deref(),
                first.status,
                first.tags.clone()
            ),
            (
                Some("dc01"),
                Some("Windows"),
                HostStatus::Owned,
                vec!["keep".to_string()]
            )
        );
        let created = all.iter().find(|h| h.ip == "10.0.0.9").unwrap();
        assert_eq!(
            (created.status, created.tags.clone()),
            (HostStatus::New, vec!["web".to_string()])
        );
        assert!(
            std::fs::read_to_string(&created.path)
                .unwrap()
                .contains("## Notes\n\nSeen on the portal")
        );
    }
}
