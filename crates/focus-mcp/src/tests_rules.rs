use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use focus_ai::agent::{self, ToolHost};
use serde_json::{Value, json};

use crate::VaultTools;
use crate::rules::etag;
use crate::testvault::{self, error, ok, run};

const NMAP_XML: &str = include_str!("../../focus-ops/tests/data/scan.xml");
const ENGAGEMENT: &str = "Test mission";

struct Setup {
    tools: VaultTools,
    fixture: testvault::Fixture,
    engagement: PathBuf,
    history: PathBuf,
}

fn setup(name: &str) -> Setup {
    let base = testvault::scratch(name);
    let root = base.join("vault");
    std::fs::create_dir_all(&root).unwrap();
    testvault::populate(&root);
    let ops_dir = root.join("Engagements");
    std::fs::create_dir_all(&ops_dir).unwrap();
    focus_ops::create(
        &ops_dir,
        &focus_ops::NewEngagement {
            name: ENGAGEMENT.into(),
            kind: focus_ops::EngagementKind::InternalPentest,
            client: Some("Example Company".into()),
            scope: vec!["198.51.100.0/24".into()],
            exam: None,
            checklist: None,
        },
    )
    .unwrap();
    let fixture = testvault::open_at(&root, &base.join("cache"));
    let history = fixture.root.parent().unwrap().join("history");
    fixture
        .vault
        .set_history(&history, focus_core::HistoryConfig::default())
        .unwrap();
    let tools = VaultTools::new(fixture.vault.clone(), fixture.index.clone())
        .with_extended_tools()
        .with_unconfirmed_writes()
        .with_structured_errors()
        .with_ops(ops_dir.clone(), false)
        .with_app(crate::ipc::Client::for_vault(&fixture.root));
    Setup {
        tools,
        engagement: ops_dir.join(ENGAGEMENT),
        fixture,
        history,
    }
}

fn snapshot(folders: &[&Path]) -> BTreeMap<PathBuf, Vec<u8>> {
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
    for folder in folders {
        walk(folder, &mut out);
    }
    out
}

fn fault(text: &str) -> Value {
    let value: Value = serde_json::from_str(text).unwrap_or_else(|e| panic!("{text}: {e}"));
    assert!(value["code"].is_string(), "{text}");
    assert!(value["message"].is_string(), "{text}");
    let issues = value["issues"]
        .as_array()
        .unwrap_or_else(|| panic!("{text}"));
    assert!(!issues.is_empty(), "{text}");
    for issue in issues {
        for key in ["path", "message", "hint"] {
            assert!(
                issue[key].as_str().is_some_and(|s| !s.is_empty()),
                "{key} of {text}"
            );
        }
    }
    value
}

fn etag_of_read(answer: &str) -> String {
    let at = answer.find("(etag ").unwrap_or_else(|| panic!("{answer}")) + "(etag ".len();
    answer[at..at + 12].to_string()
}

fn etag_of_answer(answer: &str) -> String {
    let at = answer.rfind("Etag: ").unwrap_or_else(|| panic!("{answer}")) + "Etag: ".len();
    answer[at..at + 12].to_string()
}

