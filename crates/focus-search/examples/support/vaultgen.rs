use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    pub fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

pub const FR: &[&str] = &[
    "élévation",
    "privilèges",
    "réseau",
    "accès",
    "contrôleur",
    "domaine",
    "énumération",
    "mot",
    "passe",
    "utilisateur",
    "serveur",
    "requête",
    "sécurité",
    "attaque",
    "défense",
    "journal",
    "événement",
    "système",
    "fichier",
    "dossier",
    "clé",
    "chiffrement",
    "certificat",
    "écoute",
    "la",
    "le",
    "les",
    "des",
    "une",
    "un",
    "pour",
    "avec",
    "dans",
    "sur",
    "par",
    "est",
    "sont",
    "qui",
    "que",
    "pas",
    "plus",
    "cette",
    "être",
    "vérifier",
    "exécuter",
    "obtenir",
    "récupérer",
    "configurer",
    "déployer",
    "tester",
    "noter",
    "après",
    "avant",
    "ensuite",
    "puis",
    "très",
];

pub const EN: &[&str] = &[
    "kerberos",
    "ticket",
    "golden",
    "silver",
    "hash",
    "relay",
    "token",
    "service",
    "account",
    "privilege",
    "escalation",
    "lateral",
    "movement",
    "the",
    "of",
    "and",
    "to",
    "in",
    "is",
    "for",
    "with",
    "on",
    "that",
    "this",
    "by",
    "from",
    "use",
    "run",
    "check",
    "dump",
    "enumerate",
    "payload",
    "shell",
    "reverse",
    "listener",
    "proxy",
    "tunnel",
    "pivot",
    "credential",
    "spray",
    "brute",
    "force",
    "scan",
    "port",
    "host",
    "windows",
    "linux",
    "registry",
    "process",
    "memory",
    "kernel",
    "exploit",
    "vulnerability",
    "patch",
    "update",
    "firewall",
    "rule",
];

pub const TAGS: &[&str] = &[
    "red-team",
    "blue-team",
    "ctf",
    "ad",
    "ad/kerberos",
    "ad/adcs",
    "web",
    "web/sqli",
    "linux/privesc",
    "windows/privesc",
    "osint",
    "tools",
    "attack/T1558",
    "draft",
    "index",
];
pub const DIRS: &[&str] = &["Red", "Labs", "Blue", "Projects", "Library", "Journal"];
pub const SUBDIRS: &[&str] = &[
    "Active Directory",
    "Web",
    "Linux",
    "Windows",
    "Réseau",
    "Outils",
    "Notes",
    "Labs",
    "Certifications",
    "Écrits",
];
pub const CODE: &[&str] = &[
    "nmap -sC -sV -oA scan $RHOST",
    "impacket-GetUserSPNs $DOMAIN/$USER:$PASS -request",
    "crackmapexec smb $RHOST -u users.txt -p pass.txt",
    "Get-ADUser -Filter * -Properties ServicePrincipalName",
    "curl -s http://$RHOST/ | grep -i token",
];

const SYLLABLES: &[&str] = &[
    "ra", "tion", "ble", "men", "que", "ver", "sé", "té", "pri", "con", "dé", "ment", "lo", "gi",
    "ar", "ex", "in", "ter", "pro", "ca", "li", "sa", "mo", "ri", "nu", "ta", "é", "or", "il",
    "ku", "zan", "phi", "sto", "glo", "vré", "mi", "no", "bu", "shi", "tor", "wen", "dra", "plu",
    "cè",
];

const VOCAB: usize = 40_000;

pub fn synthetic(k: usize) -> String {
    let mut rng = Rng::new(k as u64 * 7919 + 13);
    let n = 2 + rng.below(3);
    (0..n).map(|_| *rng.pick(SYLLABLES)).collect()
}

fn zipf(rng: &mut Rng) -> usize {
    let u = (rng.next() >> 11) as f64 / (1u64 << 53) as f64;
    ((VOCAB as f64).powf(u) as usize)
        .saturating_sub(1)
        .min(VOCAB - 1)
}

fn word(rng: &mut Rng) -> String {
    match rng.below(100) {
        0..45 => rng.pick(FR).to_string(),
        45..75 => rng.pick(EN).to_string(),
        _ => synthetic(zipf(rng)),
    }
}

fn sentence(rng: &mut Rng, out: &mut String, words: usize) {
    for i in 0..words {
        let w = word(rng);
        if i == 0 {
            let mut c = w.chars();
            if let Some(f) = c.next() {
                out.extend(f.to_uppercase());
                out.push_str(c.as_str());
            }
        } else {
            out.push(' ');
            out.push_str(&w);
        }
    }
    out.push('.');
}

