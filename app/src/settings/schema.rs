use std::borrow::Cow;
use std::collections::HashMap;

use gpui::{Context, EventEmitter};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    General,
    Appearance,
    Editor,
    Files,
    Search,
    Graph,
    Ai,
    Ops,
    Shortcuts,
    Transfer,
    About,
}

impl Section {
    pub const ALL: [Section; 11] = [
        Section::General,
        Section::Appearance,
        Section::Editor,
        Section::Files,
        Section::Search,
        Section::Graph,
        Section::Ai,
        Section::Ops,
        Section::Shortcuts,
        Section::Transfer,
        Section::About,
    ];

    pub fn label(self) -> String {
        focus_i18n::t(&format!("settings-section-{}", self.id()))
    }

    pub fn icon(self) -> focus_kit::Icon {
        use focus_kit::Icon;
        match self {
            Section::General => Icon::SETTINGS,
            Section::Appearance => Icon::SLIDERS,
            Section::Editor => Icon::PENCIL,
            Section::Files => Icon::LINK,
            Section::Search => Icon::SEARCH,
            Section::Graph => Icon::WAYPOINTS,
            Section::Ai => Icon::SPARKLES,
            Section::Ops => Icon::CROSSHAIR,
            Section::Shortcuts => Icon::KEYBOARD,
            Section::Transfer => Icon::ARROW_UP_DOWN,
            Section::About => Icon::INFO,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Section::General => "general",
            Section::Appearance => "appearance",
            Section::Editor => "editor",
            Section::Files => "files",
            Section::Search => "search",
            Section::Graph => "graph",
            Section::Ai => "ai",
            Section::Ops => "ops",
            Section::Shortcuts => "shortcuts",
            Section::Transfer => "transfer",
            Section::About => "about",
        }
    }

    pub fn from_name(name: &str) -> Option<Section> {
        if let Some(section) = Section::ALL.into_iter().find(|s| s.id() == name) {
            return Some(section);
        }
        let name = name.to_lowercase();
        Section::ALL.into_iter().find(|s| {
            let label = s.label().to_lowercase();
            label == name
                || label.starts_with(&name)
                || ascii_fold(&label).starts_with(&ascii_fold(&name))
        })
    }
}

pub fn ascii_fold(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'À' | 'Â' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' => 'e',
            'î' | 'ï' | 'Î' => 'i',
            'ô' | 'ö' | 'Ô' => 'o',
            'ù' | 'û' | 'ü' | 'Û' => 'u',
            'ç' | 'Ç' => 'c',
            other => other.to_ascii_lowercase(),
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Choice(usize),
    Number(f64),
    Text(Cow<'static, str>),
}

pub enum Control {
    Toggle,
    Choice(&'static [&'static str]),
    Number {
        min: f64,
        max: f64,
        unit: &'static str,
    },
    Text {
        secret: bool,
    },
    Action,
    Info,
    Custom,
    Stored,
}

pub struct Setting {
    pub key: &'static str,
    pub section: Section,
    pub control: Control,
    pub default: Option<Value>,
}

pub fn option_label(option: &str) -> String {
    if option.starts_with("settings-") || option.starts_with("ui-") {
        focus_i18n::t(option)
    } else {
        option.to_string()
    }
}

pub struct SettingText {
    pub label: String,
    pub description: String,
    pub placeholder: String,
    pub action: String,
}

impl Setting {
    pub fn shown(&self) -> bool {
        !matches!(self.control, Control::Stored)
    }

    pub fn text_base(&self) -> String {
        match (self.key.strip_prefix("ai."), self.key.rsplit('.').next()) {
            (Some(rest), Some("base_url")) if rest != "base_url" => {
                "settings-ai-provider-base-url".to_string()
            }
            (Some(rest), Some("model")) if rest != "model" => {
                "settings-ai-provider-model".to_string()
            }
            _ => format!("settings-{}", self.key.replace(['.', '_'], "-")),
        }
    }

