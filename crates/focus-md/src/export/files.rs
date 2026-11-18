use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;

#[cfg(not(windows))]
const BROWSERS: [&str; 4] = [
    "chromium",
    "google-chrome-stable",
    "chromium-browser",
    "brave",
];

#[cfg(not(windows))]
fn find_browser() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    BROWSERS.iter().find_map(|name| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(windows)]
fn find_browser() -> Option<PathBuf> {
    const INSTALLS: [&str; 3] = [
        r"Microsoft\Edge\Application\msedge.exe",
        r"Google\Chrome\Application\chrome.exe",
        r"BraveSoftware\Brave-Browser\Application\brave.exe",
    ];
    let roots: Vec<PathBuf> = ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"]
        .iter()
        .filter_map(|name| std::env::var_os(name).map(PathBuf::from))
        .collect();
    INSTALLS.iter().find_map(|install| {
        roots
            .iter()
            .map(|root| root.join(install))
            .find(|candidate| candidate.is_file())
    })
}

fn file_url(page: &Path) -> String {
    if cfg!(windows) {
        format!("file:///{}", page.display().to_string().replace('\\', "/"))
    } else {
        format!("file://{}", page.display())
    }
}

pub(super) fn pdf_available() -> bool {
    find_browser().is_some()
}

const PRINT_TIMEOUT: Duration = Duration::from_secs(120);

struct PrivateDir(PathBuf);

impl PrivateDir {
    fn new(parent: &Path) -> anyhow::Result<PrivateDir> {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let path = parent.join(format!(
            ".focus-print-{}-{nanos}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        #[cfg(unix)]
        let folder = {
            let mut folder = std::fs::DirBuilder::new();
            std::os::unix::fs::DirBuilderExt::mode(&mut folder, 0o700);
            folder
        };
        #[cfg(not(unix))]
        let folder = std::fs::DirBuilder::new();
        folder
            .create(&path)
            .with_context(|| format!("creating {}", path.display()))?;
        Ok(PrivateDir(path))
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const PRINT_POLICY: &str = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:\">";

fn with_print_policy(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let after = |tag: &str| {
        lower
            .find(tag)
            .and_then(|start| lower[start..].find('>').map(|end| start + end + 1))
    };
    let at = after("<head").or_else(|| after("<html")).unwrap_or(0);
    format!("{}{PRINT_POLICY}{}", &html[..at], &html[at..])
}

fn print_flags(profile: &Path, pdf: &Path, sandbox: bool) -> Vec<String> {
    let mut flags: Vec<String> = [
        "--headless=new",
        "--disable-gpu",
        "--no-pdf-header-footer",
        "--hide-scrollbars",
        "--proxy-server=http://127.0.0.1:1",
        "--proxy-bypass-list=<-loopback>",
        "--host-resolver-rules=MAP * ~NOTFOUND",
        "--disable-extensions",
        "--disable-sync",
        "--disable-component-update",
        "--disable-background-networking",
        "--disable-breakpad",
        "--no-first-run",
        "--no-default-browser-check",
    ]
    .map(String::from)
    .to_vec();
    if !sandbox {
        flags.push("--no-sandbox".into());
    }
    flags.push(format!("--user-data-dir={}", profile.display()));
    flags.push(format!("--print-to-pdf={}", pdf.display()));
    flags
}

fn run_browser(
    browser: &Path,
    flags: &[String],
    page: &Path,
    log: &Path,
) -> anyhow::Result<String> {
    let stderr = std::fs::File::create(log)?;
    let mut child = Command::new(browser)
        .args(flags)
        .arg(file_url(page))
        .env_remove("DEEPSEEK_API_KEY")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .spawn()
        .with_context(|| format!("starting {}", browser.display()))?;
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if started.elapsed() > PRINT_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "{} took more than {} s to print",
                browser.display(),
                PRINT_TIMEOUT.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(std::fs::read_to_string(log).unwrap_or_default())
}

pub(super) fn html_to_pdf(html_file: &Path, pdf_file: &Path) -> anyhow::Result<bool> {
    let Some(browser) = find_browser() else {
        return Ok(false);
    };
    let html_file = dunce::canonicalize(html_file)
        .with_context(|| format!("page not found: {}", html_file.display()))?;
    let parent = pdf_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let parent = dunce::canonicalize(parent)?;
    let pdf_target = parent.join(pdf_file.file_name().context("missing PDF file name")?);
    anyhow::ensure!(
        !pdf_target.exists(),
        "{} already exists",
        pdf_target.display()
    );
    let source = std::fs::read_to_string(&html_file)?;
    let opened = with_print_policy(
        &source
            .replace("<details", "<details open")
            .replace("<details open open", "<details open"),
    );
    let work = PrivateDir::new(&parent)?;
    let page = work.0.join("page.html");
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        options.open(&page)?.write_all(opened.as_bytes())?;
    }
    let printed = work.0.join("out.pdf");
    let log = work.0.join("stderr.txt");
    let mut stderr = run_browser(
        &browser,
        &print_flags(&work.0.join("profile"), &printed, true),
        &page,
        &log,
    )?;
    if !printed.is_file() && (stderr.contains("sandbox") || stderr.contains("Running as root")) {
        stderr = run_browser(
            &browser,
            &print_flags(&work.0.join("profile-2"), &printed, false),
            &page,
            &log,
        )?;
    }
    if !printed.is_file() {
        anyhow::bail!(
            "{} did not produce a PDF: {}",
            browser.display(),
            stderr.lines().last().unwrap_or("")
        );
    }
    std::fs::rename(&printed, &pdf_target)
        .with_context(|| format!("writing {}", pdf_target.display()))?;
    Ok(true)
}

pub(super) fn zip(files: &[(PathBuf, PathBuf)], destination: &Path) -> anyhow::Result<u64> {
    if let Some(parent) = destination.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(destination)
        .with_context(|| format!("creating {}", destination.display()))?;
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (source, name) in files {
        let name = name
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let bytes =
            std::fs::read(source).with_context(|| format!("reading {}", source.display()))?;
        archive.start_file(name, options)?;
        archive.write_all(&bytes)?;
    }
    archive.finish()?;
    Ok(std::fs::metadata(destination)?.len())
}

#[cfg(test)]
mod tests {
    use super::with_print_policy;

    #[test]
    fn the_policy_comes_before_any_script() {
        let page =
            "<!doctype html><HTML><HEAD lang=x><script>1</script></HEAD><body></body></HTML>";
        let out = with_print_policy(page);
        assert!(out.find("Content-Security-Policy").unwrap() < out.find("<script>").unwrap());
        assert!(
            out.starts_with("<!doctype html><HTML><HEAD lang=x><meta"),
            "{out}"
        );
        assert!(with_print_policy("<p>x</p>").starts_with("<meta"));
    }
}
