use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

mod cvss4_table;
mod engagement;
mod frontmatter;
mod fsutil;
mod overview;
mod versions;

pub use versions::{attach_history, detach_history, keep_version};

#[derive(Debug, thiserror::Error)]
pub enum OpsError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("engagement already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("{file}: {message}")]
    Parse { file: PathBuf, message: String },
    #[error("{0}")]
    Invalid(String),
    #[error("modified on disk since it was read: {0}")]
    Conflict(PathBuf),
}

pub type Result<T> = std::result::Result<T, OpsError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngagementKind {
    ExternalPentest,
    InternalPentest,
    WebPentest,
    RedTeam,
    Lab,
    Exam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngagementStatus {
    Planning,
    InProgress,
    Reporting,
    Done,
}

#[derive(Debug, Clone)]
pub struct Engagement {
    pub dir: PathBuf,
    pub name: String,
    pub kind: EngagementKind,
    pub status: EngagementStatus,
    pub client: Option<String>,
    pub started: Option<String>,
    pub deadline: Option<String>,
    pub scope: Vec<String>,
    pub out_of_scope: Vec<String>,
    pub vars: BTreeMap<String, String>,
    pub var_links: BTreeMap<String, String>,
    pub pauses: Vec<String>,
    pub stopped: Option<String>,
    pub exam: Option<String>,
    pub exam_done: Vec<String>,
    pub focus: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewEngagement {
    pub name: String,
    pub kind: EngagementKind,
    pub client: Option<String>,
    pub scope: Vec<String>,
    pub exam: Option<String>,
    pub checklist: Option<String>,
}

pub fn list(ops_dir: &Path) -> Vec<Engagement> {
    engagement::list(ops_dir)
}

pub fn load(engagement_dir: &Path) -> Result<Engagement> {
    engagement::load(engagement_dir)
}

pub fn create(ops_dir: &Path, spec: &NewEngagement) -> Result<Engagement> {
    engagement::create(ops_dir, spec)
}

pub fn save(engagement: &Engagement) -> Result<()> {
    engagement::save(engagement)
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum HostStatus {
    New,
    Enum,
    Foothold,
    User,
    Owned,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Service {
    pub port: u16,
    pub proto: String,
    pub state: String,
    pub name: String,
    pub product: String,
    pub version: String,
    pub notes: String,
}

#[derive(Debug, Clone)]
pub struct Host {
    pub path: PathBuf,
    pub ip: String,
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub status: HostStatus,
    pub services: Vec<Service>,
    pub tags: Vec<String>,
    pub flags: Vec<String>,
    pub points: Option<u32>,
}

pub mod hosts;
pub mod links;
pub mod proofs;
pub mod targets;
pub mod timer;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingStatus {
    Draft,
    Confirmed,
    Reported,
    Fixed,
    Accepted,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub path: PathBuf,
    pub id: String,
    pub title: String,
    pub severity: Severity,
    pub cvss_vector: Option<String>,
    pub cvss_score: Option<f32>,
    pub status: FindingStatus,
    pub assets: Vec<String>,
    pub cwe: Vec<String>,
    pub attack: Vec<String>,
}

pub mod findings;

pub mod cvss;
pub mod cwe;

#[derive(Debug, Clone, PartialEq)]
pub struct Credential {
    pub user: String,
    pub kind: String,
    pub secret: String,
    pub domain: String,
    pub source: String,
    pub valid_on: String,
    pub notes: String,
}

pub mod loot;

#[derive(Debug, Clone, PartialEq)]
pub struct JournalEntry {
    pub timestamp: String,
    pub target: Option<String>,
    pub text: String,
    pub id: Option<String>,
}

pub mod journal;

pub mod vars;

pub mod snippets;

pub mod checklist;

pub mod exam;

pub mod relations;

pub mod report;

#[derive(Debug, Clone, Default)]
pub struct Overview {
    pub hosts: usize,
    pub hosts_owned: usize,
    pub services: usize,
    pub findings: [usize; 5],
    pub credentials: usize,
    pub journal_entries: usize,
    pub checklist_done: u32,
    pub checklist_total: u32,
    pub points: u32,
}

pub fn overview(engagement_dir: &Path) -> Overview {
    overview::compute(engagement_dir)
}
