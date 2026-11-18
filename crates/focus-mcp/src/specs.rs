use focus_ai::ToolSpec;
use serde_json::{Value, json};

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        description: description.into(),
        parameters: json!({ "type": "object", "properties": properties, "required": required }),
    }
}

fn path() -> Value {
    json!({ "type": "string", "description": "Note path relative to the vault root. The .md extension may be omitted." })
}

fn engagement() -> Value {
    json!({ "type": "string", "description": "Engagement name (the folder under the engagements folder), as returned by ops_list_engagements." })
}

fn dry_run() -> Value {
    json!({ "type": "boolean", "description": "Check the call as a real one and answer what it would change, without writing, keeping a version or reaching the application. Default false." })
}

fn if_match(read_with: &str) -> Value {
    json!({ "type": "string", "description": format!("Etag of the content as {read_with} gave it. When it differs from the current one the call is refused with code `stale` and nothing is written.") })
}

fn idempotency_key() -> Value {
    json!({ "type": "string", "description": "Any string of at most 128 characters. A second call with the same key and arguments answers the first result and writes nothing.", "maxLength": 128 })
}

fn offset() -> Value {
    json!({ "type": "integer", "description": "Number of results to skip, to read the next page. Default 0.", "minimum": 0 })
}

fn limit(default: usize) -> Value {
    json!({ "type": "integer", "description": format!("Maximum number of results in the page. Default {default}."), "minimum": 1 })
}

pub fn adapt(mut spec: ToolSpec) -> ToolSpec {
    const PAGE: &str = "Answers one page: when results remain, the end says how many and which `offset` to send for the next page.";
    let (extra, sentence): (Vec<(&str, Value)>, &str) = match spec.name.as_str() {
        "search_notes" => (
            vec![
                ("offset", offset()),
                (
                    "fields",
                    json!({ "type": "array", "items": { "type": "string", "enum": ["title", "matches"] }, "description": "Parts of each row to return besides the path. Default both." }),
                ),
            ],
            PAGE,
        ),
        "read_note" => (
            vec![],
            "The first line ends with the etag of the note: send it as `if_match` to a write tool that changes what you read.",
        ),
        "list_folder" | "get_backlinks" => {
            (vec![("offset", offset()), ("limit", limit(100))], PAGE)
        }
        "list_tags" | "list_tasks" | "most_linked" | "orphan_notes" | "ops_journal_read" => {
            (vec![("offset", offset()), ("limit", limit(100))], PAGE)
        }
        "orphan_attachments" | "ops_hosts_list" | "ops_findings_list" | "ops_credentials_list" => {
            (vec![("offset", offset()), ("limit", limit(100))], PAGE)
        }
        "create_note" => (
            vec![
                ("dry_run", dry_run()),
                ("idempotency_key", idempotency_key()),
            ],
            "The answer gives the etag of the new note. Send `idempotency_key` to retry safely.",
        ),
        "append_to_note" => (
            vec![
                ("dry_run", dry_run()),
                ("if_match", if_match("read_note")),
                ("idempotency_key", idempotency_key()),
            ],
            "Send `if_match` when you read the note first. The answer gives the new etag.",
        ),
        "replace_in_note" | "toggle_task" => (
            vec![("dry_run", dry_run()), ("if_match", if_match("read_note"))],
            "Always send `if_match` with the etag of the note you read, so that a change made since is not overwritten. The answer gives the new etag.",
        ),
        "rename_note" | "trash_note" => (
            vec![("dry_run", dry_run()), ("if_match", if_match("read_note"))],
            "Send `if_match` with the etag of the note you read. Use `dry_run` to see the effect first.",
        ),
        "export_note" => (vec![("dry_run", dry_run())], ""),
        "ops_journal_append" | "ops_import_nmap" | "ops_finding_create" | "ops_credential_add" => (
            vec![
                ("dry_run", dry_run()),
                ("idempotency_key", idempotency_key()),
            ],
            "Send `idempotency_key` to retry safely.",
        ),
        "ops_vars_get" => (
            vec![],
            "The answer ends with the etag of the engagement README: send it as `if_match` to ops_vars_set or ops_set_focus.",
        ),
        "ops_vars_set" | "ops_set_focus" => (
            vec![
                ("dry_run", dry_run()),
                ("if_match", if_match("ops_vars_get")),
            ],
            "Send `if_match` with the etag from ops_vars_get so that a change made since is not overwritten.",
        ),
        "ops_report_build" => (
            vec![
                ("dry_run", dry_run()),
                ("if_match", if_match("read_note on Report.md")),
            ],
            "With `save`, send `if_match` when Report.md exists and you read it.",
        ),
        "app_insert_at_cursor" => (vec![("dry_run", dry_run())], ""),
        _ => (vec![], ""),
    };
    if let Some(properties) = spec
        .parameters
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    {
        for (name, schema) in extra {
            properties.entry(name).or_insert(schema);
        }
    }
    if !sentence.is_empty() {
        spec.description.push(' ');
        spec.description.push_str(sentence);
    }
    spec
}

