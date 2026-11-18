#[cfg(unix)]
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[cfg(unix)]
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(unix)]
const MAX_MESSAGE_BYTES: u64 = 4 * 1024 * 1024;
#[cfg(unix)]
const MAX_CONNECTIONS: usize = 8;

#[cfg(unix)]
fn current_uid() -> u32 {
    // SAFETY: `geteuid` takes no argument and cannot fail.
    unsafe { libc::geteuid() }
}

#[cfg(unix)]
fn peer_uid(stream: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `credentials` is a `ucred` of `length` bytes, which `SO_PEERCRED` fills for a connected Unix socket.
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    (result == 0).then_some(credentials.uid)
}

#[cfg(unix)]
fn private_to_user(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path)
        .is_ok_and(|m| m.uid() == current_uid() && m.permissions().mode() & 0o077 == 0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    #[default]
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case")]
pub enum Request {
    CurrentNote,
    OpenNote {
        path: String,
        line: Option<u32>,
        #[serde(default)]
        new_tab: bool,
    },
    ShowSearch {
        query: String,
    },
    ShowGraph {
        note: Option<String>,
    },
    InsertAtCursor {
        text: String,
    },
    Notify {
        text: String,
        #[serde(default)]
        level: Level,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "response", rename_all = "snake_case")]
pub enum Response {
    Done,
    CurrentNote(CurrentNote),
    Failed { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrentNote {
    pub path: Option<String>,
    #[serde(default)]
    pub selection: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug)]
pub enum IpcError {
    NotRunning,
    Failed(String),
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcError::NotRunning => f.write_str("the Focus application is not open on this vault"),
            IpcError::Failed(message) => f.write_str(message),
        }
    }
}

#[cfg(not(unix))]
pub fn socket_path(_vault_root: &Path) -> Option<PathBuf> {
    None
}

#[cfg(unix)]
pub fn socket_path(vault_root: &Path) -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty())?;
    let canonical = dunce::canonicalize(vault_root).unwrap_or_else(|_| vault_root.to_path_buf());
    let fingerprint = focus_core::settings::cache_dir_for(&canonical)
        .file_name()?
        .to_os_string();
    Some(
        PathBuf::from(runtime)
            .join("focus")
            .join(fingerprint)
            .with_extension("sock"),
    )
}

#[derive(Debug, Clone)]
pub struct Client {
    #[cfg_attr(
        not(unix),
        expect(dead_code, reason = "no application socket on Windows")
    )]
    socket: Option<PathBuf>,
}

impl Client {
    pub fn for_vault(vault_root: &Path) -> Client {
        Client {
            socket: socket_path(vault_root),
        }
    }

    pub fn at(socket: PathBuf) -> Client {
        Client {
            socket: Some(socket),
        }
    }

    #[cfg(not(unix))]
    pub fn request(&self, _request: &Request) -> Result<Response, IpcError> {
        Err(IpcError::NotRunning)
    }

    #[cfg(unix)]
    pub fn request(&self, request: &Request) -> Result<Response, IpcError> {
        let Some(path) = &self.socket else {
            return Err(IpcError::NotRunning);
        };
        if path.exists() && !(private_to_user(path) && path.parent().is_some_and(private_to_user)) {
            return Err(IpcError::Failed(
                "the application socket is not private to this user: refused".into(),
            ));
        }
        let stream = match UnixStream::connect(path) {
            Ok(stream) => stream,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                return Err(IpcError::NotRunning);
            }
            Err(e) => {
                return Err(IpcError::Failed(format!(
                    "cannot connect to the application: {e}"
                )));
            }
        };
        let io =
            |e: std::io::Error| IpcError::Failed(format!("exchange with the application: {e}"));
        stream.set_read_timeout(Some(REQUEST_TIMEOUT)).map_err(io)?;
        stream
            .set_write_timeout(Some(REQUEST_TIMEOUT))
            .map_err(io)?;
        let mut line =
            serde_json::to_string(request).map_err(|e| IpcError::Failed(e.to_string()))?;
        line.push('\n');
        (&stream).write_all(line.as_bytes()).map_err(io)?;
        let mut reply = String::new();
        BufReader::new(&stream)
            .take(MAX_MESSAGE_BYTES)
            .read_line(&mut reply)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                    IpcError::Failed("the application did not answer within 5 s".into())
                }
                _ => io(e),
            })?;
        if reply.trim().is_empty() {
            return Err(IpcError::Failed(
                "the application closed the connection without answering".into(),
            ));
        }
        serde_json::from_str(reply.trim())
            .map_err(|e| IpcError::Failed(format!("unreadable response from the application: {e}")))
    }
}

