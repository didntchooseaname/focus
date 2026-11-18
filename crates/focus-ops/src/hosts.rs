use std::path::{Path, PathBuf};

use crate::frontmatter::Document;
use crate::fsutil;
use crate::{Host, HostStatus, OpsError, Result, Service};

#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub services: usize,
}

pub(crate) fn status_name(status: HostStatus) -> &'static str {
    match status {
        HostStatus::New => "new",
        HostStatus::Enum => "enum",
        HostStatus::Foothold => "foothold",
        HostStatus::User => "user",
        HostStatus::Owned => "owned",
    }
}

fn status_from(name: &str) -> Option<HostStatus> {
    [
        HostStatus::New,
        HostStatus::Enum,
        HostStatus::Foothold,
        HostStatus::User,
        HostStatus::Owned,
    ]
    .into_iter()
    .find(|s| status_name(*s) == name)
}

const TABLE_HEADER: &str = "| Port | Proto | State | Service | Product | Version | Notes |";
const TABLE_RULE: &str = "|---|---|---|---|---|---|---|";

fn cell(text: &str) -> String {
    text.replace('|', "\\|")
        .replace(['\n', '\r'], " ")
        .trim()
        .to_string()
}

pub(crate) fn split_row(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = line
        .strip_suffix('|')
        .filter(|_| !line.ends_with("\\|"))
        .unwrap_or(line);
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                current.push('|');
                chars.next();
            }
            '|' => cells.push(std::mem::take(&mut current).trim().to_string()),
            _ => current.push(c),
        }
    }
    cells.push(current.trim().to_string());
    cells
}

fn services_table(services: &[Service]) -> Vec<String> {
    let mut lines = vec![TABLE_HEADER.to_string(), TABLE_RULE.to_string()];
    for s in services {
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} | {} |",
            s.port,
            cell(&s.proto),
            cell(&s.state),
            cell(&s.name),
            cell(&s.product),
            cell(&s.version),
            cell(&s.notes)
        ));
    }
    lines
}

fn parse_services(body: &str) -> Vec<Service> {
    let mut in_section = false;
    let mut services = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("## ") {
            in_section = title.trim().eq_ignore_ascii_case("services");
            continue;
        }
        if !in_section || !trimmed.starts_with('|') {
            continue;
        }
        let cells = split_row(trimmed);
        let Some(port) = cells.first().and_then(|p| p.parse::<u16>().ok()) else {
            continue;
        };
        let get = |i: usize| cells.get(i).cloned().unwrap_or_default();
        services.push(Service {
            port,
            proto: get(1),
            state: get(2),
            name: get(3),
            product: get(4),
            version: get(5),
            notes: get(6),
        });
    }
    services
}

fn replace_services(body: &str, services: &[Service]) -> String {
    replace_table(body, "Services", services_table(services))
}

pub(crate) fn replace_table(body: &str, title: &str, table: Vec<String>) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let heading = lines.iter().position(|l| {
        l.trim()
            .strip_prefix("## ")
            .is_some_and(|t| t.trim().eq_ignore_ascii_case(title))
    });
    let mut out: Vec<String> = Vec::new();
    match heading {
        None => {
            out.extend(lines.iter().map(|l| l.to_string()));
            if out.last().is_some_and(|l| !l.is_empty()) {
                out.push(String::new());
            }
            out.push(format!("## {title}"));
            out.push(String::new());
            out.extend(table);
        }
        Some(h) => {
            out.extend(lines[..=h].iter().map(|l| l.to_string()));
            let mut i = h + 1;
            while i < lines.len() && lines[i].trim().is_empty() {
                i += 1;
            }
            let leading_blank = i - h - 1;
            let table_start = i;
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                i += 1;
            }
            let had_table = i > table_start;
            out.extend(std::iter::repeat_n(
                String::new(),
                if had_table { leading_blank.max(1) } else { 1 },
            ));
            out.extend(table);
            let rest_start = if had_table { i } else { h + 1 };
            let mut tail: Vec<String> = lines[rest_start..].iter().map(|l| l.to_string()).collect();
            if !had_table {
                while tail.first().is_some_and(|l| l.trim().is_empty()) {
                    tail.remove(0);
                }
            }
            if !tail.is_empty() && !tail[0].trim().is_empty() {
                out.push(String::new());
            }
            out.extend(tail);
        }
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

