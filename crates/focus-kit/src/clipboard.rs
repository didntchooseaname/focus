use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use gpui::{App, AsyncApp, ClipboardEntry, ClipboardItem, Image, ImageFormat};

pub use crate::file_list::ClipboardFiles;
use crate::file_list::{self, GNOME_FILES, KDE_CUT, Offer, URI_LIST};

const TIMEOUT: Duration = Duration::from_secs(3);
const MAX_IMAGE: usize = 50 << 20;
const MAX_TEXT: usize = 16 << 20;
const MAX_STDERR: usize = 4096;

const TEXT_TYPES: [&str; 5] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
    "TEXT",
];
const IMAGE_TYPES: [(&str, &str); 5] = [
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
    ("image/gif", "gif"),
    ("image/bmp", "bmp"),
];

#[derive(Debug)]
pub enum Error {
    NoTool,
    Start { tool: String, source: io::Error },
    Timeout { tool: String },
    TooLarge { limit: usize },
    Failed { tool: String, detail: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoTool => write!(f, "no clipboard tool (wl-copy and wl-paste, or xclip)"),
            Error::Start { tool, source } => write!(f, "{tool} did not start: {source}"),
            Error::Timeout { tool } => write!(f, "{tool} did not answer in time"),
            Error::TooLarge { limit } => write!(f, "clipboard content over {limit} bytes"),
            Error::Failed { tool, detail } => write!(f, "{tool} failed: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
}

#[derive(Debug, Clone)]
enum Backend {
    Wayland { copy: PathBuf, paste: PathBuf },
    Xclip(PathBuf),
}

type Provider = fn(&Offer) -> Result<(), Error>;

#[derive(Debug, Clone)]
pub struct Tools {
    backend: Option<Backend>,
    timeout: Duration,
    max_image: usize,
    provider: Option<Provider>,
}

fn find(path: &[PathBuf], name: &str) -> Option<PathBuf> {
    path.iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

struct Output {
    success: bool,
    stdout: Vec<u8>,
    stderr: String,
}

impl Tools {
    pub fn detect() -> Tools {
        let path: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        let present = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty());
        #[cfg_attr(not(all(unix, not(target_os = "macos"))), expect(unused_mut))]
        let mut tools = Tools::in_path(&path, present("WAYLAND_DISPLAY"), present("DISPLAY"));
        #[cfg(all(unix, not(target_os = "macos")))]
        if matches!(tools.backend, Some(Backend::Wayland { .. })) {
            tools.provider = Some(serve_on_wayland);
        } else if matches!(tools.backend, Some(Backend::Xclip(_))) {
            tools.provider = Some(crate::x11_selection::serve);
        }
        tools
    }

    pub fn in_path(path: &[PathBuf], wayland: bool, x11: bool) -> Tools {
        let wl = if wayland {
            find(path, "wl-copy").zip(find(path, "wl-paste"))
        } else {
            None
        };
        let backend = match wl {
            Some((copy, paste)) => Some(Backend::Wayland { copy, paste }),
            None if wayland || x11 => find(path, "xclip").filter(|_| x11).map(Backend::Xclip),
            None => None,
        };
        Tools {
            backend,
            timeout: TIMEOUT,
            max_image: MAX_IMAGE,
            provider: None,
        }
    }

    pub fn is_available(&self) -> bool {
        self.backend.is_some()
    }

    pub fn write_text(&self, text: &str) -> Result<(), Error> {
        let mime = "text/plain;charset=utf-8";
        match self.backend.as_ref().ok_or(Error::NoTool)? {
            Backend::Wayland { copy, .. } => self.store(copy, &["--type", mime], text.as_bytes()),
            Backend::Xclip(xclip) => {
                self.store(xclip, &["-selection", "clipboard", "-in"], text.as_bytes())
            }
        }
    }

    pub fn write_image(&self, png: &[u8]) -> Result<(), Error> {
        match self.backend.as_ref().ok_or(Error::NoTool)? {
            Backend::Wayland { copy, .. } => self.store(copy, &["--type", "image/png"], png),
            Backend::Xclip(xclip) => self.store(
                xclip,
                &["-selection", "clipboard", "-t", "image/png", "-in"],
                png,
            ),
        }
    }

    pub fn write_files(&self, paths: &[PathBuf], cut: bool) -> Result<(), Error> {
        let offer = Offer::new(paths, cut).ok_or_else(|| Error::Failed {
            tool: "clipboard".into(),
            detail: "paths must be absolute".into(),
        })?;
        let backend = self.backend.as_ref().ok_or(Error::NoTool)?;
        if let Some(provider) = self.provider {
            match provider(&offer) {
                Ok(()) => return Ok(()),
                Err(error) => report(&error),
            }
        }
        let (mime, body) = offer.single();
        match backend {
            Backend::Wayland { copy, .. } => self.store(copy, &["--type", mime], body),
            Backend::Xclip(xclip) => {
                self.store(xclip, &["-selection", "clipboard", "-t", mime, "-in"], body)
            }
        }
    }

    pub fn read_files(&self) -> Result<Option<ClipboardFiles>, Error> {
        let Some(mime) = self.first_type(&[GNOME_FILES, URI_LIST])? else {
            return Ok(None);
        };
        let text = String::from_utf8_lossy(&self.fetch(mime, MAX_TEXT)?).into_owned();
        let files = if mime == GNOME_FILES {
            file_list::parse_gnome(&text)
        } else {
            let cut = match self.first_type(&[KDE_CUT])? {
                Some(marker) => self.fetch(marker, 16)?.first() == Some(&b'1'),
                None => false,
            };
            file_list::parse_uri_list(&text, cut)
        };
        Ok((!files.paths.is_empty() || files.refused > 0).then_some(files))
    }

    pub fn offers_files(&self) -> Result<bool, Error> {
        Ok(self.first_type(&[GNOME_FILES, URI_LIST])?.is_some())
    }

    pub fn read_text(&self) -> Result<Option<String>, Error> {
        let Some(mime) = self.first_type(&TEXT_TYPES)? else {
            return Ok(None);
        };
        let bytes = self.fetch(mime, MAX_TEXT)?;
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }

    pub fn read_image(&self) -> Result<Option<ClipboardImage>, Error> {
        let Some(mime) = self.first_type(&IMAGE_TYPES.map(|(mime, _)| mime))? else {
            return Ok(None);
        };
        let extension = IMAGE_TYPES
            .iter()
            .find(|(m, _)| *m == mime)
            .map_or("png", |(_, e)| e);
        Ok(Some(ClipboardImage {
            bytes: self.fetch(mime, self.max_image)?,
            extension,
        }))
    }

    fn store(&self, tool: &Path, args: &[&str], data: &[u8]) -> Result<(), Error> {
        let output = self.run(tool, args, Some(data), None)?;
        if output.success {
            Ok(())
        } else {
            Err(failed(tool, "non-zero exit status"))
        }
    }

    fn first_type(&self, wanted: &[&'static str]) -> Result<Option<&'static str>, Error> {
        let (tool, args): (&Path, &[&str]) = match self.backend.as_ref().ok_or(Error::NoTool)? {
            Backend::Wayland { paste, .. } => (paste, &["--list-types"]),
            Backend::Xclip(xclip) => (xclip, &["-selection", "clipboard", "-t", "TARGETS", "-o"]),
        };
        let output = self.run(tool, args, None, Some(MAX_STDERR << 4))?;
        if !output.success {
            let said = output.stderr.to_lowercase();
            if said.contains("nothing is copied")
                || said.contains("no selection")
                || said.contains("not available")
            {
                return Ok(None);
            }
            return Err(failed(tool, output.stderr.trim()));
        }
        let listed = String::from_utf8_lossy(&output.stdout);
        let offered: Vec<&str> = listed.lines().map(str::trim).collect();
        Ok(wanted
            .iter()
            .copied()
            .find(|mime| offered.iter().any(|o| o.eq_ignore_ascii_case(mime))))
    }

    fn fetch(&self, mime: &str, limit: usize) -> Result<Vec<u8>, Error> {
        let (tool, args): (&Path, Vec<&str>) = match self.backend.as_ref().ok_or(Error::NoTool)? {
            Backend::Wayland { paste, .. } => (paste, vec!["--no-newline", "--type", mime]),
            Backend::Xclip(xclip) => (xclip, vec!["-selection", "clipboard", "-t", mime, "-o"]),
        };
        let output = self.run(tool, &args, None, Some(limit))?;
        if output.success {
            Ok(output.stdout)
        } else {
            Err(failed(tool, output.stderr.trim()))
        }
    }

    fn run(
        &self,
        tool: &Path,
        args: &[&str],
        input: Option<&[u8]>,
        capture: Option<usize>,
    ) -> Result<Output, Error> {
        let name = tool
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut command = Command::new(tool);
        command
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(if capture.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stderr(if capture.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        let mut child = command.spawn().map_err(|source| Error::Start {
            tool: name.clone(),
            source,
        })?;
        if let (Some(data), Some(mut stdin)) = (input, child.stdin.take()) {
            let data = data.to_vec();
            thread::spawn(move || stdin.write_all(&data).ok());
        }
        let over = Arc::new(AtomicBool::new(false));
        let limit = capture.unwrap_or(0);
        let out = child.stdout.take().map(|stream| {
            let over = over.clone();
            thread::spawn(move || {
                let mut buffer = Vec::new();
                stream.take(limit as u64 + 1).read_to_end(&mut buffer).ok();
                over.store(buffer.len() > limit, Ordering::SeqCst);
                buffer
            })
        });
        let err = child.stderr.take().map(|stream| {
            thread::spawn(move || {
                let mut buffer = Vec::new();
                stream.take(MAX_STDERR as u64).read_to_end(&mut buffer).ok();
                buffer
            })
        });
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            if over.load(Ordering::SeqCst) {
                child.kill().ok();
                child.wait().ok();
                return Err(Error::TooLarge { limit });
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
                Ok(None) => {
                    child.kill().ok();
                    child.wait().ok();
                    return Err(Error::Timeout { tool: name });
                }
                Err(source) => return Err(Error::Start { tool: name, source }),
            }
        };
        let stdout = out
            .map(|handle| handle.join().unwrap_or_default())
            .unwrap_or_default();
        if stdout.len() > limit && capture.is_some() {
            return Err(Error::TooLarge { limit });
        }
        let stderr = err
            .map(|handle| String::from_utf8_lossy(&handle.join().unwrap_or_default()).into_owned())
            .unwrap_or_default();
        Ok(Output {
            success: status.success(),
            stdout,
            stderr,
        })
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn serve_on_wayland(offer: &Offer) -> Result<(), Error> {
    use wl_clipboard_rs::copy::{MimeSource, MimeType, Options, Source};
    let sources = offer
        .types
        .iter()
        .map(|(mime, body)| MimeSource {
            source: Source::Bytes(body.clone().into_boxed_slice()),
            mime_type: MimeType::Specific((*mime).to_string()),
        })
        .collect();
    Options::new()
        .copy_multi(sources)
        .map_err(|e| Error::Failed {
            tool: "wl-clipboard-rs".into(),
            detail: e.to_string(),
        })
}

fn failed(tool: &Path, detail: &str) -> Error {
    Error::Failed {
        tool: tool
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        detail: detail.to_string(),
    }
}

fn gpui_image(cx: &App) -> Option<ClipboardImage> {
    cx.read_from_clipboard()?
        .entries()
        .iter()
        .find_map(|entry| match entry {
            ClipboardEntry::Image(image) => Some(ClipboardImage {
                bytes: image.bytes.clone(),
                extension: extension(image.format),
            }),
            ClipboardEntry::String(_) => None,
        })
}

fn extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
    }
}

fn report(error: &Error) {
    eprintln!("focus: clipboard: {error}");
}

pub fn copy_text(cx: &mut App, text: String) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
    let tools = Tools::detect();
    if tools.is_available() {
        cx.background_executor()
            .spawn(async move { tools.write_text(&text).map_err(|e| report(&e)).ok() })
            .detach();
    }
}

pub fn copy_files(cx: &mut App, paths: Vec<PathBuf>, cut: bool) {
    let text = paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    let tools = Tools::detect();
    if tools.is_available() {
        cx.background_executor()
            .spawn(async move { tools.write_files(&paths, cut).map_err(|e| report(&e)).ok() })
            .detach();
    }
}

pub async fn read_files(cx: &AsyncApp) -> Result<Option<ClipboardFiles>, Error> {
    let tools = Tools::detect();
    if !tools.is_available() {
        return Ok(None);
    }
    cx.background_executor()
        .spawn(async move { tools.read_files() })
        .await
}

pub async fn offers_files(cx: &AsyncApp) -> Result<Option<bool>, Error> {
    let tools = Tools::detect();
    if !tools.is_available() {
        return Ok(None);
    }
    cx.background_executor()
        .spawn(async move { tools.offers_files().map(Some) })
        .await
}

pub fn copy_image(cx: &mut App, png: Vec<u8>) {
    cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
        ImageFormat::Png,
        png.clone(),
    )));
    let tools = Tools::detect();
    if tools.is_available() {
        cx.background_executor()
            .spawn(async move { tools.write_image(&png).map_err(|e| report(&e)).ok() })
            .detach();
    }
}

pub async fn read_image(cx: &AsyncApp) -> Result<Option<ClipboardImage>, Error> {
    let tools = Tools::detect();
    let failure = if tools.is_available() {
        match cx
            .background_executor()
            .spawn(async move { tools.read_image() })
            .await
        {
            Ok(image) => return Ok(image),
            Err(error) => Some(error),
        }
    } else {
        None
    };
    match cx.update(|cx| gpui_image(cx)).ok().flatten() {
        Some(image) => Ok(Some(image)),
        None => failure.map_or(Ok(None), Err),
    }
}

pub async fn read_text(cx: &AsyncApp) -> Result<Option<String>, Error> {
    let tools = Tools::detect();
    let failure = if tools.is_available() {
        match cx
            .background_executor()
            .spawn(async move { tools.read_text() })
            .await
        {
            Ok(text) => return Ok(text),
            Err(error) => Some(error),
        }
    } else {
        None
    };
    match cx
        .update(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .ok()
        .flatten()
    {
        Some(text) => Ok(Some(text)),
        None => failure.map_or(Ok(None), Err),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;

    use super::*;

    static SERIAL: Mutex<()> = Mutex::new(());

    const WL_COPY: &str =
        "#!/bin/sh\nd=$(dirname \"$0\")\necho \"$@\" > \"$d/copy.args\"\ncat > \"$d/copy.data\"\n";
    const WL_PASTE: &str = "#!/bin/sh
d=$(dirname \"$0\")
echo \"$@\" >> \"$d/paste.args\"
if [ \"$1\" = \"--list-types\" ]; then
  [ -f \"$d/types\" ] || { echo 'Nothing is copied' >&2; exit 1; }
  cat \"$d/types\"; exit 0
fi
[ -f \"$d/fail\" ] && { echo broken >&2; exit 2; }
[ -f \"$d/slow\" ] && sleep 5
[ -f \"$d/big\" ] && { head -c 3000 /dev/zero; exit 0; }
cat \"$d/data.$(echo \"$3\" | tr '/;=' '___')\"
";
    const XCLIP: &str = "#!/bin/sh
d=$(dirname \"$0\")
echo \"$@\" >> \"$d/xclip.args\"
mode=out
while [ $# -gt 0 ]; do
  case \"$1\" in -t) mime=\"$2\";; -in|-i) mode=in;; esac
  shift
done
if [ $mode = in ]; then cat > \"$d/xclip.data\"; exit 0; fi
if [ \"$mime\" = TARGETS ]; then cat \"$d/types\"; exit 0; fi
cat \"$d/data.$(echo \"$mime\" | tr '/;=' '___')\"
";

    fn tools_dir(name: &str, scripts: &[(&str, &str)]) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(format!("clipboard-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for (file, body) in scripts {
            let path = dir.join(file);
            fs::write(&path, body).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    }

    fn wayland(name: &str) -> (Tools, PathBuf, std::sync::MutexGuard<'static, ()>) {
        let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tools_dir(name, &[("wl-copy", WL_COPY), ("wl-paste", WL_PASTE)]);
        (
            Tools::in_path(std::slice::from_ref(&dir), true, false),
            dir,
            guard,
        )
    }

    fn x11(name: &str) -> (Tools, PathBuf, std::sync::MutexGuard<'static, ()>) {
        let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tools_dir(name, &[("xclip", XCLIP)]);
        (
            Tools::in_path(std::slice::from_ref(&dir), false, true),
            dir,
            guard,
        )
    }

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0, 1, 2, 0, 255, 10, 13];

    #[test]
    fn the_tools_follow_the_session() {
        let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let both = tools_dir(
            "order-both",
            &[
                ("wl-copy", WL_COPY),
                ("wl-paste", WL_PASTE),
                ("xclip", XCLIP),
            ],
        );
        let only_wl_copy = tools_dir("order-half", &[("wl-copy", WL_COPY), ("xclip", XCLIP)]);
        let path = |dir: &PathBuf| vec![dir.clone()];
        assert!(matches!(
            Tools::in_path(&path(&both), true, true).backend,
            Some(Backend::Wayland { .. })
        ));
        assert!(matches!(
            Tools::in_path(&path(&both), false, true).backend,
            Some(Backend::Xclip(_))
        ));
        assert!(matches!(
            Tools::in_path(&path(&only_wl_copy), true, true).backend,
            Some(Backend::Xclip(_))
        ));
        assert!(
            Tools::in_path(&path(&only_wl_copy), true, false)
                .backend
                .is_none()
        );
        assert!(Tools::in_path(&path(&both), false, false).backend.is_none());
        assert!(matches!(
            Tools::in_path(&[], true, true).write_text("x"),
            Err(Error::NoTool)
        ));
    }

    #[test]
    fn wl_copy_gets_the_text_on_stdin_with_an_explicit_type() {
        let (tools, dir, _guard) = wayland("wl-text");
        let text = "line one\n$(touch /tmp/never) \"quoted\" caf\u{e9}\n";
        tools.write_text(text).unwrap();
        assert_eq!(fs::read_to_string(dir.join("copy.data")).unwrap(), text);
        assert_eq!(
            fs::read_to_string(dir.join("copy.args")).unwrap().trim(),
            "--type text/plain;charset=utf-8"
        );
    }

    #[test]
    fn wl_copy_gets_an_image_as_png() {
        let (tools, dir, _guard) = wayland("wl-image-write");
        tools.write_image(PNG).unwrap();
        assert_eq!(fs::read(dir.join("copy.data")).unwrap(), PNG);
        assert_eq!(
            fs::read_to_string(dir.join("copy.args")).unwrap().trim(),
            "--type image/png"
        );
    }

    #[test]
    fn wl_paste_reads_the_best_image_type_it_lists() {
        let (tools, dir, _guard) = wayland("wl-image-read");
        fs::write(dir.join("types"), "text/plain\nimage/jpeg\nimage/png\n").unwrap();
        fs::write(dir.join("data.image_png"), PNG).unwrap();
        let image = tools.read_image().unwrap().unwrap();
        assert_eq!((image.bytes.as_slice(), image.extension), (PNG, "png"));
        let asked = fs::read_to_string(dir.join("paste.args")).unwrap();
        assert!(asked.contains("--no-newline --type image/png"), "{asked}");
        fs::write(dir.join("types"), "text/plain\nimage/jpeg\n").unwrap();
        fs::write(dir.join("data.image_jpeg"), b"jpeg bytes").unwrap();
        assert_eq!(tools.read_image().unwrap().unwrap().extension, "jpg");
    }

    #[test]
    fn wl_paste_reads_text_by_its_preferred_type() {
        let (tools, dir, _guard) = wayland("wl-text-read");
        fs::write(
            dir.join("types"),
            "TEXT\ntext/plain\ntext/plain;charset=utf-8\n",
        )
        .unwrap();
        fs::write(
            dir.join("data.text_plain_charset_utf-8"),
            "h\u{e9}llo\nworld",
        )
        .unwrap();
        assert_eq!(
            tools.read_text().unwrap().as_deref(),
            Some("h\u{e9}llo\nworld")
        );
    }

    #[test]
    fn an_empty_clipboard_or_the_wrong_kind_is_none_not_an_error() {
        let (tools, dir, _guard) = wayland("wl-none");
        assert_eq!(tools.read_image().unwrap(), None);
        assert_eq!(tools.read_text().unwrap(), None);
        fs::write(dir.join("types"), "text/plain\n").unwrap();
        assert_eq!(tools.read_image().unwrap(), None);
    }

    #[test]
    fn a_failing_tool_is_an_error_that_carries_what_it_said() {
        let (tools, dir, _guard) = wayland("wl-fail");
        fs::write(dir.join("types"), "image/png\n").unwrap();
        fs::write(dir.join("fail"), "").unwrap();
        match tools.read_image() {
            Err(Error::Failed { tool, detail }) => {
                assert_eq!((tool.as_str(), detail.as_str()), ("wl-paste", "broken"))
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_tool_that_hangs_is_stopped() {
        let (mut tools, dir, _guard) = wayland("wl-slow");
        tools.timeout = Duration::from_millis(300);
        fs::write(dir.join("types"), "image/png\n").unwrap();
        fs::write(dir.join("slow"), "").unwrap();
        let started = Instant::now();
        assert!(matches!(tools.read_image(), Err(Error::Timeout { .. })));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn an_image_over_the_limit_is_refused() {
        let (mut tools, dir, _guard) = wayland("wl-big");
        tools.max_image = 1000;
        fs::write(dir.join("types"), "image/png\n").unwrap();
        fs::write(dir.join("big"), "").unwrap();
        assert!(matches!(
            tools.read_image(),
            Err(Error::TooLarge { limit: 1000 })
        ));
    }

    #[test]
    fn xclip_is_asked_for_targets_then_for_the_type() {
        let (tools, dir, _guard) = x11("xclip-read");
        fs::write(dir.join("types"), "TARGETS\nimage/png\nUTF8_STRING\n").unwrap();
        fs::write(dir.join("data.image_png"), PNG).unwrap();
        fs::write(dir.join("data.UTF8_STRING"), "text").unwrap();
        assert_eq!(tools.read_image().unwrap().unwrap().bytes, PNG);
        assert_eq!(tools.read_text().unwrap().as_deref(), Some("text"));
        let asked = fs::read_to_string(dir.join("xclip.args")).unwrap();
        assert!(
            asked.contains("-selection clipboard -t TARGETS -o"),
            "{asked}"
        );
        assert!(
            asked.contains("-selection clipboard -t image/png -o"),
            "{asked}"
        );
    }

    #[test]
    fn xclip_gets_text_and_images_on_stdin() {
        let (tools, dir, _guard) = x11("xclip-write");
        tools.write_text("copied").unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("xclip.data")).unwrap(),
            "copied"
        );
        tools.write_image(PNG).unwrap();
        assert_eq!(fs::read(dir.join("xclip.data")).unwrap(), PNG);
        let asked = fs::read_to_string(dir.join("xclip.args")).unwrap();
        assert!(asked.contains("-selection clipboard -in"), "{asked}");
        assert!(
            asked.contains("-selection clipboard -t image/png -in"),
            "{asked}"
        );
    }

    #[test]
    fn wl_copy_gets_the_one_type_that_says_copy_or_cut() {
        let (tools, dir, _guard) = wayland("wl-files-write");
        let paths = vec![PathBuf::from("/v/a b#.md"), PathBuf::from("/v/caf\u{e9}")];
        tools.write_files(&paths, true).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("copy.args")).unwrap().trim(),
            "--type x-special/gnome-copied-files"
        );
        assert_eq!(
            fs::read_to_string(dir.join("copy.data")).unwrap(),
            "cut\nfile:///v/a%20b%23.md\nfile:///v/caf%C3%A9"
        );
        tools.write_files(&paths, false).unwrap();
        assert!(
            fs::read_to_string(dir.join("copy.data"))
                .unwrap()
                .starts_with("copy\n")
        );
        assert!(matches!(
            tools.write_files(&[PathBuf::from("relative")], false),
            Err(Error::Failed { .. })
        ));
    }

    static OFFERED: Mutex<Vec<(String, Vec<u8>)>> = Mutex::new(Vec::new());

    fn record(offer: &Offer) -> Result<(), Error> {
        *OFFERED.lock().unwrap() = offer
            .types
            .iter()
            .map(|(m, b)| (m.to_string(), b.clone()))
            .collect();
        Ok(())
    }

    fn refuse(_: &Offer) -> Result<(), Error> {
        Err(Error::Failed {
            tool: "provider".into(),
            detail: "no data control".into(),
        })
    }

    #[test]
    fn a_provider_gets_every_type_and_a_failing_one_falls_back_to_the_tool() {
        let (mut tools, dir, _guard) = wayland("wl-files-provider");
        tools.provider = Some(record);
        tools
            .write_files(&[PathBuf::from("/v/x y.md")], false)
            .unwrap();
        let kinds: Vec<String> = OFFERED
            .lock()
            .unwrap()
            .iter()
            .map(|(m, _)| m.clone())
            .collect();
        assert_eq!(
            kinds,
            [
                "text/uri-list",
                "x-special/gnome-copied-files",
                "text/plain;charset=utf-8"
            ]
        );
        assert!(!dir.join("copy.args").exists(), "the tool was not needed");
        tools.provider = Some(refuse);
        tools
            .write_files(&[PathBuf::from("/v/x y.md")], false)
            .unwrap();
        assert!(
            dir.join("copy.data").exists(),
            "the single type went through wl-copy"
        );
    }

    #[test]
    fn xclip_gets_the_gnome_type_on_stdin() {
        let (tools, dir, _guard) = x11("xclip-files-write");
        tools
            .write_files(&[PathBuf::from("/v/a.md")], false)
            .unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("xclip.data")).unwrap(),
            "copy\nfile:///v/a.md"
        );
        assert!(
            fs::read_to_string(dir.join("xclip.args"))
                .unwrap()
                .contains("-selection clipboard -t x-special/gnome-copied-files -in")
        );
    }

    #[test]
    fn files_are_read_from_the_gnome_type_with_cut_and_remote_refusals() {
        let (tools, dir, _guard) = wayland("wl-files-read");
        fs::write(
            dir.join("types"),
            "text/plain\ntext/uri-list\nx-special/gnome-copied-files\n",
        )
        .unwrap();
        fs::write(
            dir.join("data.x-special_gnome-copied-files"),
            "cut\nfile:///v/a%20b.md\nsftp://h/x.md\nfile://localhost/v/%23c",
        )
        .unwrap();
        let files = tools.read_files().unwrap().unwrap();
        assert_eq!(
            (files.paths, files.cut, files.refused),
            (
                vec![PathBuf::from("/v/a b.md"), PathBuf::from("/v/#c")],
                true,
                1
            )
        );
    }

    #[test]
    fn a_plain_uri_list_is_a_copy_and_the_kde_marker_makes_it_a_cut() {
        let (tools, dir, _guard) = wayland("wl-files-uri");
        fs::write(dir.join("types"), "text/uri-list\n").unwrap();
        fs::write(
            dir.join("data.text_uri-list"),
            "file:///v/a.md\r\nfile:///v/b.md\r\n",
        )
        .unwrap();
        let files = tools.read_files().unwrap().unwrap();
        assert_eq!((files.paths.len(), files.cut), (2, false));
        fs::write(
            dir.join("types"),
            "text/uri-list\napplication/x-kde-cutselection\n",
        )
        .unwrap();
        fs::write(dir.join("data.application_x-kde-cutselection"), "1").unwrap();
        assert!(tools.read_files().unwrap().unwrap().cut);
    }

    #[test]
    fn only_remote_addresses_are_reported_as_refused_and_no_list_is_none() {
        let (tools, dir, _guard) = wayland("wl-files-remote");
        assert_eq!(tools.read_files().unwrap(), None);
        fs::write(dir.join("types"), "text/plain\n").unwrap();
        assert_eq!(tools.read_files().unwrap(), None);
        fs::write(dir.join("types"), "text/uri-list\n").unwrap();
        fs::write(dir.join("data.text_uri-list"), "smb://srv/share/a.md\n").unwrap();
        let files = tools.read_files().unwrap().unwrap();
        assert_eq!((files.paths.len(), files.refused), (0, 1));
    }

    #[test]
    fn the_offer_of_thunar_is_files() {
        let (tools, dir, _guard) = wayland("wl-thunar");
        assert!(!tools.offers_files().unwrap());
        fs::write(
            dir.join("types"),
            "text/uri-list\nx-special/gnome-copied-files\ntext/plain;charset=utf-8\nUTF8_STRING\n",
        )
        .unwrap();
        fs::write(
            dir.join("data.text_uri-list"),
            "file:///home/u/Scope%20notes%20%C3%A9.md\r\nfile:///home/u/Evidence%20dir\r\n",
        )
        .unwrap();
        fs::write(
            dir.join("data.x-special_gnome-copied-files"),
            "copy\nfile:///home/u/Scope%20notes%20%C3%A9.md\nfile:///home/u/Evidence%20dir",
        )
        .unwrap();
        assert!(tools.offers_files().unwrap());
        let files = tools.read_files().unwrap().unwrap();
        assert_eq!(
            (files.paths, files.cut, files.refused),
            (
                vec![
                    PathBuf::from("/home/u/Scope notes \u{e9}.md"),
                    PathBuf::from("/home/u/Evidence dir")
                ],
                false,
                0
            )
        );
        fs::write(dir.join("types"), "text/plain;charset=utf-8\nUTF8_STRING\n").unwrap();
        assert!(!tools.offers_files().unwrap());
    }

    #[test]
    fn xclip_reads_files_through_targets() {
        let (tools, dir, _guard) = x11("xclip-files-read");
        fs::write(dir.join("types"), "TARGETS\nx-special/gnome-copied-files\n").unwrap();
        fs::write(
            dir.join("data.x-special_gnome-copied-files"),
            "copy\nfile:///v/a.md",
        )
        .unwrap();
        assert_eq!(
            tools.read_files().unwrap().unwrap().paths,
            vec![PathBuf::from("/v/a.md")]
        );
    }
}