#[test]
fn a_dry_run_writes_nothing_keeps_no_version_and_says_what_would_change() {
    let s = setup("rules-dry-run");
    let engagement = json!({ "engagement": ENGAGEMENT });
    ok(run(
        &s.tools,
        "append_to_note",
        json!({ "path": "Home.md", "content": "Seed line" }),
    ));
    ok(run(
        &s.tools,
        "ops_import_nmap",
        json!({ "engagement": ENGAGEMENT, "text": NMAP_XML }),
    ));
    let host = focus_ops::hosts::list(&s.engagement)[0].ip.clone();
    let vault_and_history = [s.fixture.root.as_path(), s.history.as_path()];
    let before = snapshot(&vault_and_history);

    let with = |extra: Value| {
        let mut all = engagement.as_object().unwrap().clone();
        all.extend(extra.as_object().unwrap().clone());
        Value::Object(all)
    };
    let dry = |tool: &str, arguments: Value| {
        let mut arguments = arguments;
        arguments["dry_run"] = json!(true);
        let answer = ok(run(&s.tools, tool, arguments));
        assert!(answer.starts_with("Dry run: "), "{tool}: {answer}");
        assert!(answer.contains("Nothing was written."), "{tool}: {answer}");
        answer
    };

    let replace = dry(
        "replace_in_note",
        json!({ "path": "Home.md", "old_text": "Welcome.", "new_text": "Hello." }),
    );
    assert!(
        replace.contains("1 line(s) removed, 1 added")
            && replace.contains("-3: Welcome. See [[Red Team/Kerberoasting]]")
            && replace.contains("+3: Hello. See [[Red Team/Kerberoasting]]"),
        "{replace}"
    );
    let append = dry(
        "append_to_note",
        json!({ "path": "Home.md", "content": "New line" }),
    );
    assert!(
        append.contains("0 line(s) removed, 1 added") && append.contains("New line"),
        "{append}"
    );
    let toggle = dry("toggle_task", json!({ "path": "Home.md", "line": 5 }));
    assert!(
        toggle.contains("-5: - [ ] review the home page")
            && toggle.contains("+5: - [x] review the home page"),
        "{toggle}"
    );
    let rename = dry(
        "rename_note",
        json!({ "path": "Red Team/Kerberoasting.md", "new_path": "Red Team/Roasting.md" }),
    );
    assert!(
        rename.contains("New path: Red Team/Roasting.md")
            && rename.contains("link(s) rewritten in ")
            && rename.contains("Home.md")
            && rename.contains("Red Team/Defense.md"),
        "{rename}"
    );
    let trash = dry("trash_note", json!({ "path": "Notes/Bridge.md" }));
    assert!(trash.contains("Path: Notes/Bridge.md"), "{trash}");
    let create = dry(
        "create_note",
        json!({ "path": "New/Idea", "content": "# Idea\n\nText\n" }),
    );
    assert!(
        create.contains("would create New/Idea.md") && create.contains("3 line(s)"),
        "{create}"
    );
    dry(
        "export_note",
        json!({ "path": "Home.md", "format": "html" }),
    );
    dry(
        "ops_journal_append",
        with(json!({ "text": "Found an open share" })),
    );
    dry("ops_import_nmap", with(json!({ "text": NMAP_XML })));
    dry(
        "ops_finding_create",
        with(json!({ "title": "SMB signing off", "severity": "high" })),
    );
    dry(
        "ops_credential_add",
        with(json!({ "user": "svc", "kind": "password", "secret": "Winter2026!" })),
    );
    let vars = dry(
        "ops_vars_set",
        with(json!({ "name": "RHOST", "value": "10.0.0.5" })),
    );
    assert!(
        vars.contains("RHOST: was (not set), would be 10.0.0.5"),
        "{vars}"
    );
    dry("ops_set_focus", with(json!({ "host": host })));
    let report = dry("ops_report_build", with(json!({ "save": true })));
    assert!(report.contains("Report.md would be created"), "{report}");
    let insert = dry("app_insert_at_cursor", json!({ "text": "x" }));
    assert!(insert.contains("1 character(s)"), "{insert}");

    assert_eq!(
        snapshot(&vault_and_history),
        before,
        "a dry run changed the vault or its history"
    );

    for (tool, arguments, code) in [
        (
            "replace_in_note",
            json!({ "path": "Home.md", "old_text": "absent text", "new_text": "x" }),
            "text_not_found",
        ),
        (
            "create_note",
            json!({ "path": "Home.md", "content": "x" }),
            "already_exists",
        ),
        (
            "rename_note",
            json!({ "path": "Home.md", "new_path": "../out.md" }),
            "path_refused",
        ),
    ] {
        let mut dry_arguments = arguments.clone();
        dry_arguments["dry_run"] = json!(true);
        let from_dry = fault(&error(run(&s.tools, tool, dry_arguments)));
        let from_real = fault(&error(run(&s.tools, tool, arguments)));
        assert_eq!(from_dry["code"], code);
        assert_eq!(from_dry["code"], from_real["code"]);
    }
}

#[test]
fn a_dry_run_of_a_note_with_secrets_masks_the_lines_it_shows() {
    let s = setup("rules-dry-run-secret");
    let readme = s.engagement.join("README.md");
    let relative = readme.strip_prefix(&s.fixture.root).unwrap().to_path_buf();
    let note = std::fs::read_to_string(&readme).unwrap();
    std::fs::write(
        &readme,
        format!("{note}\nDB_PASSWORD: Hunter2-Corp-Secret-9\n"),
    )
    .unwrap();
    let answer = ok(run(
        &s.tools,
        "replace_in_note",
        json!({ "path": relative.display().to_string(), "old_text": "DB_PASSWORD: Hunter2-Corp-Secret-9", "new_text": "DB_PASSWORD: Other-Secret-Value-77", "dry_run": true }),
    ));
    assert!(
        !answer.contains("Hunter2") && !answer.contains("Other-Secret"),
        "{answer}"
    );
    assert!(answer.contains("[masked]"), "{answer}");
}

