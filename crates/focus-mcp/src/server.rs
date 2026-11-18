use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use focus_ai::agent::{self, ToolOutcome};
use focus_ai::{Cancel, ToolCall};
use serde_json::{Value, json};

use crate::catalog::{McpHost, PAGE_SIZE};

pub const PROTOCOL_STATELESS: &str = "2026-07-28";
const PROTOCOL_LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const MAX_LINE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_IN_FLIGHT: usize = 16;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const UNSUPPORTED_VERSION: i64 = -32022;

#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub instructions: String,
}

type Output = Arc<Mutex<Box<dyn Write + Send>>>;
type InFlight = Arc<Mutex<HashMap<String, Cancel>>>;

fn send(output: &Output, message: &Value) {
    if let Ok(mut out) = output.lock() {
        let _ = writeln!(out, "{message}").and_then(|()| out.flush());
    }
}

fn reply(output: &Output, id: &Value, result: Value) {
    send(
        output,
        &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
    );
}

fn fail(output: &Output, id: &Value, code: i64, message: &str, data: Option<Value>) {
    let mut error = json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    send(
        output,
        &json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    );
}

fn capabilities(host: &Arc<dyn McpHost>, legacy: bool) -> Value {
    let mut caps =
        json!({ "tools": if legacy { json!({ "listChanged": false }) } else { json!({}) } });
    if host.resources(0, 0).is_some() {
        caps["resources"] = json!({});
    }
    if host.prompts().is_some() {
        caps["prompts"] = json!({});
    }
    caps
}

fn supported_versions() -> Vec<&'static str> {
    std::iter::once(PROTOCOL_STATELESS)
        .chain(PROTOCOL_LEGACY.iter().copied())
        .collect()
}

pub fn serve(
    input: impl BufRead,
    output: impl Write + Send + 'static,
    host: Arc<dyn McpHost>,
    info: &ServerInfo,
) -> std::io::Result<()> {
    let output: Output = Arc::new(Mutex::new(Box::new(output)));
    let in_flight: InFlight = Arc::new(Mutex::new(HashMap::new()));
    let mut workers: Vec<JoinHandle<()>> = Vec::new();
    let mut initialized = false;
    let mut input = input;
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = (&mut input)
            .take(MAX_LINE_BYTES)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        if !line.ends_with(b"\n") && read as u64 >= MAX_LINE_BYTES {
            fail(
                &output,
                &Value::Null,
                INVALID_REQUEST,
                "message too long",
                None,
            );
            break;
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(text) {
            Ok(m) => m,
            Err(e) => {
                fail(
                    &output,
                    &Value::Null,
                    PARSE_ERROR,
                    &format!("invalid JSON: {e}"),
                    None,
                );
                continue;
            }
        };
        workers.retain(|w| !w.is_finished());
        handle(
            &message,
            &output,
            &host,
            info,
            &in_flight,
            &mut initialized,
            &mut workers,
        );
    }
    for worker in workers {
        let _ = worker.join();
    }
    Ok(())
}

