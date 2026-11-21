#[cfg(unix)]
use std::io::{Read as _, Write as _};
#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::time::Duration;

use focus_kit::Flow as _;
use focus_kit::{ActiveTokens, Button, ButtonKind, Type, size};
use gpui::{
    AnyElement, Context, InteractiveElement, MouseButton, ParentElement, Styled, Window, div, px,
};
#[cfg(unix)]
use serde_json::{Value, json};

use crate::shell::{Overlay, Shell};

#[cfg(unix)]
const MAX_REQUEST_BYTES: u64 = 256 * 1024;
#[cfg(unix)]
const MAX_PATHS: usize = 256;
#[cfg(unix)]
const MAX_TOKEN_CHARS: usize = 512;
#[cfg(unix)]
const MAX_ADDRESSES: usize = 16;
#[cfg(unix)]
const MAX_CAPTURE_BYTES: usize = crate::capture::MAX_BYTES;
#[cfg(unix)]
const CAPTURE_WITHIN: Duration = Duration::from_secs(10);
#[cfg(unix)]
const READ_WITHIN: Duration = Duration::from_secs(2);
#[cfg(unix)]
const ANSWER_WITHIN: Duration = Duration::from_secs(4);

#[cfg(unix)]
pub type Listener = UnixListener;

#[cfg(not(unix))]
pub enum Listener {}

#[cfg(not(unix))]
pub fn socket_path() -> Option<PathBuf> {
    None
}

#[cfg(not(unix))]
pub fn hand_over(_socket: &Path, _paths: &[PathBuf], _addresses: &[String]) -> bool {
    false
}

#[cfg(not(unix))]
pub fn capture_over(_socket: &Path, _text: &str) -> Option<String> {
    None
}

#[cfg(not(unix))]
pub fn bind(_socket: &Path) -> std::io::Result<Listener> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "no socket for a second launch on this system",
    ))
}

#[cfg(not(unix))]
pub fn serve(
    _shell: &mut Shell,
    listener: Listener,
    _socket: PathBuf,
    _window: &mut Window,
    _cx: &mut Context<Shell>,
) {
    match listener {}
}

#[cfg(unix)]
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(unix)]
pub fn socket_path() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty())?;
    let config = focus_core::settings::config_dir();
    let config = std::fs::canonicalize(&config).unwrap_or(config);
    Some(Path::new(&runtime).join(format!(
        "focus-{:016x}.sock",
        fnv1a(config.as_os_str().as_encoded_bytes())
    )))
}

#[cfg(unix)]
fn launch_token() -> Option<String> {
    ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()))
}

#[cfg(unix)]
fn path_json(path: &Path) -> Value {
    match path.to_str() {
        Some(text) => Value::String(text.to_owned()),
        None => Value::Array(
            path.as_os_str()
                .as_encoded_bytes()
                .iter()
                .map(|b| Value::from(*b))
                .collect(),
        ),
    }
}

#[cfg(unix)]
fn json_path(value: &Value) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt as _;
    match value {
        Value::String(text) => Some(PathBuf::from(text)),
        Value::Array(bytes) => bytes
            .iter()
            .map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok()))
            .collect::<Option<Vec<u8>>>()
            .map(|bytes| PathBuf::from(std::ffi::OsString::from_vec(bytes))),
        _ => None,
    }
}

#[cfg(unix)]
fn ask(socket: &Path, body: &Value, within: Duration) -> Option<String> {
    let mut stream = UnixStream::connect(socket).ok()?;
    let sent = stream.set_read_timeout(Some(within)).is_ok()
        && stream.write_all(body.to_string().as_bytes()).is_ok()
        && stream.shutdown(std::net::Shutdown::Write).is_ok();
    let mut answer = String::new();
    (sent && stream.take(8192).read_to_string(&mut answer).is_ok()).then_some(answer)
}

