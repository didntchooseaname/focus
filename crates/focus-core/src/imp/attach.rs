use super::config::fnv1a;
use super::edit::{Relocation, clean_rel};
use super::fsx::{self, encode_link_path, file_name, parent_dir, relative_path, slash, slug, stem};
use super::walk::is_media;
use super::*;

pub(crate) fn attachment_dir(folder: &str, note: &Path) -> PathBuf {
    let f = folder.trim();
    if f.is_empty() || f == "/" {
        PathBuf::new()
    } else if f == "." || f == "./" {
        parent_dir(note).to_path_buf()
    } else if let Some(rest) = f.strip_prefix("./") {
        fsx::normalize(&parent_dir(note).join(rest.trim_end_matches('/'))).unwrap_or_default()
    } else {
        PathBuf::from(f.trim_matches('/'))
    }
}

pub(crate) fn note_attachment_dir(folder: &str, note: &Path, per_note: bool) -> PathBuf {
    let dir = attachment_dir(folder, note);
    if per_note { dir.join(stem(note)) } else { dir }
}

fn readable_base(note: &Path, heading: Option<&str>, prefix: Option<&str>) -> String {
    let parts = [
        prefix.map(slug).unwrap_or_default(),
        slug(&stem(note)),
        heading.map(slug).unwrap_or_default(),
    ];
    let base = parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if base.is_empty() {
        "attachment".into()
    } else {
        base
    }
}

fn timestamp(t: SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as libc::time_t;
    // SAFETY: `tm` is a plain C struct, valid when zeroed; `localtime_r` only writes
    // into it and keeps no reference.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    #[cfg(unix)]
    let ok = unsafe { !libc::localtime_r(&secs, &mut tm).is_null() };
    #[cfg(windows)]
    let ok = unsafe { libc::localtime_s(&mut tm, &secs) == 0 };
    if ok {
        return format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        );
    }
    utc_timestamp(secs as i64)
}

