use std::time::Duration;

use focus_ai::Usage;
use focus_view_ai::fake::{Beat, FakeError};

fn usage(prompt: u32, completion: u32, hit: u32) -> Beat {
    Beat::Usage(Usage {
        prompt_tokens: prompt,
        completion_tokens: completion,
        reasoning_tokens: 0,
        cache_hit_tokens: hit,
        cache_miss_tokens: prompt - hit,
    })
}

fn pick<'a>(lang: &str, en: &'a str, fr: &'a str, ar: &'a str) -> &'a str {
    match lang {
        "fr" => fr,
        "ar" => ar,
        _ => en,
    }
}

const REASONING_EN: &str = "The note has a frontmatter block, an introduction, an enumeration section and a cracking section. The user asks for a summary and a better frontmatter, so I list the sections with their status and propose the metadata.";

pub fn answer(lang: &str) -> Vec<Vec<Beat>> {
    let text = pick(
        lang,
        "The note is a technique page in the **ready** state. It covers four sections:\n\n| Section | Content | Status |\n|---|---|---|\n| Introduction | Why the technique works | ready |\n| Enumeration | Tools to list the accounts | ready |\n| Cracking | Offline recovery | draft |\n| Detection | Not written yet | missing |\n\nA frontmatter that carries the verification date makes the page easier to filter:\n\n```yaml\ntype: technique\nstatus: ready\nlast_verified: 2026-11-12\n```\n\nSee [[00 Accueil]] for the index.",
        "La note est une page de technique à l'état **ready**. Elle compte quatre sections :\n\n| Section | Contenu | Statut |\n|---|---|---|\n| Introduction | Pourquoi la technique fonctionne | ready |\n| Énumération | Outils pour lister les comptes | ready |\n| Cassage | Récupération hors ligne | draft |\n| Détection | Pas encore écrite | manquante |\n\nUn frontmatter qui porte la date de vérification facilite le filtrage :\n\n```yaml\ntype: technique\nstatus: ready\nlast_verified: 2026-11-12\n```\n\nVoir [[00 Accueil]] pour l'index.",
        "الملاحظة صفحة تقنية في حالة **ready**. تضم أربعة أقسام:\n\n| القسم | المحتوى | الحالة |\n|---|---|---|\n| مقدمة | سبب نجاح التقنية | ready |\n| الاستكشاف | أدوات سرد الحسابات | ready |\n| الاختراق | الاستعادة دون اتصال | draft |\n| الكشف | لم يُكتب بعد | مفقود |\n\nإضافة تاريخ التحقق إلى الواجهة الأمامية تسهّل التصفية:\n\n```yaml\ntype: technique\nstatus: ready\nlast_verified: 2026-11-12\n```\n\nانظر [[00 Accueil]] للفهرس.",
    );
    let reasoning = pick(
        lang,
        REASONING_EN,
        "La note a un frontmatter, une introduction, une section d'énumération et une de cassage. L'utilisateur veut un résumé et un meilleur frontmatter : je liste les sections avec leur statut et je propose les métadonnées.",
        "للملاحظة واجهة أمامية ومقدمة وقسم للاستكشاف وقسم للاختراق. يريد المستخدم ملخصًا وواجهة أمامية أفضل، فأسرد الأقسام مع حالتها وأقترح البيانات الوصفية.",
    );
    vec![vec![
        Beat::Reasoning(reasoning.into()),
        Beat::Pause(Duration::from_millis(300)),
        Beat::Content(text.into()),
        usage(1840, 236, 1536),
    ]]
}

pub fn streaming() -> Vec<Vec<Beat>> {
    let reasoning = "The question is about the sections of the current note. I read the headings first, then compare them with the status in the frontmatter. The enumeration section is complete, the cracking section is a draft, and the detection section does not exist. ";
    vec![vec![
        Beat::Reasoning(reasoning.into()),
        Beat::Content("The note has four sections. Two are ready and one is still a draft:\n\n- Introduction: ready\n- Enumeration: ready\n- Cracking: draft\n- Detection: ".into()),
        Beat::Pause(Duration::from_secs(120)),
    ]]
}