#[cfg(unix)]
pub fn hand_over(socket: &Path, paths: &[PathBuf], addresses: &[String]) -> bool {
    let paths: Vec<Value> = paths.iter().map(|p| path_json(p)).collect();
    let body = json!({ "paths": paths, "addresses": addresses, "token": launch_token() });
    ask(socket, &body, ANSWER_WITHIN).as_deref() == Some("ok\n")
}

#[cfg(unix)]
pub fn capture_over(socket: &Path, text: &str) -> Option<String> {
    ask(
        socket,
        &json!({ "paths": [], "capture": text }),
        CAPTURE_WITHIN,
    )
}

#[derive(Debug, Default, PartialEq)]
pub struct Request {
    pub paths: Vec<PathBuf>,
    pub token: Option<String>,
    pub addresses: Vec<String>,
    pub capture: Option<String>,
}

#[cfg(unix)]
pub fn bind(socket: &Path) -> std::io::Result<Listener> {
    let listener = match UnixListener::bind(socket) {
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            let stale = std::fs::symlink_metadata(socket)?.file_type().is_socket()
                && UnixStream::connect(socket).is_err();
            if !stale {
                return Err(error);
            }
            std::fs::remove_file(socket)?;
            UnixListener::bind(socket)?
        }
        other => other?,
    };
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

#[cfg(unix)]
fn read_request(stream: &mut UnixStream) -> Result<Request, String> {
    stream
        .set_read_timeout(Some(READ_WITHIN))
        .map_err(|e| e.to_string())?;
    let mut body = Vec::new();
    stream
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() as u64 > MAX_REQUEST_BYTES {
        return Err(format!(
            "the request is longer than {MAX_REQUEST_BYTES} bytes"
        ));
    }
    let request: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let paths = request
        .get("paths")
        .and_then(Value::as_array)
        .ok_or("no list of paths")?;
    if paths.len() > MAX_PATHS {
        return Err(format!("more than {MAX_PATHS} paths"));
    }
    let paths = paths
        .iter()
        .map(json_path)
        .collect::<Option<Vec<PathBuf>>>()
        .ok_or("a path is neither a string nor a list of bytes")?;
    let token = match request.get("token") {
        None | Some(Value::Null) => None,
        Some(Value::String(token))
            if token.len() <= MAX_TOKEN_CHARS && token.bytes().all(|b| b.is_ascii_graphic()) =>
        {
            Some(token.clone())
        }
        Some(_) => {
            return Err(
                "the activation token is not printable ASCII of at most 512 characters".into(),
            );
        }
    };
    let addresses = match request.get("addresses") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) if items.len() <= MAX_ADDRESSES => items
            .iter()
            .map(|item| {
                item.as_str()
                    .filter(|a| {
                        crate::obsidian::is_obsidian(a)
                            && a.len() <= crate::obsidian::MAX_ADDRESS_BYTES
                    })
                    .map(str::to_string)
            })
            .collect::<Option<Vec<String>>>()
            .ok_or("an address is not an obsidian: address of at most 64 KiB")?,
        Some(_) => return Err(format!("more than {MAX_ADDRESSES} addresses")),
    };
    let capture = match request.get("capture") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) if text.len() <= MAX_CAPTURE_BYTES => Some(text.clone()),
        Some(_) => return Err("the capture is not a text of at most 64 KiB".into()),
    };
    Ok(Request {
        paths: paths
            .into_iter()
            .filter(|p| p.is_absolute() && p.exists())
            .collect(),
        token,
        addresses,
        capture,
    })
}

