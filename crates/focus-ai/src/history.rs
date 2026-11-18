use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

use super::*;
use crate::timefmt;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub updated: u64,
    pub model: String,
    pub messages: Vec<Message>,
    pub usage: Usage,
}

fn path_for(dir: &Path, id: &str) -> anyhow::Result<PathBuf> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("invalid conversation id: {id:?}");
    }
    Ok(dir.join(format!("{id}.json")))
}

#[derive(serde::Deserialize)]
struct Header {
    id: String,
    title: String,
    updated: u64,
}

pub fn list(dir: &Path) -> Vec<(String, String, u64)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(String, String, u64)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let bytes = std::fs::read(e.path()).ok()?;
            let header: Header = serde_json::from_slice(&bytes).ok()?;
            Some((header.id, header.title, header.updated))
        })
        .collect();
    found.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    found
}

pub fn load(dir: &Path, id: &str) -> anyhow::Result<Conversation> {
    let path = path_for(dir, id)?;
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("unreadable conversation: {}", path.display()))
}

pub fn save(dir: &Path, conversation: &Conversation) -> anyhow::Result<()> {
    let path = path_for(dir, &conversation.id)?;
    let mut folder = std::fs::DirBuilder::new();
    folder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut folder, 0o700);
    folder
        .create(dir)
        .with_context(|| format!("creating {}", dir.display()))?;
    let json = serde_json::to_vec_pretty(conversation)?;
    let temporary = dir.join(format!(".{}.json.tmp", conversation.id));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(&temporary)
        .with_context(|| format!("writing {}", temporary.display()))?;
    std::io::Write::write_all(&mut file, &json)
        .with_context(|| format!("writing {}", temporary.display()))?;
    std::fs::rename(&temporary, &path).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn delete(dir: &Path, id: &str) -> anyhow::Result<()> {
    let path = path_for(dir, id)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("deleting {}", path.display())),
    }
}

pub fn to_markdown(conversation: &Conversation) -> String {
    let mut out = format!("# {}\n\n", conversation.title.trim());
    out.push_str(&format!(
        "Model: {}. Last updated: {} UTC.\n",
        conversation.model,
        timefmt::format_utc(conversation.updated)
    ));
    let usage = &conversation.usage;
    if usage.prompt_tokens + usage.completion_tokens > 0 {
        out.push_str(&format!(
            "Tokens: {} input of which {} cached, {} output.\n",
            usage.prompt_tokens, usage.cache_hit_tokens, usage.completion_tokens
        ));
    }
    for message in &conversation.messages {
        match message.role {
            Role::System => continue,
            Role::User => out.push_str("\n## Question\n\n"),
            Role::Assistant => out.push_str("\n## Answer\n\n"),
            Role::Tool => {
                out.push_str(&format!(
                    "\nResult of tool call `{}`:\n\n",
                    message.tool_call_id.as_deref().unwrap_or("?")
                ));
                out.push_str(&fenced(&message.content));
                continue;
            }
        }
        if let Some(reasoning) = message
            .reasoning
            .as_deref()
            .filter(|r| !r.trim().is_empty())
        {
            out.push_str("> [!note]- Reasoning\n");
            for line in reasoning.trim().lines() {
                out.push_str("> ");
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }
        if !message.content.trim().is_empty() {
            out.push_str(message.content.trim());
            out.push_str("\n\n");
        }
        for call in &message.tool_calls {
            out.push_str(&format!(
                "Tool call `{}`:\n\n{}",
                call.name,
                fenced(&call.arguments)
            ));
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

fn fenced(text: &str) -> String {
    let mut longest = 0usize;
    let mut run = 0usize;
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let fence = "`".repeat((longest + 1).max(3));
    format!("{fence}\n{}\n{fence}\n\n", text.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(format!("history-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn conversation(id: &str, updated: u64) -> Conversation {
        Conversation {
            id: id.into(),
            title: format!("Title {id}"),
            updated,
            model: "deepseek-flash".into(),
            messages: vec![
                Message {
                    role: Role::User,
                    content: "Hello".into(),
                    reasoning: None,
                    tool_calls: vec![],
                    tool_call_id: None,
                },
                Message {
                    role: Role::Assistant,
                    content: "Hi ```x```".into(),
                    reasoning: Some("thinking it over".into()),
                    tool_calls: vec![],
                    tool_call_id: None,
                },
            ],
            usage: Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                ..Usage::default()
            },
        }
    }

    #[test]
    fn save_list_load_delete() {
        let dir = scratch("cycle");
        save(&dir, &conversation("a", 100)).unwrap();
        save(&dir, &conversation("b", 200)).unwrap();
        let listed = list(&dir);
        assert_eq!(
            listed.iter().map(|e| e.0.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert_eq!(load(&dir, "a").unwrap().messages.len(), 2);
        delete(&dir, "a").unwrap();
        delete(&dir, "a").unwrap();
        assert_eq!(list(&dir).len(), 1);
    }

    #[test]
    fn identifiers_cannot_escape_the_directory() {
        let dir = scratch("traversal");
        assert!(load(&dir, "../x").is_err());
        assert!(save(&dir, &conversation("a/b", 1)).is_err());
        assert!(delete(&dir, "").is_err());
    }

    #[test]
    fn markdown_contains_reasoning_and_survives_inner_fences() {
        let markdown = to_markdown(&conversation("a", 1_794_000_000));
        assert!(markdown.starts_with("# Title a\n"));
        assert!(markdown.contains("> [!note]- Reasoning\n> thinking it over"));
        assert!(markdown.contains("## Question\n\nHello"));
        assert!(markdown.contains("2026-"));
    }

    #[test]
    fn corrupt_files_are_skipped_in_listing() {
        let dir = scratch("corrupt");
        save(&dir, &conversation("ok", 1)).unwrap();
        std::fs::write(dir.join("broken.json"), "{").unwrap();
        assert_eq!(list(&dir).len(), 1);
    }

    #[test]
    #[cfg(unix)]
    fn conversations_are_written_for_their_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("private");
        let conversation = conversation("20261112-000000", 1);
        save(&dir, &conversation).expect("saved");
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("20261112-000000.json")), 0o600);
    }
}
