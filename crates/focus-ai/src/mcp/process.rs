use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{McpError, ServerConfig};
use crate::Cancel;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const POLL: Duration = Duration::from_millis(20);
const MAX_LINE_BYTES: u64 = 16 * 1024 * 1024;
const STDERR_TAIL_BYTES: usize = 2048;

const INHERITED: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "LANGUAGE",
    "TZ",
    "TMPDIR",
    "XDG_RUNTIME_DIR",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

#[cfg(windows)]
const INHERITED_WINDOWS: &[&str] = &[
    "SYSTEMROOT",
    "WINDIR",
    "SYSTEMDRIVE",
    "COMSPEC",
    "PATHEXT",
    "USERPROFILE",
    "USERNAME",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMW6432",
    "COMMONPROGRAMFILES",
    "TEMP",
    "TMP",
    "PROCESSOR_ARCHITECTURE",
    "NUMBER_OF_PROCESSORS",
];

fn inherited(name: &str) -> bool {
    #[cfg(windows)]
    {
        INHERITED
            .iter()
            .chain(INHERITED_WINDOWS)
            .any(|known| known.eq_ignore_ascii_case(name))
    }
    #[cfg(not(windows))]
    {
        INHERITED.contains(&name)
    }
}

fn inherited_environment() -> Vec<(String, String)> {
    std::env::vars()
        .filter(|(name, _)| inherited(name) || name.starts_with("LC_"))
        .collect()
}

#[cfg(windows)]
fn program(command: &str) -> std::path::PathBuf {
    let bare = std::path::Path::new(command);
    if bare.extension().is_some() || bare.components().count() > 1 {
        return bare.to_path_buf();
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .flat_map(|dir| {
            ["exe", "cmd", "bat", "com"].map(|extension| dir.join(bare).with_extension(extension))
        })
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| bare.to_path_buf())
}

#[cfg(not(windows))]
fn program(command: &str) -> &str {
    command
}

#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

pub type Reply = Result<Value, RpcError>;
type Pending = Arc<Mutex<HashMap<u64, Sender<Reply>>>>;

pub struct Connection {
    server: String,
    pub pid: u32,
    next_id: AtomicU64,
    pending: Pending,
    writer: Mutex<Option<Sender<String>>>,
    alive: Arc<AtomicBool>,
    stderr_tail: Arc<Mutex<VecDeque<u8>>>,
    stderr_done: Arc<AtomicBool>,
    child: Mutex<Option<Child>>,
}