#[cfg(unix)]
pub fn serve(
    shell: &mut Shell,
    listener: Listener,
    socket: PathBuf,
    window: &mut Window,
    cx: &mut Context<Shell>,
) {
    type Reply = std::sync::mpsc::Sender<String>;
    let (sender, requests) = smol::channel::unbounded::<(Request, Option<Reply>)>();
    let spawned = std::thread::Builder::new()
        .name("focus-instance".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let answer = match read_request(&mut stream) {
                    Ok(request) if request.capture.is_some() => {
                        let (reply, answer) = std::sync::mpsc::channel();
                        if sender.send_blocking((request, Some(reply))).is_err() {
                            return;
                        }
                        answer
                            .recv_timeout(CAPTURE_WITHIN)
                            .unwrap_or_else(|_| "failed the window did not answer\n".into())
                    }
                    Ok(request) => match sender.send_blocking((request, None)) {
                        Ok(()) => "ok\n".into(),
                        Err(_) => return,
                    },
                    Err(error) => {
                        eprintln!("focus: second launch refused: {error}");
                        "refused\n".into()
                    }
                };
                let _ = stream.write_all(answer.as_bytes());
            }
        });
    if let Err(error) = spawned {
        eprintln!("focus: instance socket thread: {error}");
        return;
    }
    let quit = cx.on_app_quit(move |_, _| {
        if let Err(error) = std::fs::remove_file(&socket)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!(
                "focus: removing the instance socket {}: {error}",
                socket.display()
            );
        }
        async {}
    });
    shell._subscriptions.push(quit);
    cx.spawn_in(window, async move |shell, cx| {
        while let Ok((request, reply)) = requests.recv().await {
            let _ = shell.update_in(cx, |shell, window, cx| match request.capture {
                Some(text) => shell.capture_text(text, reply, window, cx),
                None => shell.receive_paths(request, window, cx),
            });
        }
    })
    .detach();
}

#[derive(Clone, Debug, PartialEq)]
pub struct VaultSwitch {
    pub to: PathBuf,
    pub files: Vec<PathBuf>,
}

fn vault_name(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
}

