use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use focus_update::{
    Error, Installation, Package, Progress, Release, Source, Version, check, install,
};
use sha2::{Digest as _, Sha256};

#[cfg(windows)]
const PROGRAMS: [&str; 2] = ["focus.exe", "focus-mcp.exe"];
#[cfg(not(windows))]
const PROGRAMS: [&str; 2] = ["focus", "focus-mcp"];

struct Feed {
    releases: String,
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    asked: Arc<Mutex<Vec<String>>>,
    pair: minisign::KeyPair,
}

impl Feed {
    fn start() -> Feed {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let releases = format!("http://{}", listener.local_addr().unwrap());
        let files: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::default();
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let (served, seen) = (files.clone(), asked.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut request = Vec::new();
                let mut byte = [0u8; 1];
                while !request.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                    request.push(byte[0]);
                }
                let request = String::from_utf8_lossy(&request);
                let path = request.split(' ').nth(1).unwrap_or("").to_string();
                seen.lock().unwrap().push(path.clone());
                let body = served.lock().unwrap().get(&path).cloned();
                let head = match &body {
                    Some(body) => format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    ),
                    None => {
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_string()
                    }
                };
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body.unwrap_or_default());
            }
        });
        Feed {
            releases,
            files,
            asked,
            pair: minisign::KeyPair::generate_unencrypted_keypair().unwrap(),
        }
    }

    fn source(&self) -> Source {
        Source {
            releases: self.releases.clone(),
            key: self.pair.pk.to_box().unwrap().into_string(),
        }
    }

    fn serve(&self, path: &str, bytes: &[u8]) {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_string(), bytes.to_vec());
    }

    fn sign(&self, list: &str) -> String {
        minisign::sign(
            None,
            &self.pair.sk,
            std::io::Cursor::new(list.as_bytes()),
            Some("test release"),
            None,
        )
        .unwrap()
        .into_string()
    }

    fn publish(&self, version: &str, files: &[(&str, &[u8])]) {
        let list: String = files
            .iter()
            .map(|(name, bytes)| format!("{}  {name}\n", hex(&Sha256::digest(bytes))))
            .collect();
        self.serve("/latest/download/SHA256SUMS.txt", list.as_bytes());
        self.serve(
            "/latest/download/SHA256SUMS.txt.minisig",
            self.sign(&list).as_bytes(),
        );
        for (name, bytes) in files {
            self.serve(&format!("/download/v{version}/{name}"), bytes);
        }
    }

    fn asked_for(&self, part: &str) -> bool {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .any(|path| path.contains(part))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn folder(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("focus-update-{name}"));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[cfg(not(windows))]
fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let packed = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut packed = tar::Builder::new(packed);
    for (path, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        packed.append_data(&mut header, path, *bytes).unwrap();
    }
    packed.into_inner().unwrap().finish().unwrap()
}

#[cfg(windows)]
fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut packed = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, bytes) in files {
        packed
            .start_file(*path, zip::write::SimpleFileOptions::default())
            .unwrap();
        packed.write_all(bytes).unwrap();
    }
    packed.finish().unwrap().into_inner()
}

fn offered(feed: &Feed) -> Release {
    check(&feed.source(), &Version::new(1, 0, 0))
        .unwrap()
        .expect("a newer release")
}

fn run(release: &Release, installation: &Installation) -> Result<Vec<Progress>, Error> {
    let mut steps = Vec::new();
    install(
        release,
        installation,
        &AtomicBool::new(false),
        &mut |step| steps.push(step),
    )
    .map(|()| steps)
}

fn installed(name: &str) -> (PathBuf, Installation) {
    let folder = folder(name);
    let program = folder.join(PROGRAMS[0]);
    std::fs::write(&program, b"old program").unwrap();
    (folder, Installation::new(Package::Archive, program))
}

#[test]
fn a_newer_signed_release_is_offered_with_its_page() {
    let feed = Feed::start();
    feed.publish("1.2.0", &[("focus-1.2.0-linux-x86_64.tar.gz", b"x")]);
    let release = offered(&feed);
    assert_eq!(release.version, Version::new(1, 2, 0));
    assert_eq!(release.page(), format!("{}/tag/v1.2.0", feed.releases));
}

#[test]
fn the_version_running_and_older_ones_are_not_offered() {
    let feed = Feed::start();
    feed.publish("1.2.0", &[("focus-1.2.0-linux-x86_64.tar.gz", b"x")]);
    for running in [
        Version::new(1, 2, 0),
        Version::new(1, 3, 0),
        Version::new(2, 0, 0),
    ] {
        assert_eq!(check(&feed.source(), &running).unwrap(), None);
    }
}

