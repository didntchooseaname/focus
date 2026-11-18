use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use focus_view_ai::{
    AiEvent, AiPanel, AiSettings, ChatBackend, EditorContext, NoteSnapshot, Services, Snapshot,
    WriteGate,
};
use gpui::{App, AppContext, Context, Entity, Window};

use crate::editor::{NoteEditor, ViewMode};
use crate::shell::{RightTab, Shell};

pub type ActiveCell = Rc<RefCell<Option<Entity<NoteEditor>>>>;

struct ActiveNote(ActiveCell);

impl EditorContext for ActiveNote {
    fn snapshot(&self, cx: &App) -> Snapshot {
        let Some(editor) = self.0.borrow().clone() else {
            return Snapshot::default();
        };
        let editor = editor.read(cx);
        let input = editor.input.read(cx);
        let field = editor.view == ViewMode::Source
            || (editor.view == ViewMode::Live && editor.live.is_some())
            || editor.caret_in_place(cx);
        let selection = if field {
            Some(editor.typing_input().read(cx).selection_text())
        } else {
            editor.reading.read(cx).selected_text()
        }
        .filter(|s| !s.is_empty());
        Snapshot {
            note: Some(NoteSnapshot {
                path: editor.meta.path.to_path_buf(),
                title: editor.meta.title.to_string(),
                tags: editor.meta.tags.clone(),
                text: Arc::from(input.value().as_ref()),
            }),
            selection: selection.map(Arc::from),
        }
    }
}

