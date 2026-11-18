use std::sync::OnceLock;

use gpui::SharedString;

#[derive(Clone, Copy)]
pub enum Generic {
    Sans,
    Mono,
}

static SANS: OnceLock<SharedString> = OnceLock::new();
static MONO: OnceLock<SharedString> = OnceLock::new();

pub fn system_family(generic: Generic) -> SharedString {
    let (cell, pattern, embedded) = match generic {
        Generic::Sans => (&SANS, "sans-serif", "Inter"),
        Generic::Mono => (&MONO, "monospace", "JetBrains Mono"),
    };
    cell.get_or_init(|| match system_match(pattern) {
        Ok(family) => family.into(),
        Err(error) => {
            eprintln!("focus: fc-match {pattern}: {error:#}; the embedded {embedded} is used");
            embedded.into()
        }
    })
    .clone()
}

pub fn warm() {
    system_family(Generic::Sans);
    system_family(Generic::Mono);
}

#[cfg(windows)]
fn system_match(pattern: &str) -> anyhow::Result<String> {
    Ok(match pattern {
        "monospace" => "Consolas",
        _ => "Segoe UI",
    }
    .to_string())
}

#[cfg(not(windows))]
fn system_match(pattern: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("fc-match")
        .args(["-f", "%{family[0]}", pattern])
        .output()?;
    anyhow::ensure!(output.status.success(), "exit status {}", output.status);
    let family = String::from_utf8(output.stdout)?.trim().to_string();
    anyhow::ensure!(!family.is_empty(), "no family");
    Ok(family)
}
