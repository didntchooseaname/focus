use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct OpsSettings {
    pub default_variables: Vec<(String, String)>,
    pub interface: String,
    pub mask_secrets: bool,
    pub exam_profile: Option<String>,
    pub alert_minutes: u32,
    pub author: String,
    pub library_folders: Vec<PathBuf>,
    pub substitute: bool,
    pub code_variables: bool,
}

impl Default for OpsSettings {
    fn default() -> Self {
        OpsSettings {
            default_variables: Vec::new(),
            interface: "tun0".into(),
            mask_secrets: true,
            exam_profile: None,
            alert_minutes: 60,
            author: String::new(),
            library_folders: Vec::new(),
            substitute: true,
            code_variables: true,
        }
    }
}

pub fn parse_variables(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (name, value) = line.split_once('=')?;
            let name = name.trim();
            (!name.is_empty()).then(|| (name.to_string(), value.trim().to_string()))
        })
        .collect()
}

fn folders(text: &str) -> Vec<PathBuf> {
    text.split(['\n', ','])
        .map(|part| part.trim().trim_matches('/'))
        .filter(|part| !part.is_empty() && !part.split('/').any(|segment| segment == ".."))
        .map(PathBuf::from)
        .collect()
}

impl OpsSettings {
    pub fn from_core(settings: &focus_core::settings::Settings) -> OpsSettings {
        let stored = |key: &str| settings.ui.get("settings").and_then(|s| s.get(key));
        let defaults = OpsSettings::default();
        let text = |key: &str| stored(key).and_then(serde_json::Value::as_str);
        OpsSettings {
            default_variables: text("ops.variables")
                .map(parse_variables)
                .unwrap_or_default(),
            interface: text("ops.interface")
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map_or(defaults.interface, str::to_string),
            mask_secrets: stored("ops.mask")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(defaults.mask_secrets),
            exam_profile: stored("ops.exam")
                .and_then(serde_json::Value::as_u64)
                .and_then(|i| i.checked_sub(1))
                .and_then(|i| focus_ops::exam::profiles().get(i as usize))
                .map(|p| p.id.to_string()),
            alert_minutes: stored("ops.alert")
                .and_then(serde_json::Value::as_f64)
                .map_or(defaults.alert_minutes, |m| m.clamp(1., 1000.) as u32),
            author: text("ops.author")
                .map(|a| a.trim().to_string())
                .unwrap_or_default(),
            library_folders: text("ops.library_folders").map(folders).unwrap_or_default(),
            substitute: stored("ops.substitute")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(defaults.substitute),
            code_variables: stored("ops.code_variables")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(defaults.code_variables),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(values: serde_json::Value) -> focus_core::settings::Settings {
        let mut settings = focus_core::settings::Settings::default();
        settings.ui.insert("settings".into(), values);
        settings
    }

    #[test]
    fn missing_values_keep_their_defaults() {
        assert_eq!(
            OpsSettings::from_core(&focus_core::settings::Settings::default()),
            OpsSettings::default()
        );
    }

    #[test]
    fn values_of_the_settings_window_are_read() {
        let settings = core(serde_json::json!({
            "ops.variables": "DOMAIN = corp.local\nbad line\n=x\nLPORT=4444",
            "ops.interface": " wg0 ",
            "ops.mask": false,
            "ops.exam": 3,
            "ops.alert": 15.0,
            "ops.author": " A. Operator ",
            "ops.library_folders": "Red Team/, Cheatsheets\n../outside",
            "ops.substitute": false,
            "ops.code_variables": false,
        }));
        let read = OpsSettings::from_core(&settings);
        assert_eq!(
            read.default_variables,
            [
                ("DOMAIN".into(), "corp.local".into()),
                ("LPORT".into(), "4444".into())
            ]
        );
        assert_eq!(
            (
                read.interface.as_str(),
                read.mask_secrets,
                read.alert_minutes,
                read.author.as_str()
            ),
            ("wg0", false, 15, "A. Operator")
        );
        assert_eq!(read.exam_profile.as_deref(), Some("oswe"));
        assert_eq!(
            read.library_folders,
            [PathBuf::from("Red Team"), PathBuf::from("Cheatsheets")]
        );
        assert!(!read.substitute);
        assert!(OpsSettings::default().substitute);
        assert!(!read.code_variables);
        assert!(OpsSettings::default().code_variables);
    }
}
