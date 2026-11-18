use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use focus_ai::agent::{self, ToolHost, ToolOutcome};
use focus_ai::mcp::{CompositeHost, McpError, Server, ServerConfig, StartOptions};
use focus_ai::{Cancel, ToolCall, ToolSpec};

fn config(mode: &str) -> ServerConfig {
    ServerConfig {
        name: "mock".into(),
        command: env!("CARGO_BIN_EXE_focus-mock-mcp").into(),
        args: vec!["--mode".into(), mode.into()],
        env: BTreeMap::new(),
        allowed_tools: vec!["echo".into()],
        enabled: true,
    }
}

fn quick() -> StartOptions {
    StartOptions {
        probe: Duration::from_millis(600),
        startup: Duration::from_secs(10),
    }
}

fn start(mode: &str) -> Server {
    Server::start_with(&config(mode), quick())
        .unwrap_or_else(|e| panic!("startup in mode {mode}: {e}"))
}

fn call(server: &Server, tool: &str, arguments: &str) -> ToolOutcome {
    server.call(
        &format!("mock__{tool}"),
        arguments,
        &Cancel::new(),
        Duration::from_secs(10),
    )
}

fn ok(outcome: ToolOutcome) -> String {
    match outcome {
        ToolOutcome::Ok(text) => text,
        other => panic!("expected Ok, got {other:?}"),
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(format!("mcp-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn modern_server_is_discovered_and_tools_are_paginated() {
    let server = start("stateless");
    assert_eq!(server.protocol_version(), "2026-07-28");
    assert_eq!(server.instructions(), Some("Test mock server."));
    let names: Vec<String> = server.tools().into_iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "mock__echo",
            "mock__fail",
            "mock__dotted_name",
            "mock__big",
            "mock__slow",
            "mock__crash",
            "mock__noreply",
            "mock__needs_input",
            "mock__env",
            "mock__image",
            "mock__structured",
            "mock__ping_ok"
        ]
    );
    assert_eq!(server.raw_name("mock__dotted_name"), Some("dotted.name"));
    assert_eq!(
        ok(call(&server, "echo", r#"{"text":"hello é"}"#)),
        "hello é"
    );
    assert_eq!(ok(call(&server, "dotted_name", "")), "ok dotted");
}

#[test]
fn legacy_server_falls_back_to_initialize() {
    for mode in ["legacy", "legacy-silent"] {
        let server = start(mode);
        assert_eq!(server.protocol_version(), "2025-06-18", "{mode}");
        assert_eq!(server.tools().len(), 12);
        assert_eq!(ok(call(&server, "echo", r#"{"text":"x"}"#)), "x");
        assert_eq!(ok(call(&server, "ping_ok", "{}")), "yes", "{mode}");
    }
}

#[test]
fn unsupported_or_unknown_versions_are_reported() {
    for (mode, expected) in [
        ("unsupported", "2099-01-01"),
        ("legacy-bad-version", "1999-01-01"),
    ] {
        let error = Server::start_with(&config(mode), quick())
            .err()
            .unwrap_or_else(|| panic!("{mode} should have failed"));
        assert!(matches!(error, McpError::Protocol { .. }), "{error:?}");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn tool_results_error_and_content_kinds() {
    let server = start("stateless");
    assert!(
        matches!(call(&server, "fail", "{}"), ToolOutcome::Error(t) if t == "intended failure")
    );
    assert_eq!(
        ok(call(&server, "image", "{}")),
        "here\n[image content not forwarded: image/png]"
    );
    assert_eq!(ok(call(&server, "structured", "{}")), r#"{"n":7}"#);
    assert!(
        matches!(call(&server, "needs_input", "{}"), ToolOutcome::Error(t) if t.contains("user input"))
    );
    let big = ok(call(&server, "big", "{}"));
    assert!(
        big.len() < agent::MAX_TOOL_RESULT_BYTES + 100 && big.contains("[result truncated"),
        "{}",
        big.len()
    );
    assert!(
        matches!(call(&server, "unknown", "{}"), ToolOutcome::Error(t) if t.contains("unknown"))
    );
    assert!(matches!(
        call(&server, "echo", "[1]"),
        ToolOutcome::Error(_)
    ));
    assert!(matches!(
        call(&server, "echo", "{not json"),
        ToolOutcome::Error(_)
    ));
}

#[test]
fn environment_is_added_to_the_inherited_one() {
    let mut cfg = config("stateless");
    cfg.env
        .insert("FOCUS_MOCK_VAR".into(), "valeur transmise".into());
    let server = Server::start_with(&cfg, quick()).unwrap();
    assert_eq!(ok(call(&server, "env", "{}")), "valeur transmise");
    let without = start("stateless");
    assert_eq!(ok(call(&without, "env", "{}")), "(unset)");
}

#[test]
fn non_json_banner_on_stdout_is_tolerated() {
    let server = start("noise");
    assert_eq!(ok(call(&server, "echo", r#"{"text":"ok"}"#)), "ok");
}

#[test]
fn server_that_dies_at_startup_gives_a_clear_error() {
    let error = Server::start_with(&config("die"), quick())
        .err()
        .expect("failure expected");
    let message = error.to_string();
    assert!(matches!(error, McpError::Exited { .. }), "{message}");
    assert!(message.contains("boom: missing configuration"), "{message}");
}

#[test]
fn mute_server_times_out_instead_of_blocking() {
    let started = Instant::now();
    let error = Server::start_with(
        &config("mute"),
        StartOptions {
            probe: Duration::from_millis(200),
            startup: Duration::from_millis(900),
        },
    )
    .err()
    .expect("failure expected");
    assert!(matches!(error, McpError::Timeout { .. }), "{error}");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn missing_command_and_invalid_config() {
    let mut cfg = config("stateless");
    cfg.command = "/nonexistent/mcp-server".into();
    assert!(matches!(Server::start(&cfg), Err(McpError::Spawn { .. })));
    cfg.command = String::new();
    assert!(matches!(
        Server::start(&cfg),
        Err(McpError::InvalidConfig { .. })
    ));
    cfg = config("stateless");
    cfg.name = "...".into();
    assert!(matches!(
        Server::start(&cfg),
        Err(McpError::InvalidConfig { .. })
    ));
    cfg = config("stateless");
    cfg.enabled = false;
    assert!(matches!(
        Server::start(&cfg),
        Err(McpError::InvalidConfig { .. })
    ));
}

#[test]
fn crash_during_a_call_is_reported_and_later_calls_fail_fast() {
    let server = start("stateless");
    let started = Instant::now();
    let outcome = call(&server, "crash", "{}");
    let ToolOutcome::Error(message) = outcome else {
        panic!("expected Error, got {outcome:?}")
    };
    assert!(
        message.contains("the process exited") && message.contains("simulated panic"),
        "{message}"
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    let again = Instant::now();
    assert!(
        matches!(call(&server, "echo", "{}"), ToolOutcome::Error(m) if m.contains("the process exited"))
    );
    assert!(again.elapsed() < Duration::from_secs(1));
}

#[test]
fn unanswered_call_times_out() {
    let server = start("stateless");
    let started = Instant::now();
    let outcome = server.call(
        "mock__noreply",
        "{}",
        &Cancel::new(),
        Duration::from_millis(400),
    );
    assert!(
        matches!(&outcome, ToolOutcome::Error(m) if m.contains("no response")),
        "{outcome:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        ok(call(&server, "echo", r#"{"text":"still here"}"#)),
        "still here"
    );
}

#[test]
fn cancel_interrupts_a_call_within_100ms_and_notifies_the_server() {
    let dir = scratch("cancel");
    let marker = dir.join("cancelled.txt");
    let mut cfg = config("stateless");
    cfg.env
        .insert("MOCK_CANCEL_FILE".into(), marker.display().to_string());
    let server = Server::start_with(&cfg, quick()).unwrap();
    let cancel = Cancel::new();
    let raised = Arc::new(Mutex::new(None));
    {
        let (cancel, raised) = (cancel.clone(), raised.clone());
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            *raised.lock().unwrap() = Some(Instant::now());
            cancel.cancel();
        });
    }
    let outcome = server.call("mock__slow", "{}", &cancel, Duration::from_secs(30));
    let latency =
        Instant::now().duration_since(raised.lock().unwrap().expect("cancellation raised"));
    assert!(
        matches!(&outcome, ToolOutcome::Error(m) if m == "cancelled"),
        "{outcome:?}"
    );
    assert!(latency < Duration::from_millis(100), "{latency:?}");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        marker.exists(),
        "the server did not receive notifications/cancelled"
    );
}

fn process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
        && std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| !s.contains(") Z"))
            .unwrap_or(false)
}

#[test]
fn stop_and_drop_terminate_the_process() {
    let server = start("stateless");
    ok(call(&server, "echo", "{}"));
    let pid = server.process_id();
    assert!(process_alive(pid));
    let started = Instant::now();
    server.stop();
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    server.stop();
    assert!(matches!(call(&server, "echo", "{}"), ToolOutcome::Error(_)));
    drop(server);
    std::thread::sleep(Duration::from_millis(200));
    assert!(!process_alive(pid), "process {pid} is still alive");
}

struct InternalHost {
    calls: Mutex<Vec<String>>,
}

impl ToolHost for InternalHost {
    fn specs(&self) -> Vec<ToolSpec> {
        agent::standard_tools(true)
    }
    fn call(&self, call: &ToolCall, _cancel: &Cancel) -> ToolOutcome {
        self.calls.lock().unwrap().push(call.name.clone());
        ToolOutcome::Ok(format!("interne {}", call.name))
    }
}

fn tool_call(name: &str, arguments: &str) -> ToolCall {
    ToolCall {
        id: "c1".into(),
        name: name.into(),
        arguments: arguments.into(),
    }
}

#[test]
fn composite_host_routes_and_confirms() {
    let asked: Arc<Mutex<Vec<(String, String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let answer = Arc::new(Mutex::new(true));
    let (log, decision) = (asked.clone(), answer.clone());
    let host = CompositeHost::new(
        InternalHost {
            calls: Mutex::new(Vec::new()),
        },
        vec![start("stateless")],
        move |request, _| {
            log.lock().unwrap().push((
                request.server.to_string(),
                request.tool.to_string(),
                request.arguments.to_string(),
            ));
            *decision.lock().unwrap()
        },
    );
    let specs = host.specs();
    assert_eq!(specs.len(), 10 + 12);
    assert!(
        specs.iter().any(|s| s.name == "search_notes")
            && specs.iter().any(|s| s.name == "mock__echo")
    );

    assert!(!host.requires_confirmation("search_notes"));
    assert!(host.requires_confirmation("create_note"));
    assert!(
        !host.requires_confirmation("mock__echo"),
        "echo is in allowed_tools"
    );
    assert!(host.requires_confirmation("mock__fail"));
    assert!(host.requires_confirmation("nom_inconnu"));

    let cancel = Cancel::new();
    assert!(
        matches!(host.call(&tool_call("search_notes", "{}"), &cancel), ToolOutcome::Ok(t) if t == "interne search_notes")
    );
    assert!(
        matches!(host.call(&tool_call("mock__echo", r#"{"text":"direct"}"#), &cancel), ToolOutcome::Ok(t) if t == "direct")
    );
    assert!(
        asked.lock().unwrap().is_empty(),
        "no confirmation for allowed and internal tools"
    );

    assert!(
        matches!(host.call(&tool_call("mock__dotted_name", "{}"), &cancel), ToolOutcome::Ok(t) if t == "ok dotted")
    );
    assert_eq!(
        asked.lock().unwrap().as_slice(),
        [(
            "mock".to_string(),
            "dotted.name".to_string(),
            "{}".to_string()
        )]
    );

    *answer.lock().unwrap() = false;
    assert!(matches!(
        host.call(&tool_call("mock__fail", "{}"), &cancel),
        ToolOutcome::Denied
    ));
    assert_eq!(asked.lock().unwrap().len(), 2);
}

#[test]
fn start_servers_keeps_going_after_a_failure() {
    let mut broken = config("die");
    broken.name = "casse".into();
    let mut disabled = config("stateless");
    disabled.name = "inactif".into();
    disabled.enabled = false;
    let (started, failed) =
        CompositeHost::<InternalHost>::start_servers(&[broken, disabled, config("stateless")]);
    assert_eq!(
        started.iter().map(Server::name).collect::<Vec<_>>(),
        ["mock"]
    );
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0, "casse");
}

#[test]
fn agent_loop_uses_mcp_tools() {
    let host = CompositeHost::new(
        InternalHost {
            calls: Mutex::new(Vec::new()),
        },
        vec![start("stateless")],
        |_, _| true,
    );
    for spec in host.specs() {
        assert!(
            spec.name.len() <= 128
                && spec
                    .name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "{}",
            spec.name
        );
    }
}