#[test]
fn if_match_refuses_a_stale_etag_and_accepts_the_current_one() {
    let s = setup("rules-etag");
    let read = ok(run(&s.tools, "read_note", json!({ "path": "Home.md" })));
    let first = etag_of_read(&read);
    assert_eq!(
        first,
        etag(&std::fs::read_to_string(s.fixture.root.join("Home.md")).unwrap())
    );

    let replaced = ok(run(
        &s.tools,
        "replace_in_note",
        json!({ "path": "Home.md", "old_text": "Welcome.", "new_text": "Hello.", "if_match": first }),
    ));
    let second = etag_of_answer(&replaced);
    let on_disk = std::fs::read_to_string(s.fixture.root.join("Home.md")).unwrap();
    assert_eq!(
        second,
        etag(&on_disk),
        "a write answers the etag of what it wrote"
    );
    assert_ne!(first, second);

    let before = on_disk.clone();
    for (tool, arguments) in [
        (
            "replace_in_note",
            json!({ "path": "Home.md", "old_text": "Hello.", "new_text": "Again." }),
        ),
        (
            "append_to_note",
            json!({ "path": "Home.md", "content": "more" }),
        ),
        ("toggle_task", json!({ "path": "Home.md", "line": 5 })),
        (
            "rename_note",
            json!({ "path": "Home.md", "new_path": "Start.md" }),
        ),
        ("trash_note", json!({ "path": "Home.md" })),
    ] {
        let mut stale = arguments;
        stale["if_match"] = json!(first);
        let failure = fault(&error(run(&s.tools, tool, stale)));
        assert_eq!(failure["code"], "stale", "{tool}");
        assert_eq!(failure["issues"][0]["path"], "if_match");
        assert!(
            failure["issues"][0]["hint"]
                .as_str()
                .unwrap()
                .contains("read_note"),
            "{tool}: {failure}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(s.fixture.root.join("Home.md")).unwrap(),
        before
    );
    assert!(s.fixture.root.join("Home.md").exists());

    std::fs::write(s.fixture.root.join("Home.md"), "edited elsewhere\n").unwrap();
    let outside = fault(&error(run(
        &s.tools,
        "append_to_note",
        json!({ "path": "Home.md", "content": "x", "if_match": second }),
    )));
    assert_eq!(outside["code"], "stale");

    let current = etag("edited elsewhere\n");
    let appended = ok(run(
        &s.tools,
        "append_to_note",
        json!({ "path": "Home.md", "content": "x", "if_match": current.to_uppercase() }),
    ));
    let renamed = ok(run(
        &s.tools,
        "rename_note",
        json!({ "path": "Home.md", "new_path": "Start.md", "if_match": etag_of_answer(&appended) }),
    ));
    assert!(
        renamed.contains("Renamed:") && renamed.contains("Etag: "),
        "{renamed}"
    );
    ok(run(
        &s.tools,
        "trash_note",
        json!({ "path": "Start.md", "if_match": etag_of_answer(&renamed) }),
    ));
    assert!(!s.fixture.root.join("Start.md").exists());

    let malformed = fault(&error(run(
        &s.tools,
        "append_to_note",
        json!({ "path": "Notes/Bridge.md", "content": "x", "if_match": "not an etag" }),
    )));
    assert_eq!(malformed["code"], "invalid_argument");
    assert_eq!(malformed["issues"][0]["path"], "if_match");
}

#[test]
fn if_match_guards_the_variables_and_the_report_of_an_engagement() {
    let s = setup("rules-etag-ops");
    let m = json!({ "engagement": ENGAGEMENT });
    let read = ok(run(&s.tools, "ops_vars_get", m.clone()));
    let first = etag_of_answer(&read);
    let set = ok(run(
        &s.tools,
        "ops_vars_set",
        json!({ "engagement": ENGAGEMENT, "name": "RHOST", "value": "10.0.0.5", "if_match": first }),
    ));
    let second = etag_of_answer(&set);
    assert_ne!(first, second);
    let stale = fault(&error(run(
        &s.tools,
        "ops_vars_set",
        json!({ "engagement": ENGAGEMENT, "name": "RHOST", "value": "10.0.0.6", "if_match": first }),
    )));
    assert_eq!(stale["code"], "stale");
    assert!(
        stale["issues"][0]["hint"]
            .as_str()
            .unwrap()
            .contains("ops_vars_get")
    );
    ok(run(
        &s.tools,
        "ops_set_focus",
        json!({ "engagement": ENGAGEMENT, "host": "", "if_match": second }),
    ));
    let saved = ok(run(
        &s.tools,
        "ops_report_build",
        json!({ "engagement": ENGAGEMENT, "save": true }),
    ));
    let report = etag_of_answer(&saved);
    ok(run(
        &s.tools,
        "ops_report_build",
        json!({ "engagement": ENGAGEMENT, "save": true, "if_match": report }),
    ));
    let late = fault(&error(run(
        &s.tools,
        "ops_report_build",
        json!({ "engagement": ENGAGEMENT, "save": true, "if_match": "0000deadbeef" }),
    )));
    assert_eq!(late["code"], "stale");
}

#[test]
fn a_repeated_idempotency_key_answers_the_first_result_and_writes_once() {
    let s = setup("rules-idempotency");
    let note =
        |text: &str| json!({ "path": "Keyed.md", "content": text, "idempotency_key": "k-create" });
    let first = ok(run(&s.tools, "create_note", note("one\n")));
    let again = ok(run(&s.tools, "create_note", note("one\n")));
    assert_eq!(first, again);
    assert_eq!(
        std::fs::read_to_string(s.fixture.root.join("Keyed.md")).unwrap(),
        "one\n"
    );

    let other = fault(&error(run(&s.tools, "create_note", note("two\n"))));
    assert_eq!(other["code"], "idempotency_conflict");
    assert_eq!(other["issues"][0]["path"], "idempotency_key");

    let append = json!({ "path": "Home.md", "content": "Once", "idempotency_key": "k-append" });
    let a = ok(run(&s.tools, "append_to_note", append.clone()));
    let b = ok(run(&s.tools, "append_to_note", append));
    assert_eq!(a, b);
    let home = std::fs::read_to_string(s.fixture.root.join("Home.md")).unwrap();
    assert_eq!(home.matches("Once").count(), 1);

    let journal =
        json!({ "engagement": ENGAGEMENT, "text": "scan started", "idempotency_key": "k-journal" });
    ok(run(&s.tools, "ops_journal_append", journal.clone()));
    ok(run(&s.tools, "ops_journal_append", journal));
    assert_eq!(focus_ops::journal::read(&s.engagement).len(), 1);

    let finding = json!({ "engagement": ENGAGEMENT, "title": "Exposed panel", "idempotency_key": "k-finding" });
    ok(run(&s.tools, "ops_finding_create", finding.clone()));
    ok(run(&s.tools, "ops_finding_create", finding));
    assert_eq!(focus_ops::findings::list(&s.engagement).len(), 1);

    let credential = json!({ "engagement": ENGAGEMENT, "user": "svc", "kind": "password", "secret": "Winter2026!", "idempotency_key": "k-cred" });
    let c = ok(run(&s.tools, "ops_credential_add", credential.clone()));
    assert_eq!(c, ok(run(&s.tools, "ops_credential_add", credential)));
    assert!(c.starts_with("Credential added"), "{c}");

    let import =
        json!({ "engagement": ENGAGEMENT, "text": NMAP_XML, "idempotency_key": "k-import" });
    let i = ok(run(&s.tools, "ops_import_nmap", import.clone()));
    assert_eq!(i, ok(run(&s.tools, "ops_import_nmap", import)));
    assert!(
        !i.contains(" 0 created"),
        "a second import would have created nothing: {i}"
    );

    let missing =
        json!({ "path": "Absent.md", "content": "x", "idempotency_key": "k-append-failing" });
    assert_eq!(
        fault(&error(run(&s.tools, "append_to_note", missing.clone())))["code"],
        "not_found"
    );
    ok(run(
        &s.tools,
        "create_note",
        json!({ "path": "Absent.md", "content": "x" }),
    ));
    ok(run(&s.tools, "append_to_note", missing));

    ok(run(
        &s.tools,
        "append_to_note",
        json!({ "path": "Notes/Bridge.md", "content": "y", "idempotency_key": "k-create" }),
    ));
    let dry = ok(run(
        &s.tools,
        "create_note",
        json!({ "path": "Keyed.md", "content": "one\n", "idempotency_key": "k-create", "dry_run": true }),
    ));
    assert!(dry.contains("already ran"), "{dry}");
}

#[test]
fn every_error_has_a_code_a_message_and_issues_with_a_hint() {
    let s = setup("rules-errors");
    let read_only_tools =
        VaultTools::new(s.fixture.vault.clone(), s.fixture.index.clone()).with_structured_errors();

    let bad_argument = fault(&error(run(&s.tools, "read_note", json!({}))));
    assert_eq!(bad_argument["code"], "invalid_argument");
    assert_eq!(bad_argument["issues"][0]["path"], "path");

    let bad_number = fault(&error(run(
        &s.tools,
        "search_notes",
        json!({ "query": "a", "limit": "many" }),
    )));
    assert_eq!(bad_number["issues"][0]["path"], "limit");
    assert!(
        bad_number["issues"][0]["hint"]
            .as_str()
            .unwrap()
            .contains("from 1 to 30")
    );

    let bad_enum = fault(&error(run(
        &s.tools,
        "export_note",
        json!({ "path": "Home.md", "format": "docx" }),
    )));
    assert_eq!(bad_enum["issues"][0]["path"], "format");
    assert!(
        bad_enum["issues"][0]["hint"]
            .as_str()
            .unwrap()
            .contains("html, markdown, pdf")
    );

    let refused = fault(&error(run(
        &s.tools,
        "create_note",
        json!({ "path": "../escape.md", "content": "x" }),
    )));
    assert_eq!(refused["code"], "path_refused");
    assert_eq!(refused["issues"][0]["path"], "path");
    let reserved = fault(&error(run(
        &s.tools,
        "create_note",
        json!({ "path": ".git/x.md", "content": "x" }),
    )));
    assert_eq!(reserved["code"], "path_refused");

    let missing = fault(&error(run(
        &s.tools,
        "read_note",
        json!({ "path": "Kerberoastin" }),
    )));
    assert_eq!(missing["code"], "not_found");
    assert!(
        missing["issues"][0]["hint"]
            .as_str()
            .unwrap()
            .contains("Red Team/Kerberoasting.md")
    );
    assert_eq!(
        fault(&error(run(
            &s.tools,
            "ops_overview",
            json!({ "engagement": "Nope" })
        )))["issues"][0]["path"],
        "engagement"
    );

    let read_only = fault(&error(run(
        &read_only_tools,
        "append_to_note",
        json!({ "path": "Home.md", "content": "x" }),
    )));
    assert_eq!(read_only["code"], "read_only");
    assert!(
        read_only["issues"][0]["hint"]
            .as_str()
            .unwrap()
            .contains("--write")
    );
    let unknown = fault(&error(run(&s.tools, "no_such_tool", json!({}))));
    assert_eq!(unknown["code"], "unknown_tool");

    let stale = fault(&error(run(
        &s.tools,
        "append_to_note",
        json!({ "path": "Home.md", "content": "x", "if_match": "000000000000" }),
    )));
    assert_eq!(stale["code"], "stale");

    let query = fault(&error(run(
        &s.tools,
        "search_notes",
        json!({ "query": "\"unclosed" }),
    )));
    assert_eq!(query["issues"][0]["path"], "query");

    let json_error = tools_call_raw(&s.tools, "read_note", "not json");
    assert_eq!(fault(&json_error)["code"], "invalid_argument");

    for text in [&bad_argument, &refused, &missing, &read_only, &stale] {
        let text = text.to_string();
        assert!(
            !text.contains(".rs") && !text.contains("::") && !text.contains("fn "),
            "{text}"
        );
    }
}

fn tools_call_raw(tools: &VaultTools, name: &str, arguments: &str) -> String {
    match tools.call(
        &focus_ai::ToolCall {
            id: "t".into(),
            name: name.into(),
            arguments: arguments.into(),
        },
        &focus_ai::Cancel::new(),
    ) {
        agent::ToolOutcome::Error(text) => text,
        other => panic!("{other:?}"),
    }
}

#[test]
fn lists_answer_a_page_and_say_what_remains() {
    let s = setup("rules-pages");
    let first = ok(run(
        &s.tools,
        "search_notes",
        json!({ "query": "the", "limit": 1 }),
    ));
    assert!(
        first.contains(
            "more note(s) not shown: send offset=1 for the next ones, or narrow the request"
        ),
        "{first}"
    );
    let second = ok(run(
        &s.tools,
        "search_notes",
        json!({ "query": "the", "limit": 1, "offset": 1 }),
    ));
    assert!(second.contains("(notes 2 to 2 shown)"), "{second}");
    let titles_only = ok(run(
        &s.tools,
        "search_notes",
        json!({ "query": "the", "limit": 1, "fields": ["title"] }),
    ));
    assert!(
        !titles_only.contains("\n  "),
        "no matching lines: {titles_only}"
    );
    let paths_only = ok(run(
        &s.tools,
        "search_notes",
        json!({ "query": "the", "limit": 1, "fields": [] }),
    ));
    assert!(
        paths_only.lines().nth(1).unwrap().contains("occurrence(s)")
            && !paths_only.lines().nth(1).unwrap().contains(": "),
        "{paths_only}"
    );
    let past = ok(run(
        &s.tools,
        "search_notes",
        json!({ "query": "the", "offset": 500 }),
    ));
    assert!(past.starts_with("No note after offset 500"), "{past}");
    assert_eq!(
        fault(&error(run(
            &s.tools,
            "search_notes",
            json!({ "query": "the", "fields": ["body"] })
        )))["issues"][0]["path"],
        "fields"
    );

    let folder = ok(run(
        &s.tools,
        "list_folder",
        json!({ "path": "", "limit": 1 }),
    ));
    assert!(
        folder.contains("more entries not shown: send offset=1"),
        "{folder}"
    );
    let rest = ok(run(
        &s.tools,
        "list_folder",
        json!({ "path": "", "limit": 1, "offset": 1 }),
    ));
    assert_ne!(folder.lines().next(), rest.lines().next());

    let tags = ok(run(&s.tools, "list_tags", json!({ "limit": 1 })));
    assert!(
        tags.contains("more tags not shown: send offset=1"),
        "{tags}"
    );
    let backlinks = ok(run(
        &s.tools,
        "get_backlinks",
        json!({ "path": "Red Team/Kerberoasting.md", "limit": 1 }),
    ));
    assert!(
        backlinks.contains("2 note(s) link to")
            && backlinks.contains("more links not shown: send offset=1"),
        "{backlinks}"
    );
    let linked = ok(run(&s.tools, "most_linked", json!({ "limit": 1 })));
    assert!(
        linked.contains("more notes not shown: send offset=1"),
        "{linked}"
    );
    ok(run(&s.tools, "orphan_notes", json!({ "limit": 1 })));
    let tasks = ok(run(&s.tools, "list_tasks", json!({ "limit": 1 })));
    assert!(
        tasks.contains("more tasks not shown: send offset=1"),
        "{tasks}"
    );

    for n in 0..3 {
        ok(run(
            &s.tools,
            "ops_journal_append",
            json!({ "engagement": ENGAGEMENT, "text": format!("entry {n}") }),
        ));
        ok(run(
            &s.tools,
            "ops_finding_create",
            json!({ "engagement": ENGAGEMENT, "title": format!("Finding {n}") }),
        ));
        ok(run(
            &s.tools,
            "ops_credential_add",
            json!({ "engagement": ENGAGEMENT, "user": format!("user{n}"), "kind": "password", "secret": format!("secret-{n}-value") }),
        ));
    }
    let journal = ok(run(
        &s.tools,
        "ops_journal_read",
        json!({ "engagement": ENGAGEMENT, "limit": 1 }),
    ));
    assert!(
        journal.contains("entry 2")
            && journal.contains("2 more older entries not shown: send offset=1"),
        "{journal}"
    );
    let older = ok(run(
        &s.tools,
        "ops_journal_read",
        json!({ "engagement": ENGAGEMENT, "limit": 1, "offset": 1 }),
    ));
    assert!(
        older.contains("entry 1") && !older.contains("entry 2"),
        "{older}"
    );
    for tool in ["ops_findings_list", "ops_credentials_list"] {
        let page = ok(run(
            &s.tools,
            tool,
            json!({ "engagement": ENGAGEMENT, "limit": 2 }),
        ));
        assert!(
            page.contains("1 more") && page.contains("send offset=2"),
            "{tool}: {page}"
        );
    }
}

#[test]
fn a_client_that_sends_none_of_the_new_arguments_gets_the_answers_it_always_got() {
    let fixture = testvault::open("rules-legacy");
    let tools = VaultTools::new(fixture.vault.clone(), fixture.index.clone())
        .with_extended_tools()
        .with_unconfirmed_writes();
    let created = ok(run(
        &tools,
        "create_note",
        json!({ "path": "Legacy", "content": "# Legacy\n\nText.\n" }),
    ));
    assert!(
        created.starts_with("Note created: Legacy.md (3 line(s))"),
        "{created}"
    );
    assert!(
        ok(run(
            &tools,
            "append_to_note",
            json!({ "path": "Legacy", "content": "More" })
        ))
        .starts_with("Appended to Legacy.md: 1 line(s)")
    );
    assert!(
        ok(run(
            &tools,
            "replace_in_note",
            json!({ "path": "Legacy", "old_text": "Text.", "new_text": "Body." })
        ))
        .starts_with("Replacement done in Legacy.md, at line 3")
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("Legacy.md")).unwrap(),
        "# Legacy\n\nBody.\nMore\n"
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
        ok(run(
            &tools,
            "rename_note",
            json!({ "path": "Legacy", "new_path": "Old/Legacy" })
        ))
        .starts_with("Renamed: from Legacy.md to Old/Legacy.md")
    );
    assert!(
        ok(run(&tools, "trash_note", json!({ "path": "Old/Legacy" })))
            .starts_with("Moved to trash: Old/Legacy.md")
    );

    assert!(
        ok(run(
            &tools,
            "append_to_note",
            json!({ "path": "Home.md", "content": "twice" })
        ))
        .contains("Appended")
    );
    assert!(
        ok(run(
            &tools,
            "append_to_note",
            json!({ "path": "Home.md", "content": "twice" })
        ))
        .contains("Appended")
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("Home.md"))
            .unwrap()
            .matches("twice")
            .count(),
        2
    );
    let plain = error(run(&tools, "read_note", json!({ "path": "Absent" })));
    assert!(plain.starts_with("note not found: Absent"), "{plain}");
    let search = ok(run(
        &tools,
        "search_notes",
        json!({ "query": "service accounts" }),
    ));
    assert!(search.starts_with("1 note(s), "), "{search}");
}

