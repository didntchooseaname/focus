use std::fs;
#[cfg(unix)]
use std::fs::{DirBuilder, Permissions};
use std::io;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::Path;

#[cfg(unix)]
pub fn private_dir(dir: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    tighten(dir, 0o700)
}

#[cfg(not(unix))]
pub fn private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)
}

#[cfg(not(unix))]
pub fn private_file(_file: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
pub fn private_file(file: &Path) -> io::Result<()> {
    match fs::symlink_metadata(file) {
        Ok(meta) if meta.is_file() => tighten(file, 0o600),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn tighten(path: &Path, mode: u32) -> io::Result<()> {
    if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(path, Permissions::from_mode(mode))?;
    }
    Ok(())
}

#[cfg(unix)]
pub fn same_rights(from: &Path, to: &Path) -> io::Result<()> {
    let mode = fs::metadata(from)?.permissions().mode() & 0o777;
    fs::set_permissions(to, Permissions::from_mode(mode))
}

#[cfg(not(unix))]
pub fn same_rights(from: &Path, to: &Path) -> io::Result<()> {
    fs::set_permissions(to, fs::metadata(from)?.permissions())
}

pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write as _;
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the destination is a symbolic link",
        ));
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    tighten(path, 0o600)?;
    file.write_all(bytes)
}

pub fn log_failure(what: &Path, result: io::Result<()>) {
    if let Err(error) = result {
        eprintln!(
            "focus: cannot restrict {} to its owner: {error}",
            what.display()
        );
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn folders_and_settings_are_restricted_to_their_owner() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-tmp/private");
        let _ = fs::remove_dir_all(&root);
        let loose = root.join("config");
        fs::create_dir_all(&loose).unwrap();
        fs::set_permissions(&loose, Permissions::from_mode(0o755)).unwrap();
        let settings = loose.join("settings.json");
        fs::write(&settings, "{}").unwrap();
        fs::set_permissions(&settings, Permissions::from_mode(0o644)).unwrap();

        private_dir(&loose).unwrap();
        private_file(&settings).unwrap();
        assert_eq!(
            (mode(&loose), mode(&settings)),
            (0o700, 0o600),
            "existing rights are tightened"
        );

        let fresh = root.join("cache/0123456789abcdef");
        private_dir(&fresh).unwrap();
        assert_eq!(
            (mode(&fresh), mode(fresh.parent().unwrap())),
            (0o700, 0o700),
            "created folders and their new parents"
        );
        private_file(&root.join("absent.json")).unwrap();
    }

    #[test]
    fn a_saved_journal_is_private_and_never_written_through_a_link() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-tmp/private-write");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let csv = root.join("journal.csv");
        write_private(&csv, b"time,text\n").unwrap();
        assert_eq!(mode(&csv), 0o600);
        fs::set_permissions(&csv, Permissions::from_mode(0o644)).unwrap();
        write_private(&csv, b"again\n").unwrap();
        assert_eq!(
            (mode(&csv), fs::read_to_string(&csv).unwrap().as_str()),
            (0o600, "again\n"),
            "an existing file is tightened and replaced"
        );
        let target = root.join("target.txt");
        fs::write(&target, "keep").unwrap();
        let link = root.join("link.csv");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(write_private(&link, b"x").is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
    }

    #[test]
    fn an_export_takes_the_rights_of_its_report() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-tmp/private-rights");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let (report, html) = (root.join("Report.md"), root.join("Report.html"));
        fs::write(&report, "secret").unwrap();
        fs::set_permissions(&report, Permissions::from_mode(0o600)).unwrap();
        fs::write(&html, "<p>secret</p>").unwrap();
        fs::set_permissions(&html, Permissions::from_mode(0o644)).unwrap();
        same_rights(&report, &html).unwrap();
        assert_eq!(mode(&html), 0o600);
    }
}