#[test]
fn a_list_signed_with_another_key_is_refused() {
    let feed = Feed::start();
    feed.publish("1.2.0", &[("focus-1.2.0-linux-x86_64.tar.gz", b"x")]);
    let other = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let source = Source {
        key: other.pk.to_box().unwrap().into_string(),
        ..feed.source()
    };
    assert!(matches!(
        check(&source, &Version::new(1, 0, 0)),
        Err(Error::Signature)
    ));
}

#[test]
fn a_list_changed_after_it_was_signed_is_refused() {
    let feed = Feed::start();
    feed.publish("1.2.0", &[("focus-1.2.0-linux-x86_64.tar.gz", b"x")]);
    let forged = format!("{}  focus-9.0.0-linux-x86_64.tar.gz\n", "0".repeat(64));
    feed.serve("/latest/download/SHA256SUMS.txt", forged.as_bytes());
    assert!(matches!(
        check(&feed.source(), &Version::new(1, 0, 0)),
        Err(Error::Signature)
    ));
}

#[test]
fn a_release_without_a_signature_is_refused() {
    let feed = Feed::start();
    feed.serve(
        "/latest/download/SHA256SUMS.txt",
        format!("{}  focus-1.2.0-linux-x86_64.tar.gz\n", "0".repeat(64)).as_bytes(),
    );
    assert!(matches!(
        check(&feed.source(), &Version::new(1, 0, 0)),
        Err(Error::Unsigned)
    ));
}

#[test]
fn a_repository_without_a_release_offers_nothing() {
    let feed = Feed::start();
    assert_eq!(check(&feed.source(), &Version::new(1, 0, 0)).unwrap(), None);
}

#[test]
fn a_server_that_does_not_answer_is_a_network_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let releases = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let source = Source {
        releases,
        key: focus_update::RELEASE_KEY.to_string(),
    };
    assert!(matches!(
        check(&source, &Version::new(1, 0, 0)),
        Err(Error::Network { .. })
    ));
}

#[test]
fn the_archive_replaces_the_program_and_adds_the_second_one() {
    let feed = Feed::start();
    let (folder, installation) = installed("archive");
    let name = installation.file(&Version::new(1, 2, 0));
    let packed = archive(&[
        ("focus-1.2.0/README.md", b"read me"),
        (&format!("focus-1.2.0/{}", PROGRAMS[1]), b"new server"),
        (&format!("focus-1.2.0/{}", PROGRAMS[0]), b"new program"),
        (
            &format!("focus-1.2.0/nested/{}", PROGRAMS[0]),
            b"not this one",
        ),
    ]);
    feed.publish("1.2.0", &[(&name, &packed)]);
    let steps = run(&offered(&feed), &installation).unwrap();
    assert_eq!(
        std::fs::read(folder.join(PROGRAMS[0])).unwrap(),
        b"new program"
    );
    assert_eq!(
        std::fs::read(folder.join(PROGRAMS[1])).unwrap(),
        b"new server"
    );
    let size = packed.len() as u64;
    assert_eq!(
        steps.last(),
        Some(&Progress {
            received: size,
            total: Some(size)
        })
    );
    installation.tidy();
    let mut programs = PROGRAMS.map(String::from);
    programs.sort();
    assert_eq!(names(&folder), programs);
}

#[cfg(unix)]
#[test]
fn the_programs_placed_can_be_run_and_keep_the_mode_of_the_one_replaced() {
    use std::os::unix::fs::PermissionsExt as _;
    let feed = Feed::start();
    let (folder, installation) = installed("modes");
    let mode = |name: &str| {
        std::fs::metadata(folder.join(name))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    };
    std::fs::set_permissions(
        installation.program(),
        std::fs::Permissions::from_mode(0o750),
    )
    .unwrap();
    let name = installation.file(&Version::new(1, 2, 0));
    let packed = archive(&[
        ("focus-1.2.0/focus", b"new program"),
        ("focus-1.2.0/focus-mcp", b"new server"),
    ]);
    feed.publish("1.2.0", &[(&name, &packed)]);
    run(&offered(&feed), &installation).unwrap();
    assert_eq!(mode("focus"), 0o750);
    assert_eq!(mode("focus-mcp"), 0o755);
}

#[cfg(unix)]
#[test]
fn an_app_image_is_replaced_in_place() {
    use std::os::unix::fs::PermissionsExt as _;
    let feed = Feed::start();
    let folder = folder("appimage");
    let image = folder.join("Focus-1.0.0-x86_64.AppImage");
    std::fs::write(&image, b"old image").unwrap();
    std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o700)).unwrap();
    let installation = Installation::new(Package::AppImage, image.clone());
    feed.publish("1.2.0", &[("Focus-1.2.0-x86_64.AppImage", b"new image")]);
    run(&offered(&feed), &installation).unwrap();
    assert_eq!(std::fs::read(&image).unwrap(), b"new image");
    assert_eq!(
        std::fs::metadata(&image).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(names(&folder), ["Focus-1.0.0-x86_64.AppImage"]);
}

