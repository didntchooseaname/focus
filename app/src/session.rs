use std::path::{Path, PathBuf};

use focus_core::settings::{self, Settings};
use serde_json::{Map, Value as Json, json};

use crate::settings::schema::{LANGUAGE_CODES, SETTINGS, Value};

const PROPERTY_TYPES: &str = "property_types";

pub struct Store {
    writable: bool,
    pub settings: Settings,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    pub tabs: Vec<PathBuf>,
    pub views: Vec<Option<crate::editor::ViewMode>>,
    pub pinned: Vec<bool>,
    pub active: Option<usize>,
    pub left: Option<(bool, f32)>,
    pub right: Option<(bool, f32)>,
    pub bottom: Option<(bool, f32)>,
    pub expanded: Vec<PathBuf>,
    pub second: Option<GroupSession>,
    pub share: Option<f32>,
    pub focus_second: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupSession {
    pub tabs: Vec<PathBuf>,
    pub views: Vec<Option<crate::editor::ViewMode>>,
    pub pinned: Vec<bool>,
    pub active: Option<usize>,
}

fn pins_of(json: Option<&Json>) -> Vec<bool> {
    json.and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_bool() == Some(true))
                .collect()
        })
        .unwrap_or_default()
}

fn paths_of(json: Option<&Json>) -> Vec<PathBuf> {
    json.and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Json::as_str)
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

fn views_of(json: Option<&Json>) -> Vec<Option<crate::editor::ViewMode>> {
    json.and_then(Json::as_array)
        .map(|views| views.iter().map(|v| v.as_str().and_then(view_of)).collect())
        .unwrap_or_default()
}

impl Store {
    pub fn load(scripted: bool) -> (Store, Option<String>) {
        let writable =
            !scripted || std::env::var_os("FOCUS_CONFIG_DIR").is_some_and(|v| !v.is_empty());
        if !writable {
            let settings = Settings {
                language: Some("en".into()),
                ..Settings::default()
            };
            return (Store { writable, settings }, None);
        }
        let config = settings::config_dir();
        let migrated = |mut settings: Settings| {
            if migrate(&mut settings)
                && let Err(error) = write(&settings)
            {
                eprintln!("focus: writing the migrated settings: {error:#}");
            }
            settings
        };
        crate::private::log_failure(&config, crate::private::private_dir(&config));
        let file = config.join(SETTINGS_FILE);
        crate::private::log_failure(&file, crate::private::private_file(&file));
        match settings::load() {
            Ok(settings) => (
                Store {
                    writable,
                    settings: migrated(settings),
                },
                None,
            ),
            Err(error) => {
                eprintln!("focus: reading the settings: {error:#}");
                (
                    Store {
                        writable: false,
                        settings: Settings::default(),
                    },
                    Some(format!("{error:#}")),
                )
            }
        }
    }

    pub fn first_launch(&self) -> bool {
        self.settings.language.is_none()
    }

    pub fn writable(&self) -> bool {
        self.writable
    }

    pub fn values(&self) -> Vec<(&'static str, Value)> {
        let s = &self.settings;
        let stored = s.ui.get("settings").and_then(Json::as_object);
        let mut out = Vec::new();
        for setting in SETTINGS {
            let Some(default) = &setting.default else {
                continue;
            };
            let typed = typed_value(s, setting.key, stored);
            let value = typed.or_else(|| {
                stored
                    .and_then(|m| m.get(setting.key))
                    .and_then(|j| from_json(j, default))
            });
            if let Some(value) = value {
                out.push((setting.key, value));
            }
        }
        out
    }

