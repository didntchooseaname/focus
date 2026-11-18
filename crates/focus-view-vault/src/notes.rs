use std::path::{Path, PathBuf};

use focus_core::{NoteId, PropValue, Result, Vault, VaultError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    pub hour: u32,
    pub minute: u32,
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

fn weekday(date: Date) -> usize {
    const SHIFT: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let month = date.month.clamp(1, 12) as usize;
    let year = if month < 3 { date.year - 1 } else { date.year };
    let day = (year + year.div_euclid(4) - year.div_euclid(100)
        + year.div_euclid(400)
        + SHIFT[month - 1]
        + date.day as i32)
        .rem_euclid(7);
    day as usize
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

pub fn format_date(format: &str, date: Date, time: Time) -> String {
    let month = MONTHS[(date.month.clamp(1, 12) - 1) as usize];
    let day = WEEKDAYS[weekday(date)];
    let twelve = match time.hour % 12 {
        0 => 12,
        h => h,
    };
    let (upper, lower) = if time.hour < 12 {
        ("AM", "am")
    } else {
        ("PM", "pm")
    };
    let tokens: [(&str, String); 19] = [
        ("YYYY", format!("{:04}", date.year)),
        ("YY", format!("{:02}", date.year.rem_euclid(100))),
        ("MMMM", month.to_string()),
        ("MMM", month[..3].to_string()),
        ("MM", format!("{:02}", date.month)),
        ("M", date.month.to_string()),
        ("DD", format!("{:02}", date.day)),
        ("Do", ordinal(date.day)),
        ("D", date.day.to_string()),
        ("dddd", day.to_string()),
        ("ddd", day[..3].to_string()),
        ("HH", format!("{:02}", time.hour)),
        ("H", time.hour.to_string()),
        ("hh", format!("{twelve:02}")),
        ("h", twelve.to_string()),
        ("mm", format!("{:02}", time.minute)),
        ("m", time.minute.to_string()),
        ("A", upper.to_string()),
        ("a", lower.to_string()),
    ];
    let mut out = String::new();
    let mut rest = format;
    'scan: while !rest.is_empty() {
        if let Some(inner) = rest.strip_prefix('[')
            && let Some(close) = inner.find(']')
        {
            out.push_str(&inner[..close]);
            rest = &inner[close + 1..];
            continue;
        }
        for (token, value) in &tokens {
            if let Some(after) = rest.strip_prefix(token) {
                out.push_str(value);
                rest = after;
                continue 'scan;
            }
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(c);
        }
        rest = chars.as_str();
    }
    out
}

fn fill(template: &str, title: &str, date: Date, time: Time) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find("}}").map(|c| open + c) else {
            out.push_str(&rest[open..]);
            return out;
        };
        let inner = rest[open + 2..close].trim();
        let (name, format) = inner
            .split_once(':')
            .map_or((inner, None), |(n, f)| (n.trim(), Some(f.trim())));
        match name {
            "title" => out.push_str(title),
            "date" => out.push_str(&format_date(format.unwrap_or("YYYY-MM-DD"), date, time)),
            "time" => out.push_str(&format_date(format.unwrap_or("HH:mm"), date, time)),
            _ => out.push_str(&rest[open..close + 2]),
        }
        rest = &rest[close + 2..];
    }
    out.push_str(rest);
    out
}

pub fn daily_note(
    vault: &Vault,
    folder: &str,
    format: &str,
    date: Date,
    time: Time,
    template: Option<NoteId>,
) -> Result<(NoteId, bool)> {
    let name = format_date(
        if format.trim().is_empty() {
            "YYYY-MM-DD"
        } else {
            format
        },
        date,
        time,
    );
    let path = Path::new(folder.trim_matches('/')).join(format!("{name}.md"));
    if let Some(id) = vault.note_by_path(&path) {
        return Ok((id, false));
    }
    let content = match template {
        Some(t) => fill(&vault.read(t)?, &name, date, time),
        None => String::new(),
    };
    Ok((vault.create_note(&path, &content)?, true))
}

pub fn templates(vault: &Vault, folder: &str) -> Vec<(NoteId, PathBuf)> {
    let folder = Path::new(folder.trim_matches('/'));
    if folder.as_os_str().is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(NoteId, PathBuf)> = vault
        .summaries()
        .into_iter()
        .filter(|s| s.path.starts_with(folder))
        .map(|s| (s.id, s.path.to_path_buf()))
        .collect();
    found.sort_by(|a, b| a.1.cmp(&b.1));
    found
}

