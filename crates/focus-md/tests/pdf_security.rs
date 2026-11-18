use std::io::Read as _;
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;

use focus_md::export::{html_to_pdf, pdf_available};

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp/pdf-security")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_page_cannot_reach_the_network_while_it_is_printed() {
    if !pdf_available() {
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let dir = scratch("network");
    let page = dir.join("page.html");
    let hostile = format!(
        "<!doctype html><html><head><link rel=\"stylesheet\" href=\"http://127.0.0.1:{port}/css\"></head><body><h1>Printed</h1>\
         <img src=\"http://127.0.0.1:{port}/img\"><iframe src=\"http://localhost:{port}/frame\"></iframe>\
         <script>fetch('http://127.0.0.1:{port}/fetch');new Image().src='http://127.0.0.1:{port}/script';</script></body></html>"
    );
    std::fs::write(&page, hostile).unwrap();
    let pdf = dir.join("out.pdf");
    assert!(html_to_pdf(&page, &pdf).unwrap());
    assert!(pdf.metadata().unwrap().len() > 500, "a PDF was written");
    std::thread::sleep(Duration::from_millis(300));
    let mut seen = Vec::new();
    while let Ok((mut stream, _)) = listener.accept() {
        let mut first = [0u8; 256];
        let n = stream.read(&mut first).unwrap_or(0);
        seen.push(
            String::from_utf8_lossy(&first[..n])
                .lines()
                .next()
                .unwrap_or("")
                .to_string(),
        );
    }
    assert!(seen.is_empty(), "the browser connected: {seen:?}");
    let left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        left.iter().all(|n| !n.starts_with(".focus-print")),
        "temporary files left: {left:?}"
    );
}

#[test]
fn an_existing_file_is_not_overwritten() {
    if !pdf_available() {
        return;
    }
    let dir = scratch("overwrite");
    let page = dir.join("page.html");
    std::fs::write(&page, "<p>x</p>").unwrap();
    let pdf = dir.join("out.pdf");
    std::fs::write(&pdf, "precious").unwrap();
    assert!(html_to_pdf(&page, &pdf).is_err());
    assert_eq!(std::fs::read_to_string(&pdf).unwrap(), "precious");
}

#[test]
fn a_planted_symbolic_link_is_not_followed_for_the_temporary_page() {
    if !pdf_available() {
        return;
    }
    let dir = scratch("symlink");
    let victim = dir.join("victim.txt");
    std::fs::write(&victim, "keep").unwrap();
    let planted = dir.join(format!(".focus-print-{}.html", std::process::id()));
    std::os::unix::fs::symlink(&victim, &planted).unwrap();
    let page = dir.join("page.html");
    std::fs::write(&page, "<p>x</p>").unwrap();
    let pdf = dir.join("out.pdf");
    assert!(html_to_pdf(&page, &pdf).unwrap());
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
}

#[test]
fn a_script_does_not_run_while_the_page_is_printed() {
    if !pdf_available() {
        return;
    }
    let dir = scratch("script");
    let page = dir.join("page.html");
    std::fs::write(&page, "<!doctype html><html><head><title>NOSCRIPT</title></head><body><p>x</p><script>document.title='RAN'</script></body></html>").unwrap();
    let pdf = dir.join("out.pdf");
    assert!(html_to_pdf(&page, &pdf).unwrap());
    let bytes = std::fs::read(&pdf).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("/Title (NOSCRIPT)") && !text.contains("/Title (RAN)"),
        "{:?}",
        text.lines().find(|l| l.contains("/Title"))
    );
}