pub trait Handler: Send + Sync {
    fn handle(&self, request: Request) -> Response;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEvent {
    Opened,
    Closed,
}

pub type Observer = Arc<dyn Fn(ConnectionEvent) + Send + Sync>;

pub struct ReferenceServer {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ReferenceServer {
    pub fn start(path: &Path, handler: Arc<dyn Handler>) -> std::io::Result<ReferenceServer> {
        ReferenceServer::start_observed(path, handler, None)
    }

    #[cfg(not(unix))]
    pub fn start_observed(
        _path: &Path,
        _handler: Arc<dyn Handler>,
        _observer: Option<Observer>,
    ) -> std::io::Result<ReferenceServer> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "no application socket on this system",
        ))
    }

    #[cfg(unix)]
    pub fn start_observed(
        path: &Path,
        handler: Arc<dyn Handler>,
        observer: Option<Observer>,
    ) -> std::io::Result<ReferenceServer> {
        if let Some(dir) = path.parent() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
            if !private_to_user(dir) {
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
                if !private_to_user(dir) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("{} is not a private folder of this user", dir.display()),
                    ));
                }
            }
        }
        if let Ok(found) = std::fs::symlink_metadata(path) {
            use std::os::unix::fs::FileTypeExt;
            if !found.file_type().is_socket() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("{} exists and is not a socket", path.display()),
                ));
            }
            if UnixStream::connect(path).is_ok() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    format!("an application is already listening on {}", path.display()),
                ));
            }
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let open = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let thread = std::thread::Builder::new()
            .name("focus-ipc".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if peer_uid(&stream) != Some(current_uid())
                                || open.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS
                            {
                                if peer_uid(&stream) == Some(current_uid()) {
                                    open.fetch_sub(1, Ordering::AcqRel);
                                }
                                continue;
                            }
                            let handler = handler.clone();
                            let counter = open.clone();
                            let watcher = observer.clone();
                            if let Some(watch) = &watcher {
                                watch(ConnectionEvent::Opened);
                            }
                            let spawned = std::thread::Builder::new()
                                .name("focus-ipc-conn".into())
                                .spawn(move || {
                                    serve_connection(stream, handler.as_ref());
                                    counter.fetch_sub(1, Ordering::AcqRel);
                                    if let Some(watch) = &watcher {
                                        watch(ConnectionEvent::Closed);
                                    }
                                });
                            if spawned.is_err() {
                                open.fetch_sub(1, Ordering::AcqRel);
                                if let Some(watch) = &observer {
                                    watch(ConnectionEvent::Closed);
                                }
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => return,
                    }
                }
            })?;
        Ok(ReferenceServer {
            path: path.to_path_buf(),
            stop,
            thread: Some(thread),
        })
    }

    pub fn socket(&self) -> &Path {
        &self.path
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Drop for ReferenceServer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(unix)]
fn serve_connection(stream: UnixStream, handler: &dyn Handler) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(REQUEST_TIMEOUT));
    let mut line = String::new();
    let reply = match BufReader::new(&stream)
        .take(MAX_MESSAGE_BYTES)
        .read_line(&mut line)
    {
        Ok(0) | Err(_) => return,
        Ok(_) => match serde_json::from_str::<Request>(line.trim()) {
            Ok(request) => handler.handle(request),
            Err(e) => Response::Failed {
                message: format!("unreadable request: {e}"),
            },
        },
    };
    if let Ok(mut text) = serde_json::to_string(&reply) {
        text.push('\n');
        let _ = (&stream).write_all(text.as_bytes());
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Recorder {
        seen: Mutex<Vec<Request>>,
    }

    impl Handler for Recorder {
        fn handle(&self, request: Request) -> Response {
            self.seen.lock().unwrap().push(request.clone());
            match request {
                Request::CurrentNote => Response::CurrentNote(CurrentNote {
                    path: Some("Red Team/Note.md".into()),
                    selection: "texte choisi".into(),
                    line: 12,
                    column: 4,
                }),
                Request::OpenNote { path, .. } if path == "absente.md" => Response::Failed {
                    message: "note not found".into(),
                },
                _ => Response::Done,
            }
        }
    }

    fn socket(name: &str) -> PathBuf {
        crate::testvault::scratch(name).join("n/c.sock")
    }

    #[test]
    fn requests_roundtrip_through_the_socket() {
        let path = socket("ipc-rt");
        let recorder = Arc::new(Recorder {
            seen: Mutex::new(Vec::new()),
        });
        let server = ReferenceServer::start(&path, recorder.clone()).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let client = Client::at(path.clone());
        let Response::CurrentNote(note) = client.request(&Request::CurrentNote).unwrap() else {
            panic!("unexpected response")
        };
        assert_eq!(
            (
                note.path.as_deref(),
                note.selection.as_str(),
                note.line,
                note.column
            ),
            (Some("Red Team/Note.md"), "texte choisi", 12, 4)
        );
        assert_eq!(
            client
                .request(&Request::OpenNote {
                    path: "a.md".into(),
                    line: Some(3),
                    new_tab: true
                })
                .unwrap(),
            Response::Done
        );
        assert_eq!(
            client
                .request(&Request::OpenNote {
                    path: "absente.md".into(),
                    line: None,
                    new_tab: false
                })
                .unwrap(),
            Response::Failed {
                message: "note not found".into()
            }
        );
        for request in [
            Request::ShowSearch {
                query: "tag:#x".into(),
            },
            Request::ShowGraph { note: None },
            Request::InsertAtCursor {
                text: "é\nligne".into(),
            },
            Request::Notify {
                text: "fini".into(),
                level: Level::Warning,
            },
        ] {
            assert_eq!(client.request(&request).unwrap(), Response::Done);
        }
        assert_eq!(recorder.seen.lock().unwrap().len(), 7);
        assert!(matches!(
            recorder.seen.lock().unwrap()[4],
            Request::ShowGraph { note: None }
        ));
        drop(server);
        assert!(!path.exists(), "the socket is removed on stop");
        assert!(matches!(
            client.request(&Request::CurrentNote),
            Err(IpcError::NotRunning)
        ));
    }

    #[test]
    fn a_loose_folder_is_tightened_and_a_file_in_the_way_is_not_removed() {
        let path = socket("ipc-private");
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir).unwrap();
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        let recorder = Arc::new(Recorder {
            seen: Mutex::new(Vec::new()),
        });
        let server = ReferenceServer::start(&path, recorder.clone()).unwrap();
        assert_eq!(
            std::fs::metadata(dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        drop(server);
        std::fs::write(&path, "precious").unwrap();
        assert!(ReferenceServer::start(&path, recorder).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "precious");
    }

    #[test]
    fn the_client_refuses_a_socket_that_others_can_reach() {
        let path = socket("ipc-client-trust");
        let recorder = Arc::new(Recorder {
            seen: Mutex::new(Vec::new()),
        });
        let server = ReferenceServer::start(&path, recorder).unwrap();
        let client = Client::at(path.clone());
        assert!(client.request(&Request::CurrentNote).is_ok());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        let refused = client.request(&Request::CurrentNote);
        assert!(
            matches!(&refused, Err(IpcError::Failed(m)) if m.contains("not private")),
            "{refused:?}"
        );
        drop(server);
    }

    #[test]
    fn only_a_bounded_number_of_connections_are_served_at_once() {
        struct Slow;
        impl Handler for Slow {
            fn handle(&self, _: Request) -> Response {
                std::thread::sleep(Duration::from_millis(600));
                Response::Done
            }
        }
        let path = socket("ipc-flood");
        let server = ReferenceServer::start(&path, Arc::new(Slow)).unwrap();
        let line = b"{\"request\":\"current_note\"}\n";
        let mut streams: Vec<UnixStream> = (0..MAX_CONNECTIONS + 4)
            .map(|_| UnixStream::connect(&path).unwrap())
            .collect();
        for stream in &mut streams {
            let _ = stream.write_all(line);
        }
        let answered = streams
            .iter_mut()
            .filter(|s| {
                s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut reply = String::new();
                BufReader::new(&**s)
                    .read_line(&mut reply)
                    .map(|n| n > 0)
                    .unwrap_or(false)
            })
            .count();
        assert!(
            (1..=MAX_CONNECTIONS).contains(&answered),
            "{answered} answered"
        );
        drop(server);
    }

    #[test]
    fn the_observer_sees_each_served_connection_open_and_close() {
        let path = socket("ipc-observer");
        let events = Arc::new(Mutex::new(Vec::new()));
        let log = events.clone();
        let observer: Observer = Arc::new(move |event| log.lock().unwrap().push(event));
        let server = ReferenceServer::start_observed(
            &path,
            Arc::new(Recorder {
                seen: Mutex::new(Vec::new()),
            }),
            Some(observer),
        )
        .unwrap();
        let client = Client::at(path.clone());
        assert!(client.request(&Request::CurrentNote).is_ok());
        assert!(client.request(&Request::CurrentNote).is_ok());
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while events.lock().unwrap().len() < 4 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let seen = events.lock().unwrap().clone();
        assert_eq!(
            seen.iter()
                .filter(|e| **e == ConnectionEvent::Opened)
                .count(),
            2,
            "{seen:?}"
        );
        assert_eq!(
            seen.iter()
                .filter(|e| **e == ConnectionEvent::Closed)
                .count(),
            2,
            "{seen:?}"
        );
        assert_eq!(seen[0], ConnectionEvent::Opened);
        drop(server);
    }

    #[test]
    fn wire_format_is_one_line_of_tagged_json() {
        assert_eq!(
            serde_json::to_string(&Request::CurrentNote).unwrap(),
            r#"{"request":"current_note"}"#
        );
        assert_eq!(
            serde_json::to_string(&Request::OpenNote {
                path: "a.md".into(),
                line: None,
                new_tab: false
            })
            .unwrap(),
            r#"{"request":"open_note","path":"a.md","line":null,"new_tab":false}"#
        );
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"request":"notify","text":"x"}"#).unwrap(),
            Request::Notify {
                text: "x".into(),
                level: Level::Info
            }
        );
        assert_eq!(
            serde_json::to_string(&Response::Done).unwrap(),
            r#"{"response":"done"}"#
        );
    }

    #[test]
    fn missing_socket_stale_socket_and_garbage() {
        let path = socket("ipc-st");
        let client = Client::at(path.clone());
        assert!(matches!(
            client.request(&Request::CurrentNote),
            Err(IpcError::NotRunning)
        ));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::set_permissions(
            path.parent().unwrap(),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        drop(UnixListener::bind(&path).unwrap());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(path.exists());
        assert!(matches!(
            client.request(&Request::CurrentNote),
            Err(IpcError::NotRunning)
        ));
        let recorder = Arc::new(Recorder {
            seen: Mutex::new(Vec::new()),
        });
        let first = ReferenceServer::start(&path, recorder.clone()).unwrap();
        assert!(ReferenceServer::start(&path, recorder).is_err());
        let mut raw = UnixStream::connect(&path).unwrap();
        raw.write_all(b"not json\n").unwrap();
        let mut reply = String::new();
        BufReader::new(&raw).read_line(&mut reply).unwrap();
        assert!(
            reply.contains("failed") && reply.contains("unreadable"),
            "{reply}"
        );
        drop(first);
    }

    #[test]
    fn socket_path_follows_runtime_dir_and_vault_fingerprint() {
        let a = crate::testvault::scratch("ipc-path-a");
        let b = crate::testvault::scratch("ipc-path-b");
        if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
            assert!(socket_path(&a).is_none());
            return;
        }
        let (pa, pb) = (socket_path(&a).unwrap(), socket_path(&b).unwrap());
        assert_eq!(pa, socket_path(&a).unwrap());
        assert_ne!(pa, pb);
        assert!(pa.to_string_lossy().ends_with(".sock") && pa.parent().unwrap().ends_with("focus"));
    }
}