impl Shell {
    #[cfg_attr(
        not(unix),
        expect(dead_code, reason = "no second launch reaches a window")
    )]
    fn receive_paths(&mut self, request: Request, window: &mut Window, cx: &mut Context<Self>) {
        let Request {
            paths,
            token,
            addresses,
            ..
        } = request;
        let known = self.store.settings.recent_vaults.clone();
        let (vault, files) = crate::loose::route(None, paths, &known);
        match vault {
            Some(vault) if self.vault_root.as_deref() != Some(vault.as_path()) => {
                if self.vault_root.is_some() {
                    self.overlay = Overlay::SwitchVault(VaultSwitch { to: vault, files });
                    window.focus(&self.focus);
                    cx.notify();
                } else {
                    self.open_paths.extend(files);
                    self.switch_vault(vault, window, cx);
                }
            }
            _ => {
                self.open_paths.extend(files);
                self.open_given_paths(window, cx);
            }
        }
        for address in addresses {
            self.follow_obsidian(&address, window, cx);
        }
        if let Some(token) = token {
            gpui::set_activation_token(token);
        }
        window.activate_window();
    }

    pub fn confirm_vault_switch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Overlay::SwitchVault(switch) = std::mem::replace(&mut self.overlay, Overlay::None)
        else {
            return;
        };
        self.open_paths.extend(switch.files);
        self.switch_vault(switch.to, window, cx);
    }

    pub fn render_vault_switch(&self, switch: &VaultSwitch, cx: &mut Context<Self>) -> AnyElement {
        let t = *cx.tokens();
        let from = self
            .library
            .as_ref()
            .map(|library| library.name().to_string())
            .or_else(|| self.vault_root.as_deref().map(vault_name))
            .unwrap_or_default();
        let to = vault_name(&switch.to);
        focus_kit::fade_in(
            "fade-switch",
            div()
                .id("switch-veil")
                .absolute()
                .inset_0()
                .flow_row()
                .items_center()
                .justify_center()
                .bg(t.veil)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_overlay(window, cx)),
                )
                .child(
                    focus_kit::floating(&t)
                        .id("switch-dialog")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .flex()
                        .flex_col()
                        .gap(size::S3)
                        .p(size::S4)
                        .w(px(480.))
                        .child(div().ui().text_color(t.ink).child(focus_i18n::t_args(
                            "ui-switch-title",
                            &[("to", to.as_str().into())],
                        )))
                        .child(div().ui_sm().text_color(t.ink_3).child(focus_i18n::t_args(
                            "ui-switch-why",
                            &[("from", from.as_str().into()), ("to", to.as_str().into())],
                        )))
                        .child(
                            div()
                                .mono_sm()
                                .text_color(t.ink)
                                .bg(t.card_2)
                                .p(size::S2)
                                .child(switch.to.display().to_string()),
                        )
                        .child(
                            div()
                                .flow_row()
                                .justify_end()
                                .gap(size::S2)
                                .child(
                                    Button::new("switch-cancel", focus_i18n::t("ui-action-cancel"))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.close_overlay(window, cx)
                                        })),
                                )
                                .child(
                                    Button::new("switch-open", focus_i18n::t("ui-switch-open"))
                                        .kind(ButtonKind::Primary)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.confirm_vault_switch(window, cx)
                                        })),
                                ),
                        ),
                ),
            cx,
        )
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    #[test]
    fn a_second_launch_hands_over_checked_paths() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-tmp/instance");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("Note.md");
        std::fs::write(&note, "# Note\n").unwrap();
        let latin1 = {
            use std::os::unix::ffi::OsStrExt as _;
            dir.join(std::ffi::OsStr::from_bytes(b"caf\xe9.md"))
        };
        std::fs::write(&latin1, "# Latin-1 name\n").unwrap();
        let socket = dir.join("focus.sock");
        let listener = bind(&socket).unwrap();
        assert_eq!(
            std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let server = std::thread::spawn(move || {
            let mut stream = listener.incoming().next().unwrap().unwrap();
            let request = read_request(&mut stream);
            stream.write_all(b"ok\n").unwrap();
            request
        });
        assert!(hand_over(
            &socket,
            &[
                note.clone(),
                dir.join("missing.md"),
                PathBuf::from("relative.md"),
                latin1.clone(),
            ],
            &["obsidian://open?vault=v&file=n".to_string()]
        ));
        let request = server.join().unwrap().unwrap();
        assert_eq!(request.addresses, ["obsidian://open?vault=v&file=n"]);
        assert_eq!(
            request.paths,
            vec![note, latin1],
            "missing and relative paths are dropped, a name that is not UTF-8 is kept"
        );
    }

    #[test]
    fn a_socket_left_by_an_ended_process_is_replaced() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-tmp/instance-stale");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("focus.sock");
        drop(UnixListener::bind(&socket).unwrap());
        assert!(!hand_over(&socket, &[], &[]), "nobody listens on it");
        let _listener = bind(&socket).unwrap();
        assert!(bind(&socket).is_err(), "a live socket is not taken over");
    }

    #[test]
    fn an_oversized_or_malformed_request_is_refused() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let token = "x".repeat(MAX_TOKEN_CHARS + 1);
        client
            .write_all(
                json!({ "paths": [], "token": token })
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        assert!(read_request(&mut server).is_err());
        for body in [
            json!({ "paths": [], "addresses": ["https://example.org"] }),
            json!({ "paths": [], "addresses": vec!["obsidian://open"; MAX_ADDRESSES + 1] }),
            json!({ "paths": [], "capture": "x".repeat(MAX_CAPTURE_BYTES + 1) }),
        ] {
            let (mut client, mut server) = UnixStream::pair().unwrap();
            client.write_all(body.to_string().as_bytes()).unwrap();
            client.shutdown(std::net::Shutdown::Write).unwrap();
            assert!(read_request(&mut server).is_err(), "{body}");
        }
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let writer = std::thread::spawn(move || {
            let _ = client.write_all(&vec![b' '; MAX_REQUEST_BYTES as usize + 1]);
        });
        assert!(read_request(&mut server).is_err());
        drop(server);
        writer.join().unwrap();
    }
}
