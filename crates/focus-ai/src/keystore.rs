#[cfg(not(windows))]
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::{Command, Stdio};
use std::time::Duration;
#[cfg(not(windows))]
use std::time::Instant;

use anyhow::{Context, bail};

use super::ApiKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Environment,
    Keyring,
    File,
}

const ENV_VAR: &str = "DEEPSEEK_API_KEY";
const SECRET_TOOL: &str = "secret-tool";
const ATTRIBUTES: [&str; 4] = ["service", "focus", "account", "deepseek"];
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(not(windows))]
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

pub fn load() -> Option<(ApiKey, Location)> {
    load_from(&Source {
        env: std::env::var(ENV_VAR).ok(),
        secret_tool: SECRET_TOOL.into(),
        file: default_file(),
        lookup_timeout: LOOKUP_TIMEOUT,
    })
}

pub fn store(key: &ApiKey) -> anyhow::Result<Location> {
    store_to(
        &Source {
            env: None,
            secret_tool: SECRET_TOOL.into(),
            file: default_file(),
            lookup_timeout: LOOKUP_TIMEOUT,
        },
        key,
    )
}

pub fn delete() -> anyhow::Result<()> {
    delete_from(&Source {
        env: None,
        secret_tool: SECRET_TOOL.into(),
        file: default_file(),
        lookup_timeout: LOOKUP_TIMEOUT,
    })
}

struct Source {
    env: Option<String>,
    secret_tool: String,
    file: Option<PathBuf>,
    lookup_timeout: Duration,
}

#[cfg(not(windows))]
fn default_file() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("focus").join("deepseek.key"))
}

#[cfg(windows)]
fn default_file() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").filter(|v| !v.is_empty())?;
    Some(PathBuf::from(base).join("focus").join("deepseek.key"))
}

fn load_from(source: &Source) -> Option<(ApiKey, Location)> {
    if let Some(value) = source
        .env
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return Some((ApiKey::new(value), Location::Environment));
    }
    if let Some(value) = keyring_lookup(&source.secret_tool, source.lookup_timeout) {
        return Some((ApiKey::new(value), Location::Keyring));
    }
    let text = std::fs::read_to_string(source.file.as_deref()?).ok()?;
    let value = text.trim();
    (!value.is_empty()).then(|| (ApiKey::new(value), Location::File))
}

fn store_to(source: &Source, key: &ApiKey) -> anyhow::Result<Location> {
    if key.expose().trim().is_empty() {
        bail!("empty API key");
    }
    if keyring_store(&source.secret_tool, key.expose()).is_ok() {
        if let Some(path) = &source.file {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e).with_context(|| format!("deleting {}", path.display())),
            }
        }
        return Ok(Location::Keyring);
    }
    let path = source
        .file
        .as_deref()
        .context("configuration folder not found (HOME unset)")?;
    write_private(path, key.expose())?;
    Ok(Location::File)
}

fn delete_from(source: &Source) -> anyhow::Result<()> {
    keyring::clear(&source.secret_tool, &ATTRIBUTES);
    if let Some(path) = &source.file {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("deleting {}", path.display())),
        }
    }
    Ok(())
}

pub mod provider_key {
    use super::*;
    use crate::Provider;

    fn attributes(provider: Provider) -> [String; 4] {
        [
            "service".into(),
            "focus".into(),
            "account".into(),
            format!("provider-{}", provider.spec().id),
        ]
    }

    pub fn load(provider: Provider) -> Option<ApiKey> {
        if let Some(value) = provider
            .spec()
            .key_env
            .and_then(|name| std::env::var(name).ok())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            return Some(ApiKey::new(value));
        }
        let attributes = attributes(provider);
        let attributes = attributes.each_ref().map(String::as_str);
        let value =
            String::from_utf8(keyring::lookup(SECRET_TOOL, &attributes, LOOKUP_TIMEOUT)?).ok()?;
        let value = value.trim();
        (!value.is_empty()).then(|| ApiKey::new(value))
    }

    pub fn store(provider: Provider, key: &ApiKey) -> anyhow::Result<()> {
        if key.expose().trim().is_empty() {
            bail!("empty API key");
        }
        let attributes = attributes(provider);
        keyring::store(
            SECRET_TOOL,
            &format!("Focus: {} API key", provider.spec().label),
            &attributes.each_ref().map(String::as_str),
            key.expose().trim().as_bytes(),
        )
    }

    pub fn delete(provider: Provider) {
        let attributes = attributes(provider);
        keyring::clear(SECRET_TOOL, &attributes.each_ref().map(String::as_str));
    }
}

