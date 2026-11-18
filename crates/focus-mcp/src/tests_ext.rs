use std::sync::{Arc, Mutex};

use focus_ai::agent::{self, ToolHost, ToolOutcome};
use serde_json::json;

use crate::VaultTools;
use crate::catalog::McpHost;
use crate::ipc::{self, Handler, ReferenceServer, Request, Response};
use crate::testvault::{self, error, ok, run};

fn writable(name: &str) -> (VaultTools, testvault::Fixture) {
    let f = testvault::open(name);
    (
        VaultTools::new(f.vault.clone(), f.index.clone())
            .with_extended_tools()
            .with_unconfirmed_writes(),
        f,
    )
}

fn names(tools: &dyn ToolHost) -> Vec<String> {
    tools.specs().into_iter().map(|s| s.name).collect()
}

#[test]
fn extended_tool_lists_follow_the_mode() {
    let f = testvault::open("lists-mode");
    let base = VaultTools::new(f.vault.clone(), f.index.clone());
    assert_eq!(names(&base).len(), 7);
    let read_only = VaultTools::new(f.vault.clone(), f.index.clone()).with_extended_tools();
    let list = names(&read_only);
    for expected in [
        "list_tasks",
        "list_attachments",
        "orphan_attachments",
        "shortest_path",
        "most_linked",
        "orphan_notes",
    ] {
        assert!(list.contains(&expected.to_string()), "{expected}");
    }
    for hidden in [
        "toggle_task",
        "rename_note",
        "trash_note",
        "export_note",
        "create_note",
    ] {
        assert!(
            !list.contains(&hidden.to_string()),
            "{hidden} must not be exposed when read-only"
        );
    }
    let rw = read_only.clone().with_unconfirmed_writes();
    assert!(!names(&rw.clone().with_export(false)).contains(&"export_note".to_string()));
    assert!(
        names(&rw).contains(&"rename_note".to_string())
            && names(&rw).contains(&"export_note".to_string())
    );
    assert!(
        !names(&rw)
            .iter()
            .any(|n| n.starts_with("ops_") || n.starts_with("app_"))
    );
    let rw_calls = testvault::error(run(
        &read_only,
        "rename_note",
        json!({ "path": "Home.md", "new_path": "X.md" }),
    ));
    assert!(rw_calls.contains("read-only"), "{rw_calls}");
    assert!(error(run(&read_only, "inexistant", json!({}))).contains("unknown tool"));
}

#[test]
fn tasks_list_and_toggle() {
    let (tools, f) = writable("tasks");
    let open = ok(run(&tools, "list_tasks", json!({})));
    assert!(
        open.starts_with("2 task(s)")
            && open.contains("Home.md:5 [ ] review the home page")
            && open.contains("Red Team/Kerberoasting.md:14 [ ] write the procedure"),
        "{open}"
    );
    assert!(!open.contains("create the vault"));
    let all = ok(run(&tools, "list_tasks", json!({ "include_done": true })));
    assert!(all.contains("[x] create the vault"), "{all}");
    assert!(
        ok(run(
            &tools,
            "list_tasks",
            json!({ "path": "Notes/Bridge.md" })
        ))
        .starts_with("No tasks")
    );
    assert!(
        ok(run(
            &tools,
            "toggle_task",
            json!({ "path": "Home.md", "line": 5 })
        ))
        .starts_with("Task checked: Home.md:5")
    );
    assert!(
        std::fs::read_to_string(f.root.join("Home.md"))
            .unwrap()
            .contains("- [x] review the home page")
    );
    assert!(
        ok(run(
            &tools,
            "toggle_task",
            json!({ "path": "Home.md", "line": 5 })
        ))
        .starts_with("Task unchecked")
    );
    assert!(
        error(run(
            &tools,
            "toggle_task",
            json!({ "path": "Home.md", "line": 1 })
        ))
        .contains("no task at line")
    );
    assert!(
        error(run(
            &tools,
            "toggle_task",
            json!({ "path": "Home.md", "line": 0 })
        ))
        .contains("`line`")
    );
}

