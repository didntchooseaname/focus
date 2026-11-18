use std::path::Path;

pub(crate) fn link_note_paths(text: &str, exists: &dyn Fn(&Path) -> bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    let bytes = text.as_bytes();
    let mut search = 0;
    while let Some(found) = text[search..].find(".md") {
        let end = search + found + 3;
        search = end;
        if bytes
            .get(end)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            continue;
        }
        let Some(from) = word_starts(text, cursor, end)
            .into_iter()
            .find(|s| exists(Path::new(&text[*s..end])))
        else {
            continue;
        };
        if text[..from].ends_with("[[") {
            continue;
        }
        let path = &text[from..end];
        let (open_tick, close_tick) = (text[..from].ends_with('`'), text[end..].starts_with('`'));
        let (replace_from, replace_to) = if open_tick && close_tick {
            (from - 1, end + 1)
        } else {
            (from, end)
        };
        out.push_str(&text[cursor..replace_from]);
        out.push_str(&format!("[[{}]]", path.strip_suffix(".md").unwrap_or(path)));
        cursor = replace_to;
        search = replace_to;
    }
    out.push_str(&text[cursor..]);
    out
}

fn word_starts(text: &str, floor: usize, end: usize) -> Vec<usize> {
    let from = floor.max(end.saturating_sub(160));
    let is_break = |c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '`' | '"' | '\'' | '(' | ')' | '[' | ']' | '<' | '>' | '*' | ',' | ';'
            )
    };
    let mut starts: Vec<usize> = text[..end]
        .char_indices()
        .filter(|(i, c)| *i >= from && is_break(*c))
        .map(|(i, c)| i + c.len_utf8())
        .collect();
    if from == 0 {
        starts.insert(0, 0);
    }
    starts.retain(|s| *s < end);
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(path: &Path) -> bool {
        matches!(
            path.to_str(),
            Some("Red Team/Kerberoasting.md" | "Home.md" | "Red Team/Two words.md")
        )
    }

    #[test]
    fn a_cited_path_becomes_a_wiki_link() {
        assert_eq!(
            link_note_paths("See Red Team/Kerberoasting.md for more.", &known),
            "See [[Red Team/Kerberoasting]] for more."
        );
        assert_eq!(
            link_note_paths("In `Home.md`, and (Home.md).", &known),
            "In [[Home]], and ([[Home]])."
        );
    }

    #[test]
    fn paths_with_spaces_and_existing_links_are_handled() {
        assert_eq!(
            link_note_paths("Read `Red Team/Two words.md` now", &known),
            "Read [[Red Team/Two words]] now"
        );
        assert_eq!(
            link_note_paths("Already [[Home.md]] linked", &known),
            "Already [[Home.md]] linked"
        );
    }

    #[test]
    fn unknown_paths_and_lookalikes_are_untouched() {
        assert_eq!(
            link_note_paths("The file nothing.md and Home.mdx stay", &known),
            "The file nothing.md and Home.mdx stay"
        );
        assert_eq!(link_note_paths("no path here", &known), "no path here");
    }
}
