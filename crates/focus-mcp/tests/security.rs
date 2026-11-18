use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use focus_ai::Cancel;
use focus_ai::agent::ToolOutcome;
use focus_ai::mcp::{Server, ServerConfig, StartOptions};
use serde_json::{Value, json};

const BINARY: &str = env!("CARGO_BIN_EXE_focus-mcp");
const SECRET: &str = "Hunter2-Corp-Secret-9";

struct World {
    base: PathBuf,
    vault: PathBuf,
    outside: PathBuf,
}

fn world(name: &str) -> World {
    let base = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))
        .unwrap()
        .join("test-tmp")
        .join(format!("mcp-security-{name}"));
    let _ = std::fs::remove_dir_all(&base);
    let vault = base.join("vault");
    let outside = base.join("outside");
    std::fs::create_dir_all(vault.join("Notes")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(vault.join("Notes/Inside.md"), "# Inside\n\n- [ ] a task\n").unwrap();
    std::fs::write(
        outside.join("secret.md"),
        "# Outside secret\nthe outside text\n",
    )
    .unwrap();
    std::fs::write(outside.join("picture.png"), b"OUTSIDE-IMAGE-BYTES").unwrap();
    std::os::unix::fs::symlink(outside.join("secret.md"), vault.join("Notes/link.md")).unwrap();
    std::os::unix::fs::symlink(&outside, vault.join("Notes/linked-folder")).unwrap();
    World {
        base: base.clone(),
        vault,
        outside,
    }
}