impl Connection {
    pub fn spawn(config: &ServerConfig) -> Result<Connection, McpError> {
        let mut command = Command::new(program(&config.command));
        command
            .env_clear()
            .envs(inherited_environment())
            .args(&config.args)
            .envs(&config.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut command, CREATE_NO_WINDOW);
        let spawn_error = |e: std::io::Error| McpError::Spawn {
            server: config.name.clone(),
            command: config.command.clone(),
            reason: e.to_string(),
        };
        let mut child = command.spawn().map_err(spawn_error)?;
        let (stdin, stdout, stderr) =
            match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
                (Some(i), Some(o), Some(e)) => (i, o, e),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(McpError::Protocol {
                        server: config.name.clone(),
                        reason: "process pipes unavailable".into(),
                    });
                }
            };

        let pid = child.id();
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
        let stderr_done = Arc::new(AtomicBool::new(false));
        let (writer, lines) = mpsc::channel::<String>();

        std::thread::spawn(move || write_lines(stdin, lines));
        {
            let (tail, done) = (stderr_tail.clone(), stderr_done.clone());
            std::thread::spawn(move || drain_stderr(stderr, &tail, &done));
        }
        {
            let (pending, alive, answer, tail) = (
                pending.clone(),
                alive.clone(),
                writer.clone(),
                stderr_tail.clone(),
            );
            std::thread::spawn(move || read_messages(stdout, &pending, &alive, &answer, &tail));
        }
        Ok(Connection {
            server: config.name.clone(),
            pid,
            next_id: AtomicU64::new(1),
            pending,
            writer: Mutex::new(Some(writer)),
            alive,
            stderr_tail,
            stderr_done,
            child: Mutex::new(Some(child)),
        })
    }

    pub fn protocol_error(&self, reason: String) -> McpError {
        McpError::Protocol {
            server: self.server.clone(),
            reason,
        }
    }

    fn stderr_suffix(&self) -> String {
        let tail = self
            .stderr_tail
            .lock()
            .map(|t| {
                String::from_utf8_lossy(&t.iter().copied().collect::<Vec<u8>>())
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        if tail.is_empty() {
            String::new()
        } else {
            format!(". Last stderr lines: {tail}")
        }
    }

    fn exited(&self) -> McpError {
        let deadline = Instant::now() + Duration::from_millis(200);
        let mut status = None;
        while status.is_none() && Instant::now() < deadline {
            status = self
                .child
                .lock()
                .ok()
                .and_then(|mut c| c.as_mut().and_then(|c| c.try_wait().ok().flatten()));
            if status.is_none() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let stderr_deadline = Instant::now() + Duration::from_millis(300);
        while !self.stderr_done.load(Ordering::Acquire) && Instant::now() < stderr_deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let code = status.map(|s| format!(" ({s})")).unwrap_or_default();
        McpError::Exited {
            server: self.server.clone(),
            detail: format!("{code}{}", self.stderr_suffix()),
        }
    }

    fn send(&self, message: &Value) -> bool {
        let line = message.to_string();
        match self.writer.lock() {
            Ok(guard) => guard.as_ref().is_some_and(|w| w.send(line).is_ok()),
            Err(_) => false,
        }
    }

    pub fn notify(&self, method: &str, params: Value) {
        let _ = self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    pub fn request(
        &self,
        method: &str,
        params: Value,
        cancel: &Cancel,
        timeout: Duration,
    ) -> Result<Reply, McpError> {
        if !self.alive.load(Ordering::Acquire) {
            return Err(self.exited());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| self.protocol_error("corrupted internal state".into()))?
            .insert(id, sender);
        let forget = || {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&id);
            }
        };
        if !self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })) {
            forget();
            return Err(self.exited());
        }
        let deadline = Instant::now() + timeout;
        loop {
            if cancel.is_cancelled() {
                forget();
                self.notify(
                    "notifications/cancelled",
                    json!({ "requestId": id, "reason": "cancelled by the user" }),
                );
                return Err(McpError::Cancelled);
            }
            match receiver.recv_timeout(
                POLL.min(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .max(Duration::from_millis(1)),
                ),
            ) {
                Ok(reply) => return Ok(reply),
                Err(RecvTimeoutError::Disconnected) => return Err(self.exited()),
                Err(RecvTimeoutError::Timeout) => {
                    if Instant::now() >= deadline {
                        forget();
                        self.notify(
                            "notifications/cancelled",
                            json!({ "requestId": id, "reason": "timed out" }),
                        );
                        return Err(McpError::Timeout {
                            server: self.server.clone(),
                            method: method.to_string(),
                            secs: timeout.as_secs().max(1),
                            stderr: self.stderr_suffix(),
                        });
                    }
                }
            }
        }
    }

    pub fn stop(&self) {
        if let Ok(mut writer) = self.writer.lock() {
            *writer = None;
        }
        self.alive.store(false, Ordering::Release);
        let Some(mut child) = self.child.lock().ok().and_then(|mut c| c.take()) else {
            return;
        };
        if wait_exit(&mut child, Duration::from_secs(2)) {
            return;
        }
        #[cfg(unix)]
        {
            let group = -(child.id() as i32);
            // SAFETY: `group` is the child's process group id, created by `process_group(0)`;
            // a group that is already gone just gives ESRCH.
            unsafe { libc::kill(group, libc::SIGTERM) };
            if wait_exit(&mut child, Duration::from_secs(1)) {
                return;
            }
            unsafe { libc::kill(group, libc::SIGKILL) };
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            let _ = Command::new("taskkill")
                .args(["/T", "/F", "/PID", &child.id().to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.stop();
    }
}

fn wait_exit(child: &mut Child, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => return false,
        }
    }
}

fn write_lines(mut stdin: std::process::ChildStdin, lines: Receiver<String>) {
    for line in lines {
        if stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .and_then(|()| stdin.flush())
            .is_err()
        {
            return;
        }
    }
}