pub fn note_from_template(
    vault: &Vault,
    template: NoteId,
    folder: &str,
    title: &str,
    date: Date,
    time: Time,
) -> Result<NoteId> {
    note_from_text(vault, &vault.read(template)?, folder, title, date, time)
}

pub fn note_from_text(
    vault: &Vault,
    template: &str,
    folder: &str,
    title: &str,
    date: Date,
    time: Time,
) -> Result<NoteId> {
    let text = fill(template, title, date, time);
    let folder = Path::new(folder.trim_matches('/'));
    let mut n = 0;
    loop {
        let name = if n == 0 {
            format!("{title}.md")
        } else {
            format!("{title} {n}.md")
        };
        match vault.create_note(&folder.join(name), &text) {
            Err(VaultError::AlreadyExists(_)) => n += 1,
            result => return result,
        }
    }
}

fn split_frontmatter(text: &str) -> Option<(&str, usize)> {
    let first = text.split_inclusive('\n').next()?;
    if first.trim_end() != "---" {
        return None;
    }
    let mut offset = first.len();
    for line in text[first.len()..].split_inclusive('\n') {
        if matches!(line.trim_end(), "---" | "...") {
            return Some((&text[first.len()..offset], offset + line.len()));
        }
        offset += line.len();
    }
    None
}

fn entries(yaml: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in yaml.split_inclusive('\n') {
        let top = !line.starts_with([' ', '\t', '-', '#']) && !line.trim().is_empty();
        match line.split_once(':') {
            Some((key, _)) if top => out.push((
                key.trim().trim_matches(['"', '\'']).to_string(),
                line.to_string(),
            )),
            _ => match out.last_mut() {
                Some((_, lines)) => lines.push_str(line),
                None => out.push((String::new(), line.to_string())),
            },
        }
    }
    out
}

fn yaml_scalar(text: &str) -> String {
    let plain = !text.is_empty()
        && text.trim() == text
        && !text.starts_with([
            '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|',
        ])
        && !text.starts_with(['>', '\'', '"', '%', '@', '`'])
        && !text.contains(": ")
        && !text.contains(" #")
        && !text.ends_with(':')
        && !text.contains(['\n', '\r', '\t'])
        && !matches!(
            text.to_ascii_lowercase().as_str(),
            "true" | "false" | "null" | "~" | "yes" | "no" | "on" | "off"
        )
        && text.parse::<f64>().is_err();
    if plain {
        return text.to_string();
    }
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn prop_text(value: &PropValue) -> Option<String> {
    match value {
        PropValue::Null => None,
        PropValue::Bool(b) => Some(b.to_string()),
        PropValue::Number(n) => Some(n.to_string()),
        PropValue::Text(t) if t.is_empty() => None,
        PropValue::Text(t) => Some(t.clone()),
        PropValue::List(items) => {
            let items: Vec<String> = items.iter().filter_map(prop_text).collect();
            (!items.is_empty()).then(|| items.join(", "))
        }
        PropValue::Map(_) => Some(String::new()),
    }
}

fn props_of(yaml: &str) -> Vec<(String, PropValue)> {
    focus_core::parse_meta(&format!("---\n{yaml}---\n")).props
}

pub fn merge_frontmatter<'a>(note: &str, addition: &'a str) -> (String, &'a str) {
    let Some((extra, body_start)) = split_frontmatter(addition) else {
        return (note.to_string(), addition);
    };
    let body = &addition[body_start..];
    let (yaml, note_body) = match split_frontmatter(note) {
        Some((yaml, start)) => (yaml, &note[start..]),
        None if extra.trim().is_empty() => return (note.to_string(), body),
        None => ("", note),
    };
    let mut merged = entries(yaml);
    let note_props = props_of(yaml);
    let extra_props = props_of(extra);
    let value = |props: &[(String, PropValue)], key: &str| {
        props
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(PropValue::Null)
    };
    let mut changed = false;
    for (key, lines) in entries(extra) {
        if key.is_empty() {
            continue;
        }
        let Some(at) = merged.iter().position(|(k, _)| *k == key) else {
            let mut lines = lines;
            if !lines.ends_with('\n') {
                lines.push('\n');
            }
            merged.push((key, lines));
            changed = true;
            continue;
        };
        let (mine, theirs) = (value(&note_props, &key), value(&extra_props, &key));
        let items = |value: &PropValue| -> Vec<String> {
            match value {
                PropValue::List(items) => items.iter().filter_map(prop_text).collect(),
                other => prop_text(other).into_iter().collect(),
            }
        };
        if matches!(mine, PropValue::List(_)) || matches!(theirs, PropValue::List(_)) {
            let mut list = items(&mine);
            let before = list.len();
            for item in items(&theirs) {
                if !list.contains(&item) {
                    list.push(item);
                }
            }
            if list.len() != before || !matches!(mine, PropValue::List(_)) {
                let mut text = format!("{}:\n", merged[at].1.split(':').next().unwrap_or(&key));
                for item in &list {
                    text.push_str(&format!("  - {}\n", yaml_scalar(item)));
                }
                merged[at].1 = text;
                changed = true;
            }
        } else if prop_text(&mine).is_none() && prop_text(&theirs).is_some() {
            let mut lines = lines;
            if !lines.ends_with('\n') {
                lines.push('\n');
            }
            merged[at].1 = lines;
            changed = true;
        }
    }
    if !changed {
        return (note.to_string(), body);
    }
    let mut out = String::from("---\n");
    for (_, lines) in &merged {
        out.push_str(lines);
        if !lines.ends_with('\n') {
            out.push('\n');
        }
    }
    out.push_str("---\n");
    out.push_str(note_body);
    (out, body)
}