    pub fn text(&self) -> SettingText {
        let base = self.text_base();
        let read = |suffix: &str, wanted: bool| {
            if wanted {
                focus_i18n::t(&format!("{base}-{suffix}"))
            } else {
                String::new()
            }
        };
        let description = match self.control {
            Control::Number { min, max, .. } => {
                let bound = |v: f64| {
                    if v.fract() == 0. {
                        focus_i18n::format_number(v as i64)
                    } else {
                        focus_i18n::format_decimal(v, 1)
                    }
                };
                let (min, max) = (bound(min), bound(max));
                focus_i18n::t_args(
                    &format!("{base}-desc"),
                    &[("min", min.as_str().into()), ("max", max.as_str().into())],
                )
            }
            _ => read("desc", true),
        };
        SettingText {
            label: read("label", true),
            description,
            placeholder: read(
                "placeholder",
                matches!(
                    self.key,
                    "ai.preprompt" | "ops.author" | "ops.library_folders" | "ops.variables"
                ) || base.ends_with("-provider-model"),
            ),
            action: read("action", matches!(self.control, Control::Action)),
        }
    }
}

macro_rules! setting {
    ($key:expr, $section:ident, $control:expr, $default:expr) => {
        Setting {
            key: $key,
            section: Section::$section,
            control: $control,
            default: $default,
        }
    };
}

use Control::*;
use Value as V;

pub const LANGUAGES: &[&str] = &[
    "ui-language-system",
    "ui-language-name-en",
    "ui-language-name-fr",
    "ui-language-name-ar",
    "ui-language-name-zh-cn",
];
pub const FIXED_LANGUAGES: &[&str] = &[
    "ui-language-name-en",
    "ui-language-name-fr",
    "ui-language-name-ar",
    "ui-language-name-zh-cn",
];
pub const LANGUAGE_CODES: [Option<&str>; 5] =
    [None, Some("en"), Some("fr"), Some("ar"), Some("zh-CN")];

pub const EXAM_PROFILES: &[&str] = &[
    "settings-option-exam-none",
    "OSCP+",
    "OSEP",
    "OSWE",
    "OSED",
    "CRTP",
    "CRTE",
    "CRTO",
    "PNPT",
    "CPTS",
    "BSCP",
    "eJPT",
    "eCPPT",
];