#[test]
fn toggle_shows_the_changed_line_to_the_confirmation() {
    let f = testvault::open("tasks-confirm");
    let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let tools = VaultTools::new(f.vault.clone(), f.index.clone())
        .with_extended_tools()
        .with_confirmation(move |p, _| {
            log.lock()
                .unwrap()
                .push((p.before.unwrap_or("").to_string(), p.after.to_string()));
            false
        });
    assert!(matches!(
        run(
            &tools,
            "toggle_task",
            json!({ "path": "Home.md", "line": 5 })
        ),
        ToolOutcome::Denied
    ));
    let (before, after) = seen.lock().unwrap()[0].clone();
    assert!(
        before.contains("- [ ] review")
            && after.contains("- [x] review")
            && !after.contains("- [ ] review")
    );
    assert!(
        std::fs::read_to_string(f.root.join("Home.md"))
            .unwrap()
            .contains("- [ ] review"),
        "refusal: nothing is written"
    );
}

#[test]
fn rename_rewrites_links_and_trash_moves_to_the_bin() {
    let (tools, f) = writable("rename");
    let text = ok(run(
        &tools,
        "rename_note",
        json!({ "path": "Red Team/Defense.md", "new_path": "Blue Team/Hardening" }),
    ));
    assert!(
        text.starts_with("Renamed: from Red Team/Defense.md to Blue Team/Hardening.md"),
        "{text}"
    );
    assert!(
        text.contains("link(s) rewritten")
            && text.contains("Red Team/Kerberoasting.md")
            && text.contains("Notes/Bridge.md"),
        "{text}"
    );
    assert!(
        f.root.join("Blue Team/Hardening.md").exists()
            && !f.root.join("Red Team/Defense.md").exists()
    );
    assert!(
        std::fs::read_to_string(f.root.join("Notes/Bridge.md"))
            .unwrap()
            .contains("Hardening")
    );
    assert!(
        error(run(
            &tools,
            "rename_note",
            json!({ "path": "Home.md", "new_path": "Notes/Bridge.md" })
        ))
        .contains("already exists")
    );
    assert!(
        error(run(
            &tools,
            "rename_note",
            json!({ "path": "Home.md", "new_path": "../outside.md" })
        ))
        .contains("outside the vault")
    );
    assert!(
        error(run(
            &tools,
            "rename_note",
            json!({ "path": "Home.md", "new_path": ".obsidian/x.md" })
        ))
        .contains("reserved")
    );
    assert!(
        error(run(
            &tools,
            "rename_note",
            json!({ "path": "Absente.md", "new_path": "X.md" })
        ))
        .contains("not found")
    );
    assert!(
        ok(run(
            &tools,
            "trash_note",
            json!({ "path": "Notes/Isolated note.md" })
        ))
        .contains("trash")
    );
    assert!(!f.root.join("Notes/Isolated note.md").exists() && f.root.join(".trash").exists());
}

#[test]
fn attachments_are_listed_per_note_and_orphans_are_found() {
    let (tools, _f) = writable("attachments");
    assert!(
        ok(run(
            &tools,
            "list_attachments",
            json!({ "path": "Red Team/Kerberoasting.md" })
        ))
        .contains("Red Team/assets/capture.png")
    );
    assert!(
        ok(run(
            &tools,
            "list_attachments",
            json!({ "path": "Home.md" })
        ))
        .contains("references no attachments")
    );
    let orphans = ok(run(&tools, "orphan_attachments", json!({})));
    assert!(
        orphans.contains("orphan.png") && !orphans.contains("capture.png"),
        "{orphans}"
    );
}

