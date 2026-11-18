use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use focus_ai::agent::{self, ToolHost, ToolOutcome};
use focus_ai::prompts::Template;
use focus_ai::{Cancel, ToolCall, ToolSpec};
use focus_core::{LinkTarget, NodeKind, Vault, VaultError};
use focus_search::{CancelToken, FuzzyScope, SearchIndex, SearchOptions, parse_query};
use serde_json::Value;

use crate::exposure::{Exposure, HIDDEN_NOTE, SECRET_NOTE};
use crate::fault::Fault;
use crate::ipc;
use crate::ops::OpsConfig;
use crate::paths::VaultPaths;
use crate::rules::{self, Begin, Idempotency, WriteOptions, etag};
use crate::specs;

const DEFAULT_SEARCH_LIMIT: usize = 10;
const MAX_SEARCH_LIMIT: usize = 30;
const MATCHES_PER_NOTE: usize = 3;
const DEFAULT_READ_CHARS: usize = 8_000;
const MAX_READ_CHARS: usize = 20_000;
const MAX_LIST_ENTRIES: usize = 300;
const MAX_LINK_ENTRIES: usize = 100;
pub(crate) const MAX_WRITE_BYTES: usize = 1_000_000;
pub(crate) const MAX_QUERY_CHARS: usize = 2_000;
pub(crate) const MAX_NOTICE_CHARS: usize = 4_000;

pub(crate) fn bounded(name: &str, text: &str, max: usize) -> Result<(), String> {
    let length = text.chars().count();
    if length > max {
        Err(format!(
            "parameter `{name}` is too long: {length} characters, at most {max}"
        ))
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub struct WriteProposal<'a> {
    pub tool: &'a str,
    pub path: &'a Path,
    pub before: Option<&'a str>,
    pub after: &'a str,
}

pub type Confirm = Arc<dyn Fn(&WriteProposal, &Cancel) -> bool + Send + Sync>;

pub(crate) struct ExportAvailability {
    result: std::sync::OnceLock<bool>,
    pending: Mutex<Option<std::thread::JoinHandle<bool>>>,
}

impl ExportAvailability {
    fn known(value: bool) -> Self {
        ExportAvailability {
            result: std::sync::OnceLock::from(value),
            pending: Mutex::new(None),
        }
    }

    fn get(&self) -> bool {
        *self.result.get_or_init(|| {
            self.pending
                .lock()
                .ok()
                .and_then(|mut p| p.take())
                .is_some_and(|handle| handle.join().unwrap_or(false))
        })
    }
}

#[derive(Clone)]
enum Writes {
    Refused,
    Confirmed(Confirm),
    Unconfirmed,
}

#[derive(Clone)]
pub struct VaultTools {
    pub(crate) vault: Arc<Vault>,
    pub(crate) index: Arc<SearchIndex>,
    pub(crate) paths: Arc<VaultPaths>,
    writes: Writes,
    extended: bool,
    pub(crate) ops: Option<Arc<OpsConfig>>,
    pub(crate) secret_guard: Option<Arc<PathBuf>>,
    pub(crate) app: Option<ipc::Client>,
    pub(crate) export_dir: PathBuf,
    pub(crate) templates: Arc<Vec<Template>>,
    export: Arc<ExportAvailability>,
    structured_errors: bool,
    idempotency: Arc<Idempotency>,
}

impl VaultTools {
    pub fn new(vault: Arc<Vault>, index: Arc<SearchIndex>) -> VaultTools {
        let paths = Arc::new(VaultPaths::new(vault.root()));
        VaultTools {
            vault,
            index,
            paths,
            writes: Writes::Refused,
            extended: false,
            ops: None,
            secret_guard: None,
            app: None,
            export_dir: PathBuf::from(crate::export::DEFAULT_EXPORT_DIR),
            templates: Arc::new(Vec::new()),
            export: Arc::new(ExportAvailability::known(true)),
            structured_errors: false,
            idempotency: Arc::new(Idempotency::default()),
        }
    }

    pub fn with_structured_errors(mut self) -> VaultTools {
        self.structured_errors = true;
        self
    }

    pub fn with_extended_tools(mut self) -> VaultTools {
        self.extended = true;
        self
    }

    pub fn with_ops(mut self, dir: PathBuf, reveal_secrets: bool) -> VaultTools {
        if !reveal_secrets {
            self = self.with_masked_engagements(&dir);
        }
        if dir.is_dir() {
            self.ops = Some(Arc::new(OpsConfig {
                dir,
                reveal_secrets,
            }));
        }
        self
    }

    pub fn with_masked_engagements(mut self, dir: &Path) -> VaultTools {
        self.secret_guard =
            crate::exposure::engagements_relative(self.vault.root(), dir).map(Arc::new);
        self
    }

    pub fn with_app(mut self, client: ipc::Client) -> VaultTools {
        self.app = Some(client);
        self
    }

    pub fn with_export(mut self, available: bool) -> VaultTools {
        self.export = Arc::new(ExportAvailability::known(available));
        self
    }

    pub fn with_export_probe(mut self, probe: std::thread::JoinHandle<bool>) -> VaultTools {
        self.export = Arc::new(ExportAvailability {
            result: std::sync::OnceLock::new(),
            pending: Mutex::new(Some(probe)),
        });
        self
    }

    pub fn with_export_dir(mut self, dir: PathBuf) -> VaultTools {
        self.export_dir = dir;
        self
    }

    pub fn with_prompt_templates(mut self, templates: Vec<Template>) -> VaultTools {
        self.templates = Arc::new(templates);
        self
    }

    pub fn with_confirmation(
        mut self,
        confirm: impl Fn(&WriteProposal, &Cancel) -> bool + Send + Sync + 'static,
    ) -> VaultTools {
        self.writes = Writes::Confirmed(Arc::new(confirm));
        self
    }

    pub fn with_unconfirmed_writes(mut self) -> VaultTools {
        self.writes = Writes::Unconfirmed;
        self
    }

    pub fn writes_enabled(&self) -> bool {
        !matches!(self.writes, Writes::Refused)
    }
}

pub(crate) type Outcome = Result<String, String>;

pub(crate) fn text_argument<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, String> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("parameter `{name}` is missing or is not text"))
}

pub(crate) struct Page {
    pub offset: usize,
    pub limit: usize,
}

impl Page {
    pub fn of(arguments: &Value, default_limit: usize, max_limit: usize) -> Result<Page, String> {
        let offset = optional_number(arguments, "offset")?.unwrap_or(0);
        if offset > MAX_OFFSET {
            return Err(format!(
                "parameter `offset`: at most {MAX_OFFSET}, narrow the request instead"
            ));
        }
        Ok(Page {
            offset,
            limit: optional_number(arguments, "limit")?
                .unwrap_or(default_limit)
                .clamp(1, max_limit),
        })
    }

