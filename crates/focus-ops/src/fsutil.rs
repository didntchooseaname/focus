use std::path::{Path, PathBuf};

use crate::{OpsError, Result};

pub fn io_error(path: &Path, source: std::io::Error) -> OpsError {
    OpsError::Io {
        path: path.to_path_buf(),
        source,
    }
}

pub fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| io_error(path, e))
}

pub fn read_optional(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_error(path, e)),
    }
}

pub fn create_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|e| io_error(path, e))
}

pub fn write_atomic(path: &Path, text: &str) -> Result<()> {
    crate::keep_version(path)?;
    if let Some(dir) = path.parent() {
        create_dir(dir)?;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.focus-tmp"));
    std::fs::write(&temporary, text).map_err(|e| io_error(&temporary, e))?;
    std::fs::rename(&temporary, path).map_err(|e| io_error(path, e))
}

pub fn write_private(path: &Path, text: &str) -> Result<()> {
    use std::io::Write as _;
    crate::keep_version(path)?;
    if let Some(dir) = path.parent() {
        let mut folder = std::fs::DirBuilder::new();
        folder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut folder, 0o700);
        folder.create(dir).map_err(|e| io_error(dir, e))?;
        #[cfg(unix)]
        std::fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .map_err(|e| io_error(dir, e))?;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.focus-tmp"));
    let _ = std::fs::remove_file(&temporary);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(&temporary)
        .map_err(|e| io_error(&temporary, e))?;
    file.write_all(text.as_bytes())
        .map_err(|e| io_error(&temporary, e))?;
    file.sync_all().map_err(|e| io_error(&temporary, e))?;
    std::fs::rename(&temporary, path).map_err(|e| io_error(path, e))
}

pub fn file_name(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_control()
                || matches!(
                    c,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']'
                )
            {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    cleaned
        .chars()
        .take(120)
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn local_parts(unix_secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    // SAFETY: `tm` is zero initialized and only written by `localtime_r`.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let t = unix_secs as libc::time_t;
        #[cfg(unix)]
        let failed = libc::localtime_r(&t, &mut tm).is_null();
        #[cfg(windows)]
        let failed = libc::localtime_s(&mut tm, &t) != 0;
        if failed {
            return utc_parts(unix_secs);
        }
        (
            tm.tm_year + 1900,
            (tm.tm_mon + 1) as u32,
            tm.tm_mday as u32,
            tm.tm_hour as u32,
            tm.tm_min as u32,
            tm.tm_sec as u32,
        )
    }
}

fn utc_parts(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    (
        year,
        month,
        day,
        (rem / 3600) as u32,
        (rem % 3600 / 60) as u32,
        (rem % 60) as u32,
    )
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn timestamp_now() -> String {
    let (y, mo, d, h, mi, s) = local_parts(now_unix());
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

pub fn date_today() -> String {
    let (y, mo, d, ..) = local_parts(now_unix());
    format!("{y:04}-{mo:02}-{d:02}")
}

#[cfg(test)]
pub fn test_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(format!("ops-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test folder creation");
    dir
}
