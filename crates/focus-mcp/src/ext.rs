use std::path::PathBuf;

use serde_json::Value;

use crate::exposure::Exposure;
use crate::fault::Fault;
use crate::graph::Graph;
use crate::ipc::{CurrentNote, IpcError, Level, Request, Response};
use crate::rules::{WriteOptions, changed_lines, etag};
use crate::tools::{
    Outcome, Page, Refusal, VaultTools, dry_run_text, optional_number, reread_hint, text_argument,
};

const NOT_RUNNING: &str = "The Focus application is not open on this vault: no action was taken.";

fn optional_bool(arguments: &Value, name: &str) -> Result<Option<bool>, String> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(format!("parameter `{name}`: boolean expected")),
    }
}

impl VaultTools {
    pub(crate) fn list_tasks(&self, arguments: &Value) -> Outcome {
        let include_done = optional_bool(arguments, "include_done")?.unwrap_or(false);
        let page = Page::of(arguments, 100, usize::MAX)?;
        let only = match arguments
            .get("path")
            .and_then(Value::as_str)
            .filter(|p| !p.trim().is_empty())
        {
            Some(path) => Some(self.locate(path)?.0),
            None => None,
        };
        let tasks: Vec<_> = self
            .vault
            .tasks(include_done)
            .into_iter()
            .filter(|t| only.is_none_or(|id| id == t.note) && self.shown(t.note))
            .collect();
        if tasks.is_empty() {
            return Ok("No tasks".into());
        }
        let mut sorted: Vec<(String, u32, bool, String)> = tasks
            .iter()
            .map(|t| (self.path_of(t.note), t.line, t.done, t.text.clone()))
            .collect();
        sorted.sort();
        let total = sorted.len();
        let (shown, remainder) =
            page.cut(sorted, "tasks", "send `path` to list the tasks of one note");
        let mut out = format!("{total} task(s)\n");
        for (path, line, done, text) in &shown {
            out.push_str(&format!(
                "{path}:{} [{}] {}\n",
                line + 1,
                if *done { "x" } else { " " },
                text.trim()
            ));
        }
        out.push_str(&remainder);
        Ok(out)
    }

    pub(crate) fn toggle_task(
        &self,
        arguments: &Value,
        options: &WriteOptions,
        cancel: &focus_ai::Cancel,
    ) -> Result<String, Refusal> {
        let (id, relative) = self.locate(text_argument(arguments, "path")?)?;
        self.paths.check_writable(&relative)?;
        let line = optional_number(arguments, "line")?
            .filter(|l| *l >= 1)
            .ok_or("parameter `line`: integer starting at 1 expected")?;
        let index = (line - 1) as u32;
        let (before, _) = self.read_with_stamp(id, &relative)?;
        options.check(Some(&before), "path", &reread_hint(&relative))?;
        let task = self
            .vault
            .tasks(true)
            .into_iter()
            .find(|t| t.note == id && t.line == index)
            .ok_or_else(|| format!("no task at line {line} of {}", relative.display()))?;
        let after: String = before
            .split_inclusive('\n')
            .enumerate()
            .map(|(i, text)| {
                if i as u32 == index {
                    toggle_box(text)
                } else {
                    text.to_string()
                }
            })
            .collect();
        if options.dry_run {
            let show = self.line_filter(&relative);
            return Ok(dry_run_text(
                &format!("toggle_task would change {}", relative.display()),
                &format!(
                    "{}Etag now: {}\n",
                    changed_lines(&before, &after, &*show),
                    etag(&before)
                ),
            ));
        }
        self.authorize("toggle_task", &relative, Some(&before), &after, cancel)?;
        let done = self
            .vault
            .toggle_task(id, index)
            .map_err(|e| Refusal::Error(crate::tools::describe(e)))?;
        Ok(format!(
            "Task {}: {}:{line} {}{}",
            if done { "checked" } else { "unchecked" },
            relative.display(),
            task.text.trim(),
            self.etag_line(id)
        ))
    }

    fn rewrite_line(&self, report: &focus_core::MoveReport) -> String {
        let (visible, hidden): (Vec<_>, Vec<_>) = report
            .rewritten
            .iter()
            .partition(|r| matches!(self.exposure(&r.path), Exposure::Open | Exposure::Masked));
        let mut lines = Vec::new();
        if !visible.is_empty() {
            let mut files: Vec<String> = visible
                .iter()
                .map(|r| format!("{} ({})", r.path.display(), r.links))
                .collect();
            files.sort();
            lines.push(format!(
                "{} link(s) rewritten in {} file(s): {}",
                visible.iter().map(|r| r.links).sum::<usize>(),
                files.len(),
                files.join(", ")
            ));
        }
        if !hidden.is_empty() {
            lines.push(format!(
                "{} link(s) rewritten in {} file(s) that are not shown",
                hidden.iter().map(|r| r.links).sum::<usize>(),
                hidden.len()
            ));
        }
        if lines.is_empty() {
            return "No links to rewrite.".into();
        }
        lines.join("\n")
    }