fn read_host(path: &Path) -> Result<Host> {
    let text = fsutil::read(path)?;
    let doc = Document::parse(&text);
    let ip = doc.scalar("ip").ok_or_else(|| OpsError::Parse {
        file: path.to_path_buf(),
        message: "missing `ip` key in the frontmatter".into(),
    })?;
    let status = match doc.scalar("status") {
        None => HostStatus::New,
        Some(s) => status_from(&s).ok_or_else(|| OpsError::Parse {
            file: path.to_path_buf(),
            message: format!("unknown host status: {s}"),
        })?,
    };
    Ok(Host {
        path: fsutil::absolute(path),
        ip,
        hostname: doc.scalar("hostname"),
        os: doc.scalar("os"),
        status,
        services: parse_services(doc.body()),
        tags: doc.list("tags"),
        flags: doc.list("flags"),
        points: doc.scalar("points").and_then(|p| p.parse().ok()),
    })
}

pub fn list(engagement_dir: &Path) -> Vec<Host> {
    let Ok(entries) = std::fs::read_dir(engagement_dir.join("Hosts")) else {
        return Vec::new();
    };
    let mut hosts: Vec<Host> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "md")
                && !p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .filter_map(|p| read_host(&p).ok())
        .collect();
    hosts.sort_by(|a, b| {
        ip_key(&a.ip)
            .cmp(&ip_key(&b.ip))
            .then_with(|| a.hostname.cmp(&b.hostname))
    });
    hosts
}

fn ip_key(ip: &str) -> (u8, u32, String) {
    match ip.parse::<std::net::Ipv4Addr>() {
        Ok(v4) => (0, u32::from(v4), String::new()),
        Err(_) => (1, 0, ip.to_string()),
    }
}

fn note_stem(host: &Host) -> String {
    let by_name = host
        .hostname
        .as_deref()
        .map(fsutil::file_name)
        .filter(|n| !n.is_empty());
    by_name.unwrap_or_else(|| fsutil::file_name(&host.ip))
}

fn find_by_ip(engagement_dir: &Path, ip: &str) -> Option<Host> {
    list(engagement_dir).into_iter().find(|h| h.ip == ip)
}

pub fn upsert(engagement_dir: &Path, host: &Host) -> Result<PathBuf> {
    if host.ip.trim().is_empty() {
        return Err(OpsError::Invalid("the host has no address".into()));
    }
    let hosts_dir = engagement_dir.join("Hosts");
    let path = if !host.path.as_os_str().is_empty() && host.path.exists() {
        host.path.clone()
    } else if let Some(existing) = find_by_ip(engagement_dir, &host.ip) {
        existing.path
    } else {
        let mut stem = note_stem(host);
        if hosts_dir.join(format!("{stem}.md")).exists() {
            stem = format!("{stem} ({})", fsutil::file_name(&host.ip));
        }
        hosts_dir.join(format!("{stem}.md"))
    };
    let existing = fsutil::read_optional(&path)?;
    let mut doc = match &existing {
        Some(text) => Document::parse(text),
        None => {
            let title = host
                .hostname
                .as_deref()
                .map_or_else(|| host.ip.clone(), |n| format!("{n} ({})", host.ip));
            let mut doc = Document::parse(&format!("# {title}\n\n## Notes\n\n"));
            doc.set_body(format!("# {title}\n\n## Notes\n\n"));
            doc
        }
    };
    doc.set_scalar("ip", Some(&host.ip));
    doc.set_scalar("hostname", host.hostname.as_deref());
    doc.set_scalar("os", host.os.as_deref());
    doc.set_scalar("status", Some(status_name(host.status)));
    doc.set_list("tags", &host.tags);
    doc.set_list("flags", &host.flags);
    doc.set_scalar("points", host.points.map(|p| p.to_string()).as_deref());
    let body = replace_services(doc.body(), &host.services);
    doc.set_body(body);
    fsutil::write_atomic(&path, &doc.render())?;
    Ok(fsutil::absolute(&path))
}