    pub fn cut<T>(&self, rows: Vec<T>, what: &str, narrow: &str) -> (Vec<T>, String) {
        let total = rows.len();
        let shown: Vec<T> = rows
            .into_iter()
            .skip(self.offset)
            .take(self.limit)
            .collect();
        let note = more(
            total.saturating_sub(self.offset + shown.len()),
            what,
            self.offset + shown.len(),
            narrow,
        );
        (shown, note)
    }
}

const MAX_OFFSET: usize = 100_000;

pub(crate) fn more(remaining: usize, what: &str, next_offset: usize, narrow: &str) -> String {
    if remaining == 0 {
        return String::new();
    }
    let narrow = if narrow.is_empty() {
        String::new()
    } else {
        format!(", or narrow the request: {narrow}")
    };
    format!(
        "[{remaining} more {what} not shown: send offset={next_offset} for the next ones{narrow}]\n"
    )
}

pub(crate) fn optional_number(arguments: &Value, name: &str) -> Result<Option<usize>, String> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(|n| Some(n as usize))
            .ok_or_else(|| format!("parameter `{name}`: positive integer expected")),
    }
}

impl ToolHost for VaultTools {
    fn specs(&self) -> Vec<ToolSpec> {
        let mut specs: Vec<ToolSpec> = agent::standard_tools(self.writes_enabled())
            .into_iter()
            .map(specs::adapt)
            .collect();
        let keep = |spec: &ToolSpec| self.writes_enabled() || !agent::is_write_tool(&spec.name);
        if self.extended {
            specs.extend(
                specs::vault_tools()
                    .into_iter()
                    .filter(keep)
                    .filter(|spec| spec.name != "export_note" || self.export.get()),
            );
        }
        if self.ops.is_some() {
            specs.extend(specs::ops_tools().into_iter().filter(keep));
        }
        if self.app.is_some() {
            specs.extend(specs::app_tools().into_iter().filter(keep));
        }
        specs
    }

    fn call(&self, call: &ToolCall, cancel: &Cancel) -> ToolOutcome {
        let specs = self.specs();
        let properties = specs
            .iter()
            .find(|spec| spec.name == call.name)
            .and_then(|spec| spec.parameters.get("properties"));
        let arguments: Value = if call.arguments.trim().is_empty() {
            Value::Object(Default::default())
        } else {
            match serde_json::from_str(&call.arguments) {
                Ok(v @ Value::Object(_)) => v,
                Ok(_) => {
                    return self.failure(Fault::from_message(
                        "the arguments must form a JSON object",
                        &Value::Null,
                        properties,
                    ));
                }
                Err(e) => {
                    return self.failure(Fault::from_message(
                        &format!("invalid JSON arguments: {e}"),
                        &Value::Null,
                        properties,
                    ));
                }
            }
        };
        let fail =
            |message: &str| self.failure(Fault::from_message(message, &arguments, properties));
        let known = specs.iter().any(|spec| spec.name == call.name);
        if !known && agent::is_write_tool(&call.name) && !self.writes_enabled() {
            return self.failure(Fault::read_only());
        }
        if !known {
            return fail(&format!("unknown tool: {}", call.name));
        }
        if agent::is_write_tool(&call.name)
            || call.name.starts_with("ops_")
            || call.name.starts_with("app_")
        {
            let options = if takes_write_rules(&call.name) {
                match WriteOptions::parse(&arguments) {
                    Ok(options) => options,
                    Err(fault) => return self.failure(fault),
                }
            } else {
                WriteOptions::default()
            };
            let reservation = match self.begin_keyed(&call.name, &arguments, &options) {
                Ok(Keyed::Run(reservation)) => reservation,
                Ok(Keyed::Answer(text)) => return ToolOutcome::Ok(text),
                Err(fault) => return self.failure(fault),
            };
            let result = match call.name.as_str() {
                n if n.starts_with("ops_") => self.ops_tool(n, &arguments, &options, cancel),
                n if n.starts_with("app_") => self.app_tool(n, &arguments, &options, cancel),
                "toggle_task" => self.toggle_task(&arguments, &options, cancel),
                "rename_note" => self.rename_note(&arguments, &options, cancel),
                "trash_note" => self.trash_note(&arguments, &options, cancel),
                "export_note" => self.export_note(&arguments, &options, cancel),
                n => self.write(n, &arguments, &options, cancel),
            };
            return match result {
                Ok(text) => {
                    let text = agent::truncate(&text);
                    if let Some(reservation) = reservation {
                        reservation.complete(&text);
                    }
                    ToolOutcome::Ok(text)
                }
                Err(Refusal::Error(text)) => fail(&text),
                Err(Refusal::Fault(fault)) => self.failure(fault),
                Err(Refusal::Denied) => ToolOutcome::Denied,
            };
        }
        let result = match call.name.as_str() {
            "search_notes" => self.search_notes(&arguments, cancel),
            "read_note" => self.read_note(&arguments),
            "list_folder" => self.list_folder(&arguments),
            "get_backlinks" => self.get_backlinks(&arguments),
            "get_outline" => self.get_outline(&arguments),
            "list_tags" => self.list_tags(&arguments),
            "get_neighbors" => self.get_neighbors(&arguments),
            "list_tasks" => self.list_tasks(&arguments),
            "list_attachments" => self.list_attachments(&arguments),
            "orphan_attachments" => self.orphan_attachments(&arguments),
            "shortest_path" => self.shortest_path(&arguments),
            "most_linked" => self.most_linked(&arguments),
            "orphan_notes" => self.orphan_notes(&arguments),
            other => Err(format!("unknown tool: {other}")),
        };
        match result {
            Ok(text) => ToolOutcome::Ok(agent::truncate(&text)),
            Err(text) => fail(&text),
        }
    }
}

fn takes_write_rules(tool: &str) -> bool {
    agent::is_write_tool(tool) || tool == "ops_report_build"
}

const IDEMPOTENT_TOOLS: &[&str] = &[
    "create_note",
    "append_to_note",
    "ops_journal_append",
    "ops_finding_create",
    "ops_credential_add",
    "ops_import_nmap",
];

enum Keyed<'a> {
    Run(Option<rules::Reservation<'a>>),
    Answer(String),
}

impl VaultTools {
    fn error_text(&self, fault: &Fault) -> String {
        if self.structured_errors {
            fault.to_json().to_string()
        } else {
            fault.message.clone()
        }
    }

    fn failure(&self, fault: Fault) -> ToolOutcome {
        ToolOutcome::Error(self.error_text(&fault))
    }

    fn begin_keyed(
        &self,
        tool: &str,
        arguments: &Value,
        options: &WriteOptions,
    ) -> Result<Keyed<'_>, Fault> {
        let Some(key) = options
            .idempotency_key
            .as_deref()
            .filter(|_| IDEMPOTENT_TOOLS.contains(&tool))
        else {
            return Ok(Keyed::Run(None));
        };
        if options.dry_run {
            return Ok(if self.idempotency.peek(tool, key, arguments) {
                Keyed::Answer(
                    "Dry run: this call already ran with this idempotency_key. Sent again it would change nothing. Nothing was written.".into(),
                )
            } else {
                Keyed::Run(None)
            });
        }
        match self.idempotency.begin(tool, key, arguments) {
            Begin::Fresh(reservation) => Ok(Keyed::Run(Some(reservation))),
            Begin::Replay(text) => Ok(Keyed::Answer(text)),
            Begin::Refused(fault) => Err(fault),
        }
    }
}

