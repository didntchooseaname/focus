use focus_core::settings;
use std::path::PathBuf;

#[test]
fn directories_follow_environment_and_settings_round_trip() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/focus-core-tests/settings-env");
    let _ = std::fs::remove_dir_all(&base);
    let config = base.join("config");
    let cache = base.join("cache");
    // SAFETY: no other thread reads the environment during this test, alone in its
    // binary.
    unsafe {
        std::env::set_var("FOCUS_CONFIG_DIR", &config);
        std::env::set_var("FOCUS_CACHE_DIR", &cache);
    }
    assert_eq!(settings::config_dir(), config);
    let vault_cache = settings::cache_dir_for(std::path::Path::new("/coffre/inexistant"));
    assert_eq!(vault_cache.parent(), Some(cache.as_path()));

    let defaults = settings::load().unwrap();
    assert_eq!(defaults.theme, "dark");

    let mut s = defaults;
    s.theme = "light".into();
    s.recent_vaults = vec!["/a".into(), "/b".into()];
    s.attachments.per_note = false;
    s.ui.insert("largeur.gauche".into(), serde_json::json!(280));
    settings::save(&s).unwrap();
    assert!(config.join("settings.json").exists());
    let back = settings::load().unwrap();
    assert_eq!(back.theme, "light");
    assert_eq!(back.recent_vaults, s.recent_vaults);
    assert!(!back.attachments.per_note);
    assert_eq!(back.ui.get("largeur.gauche"), Some(&serde_json::json!(280)));

    std::fs::write(
        config.join("settings.json"),
        r#"{"theme":"light","unknown":1}"#,
    )
    .unwrap();
    assert_eq!(settings::load().unwrap().reading_font_size, 15.0);
    std::fs::write(config.join("settings.json"), "{pas du json").unwrap();
    assert!(settings::load().is_err());
}