pub fn import(engagement_dir: &Path, hosts: &[Host]) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    for incoming in hosts {
        match find_by_ip(engagement_dir, &incoming.ip) {
            None => {
                let mut fresh = incoming.clone();
                fresh.path = PathBuf::new();
                let path = upsert(engagement_dir, &fresh)?;
                report.services += fresh.services.len();
                report.created.push(stem_of(&path));
            }
            Some(mut known) => {
                let mut changed = merge_identity(&mut known, incoming);
                for service in &incoming.services {
                    match known
                        .services
                        .iter_mut()
                        .find(|s| s.port == service.port && s.proto == service.proto)
                    {
                        Some(existing) => {
                            let updated = Service {
                                notes: existing.notes.clone(),
                                ..service.clone()
                            };
                            if *existing != updated {
                                *existing = updated;
                                report.services += 1;
                                changed = true;
                            }
                        }
                        None => {
                            known.services.push(service.clone());
                            report.services += 1;
                            changed = true;
                        }
                    }
                }
                known
                    .services
                    .sort_by(|a, b| (a.proto.as_str(), a.port).cmp(&(b.proto.as_str(), b.port)));
                if changed {
                    let path = upsert(engagement_dir, &known)?;
                    report.updated.push(stem_of(&path));
                }
            }
        }
    }
    Ok(report)
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn merge_identity(known: &mut Host, incoming: &Host) -> bool {
    let mut changed = false;
    if known.hostname.is_none() && incoming.hostname.is_some() {
        known.hostname.clone_from(&incoming.hostname);
        changed = true;
    }
    if known.os.is_none() && incoming.os.is_some() {
        known.os.clone_from(&incoming.os);
        changed = true;
    }
    changed
}

pub fn parse_nmap(text: &str) -> Result<Vec<Host>> {
    let head = text.trim_start_matches('\u{feff}').trim_start();
    if head.starts_with("<?xml") || head.starts_with("<nmaprun") {
        return nmap_xml(text);
    }
    if text
        .lines()
        .any(|l| l.starts_with("Host: ") && (l.contains("\tPorts: ") || l.contains("\tStatus: ")))
    {
        return Ok(nmap_grepable(text));
    }
    if text.lines().any(|l| l.starts_with("Nmap scan report for ")) {
        return Ok(nmap_normal(text));
    }
    Err(OpsError::Invalid(
        "unrecognized nmap output: not XML (-oX), grepable (-oG) or normal (-oN)".into(),
    ))
}

fn new_host(ip: String, hostname: Option<String>) -> Host {
    Host {
        path: PathBuf::new(),
        ip,
        hostname: hostname.filter(|h| !h.is_empty()),
        os: None,
        status: HostStatus::New,
        services: Vec::new(),
        tags: Vec::new(),
        flags: Vec::new(),
        points: None,
    }
}

fn is_open(state: &str) -> bool {
    state == "open" || state == "open|filtered"
}

