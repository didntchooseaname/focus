use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use focus_ai::Cancel;
use focus_ai::agent::ToolOutcome;
use focus_ai::mcp::{Server, ServerConfig, StartOptions};
use serde_json::{Value, json};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/vault");
const BINARY: &str = env!("CARGO_BIN_EXE_focus-mcp");

fn count_notes(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| {
            if e.path().is_dir() {
                count_notes(&e.path())
            } else {
                usize::from(e.path().extension().is_some_and(|x| x == "md"))
            }
        })
        .sum()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct Workspace {
    vault: PathBuf,
    cache: PathBuf,
    data: PathBuf,
    config: PathBuf,
}

fn workspace(name: &str) -> Workspace {
    assert!(
        Path::new(FIXTURE).is_dir(),
        "fixture vault missing: {FIXTURE}"
    );
    let base = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))
        .unwrap()
        .join("test-tmp")
        .join(format!("mcp-e2e-{name}"));
    let _ = std::fs::remove_dir_all(&base);
    let vault = base.join("vault");
    copy_dir(Path::new(FIXTURE), &vault);
    Workspace {
        vault,
        cache: base.join("cache"),
        data: base.join("data"),
        config: base.join("config"),
    }
}

fn environment(workspace: &Workspace) -> [(String, String); 2] {
    [
        (
            "FOCUS_DATA_DIR".into(),
            workspace.data.display().to_string(),
        ),
        (
            "FOCUS_CONFIG_DIR".into(),
            workspace.config.display().to_string(),
        ),
    ]
}

fn args(workspace: &Workspace, extra: &[&str]) -> Vec<String> {
    let mut all = vec![
        "--vault".to_string(),
        workspace.vault.display().to_string(),
        "--cache-dir".to_string(),
        workspace.cache.display().to_string(),
    ];
    all.extend(extra.iter().map(|s| s.to_string()));
    all
}

fn start(workspace: &Workspace, extra: &[&str]) -> Server {
    let config = ServerConfig {
        name: "focus".into(),
        command: BINARY.into(),
        args: args(workspace, extra),
        env: BTreeMap::from(environment(workspace)),
        allowed_tools: Vec::new(),
        enabled: true,
    };
    Server::start_with(
        &config,
        StartOptions {
            probe: Duration::from_secs(5),
            startup: Duration::from_secs(20),
        },
    )
    .unwrap_or_else(|e| panic!("start failed: {e}"))
}

fn call(server: &Server, tool: &str, arguments: Value) -> ToolOutcome {
    call_within(server, tool, arguments, Duration::from_secs(20))
}