#[test]
fn a_file_that_differs_from_its_digest_is_not_installed() {
    let feed = Feed::start();
    let (folder, installation) = installed("digest");
    let name = installation.file(&Version::new(1, 2, 0));
    let packed = archive(&[(&format!("focus-1.2.0/{}", PROGRAMS[0]), b"new program")]);
    feed.publish("1.2.0", &[(&name, &packed)]);
    let release = offered(&feed);
    let swapped = archive(&[(&format!("focus-1.2.0/{}", PROGRAMS[0]), b"another program")]);
    feed.serve(&format!("/download/v1.2.0/{name}"), &swapped);
    assert!(matches!(run(&release, &installation), Err(Error::Checksum(file)) if file == name));
    assert_eq!(
        std::fs::read(folder.join(PROGRAMS[0])).unwrap(),
        b"old program"
    );
    assert_eq!(names(&folder), [PROGRAMS[0]]);
}

#[test]
fn a_release_without_the_file_of_the_package_installed_is_refused() {
    let feed = Feed::start();
    let (folder, installation) = installed("missing");
    feed.publish("1.2.0", &[("focus-1.2.0-other-system.tar.gz", b"x")]);
    let name = installation.file(&Version::new(1, 2, 0));
    assert!(
        matches!(run(&offered(&feed), &installation), Err(Error::Missing(file)) if file == name)
    );
    assert!(!feed.asked_for("/download/v1.2.0/"));
    assert_eq!(names(&folder), [PROGRAMS[0]]);
}

#[test]
fn an_archive_without_the_program_is_refused() {
    let feed = Feed::start();
    let (folder, installation) = installed("incomplete");
    let name = installation.file(&Version::new(1, 2, 0));
    let packed = archive(&[(&format!("focus-1.2.0/{}", PROGRAMS[1]), b"new server")]);
    feed.publish("1.2.0", &[(&name, &packed)]);
    assert!(matches!(
        run(&offered(&feed), &installation),
        Err(Error::Incomplete(file)) if file == name
    ));
    assert_eq!(
        std::fs::read(folder.join(PROGRAMS[0])).unwrap(),
        b"old program"
    );
    assert_eq!(names(&folder), [PROGRAMS[0]]);
}

#[test]
fn a_cancelled_download_leaves_the_program_as_it_was() {
    let feed = Feed::start();
    let (folder, installation) = installed("cancelled");
    let name = installation.file(&Version::new(1, 2, 0));
    let packed = archive(&[(&format!("focus-1.2.0/{}", PROGRAMS[0]), b"new program")]);
    feed.publish("1.2.0", &[(&name, &packed)]);
    let outcome = install(
        &offered(&feed),
        &installation,
        &AtomicBool::new(true),
        &mut |_| {},
    );
    assert!(matches!(outcome, Err(Error::Cancelled)));
    assert_eq!(
        std::fs::read(folder.join(PROGRAMS[0])).unwrap(),
        b"old program"
    );
    assert_eq!(names(&folder), [PROGRAMS[0]]);
}

#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_written_fails_before_any_download() {
    use std::os::unix::fs::PermissionsExt as _;
    let feed = Feed::start();
    let (folder, installation) = installed("readonly");
    let name = installation.file(&Version::new(1, 2, 0));
    feed.publish("1.2.0", &[(&name, b"x")]);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    let privileged = std::fs::write(folder.join("probe"), b"").is_ok();
    let outcome = run(&offered(&feed), &installation);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    if privileged {
        return;
    }
    assert!(matches!(outcome, Err(Error::Io { path, .. }) if path.starts_with(&folder)));
    assert!(!feed.asked_for("/download/v1.2.0/"));
}

#[cfg(windows)]
#[test]
fn the_program_replaced_is_kept_aside_until_the_next_tidy() {
    let feed = Feed::start();
    let (folder, installation) = installed("aside");
    let name = installation.file(&Version::new(1, 2, 0));
    let packed = archive(&[("focus-1.2.0/focus.exe", b"new program")]);
    feed.publish("1.2.0", &[(&name, &packed)]);
    run(&offered(&feed), &installation).unwrap();
    assert_eq!(
        std::fs::read(folder.join("focus.exe.old")).unwrap(),
        b"old program"
    );
    installation.tidy();
    assert_eq!(names(&folder), ["focus.exe"]);
}