impl Shell {
    pub fn mount_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let gate = WriteGate::new();
        self.ai_guard = self.engagements_dir(cx).filter(|dir| dir.is_dir());
        let tools = ai_tools(
            library.vault().clone(),
            library.index().clone(),
            self.ai_guard.as_deref(),
        )
        .with_confirmation(gate.confirm());
        let capture_without_data_dir =
            self.scripted && std::env::var_os("FOCUS_DATA_DIR").is_none_or(|v| v.is_empty());
        let history_dir = (self.store.writable() && !capture_without_data_dir)
            .then(|| focus_view_ai::history_dir_for(library.root()));
        for folder in history_dir
            .iter()
            .flat_map(|dir| dir.ancestors().skip(1).take(2))
        {
            if folder.is_dir() {
                crate::private::log_failure(folder, crate::private::private_dir(folder));
            }
        }
        let services = Services {
            vault: library.vault().clone(),
            index: library.index().clone(),
            editor: Rc::new(ActiveNote(self.ai_note.clone())),
            tools: Arc::new(tools),
            gate,
            history_dir,
        };
        let settings = AiSettings::from_core(&self.store.settings);
        let panel = cx.new(|cx| AiPanel::new(services, None, settings, window, cx));
        self._subscriptions
            .push(cx.subscribe_in(&panel, window, Self::on_ai_event));
        self.ai = Some(panel.clone());
        self.refresh_known_secrets(cx);
        self.connect_ai(window, cx);
    }

    pub fn guard_ai_engagements(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai.is_none() || self.engagements_dir(cx).filter(|dir| dir.is_dir()) == self.ai_guard
        {
            return;
        }
        self.mount_ai(window, cx);
        if self.layout.right_tab == RightTab::Ai {
            cx.notify();
        }
    }

    pub fn refresh_known_secrets(&mut self, cx: &mut Context<Self>) {
        let (Some(panel), Some(dir)) = (self.ai.clone(), self.engagements_dir(cx)) else {
            return;
        };
        let task = cx.background_spawn(async move { engagement_secrets(&dir) });
        cx.spawn(async move |_, cx| {
            let secrets = task.await;
            let _ = panel.update(cx, |panel, _| panel.set_known_secrets(secrets));
        })
        .detach();
    }

    pub fn ai_vault_event(
        &mut self,
        event: &focus_core::VaultEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use focus_core::VaultEvent;
        let (Some(library), Some(root), Some(dir)) = (
            self.library.clone(),
            self.vault_root.clone(),
            self.engagements_dir(cx),
        ) else {
            return;
        };
        let Ok(folder) = dir.strip_prefix(&root) else {
            return;
        };
        let inside = |path: &std::path::Path| path.starts_with(folder);
        let touched = match event {
            VaultEvent::NoteAdded(id) | VaultEvent::NoteChanged(id) => {
                library.note(*id).is_some_and(|meta| inside(&meta.path))
            }
            VaultEvent::NoteMoved { from, to, .. } => inside(from) || inside(to),
            VaultEvent::NoteRemoved { path, .. } => inside(path),
            VaultEvent::TreeChanged => {
                self.guard_ai_engagements(window, cx);
                false
            }
        };
        if touched {
            self.refresh_known_secrets(cx);
        }
    }

    pub fn migrate_mcp_secrets(&mut self, cx: &mut Context<Self>) {
        let json = self.prefs.read(cx).text("ai.mcp_json");
        if !self.store.writable() || !focus_view_ai::has_plaintext(&json) {
            return;
        }
        let task = cx.background_spawn(async move {
            focus_view_ai::migrate_plaintext(&json, focus_ai::keystore::server_env::store)
        });
        cx.spawn(async move |this, cx| {
            if let Some(migration) = task.await.filter(|m| m.moved > 0) {
                let _ = this.update(cx, |this, cx| {
                    this.set_pref(
                        "ai.mcp_json",
                        crate::settings::schema::Value::Text(migration.json.into()),
                        cx,
                    )
                });
            }
        })
        .detach();
    }

    pub fn connect_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.prefs.read(cx).bool("ai.enabled") {
            self.use_ai_key(None, window, cx);
            return;
        }
        let provider = AiSettings::from_core(&self.store.settings).provider;
        let scripted = self.scripted;
        let task = cx.background_spawn(async move {
            let env = |name: &str| {
                std::env::var(name)
                    .ok()
                    .map(|k| k.trim().to_string())
                    .filter(|k| !k.is_empty())
                    .map(focus_ai::ApiKey::new)
            };
            match provider {
                focus_ai::Provider::DeepSeek => env("DEEPSEEK_API_KEY").or_else(|| {
                    if scripted {
                        None
                    } else {
                        focus_ai::keystore::load().map(|(key, _)| key)
                    }
                }),
                _ if scripted => provider.spec().key_env.and_then(env),
                _ => focus_ai::keystore::provider_key::load(provider),
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let key = task.await;
            let _ = this.update_in(cx, |this, window, cx| this.use_ai_key(key, window, cx));
        })
        .detach();
    }

    fn use_ai_key(
        &mut self,
        key: Option<focus_ai::ApiKey>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = AiSettings::from_core(&self.store.settings);
        let usable = settings.enabled && (key.is_some() || !settings.provider.spec().key_required);
        let backend = usable.then(|| {
            Arc::new(focus_view_ai::DeepSeek::for_provider(
                settings.provider,
                key.clone(),
                settings.base_url.clone(),
            )) as Arc<dyn ChatBackend>
        });
        self.search_assist = usable.then(|| {
            let mut config = focus_ai::Config::for_provider(
                settings.provider,
                key.unwrap_or_else(|| focus_ai::ApiKey::new("")),
            );
            config.base_url = settings.base_url.clone();
            config.model = settings.model.clone();
            Arc::new(focus_view_search::assist::LiveBackend::new(
                focus_ai::Client::new(config),
            )) as Arc<dyn focus_view_search::assist::AssistBackend>
        });
        if let Some(panel) = &self.ai {
            panel.update(cx, |panel, cx| panel.set_backend(backend, cx));
        }
        self.refresh_search_services(window, cx);
    }

    pub fn refresh_search_services(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        self.apply_search_settings(window, cx);
        let prefs = self.prefs.read(cx);
        let redaction = focus_ai::redact::Options {
            secrets: prefs.bool("ai.redact"),
            ip_addresses: prefs.bool("ai.redact_ips"),
            hostnames: false,
        };
        let assist = self
            .search_assist
            .clone()
            .filter(|_| prefs.bool("search.ai"));
        let services = focus_view_search::SearchServices {
            vault: library.vault().clone(),
            index: library.index().clone(),
            assist,
            redaction,
        };
        self.search.update(cx, |panel, cx| {
            panel.set_services(Some(services), window, cx)
        });
    }

    pub fn store_ai_key(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        if key.trim().is_empty() {
            return;
        }
        let task = cx.background_spawn(async move {
            focus_ai::keystore::store(&focus_ai::ApiKey::new(key.trim()))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(_) => this.connect_ai(window, cx),
                Err(error) => {
                    eprintln!("focus: storing the API key: {error:#}");
                    this.notify_user(
                        focus_kit::Tone::Danger,
                        focus_i18n::t_args(
                            "errors-ai-keystore",
                            &[("detail", format!("{error:#}").as_str().into())],
                        ),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub fn apply_search_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prefs = self.prefs.read(cx);
        let settings = focus_view_search::SearchSettings {
            respect_accents: prefs.choice("search.accents") == 1,
            result_limit: prefs.number("search.limit") as usize,
        };
        self.search
            .update(cx, |panel, cx| panel.set_settings(settings, window, cx));
    }

    pub fn ai_settings_changed(&mut self, cx: &mut Context<Self>) {
        let settings = AiSettings::from_core(&self.store.settings);
        if let Some(panel) = &self.ai {
            panel.update(cx, |panel, cx| panel.set_settings(settings, cx));
        }
    }

    pub fn show_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.layout.right_open = true;
        self.layout.right_tab = RightTab::Ai;
        if let Some(panel) = &self.ai {
            panel.update(cx, |panel, cx| {
                panel.refresh_context(cx);
                panel.focus_input(window, cx);
            });
        }
        cx.notify();
    }

    pub fn run_ai_action(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.show_ai(window, cx);
        if let Some(panel) = &self.ai {
            panel.update(cx, |panel, cx| panel.run_action(id, cx));
        }
    }

    fn on_ai_event(
        &mut self,
        _: &Entity<AiPanel>,
        event: &AiEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            AiEvent::OpenSettings => {
                self.open_settings(crate::settings::schema::Section::Ai, window, cx)
            }
            AiEvent::OpenNote { path, line } => {
                if let Some(id) = self.library.as_ref().and_then(|l| l.note_by_path(path)) {
                    self.open_note(id, false, window, cx);
                    if let Some(line) = line {
                        self.pending_line = Some((id, *line));
                    }
                }
            }
            AiEvent::InsertAtCursor(text) => self.insert_in_note(text, window, cx),
            AiEvent::OpenExternal(url) => self.open_address(url, window, cx),
            AiEvent::ModelChosen(model) => self.set_pref(
                "ai.model",
                crate::settings::schema::Value::Text(model.clone().into()),
                cx,
            ),
            AiEvent::SearchTag(tag) => {
                self.show_search(&format!("tag:#{}", tag.trim_start_matches('#')), window, cx)
            }
            AiEvent::ZoomDiagram(source) => {
                self.overlay = crate::shell::Overlay::Zoom(source.clone());
                cx.notify();
            }
            AiEvent::ReplaceText {
                original,
                replacement,
                ..
            } => {
                let replaced = self.active_editor().cloned().is_some_and(|editor| {
                    editor.update(cx, |editor, cx| {
                        editor.replace_once(original, replacement, window, cx)
                    })
                });
                if !replaced {
                    self.notify_user(
                        focus_kit::Tone::Warn,
                        focus_i18n::t("ui-ai-text-changed"),
                        cx,
                    );
                }
            }
            AiEvent::InsertBelow { scope, text } => {
                let Some(editor) = self.active_editor().cloned() else {
                    return;
                };
                let block = format!("\n\n{text}\n");
                editor.update(cx, |editor, cx| match scope {
                    focus_ai::prompts::Scope::Note => editor.append(&block, window, cx),
                    _ => editor.insert_after_selection(&block, window, cx),
                });
            }
            AiEvent::CreateNote { title, content }
            | AiEvent::SaveConversationAsNote {
                title,
                markdown: content,
            } => self.create_note(title, content, window, cx),
            AiEvent::Notify { tone, text } => {
                let tone = match tone {
                    focus_view_ai::NoticeTone::Neutral => focus_kit::Tone::Neutral,
                    focus_view_ai::NoticeTone::Ok => focus_kit::Tone::Ok,
                    focus_view_ai::NoticeTone::Warn => focus_kit::Tone::Warn,
                    focus_view_ai::NoticeTone::Danger => focus_kit::Tone::Danger,
                };
                self.notify_user(tone, text.clone(), cx);
            }
        }
    }

    pub fn insert_in_note(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor().cloned() else {
            self.notify_user(focus_kit::Tone::Warn, focus_i18n::t("ui-info-no-note"), cx);
            return;
        };
        if editor.read(cx).view == ViewMode::Reading {
            editor.update(cx, |editor, cx| {
                editor.append(&format!("\n\n{text}\n"), window, cx)
            });
        } else {
            editor.update(cx, |editor, cx| editor.insert_at_cursor(text, window, cx));
        }
    }

    pub fn create_note(
        &mut self,
        title: &str,
        content: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let folder = match self.prefs.read(cx).get("general.new_note_folder") {
            Some(crate::settings::schema::Value::Text(folder)) => folder.to_string(),
            _ => String::new(),
        };
        let name: String = title
            .chars()
            .map(|c| {
                if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        let name = if name.trim().is_empty() {
            focus_i18n::t("ui-note-untitled")
        } else {
            name.trim().to_string()
        };
        let content = content.to_string();
        let task = cx.background_spawn(async move {
            let mut attempt = 0;
            loop {
                let file = if attempt == 0 {
                    format!("{name}.md")
                } else {
                    format!("{name} {}.md", attempt + 1)
                };
                let path = std::path::Path::new(&folder).join(file);
                match library.vault().create_note(&path, &content) {
                    Err(focus_core::VaultError::AlreadyExists(_)) if attempt < 50 => attempt += 1,
                    result => return result,
                }
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(id) => this.open_note(id, true, window, cx),
                Err(error) => {
                    this.notify_user(focus_kit::Tone::Danger, crate::errors::vault(&error), cx)
                }
            });
        })
        .detach();
    }
}

fn ai_tools(
    vault: Arc<focus_core::Vault>,
    index: Arc<focus_search::SearchIndex>,
    engagements: Option<&std::path::Path>,
) -> focus_mcp::VaultTools {
    let tools = focus_mcp::VaultTools::new(vault, index).with_extended_tools();
    match engagements {
        Some(dir) => tools.with_masked_engagements(dir),
        None => tools,
    }
}

fn engagement_secrets(engagements: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(engagements) else {
        return Vec::new();
    };
    let mut secrets: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .flat_map(|path| focus_ops::loot::list(&path))
        .map(|credential| credential.secret.trim().to_string())
        .filter(|secret| !secret.is_empty())
        .collect();
    secrets.sort();
    secrets.dedup();
    secrets
}

pub fn scripted_backend(
    note: Option<&std::path::Path>,
    answer: Option<&str>,
) -> focus_view_ai::fake::FakeBackend {
    use focus_view_ai::fake::{Beat, FakeBackend};
    const ANSWER: &str = "The note has **two parts**: what the technique does, then how to check it.\n\n\
        | Part | Holds |\n| --- | --- |\n| Overview | the idea and when it applies |\n| Checks | the steps, in `code` blocks |\n\n\
        Ask for a shorter version or a list of follow-up tasks.";
    let mut beats = Vec::new();
    if let Some(note) = note {
        beats.push(Beat::Reasoning(
            "The question is about the open note: read it first.".into(),
        ));
        beats.push(Beat::Tool {
            name: "read_note".into(),
            arguments: serde_json::json!({ "path": note.to_string_lossy() }).to_string(),
        });
    }
    beats.extend(
        answer
            .unwrap_or(ANSWER)
            .split_inclusive('\n')
            .map(|part| Beat::Content(part.to_string())),
    );
    beats.push(Beat::Usage(focus_ai::Usage {
        prompt_tokens: 3412,
        completion_tokens: 187,
        reasoning_tokens: 24,
        cache_hit_tokens: 2048,
        cache_miss_tokens: 1364,
    }));
    FakeBackend::new(vec![beats], std::time::Duration::from_millis(30)).with_models(vec![
        focus_ai::MODEL_FLASH.to_string(),
        focus_ai::MODEL_PRO.to_string(),
    ])
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use focus_ai::agent::{ToolHost, ToolOutcome};
    use focus_ai::{Cancel, ToolCall};

    use super::*;

    const SECRET: &str = "Pl4inBareSecret";

    #[test]
    fn the_model_never_receives_an_engagement_credential() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/test-tmp/ai-loot");
        let _ = std::fs::remove_dir_all(&root);
        let vault_dir = root.join("vault");
        let engagements = vault_dir.join("Engagements");
        std::fs::create_dir_all(&engagements).unwrap();
        let spec = focus_ops::NewEngagement {
            name: "Demo".into(),
            kind: focus_ops::EngagementKind::Lab,
            client: None,
            scope: Vec::new(),
            exam: None,
            checklist: None,
        };
        let engagement = focus_ops::create(&engagements, &spec).unwrap();
        let credential = focus_ops::Credential {
            user: "svc_sql".into(),
            kind: "password".into(),
            secret: SECRET.into(),
            domain: "CORP".into(),
            source: String::new(),
            valid_on: String::new(),
            notes: String::new(),
        };
        focus_ops::loot::add(&engagement.dir, &credential).unwrap();
        std::fs::write(
            engagement.dir.join("Notes.md"),
            format!("| host | user | value |\n|---|---|---|\n| sql01 | svc_sql | {SECRET} |\n"),
        )
        .unwrap();

        let cache = root.join("cache");
        let vault = focus_core::Vault::open(
            &vault_dir,
            focus_core::OpenOptions {
                cache_dir: cache.clone(),
                ignore: Vec::new(),
                watch: false,
            },
        )
        .unwrap();
        let index = focus_search::SearchIndex::open(vault.clone(), &cache, &|_| {}).unwrap();
        let tools = ai_tools(vault, index, Some(&engagements));
        let call = |name: &str, arguments: serde_json::Value| match tools.call(
            &ToolCall {
                id: "1".into(),
                name: name.into(),
                arguments: arguments.to_string(),
            },
            &Cancel::new(),
        ) {
            ToolOutcome::Ok(text) | ToolOutcome::Error(text) => text,
            ToolOutcome::Denied => String::new(),
        };
        for (tool, arguments) in [
            (
                "read_note",
                serde_json::json!({ "path": "Engagements/Demo/Loot/Credentials.md" }),
            ),
            (
                "read_note",
                serde_json::json!({ "path": "Engagements/Demo/Notes.md" }),
            ),
            ("search_notes", serde_json::json!({ "query": "svc_sql" })),
        ] {
            let output = call(tool, arguments.clone());
            assert!(
                !output.contains(SECRET),
                "{tool} {arguments} gave the secret: {output}"
            );
        }
        assert_eq!(engagement_secrets(&engagements), vec![SECRET.to_string()]);
    }
}
