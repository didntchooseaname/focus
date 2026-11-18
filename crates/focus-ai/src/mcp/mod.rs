mod host;
mod process;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::agent::ToolOutcome;
use crate::{Cancel, ToolSpec};
pub use host::{CompositeHost, ConfirmRequest};
use process::Connection;

pub const PROTOCOL_STATELESS: &str = "2026-07-28";
const PROTOCOL_LEGACY: &str = "2025-11-25";
const KNOWN_LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const UNSUPPORTED_VERSION: i64 = -32022;
const MAX_TOOLS: usize = 1000;
const MAX_PAGES: usize = 50;
const MAX_DESCRIPTION_CHARS: usize = 2000;
const MAX_NAME_CHARS: usize = 128;

fn enabled_by_default() -> bool {
    true
}

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

impl std::fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConfig")
            .field("name", &self.name)
            .field("command", &self.command)
            .field("args", &self.args)
            .field("env", &self.env.keys().collect::<Vec<_>>())
            .field("allowed_tools", &self.allowed_tools)
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("MCP server \"{server}\": invalid configuration: {reason}")]
    InvalidConfig { server: String, reason: String },
    #[error("MCP server \"{server}\": cannot start `{command}`: {reason}")]
    Spawn {
        server: String,
        command: String,
        reason: String,
    },
    #[error("MCP server \"{server}\": no response to `{method}` after {secs} s{stderr}")]
    Timeout {
        server: String,
        method: String,
        secs: u64,
        stderr: String,
    },
    #[error("MCP server \"{server}\": the process exited{detail}")]
    Exited { server: String, detail: String },
    #[error("MCP server \"{server}\": {reason}")]
    Protocol { server: String, reason: String },
    #[error("cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, Copy)]
pub struct StartOptions {
    pub probe: Duration,
    pub startup: Duration,
}

impl Default for StartOptions {
    fn default() -> Self {
        StartOptions {
            probe: Duration::from_secs(5),
            startup: Duration::from_secs(30),
        }
    }
}

struct ToolEntry {
    qualified: String,
    raw: String,
    spec: ToolSpec,
}

pub struct Server {
    config: ServerConfig,
    connection: Connection,
    version: String,
    stateless: bool,
    instructions: Option<String>,
    tools: Vec<ToolEntry>,
}

impl Server {
    pub fn start(config: &ServerConfig) -> Result<Server, McpError> {
        Server::start_with(config, StartOptions::default())
    }

    pub fn start_with(config: &ServerConfig, options: StartOptions) -> Result<Server, McpError> {
        validate(config)?;
        let connection = Connection::spawn(config)?;
        let deadline = Instant::now() + options.startup;
        let (stateless, version, instructions, has_tools) =
            handshake(&connection, config, options, deadline)?;
        let raw_tools = if has_tools {
            list_tools(&connection, stateless, deadline)?
        } else {
            Vec::new()
        };
        let tools = name_tools(&config.name, raw_tools);
        Ok(Server {
            config: config.clone(),
            connection,
            version,
            stateless,
            instructions,
            tools,
        })
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }

    pub fn process_id(&self) -> u32 {
        self.connection.pid
    }