#[test]
fn graph_tools() {
    let (tools, _f) = writable("graph");
    let depth2 = ok(run(
        &tools,
        "get_neighbors",
        json!({ "path": "Notes/Bridge.md", "depth": 2 }),
    ));
    assert!(
        depth2.contains("Depth 1 (2)")
            && depth2.contains("Home.md: Home (outgoing link)")
            && depth2.contains("Depth 2"),
        "{depth2}"
    );
    assert!(depth2.contains("Red Team/Kerberoasting.md"), "{depth2}");
    assert!(
        error(run(
            &tools,
            "get_neighbors",
            json!({ "path": "Notes/Bridge.md", "depth": 4 })
        ))
        .contains("`depth`")
    );
    assert!(
        ok(run(
            &tools,
            "get_neighbors",
            json!({ "path": "Notes/Isolated note.md", "depth": 3 })
        ))
        .contains("Depth 1 (1)"),
        "the home note points to this note"
    );

    let path = ok(run(
        &tools,
        "shortest_path",
        json!({ "from": "Notes/Bridge.md", "to": "Red Team/Kerberoasting.md" }),
    ));
    assert!(path.starts_with("Shortest path: 2 link(s)"), "{path}");
    assert!(path.contains("Notes/Bridge.md") && path.contains("Red Team/Kerberoasting.md"));
    assert!(
        ok(run(
            &tools,
            "shortest_path",
            json!({ "from": "Home.md", "to": "Home.md" })
        ))
        .contains("same note")
    );

    let linked = ok(run(&tools, "most_linked", json!({ "limit": 2 })));
    let first = linked.lines().nth(1).unwrap();
    assert!(first.starts_with("2: Red Team/Defense.md"), "{linked}");
    assert_eq!(
        linked.lines().count(),
        4,
        "two notes and the line for the rest"
    );

    let orphans = ok(run(&tools, "orphan_notes", json!({})));
    assert!(
        orphans.starts_with("No notes without links"),
        "the isolated note is linked from the home note: {orphans}"
    );
}

#[test]
fn graph_orphans_and_missing_paths() {
    let (tools, f) = writable("graph-orphans");
    ok(run(
        &tools,
        "create_note",
        json!({ "path": "Alone.md", "content": "# Alone\n\nNothing.\n" }),
    ));
    ok(run(
        &tools,
        "create_note",
        json!({ "path": "Other alone.md", "content": "# Other\n" }),
    ));
    let orphans = ok(run(&tools, "orphan_notes", json!({})));
    assert!(
        orphans.starts_with("2 note(s) without links")
            && orphans.contains("Alone.md: Alone")
            && orphans.contains("Other alone.md"),
        "{orphans}"
    );
    let none = ok(run(
        &tools,
        "shortest_path",
        json!({ "from": "Alone.md", "to": "Home.md" }),
    ));
    assert!(none.starts_with("No chain of links"), "{none}");
    assert!(
        error(run(
            &tools,
            "shortest_path",
            json!({ "from": "../x.md", "to": "Home.md" })
        ))
        .contains("outside the vault")
    );
    drop(f);
}

#[test]
fn export_destinations_are_confined_and_never_overwrite() {
    let (tools, f) = writable("export");
    let outside = f.root.parent().unwrap().join("sortie");
    std::fs::create_dir_all(&outside).unwrap();
    let inside = f.root.join("Notes/export.html").display().to_string();
    assert!(
        error(run(
            &tools,
            "export_note",
            json!({ "path": "Home.md", "format": "html", "destination": inside })
        ))
        .contains("outside the export folder")
    );
    assert!(
        error(run(
            &tools,
            "export_note",
            json!({ "path": "Home.md", "format": "html", "destination": "../escape.html" })
        ))
        .contains("outside the vault")
    );
    assert!(
        error(run(
            &tools,
            "export_note",
            json!({ "path": "Home.md", "format": "docx" })
        ))
        .contains("unknown format")
    );
    let elsewhere = outside.join("elsewhere.html").display().to_string();
    assert!(
        error(run(
            &tools,
            "export_note",
            json!({ "path": "Home.md", "format": "html", "destination": elsewhere })
        ))
        .contains("outside the export folder")
    );
    assert!(!outside.join("elsewhere.html").exists());
    assert!(
        !f.root.join("Export").exists() && !f.root.join("Notes/export.html").exists(),
        "a refusal creates nothing"
    );
    std::fs::create_dir_all(f.root.join("Export")).unwrap();
    std::fs::write(f.root.join("Export/deja.html"), "existant").unwrap();
    assert!(
        error(run(
            &tools,
            "export_note",
            json!({ "path": "Home.md", "format": "html", "destination": "deja.html" })
        ))
        .contains("already exists")
    );
    assert_eq!(
        std::fs::read_to_string(f.root.join("Export/deja.html")).unwrap(),
        "existant"
    );

    let tools = tools.with_export_dir(outside.clone());
    let target = outside.join("home.md").display().to_string();
    let result = ok(run(
        &tools,
        "export_note",
        json!({ "path": "Home.md", "format": "markdown", "destination": target }),
    ));
    assert!(result.starts_with("Exported: "), "{result}");
    assert!(
        std::fs::read_to_string(outside.join("home.md"))
            .unwrap()
            .contains("Home")
    );
}