fn handle(
    message: &Value,
    output: &Output,
    host: &Arc<dyn McpHost>,
    info: &ServerInfo,
    in_flight: &InFlight,
    initialized: &mut bool,
    workers: &mut Vec<JoinHandle<()>>,
) {
    if !message.is_object() {
        fail(
            output,
            &Value::Null,
            INVALID_REQUEST,
            "invalid request: a JSON object is expected",
            None,
        );
        return;
    }
    let method = message.get("method").and_then(Value::as_str);
    let id = message.get("id").filter(|i| i.is_string() || i.is_number());
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let (Some(method), Some(id)) = (method, id) else {
        if let Some("notifications/cancelled") = method
            && let Some(request_id) = params.get("requestId")
            && let Ok(map) = in_flight.lock()
            && let Some(cancel) = map.get(&request_id.to_string())
        {
            cancel.cancel();
        }
        if method.is_none() && message.get("id").is_none() {
            fail(
                output,
                &Value::Null,
                INVALID_REQUEST,
                "invalid request: `method` is missing",
                None,
            );
        }
        return;
    };

    let meta_version = params
        .pointer("/_meta/io.modelcontextprotocol~1protocolVersion")
        .and_then(Value::as_str);
    let stateless = meta_version.is_some();
    if let Some(version) = meta_version
        && version != PROTOCOL_STATELESS
    {
        fail(
            output,
            id,
            UNSUPPORTED_VERSION,
            "Unsupported protocol version",
            Some(json!({ "supported": supported_versions(), "requested": version })),
        );
        return;
    }

    match method {
        "ping" => reply(output, id, json!({})),
        "initialize" => {
            *initialized = true;
            let requested = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("");
            let version = PROTOCOL_LEGACY
                .iter()
                .find(|v| **v == requested)
                .copied()
                .unwrap_or(PROTOCOL_LEGACY[0]);
            reply(
                output,
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": capabilities(host, true),
                    "serverInfo": { "name": info.name, "version": info.version },
                    "instructions": info.instructions,
                }),
            );
        }
        "server/discover" => reply(
            output,
            id,
            json!({
                "resultType": "complete",
                "supportedVersions": supported_versions(),
                "capabilities": capabilities(host, false),
                "instructions": info.instructions,
                "_meta": { "io.modelcontextprotocol/serverInfo": { "name": info.name, "version": info.version } },
            }),
        ),
        "tools/list" | "tools/call" | "resources/list" | "resources/read" | "prompts/list"
        | "prompts/get"
            if !stateless && !*initialized =>
        {
            fail(
                output,
                id,
                INVALID_PARAMS,
                "request without `_meta.io.modelcontextprotocol/protocolVersion` and no prior `initialize`",
                None,
            );
        }
        "tools/list" => {
            let tools: Vec<Value> = host
                .specs()
                .into_iter()
                .map(|spec| {
                    let read_only = !agent::is_write_tool(&spec.name);
                    json!({ "name": spec.name, "description": spec.description, "inputSchema": spec.parameters, "annotations": { "readOnlyHint": read_only, "destructiveHint": false } })
                })
                .collect();
            let mut result = json!({ "tools": tools });
            if stateless {
                result["resultType"] = json!("complete");
            }
            reply(output, id, result);
        }
        "resources/list" => {
            let offset = cursor_offset(&params);
            match host.resources(offset, PAGE_SIZE) {
                None => fail(
                    output,
                    id,
                    METHOD_NOT_FOUND,
                    "Method not found: resources/list",
                    None,
                ),
                Some((page, next)) => {
                    let resources: Vec<Value> = page.into_iter().map(|r| json!({ "uri": r.uri, "name": r.name, "description": r.description, "mimeType": "text/markdown" })).collect();
                    let mut result = json!({ "resources": resources });
                    if let Some(next) = next {
                        result["nextCursor"] = json!(next.to_string());
                    }
                    if stateless {
                        result["resultType"] = json!("complete");
                    }
                    reply(output, id, result);
                }
            }
        }
        "resources/read" => {
            let Some(uri) = params.get("uri").and_then(Value::as_str) else {
                fail(
                    output,
                    id,
                    INVALID_PARAMS,
                    "parameter `uri` is missing",
                    None,
                );
                return;
            };
            match host.read_resource(uri) {
                Ok(text) => {
                    let mut result = json!({ "contents": [{ "uri": uri, "mimeType": "text/markdown", "text": text }] });
                    if stateless {
                        result["resultType"] = json!("complete");
                    }
                    reply(output, id, result);
                }
                Err(message) => fail(
                    output,
                    id,
                    INVALID_PARAMS,
                    "Resource not found",
                    Some(json!({ "uri": uri, "reason": message })),
                ),
            }
        }
        "prompts/list" => match host.prompts() {
            None => fail(
                output,
                id,
                METHOD_NOT_FOUND,
                "Method not found: prompts/list",
                None,
            ),
            Some(list) => {
                let prompts: Vec<Value> = list
                    .into_iter()
                    .map(|p| json!({ "name": p.name, "description": p.description, "arguments": p.arguments.iter().map(|a| json!({ "name": a.name, "description": a.description, "required": a.required })).collect::<Vec<_>>() }))
                    .collect();
                let mut result = json!({ "prompts": prompts });
                if stateless {
                    result["resultType"] = json!("complete");
                }
                reply(output, id, result);
            }
        },
        "prompts/get" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                fail(
                    output,
                    id,
                    INVALID_PARAMS,
                    "parameter `name` is missing",
                    None,
                );
                return;
            };
            let empty = serde_json::Map::new();
            let arguments = params
                .get("arguments")
                .and_then(Value::as_object)
                .unwrap_or(&empty);
            match host.prompt(name, arguments) {
                Ok((description, text)) => {
                    let mut result = json!({ "description": description, "messages": [{ "role": "user", "content": { "type": "text", "text": text } }] });
                    if stateless {
                        result["resultType"] = json!("complete");
                    }
                    reply(output, id, result);
                }
                Err(message) => fail(output, id, INVALID_PARAMS, &message, None),
            }
        }
        "tools/call" => call_tool(&params, id, stateless, output, host, in_flight, workers),
        _ => fail(
            output,
            id,
            METHOD_NOT_FOUND,
            &format!("Method not found: {method}"),
            None,
        ),
    }
}