pub mod server_env {
    use super::*;

    fn attributes<'a>(server: &'a str, variable: &'a str) -> [&'a str; 8] {
        [
            "service", "focus", "account", "mcp-env", "server", server, "variable", variable,
        ]
    }

    pub fn store(server: &str, variable: &str, value: &str) -> anyhow::Result<()> {
        store_with(SECRET_TOOL, server, variable, value)
    }

    pub fn load(server: &str, variable: &str) -> Option<String> {
        load_with(SECRET_TOOL, server, variable)
    }

    pub fn delete(server: &str, variable: &str) {
        delete_with(SECRET_TOOL, server, variable)
    }

    pub fn available() -> bool {
        available_with(SECRET_TOOL)
    }

    pub(super) fn store_with(
        tool: &str,
        server: &str,
        variable: &str,
        value: &str,
    ) -> anyhow::Result<()> {
        if value.is_empty() {
            bail!("empty value");
        }
        keyring::store(
            tool,
            "Focus: MCP server variable",
            &attributes(server, variable),
            value.as_bytes(),
        )
    }

    pub(super) fn load_with(tool: &str, server: &str, variable: &str) -> Option<String> {
        let value = String::from_utf8(keyring::lookup(
            tool,
            &attributes(server, variable),
            LOOKUP_TIMEOUT,
        )?)
        .ok()?;
        (!value.is_empty()).then_some(value)
    }

    pub(super) fn delete_with(tool: &str, server: &str, variable: &str) {
        keyring::clear(tool, &attributes(server, variable));
    }

    #[cfg(windows)]
    pub(super) fn available_with(_tool: &str) -> bool {
        true
    }

    #[cfg(not(windows))]
    pub(super) fn available_with(tool: &str) -> bool {
        let mut command = Command::new(tool);
        command
            .arg("lookup")
            .args(attributes("", "availability-probe"));
        let mut child = match command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => return false,
        };
        let deadline = Instant::now() + LOOKUP_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
            }
        }
    }
}

fn keyring_lookup(tool: &str, timeout: Duration) -> Option<String> {
    let output = keyring::lookup(tool, &ATTRIBUTES, timeout)?;
    let value = String::from_utf8(output).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn keyring_store(tool: &str, secret: &str) -> anyhow::Result<()> {
    keyring::store(
        tool,
        "Focus: DeepSeek API key",
        &ATTRIBUTES,
        secret.as_bytes(),
    )
}

mod keyring {
    use super::*;

    #[cfg(not(windows))]
    pub(super) fn lookup(tool: &str, attributes: &[&str], timeout: Duration) -> Option<Vec<u8>> {
        let mut command = Command::new(tool);
        command.arg("lookup").args(attributes);
        run_with_timeout(command, None, timeout)
    }

    #[cfg(not(windows))]
    pub(super) fn store(
        tool: &str,
        label: &str,
        attributes: &[&str],
        secret: &[u8],
    ) -> anyhow::Result<()> {
        let mut command = Command::new(tool);
        command
            .arg("store")
            .arg(format!("--label={label}"))
            .args(attributes);
        run_with_timeout(command, Some(secret), WRITE_TIMEOUT)
            .context("secret-tool failed, took more than 30 s or was not found")?;
        Ok(())
    }

    #[cfg(not(windows))]
    pub(super) fn clear(tool: &str, attributes: &[&str]) {
        let mut command = Command::new(tool);
        command.arg("clear").args(attributes);
        let _ = run_with_timeout(command, None, WRITE_TIMEOUT);
    }

    #[cfg(windows)]
    pub(super) fn lookup(_tool: &str, attributes: &[&str], _timeout: Duration) -> Option<Vec<u8>> {
        credentials::read(&credentials::target(attributes))
    }

    #[cfg(windows)]
    pub(super) fn store(
        _tool: &str,
        label: &str,
        attributes: &[&str],
        secret: &[u8],
    ) -> anyhow::Result<()> {
        credentials::write(&credentials::target(attributes), label, secret)
    }

    #[cfg(windows)]
    pub(super) fn clear(_tool: &str, attributes: &[&str]) {
        credentials::delete(&credentials::target(attributes));
    }
}

#[cfg(windows)]
mod credentials {
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    pub(super) fn target(attributes: &[&str]) -> String {
        let values: Vec<String> = attributes
            .chunks(2)
            .skip(1)
            .filter_map(|pair| pair.get(1))
            .map(|value| value.replace('%', "%25").replace(':', "%3A"))
            .collect();
        format!("focus:{}", values.join(":"))
    }

    pub(super) fn read(target: &str) -> Option<Vec<u8>> {
        let target = wide(target);
        let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `target` is NUL terminated. On success `credential` points to a structure
        // of the system, read here and given back to `CredFree`.
        unsafe {
            if CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) == 0 {
                return None;
            }
            let size = (*credential).CredentialBlobSize as usize;
            let blob = if size == 0 || (*credential).CredentialBlob.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts((*credential).CredentialBlob, size).to_vec()
            };
            CredFree(credential.cast());
            Some(blob)
        }
    }

    pub(super) fn write(target: &str, label: &str, secret: &[u8]) -> anyhow::Result<()> {
        let mut target = wide(target);
        let mut comment = wide(label);
        let mut blob = secret.to_vec();
        // SAFETY: a C structure of integers and pointers, for which zero is a valid value.
        let mut credential: CREDENTIALW = unsafe { std::mem::zeroed() };
        credential.Type = CRED_TYPE_GENERIC;
        credential.TargetName = target.as_mut_ptr();
        credential.Comment = comment.as_mut_ptr();
        credential.CredentialBlobSize = u32::try_from(blob.len())?;
        credential.CredentialBlob = blob.as_mut_ptr();
        credential.Persist = CRED_PERSIST_LOCAL_MACHINE;
        // SAFETY: every pointer stays valid during the call, which copies what it keeps.
        let written = unsafe { CredWriteW(&credential, 0) } != 0;
        let error = std::io::Error::last_os_error();
        blob.fill(0);
        anyhow::ensure!(written, "the Credential Manager refused the value: {error}");
        Ok(())
    }

    pub(super) fn delete(target: &str) {
        let target = wide(target);
        // SAFETY: `target` is NUL terminated; a missing credential only returns an error.
        unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
    }
}