    pub fn protocol_version(&self) -> &str {
        &self.version
    }

    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }

    pub fn tools(&self) -> Vec<ToolSpec> {
        self.tools.iter().map(|t| t.spec.clone()).collect()
    }

    pub fn owns(&self, qualified_name: &str) -> bool {
        self.tools.iter().any(|t| t.qualified == qualified_name)
    }

    pub fn requires_confirmation(&self, qualified_name: &str) -> bool {
        match self.tools.iter().find(|t| t.qualified == qualified_name) {
            Some(tool) => !self.config.allowed_tools.contains(&tool.raw),
            None => true,
        }
    }

    pub fn raw_name(&self, qualified_name: &str) -> Option<&str> {
        self.tools
            .iter()
            .find(|t| t.qualified == qualified_name)
            .map(|t| t.raw.as_str())
    }

    pub fn call(
        &self,
        qualified_name: &str,
        arguments: &str,
        cancel: &Cancel,
        timeout: Duration,
    ) -> ToolOutcome {
        let Some(tool) = self.tools.iter().find(|t| t.qualified == qualified_name) else {
            return ToolOutcome::Error(format!(
                "unknown tool for server \"{}\": {qualified_name}",
                self.config.name
            ));
        };
        let arguments: Value = if arguments.trim().is_empty() {
            json!({})
        } else {
            match serde_json::from_str(arguments) {
                Ok(value @ Value::Object(_)) => value,
                Ok(_) => return ToolOutcome::Error("the arguments must form a JSON object".into()),
                Err(e) => return ToolOutcome::Error(format!("invalid JSON arguments: {e}")),
            }
        };
        let params = with_meta(
            self.stateless,
            json!({ "name": tool.raw, "arguments": arguments }),
        );
        match self
            .connection
            .request("tools/call", params, cancel, timeout)
        {
            Ok(Ok(result)) => outcome_from_result(&result),
            Ok(Err(rpc)) => ToolOutcome::Error(format!(
                "server \"{}\" rejected the call ({}): {}",
                self.config.name, rpc.code, rpc.message
            )),
            Err(error) => ToolOutcome::Error(error.to_string()),
        }
    }

    pub fn stop(&self) {
        self.connection.stop();
    }
}

fn validate(config: &ServerConfig) -> Result<(), McpError> {
    let invalid = |reason: &str| McpError::InvalidConfig {
        server: config.name.clone(),
        reason: reason.into(),
    };
    if sanitize(&config.name).is_empty() {
        return Err(invalid("the name must contain a letter or a digit"));
    }
    if config.command.trim().is_empty() {
        return Err(invalid("the command is empty"));
    }
    if !config.enabled {
        return Err(invalid("the server is disabled"));
    }
    Ok(())
}

fn with_meta(stateless: bool, mut params: Value) -> Value {
    if stateless {
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": PROTOCOL_STATELESS,
            "io.modelcontextprotocol/clientInfo": client_info(),
            "io.modelcontextprotocol/clientCapabilities": {},
        });
    }
    params
}

fn client_info() -> Value {
    json!({ "name": "focus", "version": env!("CARGO_PKG_VERSION") })
}

