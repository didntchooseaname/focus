#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Property,
    Diagram,
}

impl Place {
    pub fn label(self) -> &'static str {
        match self {
            Place::Property => "property",
            Place::Diagram => "diagram",
        }
    }
}

fn wiki_targets(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find("[[") {
        let after = &rest[open + 2..];
        let Some(close) = after.find("]]") else { break };
        let target = after[..close].split(['|', '#']).next().unwrap_or("").trim();
        if !target.is_empty() {
            out.push(target.to_string());
        }
        rest = &after[close + 2..];
    }
    out
}

fn click_target(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("click ")?;
    let start = rest.find('"')? + 1;
    let end = rest[start..].find('"')? + start;
    let target = rest[start..end].trim();
    let target = target
        .strip_prefix("[[")
        .and_then(|t| t.strip_suffix("]]"))
        .unwrap_or(target);
    let target = target.split(['|', '#']).next().unwrap_or("").trim();
    (!target.is_empty() && !target.contains("://")).then(|| target.to_string())
}

pub(crate) fn hidden_links(text: &str) -> Vec<(u32, String, Place)> {
    let mut out = Vec::new();
    let mut in_front = text.starts_with("---\n") || text.starts_with("---\r\n");
    let mut fence: Option<(char, usize, bool)> = None;
    for (index, line) in text.lines().enumerate() {
        let index = index as u32;
        if in_front {
            if index > 0 && line.trim_end() == "---" {
                in_front = false;
            } else {
                out.extend(
                    wiki_targets(line)
                        .into_iter()
                        .map(|t| (index, t, Place::Property)),
                );
            }
            continue;
        }
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'));
        if let Some(c) = marker {
            let run = trimmed.chars().take_while(|x| *x == c).count();
            if run >= 3 {
                match fence {
                    Some((open, length, _))
                        if open == c && run >= length && trimmed[run..].trim().is_empty() =>
                    {
                        fence = None
                    }
                    None => {
                        fence = Some((
                            c,
                            run,
                            trimmed[run..].trim().to_lowercase().starts_with("mermaid"),
                        ))
                    }
                    _ => {}
                }
                continue;
            }
        }
        if let Some((_, _, true)) = fence {
            out.extend(
                wiki_targets(line)
                    .into_iter()
                    .map(|t| (index, t, Place::Diagram)),
            );
            if let Some(target) = click_target(line) {
                out.push((index, target, Place::Diagram));
            }
        }
    }
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_and_diagram_links_are_found_with_their_lines() {
        let note = "---\nrelated: \"[[Alpha|first]]\"\nlinks:\n  - \"[[Beta#part]]\"\n---\n\nBody with [[Body link]].\n\n```mermaid\ngraph TD\n  A[\"[[Gamma]]\"] --> B\n  click B \"Delta.md\"\n  click A href \"https://example.com\"\n```\n\n```bash\necho [[not a link]]\n```\n";
        let found = hidden_links(note);
        let pairs: Vec<(u32, &str, Place)> =
            found.iter().map(|(l, t, p)| (*l, t.as_str(), *p)).collect();
        assert_eq!(
            pairs,
            [
                (1, "Alpha", Place::Property),
                (3, "Beta", Place::Property),
                (10, "Gamma", Place::Diagram),
                (11, "Delta.md", Place::Diagram)
            ]
        );
    }

    #[test]
    fn a_note_without_frontmatter_or_diagrams_has_none() {
        assert!(hidden_links("# Title\n\ntext [[Link]]\n").is_empty());
    }
}