pub const SETTINGS: &[Setting] = &[
    setting!(
        "general.language",
        General,
        Choice(LANGUAGES),
        Some(V::Choice(0))
    ),
    setting!("general.vault", General, Info, None),
    setting!("general.open", General, Action, None),
    setting!("general.recent", General, Custom, None),
    setting!("general.reopen_tabs", General, Toggle, Some(V::Bool(true))),
    setting!(
        "general.default_view",
        General,
        Choice(&[
            "ui-view-source",
            "settings-option-view-live",
            "ui-view-reading"
        ]),
        Some(V::Choice(1))
    ),
    setting!(
        "general.new_note_folder",
        General,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!(
        "general.templates",
        General,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("Templates")))
    ),
    setting!(
        "general.daily_folder",
        General,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!(
        "general.daily_format",
        General,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("YYYY-MM-DD")))
    ),
    setting!(
        "general.daily_template",
        General,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!(
        "general.capture_note",
        General,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!(
        "general.capture_format",
        General,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("- {{time}} {{text}}")))
    ),
    setting!(
        "general.confirm_delete",
        General,
        Toggle,
        Some(V::Bool(true))
    ),
    setting!(
        "appearance.theme",
        Appearance,
        Choice(&[
            "settings-option-theme-dark",
            "settings-option-theme-light",
            "settings-option-theme-system"
        ]),
        Some(V::Choice(0))
    ),
    setting!(
        "appearance.ui_scale",
        Appearance,
        Number {
            min: 90.,
            max: 120.,
            unit: "%"
        },
        Some(V::Number(100.))
    ),
    setting!(
        "appearance.reading_size",
        Appearance,
        Number {
            min: 13.,
            max: 20.,
            unit: "px"
        },
        Some(V::Number(15.))
    ),
    setting!(
        "appearance.reading_leading",
        Appearance,
        Number {
            min: 1.4,
            max: 2.,
            unit: ""
        },
        Some(V::Number(1.65))
    ),
    setting!(
        "appearance.measure",
        Appearance,
        Number {
            min: 560.,
            max: 1000.,
            unit: "px"
        },
        Some(V::Number(760.))
    ),
    setting!("appearance.hatch", Appearance, Toggle, Some(V::Bool(true))),
    setting!(
        "appearance.remote_images",
        Appearance,
        Choice(&[
            "settings-option-remote-images-never",
            "settings-option-remote-images-click",
            "settings-option-remote-images-always"
        ]),
        Some(V::Choice(1))
    ),
    setting!(
        "appearance.reading_font",
        Appearance,
        Choice(&[
            "settings-option-font-inter",
            "settings-option-font-sora",
            "settings-option-font-system"
        ]),
        Some(V::Choice(0))
    ),
    setting!(
        "appearance.code_font",
        Appearance,
        Choice(&[
            "settings-option-code-font-jetbrains",
            "settings-option-code-font-system"
        ]),
        Some(V::Choice(0))
    ),
    setting!(
        "appearance.editor_font",
        Appearance,
        Choice(&[
            "settings-option-editor-font-inter",
            "settings-option-editor-font-jetbrains"
        ]),
        Some(V::Choice(0))
    ),
    setting!(
        "appearance.code_colors",
        Appearance,
        Choice(&[
            "settings-option-code-colors-muted",
            "settings-option-code-colors-mono"
        ]),
        Some(V::Choice(0))
    ),
    setting!(
        "appearance.reduce_motion",
        Appearance,
        Toggle,
        Some(V::Bool(false))
    ),
    setting!(
        "appearance.density",
        Appearance,
        Choice(&[
            "settings-option-density-normal",
            "settings-option-density-compact"
        ]),
        Some(V::Choice(0))
    ),
    setting!(
        "editor.autosave",
        Editor,
        Number {
            min: 200.,
            max: 5000.,
            unit: "ms"
        },
        Some(V::Number(800.))
    ),
    setting!("editor.wrap", Editor, Toggle, Some(V::Bool(true))),
    setting!("editor.line_numbers", Editor, Toggle, Some(V::Bool(false))),
    setting!(
        "editor.tab_width",
        Editor,
        Number {
            min: 2.,
            max: 8.,
            unit: ""
        },
        Some(V::Number(4.))
    ),
    setting!("editor.edit_in_place", Editor, Toggle, Some(V::Bool(true))),
    setting!("editor.selection_bar", Editor, Toggle, Some(V::Bool(true))),
    setting!("editor.block_handles", Editor, Toggle, Some(V::Bool(true))),
    setting!("editor.continue_lists", Editor, Toggle, Some(V::Bool(true))),
    setting!("editor.close_pairs", Editor, Toggle, Some(V::Bool(true))),
    setting!(
        "editor.link_style",
        Editor,
        Choice(&["settings-option-link-markdown", "settings-option-link-wiki"]),
        Some(V::Choice(0))
    ),
    setting!(
        "editor.link_path",
        Editor,
        Choice(&[
            "settings-option-link-path-relative",
            "settings-option-link-path-root",
            "settings-option-link-path-shortest"
        ]),
        Some(V::Choice(0))
    ),
    setting!(
        "editor.attachments",
        Editor,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("./assets")))
    ),
    setting!("editor.paste_names", Editor, Toggle, Some(V::Bool(true))),
    setting!("editor.check_links", Editor, Toggle, Some(V::Bool(true))),
    setting!("files.update_links", Files, Toggle, Some(V::Bool(true))),
    setting!(
        "files.exclude",
        Files,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!("files.show_other", Files, Toggle, Some(V::Bool(false))),
    setting!("files.hide_templates", Files, Toggle, Some(V::Bool(true))),
    setting!(
        "files.follow_external_links",
        Files,
        Toggle,
        Some(V::Bool(false))
    ),
    setting!(
        "files.trash",
        Files,
        Choice(&[
            "settings-option-trash-vault",
            "settings-option-trash-system"
        ]),
        Some(V::Choice(0))
    ),
    setting!("files.history", Files, Toggle, Some(V::Bool(true))),
    setting!(
        "files.history_days",
        Files,
        Number {
            min: 1.,
            max: 365.,
            unit: "days"
        },
        Some(V::Number(14.))
    ),
    setting!(
        "files.history_size",
        Files,
        Number {
            min: 10.,
            max: 10000.,
            unit: "mb"
        },
        Some(V::Number(200.))
    ),
    setting!("files.history_used", Files, Info, None),
    setting!("files.history_clear", Files, Action, None),
    setting!(
        "search.accents",
        Search,
        Choice(&[
            "settings-option-accents-ignore",
            "settings-option-accents-respect"
        ]),
        Some(V::Choice(0))
    ),
    setting!(
        "search.limit",
        Search,
        Number {
            min: 50.,
            max: 2000.,
            unit: ""
        },
        Some(V::Number(500.))
    ),
    setting!("search.index", Search, Info, None),
    setting!("search.rebuild", Search, Action, None),
    setting!("search.ai", Search, Toggle, Some(V::Bool(true))),
    setting!("graph.tags", Graph, Toggle, Some(V::Bool(false))),
    setting!("graph.unresolved", Graph, Toggle, Some(V::Bool(false))),
    setting!("graph.orphans", Graph, Toggle, Some(V::Bool(true))),
    setting!(
        "graph.repulsion",
        Graph,
        Number {
            min: 0.,
            max: 100.,
            unit: ""
        },
        Some(V::Number(50.))
    ),
    setting!(
        "graph.distance",
        Graph,
        Number {
            min: 10.,
            max: 300.,
            unit: "px"
        },
        Some(V::Number(80.))
    ),
    setting!(
        "graph.gravity",
        Graph,
        Number {
            min: 0.,
            max: 100.,
            unit: ""
        },
        Some(V::Number(20.))
    ),
    setting!(
        "graph.node_size",
        Graph,
        Number {
            min: 50.,
            max: 200.,
            unit: "%"
        },
        Some(V::Number(100.))
    ),
    setting!(
        "graph.labels",
        Graph,
        Number {
            min: 0.,
            max: 400.,
            unit: "%"
        },
        Some(V::Number(120.))
    ),
    setting!("graph.reset", Graph, Action, None),
    setting!("ai.enabled", Ai, Toggle, Some(V::Bool(true))),
    setting!(
        "ai.provider",
        Ai,
        Choice(&[
            "settings-option-provider-deepseek",
            "settings-option-provider-openai",
            "settings-option-provider-ollama",
            "settings-option-provider-lmstudio",
            "settings-option-provider-vllm",
            "settings-option-provider-anthropic",
            "settings-option-provider-compatible"
        ]),
        Some(V::Choice(0))
    ),
    setting!(
        "ai.openai.base_url",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("https://api.openai.com/v1")))
    ),
    setting!(
        "ai.openai.model",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!(
        "ai.ollama.base_url",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("http://localhost:11434/v1")))
    ),
    setting!(
        "ai.ollama.model",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!(
        "ai.lmstudio.base_url",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("http://localhost:1234/v1")))
    ),
    setting!(
        "ai.lmstudio.model",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!(
        "ai.vllm.base_url",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("http://localhost:8000/v1")))
    ),
    setting!(
        "ai.vllm.model",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!(
        "ai.anthropic.base_url",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("https://api.anthropic.com/v1")))
    ),
    setting!(
        "ai.anthropic.model",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!(
        "ai.compatible.base_url",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("http://localhost:8000/v1")))
    ),
    setting!(
        "ai.compatible.model",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!(
        "ai.provider_key",
        Ai,
        Text { secret: true },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!("ai.provider_status", Ai, Info, None),
    setting!("ai.provider_test", Ai, Action, None),
    setting!("ai.provider_models", Ai, Custom, None),
    setting!(
        "ai.key",
        Ai,
        Text { secret: true },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!("ai.key_status", Ai, Info, None),
    setting!("ai.test", Ai, Action, None),
    setting!("ai.balance", Ai, Info, None),
    setting!(
        "ai.base_url",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("https://api.deepseek.com")))
    ),
    setting!(
        "ai.model",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed(focus_ai::MODEL_FLASH)))
    ),
    setting!("ai.models", Ai, Custom, None),
    setting!(
        "ai.temperature",
        Ai,
        Number {
            min: 0.,
            max: 2.,
            unit: ""
        },
        Some(V::Number(1.))
    ),
    setting!(
        "ai.thinking",
        Ai,
        Choice(&[
            "settings-option-thinking-off",
            "settings-option-thinking-on"
        ]),
        Some(V::Choice(0))
    ),
    setting!(
        "ai.reply_language",
        Ai,
        Choice(&[
            "settings-option-reply-interface",
            "settings-option-reply-note",
            "settings-option-reply-fixed"
        ]),
        Some(V::Choice(0))
    ),
    setting!(
        "ai.reply_language_fixed",
        Ai,
        Choice(FIXED_LANGUAGES),
        Some(V::Choice(0))
    ),
    setting!(
        "ai.max_tokens",
        Ai,
        Number {
            min: 256.,
            max: 32000.,
            unit: ""
        },
        Some(V::Number(4096.))
    ),
    setting!(
        "ai.preprompt",
        Ai,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!("ai.actions", Ai, Custom, None),
    setting!(
        "ai.actions_json",
        Ai,
        Stored,
        Some(V::Text(Cow::Borrowed("[]")))
    ),
    setting!("ai.write_tools", Ai, Toggle, Some(V::Bool(true))),
    setting!(
        "ai.max_steps",
        Ai,
        Number {
            min: 1.,
            max: 20.,
            unit: ""
        },
        Some(V::Number(8.))
    ),
    setting!("ai.mcp_servers", Ai, Custom, None),
    setting!(
        "ai.mcp_json",
        Ai,
        Stored,
        Some(V::Text(Cow::Borrowed("[]")))
    ),
    setting!("ai.mcp_command", Ai, Info, None),
    setting!("ai.skill", Ai, Action, None),
    setting!("ai.agents_md", Ai, Action, None),
    setting!("ai.agent_socket", Ai, Toggle, Some(V::Bool(true))),
    setting!("ai.redact", Ai, Toggle, Some(V::Bool(true))),
    setting!("ai.redact_ips", Ai, Toggle, Some(V::Bool(false))),
    setting!("ai.history", Ai, Toggle, Some(V::Bool(true))),
    setting!("ai.clear", Ai, Action, None),
    setting!("ops.enabled", Ops, Toggle, Some(V::Bool(false))),
    setting!(
        "ops.folder",
        Ops,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("Engagements")))
    ),
    setting!("ops.active", Ops, Info, None),
    setting!(
        "ops.library_folders",
        Ops,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!(
        "ops.variables",
        Ops,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!("ops.substitute", Ops, Toggle, Some(V::Bool(true))),
    setting!("ops.code_variables", Ops, Toggle, Some(V::Bool(true))),
    setting!(
        "ops.interface",
        Ops,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("tun0")))
    ),
    setting!("ops.mask", Ops, Toggle, Some(V::Bool(true))),
    setting!("ops.exam", Ops, Choice(EXAM_PROFILES), Some(V::Choice(0))),
    setting!(
        "ops.alert",
        Ops,
        Number {
            min: 5.,
            max: 240.,
            unit: "min"
        },
        Some(V::Number(60.))
    ),
    setting!(
        "ops.author",
        Ops,
        Text { secret: false },
        Some(V::Text(Cow::Borrowed("")))
    ),
    setting!(
        "shortcuts.overrides",
        Shortcuts,
        Stored,
        Some(V::Text(Cow::Borrowed("{}")))
    ),
    setting!("transfer.import", Transfer, Action, None),
    setting!("transfer.export_vault", Transfer, Action, None),
    setting!(
        "transfer.html_theme",
        Transfer,
        Choice(&["settings-option-theme-dark", "settings-option-theme-light"]),
        Some(V::Choice(0))
    ),
    setting!(
        "transfer.html_images",
        Transfer,
        Toggle,
        Some(V::Bool(true))
    ),
    setting!("transfer.html_fonts", Transfer, Toggle, Some(V::Bool(true))),
    setting!("transfer.settings_export", Transfer, Action, None),
    setting!("transfer.settings_import", Transfer, Action, None),
];

