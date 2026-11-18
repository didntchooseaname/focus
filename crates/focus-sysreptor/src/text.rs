use std::path::Path;
use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::images::{Images, Outcome};

static EMBED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!\[\[([^\]|]+?)(?:\|([^\]]*))?\]\]").expect("constant pattern"));
static IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)"#).expect("constant pattern")
});
static WIKILINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|([^\]]*))?\]\]").expect("constant pattern")
});

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Report {
    pub(crate) missing_images: Vec<String>,
    pub(crate) rejected_images: Vec<(String, String)>,
    pub(crate) remote_images: usize,
    pub(crate) note_embeds: Vec<String>,
    pub(crate) comments: usize,
}

impl Report {
    pub(crate) fn merge(&mut self, other: Report) {
        self.missing_images.extend(other.missing_images);
        self.rejected_images.extend(other.rejected_images);
        self.remote_images += other.remote_images;
        self.note_embeds.extend(other.note_embeds);
        self.comments += other.comments;
    }
}

fn image_markdown(alt: &str, name: &str) -> String {
    format!("![{alt}](/images/name/{name})")
}

fn stem(target: &str) -> String {
    Path::new(target)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn is_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

fn without_comments(text: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut removed = 0;
    let mut open = false;
    let mut fenced = false;
    for line in text.split_inclusive('\n') {
        if !open && is_fence(line) {
            fenced = !fenced;
        }
        if fenced {
            out.push_str(line);
            continue;
        }
        let mut rest = line;
        let mut kept = String::new();
        while !rest.is_empty() {
            match rest.find("%%") {
                Some(at) if !open => {
                    kept.push_str(&rest[..at]);
                    rest = &rest[at + 2..];
                    open = true;
                    removed += 1;
                }
                Some(at) => {
                    rest = &rest[at + 2..];
                    open = false;
                }
                None => {
                    if !open {
                        kept.push_str(rest);
                    }
                    break;
                }
            }
        }
        if !(kept.trim().is_empty() && line.contains("%%")) {
            out.push_str(&kept);
        }
    }
    (out, removed)
}

fn convert_line(line: &str, images: &mut Images, note_dir: &Path, report: &mut Report) -> String {
    let embedded = EMBED.replace_all(line, |caps: &Captures| {
        let target = caps[1].trim();
        let label = caps
            .get(2)
            .map(|m| m.as_str().trim())
            .filter(|l| !l.is_empty() && !l.chars().all(|c| c.is_ascii_digit() || c == 'x'));
        match images.resolve(target, note_dir, true) {
            Outcome::Attached(name) => image_markdown(label.unwrap_or(&stem(target)), &name),
            Outcome::NotFound => {
                report.missing_images.push(target.to_string());
                String::new()
            }
            Outcome::Rejected(reason) => {
                report.rejected_images.push((target.to_string(), reason));
                String::new()
            }
            Outcome::Remote => {
                report.remote_images += 1;
                caps[0].to_string()
            }
            Outcome::NoteEmbed => {
                report.note_embeds.push(target.to_string());
                String::new()
            }
        }
    });
    let linked = IMAGE.replace_all(&embedded, |caps: &Captures| {
        let (alt, target) = (&caps[1], &caps[2]);
        if target.starts_with("/images/name/") {
            return caps[0].to_string();
        }
        match images.resolve(target, note_dir, false) {
            Outcome::Attached(name) => image_markdown(alt, &name),
            Outcome::NotFound => {
                report.missing_images.push(target.to_string());
                String::new()
            }
            Outcome::Rejected(reason) => {
                report.rejected_images.push((target.to_string(), reason));
                String::new()
            }
            Outcome::Remote => {
                report.remote_images += 1;
                caps[0].to_string()
            }
            Outcome::NoteEmbed => String::new(),
        }
    });
    WIKILINK
        .replace_all(&linked, |caps: &Captures| {
            caps.get(2)
                .map(|m| m.as_str().trim())
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| caps[1].trim())
                .to_string()
        })
        .into_owned()
}

pub(crate) fn convert(text: &str, images: &mut Images, note_dir: &Path) -> (String, Report) {
    let (text, comments) = without_comments(text);
    let mut report = Report {
        comments,
        ..Report::default()
    };
    let mut out = String::with_capacity(text.len());
    let mut fenced = false;
    for line in text.split_inclusive('\n') {
        if is_fence(line) {
            fenced = !fenced;
            out.push_str(line);
        } else if fenced {
            out.push_str(line);
        } else {
            out.push_str(&convert_line(line, images, note_dir, &mut report));
        }
    }
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engagement(name: &str) -> std::path::PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(format!("sysreptor-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Evidence")).unwrap();
        std::fs::create_dir_all(dir.join("Findings")).unwrap();
        dir
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest";

    #[test]
    fn comments_are_removed_outside_code_and_wiki_links_become_text() {
        let dir = engagement("text-comments");
        let mut images = Images::new(&dir);
        let (out, report) = convert(
            "keep %% hidden %% this\n%% multi\nline %%\n```\n%% in code %% [[x]]\n```\nsee [[Host A|the host]] and [[Note#Heading]]\n",
            &mut images,
            &dir.join("Findings"),
        );
        assert_eq!(
            out,
            "keep  this\n```\n%% in code %% [[x]]\n```\nsee the host and Note\n"
        );
        assert_eq!(report.comments, 2);
    }

    #[test]
    fn local_images_are_attached_and_everything_else_is_counted() {
        let dir = engagement("text-images");
        std::fs::write(dir.join("Evidence/shell one.png"), PNG).unwrap();
        std::fs::write(dir.join("Evidence/fake.png"), b"not a png").unwrap();
        std::fs::write(dir.join("Evidence/vector.svg"), b"<svg/>").unwrap();
        let mut images = Images::new(&dir);
        let note = "![[shell one.png|300]]\n![caption](../Evidence/shell%20one.png)\n![[gone.png]]\n![](../Evidence/fake.png)\n![x](../Evidence/vector.svg)\n![r](https://example.test/a.png)\n![[Other note]]\n";
        let (out, report) = convert(note, &mut images, &dir.join("Findings"));
        assert_eq!(
            out,
            "![shell one](/images/name/shell_one.png)\n![caption](/images/name/shell_one.png)\n\n\n\n![r](https://example.test/a.png)\n\n"
        );
        assert_eq!(report.missing_images, ["gone.png"]);
        assert_eq!(report.rejected_images.len(), 2);
        assert_eq!(report.remote_images, 1);
        assert_eq!(report.note_embeds, ["Other note"]);
        let attached = images.into_attached();
        assert_eq!(attached.len(), 1, "the same file is attached once");
        assert_eq!(attached[0].name, "shell_one.png");
    }

    #[test]
    #[cfg(unix)]
    fn a_link_out_of_the_engagement_is_refused_even_through_a_symbolic_link() {
        let dir = engagement("text-escape");
        let outside = dir
            .parent()
            .unwrap()
            .join("sysreptor-text-escape-outside.png");
        std::fs::write(&outside, PNG).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("Evidence/link.png")).unwrap();
        let mut images = Images::new(&dir);
        let (out, report) = convert(
            "![](../Evidence/link.png)\n![](../../sysreptor-text-escape-outside.png)\n![[/etc/passwd.png]]\n",
            &mut images,
            &dir.join("Findings"),
        );
        assert_eq!(out.trim(), "");
        assert!(images.into_attached().is_empty());
        assert_eq!(
            report.rejected_images.len() + report.missing_images.len(),
            3,
            "{report:?}"
        );
    }
}
