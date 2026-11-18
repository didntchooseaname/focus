use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use semver::Version;

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Package {
    AppImage,
    Archive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installation {
    package: Package,
    program: PathBuf,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const ARCHIVE: &str = "linux-x86_64.tar.gz";
#[cfg(all(windows, target_arch = "x86_64"))]
const ARCHIVE: &str = "windows-x64.zip";
#[cfg(all(windows, target_arch = "aarch64"))]
const ARCHIVE: &str = "windows-arm64.zip";
#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(windows, target_arch = "x86_64"),
    all(windows, target_arch = "aarch64")
)))]
const ARCHIVE: &str = "";

#[cfg(windows)]
const PROGRAMS: [&str; 2] = ["focus.exe", "focus-mcp.exe"];
#[cfg(not(windows))]
const PROGRAMS: [&str; 2] = ["focus", "focus-mcp"];

const STAGED: &str = ".focus-update-";

#[cfg(windows)]
const REPLACED: &str = "old";

fn io(path: &Path) -> impl Fn(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

impl Installation {
    pub fn detect() -> Option<Installation> {
        if ARCHIVE.is_empty() {
            return None;
        }
        let program = std::env::current_exe().ok()?;
        let image = mounted_image(
            std::env::var_os("APPIMAGE").map(PathBuf::from),
            std::env::var_os("APPDIR").map(PathBuf::from),
            &program,
        );
        Some(match image {
            Some(image) => Installation::new(Package::AppImage, image),
            None => Installation::new(Package::Archive, program),
        })
    }

    pub fn new(package: Package, program: PathBuf) -> Installation {
        Installation { package, program }
    }

    pub fn package(&self) -> Package {
        self.package
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    pub fn file(&self, version: &Version) -> String {
        match self.package {
            Package::AppImage => format!("Focus-{version}-x86_64.AppImage"),
            Package::Archive => format!("focus-{version}-{ARCHIVE}"),
        }
    }

    pub fn tidy(&self) {
        #[cfg(windows)]
        for program in PROGRAMS {
            let _ = fs::remove_file(replaced(&self.target(program)));
        }
        let Ok(entries) = fs::read_dir(self.folder()) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(STAGED) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    fn folder(&self) -> &Path {
        self.program.parent().unwrap_or(Path::new("."))
    }

    fn target(&self, program: &str) -> PathBuf {
        if program == PROGRAMS[0] {
            self.program.clone()
        } else {
            self.folder().join(program)
        }
    }
}

fn mounted_image(
    image: Option<PathBuf>,
    mount: Option<PathBuf>,
    program: &Path,
) -> Option<PathBuf> {
    let (image, mount) = (image?, mount?);
    let inside = mount.is_absolute() && program.starts_with(&mount);
    (cfg!(target_os = "linux") && inside && image.is_absolute() && image.is_file()).then_some(image)
}

pub(crate) struct Staging<'a> {
    installation: &'a Installation,
    files: Vec<PathBuf>,
}

impl Drop for Staging<'_> {
    fn drop(&mut self) {
        for file in &self.files {
            let _ = fs::remove_file(file);
        }
    }
}

impl<'a> Staging<'a> {
    pub(crate) fn new(installation: &'a Installation) -> Staging<'a> {
        Staging {
            installation,
            files: Vec::new(),
        }
    }

    pub(crate) fn create(&mut self, label: &str) -> Result<(PathBuf, File), Error> {
        let path = self
            .installation
            .folder()
            .join(format!("{STAGED}{}-{label}", std::process::id()));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(io(&path))?;
        self.files.push(path.clone());
        Ok((path, file))
    }

    pub(crate) fn place(mut self, download: &Path, name: &str) -> Result<(), Error> {
        let installation = self.installation;
        let mut moves = match installation.package {
            Package::AppImage => vec![(download.to_path_buf(), installation.program.clone())],
            Package::Archive => self.unpack(download, name)?,
        };
        moves.sort_by_key(|(_, to)| *to == installation.program);
        for (from, to) in &moves {
            runnable_like(from, to)?;
        }
        for (from, to) in &moves {
            swap(from, to)?;
        }
        if let Ok(folder) = File::open(installation.folder()) {
            let _ = folder.sync_all();
        }
        Ok(())
    }

    #[cfg(not(windows))]
    fn unpack(&mut self, archive: &Path, name: &str) -> Result<Vec<(PathBuf, PathBuf)>, Error> {
        let packed = flate2::read::GzDecoder::new(File::open(archive).map_err(io(archive))?);
        let mut packed = tar::Archive::new(packed);
        let mut moves = Vec::new();
        for entry in packed.entries().map_err(io(archive))? {
            let mut entry = entry.map_err(io(archive))?;
            let Some(program) = program_of(&entry.path().map_err(io(archive))?) else {
                continue;
            };
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let (path, mut file) = self.create(program)?;
            std::io::copy(&mut entry, &mut file).map_err(io(&path))?;
            file.sync_all().map_err(io(&path))?;
            moves.push((path, self.installation.target(program)));
        }
        self.complete(moves, name)
    }

    #[cfg(windows)]
    fn unpack(&mut self, archive: &Path, name: &str) -> Result<Vec<(PathBuf, PathBuf)>, Error> {
        let damaged = |error: zip::result::ZipError| Error::Io {
            path: archive.to_path_buf(),
            source: std::io::Error::other(error),
        };
        let mut packed =
            zip::ZipArchive::new(File::open(archive).map_err(io(archive))?).map_err(damaged)?;
        let mut moves = Vec::new();
        for index in 0..packed.len() {
            let mut entry = packed.by_index(index).map_err(damaged)?;
            let Some(program) = entry.enclosed_name().as_deref().and_then(program_of) else {
                continue;
            };
            if !entry.is_file() {
                continue;
            }
            let (path, mut file) = self.create(program)?;
            std::io::copy(&mut entry, &mut file).map_err(io(&path))?;
            file.sync_all().map_err(io(&path))?;
            moves.push((path, self.installation.target(program)));
        }
        self.complete(moves, name)
    }

    fn complete(
        &self,
        moves: Vec<(PathBuf, PathBuf)>,
        name: &str,
    ) -> Result<Vec<(PathBuf, PathBuf)>, Error> {
        if moves.iter().any(|(_, to)| *to == self.installation.program) {
            Ok(moves)
        } else {
            Err(Error::Incomplete(name.to_string()))
        }
    }
}

fn program_of(path: &Path) -> Option<&'static str> {
    let mut parts = path.components();
    let (Some(std::path::Component::Normal(_)), Some(std::path::Component::Normal(name)), None) =
        (parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    PROGRAMS.into_iter().find(|program| name == *program)
}

#[cfg(unix)]
fn runnable_like(staged: &Path, current: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = fs::metadata(current).map_or(0o755, |meta| meta.permissions().mode() | 0o100);
    fs::set_permissions(staged, fs::Permissions::from_mode(mode & 0o777)).map_err(io(staged))
}

#[cfg(not(unix))]
fn runnable_like(_: &Path, _: &Path) -> Result<(), Error> {
    Ok(())
}

#[cfg(not(windows))]
fn swap(from: &Path, to: &Path) -> Result<(), Error> {
    fs::rename(from, to).map_err(io(to))
}

#[cfg(windows)]
fn replaced(program: &Path) -> PathBuf {
    let mut name = program.as_os_str().to_os_string();
    name.push(".");
    name.push(REPLACED);
    PathBuf::from(name)
}

#[cfg(windows)]
fn swap(from: &Path, to: &Path) -> Result<(), Error> {
    let old = replaced(to);
    let running = to.exists();
    if running {
        let _ = fs::remove_file(&old);
        fs::rename(to, &old).map_err(io(to))?;
    }
    fs::rename(from, to).map_err(|source| {
        if running {
            let _ = fs::rename(&old, to);
        }
        Error::Io {
            path: to.to_path_buf(),
            source,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_programs_at_the_top_of_the_archive_folder_are_taken() {
        let main = PROGRAMS[0];
        assert_eq!(program_of(&Path::new("focus-1.0.0").join(main)), Some(main));
        assert_eq!(
            program_of(&Path::new("focus-1.0.0").join(PROGRAMS[1])),
            Some(PROGRAMS[1])
        );
        assert_eq!(program_of(Path::new(main)), None);
        assert_eq!(program_of(&Path::new("a").join("b").join(main)), None);
        assert_eq!(program_of(&Path::new("..").join(main)), None);
        assert_eq!(
            program_of(&Path::new("focus-1.0.0").join("README.md")),
            None
        );
    }

    #[test]
    fn the_file_asked_for_is_the_one_of_the_package_installed() {
        let version = Version::new(1, 4, 2);
        let image = Installation::new(Package::AppImage, PathBuf::from("/opt/Focus.AppImage"));
        assert_eq!(image.file(&version), "Focus-1.4.2-x86_64.AppImage");
        let archive = Installation::new(Package::Archive, PathBuf::from("/opt/focus/focus"));
        assert_eq!(archive.file(&version), format!("focus-1.4.2-{ARCHIVE}"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_app_image_of_the_environment_counts_only_for_a_program_inside_its_mount() {
        let image = std::env::current_exe().unwrap();
        let mount = PathBuf::from("/tmp/.mount_Focus");
        let inside = mount.join("usr/bin/focus");
        let named = |mount: Option<PathBuf>, program: &Path| {
            mounted_image(Some(image.clone()), mount, program)
        };
        assert_eq!(named(Some(mount.clone()), &inside), Some(image.clone()));
        assert_eq!(
            named(Some(mount.clone()), Path::new("/usr/bin/focus")),
            None
        );
        assert_eq!(named(None, &inside), None);
        assert_eq!(named(Some(PathBuf::new()), &inside), None);
        assert_eq!(
            mounted_image(Some(mount.join("missing")), Some(mount), &inside),
            None
        );
    }

    #[test]
    fn the_second_program_goes_beside_the_one_running() {
        let folder = Path::new("opt").join("tools");
        let installation = Installation::new(Package::Archive, folder.join("focus-renamed"));
        assert_eq!(
            installation.target(PROGRAMS[0]),
            folder.join("focus-renamed")
        );
        assert_eq!(installation.target(PROGRAMS[1]), folder.join(PROGRAMS[1]));
    }
}
