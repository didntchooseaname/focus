use focus_ai::mcp::ServerConfig;

pub fn with_secrets(
    config: &ServerConfig,
    load: impl Fn(&str, &str) -> Option<String>,
) -> ServerConfig {
    let mut config = config.clone();
    for (name, value) in config.env.iter_mut() {
        if value.is_empty()
            && let Some(secret) = load(&config.name, name)
        {
            *value = secret;
        }
    }
    config.env.retain(|_, value| !value.is_empty());
    config
}

fn servers(json: &str) -> Vec<ServerConfig> {
    serde_json::from_str(json).unwrap_or_default()
}

pub fn has_plaintext(json: &str) -> bool {
    servers(json)
        .iter()
        .any(|s| s.env.values().any(|v| !v.is_empty()))
}

pub fn without_values(json: &str) -> String {
    let mut list = servers(json);
    for server in &mut list {
        server.env.values_mut().for_each(String::clear);
    }
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
}

pub struct Migration {
    pub json: String,
    pub moved: usize,
    pub left: usize,
}

pub fn migrate_plaintext(
    json: &str,
    store: impl Fn(&str, &str, &str) -> anyhow::Result<()>,
) -> Option<Migration> {
    if !has_plaintext(json) {
        return None;
    }
    let mut list = servers(json);
    let (mut moved, mut left) = (0, 0);
    for server in &mut list {
        let name = server.name.clone();
        for (variable, value) in server.env.iter_mut().filter(|(_, v)| !v.is_empty()) {
            match store(&name, variable, value) {
                Ok(()) => {
                    value.clear();
                    moved += 1;
                }
                Err(_) => left += 1,
            }
        }
    }
    Some(Migration {
        json: serde_json::to_string(&list).unwrap_or_else(|_| json.to_string()),
        moved,
        left,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const JSON: &str = r#"[{"name":"docs","command":"d","env":{"TOKEN":"abc","MODE":""}},{"name":"b","command":"x"}]"#;

    #[test]
    fn values_are_found_blanked_and_filled_back() {
        assert!(has_plaintext(JSON));
        let blank = without_values(JSON);
        assert!(!blank.contains("abc") && !has_plaintext(&blank));
        let config = &servers(&blank)[0];
        let filled = with_secrets(config, |server, name| {
            (server == "docs" && name == "TOKEN").then(|| "abc".to_string())
        });
        assert_eq!(filled.env.get("TOKEN").map(String::as_str), Some("abc"));
        assert!(
            !filled.env.contains_key("MODE"),
            "a variable with no stored value is not passed"
        );
    }

    #[test]
    fn migration_moves_what_the_keyring_accepts_and_counts_the_rest() {
        let stored = RefCell::new(Vec::new());
        let moved = migrate_plaintext(JSON, |server, name, value| {
            stored
                .borrow_mut()
                .push((server.to_string(), name.to_string(), value.to_string()));
            Ok(())
        })
        .expect("a value to move");
        assert_eq!((moved.moved, moved.left), (1, 0));
        assert_eq!(
            stored.into_inner(),
            [("docs".to_string(), "TOKEN".to_string(), "abc".to_string())]
        );
        assert!(!moved.json.contains("abc"));
        assert!(
            migrate_plaintext(&moved.json, |_, _, _| Ok(())).is_none(),
            "nothing left to move"
        );

        let refused = migrate_plaintext(JSON, |_, _, _| anyhow::bail!("no keyring"))
            .expect("a value to move");
        assert_eq!((refused.moved, refused.left), (0, 1));
        assert!(
            refused.json.contains("abc"),
            "a value the keyring refused stays where it was"
        );
    }
}
