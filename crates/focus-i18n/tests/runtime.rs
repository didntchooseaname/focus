use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use focus_i18n::{Arg, Direction};

static SERIAL: Mutex<()> = Mutex::new(());

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn language_switch_updates_text_direction_and_generation() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    assert!(focus_i18n::init(None).is_empty());
    focus_i18n::set_language("en").unwrap();
    let before = focus_i18n::generation();
    assert_eq!(focus_i18n::t("ui-cmd-open-note"), "Open a note");
    focus_i18n::set_language("fr").unwrap();
    assert!(focus_i18n::generation() > before);
    assert_eq!(focus_i18n::t("ui-cmd-open-note"), "Ouvrir une note");
    assert_eq!(focus_i18n::direction(), Direction::Ltr);
    focus_i18n::set_language("ar").unwrap();
    assert_eq!(focus_i18n::direction(), Direction::Rtl);
    focus_i18n::set_language("zh_cn.UTF-8").unwrap();
    assert_eq!(focus_i18n::language(), "zh-CN");
    focus_i18n::set_language("zh").unwrap();
    assert_eq!(focus_i18n::language(), "zh-CN");
    assert!(focus_i18n::set_language("tlh").is_err());
    let codes: Vec<String> = focus_i18n::available()
        .into_iter()
        .map(|l| l.code)
        .collect();
    assert_eq!(codes, ["en", "fr", "ar", "zh-CN"]);
    let names: Vec<String> = focus_i18n::available()
        .into_iter()
        .map(|l| l.native_name)
        .collect();
    assert_eq!(names, ["English", "Français", "العربية", "简体中文"]);
    focus_i18n::set_language("en").unwrap();
}

#[test]
fn missing_key_falls_back_then_returns_the_key() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("fallback");
    fs::create_dir_all(dir.join("fr")).unwrap();
    fs::write(
        dir.join("fr/ui.ftl"),
        "ui-cmd-open-note = Ouvrir (utilisateur)\n",
    )
    .unwrap();
    assert!(focus_i18n::init(Some(&dir)).is_empty());
    focus_i18n::set_language("fr").unwrap();
    focus_i18n::reset_missing_count();
    assert_eq!(focus_i18n::t("ui-cmd-open-note"), "Ouvrir (utilisateur)");
    assert_eq!(focus_i18n::t("ui-cmd-commands"), "Afficher les commandes");
    assert_eq!(focus_i18n::missing_count(), 0);
    assert_eq!(focus_i18n::t("no-such-key"), "no-such-key");
    assert_eq!(focus_i18n::missing_count(), 1);
    focus_i18n::init(None);
    focus_i18n::set_language("en").unwrap();
}

#[test]
fn user_folder_adds_a_language_and_reports_invalid_files() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("user-language");
    fs::create_dir_all(dir.join("es/templates/ops")).unwrap();
    fs::write(
        dir.join("es/ui.ftl"),
        "meta-native-name = Español\nui-cmd-open-note = Abrir una nota\n",
    )
    .unwrap();
    fs::write(dir.join("es/settings.ftl"), "settings-title = { \n").unwrap();
    fs::write(dir.join("es/templates/ops/finding.md"), "# { title }\n").unwrap();
    let diagnostics = focus_i18n::init(Some(&dir));
    assert_eq!(diagnostics.len(), 1);
    assert!(
        diagnostics[0].file.ends_with("es/settings.ftl"),
        "{diagnostics:?}"
    );
    let spanish = focus_i18n::available()
        .into_iter()
        .find(|l| l.code == "es")
        .expect("user language listed");
    assert_eq!(spanish.native_name, "Español");
    assert_eq!(spanish.direction, Direction::Ltr);
    focus_i18n::set_language("es").unwrap();
    assert_eq!(focus_i18n::t("ui-cmd-open-note"), "Abrir una nota");
    assert_eq!(focus_i18n::t("ui-cmd-commands"), "Show commands");
    assert_eq!(
        focus_i18n::template("ops/finding").as_deref(),
        Some("# { title }\n")
    );
    assert!(
        focus_i18n::template("ops/readme-lab")
            .unwrap()
            .contains("{name}")
    );
    focus_i18n::init(None);
    assert_eq!(focus_i18n::language(), "en");
}

#[test]
fn system_language_follows_the_environment_variables() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    focus_i18n::init(None);
    // SAFETY: the serial lock keeps the other tests of this binary from reading the environment.
    unsafe {
        std::env::set_var("LC_ALL", "fr_FR.UTF-8");
        assert_eq!(focus_i18n::detect_system(), "fr");
        std::env::set_var("LC_ALL", "ar_EG.UTF-8");
        assert_eq!(focus_i18n::detect_system(), "ar");
        std::env::set_var("LC_ALL", "de_DE.UTF-8");
        assert_eq!(focus_i18n::detect_system(), "en");
        std::env::set_var("LC_ALL", "C");
        std::env::remove_var("LC_MESSAGES");
        std::env::set_var("LANG", "zh_CN.UTF-8");
        assert_eq!(focus_i18n::detect_system(), "zh-CN");
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LANG");
        assert_eq!(focus_i18n::detect_system(), "en");
    }
}

#[test]
fn plurals_follow_each_language() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    focus_i18n::init(None);
    let count = |n: i64| focus_i18n::t_args("ui-notes-count", &[("count", Arg::Int(n))]);
    focus_i18n::set_language("en").unwrap();
    assert_eq!(count(1), "1 note");
    assert_eq!(count(2), "2 notes");
    assert_eq!(count(2545), "2,545 notes");
    focus_i18n::set_language("fr").unwrap();
    assert_eq!(count(0), "0 note");
    assert_eq!(count(1), "1 note");
    assert_eq!(count(2), "2 notes");
    assert_eq!(count(2545), "2\u{a0}545 notes");
    focus_i18n::set_language("ar").unwrap();
    let forms: Vec<String> = [0, 1, 2, 3, 11, 100].iter().map(|n| count(*n)).collect();
    let nouns: std::collections::BTreeSet<String> = forms
        .iter()
        .map(|f| {
            f.chars()
                .filter(|c| !c.is_ascii_digit() && !c.is_whitespace())
                .collect()
        })
        .collect();
    assert!(nouns.len() >= 5, "Arabic forms barely differ: {forms:?}");
    focus_i18n::set_language("zh-CN").unwrap();
    assert_eq!(count(1), count(2).replacen('2', "1", 1));
    focus_i18n::set_language("en").unwrap();
}