fn cursor_offset(params: &Value) -> usize {
    params
        .get("cursor")
        .and_then(Value::as_str)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0)
}

fn call_tool(
    params: &Value,
    id: &Value,
    stateless: bool,
    output: &Output,
    host: &Arc<dyn McpHost>,
    in_flight: &InFlight,
    workers: &mut Vec<JoinHandle<()>>,
) {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        fail(
            output,
            id,
            INVALID_PARAMS,
            "parameter `name` is missing",
            None,
        );
        return;
    };
    if !host.specs().iter().any(|s| s.name == name) {
        fail(
            output,
            id,
            INVALID_PARAMS,
            &format!("Unknown tool: {name}"),
            None,
        );
        return;
    }
    let arguments = match params.get("arguments") {
        None | Some(Value::Null) => "{}".to_string(),
        Some(value @ Value::Object(_)) => value.to_string(),
        Some(_) => {
            fail(
                output,
                id,
                INVALID_PARAMS,
                "`arguments` must be an object",
                None,
            );
            return;
        }
    };
    if in_flight.lock().is_ok_and(|map| map.len() >= MAX_IN_FLIGHT) {
        fail(
            output,
            id,
            INVALID_REQUEST,
            "too many calls in progress: wait for an answer",
            None,
        );
        return;
    }
    let call = ToolCall {
        id: id.to_string(),
        name: name.to_string(),
        arguments,
    };
    let cancel = Cancel::new();
    let key = id.to_string();
    if let Ok(mut map) = in_flight.lock() {
        map.insert(key.clone(), cancel.clone());
    }
    let (output_for_error, id_for_error) = (output.clone(), id.clone());
    let (output, host, in_flight, id) =
        (output.clone(), host.clone(), in_flight.clone(), id.clone());
    let spawned = std::thread::Builder::new()
        .name("focus-mcp-call".into())
        .spawn(move || {
            let outcome = host.call(&call, &cancel);
            if let Ok(mut map) = in_flight.lock() {
                map.remove(&key);
            }
            if cancel.is_cancelled() {
                return;
            }
            let (text, is_error) = match outcome {
                ToolOutcome::Ok(text) => (text, false),
                ToolOutcome::Error(text) => (text, true),
                ToolOutcome::Denied => (crate::fault::Fault::denied().to_json().to_string(), true),
            };
            let mut result =
                json!({ "content": [{ "type": "text", "text": text }], "isError": is_error });
            if stateless {
                result["resultType"] = json!("complete");
            }
            reply(&output, &id, result);
        });
    match spawned {
        Ok(handle) => workers.push(handle),
        Err(e) => fail(
            &output_for_error,
            &id_for_error,
            -32603,
            &format!("execution thread unavailable: {e}"),
            None,
        ),
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use focus_ai::ToolSpec;
    use focus_ai::agent::ToolHost;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    struct Fake;

    impl ToolHost for Fake {
        fn specs(&self) -> Vec<ToolSpec> {
            ["echo", "slow", "boom"]
                .iter()
                .map(|n| ToolSpec {
                    name: n.to_string(),
                    description: format!("outil {n}"),
                    parameters: json!({ "type": "object", "properties": {} }),
                })
                .collect()
        }
        fn call(&self, call: &ToolCall, cancel: &Cancel) -> ToolOutcome {
            match call.name.as_str() {
                "echo" => ToolOutcome::Ok(call.arguments.clone()),
                "slow" => {
                    let started = Instant::now();
                    while started.elapsed() < Duration::from_secs(5) {
                        if cancel.is_cancelled() {
                            return ToolOutcome::Error("cancelled".into());
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    ToolOutcome::Ok("trop tard".into())
                }
                _ => ToolOutcome::Error("deliberate failure".into()),
            }
        }
    }

    impl McpHost for Fake {}

    struct Session {
        writer: UnixStream,
        reader: BufReader<UnixStream>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    fn start() -> Session {
        let (client, server) = UnixStream::pair().unwrap();
        let (server_in, server_out) = (server.try_clone().unwrap(), server);
        let info = ServerInfo {
            name: "fake".into(),
            version: "1".into(),
            instructions: "consignes".into(),
        };
        let thread = std::thread::spawn(move || {
            serve(BufReader::new(server_in), server_out, Arc::new(Fake), &info).unwrap();
        });
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        Session {
            writer: client.try_clone().unwrap(),
            reader: BufReader::new(client),
            thread: Some(thread),
        }
    }

    impl Session {
        fn send(&mut self, value: Value) {
            writeln!(self.writer, "{value}").unwrap();
        }
        fn read(&mut self) -> Value {
            let mut line = String::new();
            self.reader.read_line(&mut line).unwrap();
            serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("unreadable response {line:?}: {e}"))
        }
        fn ask(&mut self, value: Value) -> Value {
            self.send(value);
            self.read()
        }
        fn finish(mut self) -> Vec<String> {
            self.writer.shutdown(std::net::Shutdown::Write).unwrap();
            self.thread.take().unwrap().join().unwrap();
            let mut rest = Vec::new();
            let mut line = String::new();
            while self.reader.read_line(&mut line).unwrap_or(0) > 0 {
                rest.push(line.trim().to_string());
                line.clear();
            }
            rest
        }
    }

    fn stateless_meta() -> Value {
        json!({ "io.modelcontextprotocol/protocolVersion": PROTOCOL_STATELESS, "io.modelcontextprotocol/clientCapabilities": {} })
    }

    #[test]
    fn stateless_discover_list_and_call() {
        let mut s = start();
        let discover = s.ask(json!({ "jsonrpc": "2.0", "id": "d1", "method": "server/discover", "params": { "_meta": stateless_meta() } }));
        assert_eq!(discover["id"], "d1");
        assert_eq!(discover["result"]["resultType"], "complete");
        assert_eq!(
            discover["result"]["supportedVersions"][0],
            PROTOCOL_STATELESS
        );
        assert!(
            discover["result"]["supportedVersions"]
                .as_array()
                .unwrap()
                .len()
                >= 2
        );
        assert_eq!(discover["result"]["capabilities"], json!({ "tools": {} }));
        assert_eq!(
            discover["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
            "fake"
        );
        let list = s.ask(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": { "_meta": stateless_meta() } }));
        assert_eq!(list["result"]["resultType"], "complete");
        assert_eq!(list["result"]["tools"][0]["name"], "echo");
        assert_eq!(
            list["result"]["tools"][0]["annotations"]["readOnlyHint"],
            true
        );
        let call = s.ask(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "echo", "arguments": { "a": 1 }, "_meta": stateless_meta() } }));
        assert_eq!(call["result"]["content"][0]["text"], r#"{"a":1}"#);
        assert_eq!(call["result"]["isError"], false);
        assert_eq!(call["result"]["resultType"], "complete");
        let failed = s.ask(json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "boom", "_meta": stateless_meta() } }));
        assert_eq!(failed["result"]["isError"], true);
        assert_eq!(failed["result"]["content"][0]["text"], "deliberate failure");
        assert!(s.finish().is_empty());
    }

    #[test]
    fn legacy_initialize_flow_and_version_negotiation() {
        let mut s = start();
        let before = s.ask(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }));
        assert_eq!(
            before["error"]["code"], INVALID_PARAMS,
            "ni session ni _meta"
        );
        let init = s.ask(json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize", "params": { "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } } }));
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "fake");
        assert_eq!(
            init["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
        assert_eq!(init["result"]["instructions"], "consignes");
        s.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        let list = s.ask(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }));
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 3);
        assert!(
            list["result"].get("resultType").is_none(),
            "an older client gets no resultType"
        );
        let call = s.ask(json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "echo", "arguments": { "x": "é" } } }));
        assert_eq!(call["result"]["content"][0]["text"], r#"{"x":"é"}"#);
        let unknown_version = s.ask(json!({ "jsonrpc": "2.0", "id": 5, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } }));
        assert_eq!(
            unknown_version["result"]["protocolVersion"], "2025-11-25",
            "unknown version: the server proposes its own"
        );
        assert!(s.finish().is_empty());
    }

    #[test]
    fn ping_errors_and_malformed_input() {
        let mut s = start();
        assert_eq!(
            s.ask(json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }))["result"],
            json!({})
        );
        assert_eq!(s.ask(json!({ "jsonrpc": "2.0", "id": 2, "method": "inconnue", "params": { "_meta": stateless_meta() } }))["error"]["code"], METHOD_NOT_FOUND);
        let unsupported = s.ask(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": { "_meta": { "io.modelcontextprotocol/protocolVersion": "1900-01-01" } } }));
        assert_eq!(unsupported["error"]["code"], UNSUPPORTED_VERSION);
        assert_eq!(unsupported["error"]["data"]["requested"], "1900-01-01");
        assert!(
            unsupported["error"]["data"]["supported"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == PROTOCOL_STATELESS)
        );
        let unknown_tool = s.ask(json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "absent", "_meta": stateless_meta() } }));
        assert_eq!(unknown_tool["error"]["code"], INVALID_PARAMS);
        assert!(
            unknown_tool["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Unknown tool")
        );
        let bad_arguments = s.ask(json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "echo", "arguments": [1], "_meta": stateless_meta() } }));
        assert_eq!(bad_arguments["error"]["code"], INVALID_PARAMS);
        let no_name = s.ask(json!({ "jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": { "_meta": stateless_meta() } }));
        assert_eq!(no_name["error"]["code"], INVALID_PARAMS);
        s.writer.write_all(b"not json\n").unwrap();
        let parse = s.read();
        assert_eq!(
            (parse["error"]["code"].as_i64(), parse["id"].clone()),
            (Some(PARSE_ERROR), Value::Null)
        );
        assert_eq!(s.ask(json!([1, 2]))["error"]["code"], INVALID_REQUEST);
        s.writer.write_all(b"\n   \n").unwrap();
        assert_eq!(s.ask(json!({ "jsonrpc": "2.0", "id": 7, "method": "resources/list", "params": { "_meta": stateless_meta() } }))["error"]["code"], METHOD_NOT_FOUND, "the fake host serves no resources");
        assert_eq!(s.ask(json!({ "jsonrpc": "2.0", "id": 8, "method": "prompts/list", "params": { "_meta": stateless_meta() } }))["error"]["code"], METHOD_NOT_FOUND);
        assert!(s.finish().is_empty());
    }

    #[test]
    fn cancelled_calls_get_no_response_and_concurrent_calls_overlap() {
        let mut s = start();
        s.send(json!({ "jsonrpc": "2.0", "id": 10, "method": "tools/call", "params": { "name": "slow", "_meta": stateless_meta() } }));
        s.send(json!({ "jsonrpc": "2.0", "id": 11, "method": "tools/call", "params": { "name": "echo", "arguments": { "n": 1 }, "_meta": stateless_meta() } }));
        let early = s.read();
        assert_eq!(early["id"], 11);
        let started = Instant::now();
        s.send(json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": 10, "reason": "test" } }));
        let after = s.ask(json!({ "jsonrpc": "2.0", "id": 12, "method": "ping" }));
        assert_eq!(after["id"], 12);
        let rest = s.finish();
        assert!(
            rest.is_empty(),
            "no response for the cancelled request: {rest:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn closing_input_lets_running_calls_finish_and_answer() {
        let mut s = start();
        s.send(json!({ "jsonrpc": "2.0", "id": 20, "method": "tools/call", "params": { "name": "boom", "_meta": stateless_meta() } }));
        let rest = s.finish();
        assert_eq!(
            rest.len(),
            1,
            "the call finished and answered before the server returned: {rest:?}"
        );
        assert!(rest[0].contains("\"id\":20") && rest[0].contains("deliberate failure"));
    }
}
