use std::path::Path;

use focus_i18n::audit::embedded;

const LANGUAGES: [&str; 3] = ["fr", "ar", "zh-CN"];

#[test]
fn every_language_has_the_keys_and_variables_of_english() {
    let english = embedded("en");
    assert!(english.problems.is_empty(), "{:?}", english.problems);
    for code in LANGUAGES {
        let other = embedded(code);
        assert!(other.problems.is_empty(), "{code}: {:?}", other.problems);
        let missing: Vec<_> = english
            .messages
            .keys()
            .filter(|k| !other.messages.contains_key(*k))
            .collect();
        let extra: Vec<_> = other
            .messages
            .keys()
            .filter(|k| !english.messages.contains_key(*k))
            .collect();
        assert!(missing.is_empty(), "{code} is missing {missing:?}");
        assert!(extra.is_empty(), "{code} has extra keys {extra:?}");
        for (key, en) in &english.messages {
            assert_eq!(
                other.messages[key].variables, en.variables,
                "{code}: variables of {key}"
            );
        }
    }
}

#[test]
fn plural_messages_have_the_categories_of_their_language() {
    let english = embedded("en");
    let plural: Vec<&String> = english
        .messages
        .iter()
        .filter(|(_, m)| m.variant_keys.contains("one"))
        .map(|(k, _)| k)
        .collect();
    assert!(plural.len() > 15);
    let arabic = embedded("ar");
    let french = embedded("fr");
    let chinese = embedded("zh-CN");
    for key in plural {
        for category in ["zero", "one", "two", "few", "many", "other"] {
            assert!(
                arabic.messages[key].variant_keys.contains(category),
                "ar: {key} lacks {category}"
            );
        }
        assert!(
            french.messages[key].variant_keys.contains("one"),
            "fr: {key} lacks one"
        );
        assert!(
            !chinese.messages[key].variant_keys.contains("one"),
            "zh-CN: {key} has a plural form"
        );
    }
}

#[test]
fn every_language_has_the_templates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../locales");
    for code in ["en", "fr", "ar", "zh-CN"] {
        for (folder, count) in [("ops", 6), ("slash", 9), ("help", 10)] {
            let names: Vec<String> =
                std::fs::read_dir(root.join(code).join("templates").join(folder))
                    .unwrap()
                    .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect();
            assert_eq!(names.len(), count, "{code}/{folder}: {names:?}");
        }
    }
}

#[test]
fn texts_avoid_forbidden_typography() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../locales");
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for forbidden in [
                '\u{2014}', '\u{2013}', '\u{2192}', '\u{2190}', '\u{00b7}', '\u{2022}',
            ] {
                assert!(
                    !text.contains(forbidden),
                    "{} contains U+{:04X}",
                    path.display(),
                    forbidden as u32
                );
            }
            assert!(
                !text
                    .chars()
                    .any(|c| ('\u{1F300}'..='\u{1FAFF}').contains(&c)
                        || ('\u{2600}'..='\u{27BF}').contains(&c)),
                "{} contains an emoji",
                path.display()
            );
        }
    }
}

fn wiki_and_link_targets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("](") {
        let tail = &rest[i + 2..];
        if let Some(j) = tail.find(')') {
            out.push(if tail[..j].contains('/') {
                "path".to_string()
            } else {
                tail[..j].to_string()
            });
        }
        rest = &rest[i + 2..];
    }
    let mut rest = text;
    while let Some(i) = rest.find("[[") {
        let tail = &rest[i + 2..];
        if let Some(j) = tail.find("]]") {
            out.push(format!(
                "[[{}",
                tail[..j].split(['|', '#']).next().unwrap_or("")
            ));
        }
        rest = &rest[i + 2..];
    }
    out
}

fn code_blocks(text: &str, mermaid: bool) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<(bool, String)> = None;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            match current.take() {
                Some((is_mermaid, body)) => {
                    if is_mermaid == mermaid {
                        blocks.push(body);
                    }
                }
                None => {
                    current = Some((line.trim_start().starts_with("```mermaid"), String::new()))
                }
            }
        } else if let Some((_, body)) = current.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    blocks
}