pub fn insert_template(
    note: &str,
    at: usize,
    template: &str,
    title: &str,
    date: Date,
    time: Time,
) -> (String, usize) {
    let filled = fill(template, title, date, time);
    let (merged, body) = merge_frontmatter(note, &filled);
    let old_start = split_frontmatter(note).map_or(0, |(_, start)| start);
    let new_start = split_frontmatter(&merged).map_or(0, |(_, start)| start);
    let mut at = at.clamp(old_start, note.len());
    while !note.is_char_boundary(at) {
        at += 1;
    }
    let at = at - old_start + new_start;
    let mut out = String::with_capacity(merged.len() + body.len());
    out.push_str(&merged[..at]);
    out.push_str(body);
    out.push_str(&merged[at..]);
    (out, at + body.len())
}

#[derive(Clone, Copy, Debug)]
pub enum CaptureTo<'a> {
    Note(&'a str),
    Daily {
        folder: &'a str,
        format: &'a str,
        template: Option<NoteId>,
    },
}

pub fn capture_line(format: &str, text: &str, title: &str, date: Date, time: Time) -> String {
    let text = text
        .split(['\n', '\r'])
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    format
        .split("{{text}}")
        .map(|part| fill(part, title, date, time))
        .collect::<Vec<_>>()
        .join(&text)
        .replace(['\n', '\r'], " ")
}

pub fn append_line(content: &str, line: &str) -> String {
    let mut out = String::with_capacity(content.len() + line.len() + 2);
    out.push_str(content);
    if !content.is_empty() && !content.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(line);
    out.push('\n');
    out
}