fn call_within(server: &Server, tool: &str, arguments: Value, limit: Duration) -> ToolOutcome {
    server.call(
        &format!("focus__{tool}"),
        &arguments.to_string(),
        &Cancel::new(),
        limit,
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

fn ok_or_error(outcome: ToolOutcome) -> String {
    match outcome {
        ToolOutcome::Ok(text) | ToolOutcome::Error(text) => text,
        ToolOutcome::Denied => "denied".into(),
    }
}

fn tool_names(server: &Server) -> Vec<String> {
    server
        .tools()
        .into_iter()
        .map(|t| t.name.trim_start_matches("focus__").to_string())
        .collect()
}

#[test]
fn read_only_server_lists_and_runs_every_read_tool() {
    let w = workspace("read");
    let server = start(&w, &[]);
    assert_eq!(server.protocol_version(), "2026-07-28");
    assert!(server.instructions().unwrap_or("").contains("read-only"));
    let names = tool_names(&server);
    for tool in [
        "search_notes",
        "read_note",
        "list_folder",
        "get_backlinks",
        "get_outline",
        "list_tags",
        "get_neighbors",
        "list_tasks",
        "shortest_path",
        "most_linked",
        "orphan_notes",
        "list_attachments",
        "orphan_attachments",
        "app_current_note",
        "app_open_note",
        "app_show_search",
        "app_show_graph",
        "app_notify",
    ] {
        assert!(
            names.contains(&tool.to_string()),
            "missing tool {tool}: {names:?}"
        );
    }
    for tool in [
        "create_note",
        "append_to_note",
        "replace_in_note",
        "toggle_task",
        "rename_note",
        "trash_note",
        "export_note",
        "app_insert_at_cursor",
    ] {
        assert!(
            !names.contains(&tool.to_string()),
            "{tool} must not be exposed read-only"
        );
    }
    assert!(
        !names.iter().any(|n| n.starts_with("ops_")),
        "no engagements folder, no ops tools"
    );
    assert!(names.iter().all(|n| {
        server
            .tools()
            .iter()
            .any(|t| t.name == format!("focus__{n}"))
            && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }));

    let search = ok(call(
        &server,
        "search_notes",
        json!({ "query": "kerberoasting", "limit": 5 }),
    ));
    assert!(
        search.contains("Red Team/Kerberoasting.md: Kerberoasting"),
        "{search}"
    );
    let read = ok(call(
        &server,
        "read_note",
        json!({ "path": "Red Team/Kerberoasting" }),
    ));
    assert!(
        read.starts_with("Red Team/Kerberoasting.md: lines 1-") && read.contains("# Kerberoasting"),
        "{read}"
    );
    assert!(
        ok(call(
            &server,
            "read_note",
            json!({ "path": "Red Team/Kerberoasting.md", "start_line": 9, "end_line": 9 })
        ))
        .ends_with("9: # Kerberoasting\n")
    );
    let folder = ok(call(&server, "list_folder", json!({ "path": "" })));
    assert!(
        folder.contains("Red Team/") && folder.contains("00 Accueil.md: Accueil"),
        "{folder}"
    );
    assert!(
        ok(call(&server, "list_folder", json!({ "path": "Red Team" })))
            .contains("Red Team/Kerberoasting.md")
    );
    let backlinks = ok(call(
        &server,
        "get_backlinks",
        json!({ "path": "Red Team/Kerberoasting.md" }),
    ));
    assert!(backlinks.contains("00 Accueil.md"), "{backlinks}");
    let outline = ok(call(
        &server,
        "get_outline",
        json!({ "path": "Red Team/Kerberoasting.md" }),
    ));
    assert!(outline.contains("# Kerberoasting"), "{outline}");
    let tags = ok(call(&server, "list_tags", json!({})));
    assert!(
        tags.contains("#red-team") && tags.contains("#ad/kerberos"),
        "{tags}"
    );
    let neighbors = ok(call(
        &server,
        "get_neighbors",
        json!({ "path": "00 Accueil.md" }),
    ));
    assert!(
        neighbors.contains("Outgoing links")
            && neighbors.contains("Note absente: unresolved")
            && neighbors.contains("Incoming links"),
        "{neighbors}"
    );
    assert!(
        ok(call(
            &server,
            "get_neighbors",
            json!({ "path": "00 Accueil.md", "depth": 2 })
        ))
        .contains("Depth 1")
    );
    let tasks = ok(call(&server, "list_tasks", json!({})));
    assert!(
        tasks.contains("00 Accueil.md:")
            && tasks.contains("[ ] Vérifier le rendu en thème Blanc")
            && !tasks.contains("Écrire les notes d'essai"),
        "{tasks}"
    );
    assert!(
        ok(call(&server, "list_tasks", json!({ "include_done": true })))
            .contains("[x] Écrire les notes d'essai")
    );
    let path = ok(call(
        &server,
        "shortest_path",
        json!({ "from": "Red Team/Kerberoasting.md", "to": "00 Accueil.md" }),
    ));
    assert!(path.starts_with("Shortest path: "), "{path}");
    assert!(
        ok(call(&server, "most_linked", json!({ "limit": 3 })))
            .starts_with("Most referenced notes")
    );
    assert!(ok(call(&server, "orphan_notes", json!({}))).len() > 5);
    assert!(
        ok(call(
            &server,
            "list_attachments",
            json!({ "path": "00 Accueil.md" })
        ))
        .contains("references no attachments")
    );
    assert!(ok(call(&server, "orphan_attachments", json!({}))).len() > 5);
    let app = ok(call(&server, "app_current_note", json!({})));
    assert!(
        app.contains("not open") && app.contains("no action"),
        "{app}"
    );
}

#[test]
fn refusals_come_back_as_readable_errors() {
    let w = workspace("refusals");
    let server = start(&w, &[]);
    for bad in [
        "../outside.md",
        "/etc/passwd",
        "Red Team/../../x.md",
        "~/x.md",
    ] {
        let message = error(call(&server, "read_note", json!({ "path": bad })));
        assert!(message.contains("outside the vault"), "{bad}: {message}");
    }
    assert!(
        error(call(&server, "read_note", json!({ "path": "Absent note" })))
            .contains("note not found")
    );
    assert!(
        error(call(&server, "search_notes", json!({ "query": "/(/" }))).contains("invalid query")
    );
    assert!(
        error(call(&server, "list_folder", json!({ "path": "../.." })))
            .contains("outside the vault")
    );
    assert!(
        error(call(
            &server,
            "create_note",
            json!({ "path": "New.md", "content": "x" })
        ))
        .contains("unknown tool")
    );
    assert!(!w.vault.join("New.md").exists());
}

#[test]
fn write_mode_writes_to_disk_and_the_index_follows() {
    let w = workspace("write");
    let server = start(&w, &["--write"]);
    let names = tool_names(&server);
    for tool in [
        "create_note",
        "append_to_note",
        "replace_in_note",
        "toggle_task",
        "rename_note",
        "trash_note",
        "app_insert_at_cursor",
    ] {
        assert!(names.contains(&tool.to_string()), "{tool}");
    }
    let export_usable = Command::new(BINARY)
        .arg("--probe-export")
        .output()
        .unwrap()
        .status
        .success();
    assert_eq!(
        names.contains(&"export_note".to_string()),
        export_usable,
        "export_note is listed exactly when focus-md can export"
    );
    assert!(ok(call(&server, "create_note", json!({ "path": "Agent/Summary", "content": "# Summary\n\nSee [[Kerberoasting]].\n\n- [ ] check the summary\n" }))).starts_with("Note created: Agent/Summary.md"));
    assert_eq!(
        std::fs::read_to_string(w.vault.join("Agent/Summary.md")).unwrap(),
        "# Summary\n\nSee [[Kerberoasting]].\n\n- [ ] check the summary\n"
    );
    assert!(
        ok(call(
            &server,
            "append_to_note",
            json!({ "path": "Agent/Summary", "content": "Appended line." })
        ))
        .starts_with("Appended to Agent/Summary.md")
    );
    assert!(ok(call(&server, "replace_in_note", json!({ "path": "Agent/Summary.md", "old_text": "Appended line.", "new_text": "Replaced line." }))).contains("at line 6"));
    assert!(
        std::fs::read_to_string(w.vault.join("Agent/Summary.md"))
            .unwrap()
            .ends_with("Replaced line.\n")
    );
    assert!(
        error(call(
            &server,
            "replace_in_note",
            json!({ "path": "Agent/Summary.md", "old_text": "absent text", "new_text": "x" })
        ))
        .contains("not found")
    );
    assert!(
        ok(call(
            &server,
            "toggle_task",
            json!({ "path": "Agent/Summary.md", "line": 5 })
        ))
        .starts_with("Task checked")
    );
    assert!(
        std::fs::read_to_string(w.vault.join("Agent/Summary.md"))
            .unwrap()
            .contains("- [x] check the summary")
    );
    let backlinks = ok(call(
        &server,
        "get_backlinks",
        json!({ "path": "Red Team/Kerberoasting.md" }),
    ));
    assert!(backlinks.contains("Agent/Summary.md"), "{backlinks}");
    let renamed = ok(call(
        &server,
        "rename_note",
        json!({ "path": "Red Team/Kerberoasting.md", "new_path": "Red Team/Kerberoast attack.md" }),
    ));
    assert!(
        renamed.contains("link(s) rewritten") && renamed.contains("00 Accueil.md"),
        "{renamed}"
    );
    assert!(
        std::fs::read_to_string(w.vault.join("Agent/Summary.md"))
            .unwrap()
            .contains("Kerberoast attack")
    );
    assert!(
        ok(call(
            &server,
            "trash_note",
            json!({ "path": "Agent/Summary.md" })
        ))
        .contains("Moved to trash")
    );
    assert!(!w.vault.join("Agent/Summary.md").exists() && w.vault.join(".trash").exists());
    for bad in ["../escape.md", ".obsidian/x.md", ".git/x.md"] {
        let message = error(call(
            &server,
            "create_note",
            json!({ "path": bad, "content": "x" }),
        ));
        assert!(
            message.contains("outside the vault") || message.contains("reserved"),
            "{bad}: {message}"
        );
    }
    assert!(!w.vault.parent().unwrap().join("escape.md").exists());
}

#[test]
fn write_mode_keeps_a_version_of_the_note_it_replaces() {
    let w = workspace("history");
    let server = start(&w, &["--write"]);
    ok(call(
        &server,
        "create_note",
        json!({ "path": "Agent/Draft", "content": "first text\n" }),
    ));
    ok(call(
        &server,
        "replace_in_note",
        json!({ "path": "Agent/Draft.md", "old_text": "first", "new_text": "second" }),
    ));
    assert_eq!(
        std::fs::read_to_string(w.vault.join("Agent/Draft.md")).unwrap(),
        "second text\n"
    );
    let stores: Vec<PathBuf> = std::fs::read_dir(w.data.join("history"))
        .expect("the write server created the history of the vault")
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(stores.len(), 1);
    let history = focus_history::History::open(&stores[0], Default::default()).unwrap();
    let note = Path::new("Agent/Draft.md");
    let versions = history.list(note).unwrap();
    assert_eq!(
        versions.len(),
        1,
        "creation keeps nothing, the replace keeps one"
    );
    assert_eq!(history.read(note, &versions[0].id).unwrap(), "first text\n");
}

#[test]
fn write_mode_keeps_a_version_of_an_engagement_file_and_of_report_md() {
    let w = workspace("history-ops");
    let engagements = w.vault.join("Engagements");
    std::fs::create_dir_all(&engagements).unwrap();
    focus_ops::create(
        &engagements,
        &focus_ops::NewEngagement {
            name: "Demo".into(),
            kind: focus_ops::EngagementKind::Lab,
            client: None,
            scope: vec![],
            exam: None,
            checklist: None,
        },
    )
    .unwrap();
    let credentials = w.vault.join("Engagements/Demo/Loot/Credentials.md");
    let before = std::fs::read_to_string(&credentials).unwrap();
    let server = start(&w, &["--write"]);
    ok(call(
        &server,
        "ops_credential_add",
        json!({ "engagement": "Demo", "user": "alice", "kind": "password", "secret": "Hunter2-secret" }),
    ));
    for _ in 0..2 {
        ok(call(
            &server,
            "ops_report_build",
            json!({ "engagement": "Demo", "save": true }),
        ));
    }
    let stores: Vec<PathBuf> = std::fs::read_dir(w.data.join("history"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    let history = focus_history::History::open(&stores[0], Default::default()).unwrap();
    let texts = |rel: &str| -> Vec<String> {
        let rel = Path::new(rel);
        history
            .list(rel)
            .unwrap()
            .iter()
            .map(|v| history.read(rel, &v.id).unwrap())
            .collect()
    };
    assert_eq!(texts("Engagements/Demo/Loot/Credentials.md"), [before]);
    let report = texts("Engagements/Demo/Report.md");
    assert_eq!(report.len(), 1, "the second build replaced the first");
    assert!(report[0].contains("Demo"));
}

#[test]
fn read_only_mode_creates_no_history() {
    let w = workspace("history-read-only");
    let server = start(&w, &[]);
    ok(call(
        &server,
        "read_note",
        json!({ "path": "00 Accueil.md" }),
    ));
    assert!(!w.data.join("history").exists());
}

#[test]
fn changes_made_outside_the_server_reach_the_index() {
    let w = workspace("watch");
    let server = start(&w, &[]);
    assert!(
        ok(call(
            &server,
            "search_notes",
            json!({ "query": "zebracrossing" })
        ))
        .starts_with("No note matches")
    );
    std::fs::write(
        w.vault.join("Written elsewhere.md"),
        "# Elsewhere\n\nA zebracrossing appears here.\n",
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let result = ok(call(
            &server,
            "search_notes",
            json!({ "query": "zebracrossing" }),
        ));
        if result.contains("Written elsewhere.md") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the new note never reached the index: {result}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    std::fs::remove_file(w.vault.join("Written elsewhere.md")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while ok(call(
        &server,
        "search_notes",
        json!({ "query": "zebracrossing" }),
    ))
    .contains("Written elsewhere.md")
    {
        assert!(
            Instant::now() < deadline,
            "the removed note stayed in the index"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn ops_tools_appear_with_the_engagements_folder() {
    let w = workspace("ops");
    let engagements = w.vault.join("Engagements");
    std::fs::create_dir_all(&engagements).unwrap();
    focus_ops::create(
        &engagements,
        &focus_ops::NewEngagement {
            name: "Demo".into(),
            kind: focus_ops::EngagementKind::Lab,
            client: None,
            scope: vec![],
            exam: None,
            checklist: None,
        },
    )
    .unwrap();
    let server = start(&w, &["--write"]);
    assert_eq!(
        tool_names(&server)
            .iter()
            .filter(|n| n.starts_with("ops_"))
            .count(),
        16
    );
    assert!(ok(call(&server, "ops_list_engagements", json!({}))).contains("Demo"));
    assert!(
        ok(call(
            &server,
            "ops_journal_append",
            json!({ "engagement": "Demo", "text": "first line" })
        ))
        .contains("first line")
    );
    assert!(
        ok(call(
            &server,
            "ops_journal_read",
            json!({ "engagement": "Demo" })
        ))
        .contains("first line")
    );
    ok(call(
        &server,
        "ops_credential_add",
        json!({ "engagement": "Demo", "user": "alice", "kind": "password", "secret": "Hunter2-secret" }),
    ));
    let masked = ok(call(
        &server,
        "ops_credentials_list",
        json!({ "engagement": "Demo" }),
    ));
    assert!(!masked.contains("Hunter2-secret"), "{masked}");
    let revealing = start(&w, &["--reveal-secrets"]);
    assert!(
        ok(call(
            &revealing,
            "ops_credentials_list",
            json!({ "engagement": "Demo" })
        ))
        .contains("Hunter2-secret")
    );
}

struct Raw {
    child: std::process::Child,
    input: std::process::ChildStdin,
    output: BufReader<std::process::ChildStdout>,
}

fn raw(workspace: &Workspace, extra: &[&str]) -> Raw {
    let mut child = Command::new(BINARY)
        .args(args(workspace, extra))
        .envs(environment(workspace))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = child.stdin.take().unwrap();
    let output = BufReader::new(child.stdout.take().unwrap());
    Raw {
        child,
        input,
        output,
    }
}

impl Raw {
    fn ask(&mut self, message: Value) -> Value {
        writeln!(self.input, "{message}").unwrap();
        self.read()
    }
    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| {
            panic!("stdout carried something else than protocol: {line:?} ({e})")
        })
    }
    fn finish(mut self) -> (bool, Vec<String>, String) {
        drop(self.input);
        let mut rest = Vec::new();
        let mut line = String::new();
        while self.output.read_line(&mut line).unwrap_or(0) > 0 {
            rest.push(line.trim().to_string());
            line.clear();
        }
        let status = self.child.wait().unwrap();
        let mut stderr = String::new();
        std::io::Read::read_to_string(&mut self.child.stderr.take().unwrap(), &mut stderr).unwrap();
        (status.success(), rest, stderr)
    }
}

fn meta() -> Value {
    json!({ "io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {} })
}

#[test]
fn raw_protocol_legacy_session_resources_prompts_and_clean_exit() {
    let w = workspace("raw");
    let mut s = raw(&w, &[]);
    let init = s.ask(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } } }));
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(
        init["result"]["capabilities"]["resources"].is_object()
            && init["result"]["capabilities"]["prompts"].is_object()
    );
    writeln!(
        s.input,
        "{}",
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
    )
    .unwrap();
    assert_eq!(
        s.ask(json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" }))["result"],
        json!({})
    );
    let list = s.ask(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }));
    assert!(
        list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["annotations"]["readOnlyHint"] == true),
        "read-only server lists only read tools"
    );

    let resources = s.ask(json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/list" }));
    let listed = resources["result"]["resources"].as_array().unwrap();
    assert_eq!(listed.len(), count_notes(&w.vault));
    let kerberoasting = listed
        .iter()
        .find(|r| r["name"] == "Kerberoasting")
        .unwrap();
    assert_eq!(
        kerberoasting["uri"],
        "focus://note/Red%20Team/Kerberoasting.md"
    );
    let read = s.ask(json!({ "jsonrpc": "2.0", "id": 5, "method": "resources/read", "params": { "uri": kerberoasting["uri"] } }));
    assert!(
        read["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("# Kerberoasting")
    );
    assert_eq!(read["result"]["contents"][0]["mimeType"], "text/markdown");
    let bad = s.ask(json!({ "jsonrpc": "2.0", "id": 6, "method": "resources/read", "params": { "uri": "focus://note/../../etc/passwd" } }));
    assert_eq!(bad["error"]["code"], -32602);

    let prompts = s.ask(json!({ "jsonrpc": "2.0", "id": 7, "method": "prompts/list" }));
    let names: Vec<&str> = prompts["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"summarize") && names.contains(&"refine"),
        "{names:?}"
    );
    let prompt = s.ask(json!({ "jsonrpc": "2.0", "id": 8, "method": "prompts/get", "params": { "name": "summarize", "arguments": { "path": "Red Team/Kerberoasting.md" } } }));
    assert_eq!(prompt["result"]["messages"][0]["role"], "user");
    assert!(
        prompt["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("Kerberoasting")
    );

    let refused = s.ask(json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": { "name": "create_note", "arguments": { "path": "x.md", "content": "x" } } }));
    assert_eq!(
        refused["error"]["code"], -32602,
        "write tool in read-only mode"
    );
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Unknown tool")
    );
    assert!(!w.vault.join("x.md").exists());

    let stateless = s.ask(json!({ "jsonrpc": "2.0", "id": 10, "method": "server/discover", "params": { "_meta": meta() } }));
    assert_eq!(stateless["result"]["supportedVersions"][0], "2026-07-28");
    let (success, rest, stderr) = s.finish();
    assert!(success, "exit code after closing stdin: {stderr}");
    assert!(rest.is_empty(), "nothing but protocol on stdout: {rest:?}");
    assert!(
        stderr.contains("focus-mcp: ") && stderr.contains("notes, ready in"),
        "{stderr}"
    );
}

#[test]
fn help_states_the_write_rule_and_bad_arguments_fail_cleanly() {
    let help = Command::new(BINARY).arg("--help").output().unwrap();
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(
        help.status.success()
            && text.contains("--write")
            && text.contains("WITHOUT any confirmation")
            && text.contains("read-only"),
        "{text}"
    );
    let missing = Command::new(BINARY).output().unwrap();
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--vault is required"));
    let unknown = Command::new(BINARY)
        .args(["--vault", "/nonexistent-vault-path"])
        .output()
        .unwrap();
    assert_eq!(unknown.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("does not exist"));
    assert_eq!(
        Command::new(BINARY)
            .arg("--nope")
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
}

#[test]
#[ignore]
fn private_vault_startup_and_queries() {
    let Ok(vault) = std::env::var("FOCUS_REAL_VAULT") else {
        return;
    };
    let cache = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))
        .unwrap()
        .join("test-tmp/mcp-private-cache");
    let ws = Workspace {
        vault: PathBuf::from(vault),
        cache: cache.clone(),
        data: cache.join("data"),
        config: cache.join("config"),
    };
    for round in 0..3 {
        let started = Instant::now();
        let mut s = raw(&ws, &[]);
        s.ask(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }));
        let initialized = started.elapsed();
        let t = Instant::now();
        let search = s.ask(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "search_notes", "arguments": { "query": "kerberoasting" } } }));
        let search_time = t.elapsed();
        let t = Instant::now();
        s.ask(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "read_note", "arguments": { "path": "00_Home.md" } } }));
        println!(
            "round {round}: initialize {initialized:?}, search {search_time:?} ({}), read {:?}",
            search["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .lines()
                .next()
                .unwrap(),
            t.elapsed()
        );
        s.finish();
        if round > 0 {
            assert!(
                initialized < Duration::from_millis(100),
                "startup {initialized:?}"
            );
        }
    }
}