fn headings(text: &str) -> Vec<usize> {
    let mut in_code = false;
    text.lines()
        .filter_map(|l| {
            if l.trim_start().starts_with("```") {
                in_code = !in_code;
            }
            (!in_code && l.starts_with('#')).then(|| l.chars().take_while(|c| *c == '#').count())
        })
        .collect()
}

#[test]
fn templates_keep_the_structure_of_english() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../locales");
    let shape = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| l.starts_with('#') || l.starts_with("- [ ]") || l.trim().is_empty())
            .map(|l| l.split(' ').next().unwrap_or("").to_string())
            .collect()
    };
    for folder in ["ops", "slash", "help"] {
        for entry in std::fs::read_dir(root.join("en/templates").join(folder)).unwrap() {
            let name = entry.unwrap().file_name();
            let english =
                std::fs::read_to_string(root.join("en/templates").join(folder).join(&name))
                    .unwrap();
            for code in LANGUAGES {
                let translated = std::fs::read_to_string(
                    root.join(code).join("templates").join(folder).join(&name),
                )
                .unwrap();
                if folder == "ops" {
                    assert_eq!(
                        shape(&translated),
                        shape(&english),
                        "{code}/{folder}/{}",
                        name.to_string_lossy()
                    );
                    assert_eq!(
                        translated.matches("{title}").count(),
                        english.matches("{title}").count()
                    );
                    assert_eq!(
                        translated.matches("{name}").count(),
                        english.matches("{name}").count()
                    );
                } else if folder == "help" {
                    assert_eq!(
                        wiki_and_link_targets(&translated),
                        wiki_and_link_targets(&english),
                        "{code}/help/{} links",
                        name.to_string_lossy()
                    );
                    if code == "fr" {
                        assert_eq!(
                            code_blocks(&translated, false).len(),
                            code_blocks(&english, false).len(),
                            "{code}/help/{} code blocks",
                            name.to_string_lossy()
                        );
                    } else {
                        assert_eq!(
                            code_blocks(&translated, false),
                            code_blocks(&english, false),
                            "{code}/help/{} code blocks",
                            name.to_string_lossy()
                        );
                    }
                    assert_eq!(
                        code_blocks(&translated, true).len(),
                        code_blocks(&english, true).len(),
                        "{code}/help/{} diagrams",
                        name.to_string_lossy()
                    );
                    assert_eq!(
                        headings(&translated),
                        headings(&english),
                        "{code}/help/{} headings",
                        name.to_string_lossy()
                    );
                } else {
                    assert!(
                        translated
                            .lines()
                            .next()
                            .is_some_and(|first| !first.trim().is_empty()),
                        "{code}/{folder}/{} is empty",
                        name.to_string_lossy()
                    );
                }
            }
        }
    }
}

#[test]
fn every_builtin_ai_action_has_a_label_and_a_description_in_each_language() {
    const IDS: [&str; 15] = [
        "summarize",
        "refine",
        "rephrase",
        "correct",
        "translate-fr",
        "translate-en",
        "continue",
        "explain",
        "tasks",
        "title-tags",
        "mermaid",
        "note-from-outline",
        "ops-finding",
        "ops-exec-summary",
        "ops-next-enum",
    ];
    for code in ["en", "fr", "ar", "zh-CN"] {
        let messages = embedded(code).messages;
        for id in IDS {
            for suffix in ["label", "desc"] {
                let key = format!("ai-action-{id}-{suffix}");
                assert!(
                    messages.get(&key).is_some_and(|m| !m.text.is_empty()),
                    "{code} lacks {key}"
                );
            }
        }
    }
}

#[test]
fn keys_cited_by_the_ops_crate_exist() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../focus-ops/src/exam.rs"),
    )
    .unwrap();
    let english = embedded("en").messages;
    let mut cited = 0;
    for piece in source.split('"') {
        if piece.starts_with("ops-exam-")
            && !piece.ends_with('-')
            && piece
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            assert!(
                english.contains_key(piece),
                "{piece} is cited by focus-ops but not defined"
            );
            cited += 1;
        }
    }
    assert!(cited >= 40, "only {cited} keys found");
}