pub fn note_path(i: usize) -> PathBuf {
    let d = DIRS[i % DIRS.len()];
    let s = SUBDIRS[(i / DIRS.len()) % SUBDIRS.len()];
    let bucket = i / (DIRS.len() * SUBDIRS.len() * 40);
    PathBuf::from(d)
        .join(s)
        .join(format!("Lot {bucket:03}"))
        .join(format!("{} {i}.md", title_words(i)))
}

fn title_words(i: usize) -> String {
    let mut rng = Rng::new(i as u64 ^ 0xA5A5);
    let a = rng.pick(EN);
    let b = synthetic(zipf(&mut rng));
    format!("{a} {b}")
}

pub fn note_text(i: usize, total: usize) -> String {
    let mut rng = Rng::new(i as u64 + 1);
    let mut s = String::with_capacity(2048);
    let title = title_words(i);
    s.push_str("---\n");
    let _ = writeln!(s, "title: \"{} {i}\"", title);
    let ntags = 1 + rng.below(3);
    s.push_str("tags:\n");
    for _ in 0..ntags {
        let _ = writeln!(s, "  - {}", rng.pick(TAGS));
    }
    let _ = writeln!(s, "status: {}", rng.pick(&["draft", "ready", "review"]));
    let _ = writeln!(
        s,
        "type: {}",
        rng.pick(&["note", "index", "lab", "writeup"])
    );
    if rng.chance(10) {
        let _ = writeln!(s, "aliases: [{} {}]", rng.pick(EN), i);
    }
    s.push_str("---\n\n");
    let _ = writeln!(s, "# {} {i}\n", title);
    let here = note_path(i);
    let here_dir = here.parent().unwrap_or(Path::new(""));
    let sections = 1 + rng.below(4);
    for _ in 0..sections {
        let _ = write!(s, "## ");
        let n = 2 + rng.below(3);
        sentence(&mut rng, &mut s, n);
        s.push_str("\n\n");
        let paras = 1 + rng.below(3);
        for _ in 0..paras {
            let n = 1 + rng.below(4);
            for _ in 0..n {
                let n = 6 + rng.below(12);
                sentence(&mut rng, &mut s, n);
                s.push(' ');
            }
            if rng.chance(30) {
                let _ = write!(s, "#{} ", rng.pick(TAGS));
            }
            if rng.chance(50) {
                let j = rng.below(total);
                let target = note_path(j);
                if rng.chance(60) {
                    let rel = relative(here_dir, &target);
                    let _ = write!(s, "Voir [{}]({}).", title_words(j), rel.replace(' ', "%20"));
                } else {
                    let stem = target
                        .file_stem()
                        .map(|x| x.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let _ = write!(s, "See [[{stem}]].");
                }
            }
            s.push_str("\n\n");
        }
        if rng.chance(35) {
            for _ in 0..1 + rng.below(4) {
                s.push_str(if rng.chance(40) { "- [x] " } else { "- [ ] " });
                let n = 3 + rng.below(5);
                sentence(&mut rng, &mut s, n);
                s.push('\n');
            }
            s.push('\n');
        }
        if rng.chance(30) {
            let _ = writeln!(s, "```bash\n{}\n```\n", rng.pick(CODE));
        }
        if rng.chance(25) {
            let _ = write!(
                s,
                "> [!tip] {}\n> ",
                rng.pick(&["Astuce", "Note", "Warning"])
            );
            sentence(&mut rng, &mut s, 8);
            s.push_str("\n\n");
        }
    }
    s
}

fn relative(from_dir: &Path, to: &Path) -> String {
    let a: Vec<_> = from_dir.components().collect();
    let b: Vec<_> = to.components().collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut parts: Vec<String> = Vec::new();
    for _ in common..a.len() {
        parts.push("..".into());
    }
    for c in &b[common..] {
        parts.push(c.as_os_str().to_string_lossy().into_owned());
    }
    parts.join("/")
}

const GEN_VERSION: u32 = 3;

pub fn generate(root: &Path, count: usize) -> std::io::Result<bool> {
    let marker = root.join(format!(".focusgen-v{GEN_VERSION}-{count}"));
    if marker.exists() {
        return Ok(false);
    }
    let generated_before = std::fs::read_dir(root)
        .map(|rd| {
            rd.flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with(".focusgen-"))
        })
        .unwrap_or(false);
    if generated_before {
        std::fs::remove_dir_all(root)?;
    }
    std::fs::create_dir_all(root.join(".obsidian"))?;
    std::fs::write(
        root.join(".obsidian/app.json"),
        r#"{"attachmentFolderPath":"./assets","useMarkdownLinks":true,"newLinkFormat":"relative"}"#,
    )?;
    use rayon::prelude::*;
    (0..count).into_par_iter().try_for_each(|i| {
        let p = root.join(note_path(i));
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(p, note_text(i, count))
    })?;
    std::fs::write(marker, b"")?;
    Ok(true)
}