fn nmap_xml(text: &str) -> Result<Vec<Host>> {
    let parse_error = |e: roxmltree::Error| OpsError::Parse {
        file: PathBuf::new(),
        message: format!("unreadable nmap XML: {e}"),
    };
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = roxmltree::Document::parse_with_options(text.trim_start_matches('\u{feff}'), options)
        .map_err(parse_error)?;
    let mut hosts = Vec::new();
    for node in doc.descendants().filter(|n| n.has_tag_name("host")) {
        let up = node
            .children()
            .find(|c| c.has_tag_name("status"))
            .and_then(|s| s.attribute("state"))
            == Some("up");
        if !up {
            continue;
        }
        let address = |kind: &str| {
            node.children()
                .filter(|c| c.has_tag_name("address"))
                .find(|a| a.attribute("addrtype") == Some(kind))
                .and_then(|a| a.attribute("addr"))
        };
        let Some(ip) = address("ipv4").or_else(|| address("ipv6")) else {
            continue;
        };
        let names: Vec<_> = node
            .descendants()
            .filter(|n| n.has_tag_name("hostname"))
            .collect();
        let name = names
            .iter()
            .find(|n| n.attribute("type") == Some("user"))
            .or_else(|| names.first())
            .and_then(|n| n.attribute("name"));
        let mut host = new_host(ip.to_string(), name.map(str::to_string));
        for port in node.descendants().filter(|n| n.has_tag_name("port")) {
            let state = port
                .children()
                .find(|c| c.has_tag_name("state"))
                .and_then(|s| s.attribute("state"))
                .unwrap_or("");
            let Some(number) = port.attribute("portid").and_then(|p| p.parse::<u16>().ok()) else {
                continue;
            };
            if !is_open(state) {
                continue;
            }
            let service = port.children().find(|c| c.has_tag_name("service"));
            let attr = |k: &str| {
                service
                    .and_then(|s| s.attribute(k))
                    .unwrap_or("")
                    .to_string()
            };
            let mut name = attr("name");
            if service.and_then(|s| s.attribute("tunnel")) == Some("ssl") && !name.is_empty() {
                name = format!("ssl/{name}");
            }
            let mut version = attr("version");
            let extra = attr("extrainfo");
            if !extra.is_empty() {
                version = if version.is_empty() {
                    format!("({extra})")
                } else {
                    format!("{version} ({extra})")
                };
            }
            host.services.push(Service {
                port: number,
                proto: port.attribute("protocol").unwrap_or("tcp").to_string(),
                state: state.to_string(),
                name,
                product: attr("product"),
                version,
                notes: String::new(),
            });
        }
        host.os = node
            .descendants()
            .filter(|n| n.has_tag_name("osmatch"))
            .max_by_key(|n| {
                n.attribute("accuracy")
                    .and_then(|a| a.parse::<u32>().ok())
                    .unwrap_or(0)
            })
            .and_then(|n| n.attribute("name"))
            .map(str::to_string)
            .or_else(|| {
                node.descendants()
                    .filter(|n| n.has_tag_name("service"))
                    .find_map(|s| s.attribute("ostype"))
                    .map(str::to_string)
            });
        hosts.push(host);
    }
    Ok(hosts)
}

fn split_product_version(info: &str) -> (String, String) {
    let words: Vec<&str> = info.split_whitespace().collect();
    let cut = words
        .iter()
        .position(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or(words.len());
    (words[..cut].join(" "), words[cut..].join(" "))
}

fn nmap_grepable(text: &str) -> Vec<Host> {
    let mut hosts: Vec<Host> = Vec::new();
    let mut down: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| l.starts_with("Host: ")) {
        let mut fields = line.split('\t');
        let ident = fields.next().unwrap_or("").trim_start_matches("Host: ");
        let (ip, name) = match ident.split_once(' ') {
            Some((ip, rest)) => (
                ip,
                rest.trim().trim_start_matches('(').trim_end_matches(')'),
            ),
            None => (ident, ""),
        };
        if ip.is_empty() {
            continue;
        }
        let position = match hosts.iter().position(|h| h.ip == ip) {
            Some(p) => p,
            None => {
                hosts.push(new_host(ip.to_string(), Some(name.to_string())));
                hosts.len() - 1
            }
        };
        for field in fields {
            if let Some(status) = field.strip_prefix("Status: ") {
                if status.trim().eq_ignore_ascii_case("down") {
                    down.push(ip.to_string());
                }
            } else if let Some(ports) = field.strip_prefix("Ports: ") {
                for entry in ports.split(", ") {
                    let parts: Vec<&str> = entry.trim().split('/').collect();
                    if parts.len() < 5 {
                        continue;
                    }
                    let Ok(number) = parts[0].trim().parse::<u16>() else {
                        continue;
                    };
                    if !is_open(parts[1]) {
                        continue;
                    }
                    let (product, version) =
                        split_product_version(parts.get(6).copied().unwrap_or(""));
                    hosts[position].services.push(Service {
                        port: number,
                        proto: parts[2].to_string(),
                        state: parts[1].to_string(),
                        name: parts[4].replace('|', "/"),
                        product,
                        version,
                        notes: String::new(),
                    });
                }
            } else if let Some(os) = field.strip_prefix("OS: ") {
                let os = os.trim();
                if !os.is_empty() {
                    hosts[position].os = Some(os.to_string());
                }
            }
        }
    }
    hosts.retain(|h| !down.contains(&h.ip));
    hosts
}

