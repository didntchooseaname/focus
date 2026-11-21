use std::path::{Component, Path, PathBuf};

use focus_core::{LinkTarget, NoteId, Vault, VaultError};
use focus_kit::{ActiveTokens, Button, ButtonKind, Flow as _, Type, size};
use gpui::{
    AnyElement, AppContext as _, Context, InteractiveElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use crate::shell::{Overlay, Shell};

pub const MAX_ADDRESS_BYTES: usize = 64 * 1024;

pub fn is_obsidian(target: &str) -> bool {
    target
        .get(..9)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("obsidian:"))
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Vault,
    File {
        file: String,
        anchor: Option<String>,
    },
    Path {
        path: PathBuf,
        anchor: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteAction {
    New,
    Daily,
    Unique,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    Untitled,
    Name(String),
    File(String),
    Path(PathBuf),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Text(String),
    Clipboard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Create,
    Append,
    Prepend,
    Overwrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spacing {
    Auto,
    Newline,
    Inline,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WriteRequest {
    pub action: WriteAction,
    pub vault: Option<String>,
    pub place: Place,
    pub content: Option<Content>,
    pub mode: Mode,
    pub heading: Option<String>,
    pub spacing: Spacing,
    pub silent: bool,
    pub new_tab: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Address {
    Open {
        vault: Option<String>,
        target: Target,
        new_tab: bool,
    },
    Search {
        vault: Option<String>,
        query: String,
    },
    Write(WriteRequest),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    Refused(String),
    Unknown(String),
    Malformed,
    NoVault,
    OtherVault(String),
    Outside(String),
    NotFound(String),
    HeadingMissing { path: PathBuf, heading: String },
    Changed(PathBuf),
    Vault(String),
}

impl Refusal {
    pub fn message(&self, open: &str) -> String {
        use focus_i18n::{t, t_args};
        match self {
            Refusal::Refused(name) => t_args("ui-obsidian-refused", &[("name", name.into())]),
            Refusal::Unknown(action) => t_args("ui-obsidian-unknown", &[("action", action.into())]),
            Refusal::Malformed => t("ui-obsidian-malformed"),
            Refusal::NoVault => t("ui-obsidian-no-vault"),
            Refusal::OtherVault(vault) => t_args(
                "ui-obsidian-other-vault",
                &[("vault", vault.into()), ("open", open.into())],
            ),
            Refusal::Outside(path) => t_args(
                "ui-obsidian-outside",
                &[("path", path.into()), ("open", open.into())],
            ),
            Refusal::NotFound(target) => t_args(
                "ui-obsidian-not-found",
                &[("target", target.into()), ("open", open.into())],
            ),
            Refusal::HeadingMissing { path, heading } => t_args(
                "ui-obsidian-heading-missing",
                &[
                    ("path", path.display().to_string().as_str().into()),
                    ("heading", heading.into()),
                ],
            ),
            Refusal::Changed(path) => t_args(
                "ui-obsidian-changed",
                &[("path", path.display().to_string().as_str().into())],
            ),
            Refusal::Vault(text) => text.clone(),
        }
    }
}

fn vault_refusal(error: VaultError) -> Refusal {
    Refusal::Vault(crate::errors::vault(&error))
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((high * 16 + low) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn split_anchor(file: &str) -> (String, Option<String>) {
    match file.split_once('#') {
        Some((file, anchor)) if !anchor.is_empty() => (file.to_string(), Some(anchor.to_string())),
        Some((file, _)) => (file.to_string(), None),
        None => (file.to_string(), None),
    }
}

fn params(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (decode(key), decode(value)),
            None => (decode(pair), String::new()),
        })
        .collect()
}

fn flag(params: &[(String, String)], name: &str) -> bool {
    params
        .iter()
        .any(|(key, value)| key == name && !value.eq_ignore_ascii_case("false"))
}

fn param(params: &[(String, String)], name: &str) -> Option<String> {
    params
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}

pub fn parse(address: &str) -> Result<Address, Refusal> {
    if address.len() > MAX_ADDRESS_BYTES || !is_obsidian(address) {
        return Err(Refusal::Malformed);
    }
    let rest = address[9..].strip_prefix("//").ok_or(Refusal::Malformed)?;
    if rest.starts_with('/') {
        let (path, anchor) = split_anchor(&decode(rest.split('?').next().unwrap_or(rest)));
        return Ok(Address::Open {
            vault: None,
            target: Target::Path {
                path: PathBuf::from(path),
                anchor,
            },
            new_tab: false,
        });
    }
    let (head, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (action, tail) = head.split_once('/').unwrap_or((head, ""));
    let action = decode(action).to_ascii_lowercase();
    if action == "vault" {
        let tail = decode(tail);
        let (vault, file) = tail.split_once('/').unwrap_or((tail.as_str(), ""));
        if vault.is_empty() {
            return Err(Refusal::Malformed);
        }
        let target = if file.is_empty() {
            Target::Vault
        } else {
            let (file, anchor) = split_anchor(file);
            Target::File { file, anchor }
        };
        return Ok(Address::Open {
            vault: Some(vault.to_string()),
            target,
            new_tab: false,
        });
    }
    let params = params(query);
    for refused in ["x-success", "x-error"] {
        if params.iter().any(|(key, _)| key == refused) {
            return Err(Refusal::Refused(refused.to_string()));
        }
    }
    let vault = param(&params, "vault").filter(|v| !v.is_empty());
    let new_tab = param(&params, "paneType").is_some_and(|p| !p.is_empty());
    match action.as_str() {
        "choose-vault" | "hook-get-address" => Err(Refusal::Refused(action)),
        "open" => {
            let target = if let Some(path) = param(&params, "path").filter(|p| !p.is_empty()) {
                let (path, anchor) = split_anchor(&path);
                Target::Path {
                    path: PathBuf::from(path),
                    anchor,
                }
            } else if let Some(file) = param(&params, "file").filter(|f| !f.is_empty()) {
                let (file, anchor) = split_anchor(&file);
                Target::File { file, anchor }
            } else {
                Target::Vault
            };
            Ok(Address::Open {
                vault,
                target,
                new_tab,
            })
        }
        "search" => Ok(Address::Search {
            vault,
            query: param(&params, "query").unwrap_or_default(),
        }),
        "new" | "daily" | "unique" => {
            let place = if let Some(path) = param(&params, "path").filter(|p| !p.is_empty()) {
                Place::Path(PathBuf::from(path))
            } else if let Some(file) = param(&params, "file").filter(|f| !f.is_empty()) {
                Place::File(file)
            } else if let Some(name) = param(&params, "name").filter(|n| !n.is_empty()) {
                Place::Name(name)
            } else {
                Place::Untitled
            };
            let content = if flag(&params, "clipboard") {
                Some(Content::Clipboard)
            } else {
                param(&params, "content").map(Content::Text)
            };
            let mode = if flag(&params, "append") {
                Mode::Append
            } else if flag(&params, "prepend") {
                Mode::Prepend
            } else if flag(&params, "overwrite") {
                Mode::Overwrite
            } else {
                Mode::Create
            };
            let spacing = match param(&params, "spacing").as_deref() {
                Some("newline") => Spacing::Newline,
                Some("inline") => Spacing::Inline,
                _ => Spacing::Auto,
            };
            Ok(Address::Write(WriteRequest {
                action: match action.as_str() {
                    "new" => WriteAction::New,
                    "daily" => WriteAction::Daily,
                    _ => WriteAction::Unique,
                },
                vault,
                place,
                content,
                mode,
                heading: param(&params, "heading").filter(|h| !h.is_empty()),
                spacing,
                silent: flag(&params, "silent"),
                new_tab,
            }))
        }
        "" => Err(Refusal::Malformed),
        other => Err(Refusal::Unknown(other.to_string())),
    }
}

pub fn vault_name(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
}

fn check_vault(root: &Path, vault: Option<&str>) -> Result<(), Refusal> {
    match vault {
        Some(name) if name != vault_name(root) => Err(Refusal::OtherVault(name.to_string())),
        _ => Ok(()),
    }
}

fn relative(text: &str) -> Option<PathBuf> {
    let path = Path::new(text.trim_start_matches('/'));
    let clean = path.components().all(|c| matches!(c, Component::Normal(_)))
        && path.components().next().is_some();
    clean.then(|| path.to_path_buf())
}

fn inside(root: &Path, path: &Path) -> Result<PathBuf, Refusal> {
    let outside = || Refusal::Outside(path.display().to_string());
    if !path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
        return Err(outside());
    }
    path.strip_prefix(root)
        .ok()
        .and_then(|rest| relative(&rest.to_string_lossy()))
        .ok_or_else(outside)
}

fn find_note(vault: &Vault, file: &str) -> Result<NoteId, Refusal> {
    let path = relative(file).ok_or_else(|| Refusal::Outside(file.to_string()))?;
    if let Some(id) = vault.note_by_path(&path) {
        return Ok(id);
    }
    let with_md = PathBuf::from(format!("{}.md", path.display()));
    if let Some(id) = vault.note_by_path(&with_md) {
        return Ok(id);
    }
    match vault.resolve_link_from_root(file) {
        LinkTarget::Note { id, .. } => Ok(id),
        _ => Err(Refusal::NotFound(file.to_string())),
    }
}

#[derive(Debug, PartialEq)]
pub enum Reading {
    Nothing,
    Note {
        id: NoteId,
        anchor: Option<String>,
        new_tab: bool,
    },
    Search(String),
}

pub fn resolve_reading(vault: &Vault, address: &Address) -> Result<Reading, Refusal> {
    let root = vault.root();
    match address {
        Address::Search { vault: name, query } => {
            check_vault(root, name.as_deref())?;
            Ok(Reading::Search(query.clone()))
        }
        Address::Open {
            vault: name,
            target,
            new_tab,
        } => {
            let (id, anchor) = match target {
                Target::Vault => {
                    check_vault(root, name.as_deref())?;
                    return Ok(Reading::Nothing);
                }
                Target::Path { path, anchor } => {
                    let rel = inside(root, path)?;
                    let id = vault
                        .note_by_path(&rel)
                        .or_else(|| {
                            vault.note_by_path(&PathBuf::from(format!("{}.md", rel.display())))
                        })
                        .ok_or_else(|| Refusal::NotFound(path.display().to_string()))?;
                    (id, anchor.clone())
                }
                Target::File { file, anchor } => {
                    check_vault(root, name.as_deref())?;
                    (find_note(vault, file)?, anchor.clone())
                }
            };
            Ok(Reading::Note {
                id,
                anchor,
                new_tab: *new_tab,
            })
        }
        Address::Write(_) => Err(Refusal::Malformed),
    }
}

#[derive(Clone, Debug, Default)]
pub struct WriteSettings {
    pub new_note_folder: String,
    pub daily_folder: String,
    pub daily_format: String,
    pub daily_template: Option<NoteId>,
    pub untitled: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: String,
    pub added: String,
    pub mode: Mode,
    pub open: bool,
    pub new_tab: bool,
}

#[derive(Debug, PartialEq)]
pub enum Writing {
    OpenOrCreate {
        path: PathBuf,
        new_tab: bool,
        daily: bool,
    },
    Confirm(Plan),
}

fn separator(spacing: Spacing) -> &'static str {
    match spacing {
        Spacing::Auto => "\n\n",
        Spacing::Newline => "\n",
        Spacing::Inline => "",
    }
}

fn join(base: &str, text: &str, spacing: Spacing, prepend: bool) -> String {
    let separator = separator(spacing);
    if prepend {
        let base_trimmed = base.trim_start_matches(['\n', '\r']);
        if base_trimmed.is_empty() {
            return format!("{}\n", text.trim_end_matches('\n'));
        }
        format!("{}{separator}{base_trimmed}", text.trim_end_matches('\n'))
    } else {
        let base_trimmed = base.trim_end_matches(['\n', '\r']);
        if base_trimmed.is_empty() {
            return format!("{}\n", text.trim_end_matches('\n'));
        }
        format!("{base_trimmed}{separator}{}\n", text.trim_end_matches('\n'))
    }
}

fn section(text: &str, heading: &str) -> Option<std::ops::Range<usize>> {
    let mut offset = 0;
    let mut found: Option<(usize, usize)> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end();
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        let is_heading = (1..=6).contains(&level) && trimmed[level..].starts_with(' ');
        if is_heading {
            match found {
                Some((start, open)) if level <= open => return Some(start..offset),
                None if trimmed[level..].trim().eq_ignore_ascii_case(heading.trim()) => {
                    found = Some((offset + line.len(), level));
                }
                _ => {}
            }
        }
        offset += line.len();
    }
    found.map(|(start, _)| start..text.len())
}

fn place(
    before: &str,
    added: &str,
    mode: Mode,
    heading: Option<&str>,
    spacing: Spacing,
    path: &Path,
) -> Result<String, Refusal> {
    if mode == Mode::Overwrite {
        return Ok(added.to_string());
    }
    let prepend = mode == Mode::Prepend;
    let (merged, body) = focus_view_vault::merge_frontmatter(before, added);
    let body_start = merged
        .strip_prefix("---")
        .and_then(|_| {
            let (_, rest) = merged.split_once('\n')?;
            let close = rest
                .split_inclusive('\n')
                .scan(0, |offset, line| {
                    let start = *offset;
                    *offset += line.len();
                    Some((start, line))
                })
                .find(|(_, line)| matches!(line.trim_end(), "---" | "..."))?;
            Some(merged.len() - rest.len() + close.0 + close.1.len())
        })
        .unwrap_or(0);
    let (head, rest) = merged.split_at(body_start);
    let Some(heading) = heading else {
        return Ok(format!("{head}{}", join(rest, body, spacing, prepend)));
    };
    let range = section(rest, heading).ok_or_else(|| Refusal::HeadingMissing {
        path: path.to_path_buf(),
        heading: heading.to_string(),
    })?;
    let inner = &rest[range.clone()];
    let joined = join(inner, body, spacing, prepend);
    let tail = &rest[range.end..];
    let gap = if tail.is_empty() || joined.ends_with("\n\n") {
        ""
    } else {
        "\n"
    };
    Ok(format!("{head}{}{joined}{gap}{tail}", &rest[..range.start]))
}

fn with_md(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
    {
        path
    } else {
        PathBuf::from(format!("{}.md", path.display()))
    }
}

fn free_path(vault: &Vault, path: &Path) -> PathBuf {
    let stem = path.with_extension("");
    (0..)
        .map(|n| match n {
            0 => with_md(stem.clone()),
            n => PathBuf::from(format!("{} {n}.md", stem.display())),
        })
        .find(|candidate| {
            vault.note_by_path(candidate).is_none() && !vault.absolute(candidate).exists()
        })
        .unwrap_or_else(|| with_md(stem))
}

pub fn plan(
    vault: &Vault,
    request: &WriteRequest,
    settings: &WriteSettings,
    clipboard: Option<String>,
    date: focus_view_vault::Date,
    time: focus_view_vault::Time,
) -> Result<Writing, Refusal> {
    let root = vault.root();
    let content = match &request.content {
        Some(Content::Text(text)) => Some(text.clone()),
        Some(Content::Clipboard) => Some(clipboard.unwrap_or_default()),
        None => None,
    };
    let folder = Path::new(settings.new_note_folder.trim_matches('/'));
    let path = match request.action {
        WriteAction::Daily => {
            check_vault(root, request.vault.as_deref())?;
            let format = if settings.daily_format.trim().is_empty() {
                "YYYY-MM-DD"
            } else {
                settings.daily_format.as_str()
            };
            let name = focus_view_vault::format_date(format, date, time);
            Path::new(settings.daily_folder.trim_matches('/')).join(format!("{name}.md"))
        }
        WriteAction::Unique => {
            check_vault(root, request.vault.as_deref())?;
            let name = focus_view_vault::format_date("YYYYMMDDHHmm", date, time);
            free_path(vault, &folder.join(name))
        }
        WriteAction::New => match &request.place {
            Place::Path(path) => with_md(inside(root, path)?),
            Place::File(file) => {
                check_vault(root, request.vault.as_deref())?;
                with_md(relative(file).ok_or_else(|| Refusal::Outside(file.clone()))?)
            }
            Place::Name(name) => {
                check_vault(root, request.vault.as_deref())?;
                let name = relative(name).ok_or_else(|| Refusal::Outside(name.clone()))?;
                with_md(folder.join(name))
            }
            Place::Untitled => {
                check_vault(root, request.vault.as_deref())?;
                with_md(folder.join(&settings.untitled))
            }
        },
    };
    let open = !request.silent;
    let Some(added) = content else {
        if request.action != WriteAction::Unique {
            return Ok(Writing::OpenOrCreate {
                path,
                new_tab: request.new_tab,
                daily: request.action == WriteAction::Daily,
            });
        }
        return Ok(Writing::Confirm(Plan {
            path,
            before: None,
            after: String::new(),
            added: String::new(),
            mode: Mode::Create,
            open,
            new_tab: request.new_tab,
        }));
    };
    let existing = vault.note_by_path(&path);
    let before = match existing {
        Some(id) => Some(vault.read(id).map_err(vault_refusal)?),
        None => None,
    };
    let base = match (&before, request.action) {
        (None, WriteAction::Daily) => match settings.daily_template {
            Some(template) => {
                let title = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let text = vault.read(template).map_err(vault_refusal)?;
                let (filled, _) =
                    focus_view_vault::insert_template("", 0, &text, &title, date, time);
                Some(filled)
            }
            None => None,
        },
        _ => None,
    };
    let mode = match (request.mode, request.action, &before) {
        (Mode::Create, WriteAction::Daily, _) => Mode::Append,
        (mode, _, _) => mode,
    };
    let (path, before, mode) = match (&before, mode) {
        (Some(_), Mode::Create) => (free_path(vault, &path), None, Mode::Create),
        (Some(_), _) => (path, before, mode),
        (None, Mode::Overwrite) => (path, None, Mode::Create),
        (None, mode) => (path, None, mode),
    };
    let after = match (&before, &base) {
        (Some(text), _) => place(
            text,
            &added,
            mode,
            request.heading.as_deref(),
            request.spacing,
            &path,
        )?,
        (None, Some(base)) => place(
            base,
            &added,
            mode,
            request.heading.as_deref(),
            request.spacing,
            &path,
        )?,
        (None, None) => format!("{}\n", added.trim_end_matches('\n')),
    };
    Ok(Writing::Confirm(Plan {
        path,
        before,
        after,
        added,
        mode,
        open,
        new_tab: request.new_tab,
    }))
}

pub fn apply(vault: &Vault, plan: &Plan) -> Result<NoteId, Refusal> {
    match (&plan.before, vault.note_by_path(&plan.path)) {
        (Some(before), Some(id)) => {
            if vault.read(id).map_err(vault_refusal)? != *before {
                return Err(Refusal::Changed(plan.path.clone()));
            }
            vault.write(id, &plan.after, None).map_err(vault_refusal)?;
            Ok(id)
        }
        (None, None) => vault
            .create_note(&plan.path, &plan.after)
            .map_err(|error| match error {
                VaultError::AlreadyExists(_) => Refusal::Changed(plan.path.clone()),
                other => vault_refusal(other),
            }),
        _ => Err(Refusal::Changed(plan.path.clone())),
    }
}

#[derive(Default)]
pub struct Pending(pub Vec<String>);

#[derive(Clone, Debug)]
pub struct Confirmation {
    pub plan: Plan,
}

impl Shell {
    fn text_setting(&self, key: &str, cx: &gpui::App) -> String {
        self.prefs.read(cx).text(key)
    }

    pub fn daily_template(&self, cx: &gpui::App) -> Option<NoteId> {
        let library = self.library.as_ref()?;
        crate::capture::find_template(
            library.vault(),
            &self.text_setting("general.daily_template", cx),
        )
    }

    pub fn follow_obsidian(&mut self, address: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            if self.vault_root.is_some() {
                self.obsidian.0.push(address.to_string());
            } else {
                self.refuse_address(&Refusal::NoVault, cx);
            }
            return;
        };
        let parsed = match parse(address) {
            Ok(parsed) => parsed,
            Err(refusal) => return self.refuse_address(&refusal, cx),
        };
        let vault = library.vault().clone();
        match &parsed {
            Address::Write(request) => {
                let settings = WriteSettings {
                    new_note_folder: self.text_setting("general.new_note_folder", cx),
                    daily_folder: self.text_setting("general.daily_folder", cx),
                    daily_format: self.text_setting("general.daily_format", cx),
                    daily_template: self.daily_template(cx),
                    untitled: focus_i18n::t("ui-note-untitled"),
                };
                let clipboard = (request.content == Some(Content::Clipboard))
                    .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
                    .flatten();
                let (date, time) = crate::capture::now();
                match plan(&vault, request, &settings, clipboard, date, time) {
                    Ok(Writing::Confirm(plan)) => {
                        self.overlay = Overlay::ObsidianWrite(Confirmation { plan });
                        window.focus(&self.focus);
                        cx.notify();
                    }
                    Ok(Writing::OpenOrCreate {
                        path,
                        new_tab,
                        daily,
                    }) => {
                        if daily {
                            return self.open_daily_note(window, cx);
                        }
                        let id = match vault.note_by_path(&path) {
                            Some(id) => Ok(id),
                            None => vault.create_note(&path, ""),
                        };
                        match id {
                            Ok(id) => self.open_note(id, new_tab, window, cx),
                            Err(error) => self.refuse_address(&vault_refusal(error), cx),
                        }
                    }
                    Err(refusal) => self.refuse_address(&refusal, cx),
                }
            }
            reading => match resolve_reading(&vault, reading) {
                Ok(Reading::Nothing) => {}
                Ok(Reading::Search(query)) => self.show_search(&query, window, cx),
                Ok(Reading::Note {
                    id,
                    anchor,
                    new_tab,
                }) => {
                    self.open_note(id, new_tab, window, cx);
                    if let Some(line) = anchor.and_then(|a| vault.anchor_line(id, &a)) {
                        self.pending_line = Some((id, line));
                    }
                }
                Err(refusal) => self.refuse_address(&refusal, cx),
            },
        }
    }

    pub fn follow_pending_addresses(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for address in std::mem::take(&mut self.obsidian.0) {
            self.follow_obsidian(&address, window, cx);
        }
    }

    fn refuse_address(&mut self, refusal: &Refusal, cx: &mut Context<Self>) {
        let open = self
            .library
            .as_ref()
            .map(|library| library.name())
            .unwrap_or_default();
        let tone = match refusal {
            Refusal::Vault(_) | Refusal::Changed(_) => focus_kit::Tone::Danger,
            _ => focus_kit::Tone::Warn,
        };
        self.notify_user(tone, refusal.message(&open), cx);
    }

    pub fn confirm_obsidian_write(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Overlay::ObsidianWrite(confirmation) =
            std::mem::replace(&mut self.overlay, Overlay::None)
        else {
            return;
        };
        self.focus_content(window, cx);
        cx.notify();
        let Some(library) = self.library.clone() else {
            return;
        };
        let plan = confirmation.plan;
        let vault = library.vault().clone();
        let job = plan.clone();
        let task = cx.background_spawn(async move { apply(&vault, &job) });
        self.pending += 1;
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.pending -= 1;
                match result {
                    Ok(id) => {
                        this.poll_history_error(cx);
                        this.reload_notes(&[id], window, cx);
                        this.notify_user(
                            focus_kit::Tone::Ok,
                            focus_i18n::t_args(
                                "ui-obsidian-written",
                                &[("path", plan.path.display().to_string().as_str().into())],
                            ),
                            cx,
                        );
                        if plan.open {
                            this.open_note(id, plan.new_tab, window, cx);
                        }
                    }
                    Err(refusal) => this.refuse_address(&refusal, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn render_obsidian_write(
        &self,
        confirmation: &Confirmation,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = *cx.tokens();
        let plan = &confirmation.plan;
        let path = plan.path.display().to_string();
        let (title, verb) = match plan.mode {
            Mode::Create => ("ui-obsidian-create-title", "ui-obsidian-create"),
            Mode::Append => ("ui-obsidian-append-title", "ui-obsidian-append"),
            Mode::Prepend => ("ui-obsidian-prepend-title", "ui-obsidian-prepend"),
            Mode::Overwrite => ("ui-obsidian-overwrite-title", "ui-obsidian-overwrite"),
        };
        let block = |id: &'static str, text: String| {
            div()
                .id(id)
                .max_h(px(160.))
                .overflow_y_scroll()
                .mono_sm()
                .text_left()
                .text_color(t.ink)
                .bg(t.card_2)
                .p(size::S2)
                .child(text)
        };
        let label = |key: &str| div().ui_sm().text_color(t.ink_3).child(focus_i18n::t(key));
        let replaced = plan.before.clone().filter(|_| plan.mode == Mode::Overwrite);
        focus_kit::fade_in(
            "fade-obsidian",
            div()
                .id("obsidian-veil")
                .absolute()
                .inset_0()
                .flow_row()
                .items_center()
                .justify_center()
                .bg(t.veil)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_overlay(window, cx)),
                )
                .child(
                    focus_kit::floating(&t)
                        .id("obsidian-dialog")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .flex()
                        .flex_col()
                        .gap(size::S3)
                        .p(size::S4)
                        .w(px(560.))
                        .max_w_full()
                        .child(
                            div()
                                .display_sm()
                                .text_color(t.ink)
                                .child(focus_i18n::t(title)),
                        )
                        .child(label("ui-obsidian-why"))
                        .child(
                            div()
                                .mono_sm()
                                .text_left()
                                .text_color(t.ink)
                                .child(path.clone()),
                        )
                        .child(label("ui-obsidian-text"))
                        .child(block(
                            "obsidian-added",
                            if plan.added.is_empty() {
                                focus_i18n::t("ui-obsidian-empty")
                            } else {
                                plan.added.clone()
                            },
                        ))
                        .when_some(replaced, |this, before| {
                            this.child(div().ui_sm().text_color(t.ink_2).child(focus_i18n::t_args(
                                "ui-obsidian-replaces",
                                &[("lines", before.lines().count().into())],
                            )))
                            .child(block("obsidian-before", before))
                        })
                        .child(
                            div()
                                .flow_row()
                                .justify_end()
                                .gap(size::S2)
                                .child(
                                    Button::new(
                                        "obsidian-cancel",
                                        focus_i18n::t("ui-action-cancel"),
                                    )
                                    .on_click(cx.listener(
                                        |this, _, window, cx| this.close_overlay(window, cx),
                                    )),
                                )
                                .child(
                                    Button::new("obsidian-confirm", focus_i18n::t(verb))
                                        .kind(if plan.mode == Mode::Overwrite {
                                            ButtonKind::Danger
                                        } else {
                                            ButtonKind::Primary
                                        })
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.confirm_obsidian_write(window, cx)
                                        })),
                                ),
                        ),
                ),
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focus_core::OpenOptions;
    use focus_view_vault::{Date, Time};

    const DAY: Date = Date {
        year: 2026,
        month: 11,
        day: 17,
    };
    const TIME: Time = Time {
        hour: 14,
        minute: 30,
    };

    fn open(name: &str) -> (PathBuf, std::sync::Arc<Vault>) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-tmp/obsidian");
        let base = dir.join(name);
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("Work");
        std::fs::create_dir_all(root.join("Projects")).unwrap();
        std::fs::create_dir_all(root.join("Templates")).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        let root = base.join("Work");
        std::fs::write(
            root.join("Projects/Plan.md"),
            "# Plan\n\n## Tasks\n- one\n\n## Notes\nText\n",
        )
        .unwrap();
        std::fs::write(root.join("Inbox.md"), "---\ntags: [a]\n---\nFirst\n").unwrap();
        std::fs::write(root.join("Templates/Day.md"), "# {{title}}\n").unwrap();
        std::fs::write(base.join("secret.md"), "outside\n").unwrap();
        let vault = Vault::open(
            &root,
            OpenOptions {
                cache_dir: base.join("cache"),
                ignore: Vec::new(),
                watch: false,
            },
        )
        .unwrap();
        (base, vault)
    }

    fn settings(vault: &Vault) -> WriteSettings {
        WriteSettings {
            new_note_folder: "Inbox notes".into(),
            daily_folder: "Daily".into(),
            daily_format: "YYYY-MM-DD".into(),
            daily_template: vault.note_by_path(Path::new("Templates/Day.md")),
            untitled: "Untitled".into(),
        }
    }

    fn write_request(address: &str) -> WriteRequest {
        match parse(address).unwrap() {
            Address::Write(request) => request,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_form_of_address_is_parsed() {
        assert_eq!(
            parse("obsidian://open?vault=my%20vault&file=path%2Fto%2Fmy%20note").unwrap(),
            Address::Open {
                vault: Some("my vault".into()),
                target: Target::File {
                    file: "path/to/my note".into(),
                    anchor: None
                },
                new_tab: false
            }
        );
        assert_eq!(
            parse("obsidian://open?vault=v&file=Note%23Heading%20two&paneType=tab").unwrap(),
            Address::Open {
                vault: Some("v".into()),
                target: Target::File {
                    file: "Note".into(),
                    anchor: Some("Heading two".into())
                },
                new_tab: true
            }
        );
        assert_eq!(
            parse("obsidian://open?file=Note%23%5Eblock-1").unwrap(),
            Address::Open {
                vault: None,
                target: Target::File {
                    file: "Note".into(),
                    anchor: Some("^block-1".into())
                },
                new_tab: false
            }
        );
        assert_eq!(
            parse("OBSIDIAN://open?path=%2Fhome%2Fuser%2Fmy%20vault%2Fnote").unwrap(),
            Address::Open {
                vault: None,
                target: Target::Path {
                    path: "/home/user/my vault/note".into(),
                    anchor: None
                },
                new_tab: false
            }
        );
        assert_eq!(
            parse("obsidian://open?vault=my%20vault").unwrap(),
            Address::Open {
                vault: Some("my vault".into()),
                target: Target::Vault,
                new_tab: false
            }
        );
        assert_eq!(
            parse("obsidian://vault/my vault/my note").unwrap(),
            Address::Open {
                vault: Some("my vault".into()),
                target: Target::File {
                    file: "my note".into(),
                    anchor: None
                },
                new_tab: false
            }
        );
        assert_eq!(
            parse("obsidian://vault/my%20vault/Projects/Plan%23Tasks").unwrap(),
            Address::Open {
                vault: Some("my vault".into()),
                target: Target::File {
                    file: "Projects/Plan".into(),
                    anchor: Some("Tasks".into())
                },
                new_tab: false
            }
        );
        assert_eq!(
            parse("obsidian:///absolute/path/to/my%20note").unwrap(),
            Address::Open {
                vault: None,
                target: Target::Path {
                    path: "/absolute/path/to/my note".into(),
                    anchor: None
                },
                new_tab: false
            }
        );
        assert_eq!(
            parse("obsidian://search?vault=v&query=tag%3A%23work%20a%2Bb").unwrap(),
            Address::Search {
                vault: Some("v".into()),
                query: "tag:#work a+b".into()
            }
        );
        let request = write_request(
            "obsidian://new?vault=v&file=a%2Fb&append=true&heading=Tasks&content=Buy%20milk&spacing=newline&silent",
        );
        assert_eq!(request.action, WriteAction::New);
        assert_eq!(request.place, Place::File("a/b".into()));
        assert_eq!(request.mode, Mode::Append);
        assert_eq!(request.heading.as_deref(), Some("Tasks"));
        assert_eq!(request.content, Some(Content::Text("Buy milk".into())));
        assert_eq!(request.spacing, Spacing::Newline);
        assert!(request.silent);
        assert_eq!(
            write_request("obsidian://daily?vault=v&content=x&overwrite").mode,
            Mode::Overwrite
        );
        assert_eq!(
            write_request("obsidian://unique?content=x&append=false").mode,
            Mode::Create
        );
        assert_eq!(
            write_request("obsidian://new?name=n&clipboard").content,
            Some(Content::Clipboard)
        );
        assert_eq!(decode("caf%C3%A9%2"), "café%2");
    }

    #[test]
    fn callbacks_and_the_vault_manager_are_refused_by_name() {
        for (address, name) in [
            ("obsidian://choose-vault", "choose-vault"),
            ("obsidian://hook-get-address", "hook-get-address"),
            (
                "obsidian://open?vault=v&file=n&x-success=myapp%3A%2F%2Fx",
                "x-success",
            ),
            ("obsidian://new?name=n&content=c&x-error=a%3A", "x-error"),
        ] {
            assert_eq!(
                parse(address),
                Err(Refusal::Refused(name.into())),
                "{address}"
            );
        }
        assert_eq!(
            parse("obsidian://launch?x=1"),
            Err(Refusal::Unknown("launch".into()))
        );
        assert_eq!(parse("obsidian:open"), Err(Refusal::Malformed));
        assert_eq!(parse("https://example.org"), Err(Refusal::Malformed));
    }

    #[test]
    fn reading_addresses_resolve_inside_the_open_vault_only() {
        let (base, vault) = open("reading");
        let plan = vault.note_by_path(Path::new("Projects/Plan.md")).unwrap();
        let resolve = |address: &str| resolve_reading(&vault, &parse(address).unwrap());
        let note = |anchor: Option<&str>| {
            Ok(Reading::Note {
                id: plan,
                anchor: anchor.map(str::to_string),
                new_tab: false,
            })
        };
        assert_eq!(
            resolve("obsidian://open?vault=Work&file=Projects%2FPlan"),
            note(None)
        );
        assert_eq!(
            resolve("obsidian://open?vault=Work&file=Plan.md"),
            note(None)
        );
        assert_eq!(
            resolve("obsidian://vault/Work/Plan%23Tasks"),
            note(Some("Tasks"))
        );
        let absolute = base.join("Work/Projects/Plan");
        assert_eq!(
            resolve(&format!("obsidian://{}", absolute.display())),
            note(None)
        );
        assert_eq!(
            resolve("obsidian://open?vault=Personal&file=Plan"),
            Err(Refusal::OtherVault("Personal".into()))
        );
        assert_eq!(
            resolve("obsidian://search?vault=Personal&query=x"),
            Err(Refusal::OtherVault("Personal".into()))
        );
        assert_eq!(
            resolve("obsidian://search?vault=Work&query=x"),
            Ok(Reading::Search("x".into()))
        );
        assert_eq!(
            resolve("obsidian://open?file=Missing"),
            Err(Refusal::NotFound("Missing".into()))
        );
        for outside in [
            format!("obsidian://open?path={}", base.join("secret.md").display()),
            format!(
                "obsidian://open?path={}",
                base.join("Work/../secret.md").display()
            ),
            format!("obsidian://{}", base.join("secret").display()),
        ] {
            assert!(
                matches!(resolve(&outside), Err(Refusal::Outside(_))),
                "{outside}"
            );
        }
        assert!(matches!(
            resolve("obsidian://open?vault=Work&file=..%2Fsecret"),
            Err(Refusal::Outside(_))
        ));
        assert_eq!(vault.anchor_line(plan, "Tasks"), Some(2));
    }

    #[test]
    fn a_write_does_nothing_before_its_confirmation() {
        let (base, vault) = open("writes");
        let root = base.join("Work");
        let settings = settings(&vault);
        let planned =
            |address: &str| plan(&vault, &write_request(address), &settings, None, DAY, TIME);
        let before = std::fs::read_to_string(root.join("Projects/Plan.md")).unwrap();
        let Ok(Writing::Confirm(append)) = planned(
            "obsidian://new?vault=Work&file=Projects%2FPlan&append&heading=Tasks&content=-%20two&spacing=newline",
        ) else {
            panic!("an append is confirmed first")
        };
        assert_eq!(
            std::fs::read_to_string(root.join("Projects/Plan.md")).unwrap(),
            before,
            "nothing is written while the confirmation waits"
        );
        assert_eq!(append.added, "- two");
        assert_eq!(
            append.after,
            "# Plan\n\n## Tasks\n- one\n- two\n\n## Notes\nText\n"
        );
        apply(&vault, &append).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("Projects/Plan.md")).unwrap(),
            append.after
        );
        assert_eq!(
            apply(&vault, &append),
            Err(Refusal::Changed(append.path.clone()))
        );

        let Ok(Writing::Confirm(overwrite)) =
            planned("obsidian://new?file=Inbox&overwrite&content=Replaced")
        else {
            panic!("an overwrite is confirmed first")
        };
        assert_eq!(overwrite.mode, Mode::Overwrite);
        assert_eq!(
            overwrite.before.as_deref(),
            Some("---\ntags: [a]\n---\nFirst\n")
        );
        assert_eq!(overwrite.after, "Replaced");

        let Ok(Writing::Confirm(merged)) = planned(
            "obsidian://new?file=Inbox&prepend&content=---%0Atags%3A%20%5Bb%5D%0A---%0ATop",
        ) else {
            panic!()
        };
        assert_eq!(
            merged.after,
            "---\ntags:\n  - a\n  - b\n---\nTop\n\nFirst\n"
        );

        let Ok(Writing::Confirm(taken)) = planned("obsidian://new?name=Plan&content=x") else {
            panic!()
        };
        assert_eq!(taken.path, Path::new("Inbox notes/Plan.md"));
        let Ok(Writing::Confirm(second)) = planned("obsidian://new?file=Inbox&content=x") else {
            panic!()
        };
        assert_eq!(
            second.path,
            Path::new("Inbox 1.md"),
            "a taken name is not overwritten"
        );

        let Ok(Writing::Confirm(daily)) = planned("obsidian://daily?content=Called%20Sam") else {
            panic!()
        };
        assert_eq!(daily.path, Path::new("Daily/2026-11-17.md"));
        assert_eq!(daily.after, "# 2026-11-17\n\nCalled Sam\n");
        assert!(!root.join("Daily").exists());
        apply(&vault, &daily).unwrap();
        assert!(root.join("Daily/2026-11-17.md").exists());

        let Ok(Writing::Confirm(unique)) = planned("obsidian://unique?vault=Work") else {
            panic!("a unique note is confirmed even without content")
        };
        assert_eq!(unique.path, Path::new("Inbox notes/202611171430.md"));

        assert_eq!(
            planned("obsidian://new?name=Plan"),
            Ok(Writing::OpenOrCreate {
                path: "Inbox notes/Plan.md".into(),
                new_tab: false,
                daily: false
            })
        );
        assert_eq!(
            planned("obsidian://new?vault=Personal&name=x&content=y"),
            Err(Refusal::OtherVault("Personal".into()))
        );
        assert!(matches!(
            planned(&format!(
                "obsidian://new?path={}&content=y",
                base.join("secret.md").display()
            )),
            Err(Refusal::Outside(_))
        ));
        assert!(matches!(
            planned("obsidian://new?file=..%2F..%2Fescape&content=y"),
            Err(Refusal::Outside(_))
        ));
        assert!(matches!(
            planned("obsidian://new?file=Projects%2FPlan&append&heading=Nope&content=y"),
            Err(Refusal::HeadingMissing { .. })
        ));
        assert_eq!(
            std::fs::read_to_string(base.join("secret.md")).unwrap(),
            "outside\n"
        );
    }
}