pub fn text_refusal(key: &str, text: &str) -> Option<&'static str> {
    let folder = |entry: &str| {
        let entry = entry.trim();
        entry.starts_with('/')
            || entry.contains(['\\', '\0'])
            || entry.split('/').any(|part| part == "..")
    };
    match key {
        "ai.base_url" => {
            let text = text.trim();
            let host = text
                .strip_prefix("https://")
                .or_else(|| text.strip_prefix("http://"));
            (host.is_none_or(|h| h.is_empty() || h.contains(char::is_whitespace)))
                .then_some("settings-text-refused-url")
        }
        "ai.model" => text
            .trim()
            .is_empty()
            .then_some("settings-text-refused-empty"),
        "general.new_note_folder"
        | "general.templates"
        | "general.daily_folder"
        | "general.daily_template"
        | "general.capture_note"
        | "ops.folder"
        | "editor.attachments" => folder(text).then_some("settings-text-refused-folder"),
        "ops.library_folders" => text
            .split(['\n', ','])
            .any(folder)
            .then_some("settings-text-refused-folder"),
        "ops.interface" => {
            let name = text.trim();
            (name.is_empty()
                || name.len() > 15
                || !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_.:-".contains(c)))
            .then_some("settings-text-refused-interface")
        }
        "general.daily_format" => {
            let format = text.trim();
            (format.is_empty() || format.contains(['/', '\\']) || !format.contains(['Y', 'M', 'D']))
                .then_some("settings-text-refused-format")
        }
        "ops.variables" => text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .any(|l| !variable_line(l))
            .then_some("settings-text-refused-variables"),
        _ => None,
    }
}