fn utc_timestamp(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

impl Inner {
    fn taken(&self, s: &State, rel: &Path) -> bool {
        s.files.contains_key(rel)
            || s.by_path.contains_key(rel)
            || std::fs::symlink_metadata(self.root.join(rel)).is_ok()
    }

    pub(crate) fn attach(&self, request: &AttachmentRequest) -> Result<SavedAttachment> {
        let _ops = self.ops.lock();
        let policy = request.policy;
        let cfg = self.obsidian.read().clone();
        let note_path = self.note_path(request.note)?;
        let dir = match request.directory {
            Some(d) if d.as_os_str().is_empty() => PathBuf::new(),
            Some(d) => clean_rel(&self.root, d)?,
            None => note_attachment_dir(&cfg.attachment_folder, &note_path, policy.per_note),
        };
        let ext = request
            .extension
            .trim_start_matches('.')
            .to_ascii_lowercase();
        let ext = if ext.is_empty() {
            "bin".to_string()
        } else {
            ext
        };

        let reused = if policy.deduplicate {
            let candidates: Vec<Arc<Path>> = {
                let s = self.state.read();
                s.files
                    .iter()
                    .filter(|(p, f)| parent_dir(p) == dir && f.size == request.bytes.len() as u64)
                    .map(|(p, _)| p.clone())
                    .collect()
            };
            candidates
                .into_iter()
                .find(|p| std::fs::read(self.root.join(p)).is_ok_and(|b| b == request.bytes))
                .map(|p| p.to_path_buf())
        } else {
            None
        };

        let path = match &reused {
            Some(p) => p.clone(),
            None => {
                let s = self.state.read();
                let path = if policy.readable_names {
                    let base = readable_base(&note_path, request.heading, request.name_prefix);
                    (1..)
                        .map(|n| dir.join(format!("{base}-{n:02}.{ext}")))
                        .find(|p| !self.taken(&s, p))
                        .expect("infinite name sequence")
                } else {
                    let original = request
                        .original_name
                        .map(|n| fsx::file_name(Path::new(n)))
                        .filter(|n| !n.is_empty());
                    let (base, ext) = match &original {
                        Some(n) => {
                            let p = Path::new(n);
                            (
                                stem(p),
                                p.extension()
                                    .map(|e| e.to_string_lossy().into_owned())
                                    .unwrap_or(ext.clone()),
                            )
                        }
                        None => (
                            format!("Pasted image {}", timestamp(SystemTime::now())),
                            ext.clone(),
                        ),
                    };
                    std::iter::once(dir.join(format!("{base}.{ext}")))
                        .chain((1..).map(|n| dir.join(format!("{base} {n}.{ext}"))))
                        .find(|p| !self.taken(&s, p))
                        .expect("infinite name sequence")
                };
                drop(s);
                let abs = self.root.join(&path);
                fsx::write_atomic(&abs, request.bytes).map_err(|e| io_err(&abs, e))?;
                let (size, modified) = {
                    let m = std::fs::metadata(&abs).map_err(|e| io_err(&abs, e))?;
                    (m.len(), m.modified().map_err(|e| io_err(&abs, e))?)
                };
                let mut s = self.state.write();
                s.files.insert(
                    super::fsx::stored(&path),
                    FileEntry {
                        size,
                        modified,
                        kind: walk::kind_of(&path),
                    },
                );
                s.add_folder_chain(&dir);
                s.rebuild();
                path
            }
        };
        if reused.is_none() {
            self.emit(vec![VaultEvent::TreeChanged]);
        }
        let link = {
            let s = self.state.read();
            link_text(&cfg, &s, &note_path, &path, request.heading)
        };
        Ok(SavedAttachment {
            path,
            link,
            reused: reused.is_some(),
        })
    }

    pub(crate) fn attachments_of(&self, note: NoteId) -> Vec<PathBuf> {
        let s = self.state.read();
        let Some(e) = s.entry(note) else {
            return Vec::new();
        };
        let mut out: Vec<PathBuf> = Vec::new();
        for r in &e.resolved {
            if let Resolved::File(p) = r
                && !out.iter().any(|o| o == p.as_ref())
            {
                out.push(p.to_path_buf());
            }
        }
        out
    }

    pub(crate) fn orphan_attachments(&self) -> Vec<PathBuf> {
        let s = self.state.read();
        let mut out: Vec<PathBuf> = s
            .files
            .keys()
            .filter(|p| is_media(p) && !s.file_incoming.contains_key(*p))
            .map(|p| p.to_path_buf())
            .collect();
        out.sort();
        out
    }

    pub(crate) fn plan_tidy_attachments(
        &self,
        scope: Option<NoteId>,
        policy: &AttachmentPolicy,
    ) -> TidyPlan {
        let s = self.state.read();
        let folder = self.obsidian.read().attachment_folder.clone();
        let mut notes: Vec<&NoteEntry> = match scope {
            Some(id) => s.entry(id).into_iter().collect(),
            None => s.notes.iter().flatten().collect(),
        };
        notes.sort_by(|a, b| a.path.cmp(&b.path));
        let mut plan = TidyPlan::default();
        let mut claimed: HashSet<Arc<Path>> = HashSet::new();
        let mut reserved: HashSet<PathBuf> = HashSet::new();
        for e in notes {
            let dir = note_attachment_dir(&folder, &e.path, policy.per_note);
            let parsed = e.parsed().meta;
            let mut own: Vec<(Arc<Path>, Option<&str>)> = Vec::new();
            for (link, r) in parsed.links.iter().zip(e.resolved.iter()) {
                let Resolved::File(p) = r else { continue };
                if !is_media(p)
                    || s.file_incoming.get(p).is_some_and(|v| v.len() > 1)
                    || !claimed.insert(p.clone())
                {
                    continue;
                }
                let heading = parsed
                    .headings
                    .iter()
                    .rev()
                    .find(|h| h.line <= link.line)
                    .map(|h| h.text.as_str());
                own.push((p.clone(), heading));
            }
            if own.is_empty() {
                continue;
            }

            let mut dup_of: HashMap<Arc<Path>, Arc<Path>> = HashMap::new();
            type Group = Vec<(Arc<Path>, Vec<u8>)>;
            let mut by_hash: HashMap<(u64, u64), Group> = HashMap::new();
            for (p, _) in &own {
                let size = s.files.get(p).map_or(0, |f| f.size);
                if own
                    .iter()
                    .filter(|(q, _)| s.files.get(q).map_or(0, |f| f.size) == size)
                    .count()
                    < 2
                {
                    continue;
                }
                let Ok(bytes) = std::fs::read(self.root.join(p)) else {
                    continue;
                };
                let group = by_hash.entry((size, fnv1a(&bytes))).or_default();
                match group.iter().find(|(_, b)| *b == bytes) {
                    Some((kept, _)) => {
                        dup_of.insert(p.clone(), kept.clone());
                    }
                    None => group.push((p.clone(), bytes)),
                }
            }
            for (dup, kept) in &dup_of {
                plan.duplicates
                    .push((dup.to_path_buf(), kept.to_path_buf()));
            }

            let mut targets: Vec<(Arc<Path>, PathBuf)> = Vec::new();
            let mut pending = Vec::new();
            for (p, heading) in own.iter().filter(|(p, _)| !dup_of.contains_key(p)) {
                let ext = p
                    .extension()
                    .map(|x| x.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                if !policy.readable_names {
                    let dest = dir.join(file_name(p));
                    if dest.as_path() == p.as_ref()
                        || (!self.taken(&s, &dest) && reserved.insert(dest.clone()))
                    {
                        targets.push((p.clone(), dest));
                    }
                    continue;
                }
                let base = readable_base(&e.path, *heading, None);
                let name = file_name(p);
                let numbered = name
                    .strip_prefix(&format!("{base}-"))
                    .and_then(|rest| rest.strip_suffix(&format!(".{ext}")))
                    .is_some_and(|n| n.len() == 2 && n.bytes().all(|c| c.is_ascii_digit()));
                if parent_dir(p) == dir && numbered {
                    reserved.insert(p.to_path_buf());
                    targets.push((p.clone(), p.to_path_buf()));
                } else {
                    pending.push((p.clone(), base, ext));
                }
            }
            for (p, base, ext) in pending {
                let dest = (1..)
                    .map(|n| dir.join(format!("{base}-{n:02}.{ext}")))
                    .find(|d| !reserved.contains(d) && !self.taken(&s, d))
                    .expect("infinite name sequence");
                reserved.insert(dest.clone());
                targets.push((p, dest));
            }
            for (p, dest) in targets {
                if dest.as_path() != p.as_ref() {
                    plan.moves.push((p.to_path_buf(), dest));
                }
            }
        }
        for path in plan
            .moves
            .iter()
            .map(|(a, _)| a)
            .chain(plan.duplicates.iter().map(|(a, _)| a))
        {
            if let Some(f) = s.files.get(path.as_path()) {
                plan.stamps.push((path.clone(), f.size, f.modified));
            }
        }
        let touched: HashSet<&Path> = plan
            .moves
            .iter()
            .map(|(a, _)| a.as_path())
            .chain(plan.duplicates.iter().map(|(a, _)| a.as_path()))
            .collect();
        plan.link_updates = s
            .notes
            .iter()
            .flatten()
            .map(|e| {
                e.resolved
                    .iter()
                    .filter(|r| matches!(r, Resolved::File(p) if touched.contains(p.as_ref())))
                    .count()
            })
            .sum();
        plan
    }

    pub(crate) fn apply_tidy(&self, plan: &TidyPlan) -> Result<MoveReport> {
        let _ops = self.ops.lock();
        let mut reloc = Relocation::default();
        let mut skipped = Vec::new();
        let present = |p: &Path| std::fs::symlink_metadata(self.root.join(p)).is_ok();
        let unchanged = |p: &Path| match plan.stamps.iter().find(|(q, _, _)| q == p) {
            Some((_, size, modified)) => std::fs::metadata(self.root.join(p))
                .is_ok_and(|m| m.len() == *size && m.modified().ok() == Some(*modified)),
            None => true,
        };
        for (from, to) in &plan.moves {
            if !present(from) || present(to) || !unchanged(from) {
                skipped.push(from.clone());
                continue;
            }
            reloc.fs_moves.push((from.clone(), to.clone()));
            reloc.moved.push((from.clone(), to.clone()));
        }
        let final_of: HashMap<&Path, &Path> = reloc
            .moved
            .iter()
            .map(|(a, b)| (a.as_path(), b.as_path()))
            .collect();
        let mut redirect: HashMap<PathBuf, PathBuf> = reloc.moved.iter().cloned().collect();
        for (dup, kept) in &plan.duplicates {
            if !present(dup) || !present(kept) || !unchanged(dup) {
                skipped.push(dup.clone());
                continue;
            }
            let kept_final = final_of
                .get(kept.as_path())
                .copied()
                .unwrap_or(kept.as_path());
            redirect.insert(dup.clone(), kept_final.to_path_buf());
            reloc.trash.push(dup.clone());
        }
        reloc.redirect = redirect;
        let mut report = self.relocate(reloc, true)?;
        report.skipped = skipped;
        Ok(report)
    }
}

fn link_text(
    cfg: &ObsidianConfig,
    s: &State,
    note: &Path,
    path: &Path,
    heading: Option<&str>,
) -> String {
    let caption = heading
        .map(|h| h.trim().replace(['[', ']', '|'], ""))
        .unwrap_or_default();
    let unique = s.names.files_named(&file_name(path)).len() <= 1;
    let target = match cfg.new_link_format.as_str() {
        "absolute" => slash(path),
        "relative" => slash(&relative_path(parent_dir(note), path)),
        _ if unique => file_name(path),
        _ => slash(path),
    };
    if cfg.use_markdown_links {
        format!("![{caption}]({})", encode_link_path(&target))
    } else if caption.is_empty() {
        format!("![[{target}]]")
    } else {
        format!("![[{target}|{caption}]]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_dirs() {
        let n = Path::new("Red/AD/kerberoasting.md");
        assert_eq!(
            attachment_dir("./assets", n),
            PathBuf::from("Red/AD/assets")
        );
        assert_eq!(attachment_dir("/", n), PathBuf::new());
        assert_eq!(attachment_dir("", n), PathBuf::new());
        assert_eq!(attachment_dir("Files/", n), PathBuf::from("Files"));
        assert_eq!(attachment_dir("./", n), PathBuf::from("Red/AD"));
        assert_eq!(
            note_attachment_dir("./assets", n, true),
            PathBuf::from("Red/AD/assets/kerberoasting")
        );
    }

    #[test]
    fn timestamps() {
        assert_eq!(utc_timestamp(0), "19700101000000");
        assert_eq!(utc_timestamp(1_700_000_000), "20231114221320");
        let local =
            timestamp(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000));
        assert_eq!(local.len(), 14);
        assert!(local.starts_with("2023111"), "{local}");
    }
}
