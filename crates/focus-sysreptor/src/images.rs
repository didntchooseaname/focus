use std::collections::HashMap;
use std::path::{Path, PathBuf};

const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_SEARCH_DEPTH: usize = 6;

#[derive(Debug, Clone)]
pub struct Image {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Attached(String),
    NotFound,
    Rejected(String),
    Remote,
    NoteEmbed,
}

pub(crate) struct Images {
    root: PathBuf,
    attached: Vec<Image>,
    by_path: HashMap<PathBuf, String>,
}

fn extension(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
}

pub(crate) fn is_image_extension(extension: &str) -> bool {
    matches!(
        extension,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp"
    )
}

fn signature_matches(extension: &str, bytes: &[u8]) -> bool {
    match extension {
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "webp" => bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        _ => false,
    }
}

fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(value) = bytes
                .get(i + 1..i + 3)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(value);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn project_name(file_name: &str) -> String {
    let cleaned: String = file_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_start_matches('.').to_string();
    if cleaned.is_empty() {
        "image".into()
    } else {
        cleaned
    }
}

fn find_by_name(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let mut subdirs = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.filter_map(Result::ok) {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_file() && entry.file_name().to_string_lossy() == name {
            return Some(entry.path());
        }
        if kind.is_dir() && !entry.file_name().to_string_lossy().starts_with('.') {
            subdirs.push(entry.path());
        }
    }
    if depth == 0 {
        return None;
    }
    subdirs.sort();
    subdirs
        .into_iter()
        .find_map(|d| find_by_name(&d, name, depth - 1))
}

impl Images {
    pub(crate) fn new(engagement_dir: &Path) -> Images {
        let root =
            dunce::canonicalize(engagement_dir).unwrap_or_else(|_| engagement_dir.to_path_buf());
        Images {
            root,
            attached: Vec::new(),
            by_path: HashMap::new(),
        }
    }

    pub(crate) fn into_attached(self) -> Vec<Image> {
        self.attached
    }

    pub(crate) fn resolve(&mut self, target: &str, note_dir: &Path, embed: bool) -> Outcome {
        let target = target.trim().trim_start_matches('<').trim_end_matches('>');
        let lower = target.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            return Outcome::Remote;
        }
        if lower.contains("://") || lower.starts_with("data:") || lower.starts_with("file:") {
            return Outcome::Rejected("scheme".into());
        }
        let decoded = percent_decoded(target);
        let Some(extension) = extension(&decoded) else {
            return if embed {
                Outcome::NoteEmbed
            } else {
                Outcome::Rejected("no file extension".into())
            };
        };
        if !is_image_extension(&extension) {
            return if embed && extension == "md" {
                Outcome::NoteEmbed
            } else {
                Outcome::Rejected(format!("not an image: .{extension}"))
            };
        }
        if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
            return Outcome::Rejected(format!("kind not taken by SysReptor: .{extension}"));
        }
        if Path::new(&decoded).is_absolute() {
            return Outcome::Rejected("absolute path".into());
        }
        let found = [
            note_dir.join(&decoded),
            self.root.join(&decoded),
            self.root.join("Evidence").join(&decoded),
        ]
        .into_iter()
        .find(|p| p.is_file())
        .or_else(|| {
            embed
                .then(|| {
                    Path::new(&decoded).file_name().and_then(|n| {
                        find_by_name(&self.root, &n.to_string_lossy(), MAX_SEARCH_DEPTH)
                    })
                })
                .flatten()
        });
        let Some(found) = found else {
            return Outcome::NotFound;
        };
        let Ok(canonical) = dunce::canonicalize(&found) else {
            return Outcome::NotFound;
        };
        if !canonical.starts_with(&self.root) {
            return Outcome::Rejected("outside the engagement folder".into());
        }
        if let Some(name) = self.by_path.get(&canonical) {
            return Outcome::Attached(name.clone());
        }
        let Ok(metadata) = std::fs::metadata(&canonical) else {
            return Outcome::NotFound;
        };
        if !metadata.is_file() {
            return Outcome::Rejected("not a regular file".into());
        }
        if metadata.len() > MAX_IMAGE_BYTES {
            return Outcome::Rejected(format!("larger than {} MiB", MAX_IMAGE_BYTES / 1024 / 1024));
        }
        let Ok(bytes) = std::fs::read(&canonical) else {
            return Outcome::NotFound;
        };
        if !signature_matches(&extension, &bytes) {
            return Outcome::Rejected("content does not match the extension".into());
        }
        let wanted = project_name(
            &canonical
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        let name = self.unique_name(&wanted);
        self.by_path.insert(canonical, name.clone());
        self.attached.push(Image {
            name: name.clone(),
            bytes,
        });
        Outcome::Attached(name)
    }

    fn unique_name(&self, wanted: &str) -> String {
        let taken = |name: &str| self.attached.iter().any(|i| i.name == name);
        if !taken(wanted) {
            return wanted.to_string();
        }
        let (stem, ext) = wanted.rsplit_once('.').unwrap_or((wanted, ""));
        (2..)
            .map(|n| {
                if ext.is_empty() {
                    format!("{stem}-{n}")
                } else {
                    format!("{stem}-{n}.{ext}")
                }
            })
            .find(|n| !taken(n))
            .unwrap_or_else(|| wanted.to_string())
    }
}
