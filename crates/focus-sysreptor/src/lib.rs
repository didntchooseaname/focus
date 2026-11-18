mod archive;
mod cwe;
mod cwe_ids;
mod design;
mod export;
mod finding;
mod ids;
mod images;
mod mask;
mod project;
mod summary;
mod text;

use std::io::Write;
use std::path::{Path, PathBuf};

use focus_ops::Engagement;
use serde_json::Value;

pub use design::Fields;
pub use images::Image;
pub use summary::{Entry, Kind, Summary};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Ops(#[from] focus_ops::OpsError),
    #[error("cannot write the archive: {0}")]
    Archive(#[source] std::io::Error),
    #[error("cannot encode {what}: {message}")]
    Encode { what: &'static str, message: String },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Secrets {
    Masked,
    Clear,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub secrets: Secrets,
    pub credentials: bool,
    pub language: String,
    pub author: String,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            secrets: Secrets::Masked,
            credentials: false,
            language: "en-US".into(),
            author: String::new(),
        }
    }
}

#[derive(Debug)]
pub struct ProjectArchive {
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub summary: Summary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    Toml,
    Json,
}

#[derive(Debug)]
pub struct FindingFile {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct FindingFiles {
    pub files: Vec<FindingFile>,
    pub images: Vec<Image>,
    pub summary: Summary,
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    out.chars()
        .take(60)
        .collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

fn encode(value: &Value, what: &'static str) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|e| Error::Encode {
        what,
        message: e.to_string(),
    })
}

pub fn export_project(engagement: &Engagement, options: &Options) -> Result<ProjectArchive> {
    let (prepared, mut masker) = export::prepare(engagement, options)?;
    let mut summary = prepared.summary.clone();
    summary.design = design::DESIGN_NAME.to_string();
    let project = project::project(&prepared, options, &mut masker, &mut summary);
    let id = project::project_id(&prepared);
    let mut entries = vec![(format!("{id}.json"), encode(&project, "the project")?)];
    for image in &prepared.images {
        entries.push((format!("{id}-images/{}", image.name), image.bytes.clone()));
    }
    let bytes = archive::tar_gz(&entries).map_err(Error::Archive)?;
    summary.masked = masker.replaced;
    let name = slug(&engagement.name);
    Ok(ProjectArchive {
        file_name: format!(
            "{}-sysreptor.tar.gz",
            if name.is_empty() { "engagement" } else { &name }
        ),
        bytes,
        summary,
    })
}

pub fn export_finding_files(
    engagement: &Engagement,
    options: &Options,
    fields: &Fields,
    format: FileFormat,
    design_name: &str,
) -> Result<FindingFiles> {
    let (prepared, masker) = export::prepare(engagement, options)?;
    let mut summary = prepared.summary.clone();
    summary.design = design_name.to_string();
    let mut missing = Vec::new();
    let mut files = Vec::with_capacity(prepared.findings.len());
    for finding in &prepared.findings {
        let data = export::finding_data(finding, fields, &mut summary, &mut missing);
        let value =
            serde_json::json!({"status": export::review_status(finding.status), "data": data});
        let bytes = match format {
            FileFormat::Json => {
                let mut bytes = serde_json::to_vec_pretty(&value).map_err(|e| Error::Encode {
                    what: "a finding",
                    message: e.to_string(),
                })?;
                bytes.push(b'\n');
                bytes
            }
            FileFormat::Toml => toml::to_string_pretty(&value)
                .map_err(|e| Error::Encode {
                    what: "a finding",
                    message: e.to_string(),
                })?
                .into_bytes(),
        };
        let extension = if format == FileFormat::Json {
            "json"
        } else {
            "toml"
        };
        let title = slug(&finding.title);
        let file_name = if title.is_empty() {
            format!("{}.{extension}", finding.id)
        } else {
            format!("{}-{title}.{extension}", finding.id)
        };
        files.push(FindingFile { file_name, bytes });
    }
    for name in missing {
        summary.note(Kind::NoFieldInDesign, None, name);
    }
    if !prepared.images.is_empty() {
        summary.note_count(Kind::ImagesWrittenAside, None, prepared.images.len());
    }
    summary.masked = masker.replaced;
    Ok(FindingFiles {
        files,
        images: prepared.images,
        summary,
    })
}

pub fn write_output(path: &Path, bytes: &[u8], secrets: Secrets) -> Result<()> {
    let io = |source| Error::Io {
        path: path.to_path_buf(),
        source,
    };
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if secrets == Secrets::Clear {
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    }
    #[cfg(not(unix))]
    let _ = secrets;
    let mut file = options.open(path).map_err(io)?;
    #[cfg(unix)]
    if secrets == Secrets::Clear {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(io)?;
    }
    file.write_all(bytes).map_err(io)
}
