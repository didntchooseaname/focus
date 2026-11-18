use std::path::{Path, PathBuf};

use focus_core::{AttachmentPolicy, MoveReport, NoteId, TidyPlan, Vault};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Rename,
    Move,
    Merge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub from: PathBuf,
    pub to: PathBuf,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone)]
pub struct TidyPreview {
    pub changes: Vec<Change>,
    pub link_updates: usize,
    plan: TidyPlan,
}

fn kind_of(from: &Path, to: &Path) -> ChangeKind {
    if from.parent() == to.parent() {
        ChangeKind::Rename
    } else {
        ChangeKind::Move
    }
}

impl TidyPreview {
    pub fn from_plan(plan: TidyPlan) -> TidyPreview {
        let moves = plan.moves.iter().map(|(from, to)| Change {
            from: from.clone(),
            to: to.clone(),
            kind: kind_of(from, to),
        });
        let merges = plan.duplicates.iter().map(|(dup, kept)| Change {
            from: dup.clone(),
            to: kept.clone(),
            kind: ChangeKind::Merge,
        });
        TidyPreview {
            changes: moves.chain(merges).collect(),
            link_updates: plan.link_updates,
            plan,
        }
    }

    pub fn count(&self, kind: ChangeKind) -> usize {
        self.changes.iter().filter(|c| c.kind == kind).count()
    }
}

pub fn plan_tidy(vault: &Vault, scope: Option<NoteId>, policy: &AttachmentPolicy) -> TidyPreview {
    TidyPreview::from_plan(vault.plan_tidy_attachments(scope, policy))
}

pub fn apply_tidy(vault: &Vault, preview: &TidyPreview) -> focus_core::Result<MoveReport> {
    vault.apply_tidy(&preview.plan)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    pub path: PathBuf,
    pub bytes: u64,
    pub is_image: bool,
}

pub fn is_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg")
    )
}

pub fn orphans(vault: &Vault) -> Vec<Orphan> {
    let mut list: Vec<Orphan> = vault
        .orphan_attachments()
        .into_iter()
        .map(|path| {
            let bytes = std::fs::metadata(vault.absolute(&path))
                .map(|m| m.len())
                .unwrap_or(0);
            let is_image = is_image(&path);
            Orphan {
                path,
                bytes,
                is_image,
            }
        })
        .collect();
    list.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    list
}

#[derive(Debug, Clone, Default)]
pub struct TrashReport {
    pub trashed: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
}

pub fn trash(vault: &Vault, paths: &[PathBuf]) -> TrashReport {
    let mut report = TrashReport::default();
    for path in paths {
        match vault.trash(path) {
            Ok(()) => report.trashed.push(path.clone()),
            Err(error) => report.failed.push((path.clone(), error.to_string())),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_changes_are_sorted_into_renames_moves_and_merges() {
        let plan = TidyPlan {
            moves: vec![
                ("a/x.png".into(), "a/note-01.png".into()),
                ("img/y.png".into(), "a/y.png".into()),
            ],
            duplicates: vec![("img/z.png".into(), "a/z.png".into())],
            link_updates: 3,
            stamps: Vec::new(),
        };
        let preview = TidyPreview::from_plan(plan);
        assert_eq!(
            (
                preview.count(ChangeKind::Rename),
                preview.count(ChangeKind::Move),
                preview.count(ChangeKind::Merge)
            ),
            (1, 1, 1)
        );
        assert_eq!(preview.link_updates, 3);
        assert_eq!(
            preview.changes[2],
            Change {
                from: "img/z.png".into(),
                to: "a/z.png".into(),
                kind: ChangeKind::Merge
            }
        );
    }

    #[test]
    fn orphans_are_listed_and_trashed_without_touching_used_files() {
        let (vault, root) = crate::export::scratch_vault(
            "attach-orphans",
            &[
                ("N.md", b"# N\n![](used.png)\n"),
                ("used.png", &[1u8; 8]),
                ("lost.png", &[2u8; 64]),
                ("lost.pdf", &[3u8; 16]),
            ],
        );
        let list = orphans(&vault);
        assert_eq!(
            list.iter()
                .map(|o| o.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["lost.png", "lost.pdf"]
        );
        assert!(list[0].is_image && !list[1].is_image);
        let report = trash(
            &vault,
            &[PathBuf::from("lost.png"), PathBuf::from("missing.png")],
        );
        assert_eq!(report.trashed, [PathBuf::from("lost.png")]);
        assert_eq!(report.failed.len(), 1);
        assert!(!root.join("lost.png").exists() && root.join("used.png").is_file());
    }
}
