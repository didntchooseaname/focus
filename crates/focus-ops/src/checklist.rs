#[derive(Debug, Clone)]
pub struct Template {
    pub id: &'static str,
    pub label: &'static str,
    pub markdown: &'static str,
}

const TEMPLATES: &[Template] = &[
    Template {
        id: "mission",
        label: "Engagement workflow",
        markdown: include_str!("../templates/checklist-mission.md"),
    },
    Template {
        id: "exam",
        label: "Exam workflow",
        markdown: include_str!("../templates/checklist-exam.md"),
    },
];

pub fn templates() -> &'static [Template] {
    TEMPLATES
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Section {
    pub title: String,
    pub done: u32,
    pub total: u32,
    pub next: Option<(u32, String)>,
}

fn task(line: &str) -> Option<(bool, &str, usize)> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let after_marker = if let Some(rest) = trimmed.strip_prefix(['-', '*', '+']) {
        rest
    } else {
        let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        trimmed[digits..].strip_prefix(['.', ')'])?
    };
    let spaces = after_marker.len() - after_marker.trim_start().len();
    if spaces == 0 {
        return None;
    }
    let body = after_marker.trim_start();
    let marker_len = trimmed.len() - after_marker.len() + spaces;
    let mut chars = body.chars();
    if chars.next() != Some('[') {
        return None;
    }
    let state = chars.next()?;
    if chars.next() != Some(']')
        || !matches!(state, ' ' | 'x' | 'X')
        || !chars.next().is_none_or(char::is_whitespace)
    {
        return None;
    }
    let text = body[3..].trim();
    Some((state != ' ', text, indent + marker_len + 1))
}

fn is_fence(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

pub fn progress(markdown: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut in_fence = false;
    let mut in_front = markdown.starts_with("---\n") || markdown.starts_with("---\r\n");
    for (index, line) in markdown.lines().enumerate() {
        if in_front {
            if index > 0 && line.trim_end() == "---" {
                in_front = false;
            }
            continue;
        }
        if is_fence(line) {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(title) = line.strip_prefix("## ") {
            sections.push(Section {
                title: title.trim().trim_end_matches('#').trim().to_string(),
                ..Section::default()
            });
            continue;
        }
        let Some((checked, text, _)) = task(line) else {
            continue;
        };
        if sections.is_empty() {
            sections.push(Section::default());
        }
        let section = sections.last_mut().expect("a section exists");
        section.total += 1;
        if checked {
            section.done += 1;
        } else if section.next.is_none() {
            section.next = Some((index as u32 + 1, text.to_string()));
        }
    }
    sections
}

pub fn from_note(markdown: &str) -> String {
    let mut in_fence = false;
    let mut out = String::with_capacity(markdown.len());
    for line in markdown.split_inclusive('\n') {
        if is_fence(line) {
            in_fence = !in_fence;
            out.push_str(line);
            continue;
        }
        match (!in_fence).then(|| task(line)).flatten() {
            Some((true, _, bracket)) => {
                out.push_str(&line[..bracket]);
                out.push(' ');
                out.push_str(&line[bracket + 1..]);
            }
            _ => out.push_str(line),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_generic_and_well_formed() {
        let ids: Vec<&str> = templates().iter().map(|t| t.id).collect();
        assert_eq!(ids, ["mission", "exam"]);
        for template in templates() {
            let sections = progress(template.markdown);
            assert!(sections.len() >= 5, "{}", template.id);
            assert!(
                sections
                    .iter()
                    .all(|s| s.total > 0 && s.done == 0 && s.next.is_some()),
                "{}: boxes already checked or empty section",
                template.id
            );
            assert!(
                !template.markdown.contains("```"),
                "no command in the shipped templates"
            );
        }
        let mission: Vec<String> = progress(templates()[0].markdown)
            .into_iter()
            .map(|s| s.title)
            .collect();
        assert_eq!(
            mission,
            [
                "Scoping and authorization",
                "Scope",
                "Collection",
                "Analysis",
                "Evidence and timestamps",
                "Cleanup",
                "Debrief and report"
            ]
        );
        let exam: Vec<String> = progress(templates()[1].markdown)
            .into_iter()
            .map(|s| s.title)
            .collect();
        for needed in ["Time management", "Required evidence", "Backups", "Report"] {
            assert!(exam.iter().any(|t| t == needed), "{needed}");
        }
    }

    #[test]
    fn progress_counts_per_section_and_finds_next_task() {
        let note = "---\nkey: [ ] not a box\n---\n# List\n\n- [x] before the sections\n\n## One\n- [x] done\n- [ ] to do\n  - [X] done subtask\n* [ ] other\n1. [ ] numbered\n- [] not a box\n- [ ]not a box\n```\n- [ ] in a block\n```\n\n## Two ##\n- [x] a\n- [x] b\n\n## Empty\ntext\n";
        let sections = progress(note);
        assert_eq!(sections.len(), 4);
        assert_eq!(
            sections[0],
            Section {
                title: String::new(),
                done: 1,
                total: 1,
                next: None
            }
        );
        assert_eq!(
            (
                sections[1].title.as_str(),
                sections[1].done,
                sections[1].total
            ),
            ("One", 2, 5)
        );
        assert_eq!(sections[1].next, Some((10, "to do".to_string())));
        assert_eq!(
            (
                sections[2].title.as_str(),
                sections[2].done,
                sections[2].total,
                sections[2].next.clone()
            ),
            ("Two", 2, 2, None)
        );
        assert_eq!(
            (sections[3].title.as_str(), sections[3].total),
            ("Empty", 0)
        );
    }

    #[test]
    fn from_note_unchecks_boxes_outside_code_only() {
        let note = "# My list\n\nImportant instruction.\n\n## Phase\n- [x] done\n  - [X] subtask\n3) [x] numbered\n- [ ] open\n\n```md\n- [x] example in a block\n```\n\nText with [x] in the middle.\n";
        let template = from_note(note);
        assert_eq!(
            template,
            "# My list\n\nImportant instruction.\n\n## Phase\n- [ ] done\n  - [ ] subtask\n3) [ ] numbered\n- [ ] open\n\n```md\n- [x] example in a block\n```\n\nText with [x] in the middle.\n"
        );
        assert!(progress(&template).iter().all(|s| s.done == 0));
        assert_eq!(from_note(&template), template);
        assert_eq!(from_note(""), "");
    }
}