struct Recorder {
    seen: Mutex<Vec<Request>>,
}

impl Handler for Recorder {
    fn handle(&self, request: Request) -> Response {
        self.seen.lock().unwrap().push(request.clone());
        match request {
            Request::CurrentNote => Response::CurrentNote(ipc::CurrentNote {
                path: Some("Home.md".into()),
                selection: "Welcome".into(),
                line: 3,
                column: 9,
            }),
            Request::OpenNote { ref path, .. } if path == "Notes/Bridge.md" => Response::Failed {
                message: "locked tab".into(),
            },
            _ => Response::Done,
        }
    }
}

#[test]
#[cfg(unix)]
fn app_tools_answer_with_a_sentence_when_the_app_is_closed_and_drive_it_when_open() {
    let f = testvault::open("app");
    let socket = testvault::scratch("app-sock").join("n/c.sock");
    let tools =
        VaultTools::new(f.vault.clone(), f.index.clone()).with_app(ipc::Client::at(socket.clone()));
    assert!(
        names(&tools).contains(&"app_current_note".to_string())
            && !names(&tools).contains(&"app_insert_at_cursor".to_string()),
        "l'insertion exige --write"
    );
    for (tool, args) in [
        ("app_current_note", json!({})),
        ("app_open_note", json!({ "path": "Home.md" })),
        ("app_show_search", json!({ "query": "x" })),
        ("app_show_graph", json!({})),
        ("app_notify", json!({ "text": "x" })),
    ] {
        let text = ok(run(&tools, tool, args));
        assert!(
            text.contains("is not open") && text.contains("no action"),
            "{tool}: {text}"
        );
    }

    let recorder = Arc::new(Recorder {
        seen: Mutex::new(Vec::new()),
    });
    let _server = ReferenceServer::start(&socket, recorder.clone()).unwrap();
    assert_eq!(
        ok(run(&tools, "app_current_note", json!({}))),
        "Open note: Home.md\nCursor: line 3, column 9\nSelection (7 characters):\nWelcome\n"
    );
    assert_eq!(
        ok(run(
            &tools,
            "app_open_note",
            json!({ "path": "Red Team/Defense", "line": 7, "new_tab": true })
        )),
        "Done."
    );
    assert_eq!(
        ok(run(
            &tools,
            "app_show_search",
            json!({ "query": "tag:#red/ad" })
        )),
        "Done."
    );
    assert_eq!(
        ok(run(&tools, "app_show_graph", json!({ "note": "Home" }))),
        "Done."
    );
    assert_eq!(
        ok(run(
            &tools,
            "app_notify",
            json!({ "text": "finished", "level": "warning" })
        )),
        "Done."
    );
    assert!(
        error(run(
            &tools,
            "app_open_note",
            json!({ "path": "Notes/Bridge.md" })
        ))
        .contains("locked tab")
    );
    assert!(
        error(run(&tools, "app_open_note", json!({ "path": "../x.md" })))
            .contains("outside the vault")
    );
    assert!(
        error(run(
            &tools,
            "app_open_note",
            json!({ "path": "Absente.md" })
        ))
        .contains("not found")
    );
    assert!(
        error(run(
            &tools,
            "app_notify",
            json!({ "text": "x", "level": "fatal" })
        ))
        .contains("unknown level")
    );
    assert!(error(run(&tools, "app_show_search", json!({ "query": " " }))).contains("empty"));
    let seen = recorder.seen.lock().unwrap().clone();
    assert!(seen.contains(&Request::OpenNote {
        path: "Red Team/Defense.md".into(),
        line: Some(7),
        new_tab: true
    }));
    assert!(seen.contains(&Request::ShowGraph {
        note: Some("Home.md".into())
    }));
    assert!(seen.contains(&Request::Notify {
        text: "finished".into(),
        level: ipc::Level::Warning
    }));
    assert!(
        !seen
            .iter()
            .any(|r| matches!(r, Request::OpenNote { path, .. } if path.contains(".."))),
        "a refused path is not sent"
    );

    let writable = tools.clone().with_unconfirmed_writes();
    assert_eq!(
        ok(run(
            &writable,
            "app_insert_at_cursor",
            json!({ "text": "added\nmulti" })
        )),
        "Done."
    );
    assert!(
        recorder
            .seen
            .lock()
            .unwrap()
            .contains(&Request::InsertAtCursor {
                text: "added\nmulti".into()
            })
    );
    assert!(
        error(run(&tools, "app_insert_at_cursor", json!({ "text": "x" }))).contains("read-only")
    );
    let denied = tools.clone().with_confirmation(|_, _| false);
    assert!(matches!(
        run(&denied, "app_insert_at_cursor", json!({ "text": "x" })),
        ToolOutcome::Denied
    ));
}