fn nmap_normal(text: &str) -> Vec<Host> {
    let mut hosts: Vec<Host> = Vec::new();
    let mut current: Option<Host> = None;
    let mut up = false;
    let mut columns: Option<(usize, usize, Option<usize>)> = None;
    let mut os_details: Option<String> = None;
    let mut service_os: Option<String> = None;
    let mut finish = |current: &mut Option<Host>,
                      up: bool,
                      os_details: &mut Option<String>,
                      service_os: &mut Option<String>| {
        if let Some(mut host) = current.take() {
            host.os = os_details.take().or_else(|| service_os.take());
            if up {
                hosts.push(host);
            }
        }
        *os_details = None;
        *service_os = None;
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Nmap scan report for ") {
            finish(&mut current, up, &mut os_details, &mut service_os);
            up = false;
            columns = None;
            let rest = rest.trim();
            let (name, ip) = match rest.rsplit_once(" (") {
                Some((name, ip)) if ip.ends_with(')') => {
                    (Some(name.to_string()), ip.trim_end_matches(')').to_string())
                }
                _ => (None, rest.to_string()),
            };
            current = Some(new_host(ip, name));
            continue;
        }
        let Some(host) = current.as_mut() else {
            continue;
        };
        if line.starts_with("Host is up") {
            up = true;
        } else if line.starts_with("PORT") && line.contains("STATE") && line.contains("SERVICE") {
            let find = |w: &str| line.find(w);
            columns = Some((
                find("STATE").unwrap_or(0),
                find("SERVICE").unwrap_or(0),
                find("VERSION"),
            ));
        } else if let Some(rest) = line.strip_prefix("OS details: ") {
            os_details = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Aggressive OS guesses: ") {
            if os_details.is_none() {
                os_details = rest
                    .split(", ")
                    .next()
                    .map(|g| g.split(" (").next().unwrap_or(g).to_string());
            }
        } else if let Some(rest) = line.strip_prefix("Service Info: ") {
            service_os = rest
                .split("; ")
                .find_map(|p| p.strip_prefix("OS: "))
                .map(str::to_string);
        } else if let (Some((state_at, service_at, version_at)), true) = (
            columns,
            line.chars().next().is_some_and(|c| c.is_ascii_digit()),
        ) {
            let Some((port_proto, _)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            let Some((number, proto)) = port_proto.split_once('/') else {
                continue;
            };
            let Ok(number) = number.parse::<u16>() else {
                continue;
            };
            let slice = |from: usize, to: Option<usize>| -> String {
                let chars: Vec<char> = line.chars().collect();
                let end = to.unwrap_or(chars.len()).min(chars.len());
                if from >= end {
                    String::new()
                } else {
                    chars[from..end]
                        .iter()
                        .collect::<String>()
                        .trim()
                        .to_string()
                }
            };
            let state = slice(state_at, Some(service_at));
            if !is_open(&state) {
                continue;
            }
            let name = slice(service_at, version_at);
            let (product, version) =
                split_product_version(&version_at.map(|v| slice(v, None)).unwrap_or_default());
            host.services.push(Service {
                port: number,
                proto: proto.to_string(),
                state,
                name,
                product,
                version,
                notes: String::new(),
            });
        }
    }
    finish(&mut current, up, &mut os_details, &mut service_os);
    hosts
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = include_str!("../tests/data/scan.xml");
    const GREPABLE: &str = include_str!("../tests/data/scan.gnmap");
    const NORMAL: &str = include_str!("../tests/data/scan.nmap");

    fn ports(host: &Host) -> Vec<(u16, &str, &str, &str)> {
        host.services
            .iter()
            .map(|s| (s.port, s.proto.as_str(), s.state.as_str(), s.name.as_str()))
            .collect()
    }

    #[test]
    fn xml_output() {
        let hosts = parse_nmap(XML).unwrap();
        assert_eq!(hosts.len(), 2, "the down host is dropped");
        let web = &hosts[0];
        assert_eq!(
            (web.ip.as_str(), web.hostname.as_deref(), web.os.as_deref()),
            (
                "192.0.2.5",
                Some("srv-web.example.test"),
                Some("Linux 5.4 - 5.15")
            )
        );
        assert_eq!(
            ports(web),
            [
                (22, "tcp", "open", "ssh"),
                (80, "tcp", "open", "http"),
                (443, "tcp", "open", "ssl/http")
            ]
        );
        assert_eq!(
            (
                web.services[0].product.as_str(),
                web.services[0].version.as_str()
            ),
            (
                "OpenSSH",
                "8.9p1 Ubuntu 3ubuntu0.1 (Ubuntu Linux; protocol 2.0)"
            )
        );
        let windows = &hosts[1];
        assert_eq!(
            (windows.hostname.clone(), windows.os.as_deref()),
            (None, Some("Windows"))
        );
        assert_eq!(
            ports(windows),
            [
                (445, "tcp", "open", "microsoft-ds"),
                (161, "udp", "open|filtered", "snmp")
            ]
        );
    }

    #[test]
    fn grepable_output() {
        let hosts = parse_nmap(GREPABLE).unwrap();
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].hostname.as_deref(), Some("srv-web.example.test"));
        assert_eq!(
            ports(&hosts[0]),
            [
                (22, "tcp", "open", "ssh"),
                (80, "tcp", "open", "http"),
                (443, "tcp", "open", "ssl/http")
            ]
        );
        assert_eq!(
            (
                hosts[0].services[0].product.as_str(),
                hosts[0].services[0].version.as_str()
            ),
            (
                "OpenSSH",
                "8.9p1 Ubuntu 3ubuntu0.1 (Ubuntu Linux; protocol 2.0)"
            )
        );
        assert_eq!(hosts[1].hostname, None);
        assert_eq!(
            ports(&hosts[1]),
            [
                (445, "tcp", "open", "microsoft-ds"),
                (161, "udp", "open|filtered", "snmp")
            ]
        );
    }

    #[test]
    fn normal_output() {
        let hosts = parse_nmap(NORMAL).unwrap();
        assert_eq!(hosts.len(), 2);
        let web = &hosts[0];
        assert_eq!(
            (web.ip.as_str(), web.hostname.as_deref(), web.os.as_deref()),
            (
                "192.0.2.5",
                Some("srv-web.example.test"),
                Some("Linux 5.4 - 5.15")
            )
        );
        assert_eq!(
            ports(web),
            [
                (22, "tcp", "open", "ssh"),
                (80, "tcp", "open", "http"),
                (443, "tcp", "open", "ssl/http")
            ]
        );
        assert_eq!(
            (
                web.services[1].product.as_str(),
                web.services[1].version.as_str()
            ),
            ("nginx", "1.18.0")
        );
        let windows = &hosts[1];
        assert_eq!(
            (
                windows.ip.as_str(),
                windows.hostname.clone(),
                windows.os.as_deref()
            ),
            ("192.0.2.6", None, Some("Windows"))
        );
        assert_eq!(
            ports(windows),
            [
                (445, "tcp", "open", "microsoft-ds"),
                (161, "udp", "open|filtered", "snmp")
            ]
        );
    }

    #[test]
    fn unknown_and_broken_input() {
        assert!(matches!(parse_nmap("bonjour"), Err(OpsError::Invalid(_))));
        assert!(matches!(
            parse_nmap("<?xml version=\"1.0\"?><nmaprun><host>"),
            Err(OpsError::Parse { .. })
        ));
        assert!(
            parse_nmap("<?xml version=\"1.0\"?><nmaprun></nmaprun>")
                .unwrap()
                .is_empty()
        );
    }

    fn sample(ip: &str, name: Option<&str>, ports: &[u16]) -> Host {
        let mut host = new_host(ip.into(), name.map(str::to_string));
        host.services = ports
            .iter()
            .map(|p| Service {
                port: *p,
                proto: "tcp".into(),
                state: "open".into(),
                name: "svc".into(),
                product: String::new(),
                version: String::new(),
                notes: String::new(),
            })
            .collect();
        host
    }

    #[test]
    fn upsert_keeps_free_text_and_rewrites_only_frontmatter_and_table() {
        let dir = fsutil::test_dir("hosts-upsert");
        let mut host = sample("192.0.2.5", Some("srv-web"), &[22, 80]);
        host.tags = vec!["web".into()];
        let path = upsert(&dir, &host).unwrap();
        assert_eq!(path, fsutil::absolute(&dir.join("Hosts/srv-web.md")));
        let mut text = std::fs::read_to_string(&path).unwrap();
        text = text.replace("## Notes\n\n", "## Notes\n\nDefault landing page.\n\n")
            + "\n## Other section\n\nText after the table.\n";
        std::fs::write(&path, text).unwrap();

        let mut loaded = list(&dir).remove(0);
        loaded.status = HostStatus::Foothold;
        loaded.flags = vec!["local.txt".into()];
        loaded.points = Some(10);
        loaded.services[0].notes = "key | allowed".into();
        loaded
            .services
            .push(sample("x", None, &[8080]).services.remove(0));
        upsert(&dir, &loaded).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            after.contains("Default landing page.")
                && after.contains("## Other section\n\nText after the table.\n")
        );
        let again = list(&dir).remove(0);
        assert_eq!(
            (again.status, again.points, again.flags.clone()),
            (
                HostStatus::Foothold,
                Some(10),
                vec!["local.txt".to_string()]
            )
        );
        assert_eq!(again.services.len(), 3);
        assert_eq!(again.services[0].notes, "key | allowed");
        assert_eq!(again.tags, ["web"]);
    }

    #[test]
    fn import_merges_without_losing_notes_or_status() {
        let dir = fsutil::test_dir("hosts-import");
        let first = parse_nmap(XML).unwrap();
        let report = import(&dir, &first).unwrap();
        assert_eq!(report.created, ["srv-web.example.test", "192.0.2.6"]);
        assert_eq!((report.updated.len(), report.services), (0, 5));

        let mut web = list(&dir)
            .into_iter()
            .find(|h| h.ip == "192.0.2.5")
            .unwrap();
        web.status = HostStatus::Enum;
        web.services[0].notes = "key access only".into();
        web.hostname = Some("name typed by hand".into());
        upsert(&dir, &web).unwrap();

        let mut second = parse_nmap(XML).unwrap();
        second[0].services.push(Service {
            port: 3306,
            proto: "tcp".into(),
            state: "open".into(),
            name: "mysql".into(),
            product: "MariaDB".into(),
            version: "10.6".into(),
            notes: String::new(),
        });
        second[0].services[1].version = "1.20.1".into();
        let report = import(&dir, &second).unwrap();
        assert!(report.created.is_empty());
        assert_eq!(report.updated.len(), 1);
        assert_eq!(report.services, 2);
        let web = list(&dir)
            .into_iter()
            .find(|h| h.ip == "192.0.2.5")
            .unwrap();
        assert_eq!(web.status, HostStatus::Enum);
        assert_eq!(web.hostname.as_deref(), Some("name typed by hand"));
        assert_eq!(
            web.services.iter().find(|s| s.port == 22).unwrap().notes,
            "key access only"
        );
        assert_eq!(
            web.services.iter().find(|s| s.port == 80).unwrap().version,
            "1.20.1"
        );
        assert!(web.services.iter().any(|s| s.port == 3306));

        let third = import(&dir, &second).unwrap();
        assert!(
            third.created.is_empty() && third.updated.is_empty() && third.services == 0,
            "un second import identique ne change rien"
        );
    }

    #[test]
    fn same_name_other_address_gets_a_distinct_file_and_listing_is_sorted() {
        let dir = fsutil::test_dir("hosts-names");
        upsert(&dir, &sample("192.0.2.20", Some("dc"), &[88])).unwrap();
        upsert(&dir, &sample("192.0.2.3", Some("dc"), &[88])).unwrap();
        upsert(&dir, &sample("192.0.2.100", None, &[22])).unwrap();
        let ips: Vec<String> = list(&dir).into_iter().map(|h| h.ip).collect();
        assert_eq!(ips, ["192.0.2.3", "192.0.2.20", "192.0.2.100"]);
        assert!(
            dir.join("Hosts/dc (192.0.2.3).md").exists()
                && dir.join("Hosts/192.0.2.100.md").exists()
        );
        assert!(matches!(
            upsert(&dir, &sample(" ", None, &[])),
            Err(OpsError::Invalid(_))
        ));
    }
}
