use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::SystemTime;

pub type NoteId = u32;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("path leaves the vault: {0}")]
    OutsideVault(PathBuf),
    #[error("already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("unknown note: {0}")]
    UnknownNote(NoteId),
    #[error("modified on disk since last read: {0}")]
    Conflict(PathBuf),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, VaultError>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExternalLinks {
    #[default]
    Skip,
    Follow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkOut {
    Followed,
    NotFollowed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TrashKind {
    #[default]
    Vault,
    System,
}

#[derive(Debug, Clone)]
pub struct OpenOptions {
    pub cache_dir: PathBuf,
    pub ignore: Vec<String>,
    pub watch: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VaultStats {
    pub notes: usize,
    pub folders: usize,
    pub attachments: usize,
    pub bytes: u64,
    pub open_ms: f32,
    pub reparsed: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ObsidianConfig {
    pub attachment_folder: String,
    pub use_markdown_links: bool,
    pub new_link_format: String,
    pub templates_folder: Option<String>,
    pub daily_notes_folder: Option<String>,
    pub daily_notes_format: Option<String>,
    pub bookmarks: Vec<PathBuf>,
    pub property_types: BTreeMap<String, PropertyType>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum PropertyType {
    Text,
    Multitext,
    Number,
    Checkbox,
    Date,
    Datetime,
    Tags,
    Aliases,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeOrigin {
    Focus,
    Obsidian,
    Default,
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertyKind {
    pub kind: PropertyType,
    pub origin: TypeOrigin,
    pub obsidian: Option<PropertyType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ValueError {
    #[error("not a number")]
    NotNumber,
    #[error("not a date written YYYY-MM-DD")]
    DateForm,
    #[error("not a date and time written YYYY-MM-DDTHH:MM:SS")]
    DatetimeForm,
    #[error("no such day in that month")]
    NoSuchDay,
    #[error("no such time of day")]
    NoSuchTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertyDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub time: Option<(u32, u32, Option<u32>)>,
}

impl PropertyDate {
    pub fn unix_seconds(&self) -> i64 {
        imp::props::unix_seconds(self)
    }
}

impl PropertyType {
    pub const CHOOSABLE: [PropertyType; 6] = [
        PropertyType::Text,
        PropertyType::Multitext,
        PropertyType::Number,
        PropertyType::Checkbox,
        PropertyType::Date,
        PropertyType::Datetime,
    ];

    pub fn name(self) -> &'static str {
        imp::props::name(self)
    }
    pub fn from_name(name: &str) -> Option<PropertyType> {
        imp::props::from_name(name)
    }
    pub fn is_list(self) -> bool {
        matches!(
            self,
            PropertyType::Multitext | PropertyType::Tags | PropertyType::Aliases
        )
    }
    pub fn of_value(value: &PropValue) -> PropertyType {
        imp::props::of_value(value)
    }
    pub fn fits(self, value: &PropValue) -> bool {
        imp::props::fits(self, value)
    }
    pub fn check(self, text: &str) -> std::result::Result<(), ValueError> {
        imp::props::check(self, text)
    }
    pub fn accepts_typing(self, text: &str) -> bool {
        imp::props::accepts_typing(self, text)
    }
}

pub fn parse_property_date(
    text: &str,
    with_time: bool,
) -> std::result::Result<PropertyDate, ValueError> {
    imp::props::parse_date(text, with_time)
}

pub fn property_type(
    name: &str,
    value: &PropValue,
    chosen: Option<PropertyType>,
    obsidian: Option<PropertyType>,
) -> PropertyKind {
    imp::props::resolve(name, value, chosen, obsidian)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigEvent {
    PropertyTypes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Folder,
    Note,
    Attachment,
    Other,
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub name: Arc<str>,
    pub path: Arc<Path>,
    pub kind: NodeKind,
    pub depth: u16,
    pub parent: Option<u32>,
    pub subtree_end: u32,
    pub note: Option<NoteId>,
    pub note_count: u32,
    pub link_out: Option<LinkOut>,
}

#[derive(Debug, Clone, Default)]
pub struct TreeSnapshot {
    pub nodes: Vec<TreeNode>,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Wiki,
    Markdown,
    Embed,
    Property,
    Diagram,
    Include,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutLink {
    pub kind: LinkKind,
    pub target: String,
    pub anchor: Option<String>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    Note { id: NoteId, anchor: Option<String> },
    Attachment(PathBuf),
    External(String),
    Unresolved,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    List(Vec<PropValue>),
    Map(Vec<(String, PropValue)>),
}

#[derive(Debug, Clone)]
pub struct NoteMeta {
    pub id: NoteId,
    pub path: Arc<Path>,
    pub title: Arc<str>,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub headings: Vec<Heading>,
    pub blocks: Vec<(String, u32)>,
    pub props: Vec<(String, PropValue)>,
    pub links: Vec<OutLink>,
    pub words: u32,
    pub tasks_open: u32,
    pub tasks_done: u32,
    pub size: u64,
    pub modified: SystemTime,
}

#[derive(Debug, Clone)]
pub struct NoteSummary {
    pub id: NoteId,
    pub path: Arc<Path>,
    pub title: Arc<str>,
    pub aliases: Vec<String>,
    pub size: u64,
    pub modified: SystemTime,
}

#[derive(Debug, Clone)]
pub struct Backlink {
    pub source: NoteId,
    pub line: u32,
    pub context: String,
    pub range: std::ops::Range<usize>,
}

#[derive(Debug, Clone)]
pub struct TagCount {
    pub tag: String,
    pub count: u32,
}

#[derive(Debug, Clone)]
pub struct TaskItem {
    pub note: NoteId,
    pub line: u32,
    pub text: String,
    pub done: bool,
}

#[derive(Debug, Clone)]
pub enum VaultEvent {
    NoteAdded(NoteId),
    NoteChanged(NoteId),
    NoteRemoved {
        id: NoteId,
        path: PathBuf,
    },
    NoteMoved {
        id: NoteId,
        from: PathBuf,
        to: PathBuf,
    },
    TreeChanged,
}

#[derive(Debug, Clone)]
pub struct Rewritten {
    pub id: NoteId,
    pub path: PathBuf,
    pub links: usize,
    pub canvas: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MoveReport {
    pub updated_notes: Vec<NoteId>,
    pub updated_links: usize,
    pub rewritten: Vec<Rewritten>,
    pub skipped: Vec<PathBuf>,
}

#[derive(Debug, Default)]
pub struct TagRenameReport {
    pub notes: Vec<NoteId>,
    pub occurrences: usize,
    pub failed: Vec<(PathBuf, VaultError)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Placement {
    #[default]
    End,
    Start,
}

#[derive(Debug, Clone, Default)]
pub struct MergeReport {
    pub links: usize,
    pub rewritten: Vec<Rewritten>,
    pub trashed: Option<PathBuf>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AttachmentPolicy {
    pub per_note: bool,
    pub readable_names: bool,
    pub deduplicate: bool,
    pub follow_note: bool,
}

impl Default for AttachmentPolicy {
    fn default() -> Self {
        AttachmentPolicy {
            per_note: true,
            readable_names: true,
            deduplicate: true,
            follow_note: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AttachmentRequest<'a> {
    pub note: NoteId,
    pub bytes: &'a [u8],
    pub extension: &'a str,
    pub original_name: Option<&'a str>,
    pub heading: Option<&'a str>,
    pub directory: Option<&'a Path>,
    pub name_prefix: Option<&'a str>,
    pub policy: &'a AttachmentPolicy,
}

#[derive(Debug, Clone)]
pub struct SavedAttachment {
    pub path: PathBuf,
    pub link: String,
    pub reused: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TidyPlan {
    pub moves: Vec<(PathBuf, PathBuf)>,
    pub duplicates: Vec<(PathBuf, PathBuf)>,
    pub link_updates: usize,
    pub stamps: Vec<(PathBuf, u64, SystemTime)>,
}

pub struct Vault {
    inner: imp::Inner,
}

impl Vault {
    pub fn open(root: &Path, options: OpenOptions) -> Result<Arc<Vault>> {
        imp::open(root, options, ExternalLinks::Skip)
    }

    pub fn open_with(
        root: &Path,
        options: OpenOptions,
        external: ExternalLinks,
    ) -> Result<Arc<Vault>> {
        imp::open(root, options, external)
    }

    pub fn root(&self) -> &Path {
        self.inner.root()
    }
    pub fn name(&self) -> String {
        self.inner.name()
    }
    pub fn stats(&self) -> VaultStats {
        self.inner.stats()
    }
    pub fn obsidian(&self) -> ObsidianConfig {
        self.inner.obsidian()
    }
    pub fn property_type(&self, name: &str, value: &PropValue) -> PropertyKind {
        self.inner.property_type(name, value)
    }
    pub fn set_chosen_property_types(&self, types: BTreeMap<String, PropertyType>) {
        self.inner.set_chosen_property_types(types)
    }
    pub fn chosen_property_types(&self) -> BTreeMap<String, PropertyType> {
        self.inner.chosen_property_types()
    }
    pub fn subscribe_config(&self) -> Receiver<ConfigEvent> {
        self.inner.subscribe_config()
    }

    pub fn tree(&self) -> Arc<TreeSnapshot> {
        self.inner.tree()
    }
    pub fn note(&self, id: NoteId) -> Option<Arc<NoteMeta>> {
        self.inner.note(id)
    }
    pub fn note_by_path(&self, relative: &Path) -> Option<NoteId> {
        self.inner.note_by_path(relative)
    }
    pub fn notes(&self) -> Vec<Arc<NoteMeta>> {
        self.inner.notes()
    }
    pub fn summaries(&self) -> Vec<NoteSummary> {
        self.inner.summaries()
    }
    pub fn absolute(&self, relative: &Path) -> PathBuf {
        self.inner.absolute(relative)
    }

    pub fn read(&self, id: NoteId) -> Result<String> {
        self.inner.read(id)
    }
    pub fn write(
        &self,
        id: NoteId,
        content: &str,
        expected_modified: Option<SystemTime>,
    ) -> Result<SystemTime> {
        self.inner
            .write(id, content, expected_modified, HistoryKind::Edit)
    }
    pub fn write_with(
        &self,
        id: NoteId,
        content: &str,
        expected_modified: Option<SystemTime>,
        kind: HistoryKind,
    ) -> Result<SystemTime> {
        self.inner.write(id, content, expected_modified, kind)
    }
    pub fn set_history(&self, dir: &Path, config: HistoryConfig) -> Result<()> {
        self.inner.set_history(dir, config)
    }
    pub fn history(&self) -> Option<Arc<focus_history::History>> {
        self.inner.history()
    }
    pub fn take_history_error(&self) -> Option<VaultError> {
        self.inner.take_history_error()
    }
    pub fn create_note(&self, relative: &Path, content: &str) -> Result<NoteId> {
        self.inner.create_note(relative, content)
    }
    pub fn duplicate(&self, id: NoteId) -> Result<NoteId> {
        let meta = self.note(id).ok_or(VaultError::UnknownNote(id))?;
        let content = self.read(id)?;
        let stem = meta
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let base = match stem.rsplit_once(' ') {
            Some((base, number))
                if !base.is_empty()
                    && !number.is_empty()
                    && number.bytes().all(|b| b.is_ascii_digit()) =>
            {
                base.to_string()
            }
            _ => stem,
        };
        let stem = base;
        let folder = meta
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let mut n = 1;
        loop {
            match self.create_note(&folder.join(format!("{stem} {n}.md")), &content) {
                Err(VaultError::AlreadyExists(_)) => n += 1,
                result => return result,
            }
        }
    }
    pub fn is_outside(&self, relative: &Path) -> bool {
        self.inner.is_outside(relative)
    }
    pub fn tasks_of(&self, id: NoteId) -> Vec<TaskItem> {
        self.inner.tasks_of(id)
    }
    pub fn import(
        &self,
        sources: &[PathBuf],
        into: &Path,
        mode: ImportMode,
        cancel: &std::sync::atomic::AtomicBool,
        progress: &dyn Fn(ImportProgress),
    ) -> Result<ImportReport> {
        imp::import::import(self, sources, into, mode, cancel, progress)
    }

    pub fn create_folder(&self, relative: &Path) -> Result<()> {
        self.inner.create_folder(relative)
    }
    pub fn rename(&self, from: &Path, to: &Path, update_links: bool) -> Result<MoveReport> {
        self.inner.rename(from, to, update_links)
    }
    pub fn preview_rename(&self, from: &Path, to: &Path) -> Result<MoveReport> {
        self.inner.preview_rename(from, to)
    }
    pub fn trash(&self, relative: &Path) -> Result<()> {
        self.inner.trash(relative, TrashKind::Vault)
    }
    pub fn trash_with(&self, relative: &Path, kind: TrashKind) -> Result<()> {
        self.inner.trash(relative, kind)
    }
    pub fn set_attachment_policy(&self, policy: AttachmentPolicy) {
        self.inner.set_attachment_policy(policy)
    }
    pub fn attach(&self, request: &AttachmentRequest) -> Result<SavedAttachment> {
        self.inner.attach(request)
    }
    pub fn attachments_of(&self, note: NoteId) -> Vec<PathBuf> {
        self.inner.attachments_of(note)
    }
    pub fn orphan_attachments(&self) -> Vec<PathBuf> {
        self.inner.orphan_attachments()
    }
    pub fn plan_tidy_attachments(
        &self,
        scope: Option<NoteId>,
        policy: &AttachmentPolicy,
    ) -> TidyPlan {
        self.inner.plan_tidy_attachments(scope, policy)
    }
    pub fn apply_tidy(&self, plan: &TidyPlan) -> Result<MoveReport> {
        self.inner.apply_tidy(plan)
    }

    pub fn resolve_link(&self, from: NoteId, target: &str) -> LinkTarget {
        self.inner.resolve_link(Some(from), target)
    }
    pub fn resolve_link_from_root(&self, target: &str) -> LinkTarget {
        self.inner.resolve_link(None, target)
    }
    pub fn anchor_line(&self, id: NoteId, anchor: &str) -> Option<u32> {
        let meta = self.note(id)?;
        match anchor.strip_prefix('^') {
            Some(block) => meta
                .blocks
                .iter()
                .find(|(b, _)| b.eq_ignore_ascii_case(block))
                .map(|(_, line)| *line),
            None => {
                let wanted = anchor.rsplit('#').next().unwrap_or(anchor).trim();
                let wanted = wanted.to_lowercase();
                meta.headings
                    .iter()
                    .find(|h| h.text.trim().to_lowercase() == wanted)
                    .map(|h| h.line)
            }
        }
    }
    pub fn links_of(&self, id: NoteId) -> Vec<(OutLink, LinkTarget)> {
        self.inner.links_of(id)
    }
    pub fn source_path(&self, id: NoteId) -> Option<Arc<Path>> {
        self.inner.source_path(id)
    }
    pub fn canvas_by_path(&self, relative: &Path) -> Option<NoteId> {
        self.inner.canvas_by_path(relative)
    }
    pub fn canvases(&self) -> Vec<(NoteId, Arc<Path>)> {
        self.inner.canvases()
    }
    pub fn linked_from(&self, id: NoteId) -> Vec<NoteId> {
        self.inner.linked_from(id)
    }
    pub fn backlinks(&self, id: NoteId) -> Vec<Backlink> {
        self.inner.backlinks(id)
    }
    pub fn unresolved(&self, id: NoteId) -> Vec<OutLink> {
        self.inner.unresolved(id)
    }
    pub fn all_unresolved(&self) -> Vec<(NoteId, OutLink)> {
        self.inner.all_unresolved()
    }

    pub fn tags(&self) -> Vec<TagCount> {
        self.inner.tags()
    }
    pub fn rename_tag(&self, from: &str, to: &str) -> Result<TagRenameReport> {
        self.inner.rename_tag(from, to)
    }
    pub fn preview_rename_tag(&self, from: &str, to: &str) -> Result<TagRenameReport> {
        self.inner.preview_rename_tag(from, to)
    }
    pub fn merge(
        &self,
        source: NoteId,
        destination: NoteId,
        placement: Placement,
        trash: TrashKind,
    ) -> Result<MergeReport> {
        self.inner.merge(source, destination, placement, trash)
    }
    pub fn preview_merge(&self, source: NoteId, destination: NoteId) -> Result<MergeReport> {
        self.inner.preview_merge(source, destination)
    }
    pub fn insert_text(&self, id: NoteId, text: &str, placement: Placement) -> Result<SystemTime> {
        self.inner.insert_text(id, text, placement)
    }
    pub fn move_text(&self, from: NoteId, to: &Path, text: &str) -> String {
        self.inner.move_text(from, to, text)
    }
    pub fn link_to(&self, from: NoteId, to: NoteId) -> Option<String> {
        self.inner.link_to(from, to)
    }
    pub fn notes_with_tag(&self, tag: &str) -> Vec<NoteId> {
        self.inner.notes_with_tag(tag)
    }
    pub fn tasks(&self, include_done: bool) -> Vec<TaskItem> {
        self.inner.tasks(include_done)
    }
    pub fn toggle_task(&self, note: NoteId, line: u32) -> Result<bool> {
        self.inner.toggle_task(note, line)
    }

    pub fn subscribe(&self) -> Receiver<VaultEvent> {
        self.inner.subscribe()
    }
}

pub fn parse_meta(source: &str) -> ParsedMeta {
    imp::parse_meta(source)
}

#[derive(Debug, Clone, Default)]
pub struct ParsedMeta {
    pub title: Option<String>,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub headings: Vec<Heading>,
    pub blocks: Vec<(String, u32)>,
    pub props: Vec<(String, PropValue)>,
    pub links: Vec<OutLink>,
    pub words: u32,
    pub tasks_open: u32,
    pub tasks_done: u32,
    pub body_start_line: u32,
}

pub fn is_valid_tag(name: &str) -> bool {
    imp::retag::is_valid(name)
}

pub fn strip_accent(c: char) -> char {
    imp::fold_char(c)
}

pub mod settings {
    use super::*;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    #[serde(default)]
    pub struct Settings {
        pub language: Option<String>,
        pub last_vault: Option<PathBuf>,
        pub recent_vaults: Vec<PathBuf>,
        pub theme: String,
        pub reading_font_size: f32,
        pub measure_px: f32,
        pub show_hatch: bool,
        pub reduce_motion: bool,
        pub autosave_ms: u32,
        pub default_view: String,
        pub attachments: AttachmentPolicy,
        pub ops_enabled: bool,
        pub ops_folder: String,
        pub ai_model: String,
        pub ai_base_url: String,
        pub ai_redact: bool,
        pub ai_redact_ips: bool,
        pub ai_temperature: Option<f32>,
        pub ai_thinking: bool,
        pub ai_reply_language: String,
        pub ai_search: Option<bool>,
        pub ai_preprompt: String,
        pub ai_actions: Vec<serde_json::Value>,
        pub ai_write_tools: bool,
        pub history: HistoryConfig,
        pub ui: BTreeMap<String, serde_json::Value>,
    }

    impl Default for Settings {
        fn default() -> Self {
            Settings {
                language: None,
                last_vault: None,
                recent_vaults: Vec::new(),
                theme: "dark".into(),
                reading_font_size: 15.0,
                measure_px: 760.0,
                show_hatch: true,
                reduce_motion: false,
                autosave_ms: 800,
                default_view: "live".into(),
                attachments: AttachmentPolicy::default(),
                ops_enabled: false,
                ops_folder: "Engagements".into(),
                ai_model: "deepseek-flash".into(),
                ai_base_url: "https://api.deepseek.com".into(),
                ai_redact: true,
                ai_redact_ips: false,
                ai_temperature: None,
                ai_thinking: false,
                ai_reply_language: "interface".into(),
                ai_search: None,
                ai_preprompt: String::new(),
                ai_actions: Vec::new(),
                ai_write_tools: true,
                history: HistoryConfig::default(),
                ui: BTreeMap::new(),
            }
        }
    }

    pub fn config_dir() -> PathBuf {
        super::imp::config_dir()
    }
    pub fn home_dir() -> Option<PathBuf> {
        super::imp::home_dir()
    }
    pub fn data_dir() -> PathBuf {
        super::imp::data_dir()
    }
    pub fn history_dir_for(vault_root: &Path) -> PathBuf {
        super::imp::history_dir_for(vault_root)
    }
    pub fn cache_dir_for(vault_root: &Path) -> PathBuf {
        super::imp::cache_dir_for(vault_root)
    }
    pub fn load() -> anyhow::Result<Settings> {
        super::imp::load_settings()
    }
    pub fn save(settings: &Settings) -> anyhow::Result<()> {
        super::imp::save_settings(settings)
    }
}

mod imp;

pub use focus_history::{HistoryConfig, Kind as HistoryKind};
pub use imp::import::{ImportMode, ImportProgress, ImportReport, SkipReason};