    pub(crate) fn rename_note(
        &self,
        arguments: &Value,
        options: &WriteOptions,
        cancel: &focus_ai::Cancel,
    ) -> Result<String, Refusal> {
        let (id, from) = self.locate(text_argument(arguments, "path")?)?;
        self.paths.check_writable(&from)?;
        let mut to = self
            .paths
            .clean(text_argument(arguments, "new_path")?, false)?;
        if to.extension().is_none_or(|e| !e.eq_ignore_ascii_case("md")) {
            to = PathBuf::from(format!("{}.md", to.display()));
        }
        self.paths.check_writable(&to)?;
        if to == from {
            return Err(Refusal::Error(
                "the new path is the same as the old one".into(),
            ));
        }
        if self.vault.note_by_path(&to).is_some()
            || std::fs::symlink_metadata(self.vault.absolute(&to)).is_ok()
        {
            return Err(Refusal::Error(format!("already exists: {}", to.display())));
        }
        if options.if_match.is_some() {
            let (current, _) = self.read_with_stamp(id, &from)?;
            options.check(Some(&current), "path", &reread_hint(&from))?;
        }
        if options.dry_run {
            let report = self
                .vault
                .preview_rename(&from, &to)
                .map_err(|e| Refusal::Error(crate::tools::describe(e)))?;
            return Ok(dry_run_text(
                &format!("rename_note would move {}", from.display()),
                &format!(
                    "New path: {}\n{}\n",
                    to.display(),
                    self.rewrite_line(&report)
                ),
            ));
        }
        self.authorize(
            "rename_note",
            &from,
            None,
            &format!("rename to {}", to.display()),
            cancel,
        )?;
        let report = self
            .vault
            .rename(&from, &to, true)
            .map_err(|e| Refusal::Error(crate::tools::describe(e)))?;
        let mut out = format!("Renamed: from {} to {}", from.display(), to.display());
        out.push_str(&format!("\n{}", self.rewrite_line(&report)));
        for skipped in &report.skipped {
            out.push_str(&format!("\nLeft in place: {}", skipped.display()));
        }
        if let Some(moved) = self.vault.note_by_path(&to) {
            out.push_str(&self.etag_line(moved));
        }
        Ok(out)
    }

    pub(crate) fn trash_note(
        &self,
        arguments: &Value,
        options: &WriteOptions,
        cancel: &focus_ai::Cancel,
    ) -> Result<String, Refusal> {
        let (id, relative) = self.locate(text_argument(arguments, "path")?)?;
        self.paths.check_writable(&relative)?;
        if options.if_match.is_some() {
            let (current, _) = self.read_with_stamp(id, &relative)?;
            options.check(Some(&current), "path", &reread_hint(&relative))?;
        }
        if options.dry_run {
            return Ok(dry_run_text(
                &format!(
                    "trash_note would move {} to the vault trash",
                    relative.display()
                ),
                &format!("Path: {}\n", relative.display()),
            ));
        }
        self.authorize("trash_note", &relative, None, "move to trash", cancel)?;
        self.vault
            .trash(&relative)
            .map_err(|e| Refusal::Error(crate::tools::describe(e)))?;
        Ok(format!(
            "Moved to trash: {} (the .trash folder of the vault)",
            relative.display()
        ))
    }

    pub(crate) fn list_attachments(&self, arguments: &Value) -> Outcome {
        let (id, path) = self.locate(text_argument(arguments, "path")?)?;
        let files = self.vault.attachments_of(id);
        if files.is_empty() {
            return Ok(format!("{} references no attachments", path.display()));
        }
        let mut out = format!("{} attachment(s) in {}\n", files.len(), path.display());
        for file in files {
            out.push_str(&format!("{}\n", file.display()));
        }
        Ok(out)
    }

    pub(crate) fn orphan_attachments(&self, arguments: &Value) -> Outcome {
        let page = Page::of(arguments, 300, usize::MAX)?;
        let mut files = self.vault.orphan_attachments();
        if files.is_empty() {
            return Ok("No orphan attachments".into());
        }
        files.sort();
        let total = files.len();
        let (shown, remainder) = page.cut(files, "attachments", "");
        let mut out = format!("{total} attachment(s) that no note references\n");
        for file in &shown {
            out.push_str(&format!("{}\n", file.display()));
        }
        out.push_str(&remainder);
        Ok(out)
    }

