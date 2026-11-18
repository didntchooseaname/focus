use std::sync::LazyLock;

const CATALOG: &str = include_str!("../data/cwe.tsv");

const COMMON: &[(u32, &str)] = &[
    (79, "xss cross-site scripting"),
    (89, "sqli sql injection"),
    (22, "path traversal directory traversal lfi"),
    (78, "os command injection rce"),
    (94, "code injection rce"),
    (77, "command injection"),
    (352, "csrf cross-site request forgery"),
    (918, "ssrf server-side request forgery"),
    (611, "xxe xml external entity"),
    (434, "unrestricted file upload"),
    (502, "deserialization"),
    (798, "hardcoded credentials password"),
    (287, "authentication"),
    (306, "missing authentication"),
    (862, "missing authorization idor"),
    (863, "incorrect authorization idor"),
    (269, "privilege escalation"),
    (200, "information disclosure exposure"),
    (601, "open redirect"),
    (521, "weak password"),
    (787, "out-of-bounds write buffer overflow"),
    (125, "out-of-bounds read"),
    (416, "use after free"),
    (476, "null pointer"),
    (190, "integer overflow"),
    (20, "input validation"),
    (400, "resource consumption denial of service dos"),
    (119, "memory buffer"),
    (327, "weak cryptography"),
    (319, "cleartext transmission"),
    (312, "cleartext storage"),
    (384, "session fixation"),
    (426, "untrusted search path"),
    (732, "incorrect permission assignment"),
    (1021, "clickjacking"),
];

#[derive(Debug, PartialEq, Eq)]
pub struct Weakness {
    pub id: u32,
    pub name: &'static str,
}

static ALL: LazyLock<Vec<Weakness>> = LazyLock::new(|| {
    CATALOG
        .lines()
        .filter_map(|line| {
            let (id, name) = line.split_once('\t')?;
            Some(Weakness {
                id: id.parse().ok()?,
                name,
            })
        })
        .collect()
});

pub fn all() -> &'static [Weakness] {
    &ALL
}

pub fn name(id: u32) -> Option<&'static str> {
    ALL.binary_search_by_key(&id, |w| w.id)
        .ok()
        .map(|i| ALL[i].name)
}

pub fn parse_id(text: &str) -> Option<u32> {
    let text = text.trim();
    let digits = match text.get(..3) {
        Some(prefix) if prefix.eq_ignore_ascii_case("cwe") => text[3..]
            .strip_prefix(['-', ':', ' ', '_'])
            .unwrap_or(&text[3..])
            .trim_start(),
        _ => text,
    };
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
        .filter(|id| *id > 0)
}

pub fn format_id(id: u32) -> String {
    format!("CWE-{id}")
}

pub fn normalize(text: &str) -> Option<String> {
    parse_id(text).map(format_id)
}

pub fn search(query: &str, limit: usize) -> Vec<&'static Weakness> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return COMMON
            .iter()
            .filter_map(|(id, _)| ALL.binary_search_by_key(id, |w| w.id).ok().map(|i| &ALL[i]))
            .take(limit)
            .collect();
    }
    let number = query.strip_prefix("cwe").map_or(query.as_str(), |rest| {
        rest.trim_start_matches(['-', ':', ' ', '_'])
    });
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut hits: Vec<(u32, &Weakness)> = ALL
        .iter()
        .filter_map(|weakness| {
            let id_text = weakness.id.to_string();
            let name = weakness.name.to_lowercase();
            let alias = COMMON
                .iter()
                .find(|(id, _)| *id == weakness.id)
                .map_or("", |(_, words)| *words);
            if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) {
                return if id_text == number {
                    Some((1000, weakness))
                } else {
                    id_text.starts_with(number).then_some((500, weakness))
                };
            }
            let mut score = 0;
            for word in &words {
                let points = if alias.split_whitespace().any(|a| a == *word) {
                    60
                } else if name.starts_with(word) {
                    40
                } else if name
                    .split(|c: char| !c.is_alphanumeric())
                    .any(|part| part.starts_with(word))
                    || alias.contains(word)
                {
                    20
                } else if name.contains(word) {
                    10
                } else {
                    return None;
                };
                score += points;
            }
            Some((score + if alias.is_empty() { 0 } else { 5 }, weakness))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    hits.into_iter().take(limit).map(|(_, w)| w).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_the_mitre_list_sorted_by_identifier() {
        assert!(all().len() > 900);
        assert!(all().windows(2).all(|pair| pair[0].id < pair[1].id));
        assert_eq!(
            name(79),
            Some("Improper Neutralization of Input During Web Page Generation")
        );
        assert_eq!(name(4_000_000), None);
    }

    #[test]
    fn every_common_weakness_exists_in_the_catalog() {
        for (id, _) in COMMON {
            assert!(name(*id).is_some(), "CWE-{id}");
        }
    }

    #[test]
    fn identifiers_are_read_in_every_written_form() {
        for text in ["CWE-79", "cwe-79", "CWE 79", "CWE:79", " 79 ", "cwe_79"] {
            assert_eq!(normalize(text).as_deref(), Some("CWE-79"), "{text}");
        }
        assert_eq!(normalize("CWE-9999999").as_deref(), Some("CWE-9999999"));
        for text in ["", "CWE", "CWE-", "CWE-x1", "0", "XSS", "79a", "CWE--79"] {
            assert_eq!(normalize(text), None, "{text}");
        }
    }

    #[test]
    fn a_search_finds_by_identifier_by_name_and_by_field_word() {
        let first = |query: &str| search(query, 5).first().map(|w| w.id);
        assert_eq!(first("79"), Some(79));
        assert_eq!(first("CWE-89"), Some(89));
        assert_eq!(first("xss"), Some(79));
        assert_eq!(first("sql injection"), Some(89));
        assert_eq!(first("ssrf"), Some(918));
        assert_eq!(first("hard-coded"), Some(798));
        assert!(search("pathname", 5).iter().any(|w| w.id == 22));
        assert!(search("zzzzqq", 5).is_empty());
        let by_prefix: Vec<u32> = search("7", 50).iter().map(|w| w.id).collect();
        assert_eq!(by_prefix.first(), Some(&7));
        assert!(by_prefix.iter().all(|id| id.to_string().starts_with('7')));
    }

    #[test]
    fn nothing_typed_lists_the_common_ones_first() {
        let ids: Vec<u32> = search("", 3).iter().map(|w| w.id).collect();
        assert_eq!(ids, [79, 89, 22]);
    }
}