pub fn capture(
    vault: &Vault,
    to: CaptureTo,
    format: &str,
    text: &str,
    date: Date,
    time: Time,
) -> Result<NoteId> {
    let id = match to {
        CaptureTo::Daily {
            folder,
            format,
            template,
        } => daily_note(vault, folder, format, date, time, template)?.0,
        CaptureTo::Note(path) => {
            let mut path = PathBuf::from(path.trim().trim_matches('/'));
            if path.extension().is_none() {
                path.set_extension("md");
            }
            match vault.note_by_path(&path) {
                Some(id) => id,
                None => match vault.create_note(&path, "") {
                    Err(VaultError::AlreadyExists(rel)) => vault
                        .note_by_path(&rel)
                        .ok_or(VaultError::AlreadyExists(rel))?,
                    other => other?,
                },
            }
        }
    };
    let title = vault
        .note(id)
        .map(|meta| meta.title.to_string())
        .unwrap_or_default();
    let content = vault.read(id)?;
    let line = capture_line(format, text, &title, date, time);
    vault.write(id, &append_line(&content, &line), None)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use focus_core::OpenOptions;

    const DAY: Date = Date {
        year: 2026,
        month: 11,
        day: 3,
    };
    const NOON: Time = Time { hour: 9, minute: 5 };

    #[test]
    fn dates_follow_the_moment_tokens() {
        assert_eq!(format_date("YYYY-MM-DD", DAY, NOON), "2026-11-03");
        assert_eq!(
            format_date("D/M/YY [at] HH:mm", DAY, NOON),
            "3/11/26 at 09:05"
        );
        assert_eq!(format_date("[Journal] YYYY", DAY, NOON), "Journal 2026");
    }

    #[test]
    fn templates_fill_their_variables() {
        assert_eq!(
            fill(
                "# {{title}}\n{{date}} {{time}} {{date:DD/MM}} {{other}}",
                "Kick-off",
                DAY,
                NOON
            ),
            "# Kick-off\n2026-11-03 09:05 03/11 {{other}}"
        );
    }

    #[test]
    fn daily_note_and_template_notes_on_a_copy() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/focus-view-vault-tests/notes");
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("vault");
        std::fs::create_dir_all(root.join("Templates")).unwrap();
        std::fs::write(
            root.join("Templates/Meeting.md"),
            "# {{title}}\nDate: {{date}}\n",
        )
        .unwrap();
        let vault = Vault::open(
            &root,
            OpenOptions {
                cache_dir: dir.join("cache"),
                ignore: Vec::new(),
                watch: false,
            },
        )
        .unwrap();
        let (day, created) = daily_note(&vault, "Daily", "YYYY-MM-DD", DAY, NOON, None).unwrap();
        assert!(created);
        assert_eq!(
            &*vault.note(day).unwrap().path,
            Path::new("Daily/2026-11-03.md")
        );
        assert_eq!(
            daily_note(&vault, "Daily", "YYYY-MM-DD", DAY, NOON, None).unwrap(),
            (day, false)
        );
        let list = templates(&vault, "Templates");
        assert_eq!(list.len(), 1);
        let a = note_from_template(&vault, list[0].0, "Meetings", "Kick-off", DAY, NOON).unwrap();
        let b = note_from_template(&vault, list[0].0, "Meetings", "Kick-off", DAY, NOON).unwrap();
        assert_eq!(vault.read(a).unwrap(), "# Kick-off\nDate: 2026-11-03\n");
        assert_eq!(
            &*vault.note(b).unwrap().path,
            Path::new("Meetings/Kick-off 1.md")
        );
        let host = note_from_text(
            &vault,
            "# {{title}}\nOSCP, {{date}}\n",
            "Exams",
            "Lab 1",
            DAY,
            NOON,
        )
        .unwrap();
        assert_eq!(vault.read(host).unwrap(), "# Lab 1\nOSCP, 2026-11-03\n");
    }
    #[test]
    fn moment_names_and_twelve_hour_tokens() {
        let evening = Time {
            hour: 21,
            minute: 7,
        };
        assert_eq!(
            format_date("dddd, MMMM Do YYYY h:mm A", DAY, evening),
            "Tuesday, November 3rd 2026 9:07 PM"
        );
        assert_eq!(
            format_date("ddd D MMM, hh:m a", DAY, NOON),
            "Tue 3 Nov, 09:5 am"
        );
        let first = Date {
            year: 2024,
            month: 2,
            day: 29,
        };
        assert_eq!(format_date("dddd Do", first, NOON), "Thursday 29th");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(22), "22nd");
    }

    #[test]
    fn an_inserted_template_fills_its_variables_at_the_caret() {
        let note = "# Plan\n\nBefore.\nAfter.\n";
        let at = note.find("After").unwrap();
        let (text, caret) = insert_template(
            note,
            at,
            "Met on {{date:DD/MM}} at {{time}} for {{title}}.\n",
            "Plan",
            DAY,
            NOON,
        );
        assert_eq!(
            text,
            "# Plan\n\nBefore.\nMet on 03/11 at 09:05 for Plan.\nAfter.\n"
        );
        assert_eq!(&text[caret..], "After.\n");
    }

    #[test]
    fn an_inserted_template_joins_its_properties_to_the_note() {
        let note = "---\ntags:\n  - work\nstatus: draft\ntopic:\n---\n# Plan\n";
        let template = "---\ntags:\n  - meeting\n  - work\nstatus: ready\ntopic: \"{{title}}\"\ndate: \"{{date}}\"\n---\nAgenda for {{title}}\n";
        let (text, caret) = insert_template(note, note.len(), template, "Plan", DAY, NOON);
        assert_eq!(
            text,
            "---\ntags:\n  - work\n  - meeting\nstatus: draft\ntopic: \"Plan\"\ndate: \"2026-11-03\"\n---\n# Plan\nAgenda for Plan\n"
        );
        assert_eq!(caret, text.len());
        let (text, _) = insert_template("---\na: 1\n---\nBody\n", 5, "X\n", "T", DAY, NOON);
        assert_eq!(text, "---\na: 1\n---\nX\nBody\n");
    }

    #[test]
    fn a_note_without_properties_takes_the_template_ones() {
        let (text, caret) = insert_template(
            "Body\n",
            5,
            "---\ntags: [a, \"b: c\"]\n---\nMore\n",
            "T",
            DAY,
            NOON,
        );
        assert_eq!(text, "---\ntags: [a, \"b: c\"]\n---\nBody\nMore\n");
        assert_eq!(&text[caret..], "");
        let (text, _) = insert_template(
            "---\ntags: x\n---\n",
            14,
            "---\ntags: [y]\n---\n",
            "T",
            DAY,
            NOON,
        );
        assert_eq!(text, "---\ntags:\n  - x\n  - y\n---\n");
        assert_eq!(yaml_scalar("b: c"), "\"b: c\"");
        assert_eq!(yaml_scalar("true"), "\"true\"");
        assert_eq!(yaml_scalar("plain words"), "plain words");
    }

    #[test]
    fn the_capture_line_follows_its_format() {
        assert_eq!(
            capture_line(
                "- {{time}} {{text}}",
                "Call {{date}}\nback",
                "Inbox",
                DAY,
                NOON
            ),
            "- 09:05 Call {{date}} back"
        );
        assert_eq!(
            capture_line(
                "- [ ] {{text}} ([[{{title}}]], {{date:D MMM}})",
                "Buy",
                "Inbox",
                DAY,
                NOON
            ),
            "- [ ] Buy ([[Inbox]], 3 Nov)"
        );
        assert_eq!(append_line("a", "b"), "a\nb\n");
        assert_eq!(append_line("", "b"), "b\n");
        assert_eq!(append_line("a\n", "b"), "a\nb\n");
    }

    #[test]
    fn a_capture_appends_to_a_note_or_to_a_daily_note_it_creates() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/focus-view-vault-tests/capture");
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("vault");
        std::fs::create_dir_all(root.join("Templates")).unwrap();
        std::fs::write(root.join("Inbox.md"), "# Inbox\n- earlier").unwrap();
        std::fs::write(root.join("Templates/Day.md"), "# {{title}}\n\n## Log\n").unwrap();
        let vault = Vault::open(
            &root,
            OpenOptions {
                cache_dir: dir.join("cache"),
                ignore: Vec::new(),
                watch: false,
            },
        )
        .unwrap();
        let format = "- {{time}} {{text}}";
        let inbox = capture(&vault, CaptureTo::Note("Inbox"), format, "First", DAY, NOON).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("Inbox.md")).unwrap(),
            "# Inbox\n- earlier\n- 09:05 First\n"
        );
        assert_eq!(
            capture(
                &vault,
                CaptureTo::Note("Inbox.md"),
                format,
                "Second",
                DAY,
                NOON
            )
            .unwrap(),
            inbox
        );
        assert!(
            std::fs::read_to_string(root.join("Inbox.md"))
                .unwrap()
                .ends_with("- 09:05 First\n- 09:05 Second\n")
        );
        let template = vault.note_by_path(Path::new("Templates/Day.md"));
        let to = CaptureTo::Daily {
            folder: "Daily",
            format: "YYYY-MM-DD",
            template,
        };
        capture(&vault, to, format, "Standup", DAY, NOON).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("Daily/2026-11-03.md")).unwrap(),
            "# 2026-11-03\n\n## Log\n- 09:05 Standup\n"
        );
        capture(
            &vault,
            CaptureTo::Note("New/Ideas"),
            format,
            "One",
            DAY,
            NOON,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("New/Ideas.md")).unwrap(),
            "- 09:05 One\n"
        );
        assert!(capture(&vault, CaptureTo::Note("../out"), format, "x", DAY, NOON).is_err());
        assert!(!dir.join("out.md").exists());
    }
}