    pub(crate) fn neighbors_deep(
        &self,
        id: focus_core::NoteId,
        path: &std::path::Path,
        depth: usize,
    ) -> Outcome {
        let graph = Graph::build(&self.vault);
        let layers = graph.layers(id, depth);
        if layers.is_empty() {
            return Ok(format!("{} has no neighbors", path.display()));
        }
        let total: usize = layers.iter().map(Vec::len).sum();
        let mut out = format!(
            "Neighbors of {} up to depth {depth}: {total} note(s)\n",
            path.display()
        );
        for (i, layer) in layers.iter().enumerate() {
            out.push_str(&format!("Depth {} ({}):\n", i + 1, layer.len()));
            let mut lines: Vec<String> = layer
                .iter()
                .filter(|n| self.shown(**n))
                .map(|n| {
                    let title = self
                        .vault
                        .note(*n)
                        .map(|m| m.title.to_string())
                        .unwrap_or_default();
                    let direction = if i == 0 {
                        match (
                            graph.outgoing(id).contains(n),
                            graph.incoming(id).contains(n),
                        ) {
                            (true, true) => " (links in both directions)",
                            (true, false) => " (outgoing link)",
                            _ => " (incoming link)",
                        }
                    } else {
                        ""
                    };
                    format!("  {}: {title}{direction}", self.path_of(*n))
                })
                .collect();
            lines.sort();
            let hidden = lines.len().saturating_sub(60);
            lines.truncate(60);
            for line in lines {
                out.push_str(&line);
                out.push('\n');
            }
            if hidden > 0 {
                out.push_str(&format!(
                    "  [{hidden} more not shown: lower `depth`, or call get_neighbors on one of these notes]\n"
                ));
            }
        }
        Ok(out)
    }