fn start(world: &World, extra: &[&str]) -> Server {
    let mut args = vec![
        "--vault".to_string(),
        world.vault.display().to_string(),
        "--cache-dir".to_string(),
        world.base.join("cache").display().to_string(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    let config = ServerConfig {
        name: "focus".into(),
        command: BINARY.into(),
        args,
        env: BTreeMap::from([
            (
                "FOCUS_DATA_DIR".to_string(),
                world.base.join("data").display().to_string(),
            ),
            (
                "FOCUS_CONFIG_DIR".to_string(),
                world.base.join("config").display().to_string(),
            ),
        ]),
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
    server.call(
        &format!("focus__{tool}"),
        &arguments.to_string(),
        &Cancel::new(),
        Duration::from_secs(20),
    )
}

fn text(outcome: ToolOutcome) -> String {
    match outcome {
        ToolOutcome::Ok(t) | ToolOutcome::Error(t) => t,
        ToolOutcome::Denied => "denied".into(),
    }
}

fn is_ok(outcome: &ToolOutcome) -> bool {
    matches!(outcome, ToolOutcome::Ok(_))
}

fn tools(server: &Server) -> Vec<String> {
    server
        .tools()
        .into_iter()
        .map(|t| t.name.trim_start_matches("focus__").to_string())
        .collect()
}

fn hostile_paths(world: &World) -> Vec<String> {
    vec![
        "../outside/secret.md".into(),
        "Notes/../../outside/secret.md".into(),
        world.outside.join("secret.md").display().to_string(),
        "/etc/passwd".into(),
        "~/.ssh/id_rsa".into(),
        "Notes/link.md".into(),
        "Notes/linked-folder/secret.md".into(),
        "Notes/linked-folder".into(),
    ]
}

#[test]
fn no_read_tool_returns_what_lies_outside_the_vault() {
    let w = world("read");
    let server = start(&w, &[]);
    for path in hostile_paths(&w) {
        for tool in [
            "read_note",
            "get_outline",
            "get_backlinks",
            "get_neighbors",
            "list_attachments",
            "list_folder",
        ] {
            let outcome = call(&server, tool, json!({ "path": path }));
            let output = text(outcome);
            assert!(
                !output.contains("the outside text") && !output.contains("Outside secret"),
                "{tool} {path}: {output}"
            );
        }
    }
    let found = text(call(&server, "search_notes", json!({ "query": "outside" })));
    assert!(
        !found.contains("Outside secret"),
        "a note reached through a symbolic link is not searched: {found}"
    );
    assert!(found.starts_with("No note matches"), "nor counted: {found}");
}

#[test]
fn nothing_writes_without_the_write_flag() {
    let w = world("readonly");
    let server = start(&w, &[]);
    let listed = tools(&server);
    for tool in [
        "create_note",
        "append_to_note",
        "replace_in_note",
        "toggle_task",
        "rename_note",
        "trash_note",
        "export_note",
        "app_insert_at_cursor",
        "ops_journal_append",
        "ops_import_nmap",
        "ops_finding_create",
        "ops_credential_add",
        "ops_vars_set",
        "ops_set_focus",
    ] {
        assert!(
            !listed.contains(&tool.to_string()),
            "{tool} is listed on a read-only server"
        );
        let outcome = call(
            &server,
            tool,
            json!({ "path": "New.md", "content": "x", "text": "x", "engagement": "x" }),
        );
        assert!(!is_ok(&outcome), "{tool} ran on a read-only server");
    }
    assert!(!w.vault.join("New.md").exists());
}

#[test]
fn no_write_tool_reaches_outside_the_vault() {
    let w = world("write");
    let server = start(&w, &["--write"]);
    for path in hostile_paths(&w) {
        let attempts = [
            ("create_note", json!({ "path": path, "content": "planted" })),
            (
                "append_to_note",
                json!({ "path": path, "content": "planted" }),
            ),
            (
                "replace_in_note",
                json!({ "path": path, "old_text": "the outside text", "new_text": "planted" }),
            ),
            ("toggle_task", json!({ "path": path, "line": 1 })),
            ("trash_note", json!({ "path": path })),
            (
                "rename_note",
                json!({ "path": path, "new_path": "Notes/Moved.md" }),
            ),
            (
                "rename_note",
                json!({ "path": "Notes/Inside.md", "new_path": path }),
            ),
        ];
        for (tool, arguments) in attempts {
            let outcome = call(&server, tool, arguments.clone());
            if path == "Notes/linked-folder" {
                continue;
            }
            assert!(!is_ok(&outcome), "{tool} {arguments}: {}", text(outcome));
        }
    }
    assert_eq!(
        std::fs::read_to_string(w.outside.join("secret.md")).unwrap(),
        "# Outside secret\nthe outside text\n"
    );
    assert!(w.outside.join("picture.png").exists());
    assert!(!w.outside.join("secret.md.bak").exists());
    let leftovers: Vec<_> = std::fs::read_dir(&w.outside)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers.len(), 2, "{leftovers:?}");
}

#[test]
fn an_export_goes_to_the_export_folder_and_nowhere_else() {
    let w = world("export");
    let server = start(&w, &["--write"]);
    if !tools(&server).contains(&"export_note".to_string()) {
        return;
    }
    let target = w.outside.join("planted.html");
    for destination in [
        target.display().to_string(),
        "../outside/planted.html".into(),
        "/tmp/focus-planted.html".into(),
    ] {
        let outcome = call(
            &server,
            "export_note",
            json!({ "path": "Notes/Inside.md", "format": "html", "destination": destination }),
        );
        assert!(!is_ok(&outcome), "{destination}: {}", text(outcome));
    }
    assert!(!target.exists());
    assert!(!Path::new("/tmp/focus-planted.html").exists());
    let inside = call(
        &server,
        "export_note",
        json!({ "path": "Notes/Inside.md", "format": "html" }),
    );
    assert!(is_ok(&inside), "{}", text(inside));
    assert!(w.vault.join("Export/Inside.html").exists());
}

#[test]
fn an_export_does_not_embed_files_from_outside_the_vault() {
    let w = world("embed");
    std::fs::write(w.vault.join("Notes/Pictures.md"), "# Pictures\n\n![a](../../outside/picture.png)\n\n![b](Notes/linked-folder/picture.png)\n\n![c](linked-folder/picture.png)\n").unwrap();
    let server = start(&w, &["--write"]);
    if !tools(&server).contains(&"export_note".to_string()) {
        return;
    }
    let outcome = call(
        &server,
        "export_note",
        json!({ "path": "Notes/Pictures.md", "format": "html" }),
    );
    assert!(is_ok(&outcome), "{}", text(outcome));
    let html = std::fs::read_to_string(w.vault.join("Export/Pictures.html")).unwrap();
    let encoded = base64_of(b"OUTSIDE-IMAGE-BYTES");
    assert!(
        !html.contains(&encoded) && !html.contains("OUTSIDE-IMAGE-BYTES"),
        "an image outside the vault was embedded"
    );
}

fn base64_of(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn engagement(world: &World) {
    let ops = world.vault.join("Engagements");
    std::fs::create_dir_all(&ops).unwrap();
    let created = focus_ops::create(
        &ops,
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
    focus_ops::loot::add(
        &created.dir,
        &focus_ops::Credential {
            user: "alice".into(),
            kind: "password".into(),
            secret: SECRET.into(),
            domain: "CORP".into(),
            source: "dump".into(),
            valid_on: String::new(),
            notes: String::new(),
        },
    )
    .unwrap();
    let mut loaded = focus_ops::load(&created.dir).unwrap();
    loaded.vars.insert("DB_PASSWORD".into(), SECRET.into());
    loaded.vars.insert("SERVICE_CRED".into(), SECRET.into());
    focus_ops::save(&loaded).unwrap();
    focus_ops::journal::append(
        &created.dir,
        None,
        &format!("found password={SECRET} on the share"),
    )
    .unwrap();
}

#[test]
fn engagement_secrets_stay_masked_everywhere_without_reveal_secrets() {
    let w = world("secrets");
    engagement(&w);
    let server = start(&w, &["--write"]);
    let outputs = [
        ("ops_credentials_list", json!({ "engagement": "Demo" })),
        ("ops_vars_get", json!({ "engagement": "Demo" })),
        ("ops_journal_read", json!({ "engagement": "Demo" })),
        (
            "ops_report_build",
            json!({ "engagement": "Demo", "include_credentials": true }),
        ),
        (
            "read_note",
            json!({ "path": "Engagements/Demo/Loot/Credentials.md" }),
        ),
        ("read_note", json!({ "path": "Engagements/Demo/README.md" })),
        ("search_notes", json!({ "query": "Hunter2" })),
        (
            "get_backlinks",
            json!({ "path": "Engagements/Demo/Loot/Credentials.md" }),
        ),
    ];
    for (tool, arguments) in outputs {
        let output = text(call(&server, tool, arguments.clone()));
        assert!(
            !output.contains(SECRET),
            "{tool} {arguments} gave a secret: {output}"
        );
    }
    let revealing = start(&w, &["--reveal-secrets"]);
    assert!(
        text(call(
            &revealing,
            "ops_credentials_list",
            json!({ "engagement": "Demo" })
        ))
        .contains(SECRET)
    );
}

#[test]
fn an_engagement_name_cannot_leave_the_engagements_folder() {
    let w = world("engagement-name");
    engagement(&w);
    std::os::unix::fs::symlink(&w.outside, w.vault.join("Engagements/Escape")).unwrap();
    std::fs::write(
        w.outside.join("README.md"),
        "---\ntype: lab\nstatus: planning\n---\n",
    )
    .unwrap();
    let server = start(&w, &["--write"]);
    for name in ["../Notes", "..", "Demo/../Demo", "/tmp", "Escape"] {
        let outcome = call(&server, "ops_overview", json!({ "engagement": name }));
        assert!(!is_ok(&outcome), "{name}: {}", text(outcome));
        let outcome = call(
            &server,
            "ops_journal_append",
            json!({ "engagement": name, "text": "planted" }),
        );
        assert!(!is_ok(&outcome), "{name}: {}", text(outcome));
    }
    assert!(!w.outside.join("Journal.md").exists());
}

#[test]
fn inputs_are_bounded() {
    let w = world("sizes");
    engagement(&w);
    let server = start(&w, &["--write"]);
    let big = "x".repeat(2_000_000);
    let outcome = call(
        &server,
        "create_note",
        json!({ "path": "Big.md", "content": big }),
    );
    assert!(
        !is_ok(&outcome) && !w.vault.join("Big.md").exists(),
        "a note over the limit was written"
    );
    let outcome = call(
        &server,
        "ops_journal_append",
        json!({ "engagement": "Demo", "text": big }),
    );
    assert!(!is_ok(&outcome), "a journal entry of 2 MB was accepted");
    let outcome = call(
        &server,
        "ops_import_nmap",
        json!({ "engagement": "Demo", "text": "y".repeat(6_000_000) }),
    );
    assert!(!is_ok(&outcome));
    let outcome = call(
        &server,
        "ops_credential_add",
        json!({ "engagement": "Demo", "user": "u", "kind": "password", "secret": big }),
    );
    assert!(!is_ok(&outcome), "a secret of 2 MB was accepted");
    let outcome = call(
        &server,
        "read_note",
        json!({ "path": "Notes/Inside.md", "max_chars": 4_000_000_000u64 }),
    );
    assert!(text(outcome).len() < 100_000);
    let outcome = call(
        &server,
        "search_notes",
        json!({ "query": "inside", "limit": 1_000_000 }),
    );
    assert!(text(outcome).len() < 200_000);
}

#[test]
fn an_oversized_message_ends_the_session_instead_of_filling_memory() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    let w = world("oversized");
    let mut child = Command::new(BINARY)
        .args([
            "--vault",
            &w.vault.display().to_string(),
            "--cache-dir",
            &w.base.join("cache").display().to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let writer = std::thread::spawn(move || {
        let chunk = vec![b'x'; 1024 * 1024];
        for _ in 0..18 {
            if input.write_all(&chunk).is_err() {
                break;
            }
        }
    });
    let mut reply = String::new();
    output.read_line(&mut reply).unwrap();
    assert!(reply.contains("message too long"), "{reply}");
    let status = child.wait().unwrap();
    assert!(status.success() || status.code().is_some());
    let _ = writer.join();
}

#[test]
fn credentials_created_after_the_start_are_guarded_too() {
    let w = world("late-engagements");
    let server = start(&w, &[]);
    let loot = w.vault.join("Engagements/Late/Loot");
    std::fs::create_dir_all(&loot).unwrap();
    std::fs::write(
        loot.join("Credentials.md"),
        format!("| alice | password | {SECRET} |\n"),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let outcome = text(call(
            &server,
            "read_note",
            json!({ "path": "Engagements/Late/Loot/Credentials.md" }),
        ));
        assert!(!outcome.contains(SECRET), "{outcome}");
        if outcome.contains("engagement secrets are not served") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the new note never reached the index: {outcome}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