#[test]
fn every_write_tool_states_the_rules_in_its_description() {
    let s = setup("rules-specs");
    let specs = s.tools.specs();
    let spec = |name: &str| {
        specs
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    let has =
        |name: &str, argument: &str| spec(name).parameters["properties"].get(argument).is_some();
    for tool in specs
        .iter()
        .filter(|t| agent::is_write_tool(&t.name))
        .map(|t| t.name.as_str())
        .chain(["ops_report_build"])
    {
        assert!(has(tool, "dry_run"), "{tool}");
        assert!(
            spec(tool).parameters["properties"]["dry_run"]["description"]
                .as_str()
                .unwrap()
                .contains("without writing")
        );
    }
    for tool in [
        "append_to_note",
        "replace_in_note",
        "toggle_task",
        "rename_note",
        "trash_note",
        "ops_vars_set",
        "ops_set_focus",
        "ops_report_build",
    ] {
        assert!(has(tool, "if_match"), "{tool}");
        assert!(spec(tool).description.contains("if_match"), "{tool}");
    }
    for tool in [
        "create_note",
        "append_to_note",
        "ops_journal_append",
        "ops_import_nmap",
        "ops_finding_create",
        "ops_credential_add",
    ] {
        assert!(has(tool, "idempotency_key"), "{tool}");
    }
    for tool in [
        "search_notes",
        "list_folder",
        "get_backlinks",
        "list_tags",
        "list_tasks",
        "orphan_attachments",
        "most_linked",
        "orphan_notes",
        "ops_journal_read",
        "ops_hosts_list",
        "ops_findings_list",
        "ops_credentials_list",
    ] {
        assert!(has(tool, "offset"), "{tool}");
        assert!(spec(tool).description.contains("offset"), "{tool}");
    }
    assert!(spec("read_note").description.contains("etag"));
    assert!(
        spec("replace_in_note")
            .description
            .contains("Always send `if_match`")
    );
    for tool in &specs {
        assert!(tool.parameters["properties"].is_object(), "{}", tool.name);
    }
}

fn rewrite_lines(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| {
            line.contains("link(s) rewritten") || line.starts_with("No links to rewrite")
        })
        .collect();
    assert!(!lines.is_empty(), "{text}");
    lines
}