#[test]
fn export_note_writes_html_markdown_and_pdf() {
    let w = workspace("export");
    let server = start(&w, &["--write"]);
    assert!(
        tool_names(&server).contains(&"export_note".to_string()),
        "focus-md can export, so the tool must be listed"
    );

    let html = ok(call(
        &server,
        "export_note",
        json!({ "path": "Red Team/Kerberoasting.md", "format": "html" }),
    ));
    assert!(
        html.starts_with("Exported: ") && html.contains("Export/Kerberoasting.html"),
        "{html}"
    );
    let page = std::fs::read_to_string(w.vault.join("Export/Kerberoasting.html")).unwrap();
    assert!(
        page.contains("<html") && page.contains("Kerberoasting") && page.contains("hashcat"),
        "standalone page with the note content"
    );

    let diagrams = ok(call(
        &server,
        "export_note",
        json!({ "path": "Rendu/Diagrammes.md", "format": "html", "destination": "diagrams.html" }),
    ));
    assert!(diagrams.contains("Export/diagrams.html"), "{diagrams}");
    assert!(
        std::fs::read_to_string(w.vault.join("Export/diagrams.html"))
            .unwrap()
            .contains("<svg"),
        "Mermaid diagrams are rendered inline"
    );

    let everything = ok(call(
        &server,
        "export_note",
        json!({ "path": "Rendu/Tout le Markdown.md", "format": "html", "destination": "everything.html" }),
    ));
    assert!(everything.starts_with("Exported: "), "{everything}");
    assert!(
        std::fs::read_to_string(w.vault.join("Export/everything.html"))
            .unwrap()
            .contains("data:image/"),
        "local images are embedded"
    );

    let markdown = ok(call(
        &server,
        "export_note",
        json!({ "path": "00 Accueil.md", "format": "markdown", "destination": "home.md" }),
    ));
    assert!(markdown.starts_with("Exported: "), "{markdown}");
    let text = std::fs::read_to_string(w.vault.join("Export/home.md")).unwrap();
    assert!(
        !text.contains("[[") && text.contains("Kerberoasting"),
        "wiki links are converted: {text}"
    );

    let outside = w.vault.parent().unwrap().join("out");
    std::fs::create_dir_all(&outside).unwrap();
    let destination = outside.join("kerberoasting.pdf");
    let elsewhere = ok_or_error(call(
        &server,
        "export_note",
        json!({ "path": "Red Team/Kerberoasting.md", "format": "pdf", "destination": destination.display().to_string() }),
    ));
    assert!(
        elsewhere.contains("outside the export folder") && !destination.exists(),
        "an agent cannot pick a place outside the export folder: {elsewhere}"
    );
    let server = start(
        &w,
        &["--write", "--export-dir", &outside.display().to_string()],
    );
    let printed = call_within(
        &server,
        "export_note",
        json!({ "path": "Red Team/Kerberoasting.md", "format": "pdf", "destination": destination.display().to_string() }),
        Duration::from_secs(150),
    );
    if !focus_md::export::pdf_available() {
        let refused = error(printed);
        assert!(refused.contains("no Chromium browser"), "{refused}");
        return;
    }
    let pdf = ok(printed);
    assert!(pdf.starts_with("Exported: "), "{pdf}");
    let bytes = std::fs::read(&destination).unwrap();
    assert!(
        bytes.starts_with(b"%PDF") && bytes.len() > 2000,
        "a real PDF: {} bytes",
        bytes.len()
    );
    assert!(
        !outside.join("kerberoasting.html.focus-tmp").exists()
            && !outside.join("kerberoasting.pdf.html.focus-tmp").exists(),
        "no temporary file left behind"
    );

    let default_folder = start(&w, &["--write"]);
    assert!(
        error(call(
            &default_folder,
            "export_note",
            json!({ "path": "Red Team/Kerberoasting.md", "format": "html" })
        ))
        .contains("already exists")
    );
}