fn engagement_tools(
    name: &str,
    reveal: bool,
) -> (VaultTools, testvault::Fixture, std::path::PathBuf) {
    let f = testvault::open(name);
    let ops_dir = f.root.join("Engagements");
    std::fs::create_dir_all(&ops_dir).unwrap();
    focus_ops::create(
        &ops_dir,
        &focus_ops::NewEngagement {
            name: "Test mission".into(),
            kind: focus_ops::EngagementKind::InternalPentest,
            client: Some("Example Company".into()),
            scope: vec!["198.51.100.0/24".into()],
            exam: None,
            checklist: None,
        },
    )
    .unwrap();
    let tools = VaultTools::new(f.vault.clone(), f.index.clone())
        .with_extended_tools()
        .with_unconfirmed_writes()
        .with_ops(ops_dir.clone(), reveal);
    (tools, f, ops_dir)
}

const NMAP_XML: &str = include_str!("../../focus-ops/tests/data/scan.xml");

#[test]
fn ops_tools_cover_the_engagement_cycle() {
    let (tools, _f, ops_dir) = engagement_tools("ops-cycle", false);
    let list = names(&tools);
    assert_eq!(list.iter().filter(|n| n.starts_with("ops_")).count(), 16);
    let engagements = ok(run(&tools, "ops_list_engagements", json!({})));
    assert!(
        engagements.contains("Test mission: InternalPentest, Planning"),
        "{engagements}"
    );

    let m = json!({ "engagement": "Test mission" });
    let with = |extra: serde_json::Value| {
        let mut base = m.as_object().unwrap().clone();
        base.extend(extra.as_object().unwrap().clone());
        serde_json::Value::Object(base)
    };
    let import = ok(run(
        &tools,
        "ops_import_nmap",
        with(json!({ "text": NMAP_XML })),
    ));
    assert!(
        import.contains("2 host(s) read, 2 created") && import.contains("5 service(s)"),
        "{import}"
    );
    let hosts = ok(run(&tools, "ops_hosts_list", m.clone()));
    assert!(
        hosts.contains("192.0.2.5 (srv-web.example.test): Linux 5.4 - 5.15")
            && hosts.contains("22/tcp ssh"),
        "{hosts}"
    );
    assert!(
        error(run(
            &tools,
            "ops_import_nmap",
            with(json!({ "text": "not nmap" }))
        ))
        .contains("unrecognized nmap output")
    );

    assert!(
        ok(run(
            &tools,
            "ops_set_focus",
            with(json!({ "host": "192.0.2.5" }))
        ))
        .contains("Targeted host: srv-web.example.test")
    );
    assert!(
        error(run(
            &tools,
            "ops_set_focus",
            with(json!({ "host": "inconnu" }))
        ))
        .contains("host not found")
    );
    let journal = ok(run(
        &tools,
        "ops_journal_append",
        with(json!({ "text": "first observation" })),
    ));
    assert!(
        journal.contains("[srv-web.example.test] first observation"),
        "the default target is the targeted host: {journal}"
    );
    ok(run(
        &tools,
        "ops_journal_append",
        with(json!({ "text": "other", "target": "192.0.2.6" })),
    ));
    let read = ok(run(&tools, "ops_journal_read", with(json!({ "limit": 1 }))));
    assert!(
        read.starts_with("2 entries, the last 1")
            && read.contains("[192.0.2.6] other")
            && !read.contains("first"),
        "{read}"
    );
    assert!(
        ok(run(
            &tools,
            "ops_journal_read",
            with(json!({ "target": "192.0.2.6" }))
        ))
        .starts_with("1 entries")
    );

    let finding = ok(run(
        &tools,
        "ops_finding_create",
        with(
            json!({ "title": "Exposed interface", "cvss": "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H", "assets": ["192.0.2.5"], "attack": ["T1000"], "cwe": ["79", "cwe-89"] }),
        ),
    ));
    assert!(
        finding.starts_with("Finding created: F-001 Exposed interface (critical), CVSS 9.8"),
        "{finding}"
    );
    ok(run(
        &tools,
        "ops_finding_create",
        with(json!({ "title": "Minor detail", "severity": "low" })),
    ));
    let findings = ok(run(&tools, "ops_findings_list", m.clone()));
    assert!(
        findings.find("F-001").unwrap() < findings.find("F-002").unwrap()
            && findings.contains("CVSS 9.8, CWE-79 CWE-89"),
        "{findings}"
    );
    assert!(
        error(run(
            &tools,
            "ops_finding_create",
            with(json!({ "title": "X", "cwe": ["XSS"] }))
        ))
        .contains("not a CWE identifier")
    );
    assert!(
        error(run(
            &tools,
            "ops_finding_create",
            with(json!({ "title": "X", "severity": "enormous" }))
        ))
        .contains("unknown severity")
    );
    assert!(
        error(run(
            &tools,
            "ops_finding_create",
            with(json!({ "title": "X", "cvss": "CVSS:3.1/AV:N" }))
        ))
        .contains("missing base metric")
    );
    let score = ok(run(
        &tools,
        "ops_cvss_score",
        json!({ "vector": "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N" }),
    ));
    assert!(score.starts_with("CVSS 4.0: 9.3 (critical)"), "{score}");

    assert!(ok(run(&tools, "ops_credential_add", with(json!({ "user": "alice", "kind": "password", "secret": "Secret-123", "domain": "example.test", "source": "note" })))).starts_with("Credential added"));
    assert!(ok(run(&tools, "ops_credential_add", with(json!({ "user": "alice", "kind": "password", "secret": "Secret-123", "domain": "example.test" })))).starts_with("Credential already present"));
    let creds = ok(run(&tools, "ops_credentials_list", m.clone()));
    assert!(
        creds.contains("example.test\\alice: password [masked]")
            && !creds.contains("Secret-123")
            && creds.contains("secrets masked"),
        "{creds}"
    );
    assert!(
        error(run(
            &tools,
            "ops_credential_add",
            with(json!({ "user": " ", "kind": "password", "secret": "x" }))
        ))
        .contains("empty")
    );

    ok(run(
        &tools,
        "ops_vars_set",
        with(json!({ "name": "RHOST", "value": "192.0.2.5" })),
    ));
    ok(run(
        &tools,
        "ops_vars_set",
        with(json!({ "name": "PASS", "value": "Secret-123" })),
    ));
    let vars = ok(run(&tools, "ops_vars_get", m.clone()));
    assert!(
        vars.contains("RHOST=192.0.2.5")
            && vars.contains("PASS=[masked]")
            && !vars.contains("Secret-123")
            && vars.contains("Targeted host: srv-web.example.test"),
        "{vars}"
    );
    assert!(
        ok(run(
            &tools,
            "ops_vars_set",
            with(json!({ "name": "RHOST", "value": "" }))
        ))
        .contains("deleted")
    );
    assert!(
        error(run(
            &tools,
            "ops_vars_set",
            with(json!({ "name": "A:B", "value": "x" }))
        ))
        .contains("invalid")
    );

    let overview = ok(run(&tools, "ops_overview", m.clone()));
    assert!(
        overview.contains("Hosts: 2, 0 owned, services: 5")
            && overview.contains("critical 1")
            && overview.contains("Credentials: 1")
            && overview.contains("Journal: 2"),
        "{overview}"
    );
    let progress = ok(run(&tools, "ops_checklist_progress", m.clone()));
    assert!(
        progress.contains("Scoping and authorization: 0/5, next task (line 4)"),
        "{progress}"
    );

    let report = ok(run(
        &tools,
        "ops_report_build",
        with(json!({ "include_credentials": true, "author": "Someone" })),
    ));
    assert!(
        report.contains("# Report: Test mission")
            && report.contains("F-001")
            && report.contains("[masked]")
            && !report.contains("Secret-123"),
        "{report}"
    );
    assert!(!ops_dir.join("Test mission/Report.md").exists());
    assert!(
        ok(run(
            &tools,
            "ops_report_build",
            with(json!({ "save": true }))
        ))
        .starts_with("Report written: ")
    );
    assert!(ops_dir.join("Test mission/Report.md").exists());
}