#[test]
fn rename_dry_run_says_what_the_rename_rewrites() {
    let base = testvault::scratch("rename-exact");
    let root = base.join("vault");
    std::fs::create_dir_all(&root).unwrap();
    testvault::populate(&root);
    let card = |file: &str| {
        format!(
            r#"{{"nodes":[{{"id":"1","type":"file","file":"{file}","x":0,"y":0,"width":400,"height":300}}],"edges":[]}}"#
        )
    };
    testvault::write(&root, "Notes/Target.md", "# Target\n");
    testvault::write(&root, "ByName.md", "See [[Target]].\n");
    testvault::write(
        &root,
        "ByPath.md",
        "See [[Notes/Target]] and [[Notes/Target|again]].\n",
    );
    testvault::write(&root, "Boards/Plan.canvas", &card("Notes/Target.md"));
    testvault::write(
        &root,
        "Engagements/Mission/Loot/Secret.canvas",
        &card("Notes/Target.md"),
    );
    testvault::write(&root, "Notes/Lonely.md", "# Lonely\n");
    testvault::write(
        &root,
        "Engagements/Mission/Loot/Lonely.canvas",
        &card("Notes/Lonely.md"),
    );
    let fixture = testvault::open_at(&root, &base.join("cache"));
    let tools = VaultTools::new(fixture.vault.clone(), fixture.index.clone())
        .with_extended_tools()
        .with_unconfirmed_writes()
        .with_structured_errors()
        .with_ops(root.join("Engagements"), false);
    let arguments = json!({ "path": "Notes/Target.md", "new_path": "Archive/Target.md" });

    let before = snapshot(&[&fixture.root]);
    let mut dry_arguments = arguments.clone();
    dry_arguments["dry_run"] = json!(true);
    let dry = ok(run(&tools, "rename_note", dry_arguments));
    assert_eq!(snapshot(&[&fixture.root]), before);
    let lines: Vec<String> = rewrite_lines(&dry).into_iter().map(String::from).collect();
    assert_eq!(
        lines,
        [
            "3 link(s) rewritten in 2 file(s): Boards/Plan.canvas (1), ByPath.md (2)",
            "1 link(s) rewritten in 1 file(s) that are not shown"
        ],
        "{dry}"
    );
    for hidden in ["ByName.md", "Secret.canvas", "Loot"] {
        assert!(!dry.contains(hidden), "{dry}");
    }

    let real = ok(run(&tools, "rename_note", arguments));
    assert_eq!(rewrite_lines(&real), lines, "{real}");
    assert!(!real.contains("Secret.canvas"), "{real}");
    assert!(
        std::fs::read_to_string(root.join("Engagements/Mission/Loot/Secret.canvas"))
            .unwrap()
            .contains("Archive/Target.md"),
        "a source that is not shown is still rewritten"
    );

    let alone = ok(run(
        &tools,
        "rename_note",
        json!({ "path": "Notes/Lonely.md", "new_path": "Notes/Alone.md" }),
    ));
    assert_eq!(
        rewrite_lines(&alone),
        ["1 link(s) rewritten in 1 file(s) that are not shown"],
        "{alone}"
    );
    assert!(
        !alone.contains("Loot") && !alone.contains("Lonely.canvas"),
        "{alone}"
    );
}

#[test]
fn rename_dry_run_refuses_like_the_call() {
    let s = setup("rename-refusals");
    let cases = [
        json!({ "path": "Home.md", "new_path": "Notes/Bridge.md" }),
        json!({ "path": "Home.md", "new_path": "Home.md" }),
        json!({ "path": "Home.md", "new_path": "../outside.md" }),
    ];
    for arguments in cases {
        let mut dry_arguments = arguments.clone();
        dry_arguments["dry_run"] = json!(true);
        assert_eq!(
            error(run(&s.tools, "rename_note", dry_arguments)),
            error(run(&s.tools, "rename_note", arguments)),
        );
    }
}