fn drain_stderr(mut stderr: impl Read, tail: &Mutex<VecDeque<u8>>, done: &AtomicBool) {
    let mut buffer = [0u8; 1024];
    while let Ok(n) = stderr.read(&mut buffer) {
        if n == 0 {
            break;
        }
        if let Ok(mut tail) = tail.lock() {
            tail.extend(&buffer[..n]);
            while tail.len() > STDERR_TAIL_BYTES {
                tail.pop_front();
            }
        }
    }
    done.store(true, Ordering::Release);
}

fn read_messages(
    stdout: impl Read,
    pending: &Pending,
    alive: &AtomicBool,
    answer: &Sender<String>,
    tail: &Mutex<VecDeque<u8>>,
) {
    let mut reader = BufReader::new(stdout);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader
            .by_ref()
            .take(MAX_LINE_BYTES)
            .read_until(b'\n', &mut line)
        {
            Ok(0) | Err(_) => break,
            Ok(_) if !line.ends_with(b"\n") && line.len() as u64 >= MAX_LINE_BYTES => break,
            Ok(_) => {}
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(text) else {
            if let Ok(mut tail) = tail.lock() {
                tail.extend(text.bytes().take(256));
                tail.push_back(b'\n');
                while tail.len() > STDERR_TAIL_BYTES {
                    tail.pop_front();
                }
            }
            continue;
        };
        dispatch(&message, pending, answer);
    }
    alive.store(false, Ordering::Release);
    if let Ok(mut pending) = pending.lock() {
        pending.clear();
    }
}

fn dispatch(message: &Value, pending: &Pending, answer: &Sender<String>) {
    let id = message.get("id");
    if let Some(method) = message.get("method").and_then(Value::as_str) {
        if let Some(id) = id {
            let reply = if method == "ping" {
                json!({ "jsonrpc": "2.0", "id": id, "result": {} })
            } else {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "Method not found" } })
            };
            let _ = answer.send(reply.to_string());
        }
        return;
    }
    let Some(id) = id.and_then(Value::as_u64) else {
        return;
    };
    let reply: Reply = match message.get("error") {
        Some(error) => Err(RpcError {
            code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("error without a message")
                .to_string(),
            data: error.get("data").cloned(),
        }),
        None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
    };
    let sender = pending.lock().ok().and_then(|mut p| p.remove(&id));
    if let Some(sender) = sender {
        let _ = sender.send(reply);
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    fn config(script: &str, env: &[(&str, &str)]) -> ServerConfig {
        ServerConfig {
            name: "probe".into(),
            command: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            allowed_tools: vec![],
            enabled: true,
        }
    }

    fn stderr_of(connection: &Connection) -> String {
        for _ in 0..100 {
            if connection.stderr_done.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        connection.stderr_suffix()
    }

    #[test]
    fn a_server_does_not_inherit_the_environment_of_focus() {
        // SAFETY: the variable is read only by this test's child process; no other test uses the name.
        unsafe { std::env::set_var("FOCUS_TEST_INHERITED_SECRET", "leaked") };
        let connection = Connection::spawn(&config("echo \"leak=${FOCUS_TEST_INHERITED_SECRET:-none} key=${DEEPSEEK_API_KEY:-none} declared=${DECLARED:-none} path=${PATH:+set}\" >&2", &[("DECLARED", "yes")])).expect("spawned");
        let seen = stderr_of(&connection);
        assert!(
            seen.contains("leak=none")
                && seen.contains("key=none")
                && seen.contains("declared=yes")
                && seen.contains("path=set"),
            "{seen}"
        );
    }

    #[test]
    fn the_command_is_never_read_by_a_shell() {
        let marker = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp/mcp-no-shell-marker");
        let _ = std::fs::remove_file(&marker);
        let hostile = ServerConfig {
            name: "probe".into(),
            command: format!("true; touch {}", marker.display()),
            args: vec![format!("$(touch {})", marker.display())],
            env: Default::default(),
            allowed_tools: vec![],
            enabled: true,
        };
        assert!(matches!(
            Connection::spawn(&hostile),
            Err(McpError::Spawn { .. })
        ));
        let argument_only = ServerConfig {
            command: "/bin/echo".into(),
            ..hostile
        };
        if let Ok(connection) = Connection::spawn(&argument_only) {
            connection.stop();
        }
        assert!(
            !marker.exists(),
            "a shell interpreted the command or an argument"
        );
    }
}