fn files_under(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.insert(path.clone(), Vec::new());
                walk(&path, out);
            } else {
                out.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, &mut out);
    out
}

fn fault(text: &str) -> Value {
    let value: Value = serde_json::from_str(text).unwrap_or_else(|e| panic!("{text}: {e}"));
    assert!(
        value["code"].is_string() && value["message"].is_string(),
        "{text}"
    );
    for issue in value["issues"].as_array().unwrap() {
        for key in ["path", "message", "hint"] {
            assert!(issue[key].is_string(), "{key} of {text}");
        }
    }
    value
}

#[test]
fn write_rules_hold_through_the_server_binary() {
    let w = workspace("write-rules");
    let server = start(&w, &["--write"]);
    let note = "Red Team/Kerberoasting.md";
    let kept = |root: &Path| -> BTreeMap<PathBuf, Vec<u8>> {
        files_under(root)
            .into_iter()
            .filter(|(path, _)| path.file_name().is_none_or(|name| name != "lock"))
            .collect()
    };
    let before_vault = files_under(&w.vault);
    let before_data = kept(&w.data);

    for (tool, arguments) in [
        (
            "append_to_note",
            json!({ "path": note, "content": "Added line" }),
        ),
        (
            "replace_in_note",
            json!({ "path": note, "old_text": "Kerberoasting", "new_text": "Roasting" }),
        ),
        (
            "create_note",
            json!({ "path": "Agent/New", "content": "# New\n" }),
        ),
        (
            "rename_note",
            json!({ "path": note, "new_path": "Red Team/Roasting.md" }),
        ),
        ("trash_note", json!({ "path": note })),
    ] {
        let mut arguments = arguments;
        arguments["dry_run"] = json!(true);
        let answer = ok_or_error(call(&server, tool, arguments));
        assert!(
            answer.starts_with("Dry run: ") || answer.contains("\"code\""),
            "{tool}: {answer}"
        );
    }
    assert_eq!(
        files_under(&w.vault),
        before_vault,
        "a dry run changed the vault"
    );
    assert_eq!(
        kept(&w.data),
        before_data,
        "a dry run kept a version or created a file"
    );

    let read = ok(call(&server, "read_note", json!({ "path": note })));
    let at = read.find("(etag ").expect("read_note gives an etag") + "(etag ".len();
    let etag = read[at..at + 12].to_string();
    let first = ok(call(
        &server,
        "append_to_note",
        json!({ "path": note, "content": "Second line", "if_match": etag, "idempotency_key": "e2e-1" }),
    ));
    assert!(first.contains("\nEtag: "), "{first}");
    let replay = ok(call(
        &server,
        "append_to_note",
        json!({ "path": note, "content": "Second line", "if_match": etag, "idempotency_key": "e2e-1" }),
    ));
    assert_eq!(first, replay, "a repeated key answers the first result");
    assert_eq!(
        std::fs::read_to_string(w.vault.join(note))
            .unwrap()
            .matches("Second line")
            .count(),
        1
    );
    let stale = fault(&error(call(
        &server,
        "append_to_note",
        json!({ "path": note, "content": "Third line", "if_match": etag }),
    )));
    assert_eq!(stale["code"], "stale");
    assert!(
        !std::fs::read_to_string(w.vault.join(note))
            .unwrap()
            .contains("Third line")
    );

    assert_eq!(
        fault(&error(call(&server, "read_note", json!({}))))["code"],
        "invalid_argument"
    );
    assert_eq!(
        fault(&error(call(
            &server,
            "read_note",
            json!({ "path": "../outside.md" })
        )))["code"],
        "path_refused"
    );
    let page = ok(call(
        &server,
        "list_folder",
        json!({ "path": "", "limit": 1 }),
    ));
    assert!(
        page.contains("more entries not shown: send offset=1"),
        "{page}"
    );
}