fn handshake(
    connection: &Connection,
    config: &ServerConfig,
    options: StartOptions,
    deadline: Instant,
) -> Result<(bool, String, Option<String>, bool), McpError> {
    let protocol = |reason: String| McpError::Protocol {
        server: config.name.clone(),
        reason,
    };
    let cancel = Cancel::new();
    let remaining = |limit: Duration| limit.min(deadline.saturating_duration_since(Instant::now()));

    let probe = connection.request(
        "server/discover",
        with_meta(true, json!({})),
        &cancel,
        remaining(options.probe),
    );
    match probe {
        Ok(Ok(result)) => {
            let versions: Vec<&str> = result["supportedVersions"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if versions.contains(&PROTOCOL_STATELESS) {
                let has_tools = !result["capabilities"]["tools"].is_null();
                let instructions = result["instructions"].as_str().map(str::to_string);
                return Ok((
                    true,
                    PROTOCOL_STATELESS.to_string(),
                    instructions,
                    has_tools,
                ));
            }
            if !versions.iter().any(|v| KNOWN_LEGACY.contains(v)) {
                return Err(protocol(format!(
                    "unsupported protocol versions: {}",
                    versions.join(", ")
                )));
            }
        }
        Ok(Err(rpc)) if rpc.code == UNSUPPORTED_VERSION => {
            let supported: Vec<&str> = rpc
                .data
                .as_ref()
                .and_then(|d| d["supported"].as_array())
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if !supported.iter().any(|v| KNOWN_LEGACY.contains(v)) {
                return Err(protocol(format!(
                    "unsupported protocol versions: {}",
                    supported.join(", ")
                )));
            }
        }
        Ok(Err(_)) | Err(McpError::Timeout { .. }) => {}
        Err(error) => return Err(error),
    }

    let params = json!({ "protocolVersion": PROTOCOL_LEGACY, "capabilities": {}, "clientInfo": client_info() });
    let result =
        match connection.request("initialize", params, &cancel, remaining(options.startup))? {
            Ok(result) => result,
            Err(rpc) => {
                return Err(protocol(format!(
                    "`initialize` rejected ({}): {}",
                    rpc.code, rpc.message
                )));
            }
        };
    let version = result["protocolVersion"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if !KNOWN_LEGACY.contains(&version.as_str()) {
        return Err(protocol(format!(
            "unsupported protocol version: {version:?}"
        )));
    }
    connection.notify("notifications/initialized", json!({}));
    let has_tools = !result["capabilities"]["tools"].is_null();
    let instructions = result["instructions"].as_str().map(str::to_string);
    Ok((false, version, instructions, has_tools))
}

fn list_tools(
    connection: &Connection,
    stateless: bool,
    deadline: Instant,
) -> Result<Vec<Value>, McpError> {
    let cancel = Cancel::new();
    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut params = json!({});
        if let Some(cursor) = &cursor {
            params["cursor"] = json!(cursor);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let result = match connection.request(
            "tools/list",
            with_meta(stateless, params),
            &cancel,
            remaining,
        )? {
            Ok(result) => result,
            Err(rpc) => {
                return Err(connection.protocol_error(format!(
                    "`tools/list` rejected ({}): {}",
                    rpc.code, rpc.message
                )));
            }
        };
        if let Some(page) = result["tools"].as_array() {
            tools.extend(page.iter().cloned());
        }
        match result["nextCursor"].as_str().filter(|c| !c.is_empty()) {
            Some(next) if tools.len() < MAX_TOOLS => cursor = Some(next.to_string()),
            _ => break,
        }
    }
    tools.truncate(MAX_TOOLS);
    Ok(tools)
}

fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.chars().any(|c| c.is_ascii_alphanumeric()) {
        cleaned
    } else {
        String::new()
    }
}

fn name_tools(server: &str, raw_tools: Vec<Value>) -> Vec<ToolEntry> {
    let prefix = sanitize(server);
    let mut entries: Vec<ToolEntry> = Vec::new();
    for raw in raw_tools {
        let Some(raw_name) = raw["name"].as_str().filter(|n| !n.is_empty()) else {
            continue;
        };
        let tool = sanitize(raw_name);
        if tool.is_empty() {
            continue;
        }
        let mut qualified: String = format!("{prefix}__{tool}")
            .chars()
            .take(MAX_NAME_CHARS)
            .collect();
        let mut suffix = 2;
        while entries.iter().any(|e| e.qualified == qualified) {
            let tail = format!("_{suffix}");
            let keep = MAX_NAME_CHARS - tail.len();
            qualified = format!(
                "{}{tail}",
                format!("{prefix}__{tool}")
                    .chars()
                    .take(keep)
                    .collect::<String>()
            );
            suffix += 1;
        }
        let description: String = raw["description"]
            .as_str()
            .or_else(|| raw["title"].as_str())
            .unwrap_or("")
            .chars()
            .take(MAX_DESCRIPTION_CHARS)
            .collect();
        let mut parameters = raw["inputSchema"].clone();
        match parameters.as_object_mut() {
            Some(schema) => {
                schema.remove("$schema");
                schema.entry("type").or_insert_with(|| json!("object"));
            }
            None => parameters = json!({ "type": "object", "properties": {} }),
        }
        entries.push(ToolEntry {
            qualified: qualified.clone(),
            raw: raw_name.to_string(),
            spec: ToolSpec {
                name: qualified,
                description,
                parameters,
            },
        });
    }
    entries
}

pub(crate) fn outcome_from_result(result: &Value) -> ToolOutcome {
    match result["resultType"].as_str().unwrap_or("complete") {
        "complete" => {}
        "input_required" => {
            return ToolOutcome::Error(
                "the server asks for user input during the call, which Focus does not support"
                    .into(),
            );
        }
        other => return ToolOutcome::Error(format!("unknown result type: {other}")),
    }
    let mut parts: Vec<String> = Vec::new();
    for item in result["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        match item["type"].as_str().unwrap_or("") {
            "text" => parts.push(item["text"].as_str().unwrap_or("").to_string()),
            "image" | "audio" => parts.push(format!(
                "[{} content not forwarded: {}]",
                item["type"].as_str().unwrap_or(""),
                item["mimeType"].as_str().unwrap_or("unknown type")
            )),
            "resource_link" => parts.push(format!(
                "[resource: {}]",
                item["uri"].as_str().unwrap_or("?")
            )),
            "resource" => match item["resource"]["text"].as_str() {
                Some(text) => parts.push(text.to_string()),
                None => parts.push(format!(
                    "[binary resource: {}]",
                    item["resource"]["uri"].as_str().unwrap_or("?")
                )),
            },
            other => parts.push(format!("[unsupported content type {other}]")),
        }
    }
    let mut text = parts.join("\n");
    if text.trim().is_empty()
        && let Some(structured) = result.get("structuredContent").filter(|v| !v.is_null())
    {
        text = structured.to_string();
    }
    let text = crate::agent::truncate(&text);
    if result["isError"].as_bool() == Some(true) {
        ToolOutcome::Error(text)
    } else {
        ToolOutcome::Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sanitised_unique_and_bounded() {
        let tools = vec![
            json!({"name": "read.file"}),
            json!({"name": "read file"}),
            json!({"name": "x".repeat(300)}),
            json!({"name": "..."}),
            json!({"description": "no name"}),
        ];
        let entries = name_tools("my server", tools);
        let names: Vec<&str> = entries.iter().map(|e| e.qualified.as_str()).collect();
        assert_eq!(names[0], "my_server__read_file");
        assert_eq!(names[1], "my_server__read_file_2");
        assert_eq!(names[2].len(), 128);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].raw, "read.file");
        assert!(names.iter().all(|n| {
            n.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        }));
    }

    #[test]
    fn schemas_are_normalised() {
        let entries = name_tools(
            "s",
            vec![
                json!({"name": "a", "inputSchema": {"$schema": "http://json-schema.org/draft-07/schema#", "properties": {}}}),
                json!({"name": "b", "inputSchema": null}),
            ],
        );
        assert!(entries[0].spec.parameters.get("$schema").is_none());
        assert_eq!(entries[0].spec.parameters["type"], "object");
        assert_eq!(entries[1].spec.parameters["type"], "object");
    }

    #[test]
    fn result_content_is_flattened() {
        let result = json!({"content": [
            {"type": "text", "text": "a"}, {"type": "image", "data": "AAAA", "mimeType": "image/png"},
            {"type": "resource_link", "uri": "file:///x"}, {"type": "resource", "resource": {"uri": "u", "text": "body"}}
        ]});
        let ToolOutcome::Ok(text) = outcome_from_result(&result) else {
            panic!("expected Ok")
        };
        assert_eq!(
            text,
            "a\n[image content not forwarded: image/png]\n[resource: file:///x]\nbody"
        );
        assert!(
            matches!(outcome_from_result(&json!({"content": [{"type": "text", "text": "x"}], "isError": true})), ToolOutcome::Error(t) if t == "x")
        );
        assert!(
            matches!(outcome_from_result(&json!({"content": [], "structuredContent": {"n": 1}})), ToolOutcome::Ok(t) if t == r#"{"n":1}"#)
        );
        assert!(matches!(
            outcome_from_result(&json!({"resultType": "input_required"})),
            ToolOutcome::Error(_)
        ));
        assert!(matches!(
            outcome_from_result(&json!({"resultType": "other"})),
            ToolOutcome::Error(_)
        ));
        let big = json!({"content": [{"type": "text", "text": "é".repeat(40_000)}]});
        let ToolOutcome::Ok(text) = outcome_from_result(&big) else {
            panic!("expected Ok")
        };
        assert!(
            text.len() < crate::agent::MAX_TOOL_RESULT_BYTES + 100 && text.contains("truncated")
        );
    }

    #[test]
    fn config_roundtrips_and_hides_env_values() {
        let config: ServerConfig = serde_json::from_str(
            r#"{"name":"fs","command":"mcp-fs","env":{"TOKEN":"sk-secret-value"}}"#,
        )
        .unwrap();
        assert!(config.enabled && config.args.is_empty() && config.allowed_tools.is_empty());
        assert!(!format!("{config:?}").contains("secret-value"));
        assert!(format!("{config:?}").contains("TOKEN"));
        let again: ServerConfig =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(again, config);
    }
}
