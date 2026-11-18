use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::fsutil;
use crate::{JournalEntry, OpsError, Result};

const FILE: &str = "Journal.md";

fn format_line(entry: &JournalEntry) -> String {
    let text = entry.text.replace(['\r', '\n'], " ");
    let text = text.trim();
    let anchor = entry
        .id
        .as_ref()
        .map(|id| format!(" ^{id}"))
        .unwrap_or_default();
    match &entry.target {
        Some(target) => format!(
            "- {} [{}] {}{anchor}",
            entry.timestamp,
            target.replace([']', '\n'], ""),
            text
        ),
        None if text.starts_with('[') => format!("- {} \\{}{anchor}", entry.timestamp, text),
        None => format!("- {} {}{anchor}", entry.timestamp, text),
    }
}

fn base_id(timestamp: &str) -> String {
    let digits: String = timestamp.chars().filter(char::is_ascii_digit).collect();
    format!(
        "j-{}-{}",
        &digits[..digits.len().min(8)],
        &digits[digits.len().min(8)..]
    )
}

fn split_id(line: &str) -> (&str, Option<String>) {
    let trimmed = line.trim_end();
    if let Some((head, tail)) = trimmed.rsplit_once(" ^")
        && tail.starts_with("j-")
        && tail.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return (head, Some(tail.to_string()));
    }
    (trimmed, None)
}

fn parse_line(line: &str) -> Option<JournalEntry> {
    let (line, id) = split_id(line);
    let rest = line.strip_prefix("- ")?;
    let (date, rest) = rest.split_at_checked(10)?;
    let (time, rest) = rest.strip_prefix(' ')?.split_at_checked(8)?;
    let is_digits = |s: &str, pattern: &[u8]| {
        s.len() == pattern.len()
            && s.bytes().zip(pattern).all(|(b, p)| {
                if *p == b'9' {
                    b.is_ascii_digit()
                } else {
                    b == *p
                }
            })
    };
    if !is_digits(date, b"9999-99-99") || !is_digits(time, b"99:99:99") {
        return None;
    }
    let rest = rest.strip_prefix(' ').unwrap_or(rest);
    let timestamp = format!("{date} {time}");
    if let Some((target, text)) = rest
        .strip_prefix('[')
        .and_then(|after| after.split_once(']'))
    {
        return Some(JournalEntry {
            timestamp,
            target: Some(target.to_string()),
            text: text.trim_start().to_string(),
            id,
        });
    }
    let text = rest
        .strip_prefix("\\[")
        .map_or_else(|| rest.to_string(), |r| format!("[{r}"));
    Some(JournalEntry {
        timestamp,
        target: None,
        text,
        id,
    })
}

pub fn read(engagement_dir: &Path) -> Vec<JournalEntry> {
    let Ok(text) = std::fs::read_to_string(engagement_dir.join(FILE)) else {
        return Vec::new();
    };
    text.lines().filter_map(parse_line).collect()
}

pub fn append(engagement_dir: &Path, target: Option<&str>, text: &str) -> Result<JournalEntry> {
    if text.trim().is_empty() {
        return Err(OpsError::Invalid("the journal entry is empty".into()));
    }
    let mut entry = JournalEntry {
        timestamp: fsutil::timestamp_now(),
        target: target
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string),
        text: text.trim().to_string(),
        id: None,
    };
    let path = engagement_dir.join(FILE);
    fsutil::create_dir(engagement_dir)?;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(&path)
        .map_err(|e| fsutil::io_error(&path, e))?;
    let io = |e| fsutil::io_error(&path, e);
    let length = file.metadata().map_err(io)?.len();
    let mut line = String::new();
    if length == 0 {
        line.push_str("# Journal\n\n");
    } else {
        let mut last = [0u8; 1];
        file.seek(SeekFrom::Start(length - 1)).map_err(io)?;
        file.read_exact(&mut last).map_err(io)?;
        if last[0] != b'\n' {
            line.push('\n');
        }
    }
    entry.id = Some(free_id(&path, &entry.timestamp));
    line.push_str(&format_line(&entry));
    line.push('\n');
    file.write_all(line.as_bytes()).map_err(io)?;
    Ok(JournalEntry {
        text: entry.text.replace(['\r', '\n'], " "),
        ..entry
    })
}

fn free_id(path: &Path, timestamp: &str) -> String {
    let existing: Vec<String> = std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| split_id(l).1)
        .collect();
    let base = base_id(timestamp);
    let mut candidate = base.clone();
    let mut n = 2;
    while existing.contains(&candidate) {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    candidate
}

fn csv_field(field: &str) -> String {
    let guarded;
    let field = if field.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        guarded = format!("'{field}");
        guarded.as_str()
    } else {
        field
    };
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