pub fn vault_tools() -> Vec<ToolSpec> {
    vault_tool_list().into_iter().map(adapt).collect()
}

pub fn ops_tools() -> Vec<ToolSpec> {
    ops_tool_list().into_iter().map(adapt).collect()
}

pub fn app_tools() -> Vec<ToolSpec> {
    app_tool_list().into_iter().map(adapt).collect()
}

fn vault_tool_list() -> Vec<ToolSpec> {
    vec![
        tool(
            "list_tasks",
            "List the checkbox tasks of the vault (`- [ ] text`) with their note and line number. Open tasks only by default.",
            json!({
                "include_done": { "type": "boolean", "description": "Include completed tasks. Default false." },
                "path": { "type": "string", "description": "Restrict to one note." },
                "limit": { "type": "integer", "description": "Maximum number of tasks. Default 100.", "minimum": 1 }
            }),
            &[],
        ),
        tool(
            "toggle_task",
            "Check or uncheck the task on a line of a note and write the file.",
            json!({ "path": path(), "line": { "type": "integer", "description": "Line number of the task, starting at 1.", "minimum": 1 } }),
            &["path", "line"],
        ),
        tool(
            "rename_note",
            "Rename or move a note. Links in other notes that pointed to it are rewritten; the result counts the links rewritten and lists the notes and canvases that changed.",
            json!({ "path": path(), "new_path": { "type": "string", "description": "New path relative to the vault, including the .md extension." } }),
            &["path", "new_path"],
        ),
        tool(
            "trash_note",
            "Move a note to the vault trash (the .trash folder), from where it can be restored.",
            json!({ "path": path() }),
            &["path"],
        ),
        tool(
            "list_attachments",
            "List the attachments (images, PDFs) that a note references.",
            json!({ "path": path() }),
            &["path"],
        ),
        tool(
            "orphan_attachments",
            "List the attachments of the vault that no note references.",
            json!({}),
            &[],
        ),
        tool(
            "shortest_path",
            "Find the shortest chain of links between two notes, following links in both directions. Gives the direction of each link.",
            json!({ "from": path(), "to": path() }),
            &["from", "to"],
        ),
        tool(
            "most_linked",
            "List the most referenced notes of the vault with their number of incoming notes.",
            json!({ "limit": { "type": "integer", "description": "Number of notes. Default 20, at most 100.", "minimum": 1, "maximum": 100 } }),
            &[],
        ),
        tool(
            "orphan_notes",
            "List the notes that have no links at all, neither incoming nor outgoing.",
            json!({ "limit": { "type": "integer", "description": "Number of notes. Default 100.", "minimum": 1 } }),
            &[],
        ),
        tool(
            "export_note",
            "Export a note as a standalone HTML page, as standard Markdown (wiki links converted, embeds expanded) or as PDF (needs a Chromium browser). Never overwrites an existing file. Without a destination, writes under the vault export folder.",
            json!({
                "path": path(),
                "format": { "type": "string", "enum": ["html", "markdown", "pdf"], "description": "Output format." },
                "destination": { "type": "string", "description": "File path inside the export folder, relative to it or absolute. Default `<export folder>/<note name>.<extension>`." }
            }),
            &["path", "format"],
        ),
    ]
}