pub fn tools() -> Vec<Vec<Beat>> {
    vec![vec![
        Beat::Tool { name: "search_notes".into(), arguments: r#"{"query":"kerberoasting","limit":5}"#.into() },
        Beat::Tool { name: "read_note".into(), arguments: r#"{"path":"Red Team/Kerberoasting.md","end_line":24}"#.into() },
        Beat::Tool { name: "get_backlinks".into(), arguments: r#"{"path":"Red Team/Kerberoasting.md"}"#.into() },
        Beat::Content("The only note about the technique is `Red Team/Kerberoasting.md`. It links back to the index and to a sibling note on AS-REP roasting.".into()),
        usage(3120, 74, 2816),
    ]]
}

pub fn write() -> Vec<Vec<Beat>> {
    vec![vec![
        Beat::Tool { name: "read_note".into(), arguments: r#"{"path":"Red Team/Kerberoasting.md","end_line":12}"#.into() },
        Beat::Reasoning("The detection section is missing. I append a short one at the end of the note.".into()),
        Beat::Tool { name: "append_to_note".into(), arguments: r###"{"path":"Red Team/Kerberoasting.md","content":"## Detection\n\n- Alert on service ticket requests that use weak encryption types.\n- Review accounts that have a service principal name."}"###.into() },
        Beat::Content("The detection section is added at the end of the note.".into()),
        usage(2410, 92, 2048),
    ]]
}

pub fn failure(kind: &str) -> Vec<Vec<Beat>> {
    let error = match kind {
        "key" => FakeError::InvalidKey,
        "balance" => FakeError::InsufficientBalance,
        "rate" => FakeError::RateLimited(42),
        "server" => FakeError::Server(503),
        _ => FakeError::Network("connection timed out after 60 s".into()),
    };
    vec![vec![Beat::Fail(error)]]
}

pub fn masking() -> Vec<Vec<Beat>> {
    vec![vec![Beat::Content("The host ‹IP_1› answers on the service port. The model only saw ‹SECRET_1›, restored here.".into()), usage(910, 31, 0)]]
}

pub fn action_result() -> Vec<Vec<Beat>> {
    vec![vec![Beat::Content("```md\n# Kerberoasting\n\nAny domain user can request a service ticket for an account that has a service principal name. The ticket is encrypted with a key derived from the account password, so it can be attacked offline.\n```".into()), usage(640, 58, 512)]]
}

pub fn models() -> Vec<String> {
    ["deepseek-flash", "deepseek-v4-pro", "deepseek-reasoner"]
        .map(String::from)
        .to_vec()
}

pub fn long() -> Vec<Vec<Beat>> {
    let text: String = (1..=60).map(|n| format!("Paragraph {n} of a long answer. It keeps the same sentence so that the layout cost is the same for every paragraph in the thread.\n\n")).collect();
    vec![vec![Beat::Content(text), usage(2200, 1400, 1024)]]
}

pub fn arabic_wrap() -> Vec<Vec<Beat>> {
    vec![vec![Beat::Content("الاستكشاف يعتمد على أدوات سرد الحسابات واستعادة المعلومات دون اتصال بالشبكة.\n\n| القسم | المحتوى |\n|---|---|\n| الاستكشاف | أدوات سرد الحسابات |\n| الاختراق | الاستعادة دون اتصال |".into())]]
}

pub fn named(name: &str, lang: &str) -> Option<Vec<Vec<Beat>>> {
    Some(match name {
        "answer" => answer(lang),
        "streaming" => streaming(),
        "tools" => tools(),
        "write" => write(),
        "masking" => masking(),
        "action" => action_result(),
        "long" => long(),
        "arabic-wrap" => arabic_wrap(),
        other => failure(other.strip_prefix("error-")?),
    })
}
