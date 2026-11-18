use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

fn send(message: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{message}");
    let _ = out.flush();
}

fn reply(id: &Value, result: Value) {
    send(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
}

fn error(id: &Value, code: i64, message: &str, data: Option<Value>) {
    let mut error = json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    send(&json!({ "jsonrpc": "2.0", "id": id, "error": error }));
}

fn text(text: &str) -> Value {
    json!({ "resultType": "complete", "content": [{ "type": "text", "text": text }] })
}

fn tool(name: &str, description: &str) -> Value {
    json!({ "name": name, "description": description, "inputSchema": { "type": "object", "properties": { "text": { "type": "string" } } } })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args
        .iter()
        .position(|a| a == "--mode")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "stateless".into());
    if mode == "die" {
        eprintln!("boom: missing configuration");
        std::process::exit(1);
    }
    if mode == "noise" {
        println!("Mock MCP server v0 starting (this line is not JSON)");
    }
    let (sender, receiver) = mpsc::channel::<Value>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && sender.send(value).is_err()
            {
                return;
            }
        }
    });
    if mode == "mute" {
        while receiver.recv().is_ok() {}
        return;
    }

    let stateless = matches!(mode.as_str(), "stateless" | "noise");
    let mut initialized = false;
    let mut ping_answered = false;
    let mut backlog: VecDeque<Value> = VecDeque::new();
    loop {
        let message = match backlog.pop_front() {
            Some(m) => m,
            None => match receiver.recv() {
                Ok(m) => m,
                Err(_) => return,
            },
        };
        let method = message["method"].as_str().unwrap_or("").to_string();
        let id = message.get("id").cloned();
        let Some(id) = id else {
            if method == "notifications/initialized" {
                initialized = true;
                send(&json!({ "jsonrpc": "2.0", "id": "srv-1", "method": "ping" }));
            }
            continue;
        };
        if method.is_empty() {
            ping_answered = message["result"].is_object();
            continue;
        }
        let params = message["params"].clone();
        if mode == "unsupported" {
            error(
                &id,
                -32022,
                "Unsupported protocol version",
                Some(json!({ "supported": ["2099-01-01"], "requested": "2026-07-28" })),
            );
            continue;
        }
        if stateless {
            let meta = &params["_meta"];
            if meta["io.modelcontextprotocol/protocolVersion"] != "2026-07-28"
                || !meta["io.modelcontextprotocol/clientCapabilities"].is_object()
            {
                error(&id, -32602, "missing required _meta fields", None);
                continue;
            }
        }
        match method.as_str() {
            "server/discover" if stateless => reply(
                &id,
                json!({ "resultType": "complete", "supportedVersions": ["2026-07-28"], "capabilities": { "tools": {} }, "instructions": "Test mock server.",
                        "_meta": { "io.modelcontextprotocol/serverInfo": { "name": "mock", "version": "0" } } }),
            ),
            "server/discover" if mode == "legacy-silent" => {}
            "server/discover" => error(&id, -32601, "Method not found", None),
            "initialize" if !stateless => {
                let version = if mode == "legacy-bad-version" {
                    "1999-01-01"
                } else {
                    "2025-06-18"
                };
                reply(
                    &id,
                    json!({ "protocolVersion": version, "capabilities": { "tools": {} }, "serverInfo": { "name": "mock", "version": "0" } }),
                );
            }
            "tools/list" | "tools/call" if !stateless && !initialized => {
                error(&id, -32600, "not initialized", None)
            }
            "tools/list" => {
                if params["cursor"].as_str() == Some("p2") {
                    let tools = [
                        "slow",
                        "crash",
                        "noreply",
                        "needs_input",
                        "env",
                        "image",
                        "structured",
                        "ping_ok",
                    ]
                    .map(|n| tool(n, "test tool"));
                    reply(&id, json!({ "resultType": "complete", "tools": tools }));
                } else {
                    let tools =
                        ["echo", "fail", "dotted.name", "big"].map(|n| tool(n, "test tool"));
                    reply(
                        &id,
                        json!({ "resultType": "complete", "tools": tools, "nextCursor": "p2" }),
                    );
                }
            }
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or("");
                match name {
                    "echo" => reply(
                        &id,
                        text(params["arguments"]["text"].as_str().unwrap_or("")),
                    ),
                    "fail" => reply(
                        &id,
                        json!({ "resultType": "complete", "content": [{ "type": "text", "text": "intended failure" }], "isError": true }),
                    ),
                    "dotted.name" => reply(&id, text("ok dotted")),
                    "big" => reply(&id, text(&"é".repeat(60_000))),
                    "env" => reply(
                        &id,
                        text(&std::env::var("FOCUS_MOCK_VAR").unwrap_or_else(|_| "(unset)".into())),
                    ),
                    "image" => reply(
                        &id,
                        json!({ "resultType": "complete", "content": [{ "type": "text", "text": "here" }, { "type": "image", "data": "AAAA", "mimeType": "image/png" }] }),
                    ),
                    "structured" => reply(
                        &id,
                        json!({ "resultType": "complete", "content": [], "structuredContent": { "n": 7 } }),
                    ),
                    "needs_input" => reply(
                        &id,
                        json!({ "resultType": "input_required", "inputRequests": {} }),
                    ),
                    "ping_ok" => reply(&id, text(if ping_answered { "yes" } else { "no" })),
                    "crash" => {
                        eprintln!("simulated panic in the crash tool");
                        std::process::exit(3);
                    }
                    "noreply" => {}
                    "slow" => {
                        let mut cancelled = false;
                        let started = std::time::Instant::now();
                        while started.elapsed() < Duration::from_secs(10) && !cancelled {
                            match receiver.recv_timeout(Duration::from_millis(20)) {
                                Ok(m)
                                    if m["method"] == "notifications/cancelled"
                                        && m["params"]["requestId"] == id =>
                                {
                                    cancelled = true
                                }
                                Ok(m) => backlog.push_back(m),
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                                Err(mpsc::RecvTimeoutError::Disconnected) => return,
                            }
                        }
                        if cancelled {
                            if let Ok(path) = std::env::var("MOCK_CANCEL_FILE") {
                                let _ = std::fs::write(path, format!("cancelled {id}\n"));
                            }
                        } else {
                            reply(&id, text("too late"));
                        }
                    }
                    _ => error(&id, -32602, "Unknown tool", None),
                }
            }
            _ => error(&id, -32601, "Method not found", None),
        }
    }
}
