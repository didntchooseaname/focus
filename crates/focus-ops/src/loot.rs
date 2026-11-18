use std::path::Path;

use crate::fsutil;
use crate::hosts::split_row;
use crate::{Credential, Result};

const FILE: &str = "Credentials.md";
const HEADER: &str = "| User | Type | Secret | Domain | Source | Valid on | Notes |";
const RULE: &str = "|---|---|---|---|---|---|---|";

pub(crate) fn empty_file() -> String {
    format!("# Credentials\n\n{HEADER}\n{RULE}\n")
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|")
        .replace(['\n', '\r'], " ")
        .trim()
        .to_string()
}

fn row(c: &Credential) -> String {
    format!(
        "| {} | {} | {} | {} | {} | {} | {} |",
        cell(&c.user),
        cell(&c.kind),
        cell(&c.secret),
        cell(&c.domain),
        cell(&c.source),
        cell(&c.valid_on),
        cell(&c.notes)
    )
}

fn parse(text: &str) -> Vec<Credential> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with('|'))
        .filter_map(|l| {
            let cells = split_row(l);
            let is_rule = cells
                .iter()
                .all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' ')));
            if is_rule
                || (cells
                    .first()
                    .is_some_and(|c| c.eq_ignore_ascii_case("user"))
                    && cells.get(1).is_some_and(|c| c.eq_ignore_ascii_case("type")))
                || cells.iter().all(String::is_empty)
            {
                return None;
            }
            let get = |i: usize| cells.get(i).cloned().unwrap_or_default();
            Some(Credential {
                user: get(0),
                kind: get(1),
                secret: get(2),
                domain: get(3),
                source: get(4),
                valid_on: get(5),
                notes: get(6),
            })
        })
        .collect()
}

pub fn list(engagement_dir: &Path) -> Vec<Credential> {
    std::fs::read_to_string(engagement_dir.join("Loot").join(FILE))
        .map(|t| parse(&t))
        .unwrap_or_default()
}

pub fn add(engagement_dir: &Path, credential: &Credential) -> Result<bool> {
    let path = engagement_dir.join("Loot").join(FILE);
    let text = fsutil::read_optional(&path)?.unwrap_or_else(empty_file);
    let same = |c: &Credential| {
        c.user == credential.user.trim()
            && c.kind == credential.kind.trim()
            && c.secret == credential.secret.trim()
            && c.domain == credential.domain.trim()
    };
    if parse(&text).iter().any(same) {
        return Ok(false);
    }
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let new_row = row(credential);
    match lines.iter().rposition(|l| l.trim_start().starts_with('|')) {
        Some(last) => lines.insert(last + 1, new_row),
        None => {
            lines.extend([String::new(), HEADER.to_string(), RULE.to_string(), new_row]);
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    fsutil::write_private(&path, &out)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential(user: &str, secret: &str) -> Credential {
        Credential {
            user: user.into(),
            kind: "password".into(),
            secret: secret.into(),
            domain: "example.test".into(),
            source: "config file".into(),
            valid_on: "192.0.2.5".into(),
            notes: String::new(),
        }
    }

    #[test]
    fn add_list_and_duplicates() {
        let dir = fsutil::test_dir("loot");
        assert!(list(&dir).is_empty());
        assert!(add(&dir, &credential("alice", "p|a\\ss")).unwrap());
        assert!(add(&dir, &credential("bob", "autre")).unwrap());
        assert!(!add(&dir, &credential("alice", "p|a\\ss")).unwrap());
        let mut other_domain = credential("alice", "p|a\\ss");
        other_domain.domain = "other.test".into();
        assert!(add(&dir, &other_domain).unwrap());
        let all = list(&dir);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].secret, "p|a\\ss");
        assert!(add(&dir, &credential("user", "x")).unwrap());
        assert_eq!(
            list(&dir).len(),
            4,
            "a user named `user` is not mistaken for the header"
        );
        assert_eq!(all[0].source, "config file");
    }

    #[test]
    fn rows_are_inserted_in_the_table_not_after_trailing_text() {
        let dir = fsutil::test_dir("loot-text");
        std::fs::create_dir_all(dir.join("Loot")).unwrap();
        std::fs::write(dir.join("Loot/Credentials.md"), format!("# Credentials\n\n{HEADER}\n{RULE}\n| a | password | x | | | | |\n\nNote written below the table.\n")).unwrap();
        add(&dir, &credential("b", "y")).unwrap();
        let text = std::fs::read_to_string(dir.join("Loot/Credentials.md")).unwrap();
        let note = text.find("Note written").unwrap();
        assert!(text.find("| b |").unwrap() < note);
        assert_eq!(list(&dir).len(), 2);
    }
}
