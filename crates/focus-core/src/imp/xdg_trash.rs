use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

pub(crate) fn data_home() -> io::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|d| d.is_absolute())
    {
        return Ok(dir);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|d| d.is_absolute());
    home.map(|h| h.join(".local/share")).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "neither XDG_DATA_HOME nor HOME is set",
        )
    })
}

pub(crate) fn trash(abs: &Path, data_home: &Path) -> io::Result<PathBuf> {
    let device = fs::symlink_metadata(abs)?.dev();
    let home_trash = data_home.join("Trash");
    fs::create_dir_all(data_home)?;
    let (trash, topdir) = if fs::metadata(data_home)?.dev() == device {
        (home_trash, None)
    } else {
        let top = mount_top(abs, device)?;
        (top_trash(&top)?, Some(top))
    };
    let (files, info) = (trash.join("files"), trash.join("info"));
    for dir in [&trash, &files, &info] {
        match fs::DirBuilder::new().mode(0o700).create(dir) {
            Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e),
            _ => {}
        }
    }
    let recorded = match &topdir {
        Some(top) => abs.strip_prefix(top).unwrap_or(abs).to_path_buf(),
        None => abs.to_path_buf(),
    };
    let contents = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        percent_encode(&recorded),
        local_now()
    );
    let name = abs
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?
        .to_string_lossy()
        .into_owned();
    for n in 0u32.. {
        let candidate = if n == 0 {
            name.clone()
        } else {
            numbered(&name, n)
        };
        let info_path = info.join(format!("{candidate}.trashinfo"));
        let target = files.join(&candidate);
        if fs::symlink_metadata(&target).is_ok() {
            continue;
        }
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&info_path)
        {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let moved = file
            .write_all(contents.as_bytes())
            .and_then(|_| file.sync_all())
            .and_then(|_| fs::rename(abs, &target));
        return match moved {
            Ok(()) => Ok(target),
            Err(e) => {
                let _ = fs::remove_file(&info_path);
                Err(e)
            }
        };
    }
    Err(io::Error::other("no free name in the trash"))
}

fn numbered(name: &str, n: u32) -> String {
    match name.rfind('.').filter(|&dot| dot > 0) {
        Some(dot) => format!("{} {}{}", &name[..dot], n + 1, &name[dot..]),
        None => format!("{name} {}", n + 1),
    }
}

fn mount_top(abs: &Path, device: u64) -> io::Result<PathBuf> {
    let mut top = abs.parent().unwrap_or(abs).to_path_buf();
    while let Some(parent) = top.parent() {
        if fs::metadata(parent)?.dev() != device {
            break;
        }
        top = parent.to_path_buf();
    }
    Ok(top)
}

fn top_trash(top: &Path) -> io::Result<PathBuf> {
    // SAFETY: getuid has no failure mode and touches no memory.
    let uid = unsafe { libc::getuid() };
    let shared = top.join(".Trash");
    if let Ok(meta) = fs::symlink_metadata(&shared)
        && meta.is_dir()
        && meta.mode() & 0o1000 != 0
    {
        return Ok(shared.join(uid.to_string()));
    }
    Ok(top.join(format!(".Trash-{uid}")))
}

fn percent_encode(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut out = String::new();
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn local_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    // SAFETY: `tm` is plain data that localtime_r fills; both pointers are valid for the
    // call.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let filled = unsafe { !libc::localtime_r(&now, &mut tm).is_null() };
    if !filled {
        return "1970-01-01T00:00:00".to_string();
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trashed_file_has_its_info_and_a_second_one_another_name() {
        let dir = crate::imp::test_dir("xdg-trash");
        let data_home = dir.join("data");
        let vault = dir.join("vault");
        fs::create_dir_all(vault.join("Red Team")).expect("vault folder");
        let note = vault.join("Red Team/Plan de test.md");
        fs::write(&note, "a").expect("note");
        let first = trash(&note, &data_home).expect("trashed");
        assert_eq!(first, data_home.join("Trash/files/Plan de test.md"));
        assert!(!note.exists());
        let info = fs::read_to_string(data_home.join("Trash/info/Plan de test.md.trashinfo"))
            .expect("info file");
        let lines: Vec<&str> = info.lines().collect();
        assert_eq!(lines[0], "[Trash Info]");
        assert_eq!(lines[1], format!("Path={}", percent_encode(&note)));
        assert!(lines[1].ends_with("/Red%20Team/Plan%20de%20test.md"));
        assert!(
            lines[2].starts_with("DeletionDate=") && lines[2].len() == "DeletionDate=".len() + 19
        );
        fs::write(&note, "b").expect("note again");
        let second = trash(&note, &data_home).expect("trashed again");
        assert_eq!(second, data_home.join("Trash/files/Plan de test 2.md"));
        assert!(
            data_home
                .join("Trash/info/Plan de test 2.md.trashinfo")
                .exists()
        );
        assert_eq!(fs::read_to_string(second).expect("second file"), "b");
    }

    #[test]
    fn names_and_paths_follow_the_specification() {
        assert_eq!(numbered("a.md", 1), "a 2.md");
        assert_eq!(numbered(".hidden", 2), ".hidden 3");
        assert_eq!(
            percent_encode(Path::new("/x/é b%.md")),
            "/x/%C3%A9%20b%25.md"
        );
    }
}
