use std::path::{Path, PathBuf};

use gpui::{AppContext, Context, Window};
use serde_json::{Map, Value as Json, json};

use super::SettingsView;
use super::schema::{Prefs, SETTINGS, Setting, Value};

const FORMAT: &str = "focus-settings";
const VERSION: u64 = 1;

fn encode(value: &Value) -> Json {
    match value {
        Value::Bool(b) => Json::Bool(*b),
        Value::Choice(i) => json!(i),
        Value::Number(n) => json!(n),
        Value::Text(t) => Json::String(t.to_string()),
    }
}

fn decode(json: &Json, default: &Value) -> Option<Value> {
    Some(match default {
        Value::Bool(_) => Value::Bool(json.as_bool()?),
        Value::Choice(_) => Value::Choice(usize::try_from(json.as_u64()?).ok()?),
        Value::Number(_) => Value::Number(json.as_f64()?),
        Value::Text(_) => Value::Text(json.as_str()?.to_string().into()),
    })
}

fn exported(setting: &Setting) -> bool {
    setting.default.is_some() && setting.key != "ai.key"
}

pub fn to_document(prefs: &Prefs) -> Json {
    let mut values = Map::new();
    for setting in SETTINGS.iter().filter(|s| exported(s)) {
        if let Some(value) = prefs.get(setting.key) {
            let mut json = encode(value);
            if let ("ai.mcp_json", Json::String(text)) = (setting.key, &json) {
                json = Json::String(focus_view_ai::without_values(text));
            }
            values.insert(setting.key.to_string(), json);
        }
    }
    json!({ "format": FORMAT, "version": VERSION, "settings": values })
}

type Imported = (Vec<(&'static str, Value)>, usize);

pub fn from_document(text: &str) -> Result<Imported, String> {
    let document: Json = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if document.get("format").and_then(Json::as_str) != Some(FORMAT) {
        return Err("not a Focus settings file".into());
    }
    let values = document
        .get("settings")
        .and_then(Json::as_object)
        .ok_or("no settings object")?;
    let mut accepted = Vec::new();
    let mut refused = 0;
    for (key, json) in values {
        let value = SETTINGS
            .iter()
            .find(|s| s.key == key && exported(s))
            .and_then(|s| {
                let value = decode(json, s.default.as_ref()?)?;
                Prefs::accepts(s, &value).then_some((s.key, value))
            });
        match value {
            Some(value) => accepted.push(value),
            None => refused += 1,
        }
    }
    Ok((accepted, refused))
}

fn start_dir() -> PathBuf {
    focus_core::settings::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

impl SettingsView {
    pub(super) fn export_settings(&mut self, cx: &mut Context<Self>) {
        let document = to_document(self.prefs.read(cx));
        let chosen = cx.prompt_for_new_path(&start_dir(), Some("focus-settings.json"));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            let written = cx
                .background_spawn(async move { write(&path, &document).map(|()| path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.notice = Some(match written {
                    Ok(path) => (
                        false,
                        focus_i18n::t_args(
                            "settings-transfer-exported",
                            &[("path", path.display().to_string().as_str().into())],
                        )
                        .into(),
                    ),
                    Err(error) => {
                        eprintln!("focus: exporting the settings: {error}");
                        (
                            true,
                            focus_i18n::t("settings-transfer-export-failed").into(),
                        )
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn import_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(mut paths))) = chosen.await else {
                return;
            };
            let Some(path) = paths.pop() else { return };
            let read = cx
                .background_spawn(async move {
                    std::fs::read_to_string(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|text| from_document(&text))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.notice = Some(match read {
                    Ok((values, refused)) => {
                        let applied = values.len();
                        this.apply_imported(values, window, cx);
                        (
                            false,
                            focus_i18n::t_args(
                                "settings-transfer-imported",
                                &[("count", applied.into()), ("refused", refused.into())],
                            )
                            .into(),
                        )
                    }
                    Err(error) => {
                        eprintln!("focus: importing the settings: {error}");
                        (
                            true,
                            focus_i18n::t("settings-transfer-import-failed").into(),
                        )
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_imported(
        &mut self,
        values: Vec<(&'static str, Value)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (key, value) in values {
            self.errors.remove(key);
            self.show_in_field(key, &value, window, cx);
            self.prefs.update(cx, |prefs, cx| prefs.set(key, value, cx));
        }
        self.migrate_server_secrets(cx);
    }
}

fn write(path: &Path, document: &Json) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(document).map_err(std::io::Error::other)?;
    std::fs::write(path, text + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_never_exported() {
        let document = to_document(&Prefs::new());
        assert!(document["settings"].get("ai.key").is_none());
        assert!(document["settings"].get("general.language").is_some());
    }

    #[test]
    fn an_export_holds_no_environment_value() {
        use super::super::schema::Value;
        let mut prefs = Prefs::new();
        let servers = r#"[{"name":"docs","command":"d","env":{"TOKEN":"s3cret-value"}}]"#;
        prefs.load(vec![(
            "ai.mcp_json",
            Value::Text(servers.to_string().into()),
        )]);
        let text = to_document(&prefs).to_string();
        assert!(!text.contains("s3cret-value"), "{text}");
        assert!(text.contains("TOKEN"), "the names are kept");
    }

    #[test]
    fn a_document_round_trips() {
        let prefs = Prefs::new();
        let text = to_document(&prefs).to_string();
        let (values, refused) = from_document(&text).expect("readable");
        assert_eq!(refused, 0);
        assert!(values.len() > 40);
    }

    #[test]
    fn entries_out_of_range_or_unknown_are_refused() {
        let text = r#"{"format":"focus-settings","version":1,"settings":{"editor.autosave":99999,"nope":1,"ai.key":"secret","editor.wrap":false}}"#;
        let (values, refused) = from_document(text).expect("readable");
        assert_eq!(values, vec![("editor.wrap", Value::Bool(false))]);
        assert_eq!(refused, 3);
    }

    #[test]
    fn another_file_is_not_a_settings_file() {
        assert!(from_document("{\"hello\":1}").is_err());
        assert!(from_document("nope").is_err());
    }
}
