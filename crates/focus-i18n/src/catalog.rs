use std::collections::HashMap;
use std::path::{Path, PathBuf};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentResource, FluentValue};
use intl_memoizer::concurrent::IntlLangMemoizer;
use unic_langid::LanguageIdentifier;

use crate::{Direction, LanguageInfo};

pub type Bundle = FluentBundle<FluentResource>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: PathBuf,
    pub message: String,
}

pub struct Locale {
    pub info: LanguageInfo,
    pub bundle: Bundle,
    pub templates: HashMap<String, String>,
}

struct Embedded {
    code: &'static str,
    files: &'static [(&'static str, &'static str)],
    templates: &'static [(&'static str, &'static str)],
}

macro_rules! domain {
    ($code:literal, $domain:literal) => {
        (
            $domain,
            include_str!(concat!("../../../locales/", $code, "/", $domain, ".ftl")),
        )
    };
}

macro_rules! template {
    ($code:literal, $name:literal) => {
        (
            $name,
            include_str!(concat!(
                "../../../locales/",
                $code,
                "/templates/",
                $name,
                ".md"
            )),
        )
    };
}

macro_rules! embedded {
    ($code:literal) => {
        Embedded {
            code: $code,
            files: &[
                domain!($code, "ui"),
                domain!($code, "settings"),
                domain!($code, "reading"),
                domain!($code, "search"),
                domain!($code, "graph"),
                domain!($code, "ai"),
                domain!($code, "ops"),
                domain!($code, "errors"),
                domain!($code, "export"),
                domain!($code, "history"),
                domain!($code, "help"),
            ],
            templates: &[
                template!($code, "ops/checklist-exam"),
                template!($code, "ops/checklist-mission"),
                template!($code, "ops/finding"),
                template!($code, "ops/readme-exam"),
                template!($code, "ops/readme-lab"),
                template!($code, "ops/readme-pentest"),
                template!($code, "slash/diagram-flowchart"),
                template!($code, "slash/diagram-sequence"),
                template!($code, "slash/diagram-state"),
                template!($code, "slash/diagram-class"),
                template!($code, "slash/diagram-er"),
                template!($code, "slash/diagram-gantt"),
                template!($code, "slash/diagram-pie"),
                template!($code, "slash/diagram-mindmap"),
                template!($code, "slash/diagram-timeline"),
                template!($code, "help/index"),
                template!($code, "help/getting-started"),
                template!($code, "help/keyboard-shortcuts"),
                template!($code, "help/search-syntax"),
                template!($code, "help/slash-menu"),
                template!($code, "help/links-and-embeds"),
                template!($code, "help/graph"),
                template!($code, "help/ai-and-privacy"),
                template!($code, "help/ops-mode"),
                template!($code, "help/connect-an-agent"),
            ],
        }
    };
}

const EMBEDDED: [Embedded; 4] = [
    embedded!("en"),
    embedded!("fr"),
    embedded!("ar"),
    embedded!("zh-CN"),
];

pub const FALLBACK: &str = "en";

pub fn embedded_domains(code: &str) -> Vec<(&'static str, &'static str)> {
    EMBEDDED
        .iter()
        .find(|e| e.code == code)
        .map(|e| e.files.to_vec())
        .unwrap_or_default()
}

fn number_formatter<const GROUP: char, const DECIMAL: char>(
    value: &FluentValue,
    _: &IntlLangMemoizer,
) -> Option<String> {
    match value {
        FluentValue::Number(n) => Some(crate::format::number_text(n.value, 3, GROUP, DECIMAL)),
        _ => None,
    }
}

fn set_number_formatter(bundle: &mut Bundle, code: &str) {
    let primary = code.split('-').next().unwrap_or(code);
    bundle.set_formatter(Some(if primary == "fr" {
        number_formatter::<'\u{a0}', ','>
    } else {
        number_formatter::<',', '.'>
    }));
}

fn new_bundle(code: &str) -> Bundle {
    let id: LanguageIdentifier = code.parse().unwrap_or_default();
    let mut bundle = Bundle::new_concurrent(vec![id]);
    bundle.set_use_isolating(false);
    set_number_formatter(&mut bundle, code);
    bundle
}

fn parse(label: &Path, text: String, diagnostics: &mut Vec<Diagnostic>) -> Option<FluentResource> {
    match FluentResource::try_new(text) {
        Ok(resource) => Some(resource),
        Err((_, errors)) => {
            let detail: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
            diagnostics.push(Diagnostic {
                file: label.to_path_buf(),
                message: format!("invalid Fluent syntax, file ignored: {}", detail.join("; ")),
            });
            None
        }
    }
}