pub(crate) enum Refusal {
    Error(String),
    Fault(Fault),
    Denied,
}

impl From<Fault> for Refusal {
    fn from(fault: Fault) -> Self {
        Refusal::Fault(fault)
    }
}

impl From<&str> for Refusal {
    fn from(text: &str) -> Self {
        Refusal::Error(text.to_string())
    }
}

impl From<String> for Refusal {
    fn from(text: String) -> Self {
        Refusal::Error(text)
    }
}

pub(crate) fn describe(error: VaultError) -> String {
    match error {
        VaultError::Conflict(path) => format!(
            "{} changed on disk since it was read: read the note again and retry",
            path.display()
        ),
        VaultError::AlreadyExists(path) => format!("already exists: {}", path.display()),
        VaultError::OutsideVault(path) => format!("path outside the vault: {}", path.display()),
        other => other.to_string(),
    }
}

impl VaultTools {
    pub(crate) fn locate(&self, input: &str) -> Result<(focus_core::NoteId, PathBuf), String> {
        let relative = self.paths.clean(input, false)?;
        let mut candidates = vec![relative.clone()];
        if relative
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("md"))
        {
            candidates.push(PathBuf::from(format!("{}.md", relative.display())));
        }
        for candidate in candidates {
            if let Some(id) = self.vault.note_by_path(&candidate) {
                match self.exposure(&candidate) {
                    Exposure::Outside => {
                        return Err(format!(
                            "path outside the vault: {} (symbolic link to the outside)",
                            candidate.display()
                        ));
                    }
                    Exposure::Secret => return Err(SECRET_NOTE.to_string()),
                    Exposure::Open | Exposure::Masked => {}
                }
                return Ok((id, candidate));
            }
        }
        let query = relative
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let near: Vec<String> = self
            .index
            .fuzzy(&query, FuzzyScope::Notes, &[], 3)
            .into_iter()
            .filter_map(|hit| {
                hit.note
                    .filter(|id| self.shown(*id))
                    .and_then(|id| self.vault.note(id))
            })
            .map(|meta| meta.path.display().to_string())
            .collect();
        if near.is_empty() {
            Err(format!("note not found: {input}"))
        } else {
            Err(format!(
                "note not found: {input}. Similar notes: {}",
                near.join(", ")
            ))
        }
    }

    pub(crate) fn path_of(&self, id: focus_core::NoteId) -> String {
        if !self.shown(id) {
            return HIDDEN_NOTE.to_string();
        }
        self.vault
            .source_path(id)
            .map_or_else(|| format!("(note {id})"), |p| p.display().to_string())
    }

    fn search_notes(&self, arguments: &Value, cancel: &Cancel) -> Outcome {
        let source = text_argument(arguments, "query")?.trim();
        if source.is_empty() {
            return Err("empty query".into());
        }
        bounded("query", source, MAX_QUERY_CHARS)?;
        let page = Page::of(arguments, DEFAULT_SEARCH_LIMIT, MAX_SEARCH_LIMIT)?;
        let (with_title, with_matches) = search_fields(arguments)?;
        let query = parse_query(source);
        if !query.diagnostics.is_empty() {
            return Err(format!(
                "invalid query: {}",
                query
                    .diagnostics
                    .iter()
                    .map(describe_diagnostic)
                    .collect::<Vec<_>>()
                    .join(" ; ")
            ));
        }
        let options = SearchOptions {
            max_files: page.offset + page.limit,
            max_matches_per_file: MATCHES_PER_NOTE,
            ..SearchOptions::default()
        };
        let token = CancelToken::new();
        let finished = std::sync::atomic::AtomicBool::new(false);
        let results = std::thread::scope(|scope| {
            scope.spawn(|| {
                while !finished.load(std::sync::atomic::Ordering::Relaxed) {
                    if cancel.is_cancelled() {
                        token.cancel();
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            });
            let results = self.index.search(&query, &options, &token);
            finished.store(true, std::sync::atomic::Ordering::Relaxed);
            results
        });
        if results.cancelled {
            return Err("search cancelled".into());
        }
        let visible: Vec<_> = results
            .hits
            .iter()
            .filter(|hit| self.shown(hit.note))
            .collect();
        if visible.is_empty() {
            return Ok(format!("No note matches: {source}"));
        }
        let (total_files, total_matches) = if visible.len() == results.hits.len() {
            (results.total_files, results.total_matches)
        } else {
            (
                visible.len(),
                visible.iter().map(|hit| hit.match_count as usize).sum(),
            )
        };
        let total_files = total_files.max(visible.len());
        let rows: Vec<_> = visible
            .into_iter()
            .skip(page.offset)
            .take(page.limit)
            .collect();
        if rows.is_empty() {
            return Ok(format!(
                "No note after offset {}: {total_files} note(s) match {source}",
                page.offset
            ));
        }
        let mut out = format!("{total_files} note(s), {total_matches} match(es) for: {source}");
        if rows.len() < total_files {
            out.push_str(&format!(
                " (notes {} to {} shown)",
                page.offset + 1,
                page.offset + rows.len()
            ));
        }
        out.push('\n');
        let remaining = total_files.saturating_sub(page.offset + rows.len());
        let next_offset = page.offset + rows.len();
        for hit in rows {
            let Some(meta) = self.vault.note(hit.note) else {
                continue;
            };
            let title = if with_title {
                format!(": {}", meta.title)
            } else {
                String::new()
            };
            out.push_str(&format!(
                "{}{title} ({} occurrence(s))\n",
                meta.path.display(),
                hit.match_count
            ));
            if with_matches {
                for m in &hit.matches {
                    out.push_str(&format!(
                        "  {}: {}\n",
                        m.line + 1,
                        self.displayed(hit.note, m.text.trim())
                    ));
                }
            }
        }
        out.push_str(&more(
            remaining,
            "note(s)",
            next_offset,
            "add path:, tag: or file: to the query",
        ));
        Ok(out)
    }

    fn read_note(&self, arguments: &Value) -> Outcome {
        let (id, path) = self.locate(text_argument(arguments, "path")?)?;
        let max_chars = optional_number(arguments, "max_chars")?
            .unwrap_or(DEFAULT_READ_CHARS)
            .clamp(1, MAX_READ_CHARS);
        let raw = self.vault.read(id).map_err(describe)?;
        let version = etag(&raw);
        let content = self.displayed(id, &raw);
        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        let start = optional_number(arguments, "start_line")?
            .unwrap_or(1)
            .max(1);
        let end = optional_number(arguments, "end_line")?
            .unwrap_or(total)
            .min(total);
        if total == 0 {
            return Ok(format!("{}: empty note (etag {version})", path.display()));
        }
        if start > total {
            return Err(format!("{} has only {total} line(s)", path.display()));
        }
        if end < start {
            return Err(format!("invalid range: {start} to {end}"));
        }
        let mut body = String::new();
        let mut used = 0usize;
        let mut last = start - 1;
        for (offset, line) in lines[start - 1..end].iter().enumerate() {
            let number = start + offset;
            let cost = line.chars().count() + number.to_string().len() + 3;
            if used + cost > max_chars && used > 0 {
                break;
            }
            if cost > max_chars {
                let keep = max_chars.saturating_sub(number.to_string().len() + 5);
                let cut: String = line.chars().take(keep).collect();
                body.push_str(&format!("{number}: {cut}…\n"));
                last = number;
                break;
            }
            body.push_str(&format!("{number}: {line}\n"));
            used += cost;
            last = number;
        }
        let mut out = format!(
            "{}: lines {start}-{last} of {total} (etag {version})\n{body}",
            path.display()
        );
        if last < end {
            out.push_str(&format!(
                "[cut at {max_chars} characters: resume with start_line={}]\n",
                last + 1
            ));
        }
        Ok(out)
    }

    fn list_folder(&self, arguments: &Value) -> Outcome {
        let input = text_argument(arguments, "path")?;
        let page = Page::of(arguments, MAX_LIST_ENTRIES, MAX_LIST_ENTRIES)?;
        let relative = self.paths.clean(input, true)?;
        self.paths.check_inside(&relative)?;
        let tree = self.vault.tree();
        let parent: Option<u32> = if relative.as_os_str().is_empty() {
            None
        } else {
            let index = tree
                .nodes
                .iter()
                .position(|n| n.kind == NodeKind::Folder && *n.path == *relative)
                .ok_or_else(|| format!("folder not found: {input}"))?;
            Some(index as u32)
        };
        let (from, to) = match parent {
            None => (0, tree.nodes.len()),
            Some(index) => (
                index as usize + 1,
                tree.nodes[index as usize].subtree_end as usize,
            ),
        };
        let mut lines: Vec<String> = Vec::new();
        for node in &tree.nodes[from..to] {
            if node.parent != parent {
                continue;
            }
            match node.kind {
                NodeKind::Folder => lines.push(format!(
                    "{}/ ({} note(s))",
                    node.path.display(),
                    node.note_count
                )),
                NodeKind::Note if node.note.is_some_and(|id| !self.shown(id)) => {}
                NodeKind::Note => {
                    let title = node
                        .note
                        .and_then(|id| self.vault.note(id))
                        .map(|m| m.title.to_string())
                        .unwrap_or_default();
                    lines.push(format!("{}: {title}", node.path.display()));
                }
                NodeKind::Attachment => lines.push(format!("{} (attachment)", node.path.display())),
                NodeKind::Other => {}
            }
        }
        if lines.is_empty() {
            return Ok(format!(
                "{}: empty folder",
                if input.trim().is_empty() {
                    "racine"
                } else {
                    input
                }
            ));
        }
        let (shown, remainder) = page.cut(
            lines,
            "entries",
            "call list_folder on a subfolder, or search_notes with path:<folder>",
        );
        let mut out = shown.join("\n");
        out.push('\n');
        out.push_str(&remainder);
        Ok(out)
    }

    fn get_backlinks(&self, arguments: &Value) -> Outcome {
        let (id, path) = self.locate(text_argument(arguments, "path")?)?;
        let backlinks: Vec<_> = self
            .vault
            .backlinks(id)
            .into_iter()
            .filter(|link| self.shown(link.source))
            .collect();
        if backlinks.is_empty() {
            return Ok(format!("No note links to {}", path.display()));
        }
        let page = Page::of(arguments, MAX_LINK_ENTRIES, MAX_LIST_ENTRIES)?;
        let total = backlinks.len();
        let (shown, remainder) = page.cut(backlinks, "links", "");
        let mut out = format!("{total} note(s) link to {}\n", path.display());
        for link in &shown {
            out.push_str(&format!(
                "{}: {}: {}\n",
                self.path_of(link.source),
                link.line + 1,
                self.displayed(link.source, link.context.trim())
            ));
        }
        out.push_str(&remainder);
        Ok(out)
    }

    fn get_outline(&self, arguments: &Value) -> Outcome {
        let (id, path) = self.locate(text_argument(arguments, "path")?)?;
        let meta = self
            .vault
            .note(id)
            .ok_or_else(|| format!("note not found: {}", path.display()))?;
        if meta.headings.is_empty() {
            return Ok(format!("{}: no headings", path.display()));
        }
        let mut out = format!("{}: {} heading(s)\n", path.display(), meta.headings.len());
        for heading in &meta.headings {
            out.push_str(&format!(
                "{}: {} {}\n",
                heading.line + 1,
                "#".repeat(usize::from(heading.level)),
                heading.text
            ));
        }
        Ok(out)
    }

    fn list_tags(&self, arguments: &Value) -> Outcome {
        let page = Page::of(arguments, 100, usize::MAX)?;
        let mut tags = self.vault.tags();
        if tags.is_empty() {
            return Ok("No tags in the vault".into());
        }
        tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
        let (shown, remainder) = page.cut(
            tags,
            "tags",
            "search_notes with tag:#name lists the notes of one tag",
        );
        let mut out = String::new();
        for tag in &shown {
            out.push_str(&format!("#{} ({})\n", tag.tag, tag.count));
        }
        out.push_str(&remainder);
        Ok(out)
    }

    fn get_neighbors(&self, arguments: &Value) -> Outcome {
        let (id, path) = self.locate(text_argument(arguments, "path")?)?;
        let depth = optional_number(arguments, "depth")?.unwrap_or(1);
        if !(1..=3).contains(&depth) {
            return Err("parameter `depth`: 1, 2 or 3".into());
        }
        if depth > 1 {
            return self.neighbors_deep(id, &path, depth);
        }
        let meta = self
            .vault
            .note(id)
            .ok_or_else(|| format!("note not found: {}", path.display()))?;
        let hidden: Vec<(u32, String, crate::hidden_links::Place)> = self
            .vault
            .read(id)
            .map(|text| crate::hidden_links::hidden_links(&text))
            .unwrap_or_default();
        let place_of = |line: u32, target: &str| {
            hidden
                .iter()
                .find(|(l, t, _)| *l == line && t == target)
                .map(|(_, _, p)| p.label())
        };
        let mut entries: Vec<(u32, String, Option<&'static str>)> = meta
            .links
            .iter()
            .map(|l| (l.line, l.target.clone(), place_of(l.line, &l.target)))
            .collect();
        entries.extend(
            hidden
                .iter()
                .filter(|(line, target, _)| {
                    !meta
                        .links
                        .iter()
                        .any(|l| l.line == *line && l.target == *target)
                })
                .map(|(line, target, place)| (*line, target.clone(), Some(place.label()))),
        );
        entries.sort_by_key(|(line, _, _)| *line);
        let mut out = format!("{}\nOutgoing links ({}):\n", path.display(), entries.len());
        for (line, target, place) in entries.iter().take(MAX_LINK_ENTRIES) {
            let resolved = match self.vault.resolve_link(id, target) {
                LinkTarget::Note { id, anchor } => {
                    let anchor = anchor.map(|a| format!("#{a}")).unwrap_or_default();
                    format!("{}{anchor}", self.path_of(id))
                }
                LinkTarget::Attachment(p) => format!("{} (attachment)", p.display()),
                LinkTarget::External(url) => format!("{url} (external)"),
                LinkTarget::Unresolved => "unresolved".to_string(),
            };
            let origin = place.map(|p| format!(" ({p})")).unwrap_or_default();
            out.push_str(&format!("  {}: {target}{origin}: {resolved}\n", line + 1));
        }
        if entries.len() > MAX_LINK_ENTRIES {
            out.push_str(&format!(
                "  [{} more links not shown: read_note lists every outgoing link of the note]\n",
                entries.len() - MAX_LINK_ENTRIES
            ));
        }
        let backlinks: Vec<_> = self
            .vault
            .backlinks(id)
            .into_iter()
            .filter(|link| self.shown(link.source))
            .collect();
        out.push_str(&format!("Incoming links ({}):\n", backlinks.len()));
        for link in backlinks.iter().take(MAX_LINK_ENTRIES) {
            out.push_str(&format!(
                "  {}: {}: {}\n",
                self.path_of(link.source),
                link.line + 1,
                self.displayed(link.source, link.context.trim())
            ));
        }
        if backlinks.len() > MAX_LINK_ENTRIES {
            out.push_str(&format!(
                "  [{} more links not shown: get_backlinks pages through them with offset]\n",
                backlinks.len() - MAX_LINK_ENTRIES
            ));
        }
        Ok(out)
    }

    pub(crate) fn authorize(
        &self,
        tool: &str,
        path: &Path,
        before: Option<&str>,
        after: &str,
        cancel: &Cancel,
    ) -> Result<(), Refusal> {
        match &self.writes {
            Writes::Refused => Err(Refusal::Error(
                "writes are disabled: this server is read-only".into(),
            )),
            Writes::Unconfirmed => Ok(()),
            Writes::Confirmed(confirm) => {
                if confirm(
                    &WriteProposal {
                        tool,
                        path,
                        before,
                        after,
                    },
                    cancel,
                ) {
                    Ok(())
                } else {
                    Err(Refusal::Denied)
                }
            }
        }
    }

    fn write(
        &self,
        tool: &str,
        arguments: &Value,
        options: &WriteOptions,
        cancel: &Cancel,
    ) -> Result<String, Refusal> {
        if !self.writes_enabled() {
            return Err(Fault::read_only().into());
        }
        match tool {
            "create_note" => {
                let content = text_argument(arguments, "content")?;
                if content.len() > MAX_WRITE_BYTES {
                    return Err(Refusal::Error(format!(
                        "content too long: {} bytes, at most {MAX_WRITE_BYTES}",
                        content.len()
                    )));
                }
                let mut relative = self.paths.clean(text_argument(arguments, "path")?, false)?;
                if relative
                    .extension()
                    .is_none_or(|e| !e.eq_ignore_ascii_case("md"))
                {
                    relative = PathBuf::from(format!("{}.md", relative.display()));
                }
                self.paths.check_writable(&relative)?;
                if self.vault.note_by_path(&relative).is_some()
                    || std::fs::symlink_metadata(self.vault.absolute(&relative)).is_ok()
                {
                    return Err(Refusal::Error(format!(
                        "already exists: {}. Use append_to_note or replace_in_note.",
                        relative.display()
                    )));
                }
                if options.dry_run {
                    return Ok(dry_run_text(
                        &format!("create_note would create {}", relative.display()),
                        &format!(
                            "{} line(s), {} byte(s)\n",
                            content.lines().count(),
                            content.len()
                        ),
                    ));
                }
                self.authorize(tool, &relative, None, content, cancel)?;
                let id = self
                    .vault
                    .create_note(&relative, content)
                    .map_err(|e| Refusal::Error(describe(e)))?;
                Ok(format!(
                    "Note created: {} ({} line(s)){}",
                    relative.display(),
                    content.lines().count(),
                    self.etag_line(id)
                ))
            }
            "append_to_note" => {
                let addition = text_argument(arguments, "content")?;
                if addition.len() > MAX_WRITE_BYTES {
                    return Err(Refusal::Error(format!(
                        "content too long: {} bytes, at most {MAX_WRITE_BYTES}",
                        addition.len()
                    )));
                }
                let (id, relative) = self.locate(text_argument(arguments, "path")?)?;
                self.paths.check_writable(&relative)?;
                let (before, modified) = self.read_with_stamp(id, &relative)?;
                options.check(Some(&before), "path", &reread_hint(&relative))?;
                let mut after = before.clone();
                if !after.is_empty() && !after.ends_with('\n') {
                    after.push('\n');
                }
                after.push_str(addition);
                if !after.ends_with('\n') {
                    after.push('\n');
                }
                let done = format!(
                    "Appended to {}: {} line(s)",
                    relative.display(),
                    addition.lines().count()
                );
                self.change_note(
                    tool,
                    (id, relative.as_path()),
                    (before.as_str(), after, modified),
                    options,
                    cancel,
                    done,
                )
            }
            "replace_in_note" => {
                let old = text_argument(arguments, "old_text")?;
                let new = text_argument(arguments, "new_text")?;
                if old.is_empty() {
                    return Err(Refusal::Error("`old_text` is empty".into()));
                }
                if new.len() > MAX_WRITE_BYTES {
                    return Err(Refusal::Error(format!(
                        "content too long: {} bytes, at most {MAX_WRITE_BYTES}",
                        new.len()
                    )));
                }
                let (id, relative) = self.locate(text_argument(arguments, "path")?)?;
                self.paths.check_writable(&relative)?;
                let (before, modified) = self.read_with_stamp(id, &relative)?;
                options.check(Some(&before), "path", &reread_hint(&relative))?;
                let crlf = before.contains("\r\n");
                let adapt = |s: &str| {
                    if crlf && !s.contains('\r') {
                        s.replace('\n', "\r\n")
                    } else {
                        s.to_string()
                    }
                };
                let (old, new) = (adapt(old), adapt(new));
                let count = before.matches(old.as_str()).count();
                match count {
                    0 => {
                        return Err(Refusal::Error(format!(
                            "text not found in {}: the comparison is exact (whitespace, line endings and case included)",
                            relative.display()
                        )));
                    }
                    1 => {}
                    n => {
                        let lines: Vec<String> = before
                            .match_indices(old.as_str())
                            .take(10)
                            .map(|(at, _)| (before[..at].matches('\n').count() + 1).to_string())
                            .collect();
                        return Err(Refusal::Error(format!(
                            "text appears {n} times in {} (lines {}): add context to make it unique",
                            relative.display(),
                            lines.join(", ")
                        )));
                    }
                }
                let at = before.find(old.as_str()).unwrap_or(0);
                let line = before[..at].matches('\n').count() + 1;
                let after = before.replacen(old.as_str(), &new, 1);
                let done = format!("Replacement done in {}, at line {line}", relative.display());
                self.change_note(
                    tool,
                    (id, relative.as_path()),
                    (before.as_str(), after, modified),
                    options,
                    cancel,
                    done,
                )
            }
            other => Err(Refusal::Error(format!("unknown tool: {other}"))),
        }
    }

    pub(crate) fn change_note(
        &self,
        tool: &str,
        (id, relative): (focus_core::NoteId, &Path),
        (before, after, modified): (&str, String, SystemTime),
        options: &WriteOptions,
        cancel: &Cancel,
        done: String,
    ) -> Result<String, Refusal> {
        if options.dry_run {
            let show = self.line_filter(relative);
            return Ok(dry_run_text(
                &format!("{tool} would change {}", relative.display()),
                &format!(
                    "{}Etag now: {}\n",
                    rules::changed_lines(before, &after, &*show),
                    etag(before)
                ),
            ));
        }
        self.authorize(tool, relative, Some(before), &after, cancel)?;
        self.vault
            .write(id, &after, Some(modified))
            .map_err(|e| Refusal::Error(describe(e)))?;
        Ok(format!("{done}{}", self.etag_line(id)))
    }

    pub(crate) fn etag_line(&self, id: focus_core::NoteId) -> String {
        self.vault
            .read(id)
            .map(|content| format!("\nEtag: {}", etag(&content)))
            .unwrap_or_default()
    }

    pub(crate) fn read_with_stamp(
        &self,
        id: focus_core::NoteId,
        relative: &Path,
    ) -> Result<(String, SystemTime), String> {
        let absolute = self.vault.absolute(relative);
        let modified = std::fs::metadata(&absolute)
            .and_then(|m| m.modified())
            .map_err(|e| format!("{}: {e}", relative.display()))?;
        let content = self.vault.read(id).map_err(describe)?;
        Ok((content, modified))
    }
}

pub(crate) fn dry_run_text(summary: &str, details: &str) -> String {
    format!("Dry run: {summary}. Nothing was written.\n{details}")
}

pub(crate) fn reread_hint(relative: &Path) -> String {
    format!(
        "Call read_note with path `{}` to read the note again, check that the change still applies, then send the new Etag as `if_match`.",
        relative.display()
    )
}

fn search_fields(arguments: &Value) -> Result<(bool, bool), String> {
    let Some(fields) = arguments.get("fields").filter(|f| !f.is_null()) else {
        return Ok((true, true));
    };
    let list = fields
        .as_array()
        .ok_or("parameter `fields`: a list of `title` and `matches` expected")?;
    let (mut title, mut matches) = (false, false);
    for field in list {
        match field.as_str() {
            Some("title") => title = true,
            Some("matches") => matches = true,
            _ => {
                return Err("parameter `fields`: accepted values are `title` and `matches`".into());
            }
        }
    }
    Ok((title, matches))
}

fn describe_diagnostic(diagnostic: &focus_search::Diagnostic) -> String {
    use focus_search::DiagnosticKind::*;
    match &diagnostic.kind {
        UnclosedQuote => "unclosed quote".to_string(),
        InvalidRegex { message } => format!("invalid regular expression: {message}"),
        UnknownOperator { name } => format!("unknown operator `{name}:` (searched as text)"),
        MissingValue { operator } => format!("`{operator}:` needs a value"),
        UnclosedParenthesis => "unclosed parenthesis".to_string(),
        UnmatchedParenthesis => "unmatched parenthesis".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testvault;
    use std::sync::Mutex;

    type Proposal = (String, String, Option<String>, String);

    fn tools(name: &str) -> (VaultTools, testvault::Fixture) {
        let fixture = testvault::open(name);
        (
            VaultTools::new(fixture.vault.clone(), fixture.index.clone()),
            fixture,
        )
    }

    fn run(tools: &VaultTools, name: &str, arguments: Value) -> ToolOutcome {
        tools.call(
            &ToolCall {
                id: "t".into(),
                name: name.into(),
                arguments: arguments.to_string(),
            },
            &Cancel::new(),
        )
    }

    fn ok(outcome: ToolOutcome) -> String {
        match outcome {
            ToolOutcome::Ok(text) => text,
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    fn error(outcome: ToolOutcome) -> String {
        match outcome {
            ToolOutcome::Error(text) => text,
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn search_returns_paths_lines_and_extracts() {
        let (tools, _f) = tools("search");
        let text = ok(run(
            &tools,
            "search_notes",
            serde_json::json!({ "query": "service accounts" }),
        ));
        assert!(text.starts_with("1 note(s), "), "{text}");
        assert!(
            text.contains("Red Team/Kerberoasting.md: Kerberoasting (2 occurrence(s))"),
            "{text}"
        );
        assert!(
            text.contains("\n  6: An attack on service accounts."),
            "{text}"
        );
        assert!(
            ok(run(
                &tools,
                "search_notes",
                serde_json::json!({ "query": "not found-xyz" })
            ))
            .starts_with("No note matches")
        );
        assert!(
            ok(run(
                &tools,
                "search_notes",
                serde_json::json!({ "query": "tag:#red/ad" })
            ))
            .contains("Kerberoasting.md")
        );
        assert!(
            error(run(
                &tools,
                "search_notes",
                serde_json::json!({ "query": "/(/" })
            ))
            .contains("invalid query")
        );
        assert!(
            error(run(
                &tools,
                "search_notes",
                serde_json::json!({ "query": "  " })
            ))
            .contains("empty")
        );
        assert!(error(run(&tools, "search_notes", serde_json::json!({}))).contains("`query`"));
    }

    #[test]
    fn read_note_ranges_bounds_and_suggestions() {
        let (tools, _f) = tools("read");
        let full = ok(run(
            &tools,
            "read_note",
            serde_json::json!({ "path": "Red Team/Kerberoasting" }),
        ));
        assert!(
            full.starts_with("Red Team/Kerberoasting.md: lines 1-"),
            "{full}"
        );
        assert!(full.contains("\n4: # Kerberoasting\n"), "{full}");
        let part = ok(run(
            &tools,
            "read_note",
            serde_json::json!({ "path": "Red Team/Kerberoasting.md", "start_line": 5, "end_line": 7 }),
        ));
        assert!(
            part.starts_with("Red Team/Kerberoasting.md: lines 5-7 of "),
            "{part}"
        );
        assert_eq!(part.lines().count(), 4);
        let cut = ok(run(
            &tools,
            "read_note",
            serde_json::json!({ "path": "Red Team/Kerberoasting.md", "max_chars": 40 }),
        ));
        assert!(
            cut.contains("[cut at 40 characters: resume with start_line="),
            "{cut}"
        );
        assert!(
            error(run(
                &tools,
                "read_note",
                serde_json::json!({ "path": "Red Team/Kerberoasting.md", "start_line": 900 })
            ))
            .contains("has only")
        );
        assert!(error(run(&tools, "read_note", serde_json::json!({ "path": "Red Team/Kerberoasting.md", "start_line": 7, "end_line": 5 }))).contains("invalid range"));
        let missing = error(run(
            &tools,
            "read_note",
            serde_json::json!({ "path": "Kerberoastin" }),
        ));
        assert!(
            missing.contains("note not found")
                && missing.contains("Similar notes: Red Team/Kerberoasting.md"),
            "{missing}"
        );
    }

    #[test]
    fn neighbors_include_links_written_in_properties_and_diagrams() {
        let base = testvault::scratch("hidden-links");
        let root = base.join("vault");
        std::fs::create_dir_all(&root).unwrap();
        testvault::populate(&root);
        testvault::write(
            &root,
            "Plan.md",
            "---\nrelated: \"[[Home]]\"\n---\n# Plan\n\nBody link [[Notes/Bridge]].\n\n```mermaid\ngraph TD\n  A --> B\n  click B \"Red Team/Defense\"\n```\n",
        );
        let fixture = testvault::open_at(&root, &base.join("cache"));
        let tools = VaultTools::new(fixture.vault.clone(), fixture.index.clone());
        let out = ok(run(
            &tools,
            "get_neighbors",
            serde_json::json!({ "path": "Plan.md" }),
        ));
        assert!(out.contains("Outgoing links (3)"), "{out}");
        assert!(
            out.contains("Home (property): Home.md")
                && out.contains("Red Team/Defense (diagram): Red Team/Defense.md"),
            "{out}"
        );
    }

    #[test]
    fn query_problems_are_described_in_english() {
        let (tools, _f) = tools("diagnostics");
        let error = error(run(
            &tools,
            "search_notes",
            serde_json::json!({ "query": "\"unclosed" }),
        ));
        assert!(
            error.contains("invalid query") && error.contains("unclosed quote"),
            "{error}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn paths_outside_the_vault_are_refused_by_every_tool() {
        let (tools, f) = tools("outside");
        let outside = f.root.parent().unwrap().join("outside.md");
        std::fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, f.root.join("link.md")).unwrap();
        for bad in [
            "../outside.md",
            "/etc/passwd",
            "Red Team/../../outside.md",
            "~/x.md",
        ] {
            for tool in ["read_note", "get_backlinks", "get_outline", "get_neighbors"] {
                let message = error(run(&tools, tool, serde_json::json!({ "path": bad })));
                assert!(
                    message.contains("outside the vault"),
                    "{tool} {bad}: {message}"
                );
            }
            assert!(
                error(run(
                    &tools,
                    "list_folder",
                    serde_json::json!({ "path": bad })
                ))
                .contains("outside the vault"),
                "{bad}"
            );
        }
        let linked = error(run(
            &tools,
            "read_note",
            serde_json::json!({ "path": "link.md" }),
        ));
        assert!(
            linked.contains("outside the vault") || linked.contains("not found"),
            "{linked}"
        );
    }

    #[test]
    fn list_folder_outline_backlinks_tags_and_neighbors() {
        let (tools, _f) = tools("lists");
        let root = ok(run(
            &tools,
            "list_folder",
            serde_json::json!({ "path": "" }),
        ));
        assert!(
            root.contains("Red Team/ (2 note(s))") && root.contains("Home.md: Home"),
            "{root}"
        );
        let sub = ok(run(
            &tools,
            "list_folder",
            serde_json::json!({ "path": "Red Team" }),
        ));
        assert!(
            sub.contains("Red Team/Defense.md: Defense")
                && sub.contains("Red Team/assets/ (0 note(s))"),
            "{sub}"
        );
        assert!(
            error(run(
                &tools,
                "list_folder",
                serde_json::json!({ "path": "Absent" })
            ))
            .contains("folder not found")
        );

        let outline = ok(run(
            &tools,
            "get_outline",
            serde_json::json!({ "path": "Red Team/Kerberoasting.md" }),
        ));
        assert!(
            outline.contains("4: # Kerberoasting") && outline.contains("8: ## Detection"),
            "{outline}"
        );

        let backlinks = ok(run(
            &tools,
            "get_backlinks",
            serde_json::json!({ "path": "Red Team/Kerberoasting.md" }),
        ));
        assert!(
            backlinks.contains("Home.md: 3:") && backlinks.contains("Red Team/Defense.md: 3:"),
            "{backlinks}"
        );
        assert!(
            ok(run(
                &tools,
                "get_backlinks",
                serde_json::json!({ "path": "Notes/Isolated note.md" })
            ))
            .contains("Home.md"),
            "the home note points to the isolated note"
        );

        let tags = ok(run(&tools, "list_tags", serde_json::json!({})));
        assert!(
            tags.contains("#red (1)")
                && tags.contains("#red/ad (1)")
                && tags.contains("#blue/ad (1)"),
            "{tags}"
        );
        let limited = ok(run(&tools, "list_tags", serde_json::json!({ "limit": 1 })));
        assert!(limited.contains("more tags not shown"));

        let neighbors = ok(run(
            &tools,
            "get_neighbors",
            serde_json::json!({ "path": "Red Team/Kerberoasting.md" }),
        ));
        assert!(
            neighbors.contains("Outgoing links (3)")
                && neighbors.contains("Defense: Red Team/Defense.md")
                && neighbors.contains("Missing: unresolved"),
            "{neighbors}"
        );
        assert!(
            neighbors.contains("capture.png") && neighbors.contains("Incoming links (2)"),
            "{neighbors}"
        );
    }

    #[test]
    fn writes_are_refused_without_a_callback_and_not_listed() {
        let (tools, f) = tools("readonly");
        assert!(!tools.specs().iter().any(|s| agent::is_write_tool(&s.name)));
        for (tool, args) in [
            (
                "create_note",
                serde_json::json!({ "path": "Nouvelle.md", "content": "x" }),
            ),
            (
                "append_to_note",
                serde_json::json!({ "path": "Home.md", "content": "x" }),
            ),
            (
                "replace_in_note",
                serde_json::json!({ "path": "Home.md", "old_text": "Welcome", "new_text": "Salut" }),
            ),
        ] {
            assert!(
                error(run(&tools, tool, args)).contains("read-only"),
                "{tool}"
            );
        }
        assert!(!f.root.join("Nouvelle.md").exists());
    }

    #[test]
    fn confirmed_writes_show_before_and_after_and_denial_changes_nothing() {
        let fixture = testvault::open("confirmed");
        let seen: Arc<Mutex<Vec<Proposal>>> = Arc::new(Mutex::new(Vec::new()));
        let answer = Arc::new(Mutex::new(true));
        let (log, decision) = (seen.clone(), answer.clone());
        let tools = VaultTools::new(fixture.vault.clone(), fixture.index.clone())
            .with_confirmation(move |p, _| {
                log.lock().unwrap().push((
                    p.tool.to_string(),
                    p.path.display().to_string(),
                    p.before.map(str::to_string),
                    p.after.to_string(),
                ));
                *decision.lock().unwrap()
            });
        assert!(tools.specs().iter().any(|s| s.name == "create_note"));

        assert!(
            ok(run(
                &tools,
                "create_note",
                serde_json::json!({ "path": "New/Folder/Idea", "content": "# Idea\n\nText.\n" })
            ))
            .starts_with("Note created: New/Folder/Idea.md")
        );
        assert_eq!(
            std::fs::read_to_string(fixture.root.join("New/Folder/Idea.md")).unwrap(),
            "# Idea\n\nText.\n"
        );
        assert!(
            error(run(
                &tools,
                "create_note",
                serde_json::json!({ "path": "Home.md", "content": "x" })
            ))
            .contains("already exists")
        );

        assert!(
            ok(run(
                &tools,
                "append_to_note",
                serde_json::json!({ "path": "Home", "content": "Final addition" })
            ))
            .starts_with("Appended to Home.md")
        );
        assert!(
            std::fs::read_to_string(fixture.root.join("Home.md"))
                .unwrap()
                .ends_with("- [x] create the vault\nFinal addition\n")
        );

        assert!(ok(run(&tools, "replace_in_note", serde_json::json!({ "path": "Home.md", "old_text": "Welcome.", "new_text": "Hello." }))).contains("at line 3"));
        let log = seen.lock().unwrap().clone();
        let replace = log.iter().find(|e| e.0 == "replace_in_note").unwrap();
        assert!(
            replace.2.as_deref().unwrap().contains("Welcome.")
                && replace.3.contains("Hello.")
                && !replace.3.contains("Welcome.")
        );
        assert_eq!(log[0].2, None, "a creation has no content before");

        *answer.lock().unwrap() = false;
        let before = std::fs::read_to_string(fixture.root.join("Home.md")).unwrap();
        assert!(matches!(
            run(
                &tools,
                "append_to_note",
                serde_json::json!({ "path": "Home.md", "content": "denied" })
            ),
            ToolOutcome::Denied
        ));
        assert!(matches!(
            run(
                &tools,
                "create_note",
                serde_json::json!({ "path": "Denied.md", "content": "x" })
            ),
            ToolOutcome::Denied
        ));
        assert_eq!(
            std::fs::read_to_string(fixture.root.join("Home.md")).unwrap(),
            before
        );
        assert!(!fixture.root.join("Denied.md").exists());
    }

    #[test]
    fn replace_requires_exactly_one_exact_occurrence() {
        let fixture = testvault::open("replace");
        testvault::write(&fixture.root, "Duplicates.md", "a word\na word\nother\n");
        testvault::write(&fixture.root, "Windows.md", "ligne a\r\nligne b\r\n");
        let tools =
            VaultTools::new(fixture.vault.clone(), fixture.index.clone()).with_unconfirmed_writes();
        ok(run(
            &tools,
            "create_note",
            serde_json::json!({ "path": "Duplicates2.md", "content": "a word\na word\nother\n" }),
        ));
        let twice = error(run(
            &tools,
            "replace_in_note",
            serde_json::json!({ "path": "Duplicates2.md", "old_text": "a word", "new_text": "x" }),
        ));
        assert!(
            twice.contains("2 times") && twice.contains("lines 1, 2"),
            "{twice}"
        );
        let none = error(run(
            &tools,
            "replace_in_note",
            serde_json::json!({ "path": "Duplicates2.md", "old_text": "A WORD", "new_text": "x" }),
        ));
        assert!(
            none.contains("not found") && none.contains("case"),
            "{none}"
        );
        assert!(
            error(run(
                &tools,
                "replace_in_note",
                serde_json::json!({ "path": "Duplicates2.md", "old_text": "", "new_text": "x" })
            ))
            .contains("empty")
        );
        ok(run(
            &tools,
            "replace_in_note",
            serde_json::json!({ "path": "Duplicates2.md", "old_text": "a word\na word", "new_text": "two" }),
        ));
        assert_eq!(
            std::fs::read_to_string(fixture.root.join("Duplicates2.md")).unwrap(),
            "two\nother\n"
        );
        let crlf = tools_crlf(&tools, &fixture);
        assert_eq!(crlf, "ligne a\r\nligne B\r\n");
    }

    fn tools_crlf(tools: &VaultTools, fixture: &testvault::Fixture) -> String {
        ok(run(
            tools,
            "create_note",
            serde_json::json!({ "path": "Crlf.md", "content": "ligne a\r\nligne b\r\n" }),
        ));
        ok(run(
            tools,
            "replace_in_note",
            serde_json::json!({ "path": "Crlf.md", "old_text": "ligne b\n", "new_text": "ligne B\n" }),
        ));
        std::fs::read_to_string(fixture.root.join("Crlf.md")).unwrap()
    }

    #[test]
    #[cfg(unix)]
    fn writes_cannot_escape_or_touch_reserved_folders() {
        let fixture = testvault::open("write-escape");
        let tools =
            VaultTools::new(fixture.vault.clone(), fixture.index.clone()).with_unconfirmed_writes();
        let outside = fixture.root.parent().unwrap().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, fixture.root.join("link-folder")).unwrap();
        for path in [
            "../escape.md",
            "/tmp/escape.md",
            "link-folder/escape.md",
            ".obsidian/escape.md",
            ".git/escape.md",
        ] {
            let message = error(run(
                &tools,
                "create_note",
                serde_json::json!({ "path": path, "content": "x" }),
            ));
            assert!(
                message.contains("outside the vault") || message.contains("reserved"),
                "{path}: {message}"
            );
        }
        assert!(
            !outside.join("escape.md").exists()
                && !fixture.root.parent().unwrap().join("escape.md").exists()
        );
    }

    #[test]
    fn big_outputs_are_truncated_and_unknown_tools_and_bad_arguments_are_errors() {
        let fixture = testvault::open("truncate");
        let tools =
            VaultTools::new(fixture.vault.clone(), fixture.index.clone()).with_unconfirmed_writes();
        let big: String = (0..3000)
            .map(|i| format!("line number {i} with enough text to fill\n"))
            .collect();
        ok(run(
            &tools,
            "create_note",
            serde_json::json!({ "path": "Large.md", "content": big }),
        ));
        let text = ok(run(
            &tools,
            "read_note",
            serde_json::json!({ "path": "Large.md", "max_chars": 20000 }),
        ));
        assert!(
            text.len() <= agent::MAX_TOOL_RESULT_BYTES + 200,
            "{}",
            text.len()
        );
        assert!(error(run(&tools, "inexistant", serde_json::json!({}))).contains("unknown tool"));
        let outcome = tools.call(
            &ToolCall {
                id: "t".into(),
                name: "read_note".into(),
                arguments: "not json".into(),
            },
            &Cancel::new(),
        );
        assert!(error(outcome).contains("invalid JSON"));
        assert!(
            error(run(
                &tools,
                "read_note",
                serde_json::json!({ "path": "Home.md", "start_line": "a" })
            ))
            .contains("integer")
        );
    }
}