fn variable_line(line: &str) -> bool {
    let Some((name, _)) = line.split_once('=') else {
        return false;
    };
    let name = name.trim();
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub fn setting(key: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|s| s.key == key)
}

pub struct Prefs {
    values: HashMap<&'static str, Value>,
}

pub enum PrefsEvent {
    Changed(&'static str),
}

impl EventEmitter<PrefsEvent> for Prefs {}

impl Prefs {
    pub fn new() -> Self {
        Prefs {
            values: SETTINGS
                .iter()
                .filter_map(|s| s.default.clone().map(|v| (s.key, v)))
                .collect(),
        }
    }

    pub fn load(&mut self, values: Vec<(&'static str, Value)>) {
        for (key, value) in values {
            let Some(setting) = setting(key) else {
                continue;
            };
            let valid = Prefs::accepts(setting, &value);
            if valid {
                self.values.insert(key, value);
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn bool(&self, key: &str) -> bool {
        matches!(self.values.get(key), Some(Value::Bool(true)))
    }

    pub fn choice(&self, key: &str) -> usize {
        match self.values.get(key) {
            Some(Value::Choice(i)) => *i,
            _ => 0,
        }
    }

    pub fn text(&self, key: &str) -> String {
        match self.values.get(key) {
            Some(Value::Text(text)) => text.to_string(),
            _ => String::new(),
        }
    }

    pub fn accepts(setting: &Setting, value: &Value) -> bool {
        match (&setting.control, value) {
            (Control::Choice(options), Value::Choice(i)) => *i < options.len(),
            (Control::Number { min, max, .. }, Value::Number(n)) => (*min..=*max).contains(n),
            (Control::Toggle, Value::Bool(_))
            | (Control::Text { .. } | Control::Stored, Value::Text(_)) => true,
            _ => false,
        }
    }

    pub fn number(&self, key: &str) -> f64 {
        match self.values.get(key) {
            Some(Value::Number(n)) => *n,
            _ => 0.,
        }
    }

    pub fn is_default(&self, setting: &Setting) -> bool {
        setting.default.is_none() || setting.default.as_ref() == self.values.get(setting.key)
    }

    pub fn set(&mut self, key: &'static str, value: Value, cx: &mut Context<Self>) {
        if self.values.get(key) != Some(&value) {
            self.values.insert(key, value);
            cx.emit(PrefsEvent::Changed(key));
            cx.notify();
        }
    }

    pub fn reset(&mut self, setting: &'static Setting, cx: &mut Context<Self>) {
        if let Some(default) = setting.default.clone() {
            self.set(setting.key, default, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exam_options_follow_the_profiles() {
        let names: Vec<&str> = focus_ops::exam::profiles().iter().map(|p| p.name).collect();
        assert_eq!(&EXAM_PROFILES[1..], names.as_slice());
    }

    #[test]
    fn every_default_suits_its_control() {
        for setting in SETTINGS {
            if let Some(default) = &setting.default {
                assert!(Prefs::accepts(setting, default), "{}", setting.key);
            }
        }
    }

    #[test]
    fn every_row_has_its_texts_in_english() {
        let english = focus_i18n::audit::embedded("en");
        let mut missing = Vec::new();
        for setting in SETTINGS.iter().filter(|s| s.shown()) {
            let base = setting.text_base();
            let mut keys = vec![format!("{base}-label"), format!("{base}-desc")];
            if matches!(setting.control, Control::Action) {
                keys.push(format!("{base}-action"));
            }
            missing.extend(
                keys.into_iter()
                    .filter(|key| !english.messages.contains_key(key.as_str())),
            );
        }
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn every_key_written_in_the_settings_sources_exists_in_english() {
        let english = focus_i18n::audit::embedded("en");
        let mut missing = Vec::new();
        for source in [
            include_str!("mod.rs"),
            include_str!("ai_page.rs"),
            include_str!("history_page.rs"),
            include_str!("actions_editor.rs"),
            include_str!("mcp_editor.rs"),
            include_str!("shortcuts.rs"),
            include_str!("transfer.rs"),
        ] {
            for part in source.split("\"settings-").skip(1) {
                let name = part.split('"').next().unwrap_or_default();
                let key = format!("settings-{name}");
                let complete = name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                    && !name.ends_with('-')
                    && !name.is_empty();
                let element_id = matches!(name, "content" | "panel" | "scrollbar");
                if complete && !element_id && !english.messages.contains_key(key.as_str()) {
                    missing.push(key);
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn texts_are_refused_with_a_reason() {
        assert_eq!(
            text_refusal("ai.base_url", "ftp://x"),
            Some("settings-text-refused-url")
        );
        assert_eq!(
            text_refusal("ai.base_url", "https://api.deepseek.com"),
            None
        );
        assert_eq!(
            text_refusal("ai.base_url", "https://a b"),
            Some("settings-text-refused-url")
        );
        assert_eq!(
            text_refusal("ai.model", "  "),
            Some("settings-text-refused-empty")
        );
        for folder in ["/abs", "a/../b", "a\\b"] {
            assert_eq!(
                text_refusal("ops.folder", folder),
                Some("settings-text-refused-folder"),
                "{folder}"
            );
        }
        assert_eq!(text_refusal("editor.attachments", "./assets"), None);
        assert_eq!(
            text_refusal("ops.library_folders", "Red Team,\n../x"),
            Some("settings-text-refused-folder")
        );
        assert_eq!(text_refusal("ops.interface", "tun0"), None);
        assert_eq!(
            text_refusal("ops.interface", "a b"),
            Some("settings-text-refused-interface")
        );
        assert_eq!(text_refusal("general.daily_format", "YYYY-MM-DD"), None);
        assert_eq!(
            text_refusal("general.daily_format", "YYYY/MM"),
            Some("settings-text-refused-format")
        );
        assert_eq!(text_refusal("ops.variables", "DOMAIN=a\n\nLPORT = 1"), None);
        assert_eq!(
            text_refusal("ops.variables", "DOMAIN=a\nnot a variable"),
            Some("settings-text-refused-variables")
        );
        assert_eq!(text_refusal("ai.preprompt", "anything"), None);
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<&str> = SETTINGS.iter().map(|s| s.key).collect();
        keys.sort_unstable();
        let count = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), count);
    }
}