    pub(crate) fn shortest_path(&self, arguments: &Value) -> Outcome {
        let (from, from_path) = self.locate(text_argument(arguments, "from")?)?;
        let (to, to_path) = self.locate(text_argument(arguments, "to")?)?;
        let graph = Graph::build(&self.vault);
        let Some(path) = graph.shortest_path(from, to) else {
            return Ok(format!(
                "No chain of links between {} and {} (links are followed in both directions)",
                from_path.display(),
                to_path.display()
            ));
        };
        if path.len() == 1 {
            return Ok(format!("{} is the same note", from_path.display()));
        }
        let mut out = format!(
            "Shortest path: {} link(s)\n  {}\n",
            path.len() - 1,
            self.path_of(path[0])
        );
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let direction = if graph.outgoing(a).contains(&b) {
                format!("link from {} to this note", self.path_of(a))
            } else {
                format!("link from this note to {}", self.path_of(a))
            };
            out.push_str(&format!("  {} ({direction})\n", self.path_of(b)));
        }
        Ok(out)
    }

    pub(crate) fn most_linked(&self, arguments: &Value) -> Outcome {
        let page = Page::of(arguments, 20, 100)?;
        let graph = Graph::build(&self.vault);
        let mut ranked: Vec<(usize, String, String)> = graph
            .inn
            .iter()
            .filter(|(id, _)| self.shown(**id))
            .map(|(id, from)| {
                (
                    from.len(),
                    self.path_of(*id),
                    self.vault
                        .note(*id)
                        .map(|m| m.title.to_string())
                        .unwrap_or_default(),
                )
            })
            .collect();
        if ranked.is_empty() {
            return Ok("No links between notes in the vault".into());
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let (shown, remainder) = page.cut(ranked, "notes", "");
        let mut out = String::from("Most referenced notes (incoming notes: note)\n");
        for (count, path, title) in &shown {
            out.push_str(&format!("{count}: {path}: {title}\n"));
        }
        out.push_str(&remainder);
        Ok(out)
    }

    pub(crate) fn orphan_notes(&self, arguments: &Value) -> Outcome {
        let page = Page::of(arguments, 100, usize::MAX)?;
        let graph = Graph::build(&self.vault);
        let mut orphans: Vec<String> = self
            .vault
            .notes()
            .iter()
            .filter(|m| {
                self.shown(m.id)
                    && graph.outgoing(m.id).is_empty()
                    && graph.incoming(m.id).is_empty()
            })
            .map(|m| format!("{}: {}", m.path.display(), m.title))
            .collect();
        if orphans.is_empty() {
            return Ok("No notes without links".into());
        }
        orphans.sort();
        let total = orphans.len();
        let (shown, remainder) = page.cut(orphans, "notes", "");
        let mut out = format!("{total} note(s) without links\n");
        for line in &shown {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(&remainder);
        Ok(out)
    }

    fn app(&self, request: Request) -> Result<Response, String> {
        let Some(client) = &self.app else {
            return Err(NOT_RUNNING.to_string());
        };
        match client.request(&request) {
            Ok(response) => Ok(response),
            Err(IpcError::NotRunning) => Err(NOT_RUNNING.to_string()),
            Err(IpcError::Failed(message)) => Err(format!("Focus application: {message}")),
        }
    }

    pub(crate) fn app_tool(
        &self,
        name: &str,
        arguments: &Value,
        options: &WriteOptions,
        cancel: &focus_ai::Cancel,
    ) -> Result<String, Refusal> {
        let request = match name {
            "app_current_note" => Request::CurrentNote,
            "app_open_note" => {
                let (_, path) = self.locate(text_argument(arguments, "path")?)?;
                let line = optional_number(arguments, "line")?
                    .map(|l| l as u32)
                    .filter(|l| *l >= 1);
                Request::OpenNote {
                    path: path.display().to_string(),
                    line,
                    new_tab: optional_bool(arguments, "new_tab")?.unwrap_or(false),
                }
            }
            "app_show_search" => {
                let query = text_argument(arguments, "query")?.trim().to_string();
                if query.is_empty() {
                    return Err(Refusal::Error("empty query".into()));
                }
                crate::tools::bounded("query", &query, crate::tools::MAX_QUERY_CHARS)?;
                Request::ShowSearch { query }
            }
            "app_show_graph" => {
                let note = match arguments
                    .get("note")
                    .and_then(Value::as_str)
                    .filter(|n| !n.trim().is_empty())
                {
                    Some(n) => Some(self.locate(n)?.1.display().to_string()),
                    None => None,
                };
                Request::ShowGraph { note }
            }
            "app_insert_at_cursor" => {
                if !self.writes_enabled() {
                    return Err(Fault::read_only().into());
                }
                let text = text_argument(arguments, "text")?.to_string();
                if text.len() > crate::tools::MAX_WRITE_BYTES {
                    return Err(Refusal::Error("parameter `text` is too long".into()));
                }
                if options.dry_run {
                    return Ok(dry_run_text(
                        "app_insert_at_cursor would insert text at the cursor of the active note",
                        &format!("{} character(s)\n", text.chars().count()),
                    ));
                }
                self.authorize(
                    "app_insert_at_cursor",
                    std::path::Path::new("(active note)"),
                    None,
                    &text,
                    cancel,
                )?;
                Request::InsertAtCursor { text }
            }
            "app_notify" => {
                let level = match arguments
                    .get("level")
                    .and_then(Value::as_str)
                    .unwrap_or("info")
                {
                    "info" => Level::Info,
                    "warning" => Level::Warning,
                    "error" => Level::Error,
                    other => {
                        return Err(Refusal::Error(format!(
                            "unknown level: {other} (info, warning or error)"
                        )));
                    }
                };
                let text = text_argument(arguments, "text")?.to_string();
                crate::tools::bounded("text", &text, crate::tools::MAX_NOTICE_CHARS)?;
                Request::Notify { text, level }
            }
            other => return Err(Refusal::Error(format!("unknown tool: {other}"))),
        };
        match self.app(request) {
            Ok(Response::Done) => Ok("Done.".into()),
            Ok(Response::CurrentNote(note)) => Ok(self.describe_current(&note)),
            Ok(Response::Failed { message }) => Err(Refusal::Error(format!(
                "the application refused: {message}"
            ))),
            Err(message) if message == NOT_RUNNING => Ok(message),
            Err(message) => Err(Refusal::Error(message)),
        }
    }
}

impl VaultTools {
    fn describe_current(&self, note: &CurrentNote) -> String {
        let relative = note.path.as_deref().map(std::path::PathBuf::from);
        let exposure = relative.as_deref().map(|p| self.exposure(p));
        if matches!(
            exposure,
            Some(crate::exposure::Exposure::Secret | crate::exposure::Exposure::Outside)
        ) {
            return "The open note is not available to this server.".to_string();
        }
        let masked = exposure == Some(crate::exposure::Exposure::Masked);
        let note = CurrentNote {
            selection: if masked {
                crate::exposure::mask_secrets(&note.selection)
            } else {
                note.selection.clone()
            },
            ..note.clone()
        };
        describe_note(&note)
    }
}

fn describe_note(note: &CurrentNote) -> String {
    match &note.path {
        None => "No note is open in the application.".to_string(),
        Some(path) => {
            let mut out = format!(
                "Open note: {path}\nCursor: line {}, column {}\n",
                note.line, note.column
            );
            if note.selection.is_empty() {
                out.push_str("No selection.\n");
            } else {
                out.push_str(&format!(
                    "Selection ({} characters):\n{}\n",
                    note.selection.chars().count(),
                    note.selection
                ));
            }
            out
        }
    }
}

fn toggle_box(line: &str) -> String {
    if let Some(at) = line.find("[ ]") {
        format!("{}[x]{}", &line[..at], &line[at + 3..])
    } else if let Some(at) = line.find("[x]").or_else(|| line.find("[X]")) {
        format!("{}[ ]{}", &line[..at], &line[at + 3..])
    } else {
        line.to_string()
    }
}