#[test]
fn ops_secrets_reveal_and_read_only_and_name_validation() {
    let (tools, _f, _) = engagement_tools("ops-reveal", true);
    let m = json!({ "engagement": "Test mission" });
    ok(run(
        &tools,
        "ops_credential_add",
        json!({ "engagement": "Test mission", "user": "bob", "kind": "ntlm", "secret": "0123456789abcdef", "domain": "" }),
    ));
    assert!(
        ok(run(&tools, "ops_credentials_list", m.clone())).contains("bob: ntlm 0123456789abcdef")
    );
    let f = testvault::open("ops-readonly");
    let ops_dir = f.root.join("Engagements");
    std::fs::create_dir_all(&ops_dir).unwrap();
    focus_ops::create(
        &ops_dir,
        &focus_ops::NewEngagement {
            name: "E".into(),
            kind: focus_ops::EngagementKind::Lab,
            client: None,
            scope: vec![],
            exam: None,
            checklist: None,
        },
    )
    .unwrap();
    let read_only = VaultTools::new(f.vault.clone(), f.index.clone()).with_ops(ops_dir, false);
    let list = names(&read_only);
    assert!(
        list.contains(&"ops_overview".to_string())
            && !list.contains(&"ops_journal_append".to_string())
            && !list.contains(&"ops_vars_set".to_string())
    );
    assert!(
        error(run(
            &read_only,
            "ops_journal_append",
            json!({ "engagement": "E", "text": "x" })
        ))
        .contains("read-only")
    );
    assert!(
        error(run(
            &read_only,
            "ops_report_build",
            json!({ "engagement": "E", "save": true })
        ))
        .contains("read-only")
    );
    for bad in ["../E", "E/../E", "", "a\\b", ".."] {
        assert!(
            error(run(
                &read_only,
                "ops_overview",
                json!({ "engagement": bad })
            ))
            .contains("invalid"),
            "{bad}"
        );
    }
    assert!(
        error(run(
            &read_only,
            "ops_overview",
            json!({ "engagement": "Absent" })
        ))
        .contains("Engagements: E")
    );
    let none =
        VaultTools::new(f.vault.clone(), f.index.clone()).with_ops(f.root.join("Missing"), false);
    assert!(!names(&none).iter().any(|n| n.starts_with("ops_")));
}