fn ops_tool_list() -> Vec<ToolSpec> {
    let severity = json!({ "type": "string", "enum": ["info", "low", "medium", "high", "critical"], "description": "Severity of the finding." });
    vec![
        tool(
            "ops_list_engagements",
            "List the engagements: name, type, status, targeted host.",
            json!({}),
            &[],
        ),
        tool(
            "ops_overview",
            "Key figures of an engagement: hosts, services, findings by severity, credentials, journal entries, checklist progress, points.",
            json!({ "engagement": engagement() }),
            &["engagement"],
        ),
        tool(
            "ops_journal_read",
            "Read the latest journal entries of an engagement (timestamp, target, text).",
            json!({ "engagement": engagement(), "limit": { "type": "integer", "description": "Number of entries. Default 50.", "minimum": 1 }, "target": { "type": "string", "description": "Restrict to the entries of one target." } }),
            &["engagement"],
        ),
        tool(
            "ops_journal_append",
            "Append a timestamped line to the journal of an engagement. Without a target, uses the targeted host of the engagement.",
            json!({ "engagement": engagement(), "text": { "type": "string", "description": "Entry text, on one line." }, "target": { "type": "string", "description": "Host the entry is about." } }),
            &["engagement", "text"],
        ),
        tool(
            "ops_hosts_list",
            "List the hosts of an engagement: address, name, operating system, status, open services.",
            json!({ "engagement": engagement() }),
            &["engagement"],
        ),
        tool(
            "ops_import_nmap",
            "Import an existing nmap output (XML, grepable or normal, detected automatically) into an engagement. Known hosts keep their notes. No scan is run.",
            json!({ "engagement": engagement(), "text": { "type": "string", "description": "Complete content of the nmap output." } }),
            &["engagement", "text"],
        ),
        tool(
            "ops_findings_list",
            "List the findings of an engagement from most to least severe, with identifier, severity, status, score and CWE identifiers.",
            json!({ "engagement": engagement() }),
            &["engagement"],
        ),
        tool(
            "ops_finding_create",
            "Create the note of a finding in an engagement (empty sections to fill in) and set severity, CVSS vector, affected assets, techniques and weaknesses.",
            json!({
                "engagement": engagement(),
                "title": { "type": "string", "description": "Title of the finding." },
                "severity": severity,
                "cvss": { "type": "string", "description": "Full CVSS 3.1 or 4.0 vector, for example `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N`. The score is computed; without `severity`, the severity is derived from it." },
                "assets": { "type": "array", "items": { "type": "string" }, "description": "Affected assets (addresses, host names)." },
                "attack": { "type": "array", "items": { "type": "string" }, "description": "MITRE ATT&CK technique identifiers." },
                "cwe": { "type": "array", "items": { "type": "string" }, "description": "CWE identifiers, for example `CWE-79` (`79` is accepted)." }
            }),
            &["engagement", "title"],
        ),
        tool(
            "ops_cvss_score",
            "Compute the base score of a CVSS 3.1 or 4.0 vector and give its severity.",
            json!({ "vector": { "type": "string", "description": "CVSS vector with its `CVSS:3.1/` or `CVSS:4.0/` prefix." } }),
            &["vector"],
        ),
        tool(
            "ops_credentials_list",
            "List the credentials of an engagement. Secrets are masked unless the server was started with --reveal-secrets.",
            json!({ "engagement": engagement() }),
            &["engagement"],
        ),
        tool(
            "ops_credential_add",
            "Add a credential to the engagement table. An exact duplicate (user, type, secret, domain) is not added.",
            json!({
                "engagement": engagement(),
                "user": { "type": "string" },
                "kind": { "type": "string", "description": "password, ntlm, aes256, ticket, ssh-key, token or hash." },
                "secret": { "type": "string" },
                "domain": { "type": "string" },
                "source": { "type": "string", "description": "Where it was found." },
                "valid_on": { "type": "string", "description": "Where it works." },
                "notes": { "type": "string" }
            }),
            &["engagement", "user", "kind", "secret"],
        ),
        tool(
            "ops_vars_get",
            "Get the context variables of the engagement (RHOST, LHOST, DOMAIN...). Values of passwords and hashes are masked unless --reveal-secrets is set.",
            json!({ "engagement": engagement() }),
            &["engagement"],
        ),
        tool(
            "ops_vars_set",
            "Set a context variable of the engagement; an empty value deletes it.",
            json!({ "engagement": engagement(), "name": { "type": "string", "description": "Variable name, for example RHOST." }, "value": { "type": "string" } }),
            &["engagement", "name", "value"],
        ),
        tool(
            "ops_set_focus",
            "Set the targeted host of the engagement (host note name or address). Empty string: no target.",
            json!({ "engagement": engagement(), "host": { "type": "string" } }),
            &["engagement", "host"],
        ),
        tool(
            "ops_checklist_progress",
            "Checklist progress of the engagement, phase by phase, with the next open task of each phase.",
            json!({ "engagement": engagement() }),
            &["engagement"],
        ),
        tool(
            "ops_report_build",
            "Assemble the Markdown report of the engagement (summary, scope, findings, hosts, appendices) and return it. With `save`, also write it to Report.md. Secrets are masked unless --reveal-secrets is set.",
            json!({
                "engagement": engagement(),
                "include_journal": { "type": "boolean", "description": "Journal appendix. Default true." },
                "include_credentials": { "type": "boolean", "description": "Credentials appendix. Default false." },
                "author": { "type": "string" },
                "save": { "type": "boolean", "description": "Write Report.md in the engagement folder (needs --write)." }
            }),
            &["engagement"],
        ),
    ]
}

fn app_tool_list() -> Vec<ToolSpec> {
    vec![
        tool(
            "app_current_note",
            "Get the note open in the Focus application: path, selected text, cursor position.",
            json!({}),
            &[],
        ),
        tool(
            "app_open_note",
            "Open a note in the Focus application, at a line if one is given.",
            json!({ "path": path(), "line": { "type": "integer", "description": "Line to show, starting at 1.", "minimum": 1 }, "new_tab": { "type": "boolean", "description": "Open in a new tab. Default false." } }),
            &["path"],
        ),
        tool(
            "app_show_search",
            "Show a search in the search panel of the Focus application (Focus query syntax).",
            json!({ "query": { "type": "string" } }),
            &["query"],
        ),
        tool(
            "app_show_graph",
            "Show the graph in the Focus application, centered on a note if one is given.",
            json!({ "note": { "type": "string", "description": "Path of the note to center on." } }),
            &[],
        ),
        tool(
            "app_insert_at_cursor",
            "Insert text at the cursor position of the active note of the Focus application.",
            json!({ "text": { "type": "string" } }),
            &["text"],
        ),
        tool(
            "app_notify",
            "Show a notification in the Focus application.",
            json!({ "text": { "type": "string" }, "level": { "type": "string", "enum": ["info", "warning", "error"], "description": "Level. Default info." } }),
            &["text"],
        ),
    ]
}