pub fn to_csv(entries: &[JournalEntry]) -> String {
    let mut out = String::from("timestamp,target,text\n");
    for e in entries {
        out.push_str(&format!(
            "{},{},{}\n",
            csv_field(&e.timestamp),
            csv_field(e.target.as_deref().unwrap_or("")),
            csv_field(&e.text)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_csv_cell_never_starts_like_a_formula() {
        let entries = vec![
            JournalEntry {
                timestamp: "2026-11-12 10:00:00".into(),
                target: None,
                text: "=HYPERLINK(\"http://x\",\"y\")".into(),
                id: None,
            },
            JournalEntry {
                timestamp: "2026-11-12 10:00:01".into(),
                target: Some("-5".into()),
                text: "plain, with comma".into(),
                id: None,
            },
        ];
        let csv = to_csv(&entries);
        assert!(
            csv.contains("'=HYPERLINK")
                && csv.contains(",'-5,")
                && csv.contains("\"plain, with comma\""),
            "{csv}"
        );
        assert!(
            !csv.lines()
                .skip(1)
                .any(|l| l.split(',').any(|c| c.starts_with(['=', '+', '@']))),
            "{csv}"
        );
    }

    #[test]
    fn append_then_read_roundtrip_including_awkward_text() {
        let dir = fsutil::test_dir("journal");
        let first = append(&dir, Some("192.0.2.5"), "first reading").unwrap();
        append(&dir, None, "no target").unwrap();
        append(&dir, None, "[bracket] at the start").unwrap();
        append(
            &dir,
            Some("srv-web"),
            "line 1\nline 2 with [brackets] and | bar",
        )
        .unwrap();
        assert_eq!(first.timestamp.len(), 19);
        let entries = read(&dir);
        assert_eq!(entries.len(), 4);
        assert_eq!(
            (entries[0].target.as_deref(), entries[0].text.as_str()),
            (Some("192.0.2.5"), "first reading")
        );
        assert_eq!(
            (entries[1].target.clone(), entries[1].text.as_str()),
            (None, "no target")
        );
        assert_eq!(
            (entries[2].target.clone(), entries[2].text.as_str()),
            (None, "[bracket] at the start")
        );
        assert_eq!(
            (entries[3].target.as_deref(), entries[3].text.as_str()),
            (Some("srv-web"), "line 1 line 2 with [brackets] and | bar")
        );
        assert!(
            std::fs::read_to_string(dir.join("Journal.md"))
                .unwrap()
                .starts_with("# Journal\n\n- ")
        );
    }

    #[test]
    fn appending_to_a_file_without_trailing_newline_and_hand_edits() {
        let dir = fsutil::test_dir("journal-edit");
        std::fs::write(
            dir.join("Journal.md"),
            "# Journal\n\nFree remark\n- 2026-11-12 14:03:27 [10.0.0.1] old entry",
        )
        .unwrap();
        append(&dir, None, "next").unwrap();
        let entries = read(&dir);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].timestamp, "2026-11-12 14:03:27");
        assert_eq!(entries[1].text, "next");
    }

    #[test]
    fn entries_get_a_block_identifier_that_is_read_back_and_unique() {
        let dir = fsutil::test_dir("journal-ids");
        let a = append(&dir, None, "one").unwrap();
        let b = append(&dir, None, "two").unwrap();
        let (ida, idb) = (a.id.clone().unwrap(), b.id.clone().unwrap());
        assert!(ida.starts_with("j-") && ida.len() == 17, "{ida}");
        assert_ne!(ida, idb, "two entries in the same second must differ");
        let text = std::fs::read_to_string(dir.join("Journal.md")).unwrap();
        assert!(text.contains(&format!("one ^{ida}")));
        let entries = read(&dir);
        assert_eq!(
            (entries[0].id.as_deref(), entries[0].text.as_str()),
            (Some(ida.as_str()), "one")
        );
        assert_eq!(entries[1].id.as_deref(), Some(idb.as_str()));
    }

    #[test]
    fn a_hand_written_line_has_no_identifier() {
        assert_eq!(
            parse_line("- 2026-11-12 14:03:27 free text").unwrap().id,
            None
        );
        assert_eq!(
            parse_line("- 2026-11-12 14:03:27 [h] x ^j-20261112-140327-2")
                .unwrap()
                .id
                .as_deref(),
            Some("j-20261112-140327-2")
        );
        assert_eq!(
            parse_line("- 2026-11-12 14:03:27 xor ^3").unwrap().text,
            "xor ^3"
        );
    }

    #[test]
    fn empty_text_is_refused_and_csv_is_quoted() {
        let dir = fsutil::test_dir("journal-csv");
        assert!(matches!(
            append(&dir, None, "  \n"),
            Err(OpsError::Invalid(_))
        ));
        let entries = vec![
            JournalEntry {
                timestamp: "2026-11-12 14:03:27".into(),
                target: Some("192.0.2.5".into()),
                text: "a, b".into(),
                id: None,
            },
            JournalEntry {
                timestamp: "2026-11-12 14:04:00".into(),
                target: None,
                text: "says \"yes\"".into(),
                id: None,
            },
        ];
        assert_eq!(
            to_csv(&entries),
            "timestamp,target,text\n2026-11-12 14:03:27,192.0.2.5,\"a, b\"\n2026-11-12 14:04:00,,\"says \"\"yes\"\"\"\n"
        );
        assert_eq!(to_csv(&[]), "timestamp,target,text\n");
    }
}
