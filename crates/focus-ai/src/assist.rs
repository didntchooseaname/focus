use super::*;

#[derive(Debug, Clone, Default)]
pub struct VaultHint {
    pub folders: Vec<String>,
    pub tags: Vec<String>,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Expansion {
    pub queries: Vec<String>,
    pub intent: String,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub index: u32,
    pub title: String,
    pub path: String,
    pub excerpts: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Ranked {
    pub index: u32,
    pub relevance: f32,
    pub reason: String,
}

const MAX_CANDIDATES: usize = 40;
const MAX_EXCERPTS: usize = 3;
const MAX_EXCERPT_CHARS: usize = 600;
const MAX_QUERIES: usize = 8;

pub fn expand(
    client: &Client,
    question: &str,
    hint: &VaultHint,
    cancel: &Cancel,
) -> Result<(Expansion, Usage), Error> {
    let question = question.trim();
    if question.is_empty() {
        return Ok((Expansion::default(), Usage::default()));
    }
    let mut context = String::new();
    if !hint.folders.is_empty() {
        context.push_str(&format!("Folders: {}\n", hint.folders.join(", ")));
    }
    if !hint.tags.is_empty() {
        context.push_str(&format!(
            "Frequent tags: {}\n",
            hint.tags
                .iter()
                .take(50)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !hint.languages.is_empty() {
        context.push_str(&format!("Note languages: {}\n", hint.languages.join(", ")));
    }
    let user = format!("{context}\nQuestion: {question}");
    let (text, usage) = json_call(client, EXPAND_SYSTEM, &user, 500, cancel)?;
    Ok((parse_expansion(&text), usage))
}

const EXPAND_SYSTEM: &str = "You prepare a search in a vault of Markdown notes. From the question, produce queries for a full text search engine. Syntax: words separated by spaces (all required), \"exact phrase\", OR between alternatives, -word to exclude, tag:#name, path:folder, title:word, heading:word, file:name.\n\nSuggest 3 to 8 short, varied queries: synonyms, translations between the languages of the vault, names of neighbouring tools or techniques, common abbreviations. Use tag: and path: only when the folders and tags provided justify it. Do not invent tags.\n\nAnswer with a single JSON object of the form {\"queries\": [\"query 1\", \"query 2\"], \"intent\": \"short rewording of what is searched for, one sentence, in the language of the question\"}.";

fn parse_expansion(text: &str) -> Expansion {
    let Some(value) = extract_json(text) else {
        return Expansion::default();
    };
    let mut queries: Vec<String> = Vec::new();
    if let Some(list) = value.get("queries").and_then(serde_json::Value::as_array) {
        for query in list.iter().filter_map(serde_json::Value::as_str) {
            let query = query.trim();
            if !query.is_empty() && !queries.iter().any(|q| q.eq_ignore_ascii_case(query)) {
                queries.push(query.to_string());
            }
            if queries.len() == MAX_QUERIES {
                break;
            }
        }
    }
    let intent = value
        .get("intent")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    Expansion { queries, intent }
}

pub fn rerank(
    client: &Client,
    question: &str,
    candidates: &[Candidate],
    cancel: &Cancel,
) -> Result<(Vec<Ranked>, Usage), Error> {
    let candidates = &candidates[..candidates.len().min(MAX_CANDIDATES)];
    if candidates.is_empty() {
        return Ok((Vec::new(), Usage::default()));
    }
    let user = format!(
        "Question: {}\n\nCandidates:\n\n{}",
        question.trim(),
        format_candidates(candidates)
    );
    let (text, usage) = json_call(client, RERANK_SYSTEM, &user, 2000, cancel)?;
    Ok((parse_ranking(&text, candidates), usage))
}

const RERANK_SYSTEM: &str = "You rank notes by how useful they are to answer a question. Each candidate has a number in square brackets, a title, a path and excerpts. Judge on the excerpts and the title only.\n\nFor each note that helps answer, give its relevance from 0 to 1 (1: answers directly) and one sentence saying why, in the language of the question. Omit off topic notes. Do not answer the question.\n\nAnswer with a single JSON object of the form {\"ranked\": [{\"index\": 3, \"relevance\": 0.9, \"reason\": \"explains the requested technique\"}]}, sorted by decreasing relevance.";

fn parse_ranking(text: &str, candidates: &[Candidate]) -> Vec<Ranked> {
    let Some(value) = extract_json(text) else {
        return Vec::new();
    };
    let Some(list) = value.get("ranked").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let mut ranked: Vec<Ranked> = Vec::new();
    for item in list {
        let Some(index) = item
            .get("index")
            .and_then(serde_json::Value::as_u64)
            .and_then(|i| u32::try_from(i).ok())
        else {
            continue;
        };
        if !candidates.iter().any(|c| c.index == index) || ranked.iter().any(|r| r.index == index) {
            continue;
        }
        let relevance = item
            .get("relevance")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0) as f32;
        if relevance <= 0.0 {
            continue;
        }
        let reason = item
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        ranked.push(Ranked {
            index,
            relevance,
            reason,
        });
    }
    ranked.sort_by(|a, b| b.relevance.total_cmp(&a.relevance));
    ranked
}

pub fn answer(
    client: &Client,
    question: &str,
    candidates: &[Candidate],
    cancel: &Cancel,
    on_event: &mut dyn FnMut(StreamEvent),
) -> Result<Message, Error> {
    let candidates = &candidates[..candidates.len().min(MAX_CANDIDATES)];
    let user = format!(
        "Notes:\n\n{}\nQuestion: {}",
        format_candidates(candidates),
        question.trim()
    );
    let request = ChatRequest {
        messages: vec![
            message(Role::System, ANSWER_SYSTEM),
            message(Role::User, &user),
        ],
        tools: Vec::new(),
        json: false,
    };
    client
        .derive(|c| {
            c.thinking = false;
            c.temperature = Some(0.2);
            c.max_tokens = Some(c.max_tokens.unwrap_or(1200).min(1200));
        })
        .chat_stream(&request, cancel, on_event)
}

const ANSWER_SYSTEM: &str = "You answer a question from note excerpts. Each note has a number in square brackets. Base every claim on a note and follow it with its citation in the form [n], for example [2] or [1][4]. Use only these excerpts: no outside knowledge presented as coming from the notes. If the excerpts do not answer the question, say so in one sentence and say what is missing. Answer in 6 sentences at most, in the language of the question, without a preamble.";

fn format_candidates(candidates: &[Candidate]) -> String {
    let mut out = String::new();
    for candidate in candidates {
        out.push_str(&format!(
            "[{}] {} ({})\n",
            candidate.index,
            candidate.title.trim(),
            candidate.path
        ));
        for excerpt in candidate.excerpts.iter().take(MAX_EXCERPTS) {
            let excerpt: String = excerpt.trim().chars().take(MAX_EXCERPT_CHARS).collect();
            out.push_str(&format!("> {}\n", excerpt.replace('\n', "\n> ")));
        }
        out.push('\n');
    }
    out
}

fn message(role: Role, content: &str) -> Message {
    Message {
        role,
        content: content.to_string(),
        reasoning: None,
        tool_calls: Vec::new(),
        tool_call_id: None,
    }
}

fn json_call(
    client: &Client,
    system: &str,
    user: &str,
    max_tokens: u32,
    cancel: &Cancel,
) -> Result<(String, Usage), Error> {
    let request = ChatRequest {
        messages: vec![message(Role::System, system), message(Role::User, user)],
        tools: Vec::new(),
        json: true,
    };
    let short = client.derive(|c| {
        c.thinking = false;
        c.temperature = Some(0.2);
        c.max_tokens = Some(max_tokens);
    });
    let mut usage = Usage::default();
    let message = short.chat_stream(&request, cancel, &mut |event| {
        if let StreamEvent::Usage(counted) = event {
            usage = counted;
        }
    })?;
    Ok((message.content, usage))
}

fn extract_json(text: &str) -> Option<serde_json::Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

pub fn citations(answer: &str) -> Vec<(std::ops::Range<usize>, u32)> {
    static CITATION: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\[(\d{1,4})\]").expect("citation pattern"));
    CITATION
        .captures_iter(answer)
        .filter_map(|c| {
            let whole = c.get(0)?;
            if answer[whole.end()..].starts_with('(') || answer[whole.end()..].starts_with(':') {
                return None;
            }
            Some((whole.range(), c[1].parse().ok()?))
        })
        .collect()
}
