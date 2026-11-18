use focus_i18n::audit::embedded;

fn main() {
    let reference = embedded("en");
    let mut problems = reference.problems.len();
    for problem in &reference.problems {
        println!("en: {problem}");
    }
    for code in ["fr", "ar", "zh-CN"] {
        let other = embedded(code);
        let missing: Vec<&String> = reference
            .messages
            .keys()
            .filter(|k| !other.messages.contains_key(*k))
            .collect();
        let extra: Vec<&String> = other
            .messages
            .keys()
            .filter(|k| !reference.messages.contains_key(*k))
            .collect();
        let differing: Vec<_> = reference
            .messages
            .iter()
            .filter_map(|(key, en)| {
                other
                    .messages
                    .get(key)
                    .filter(|m| m.variables != en.variables)
                    .map(|m| (key, &en.variables, &m.variables))
            })
            .collect();
        println!(
            "{code}: {} keys, {} missing, {} extra, {} with different variables, {} syntax problems",
            other.messages.len(),
            missing.len(),
            extra.len(),
            differing.len(),
            other.problems.len()
        );
        for key in &missing {
            println!("  missing {key}");
        }
        for key in &extra {
            println!("  extra {key}");
        }
        for (key, en, translated) in &differing {
            println!("  variables {key}: en {en:?}, {code} {translated:?}");
        }
        for problem in &other.problems {
            println!("  {problem}");
        }
        problems += missing.len() + extra.len() + differing.len() + other.problems.len();
    }
    if problems > 0 {
        std::process::exit(1);
    }
}