#[cfg(not(windows))]
fn run_with_timeout(
    mut command: Command,
    input: Option<&[u8]>,
    timeout: Duration,
) -> Option<Vec<u8>> {
    let mut child = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(input) = input {
        let mut stdin = child.stdin.take()?;
        let _ = stdin.write_all(input);
    }
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    reader.join().ok()
                } else {
                    None
                };
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn write_private(path: &Path, secret: &str) -> anyhow::Result<()> {
    let dir = path.parent().context("path without a parent folder")?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let temporary = path.with_extension("key.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(&temporary)
        .with_context(|| format!("creating {}", temporary.display()))?;
    #[cfg(unix)]
    file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    file.write_all(secret.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&temporary, path).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(format!("keystore-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn program(script: &Path, text: &str) -> String {
        std::fs::write(script, text).unwrap();
        std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o755)).unwrap();
        while matches!(
            Command::new(script).arg("ready").output(),
            Err(error) if error.kind() == std::io::ErrorKind::ExecutableFileBusy
        ) {
            std::thread::sleep(Duration::from_millis(2));
        }
        script.display().to_string()
    }

    fn fake_tool(dir: &Path) -> String {
        let store = dir.join("keyring.txt");
        program(
            &dir.join("secret-tool"),
            &format!(
                "#!/bin/sh\ncase \"$1\" in\n store) cat > '{0}';;\n lookup) [ -s '{0}' ] && cat '{0}' || exit 1;;\n clear) rm -f '{0}';;\nesac\n",
                store.display()
            ),
        )
    }

    #[test]
    fn file_fallback_is_private_and_roundtrips() {
        let dir = scratch("file");
        let source = Source {
            env: None,
            secret_tool: "/nonexistent/secret-tool".into(),
            file: Some(dir.join("focus/deepseek.key")),
            lookup_timeout: Duration::from_secs(2),
        };
        assert_eq!(
            store_to(&source, &ApiKey::new("sk-abcdef1234")).unwrap(),
            Location::File
        );
        let mode = std::fs::metadata(source.file.as_ref().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let (key, location) = load_from(&source).unwrap();
        assert_eq!((key.expose(), location), ("sk-abcdef1234", Location::File));
        delete_from(&source).unwrap();
        assert!(load_from(&source).is_none());
    }

    #[test]
    fn keyring_is_preferred_and_removes_stale_file() {
        let dir = scratch("keyring");
        let file = dir.join("deepseek.key");
        std::fs::write(&file, "old-key").unwrap();
        let source = Source {
            env: None,
            secret_tool: fake_tool(&dir),
            file: Some(file.clone()),
            lookup_timeout: Duration::from_secs(2),
        };
        assert_eq!(
            store_to(&source, &ApiKey::new("sk-new-0000")).unwrap(),
            Location::Keyring
        );
        assert!(!file.exists());
        let (key, location) = load_from(&source).unwrap();
        assert_eq!((key.expose(), location), ("sk-new-0000", Location::Keyring));
    }

    #[test]
    fn environment_wins_over_everything() {
        let dir = scratch("env");
        let source = Source {
            env: Some(" sk-env-1111 \n".into()),
            secret_tool: fake_tool(&dir),
            file: None,
            lookup_timeout: Duration::from_secs(2),
        };
        let (key, location) = load_from(&source).unwrap();
        assert_eq!(
            (key.expose(), location),
            ("sk-env-1111", Location::Environment)
        );
    }

    #[test]
    fn locked_keyring_does_not_block_and_falls_back_to_file() {
        let dir = scratch("locked");
        let tool = program(
            &dir.join("secret-tool"),
            "#!/bin/sh\n[ \"$1\" = ready ] && exit\nsleep 30\n",
        );
        let file = dir.join("deepseek.key");
        std::fs::write(&file, "sk-from-file-9999\n").unwrap();
        let source = Source {
            env: None,
            secret_tool: tool,
            file: Some(file),
            lookup_timeout: Duration::from_millis(300),
        };
        let started = Instant::now();
        let (key, location) = load_from(&source).unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(
            (key.expose(), location),
            ("sk-from-file-9999", Location::File)
        );
    }

    fn keyed_fake_tool(dir: &Path) -> String {
        std::fs::create_dir_all(dir.join("items")).unwrap();
        program(
            &dir.join("secret-tool"),
            &format!(
                "#!/bin/sh\nd='{}'\nverb=$1; shift\n[ \"$verb\" = store ] && shift\nk=$(printf '%s' \"$*\" | od -An -tx1 | tr -d ' \\n')\ncase \"$verb\" in\n store) cat > \"$d/$k\";;\n lookup) [ -s \"$d/$k\" ] && cat \"$d/$k\" || exit 1;;\n clear) rm -f \"$d/$k\";;\nesac\n",
                dir.join("items").display()
            ),
        )
    }

    #[test]
    fn server_variables_are_kept_apart_per_server_and_name() {
        let dir = scratch("server-env");
        let tool = keyed_fake_tool(&dir);
        server_env::store_with(&tool, "docs", "TOKEN", "abc").unwrap();
        server_env::store_with(&tool, "docs", "OTHER", "def").unwrap();
        server_env::store_with(&tool, "tickets", "TOKEN", "ghi").unwrap();
        assert_eq!(
            server_env::load_with(&tool, "docs", "TOKEN").as_deref(),
            Some("abc")
        );
        assert_eq!(
            server_env::load_with(&tool, "tickets", "TOKEN").as_deref(),
            Some("ghi")
        );
        server_env::delete_with(&tool, "docs", "TOKEN");
        assert_eq!(server_env::load_with(&tool, "docs", "TOKEN"), None);
        assert_eq!(
            server_env::load_with(&tool, "docs", "OTHER").as_deref(),
            Some("def")
        );
        assert!(server_env::available_with(&tool));
    }

    #[test]
    fn without_a_keyring_a_value_is_refused_and_nothing_is_written() {
        let dir = scratch("server-env-missing");
        let tool = "/nonexistent/secret-tool";
        assert!(!server_env::available_with(tool));
        assert!(server_env::store_with(tool, "docs", "TOKEN", "abc").is_err());
        assert_eq!(server_env::load_with(tool, "docs", "TOKEN"), None);
        assert!(server_env::store_with(&keyed_fake_tool(&dir), "docs", "TOKEN", "").is_err());
        assert_eq!(std::fs::read_dir(dir.join("items")).unwrap().count(), 0);
    }

    #[test]
    fn empty_key_is_refused() {
        let dir = scratch("empty");
        let source = Source {
            env: None,
            secret_tool: fake_tool(&dir),
            file: None,
            lookup_timeout: Duration::from_secs(2),
        };
        assert!(store_to(&source, &ApiKey::new("  ")).is_err());
    }
}