    pub fn set_values<'a>(&mut self, values: impl IntoIterator<Item = (&'static str, &'a Value)>) {
        let mut stored: Map<String, Json> = match self.settings.ui.get("settings") {
            Some(Json::Object(previous)) => previous
                .iter()
                .filter(|(key, _)| crate::settings::schema::setting(key).is_none())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            _ => Map::new(),
        };
        for (key, value) in values {
            if key == "ai.key" {
                continue;
            }
            set_typed(&mut self.settings, key, value);
            stored.insert(key.to_string(), to_json(value));
        }
        self.settings
            .ui
            .insert("settings".into(), Json::Object(stored));
    }

    pub fn session(&self) -> Session {
        let Some(session) = self.settings.ui.get("session") else {
            return Session::default();
        };
        let panel = |name: &str| {
            let panel = session.get(name)?;
            Some((
                panel.get("open")?.as_bool()?,
                panel.get("size")?.as_f64()? as f32,
            ))
        };
        let second = session
            .get("second")
            .filter(|group| group.is_object())
            .map(|group| GroupSession {
                tabs: paths_of(group.get("tabs")),
                views: views_of(group.get("views")),
                pinned: pins_of(group.get("pinned")),
                active: group
                    .get("active")
                    .and_then(Json::as_u64)
                    .map(|n| n as usize),
            });
        Session {
            second,
            share: session
                .get("share")
                .and_then(Json::as_f64)
                .map(|share| share as f32),
            focus_second: session
                .get("focus_second")
                .and_then(Json::as_bool)
                .unwrap_or(false),
            tabs: paths_of(session.get("tabs")),
            views: views_of(session.get("views")),
            pinned: pins_of(session.get("pinned")),
            active: session
                .get("active")
                .and_then(Json::as_u64)
                .map(|n| n as usize),
            left: panel("left"),
            right: panel("right"),
            bottom: panel("bottom"),
            expanded: session
                .get("expanded")
                .and_then(Json::as_array)
                .map(|folders| {
                    folders
                        .iter()
                        .filter_map(Json::as_str)
                        .map(PathBuf::from)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub fn set_session(&mut self, session: &Session) {
        let panel =
            |p: Option<(bool, f32)>| p.map(|(open, size)| json!({ "open": open, "size": size }));
        let tabs: Vec<Json> = session
            .tabs
            .iter()
            .map(|p| Json::String(p.to_string_lossy().into_owned()))
            .collect();
        self.settings.ui.insert(
            "session".into(),
            json!({
                "tabs": tabs,
                "views": session.views.iter().map(|view| view.map(name_of)).collect::<Vec<_>>(),
                "pinned": session.pinned,
                "active": session.active,
                "left": panel(session.left),
                "right": panel(session.right),
                "bottom": panel(session.bottom),
                "expanded": session.expanded.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>(),
                "second": session.second.as_ref().map(|group| json!({
                    "tabs": group.tabs.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>(),
                    "views": group.views.iter().map(|view| view.map(name_of)).collect::<Vec<_>>(),
                    "pinned": group.pinned,
                    "active": group.active,
                })),
                "share": session.share,
                "focus_second": session.focus_second,
            }),
        );
    }

    pub fn vault_list(&self, key: &str, root: &Path) -> Option<Vec<PathBuf>> {
        let list = self
            .settings
            .ui
            .get(key)?
            .get(root.to_string_lossy().as_ref())?
            .as_array()?;
        Some(
            list.iter()
                .filter_map(Json::as_str)
                .map(PathBuf::from)
                .collect(),
        )
    }

    pub fn set_vault_list(&mut self, key: &str, root: &Path, list: &[PathBuf]) {
        let entry = self
            .settings
            .ui
            .entry(key.to_string())
            .or_insert_with(|| Json::Object(Map::new()));
        if !entry.is_object() {
            *entry = Json::Object(Map::new());
        }
        if let Json::Object(vaults) = entry {
            vaults.insert(
                root.to_string_lossy().into_owned(),
                list.iter()
                    .map(|p| Json::String(p.to_string_lossy().into_owned()))
                    .collect(),
            );
        }
    }

    pub fn property_types(
        &self,
        root: &Path,
    ) -> std::collections::BTreeMap<String, focus_core::PropertyType> {
        self.settings
            .ui
            .get(PROPERTY_TYPES)
            .and_then(|vaults| vaults.get(root.to_string_lossy().as_ref()))
            .and_then(Json::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(name, kind)| {
                Some((
                    name.clone(),
                    focus_core::PropertyType::from_name(kind.as_str()?)?,
                ))
            })
            .collect()
    }

    pub fn set_property_types(
        &mut self,
        root: &Path,
        types: &std::collections::BTreeMap<String, focus_core::PropertyType>,
    ) {
        let entry = self
            .settings
            .ui
            .entry(PROPERTY_TYPES.to_string())
            .or_insert_with(|| Json::Object(Map::new()));
        if !entry.is_object() {
            *entry = Json::Object(Map::new());
        }
        if let Json::Object(vaults) = entry {
            let key = root.to_string_lossy().into_owned();
            if types.is_empty() {
                vaults.remove(&key);
            } else {
                vaults.insert(
                    key,
                    types
                        .iter()
                        .map(|(name, kind)| (name.clone(), Json::String(kind.name().into())))
                        .collect::<Map<String, Json>>()
                        .into(),
                );
            }
        }
    }

    pub fn opened_vault(&mut self, root: &Path) {
        let root = root.to_path_buf();
        self.settings.recent_vaults.retain(|p| *p != root);
        self.settings.recent_vaults.insert(0, root.clone());
        self.settings.recent_vaults.truncate(8);
        self.settings.last_vault = Some(root);
    }
}

pub fn write(settings: &Settings) -> anyhow::Result<()> {
    static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = WRITING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    settings::save(settings)?;
    let file = settings::config_dir().join(SETTINGS_FILE);
    crate::private::private_file(&file)
        .map_err(|error| anyhow::anyhow!("restricting {}: {error}", file.display()))
}

const SETTINGS_FILE: &str = "settings.json";

fn from_json(json: &Json, default: &Value) -> Option<Value> {
    Some(match default {
        Value::Bool(_) => Value::Bool(json.as_bool()?),
        Value::Choice(_) => Value::Choice(json.as_u64()? as usize),
        Value::Number(_) => Value::Number(json.as_f64()?),
        Value::Text(_) => Value::Text(json.as_str()?.to_string().into()),
    })
}

fn to_json(value: &Value) -> Json {
    match value {
        Value::Bool(b) => Json::Bool(*b),
        Value::Choice(i) => json!(i),
        Value::Number(n) => json!(n),
        Value::Text(t) => Json::String(t.to_string()),
    }
}

const VIEWS: [&str; 3] = ["source", "live", "reading"];

fn view_of(name: &str) -> Option<crate::editor::ViewMode> {
    use crate::editor::ViewMode;
    match name {
        "source" => Some(ViewMode::Source),
        "live" | "split" => Some(ViewMode::Live),
        "reading" => Some(ViewMode::Reading),
        _ => None,
    }
}

fn name_of(view: crate::editor::ViewMode) -> &'static str {
    use crate::editor::ViewMode;
    match view {
        ViewMode::Source => "source",
        ViewMode::Live => "live",
        ViewMode::Reading => "reading",
    }
}

const SETTINGS_VERSION: u64 = 1;
const VERSION_KEY: &str = "settings_version";

fn migrate(settings: &mut Settings) -> bool {
    let version = settings
        .ui
        .get(VERSION_KEY)
        .and_then(Json::as_u64)
        .unwrap_or(0);
    if version >= SETTINGS_VERSION {
        return false;
    }
    if version < 1 {
        settings.default_view = "live".into();
        if let Some(Json::Object(stored)) = settings.ui.get_mut("settings") {
            stored.insert("general.default_view".into(), json!(1));
        }
    }
    settings
        .ui
        .insert(VERSION_KEY.into(), json!(SETTINGS_VERSION));
    true
}
const MEGABYTE: u64 = 1024 * 1024;
const FIXED: [&str; 4] = ["en", "fr", "ar", "zh-CN"];

fn typed_value(s: &Settings, key: &str, stored: Option<&Map<String, Json>>) -> Option<Value> {
    Some(match key {
        "general.language" => {
            let code = s.language.as_deref()?;
            Value::Choice(
                LANGUAGE_CODES
                    .iter()
                    .position(|c| c.unwrap_or("system") == code)
                    .unwrap_or(0),
            )
        }
        "general.default_view" => Value::Choice(
            VIEWS
                .iter()
                .position(|v| *v == s.default_view)
                .or((s.default_view == "split").then_some(1))?,
        ),
        "appearance.theme" => match stored.and_then(|m| m.get(key)).and_then(Json::as_u64) {
            Some(2) => Value::Choice(2),
            _ => Value::Choice(usize::from(s.theme == "light")),
        },
        "appearance.reading_size" => Value::Number(f64::from(s.reading_font_size)),
        "appearance.measure" => Value::Number(f64::from(s.measure_px)),
        "appearance.hatch" => Value::Bool(s.show_hatch),
        "appearance.reduce_motion" => Value::Bool(s.reduce_motion),
        "editor.autosave" => Value::Number(f64::from(s.autosave_ms)),
        "ops.enabled" => Value::Bool(s.ops_enabled),
        "ops.folder" => Value::Text(s.ops_folder.clone().into()),
        "ai.model" => Value::Text(s.ai_model.clone().into()),
        "ai.base_url" => Value::Text(s.ai_base_url.clone().into()),
        "ai.redact" => Value::Bool(s.ai_redact),
        "ai.redact_ips" => Value::Bool(s.ai_redact_ips),
        "ai.temperature" => Value::Number(f64::from(s.ai_temperature?)),
        "ai.thinking" => Value::Choice(usize::from(s.ai_thinking)),
        "ai.reply_language" => Value::Choice(match s.ai_reply_language.as_str() {
            "interface" => 0,
            "note" => 1,
            _ => 2,
        }),
        "ai.reply_language_fixed" => {
            Value::Choice(FIXED.iter().position(|c| *c == s.ai_reply_language)?)
        }
        "ai.preprompt" => Value::Text(s.ai_preprompt.clone().into()),
        "ai.write_tools" => Value::Bool(s.ai_write_tools),
        "search.ai" => Value::Bool(s.ai_search?),
        "files.history" => Value::Bool(s.history.enabled),
        "files.history_days" => Value::Number(f64::from(s.history.days)),
        "files.history_size" => Value::Number((s.history.max_bytes / MEGABYTE) as f64),
        _ => return None,
    })
}

fn set_typed(s: &mut Settings, key: &str, value: &Value) {
    let number = |v: &Value| {
        if let Value::Number(n) = v {
            Some(*n)
        } else {
            None
        }
    };
    let flag = |v: &Value| {
        if let Value::Bool(b) = v {
            Some(*b)
        } else {
            None
        }
    };
    let choice = |v: &Value| {
        if let Value::Choice(i) = v {
            Some(*i)
        } else {
            None
        }
    };
    let text = |v: &Value| {
        if let Value::Text(t) = v {
            Some(t.to_string())
        } else {
            None
        }
    };
    match key {
        "general.language" => {
            if let Some(i) = choice(value) {
                s.language = Some(
                    LANGUAGE_CODES
                        .get(i)
                        .copied()
                        .flatten()
                        .unwrap_or("system")
                        .to_string(),
                );
            }
        }
        "general.default_view" => {
            if let Some(view) = choice(value).and_then(|i| VIEWS.get(i)) {
                s.default_view = view.to_string();
            }
        }
        "appearance.theme" => match choice(value) {
            Some(0) => s.theme = "dark".into(),
            Some(1) => s.theme = "light".into(),
            _ => {}
        },
        "appearance.reading_size" => {
            s.reading_font_size = number(value).map_or(s.reading_font_size, |n| n as f32)
        }
        "appearance.measure" => s.measure_px = number(value).map_or(s.measure_px, |n| n as f32),
        "appearance.hatch" => s.show_hatch = flag(value).unwrap_or(s.show_hatch),
        "appearance.reduce_motion" => s.reduce_motion = flag(value).unwrap_or(s.reduce_motion),
        "editor.autosave" => s.autosave_ms = number(value).map_or(s.autosave_ms, |n| n as u32),
        "ops.enabled" => s.ops_enabled = flag(value).unwrap_or(s.ops_enabled),
        "ops.folder" => s.ops_folder = text(value).unwrap_or_default(),
        "ai.model" => s.ai_model = text(value).unwrap_or_default(),
        "ai.base_url" => s.ai_base_url = text(value).unwrap_or_default(),
        "ai.redact" => s.ai_redact = flag(value).unwrap_or(s.ai_redact),
        "ai.redact_ips" => s.ai_redact_ips = flag(value).unwrap_or(s.ai_redact_ips),
        "ai.temperature" => s.ai_temperature = number(value).map(|n| n as f32),
        "ai.thinking" => s.ai_thinking = choice(value) == Some(1),
        "ai.reply_language" => {
            s.ai_reply_language = match choice(value) {
                Some(1) => "note".into(),
                Some(2) => FIXED[0].into(),
                _ => "interface".into(),
            }
        }
        "ai.reply_language_fixed" => {
            if !matches!(s.ai_reply_language.as_str(), "interface" | "note") {
                s.ai_reply_language = choice(value)
                    .and_then(|i| FIXED.get(i))
                    .unwrap_or(&FIXED[0])
                    .to_string();
            }
        }
        "ai.preprompt" => s.ai_preprompt = text(value).unwrap_or_default(),
        "ai.write_tools" => s.ai_write_tools = flag(value).unwrap_or(s.ai_write_tools),
        "search.ai" => s.ai_search = flag(value),
        "files.history" => s.history.enabled = flag(value).unwrap_or(s.history.enabled),
        "files.history_days" => s.history.days = number(value).map_or(s.history.days, |n| n as u32),
        "files.history_size" => {
            s.history.max_bytes = number(value).map_or(s.history.max_bytes, |n| n as u64 * MEGABYTE)
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_becomes_the_default_view_once() {
        let mut settings = Settings {
            default_view: "source".into(),
            ..Settings::default()
        };
        settings
            .ui
            .insert("settings".into(), json!({ "general.default_view": 0 }));
        assert!(migrate(&mut settings));
        assert_eq!(settings.default_view, "live");
        assert_eq!(settings.ui["settings"]["general.default_view"], json!(1));
        settings.default_view = "reading".into();
        assert!(!migrate(&mut settings), "a later choice is kept");
        assert_eq!(settings.default_view, "reading");
    }

    #[test]
    fn tab_views_are_kept_and_split_reads_as_live() {
        use crate::editor::ViewMode;
        let mut store = Store {
            writable: false,
            settings: Settings::default(),
        };
        let session = Session {
            tabs: vec![PathBuf::from("a.md"), PathBuf::from("b.md")],
            views: vec![Some(ViewMode::Reading), Some(ViewMode::Live)],
            ..Session::default()
        };
        store.set_session(&session);
        assert_eq!(store.session().views, session.views);
        assert_eq!(view_of("split"), Some(ViewMode::Live));
    }

    #[test]
    fn both_tab_groups_and_the_divider_are_kept() {
        use crate::editor::ViewMode;
        let mut store = Store {
            writable: false,
            settings: Settings::default(),
        };
        let session = Session {
            tabs: vec![PathBuf::from("a.md")],
            views: vec![Some(ViewMode::Source)],
            pinned: vec![true],
            active: Some(0),
            second: Some(GroupSession {
                tabs: vec![PathBuf::from("a.md"), PathBuf::from("b.md")],
                views: vec![Some(ViewMode::Reading), Some(ViewMode::Live)],
                pinned: vec![false, true],
                active: Some(1),
            }),
            share: Some(0.375),
            focus_second: true,
            ..Session::default()
        };
        store.set_session(&session);
        assert_eq!(store.session(), session);
        store.set_session(&Session::default());
        assert_eq!(
            store.session().second,
            None,
            "one group: no second one comes back"
        );
    }

    #[test]
    fn history_rows_fill_the_shared_settings() {
        let mut store = Store {
            writable: false,
            settings: Settings::default(),
        };
        store.set_values([
            ("files.history", &Value::Bool(false)),
            ("files.history_days", &Value::Number(30.)),
            ("files.history_size", &Value::Number(500.)),
        ]);
        let history = store.settings.history;
        assert!(!history.enabled);
        assert_eq!(history.days, 30);
        assert_eq!(history.max_bytes, 500 * 1024 * 1024);
        let values = store.values();
        let value = |key: &str| {
            values
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(value("files.history"), Some(Value::Bool(false)));
        assert_eq!(value("files.history_days"), Some(Value::Number(30.)));
        assert_eq!(value("files.history_size"), Some(Value::Number(500.)));
    }

    #[test]
    fn keys_without_a_settings_row_are_kept() {
        let mut store = Store {
            writable: false,
            settings: Settings::default(),
        };
        store.settings.ui.insert(
            "settings".into(),
            json!({ "later.version.key": "kept", "general.default_view": 0 }),
        );
        store.set_values([("general.default_view", &Value::Choice(1))]);
        assert_eq!(
            store.settings.ui["settings"]["later.version.key"],
            json!("kept")
        );
        assert_eq!(
            store.settings.ui["settings"]["general.default_view"],
            json!(1)
        );
    }

    #[test]
    fn lists_are_kept_per_vault() {
        let mut store = Store {
            writable: false,
            settings: Settings::default(),
        };
        let (a, b) = (Path::new("/vaults/a"), Path::new("/vaults/b"));
        assert_eq!(store.vault_list("bookmarks", a), None);
        store.set_vault_list(
            "bookmarks",
            a,
            &[PathBuf::from("x.md"), PathBuf::from("Dir/y.md")],
        );
        store.set_vault_list("bookmarks", b, &[]);
        assert_eq!(
            store.vault_list("bookmarks", a),
            Some(vec![PathBuf::from("x.md"), PathBuf::from("Dir/y.md")])
        );
        assert_eq!(
            store.vault_list("bookmarks", b),
            Some(Vec::new()),
            "an emptied list stays stored, so .obsidian is not read again"
        );
        assert_eq!(store.vault_list("recent_notes", a), None);
    }

    #[test]
    fn property_types_are_kept_per_vault() {
        use focus_core::PropertyType;
        let mut store = Store {
            writable: false,
            settings: Settings::default(),
        };
        let (a, b) = (Path::new("/vaults/a"), Path::new("/vaults/b"));
        assert!(store.property_types(a).is_empty());
        let types = [
            ("due".to_string(), PropertyType::Date),
            ("done".to_string(), PropertyType::Checkbox),
        ]
        .into();
        store.set_property_types(a, &types);
        assert_eq!(store.property_types(a), types);
        assert!(store.property_types(b).is_empty());
        assert_eq!(
            store.settings.ui["property_types"]["/vaults/a"],
            json!({"done": "checkbox", "due": "date"})
        );
        store.set_property_types(a, &Default::default());
        assert!(store.property_types(a).is_empty());
    }
}