fn read_dir_sorted(dir: &Path, extension: &str, diagnostics: &mut Vec<Diagnostic>) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            diagnostics.push(Diagnostic {
                file: dir.to_path_buf(),
                message: format!("cannot read directory: {error}"),
            });
            return Vec::new();
        }
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == extension))
        .collect();
    files.sort();
    files
}

fn collect_templates(
    root: &Path,
    dir: &Path,
    out: &mut HashMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_templates(root, &path, out, diagnostics);
        } else if path.extension().is_some_and(|x| x == "md") {
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let name = path.strip_prefix(root).unwrap_or(&path).with_extension("");
                    out.insert(name.to_string_lossy().replace('\\', "/"), text);
                }
                Err(error) => diagnostics.push(Diagnostic {
                    file: path,
                    message: format!("cannot read template: {error}"),
                }),
            }
        }
    }
}

fn read_info(code: &str, bundle: &Bundle) -> LanguageInfo {
    let text = |key: &str| {
        let message = bundle.get_message(key)?;
        let pattern = message.value()?;
        let mut errors = Vec::new();
        Some(
            bundle
                .format_pattern(pattern, None, &mut errors)
                .into_owned(),
        )
    };
    let native_name = text("meta-native-name")
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| code.to_string());
    let direction = if text("meta-direction").as_deref() == Some("rtl") {
        Direction::Rtl
    } else {
        Direction::Ltr
    };
    LanguageInfo {
        code: code.to_string(),
        native_name,
        direction,
    }
}

fn finish(code: &str, bundle: Bundle, templates: HashMap<String, String>) -> Locale {
    let info = read_info(code, &bundle);
    Locale {
        info,
        bundle,
        templates,
    }
}

pub fn load(user_dir: Option<&Path>) -> (Vec<Locale>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut locales: Vec<Locale> = Vec::new();
    for embedded in &EMBEDDED {
        let mut bundle = new_bundle(embedded.code);
        for (domain, text) in embedded.files {
            let label = PathBuf::from(format!("locales/{}/{domain}.ftl", embedded.code));
            if let Some(resource) = parse(&label, (*text).to_string(), &mut diagnostics)
                && let Err(errors) = bundle.add_resource(resource)
            {
                for error in errors {
                    diagnostics.push(Diagnostic {
                        file: label.clone(),
                        message: error.to_string(),
                    });
                }
            }
        }
        let templates = embedded
            .templates
            .iter()
            .map(|(name, text)| ((*name).to_string(), (*text).to_string()))
            .collect();
        locales.push(Locale {
            info: LanguageInfo {
                code: embedded.code.into(),
                native_name: String::new(),
                direction: Direction::Ltr,
            },
            bundle,
            templates,
        });
    }

    let mut user_codes: Vec<(String, PathBuf)> = Vec::new();
    if let Some(dir) = user_dir.filter(|d| d.is_dir())
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir()
                && let Some(code) = path.file_name().and_then(|n| n.to_str())
            {
                user_codes.push((code.to_string(), path));
            }
        }
        user_codes.sort();
    }

    for (code, dir) in user_codes {
        let index = match locales.iter().position(|l| l.info.code == code) {
            Some(index) => index,
            None => {
                locales.push(Locale {
                    info: LanguageInfo {
                        code: code.clone(),
                        native_name: String::new(),
                        direction: Direction::Ltr,
                    },
                    bundle: new_bundle(&code),
                    templates: HashMap::new(),
                });
                locales.len() - 1
            }
        };
        let locale = &mut locales[index];
        for file in read_dir_sorted(&dir, "ftl", &mut diagnostics) {
            match std::fs::read_to_string(&file) {
                Ok(text) => {
                    if let Some(resource) = parse(&file, text, &mut diagnostics) {
                        locale.bundle.add_resource_overriding(resource);
                    }
                }
                Err(error) => diagnostics.push(Diagnostic {
                    file,
                    message: format!("cannot read file: {error}"),
                }),
            }
        }
        let templates_dir = dir.join("templates");
        collect_templates(
            &templates_dir,
            &templates_dir,
            &mut locale.templates,
            &mut diagnostics,
        );
    }

    let locales = locales
        .into_iter()
        .map(|l| {
            let code = l.info.code.clone();
            finish(&code, l.bundle, l.templates)
        })
        .collect();
    (locales, diagnostics)
}
