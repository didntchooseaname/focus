#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyOrigin {
    Syncthing,
    ObsidianSync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictName {
    pub original: String,
    pub origin: CopyOrigin,
    pub device: Option<String>,
}

fn digits(text: &str, count: usize) -> bool {
    text.len() == count && text.bytes().all(|b| b.is_ascii_digit())
}

pub fn conflict_copy(file_name: &str) -> Option<ConflictName> {
    syncthing(file_name).or_else(|| obsidian_sync(file_name))
}

fn syncthing(name: &str) -> Option<ConflictName> {
    const MARK: &str = ".sync-conflict-";
    let at = name.rfind(MARK)?;
    let stem = &name[..at];
    let rest = &name[at + MARK.len()..];
    let (date, rest) = rest.split_at_checked(8)?;
    let rest = rest.strip_prefix('-')?;
    let (time, rest) = rest.split_at_checked(6)?;
    if stem.is_empty() || !digits(date, 8) || !digits(time, 6) {
        return None;
    }
    let (device, extension) = match rest.strip_prefix('-') {
        Some(tail) => {
            let end = tail.find('.').unwrap_or(tail.len());
            let device = &tail[..end];
            if device.is_empty() || !device.bytes().all(|b| b.is_ascii_alphanumeric()) {
                return None;
            }
            (Some(device.to_string()), &tail[end..])
        }
        None => (None, rest),
    };
    if !(extension.is_empty() || extension.starts_with('.') && !extension[1..].contains('.')) {
        return None;
    }
    Some(ConflictName {
        original: format!("{stem}{extension}"),
        origin: CopyOrigin::Syncthing,
        device,
    })
}

fn obsidian_sync(name: &str) -> Option<ConflictName> {
    const MARK: &str = " (Conflicted copy ";
    let at = name.rfind(MARK)?;
    let stem = &name[..at];
    let rest = &name[at + MARK.len()..];
    let close = rest.rfind(')')?;
    let (inner, extension) = (&rest[..close], &rest[close + 1..]);
    let (device, stamp) = inner.rsplit_once(' ')?;
    if stem.is_empty()
        || device.trim().is_empty()
        || !digits(stamp, 12)
        || !(extension.is_empty() || extension.starts_with('.'))
    {
        return None;
    }
    Some(ConflictName {
        original: format!("{stem}{extension}"),
        origin: CopyOrigin::ObsidianSync,
        device: Some(device.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(name: &str) -> (String, CopyOrigin, Option<String>) {
        let copy = conflict_copy(name).unwrap_or_else(|| panic!("{name} not recognized"));
        (copy.original, copy.origin, copy.device)
    }

    #[test]
    fn obsidian_sync_names_of_its_documentation() {
        assert_eq!(
            found("Meeting notes (Conflicted copy MyMacBook2 202411281430).md"),
            (
                "Meeting notes.md".into(),
                CopyOrigin::ObsidianSync,
                Some("MyMacBook2".into())
            )
        );
        assert_eq!(
            found("Plan (draft) (Conflicted copy Pixel 8 Pro 202501020304).md"),
            (
                "Plan (draft).md".into(),
                CopyOrigin::ObsidianSync,
                Some("Pixel 8 Pro".into())
            )
        );
    }

    #[test]
    fn syncthing_names_of_its_documentation_and_source() {
        assert_eq!(
            found("Meeting notes.sync-conflict-20241128-143005-MFZWI3D.md"),
            (
                "Meeting notes.md".into(),
                CopyOrigin::Syncthing,
                Some("MFZWI3D".into())
            )
        );
        assert_eq!(
            found("notes.sync-conflict-20150322-101010.md"),
            ("notes.md".into(), CopyOrigin::Syncthing, None)
        );
        assert_eq!(
            found("v1.2 release.sync-conflict-20260829-161614-TEQOE4Z.md").0,
            "v1.2 release.md"
        );
        assert_eq!(
            found("README.sync-conflict-20260829-161614-TEQOE4Z").0,
            "README"
        );
    }

    #[test]
    fn ordinary_names_are_not_copies() {
        for name in [
            "Meeting notes.md",
            "Conflicted copy.md",
            "Note (Conflicted copy).md",
            "Note (Conflicted copy MyMacBook2).md",
            "Note (Conflicted copy MyMacBook2 2024112814).md",
            "Note (Conflicted copy  202411281430).md",
            " (Conflicted copy Mac 202411281430).md",
            "Note.sync-conflict.md",
            "Note.sync-conflict-2024-143005-ABC.md",
            "Note.sync-conflict-20241128-1430-ABC.md",
            "Note.sync-conflict-2024112a-143005-ABC.md",
            "Note.sync-conflict-20241128-143005-.md",
            ".sync-conflict-20241128-143005-ABC.md",
            "Note (unsaved copy).md",
        ] {
            assert_eq!(conflict_copy(name), None, "{name}");
        }
    }
}
