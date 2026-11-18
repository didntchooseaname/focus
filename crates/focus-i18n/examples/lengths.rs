use focus_i18n::audit::embedded;

fn main() {
    let english = embedded("en");
    for code in ["fr", "ar"] {
        let other = embedded(code);
        let mut longer = 0;
        for (key, en) in english.messages.iter().filter(|(key, m)| {
            (m.domain == "ui" || m.domain == "settings") && !key.ends_with("-keywords")
        }) {
            let Some(translated) = other.messages.get(key) else {
                continue;
            };
            let (en_len, len) = (en.text.chars().count(), translated.text.chars().count());
            if en_len >= 6 && len as f64 > en_len as f64 * 1.5 {
                println!("{code} {key}: {len} characters against {en_len} in en");
                longer += 1;
            }
        }
        println!("{code}: {longer} keys longer than 1.5 times en");
    }
}
