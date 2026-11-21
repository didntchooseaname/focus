use std::ops::Range;

use focus_kit::Flow as _;
use focus_kit::{Icon, Kbd, Tokens, Type, ellipsis, floating, icon, size};
use gpui::{
    AnyElement, Bounds, FontWeight, HighlightStyle, InteractiveElement, IntoElement, ParentElement,
    Pixels, ScrollHandle, SharedString, StatefulInteractiveElement, Styled, div,
    prelude::FluentBuilder, px,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Format,
    Lists,
    Blocks,
    Links,
    Diagrams,
    Inserts,
    Ai,
    Ops,
}

impl Group {
    fn label(self) -> String {
        match self {
            Group::Format => focus_i18n::t("ui-slash-group-format"),
            Group::Lists => focus_i18n::t("ui-slash-group-lists"),
            Group::Blocks => focus_i18n::t("ui-slash-group-blocks"),
            Group::Links => focus_i18n::t("ui-slash-group-links"),
            Group::Diagrams => focus_i18n::t("ui-slash-group-diagrams"),
            Group::Inserts => focus_i18n::t("ui-slash-group-inserts"),
            Group::Ai => focus_i18n::t("ui-slash-group-ai"),
            Group::Ops => focus_i18n::t("ui-slash-group-ops"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Insert {
    Template(String),
    Date,
    Time,
    DateTime,
    Toc,
    Ai(String),
    Attachment,
    Language(String),
    Note {
        title: String,
        path: std::path::PathBuf,
    },
    ChooseTemplate,
    VaultTemplate(focus_core::NoteId),
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub group: Group,
    pub label: String,
    pub description: String,
    pub keywords: String,
    pub icon: Icon,
    pub shortcut: Option<&'static str>,
    pub insert: Insert,
}

fn entry(id: &str, group: Group, icon: Icon, insert: Insert) -> Entry {
    let t = |suffix: &str| focus_i18n::t(&format!("ui-slash-{id}-{suffix}"));
    Entry {
        id: id.to_string(),
        group,
        label: t("label"),
        description: t("desc"),
        keywords: t("keywords"),
        icon,
        shortcut: None,
        insert,
    }
}

fn template(t: &str) -> Insert {
    Insert::Template(t.to_string())
}

const CALLOUTS: [&str; 12] = [
    "note", "info", "tip", "warning", "danger", "example", "quote", "question", "success",
    "failure", "bug", "abstract",
];
const DIAGRAMS: [&str; 9] = [
    "flowchart",
    "sequence",
    "state",
    "class",
    "er",
    "gantt",
    "pie",
    "mindmap",
    "timeline",
];
const AI_ACTIONS: [(&str, &str, &str); 11] = [
    ("summary", "ai-action-summarize-label", "summarize"),
    ("refine", "ai-action-refine-label", "refine"),
    ("rephrase", "ai-action-rephrase-label", "rephrase"),
    ("fix", "ai-action-correct-label", "correct"),
    ("translate", "ui-slash-ai-translate-label", "translate"),
    ("continue", "ai-action-continue-label", "continue"),
    ("explain", "ai-action-explain-label", "explain"),
    ("tasks", "ai-action-tasks-label", "tasks"),
    ("title", "ai-action-title-tags-label", "title-tags"),
    ("diagram", "ai-action-mermaid-label", "mermaid"),
    ("ask", "ui-slash-ai-ask-label", "ask"),
];
const OPS_ACTIONS: [&str; 8] = [
    "journal",
    "finding",
    "credential",
    "host",
    "variable",
    "command",
    "cvss",
    "evidence",
];

pub fn catalog() -> Vec<Entry> {
    use Group::*;
    let t = focus_i18n::t;
    let mut out = Vec::new();
    for level in 1..=6u8 {
        out.push(Entry {
            id: format!("h{level}"),
            group: Format,
            label: focus_i18n::t_args(
                "ui-slash-heading-label",
                &[("level", u32::from(level).into())],
            ),
            description: t("ui-slash-heading-desc"),
            keywords: format!("{} h{level}", t("ui-slash-heading-keywords")),
            icon: Icon::HEADING,
            shortcut: None,
            insert: template(&format!("{} $0", "#".repeat(level as usize))),
        });
    }
    let mut bold = entry("bold", Format, Icon::BOLD, template("**$0**"));
    bold.shortcut = Some("ui-cmd-bold");
    let mut italic = entry("italic", Format, Icon::ITALIC, template("*$0*"));
    italic.shortcut = Some("ui-cmd-italic");
    out.extend([
        bold,
        italic,
        entry("strike", Format, Icon::STRIKETHROUGH, template("~~$0~~")),
        entry("mark", Format, Icon::HIGHLIGHTER, template("==$0==")),
        entry("code", Format, Icon::CODE, template("`$0`")),
        entry("quote", Format, Icon::TEXT_QUOTE, template("> $0")),
        entry("rule", Format, Icon::MINUS, template("---\n$0")),
        entry("bullets", Lists, Icon::LIST, template("- $0")),
        entry("numbers", Lists, Icon::LIST_ORDERED, template("1. $0")),
        entry("tasks", Lists, Icon::LIST_TODO, template("- [ ] $0")),
        entry("codeblock", Blocks, Icon::BRACES, template("```$0\n\n```")),
        entry(
            "table",
            Blocks,
            Icon::TABLE,
            template("| $0 | |\n| --- | --- |\n| | |"),
        ),
        entry(
            "tabs",
            Blocks,
            Icon::COLUMNS,
            template("```tabs\ntab: $0\n\ntab: \n\n```"),
        ),
    ]);
    for kind in CALLOUTS {
        let name = t(&format!("reading-callout-{kind}"));
        for (variant, key, mark) in [
            ("", "ui-slash-callout-label", ""),
            ("-open", "ui-slash-callout-open-label", "+"),
            ("-closed", "ui-slash-callout-closed-label", "-"),
        ] {
            out.push(Entry {
                id: format!("callout-{kind}{variant}"),
                group: Blocks,
                label: focus_i18n::t_args(key, &[("type", name.as_str().into())]),
                description: t("ui-slash-callout-desc"),
                keywords: format!("{} {kind}", t("ui-slash-callout-keywords")),
                icon: Icon::MESSAGE,
                shortcut: None,
                insert: template(&format!("> [!{kind}]{mark} $0\n> ")),
            });
        }
    }
    out.extend([
        entry(
            "details",
            Blocks,
            Icon::CHEVRON_RIGHT,
            template("<details>\n<summary>$0</summary>\n\n</details>"),
        ),
        entry("math", Blocks, Icon::SIGMA, template("$$\n$0\n$$")),
        entry("footnote", Blocks, Icon::FILE_TEXT, template("[^$0]")),
        entry("comment", Blocks, Icon::MESSAGE, template("%% $0 %%")),
        entry("wikilink", Links, Icon::LINK, template("[[$0]]")),
        entry("url", Links, Icon::EXTERNAL, template("[$0](https://)")),
        entry("image", Links, Icon::IMAGE, template("![$0]()")),
        entry("embed", Links, Icon::FILE_INPUT, template("![[$0]]")),
        entry("tag", Links, Icon::HASH, template("#$0")),
        entry("attachment", Links, Icon::PAPERCLIP, Insert::Attachment),
    ]);
    for id in DIAGRAMS {
        let skeleton = focus_i18n::template(&format!("slash/diagram-{id}")).unwrap_or_default();
        out.push(Entry {
            id: format!("mermaid-{id}"),
            group: Diagrams,
            label: t(&format!("ui-slash-diagram-{id}-label")),
            description: t("ui-slash-diagram-desc"),
            keywords: format!("{} {id}", t("ui-slash-diagram-keywords")),
            icon: Icon::WORKFLOW,
            shortcut: None,
            insert: template(&format!("```mermaid\n{}\n```\n$0", skeleton.trim_end())),
        });
    }
    out.extend([
        entry("date", Inserts, Icon::CALENDAR, Insert::Date),
        entry("time", Inserts, Icon::CLOCK, Insert::Time),
        entry("datetime", Inserts, Icon::CALENDAR, Insert::DateTime),
        entry("template", Inserts, Icon::FILE_PLUS, Insert::ChooseTemplate),
        entry(
            "property",
            Inserts,
            Icon::SLIDERS,
            template("---\n$0: \n---"),
        ),
        entry("toc", Inserts, Icon::LIST_TREE, Insert::Toc),
    ]);
    for (id, key, action) in AI_ACTIONS {
        out.push(Entry {
            id: format!("ai-{id}"),
            group: Ai,
            label: t(key),
            description: t("ui-slash-ai-desc"),
            keywords: t("ui-slash-ai-keywords"),
            icon: Icon::SPARKLES,
            shortcut: None,
            insert: Insert::Ai(action.to_string()),
        });
    }
    for id in OPS_ACTIONS {
        out.push(Entry {
            id: format!("ops-{id}"),
            group: Ops,
            label: t(&format!("ui-slash-ops-{id}-label")),
            description: t("ui-slash-ops-desc"),
            keywords: t("ui-slash-ops-keywords"),
            icon: Icon::CROSSHAIR,
            shortcut: None,
            insert: Insert::Unavailable,
        });
    }
    out
}

pub fn languages() -> Vec<Entry> {
    focus_md::highlight::languages()
        .iter()
        .map(|language| {
            let key = format!("reading-language-{}", language.id);
            let label = focus_i18n::t(&key);
            let label = if label == key {
                language.label.to_string()
            } else {
                label
            };
            Entry {
                id: format!("language-{}", language.id),
                group: Group::Blocks,
                label,
                description: language.id.to_string(),
                keywords: language.id.to_string(),
                icon: Icon::CODE,
                shortcut: None,
                insert: Insert::Language(language.id.to_string()),
            }
        })
        .collect()
}

pub fn vault_templates(templates: &[(focus_core::NoteId, std::path::PathBuf)]) -> Vec<Entry> {
    templates
        .iter()
        .map(|(id, path)| Entry {
            id: format!("template-{}", path.display()),
            group: Group::Inserts,
            label: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            description: path.display().to_string(),
            keywords: path.display().to_string(),
            icon: Icon::FILE_PLUS,
            shortcut: None,
            insert: Insert::VaultTemplate(*id),
        })
        .collect()
}

pub fn notes(library: &crate::library::Library, query: &str) -> (Vec<Entry>, Vec<Hit>) {
    let found = library.fuzzy(query, focus_search::FuzzyScope::Notes, &[], 50);
    let mut entries = Vec::new();
    let mut hits = Vec::new();
    for hit in found {
        let Some(meta) = hit.note.and_then(|id| library.note(id)) else {
            continue;
        };
        let chars: Vec<(usize, char)> = hit.text.char_indices().collect();
        let matched = hit
            .positions
            .iter()
            .filter_map(|p| {
                chars
                    .get(*p as usize)
                    .map(|(byte, c)| *byte..*byte + c.len_utf8())
            })
            .collect();
        hits.push(Hit {
            entry: entries.len(),
            matched,
        });
        entries.push(Entry {
            id: format!("note-{}", meta.path.display()),
            group: Group::Links,
            label: hit.text.clone(),
            description: meta
                .path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            keywords: String::new(),
            icon: Icon::FILE_TEXT,
            shortcut: None,
            insert: Insert::Note {
                title: meta.title.to_string(),
                path: meta.path.to_path_buf(),
            },
        });
    }
    (entries, hits)
}

fn fold(c: char) -> char {
    focus_core::strip_accent(c).to_ascii_lowercase()
}

pub fn score(query: &str, candidate: &str) -> Option<(i32, Vec<Range<usize>>)> {
    let query: Vec<char> = query
        .chars()
        .map(fold)
        .filter(|c| !c.is_whitespace())
        .collect();
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let chars: Vec<(usize, char)> = candidate.char_indices().collect();
    let mut qi = 0;
    let mut total = 0;
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut previous: Option<usize> = None;
    let mut first_at_word_start = false;
    for (i, &(byte, c)) in chars.iter().enumerate() {
        if qi < query.len() && fold(c) == query[qi] {
            let word_start = i == 0 || !chars[i - 1].1.is_alphanumeric();
            if qi == 0 {
                first_at_word_start = word_start;
            }
            total += 1
                + if word_start { 8 } else { 0 }
                + if previous == Some(i.wrapping_sub(1)) {
                    5
                } else {
                    0
                }
                + if i == 0 { 10 } else { 0 };
            let end = byte + c.len_utf8();
            match ranges.last_mut() {
                Some(last) if last.end == byte => last.end = end,
                _ => ranges.push(byte..end),
            }
            previous = Some(i);
            qi += 1;
        }
    }
    let scattered = query.len() >= 3 && !first_at_word_start;
    (qi == query.len() && !scattered).then(|| (total - chars.len() as i32 / 8, ranges))
}

pub struct Hit {
    pub entry: usize,
    pub matched: Vec<Range<usize>>,
}

pub fn filter(catalog: &[Entry], query: &str, recent: &[String]) -> Vec<Hit> {
    if query.trim().is_empty() {
        let mut hits: Vec<Hit> = recent
            .iter()
            .filter_map(|id| catalog.iter().position(|e| &e.id == id))
            .map(|entry| Hit {
                entry,
                matched: Vec::new(),
            })
            .collect();
        hits.extend(
            (0..catalog.len())
                .filter(|i| !recent.contains(&catalog[*i].id))
                .map(|entry| Hit {
                    entry,
                    matched: Vec::new(),
                }),
        );
        return hits;
    }
    let mut scored: Vec<(i32, Hit)> = catalog
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let on_label = score(query, &e.label);
            let on_keywords = e
                .keywords
                .split(' ')
                .filter_map(|k| score(query, k))
                .map(|(s, _)| s)
                .max();
            match (on_label, on_keywords) {
                (Some((s, matched)), k) => {
                    Some((s.max(k.unwrap_or(i32::MIN)), Hit { entry: i, matched }))
                }
                (None, Some(k)) => Some((
                    k - 2,
                    Hit {
                        entry: i,
                        matched: Vec::new(),
                    },
                )),
                (None, None) => None,
            }
        })
        .collect();
    scored.sort_by_key(|(s, hit)| (-s, matches!(catalog[hit.entry].insert, Insert::Unavailable)));
    scored.into_iter().map(|(_, hit)| hit).collect()
}

const ROW: Pixels = px(30.);
const GROUP_ROW: Pixels = px(24.);
const WIDTH: Pixels = px(320.);
const VISIBLE: f32 = 8.;
pub const VISIBLE_ROWS: usize = VISIBLE as usize;

pub fn child_of(
    catalog: &[Entry],
    grouped: bool,
    hits: &[Hit],
    query: &str,
    rank: usize,
) -> Option<usize> {
    hits.get(rank)?;
    if !(grouped && query.trim().is_empty()) {
        return Some(rank);
    }
    let mut last = None;
    let titles = hits[..=rank]
        .iter()
        .filter(|hit| last.replace(catalog[hit.entry].group) != Some(catalog[hit.entry].group))
        .count();
    Some(rank + titles)
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    t: &Tokens,
    catalog: &[Entry],
    grouped: bool,
    hits: &[Hit],
    selected: usize,
    query: &str,
    anchor: Bounds<Pixels>,
    viewport: gpui::Size<Pixels>,
    scroll: &ScrollHandle,
    on_pick: impl Fn(usize, &mut gpui::Window, &mut gpui::App) + Clone + 'static,
    on_dismiss: impl Fn(&mut gpui::Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let grouped = grouped && query.trim().is_empty();
    let shown = hits.len().clamp(1, VISIBLE as usize);
    let titles = if grouped {
        let mut last = None;
        hits.iter()
            .take(shown)
            .filter(|hit| last.replace(catalog[hit.entry].group) != Some(catalog[hit.entry].group))
            .count()
    } else {
        0
    };
    let height = ROW * shown as f32 + GROUP_ROW * titles as f32 + size::S1 * 2. + px(2.);
    let below = anchor.bottom() + height + px(8.) < viewport.height;
    let top = if below {
        anchor.bottom() + px(4.)
    } else {
        anchor.top() - height - px(6.)
    };
    let mut rows: Vec<AnyElement> = Vec::new();
    if hits.is_empty() {
        rows.push(
            div()
                .h(ROW)
                .px(size::S3)
                .flow_row()
                .items_center()
                .ui()
                .text_color(t.ink_3)
                .child(focus_i18n::t_args(
                    "ui-slash-no-match",
                    &[("query", query.trim().into())],
                ))
                .into_any_element(),
        );
    }
    let mut last_group = None;
    for (n, hit) in hits.iter().enumerate() {
        let e = &catalog[hit.entry];
        if grouped && last_group != Some(e.group) {
            last_group = Some(e.group);
            rows.push(
                div()
                    .h(GROUP_ROW)
                    .px(size::S3)
                    .flow_row()
                    .items_end()
                    .pb(px(4.))
                    .ui_xs()
                    .text_color(t.ink_3)
                    .child(e.group.label())
                    .into_any_element(),
            );
        }
        let disabled = matches!(e.insert, Insert::Unavailable);
        let current = n == selected;
        let label: AnyElement = if hit.matched.is_empty() {
            ellipsis(e.label.clone()).into_any_element()
        } else {
            ellipsis(e.label.clone())
                .highlights(hit.matched.iter().map(|r| {
                    (
                        r.clone(),
                        HighlightStyle {
                            color: Some(t.ink),
                            font_weight: Some(FontWeight::SEMIBOLD),
                            ..Default::default()
                        },
                    )
                }))
                .into_any_element()
        };
        let pick = on_pick.clone();
        rows.push(
            div()
                .id(("slash", n))
                .relative()
                .flow_row()
                .items_center()
                .gap(size::S2)
                .h(ROW)
                .px(size::S3)
                .ui()
                .text_color(if disabled { t.ink_4 } else { t.ink_2 })
                .when(current, |row| {
                    row.bg(t.card_3).child(
                        div()
                            .absolute()
                            .start_at(px(0.))
                            .top_0()
                            .bottom_0()
                            .w(px(2.))
                            .bg(t.ink),
                    )
                })
                .when(!current && !disabled, |row| {
                    row.cursor_pointer().hover(move |s| s.bg(t.card_2))
                })
                .child(icon(e.icon, size::ICON_DENSE).text_color(if disabled {
                    t.ink_4
                } else {
                    t.ink_3
                }))
                .child(div().flex_none().max_w(px(150.)).child(label))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flow_row()
                        .ui_sm()
                        .text_color(if disabled { t.ink_4 } else { t.ink_3 })
                        .child(ellipsis(SharedString::from(e.description.clone()))),
                )
                .when_some(
                    e.shortcut
                        .map(crate::actions::shortcut)
                        .filter(|keys| !keys.is_empty()),
                    |row, keys| row.child(Kbd::new(keys)),
                )
                .when(!disabled, |row| {
                    row.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        pick(n, window, cx)
                    })
                })
                .into_any_element(),
        );
    }
    div()
        .absolute()
        .start_at(if focus_kit::Dir::current().rtl {
            viewport.width - anchor.right()
        } else {
            anchor.left()
        })
        .top(top)
        .w(WIDTH)
        .child(
            floating(t)
                .id("slash-menu")
                .occlude()
                .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                .on_mouse_down_out(move |_, window, cx| on_dismiss(window, cx))
                .max_h(height)
                .overflow_y_scroll()
                .track_scroll(scroll)
                .py(size::S1)
                .children(rows),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{catalog, child_of, filter, score};

    #[test]
    fn a_row_is_drawn_after_the_titles_of_the_groups_above_it() {
        focus_i18n::set_language("en").expect("English is embedded");
        let entries = catalog();
        let all = filter(&entries, "", &[]);
        let lists = all
            .iter()
            .position(|hit| entries[hit.entry].id == "bullets")
            .expect("the catalog lists bullets");
        assert_eq!(child_of(&entries, true, &all, "", 0), Some(1));
        assert_eq!(child_of(&entries, true, &all, "", lists), Some(lists + 2));
        assert_eq!(child_of(&entries, false, &all, "", lists), Some(lists));
        let found = filter(&entries, "bul", &[]);
        assert_eq!(child_of(&entries, true, &found, "bul", 0), Some(0));
        assert_eq!(child_of(&entries, true, &all, "", all.len()), None);
    }

    #[test]
    fn subsequence_scores_word_starts() {
        assert!(score("tab", "Tableau").is_some());
        assert!(score("tbl", "Tableau").is_some());
        assert!(score("xyz", "Tableau").is_none());
        let (prefix, _) = score("ta", "Tableau").expect("prefix");
        let (inner, _) = score("ta", "Bloc de code avec étiquette").unwrap_or((i32::MIN, vec![]));
        assert!(prefix > inner);
    }

    #[test]
    fn table_is_found_in_three_spellings() {
        focus_i18n::set_language("fr").expect("French is embedded");
        let entries = catalog();
        for query in ["tab", "table", "tableau"] {
            let hits = filter(&entries, query, &[]);
            assert_eq!(entries[hits[0].entry].id, "table", "query {query}");
        }
    }
}