#[test]
fn resources_are_paginated_and_read_through_the_same_path_checks() {
    let f = testvault::open("resources");
    let tools = VaultTools::new(f.vault.clone(), f.index.clone());
    let (first, next) = tools.resources(0, 2).unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(next, Some(2));
    let (all, none) = tools.resources(0, 200).unwrap();
    assert_eq!((all.len(), none), (5, None));
    assert!(
        all.iter()
            .any(|r| r.uri == "focus://note/Notes/Isolated%20note.md" && r.name == "Isolated"),
        "{all:?}"
    );
    assert!(tools.resources(4, 200).unwrap().0.len() == 1);
    let text = tools
        .read_resource("focus://note/Red%20Team/Kerberoasting.md")
        .unwrap();
    assert!(text.contains("# Kerberoasting"));
    assert!(
        tools
            .read_resource("focus://note/Notes/Isolated%20note.md")
            .is_ok()
    );
    for bad in [
        "focus://note/../x.md",
        "focus://note/%2E%2E/x.md",
        "file:///etc/passwd",
        "focus://note/Absente.md",
        "focus://note/%zz",
    ] {
        assert!(tools.read_resource(bad).is_err(), "{bad}");
    }
}

#[test]
fn prompts_serve_the_ai_actions() {
    let f = testvault::open("prompts");
    let mut saved = focus_ai::prompts::builtin().remove(0);
    saved.id = "custom".into();
    saved.label = "My action".into();
    saved.description = String::new();
    saved.prompt = "Title {{title}}, tags {{tags}}, language {{language}}:\n{{selection}}".into();
    saved.builtin = false;
    let templates = focus_ai::prompts::merged(&[saved]);
    let tools = VaultTools::new(f.vault.clone(), f.index.clone()).with_prompt_templates(templates);
    let list = tools.prompts().unwrap();
    let summarize = list.iter().find(|p| p.name == "summarize").unwrap();
    assert!(
        summarize.description.starts_with("Summarize")
            && !summarize.description.contains("ai-action"),
        "{}",
        summarize.description
    );
    assert!(list.iter().any(|p| p.name == "summarize") && list.iter().any(|p| p.name == "custom"));
    assert!(
        !list.iter().any(|p| p.name.starts_with("ops-")),
        "Ops mode actions need an engagements folder"
    );
    let args = |v: serde_json::Value| v.as_object().unwrap().clone();
    let (description, text) = tools
        .prompt(
            "custom",
            &args(json!({ "path": "Red Team/Kerberoasting.md" })),
        )
        .unwrap();
    assert!(
        description.starts_with("My action")
            && text.starts_with(
                "Title Kerberoasting, tags red/ad, language English:\n---\ntags: [red/ad]"
            ),
        "{text}"
    );
    let (_, selection) = tools
        .prompt("custom", &args(json!({ "selection": "an excerpt" })))
        .unwrap();
    assert!(
        selection.ends_with(":\nan excerpt") && selection.contains("Title ,"),
        "{selection}"
    );
    assert!(
        tools
            .prompt("custom", &args(json!({})))
            .unwrap_err()
            .contains("required")
    );
    assert!(
        tools
            .prompt("absente", &args(json!({ "selection": "x" })))
            .unwrap_err()
            .contains("unknown prompt")
    );
    assert!(
        tools
            .prompt("custom", &args(json!({ "path": "../x.md" })))
            .unwrap_err()
            .contains("outside the vault")
    );
    assert_eq!(agent::standard_tools(false).len(), 7);
}

#[test]
fn backlinks_name_a_canvas_that_shows_the_note() {
    let base = testvault::scratch("canvas-backlinks");
    let root = base.join("vault");
    std::fs::create_dir_all(&root).unwrap();
    testvault::populate(&root);
    testvault::write(
        &root,
        "Boards/Plan.canvas",
        r#"{"nodes":[{"id":"1","type":"file","file":"Notes/Isolated note.md","x":0,"y":0,"width":400,"height":300}],"edges":[]}"#,
    );
    testvault::write(
        &root,
        ".hidden/Secret.canvas",
        r#"{"nodes":[{"id":"1","type":"file","file":"Notes/Isolated note.md","x":0,"y":0,"width":400,"height":300}],"edges":[]}"#,
    );
    let f = testvault::open_at(&root, &base.join("cache"));
    let tools = VaultTools::new(f.vault.clone(), f.index.clone());
    let text = ok(run(
        &tools,
        "get_backlinks",
        json!({ "path": "Notes/Isolated note.md" }),
    ));
    assert!(text.contains("Boards/Plan.canvas"), "{text}");
    assert!(!text.contains("Secret.canvas"), "{text}");
}
