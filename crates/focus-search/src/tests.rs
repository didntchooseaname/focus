use super::*;
use focus_core::OpenOptions;
use std::path::PathBuf;

#[path = "../examples/support/vaultgen.rs"]
#[allow(dead_code)]
mod vaultgen;

fn test_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/focus-search-tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("répertoire d'essai sous target/");
    dir
}

fn put(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

fn open(dir: &Path, root: &Path) -> (Arc<Vault>, Arc<SearchIndex>) {
    let vault = Vault::open(
        root,
        OpenOptions {
            cache_dir: dir.join("cache"),
            ignore: Vec::new(),
            watch: false,
        },
    )
    .unwrap();
    let index = SearchIndex::open(vault.clone(), &dir.join("cache"), &|_| {}).unwrap();
    (vault, index)
}

fn all() -> SearchOptions {
    SearchOptions {
        max_files: usize::MAX,
        budget_ms: u32::MAX,
        ..Default::default()
    }
}

fn paths(v: &Vault, r: &SearchResults) -> Vec<String> {
    let mut out: Vec<String> = r
        .hits
        .iter()
        .map(|h| v.note(h.note).unwrap().path.to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

fn q(v: &Vault, ix: &SearchIndex, query: &str) -> Vec<String> {
    paths(
        v,
        &ix.search(&parse_query(query), &all(), &CancelToken::new()),
    )
}

fn small_vault(name: &str) -> (PathBuf, PathBuf) {
    let dir = test_dir(name);
    let root = dir.join("vault");
    put(
        &root,
        "Red_Team/Kerberoasting.md",
        "---\nstatus: ready\ntags: [ad/kerberos]\n---\n# Kerberoasting\n\n## Énumération des SPN\nL'élévation de privilèges passe par un ticket.\n- [ ] lancer GetUserSPNs\n- [x] relever les hash\n",
    );
    put(
        &root,
        "Red_Team/Golden.md",
        "# Golden ticket\nUn TICKET forgé. Kerberos encore.\n```\n#not-a-tag\n```\n",
    );
    put(
        &root,
        "Sandbox/Box Elevation.md",
        "---\nstatus: draft\n---\n# Box\nelevation sans accent, Elevation majuscule.\nmot entier: tickets ticket.\n",
    );
    put(
        &root,
        "Tools_Blue/Logs.md",
        "# Logs\nÉvénements 4769 : demande de ticket de service.\nkerberos #blue\n",
    );
    (dir, root)
}

#[test]
fn query_semantics() {
    let (dir, root) = small_vault("semantics");
    let (v, ix) = open(&dir, &root);
    assert_eq!(
        q(&v, &ix, "elevation"),
        ["Red_Team/Kerberoasting.md", "Sandbox/Box Elevation.md"],
        "without accent: also finds élévation"
    );
    assert_eq!(
        q(&v, &ix, "élévation"),
        ["Red_Team/Kerberoasting.md"],
        "with accent: exact"
    );
    assert_eq!(
        q(&v, &ix, "Elevation"),
        ["Sandbox/Box Elevation.md"],
        "upper case: case respected"
    );
    let accents = SearchOptions {
        accent_sensitive: true,
        ..all()
    };
    let exact = ix.search(&parse_query("elevation"), &accents, &CancelToken::new());
    assert_eq!(
        paths(&v, &exact),
        ["Sandbox/Box Elevation.md"],
        "accents respected: élévation no longer matches"
    );
    assert_eq!(
        (exact.total_files, exact.total_matches),
        (1, 2),
        "the totals follow the option"
    );
    assert_eq!(
        paths(
            &v,
            &ix.search(&parse_query("élévation"), &accents, &CancelToken::new())
        ),
        ["Red_Team/Kerberoasting.md"]
    );
    assert_eq!(
        paths(
            &v,
            &ix.search(
                &parse_query("file:elevation"),
                &accents,
                &CancelToken::new()
            )
        ),
        ["Sandbox/Box Elevation.md"]
    );
    assert_eq!(q(&v, &ix, "TICKET"), ["Red_Team/Golden.md"]);
    assert_eq!(
        q(&v, &ix, "ticket kerberos"),
        [
            "Red_Team/Golden.md",
            "Red_Team/Kerberoasting.md",
            "Tools_Blue/Logs.md"
        ]
    );
    assert_eq!(q(&v, &ix, "ticket -kerberos"), ["Sandbox/Box Elevation.md"]);
    assert_eq!(
        q(&v, &ix, "\"ticket de service\" OR golden"),
        ["Red_Team/Golden.md", "Tools_Blue/Logs.md"]
    );
    assert_eq!(q(&v, &ix, "/4[0-9]{3}/"), ["Tools_Blue/Logs.md"]);
    assert_eq!(
        q(&v, &ix, "/kerb.ros/"),
        [
            "Red_Team/Golden.md",
            "Red_Team/Kerberoasting.md",
            "Tools_Blue/Logs.md"
        ]
    );
    assert_eq!(q(&v, &ix, "tag:#ad"), ["Red_Team/Kerberoasting.md"]);
    assert_eq!(q(&v, &ix, "tag:blue"), ["Tools_Blue/Logs.md"]);
    assert!(q(&v, &ix, "tag:not-a-tag").is_empty());
    assert_eq!(
        q(&v, &ix, "path:Red"),
        ["Red_Team/Golden.md", "Red_Team/Kerberoasting.md"]
    );
    assert_eq!(q(&v, &ix, "file:box"), ["Sandbox/Box Elevation.md"]);
    assert_eq!(q(&v, &ix, "title:golden"), ["Red_Team/Golden.md"]);
    assert_eq!(q(&v, &ix, "heading:spn"), ["Red_Team/Kerberoasting.md"]);
    assert!(q(&v, &ix, "heading:privilèges").is_empty());
    assert_eq!(
        q(&v, &ix, "task-todo:getuserspns"),
        ["Red_Team/Kerberoasting.md"]
    );
    assert!(q(&v, &ix, "task-todo:hash").is_empty());
    assert_eq!(q(&v, &ix, "task-done:hash"), ["Red_Team/Kerberoasting.md"]);
    assert_eq!(
        q(&v, &ix, "prop:status=draft"),
        ["Sandbox/Box Elevation.md"]
    );
    assert_eq!(q(&v, &ix, "prop:status").len(), 2);
    assert_eq!(q(&v, &ix, "kerberoasting"), ["Red_Team/Kerberoasting.md"]);
    assert_eq!(q(&v, &ix, "box elevation"), ["Sandbox/Box Elevation.md"]);

    let ww = SearchOptions {
        whole_word: true,
        ..all()
    };
    let r = ix.search(&parse_query("ticket"), &ww, &CancelToken::new());
    let box_hit = r
        .hits
        .iter()
        .find(|h| v.note(h.note).unwrap().path.ends_with("Box Elevation.md"))
        .unwrap();
    assert_eq!(box_hit.match_count, 1, "tickets is not the whole word");
    let cs = SearchOptions {
        case_sensitive: true,
        ..all()
    };
    assert_eq!(
        paths(
            &v,
            &ix.search(&parse_query("kerberos"), &cs, &CancelToken::new())
        ),
        ["Red_Team/Kerberoasting.md", "Tools_Blue/Logs.md"]
    );
    let re = SearchOptions {
        regex: true,
        ..all()
    };
    assert_eq!(
        paths(
            &v,
            &ix.search(&parse_query("dem.nde de"), &re, &CancelToken::new())
        ),
        ["Tools_Blue/Logs.md"]
    );
    assert_eq!(q(&v, &ix, "hs"), Vec::<String>::new());
    assert_eq!(q(&v, &ix, "4769").len(), 1);
    assert!(q(&v, &ix, "").is_empty());
}

#[test]
fn ranking_snippets_and_ranges() {
    let (dir, root) = small_vault("ranking");
    let (v, ix) = open(&dir, &root);
    let r = ix.search(&parse_query("ticket"), &all(), &CancelToken::new());
    assert_eq!(r.total_files, 4);
    assert_eq!(
        v.note(r.hits[0].note).unwrap().path.as_ref(),
        Path::new("Red_Team/Golden.md")
    );
    assert_eq!(r.hits[0].title_ranges, vec![(7..13)]);
    let r = ix.search(&parse_query("kerberos"), &all(), &CancelToken::new());
    for h in &r.hits {
        for m in &h.matches {
            for range in &m.ranges {
                assert!(m.text.is_char_boundary(range.start) && m.text.is_char_boundary(range.end));
                assert_eq!(m.text[range.clone()].to_lowercase(), "kerberos");
                assert!(m.text.len() <= 200 * 4);
            }
        }
    }
    let r = ix.search(&parse_query("evenements"), &all(), &CancelToken::new());
    let m = &r.hits[0].matches[0];
    assert_eq!(&m.text[m.ranges[0].clone()], "Événements");
    assert_eq!(m.line, 1);

    let limited = SearchOptions {
        max_files: 1,
        ..all()
    };
    let r = ix.search(&parse_query("ticket"), &limited, &CancelToken::new());
    assert_eq!(r.hits.len(), 1);
    assert!(r.truncated);

    let cancel = CancelToken::new();
    cancel.cancel();
    let r = ix.search(&parse_query("ticket"), &all(), &cancel);
    assert!(r.cancelled);
}

fn naive(v: &Vault, term: &str) -> Vec<String> {
    let has_upper = term.chars().any(char::is_uppercase);
    let has_accent = term.chars().any(|c| focus_core::strip_accent(c) != c);
    let norm = |s: &str| -> String {
        s.chars()
            .map(|c| {
                let c = if has_upper {
                    c
                } else {
                    c.to_lowercase().next().unwrap_or(c)
                };
                if has_accent {
                    c
                } else {
                    focus_core::strip_accent(c)
                }
            })
            .collect()
    };
    let t = norm(term);
    let whole = term.chars().count() < 3;
    let found = |hay: &str| -> bool {
        let h = norm(hay);
        if !whole {
            return h.contains(&t);
        }
        let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
        h.match_indices(&t).any(|(i, m)| {
            !word(h[..i].chars().next_back()) && !word(h[i + m.len()..].chars().next())
        })
    };
    let mut out: Vec<String> = v
        .notes()
        .iter()
        .filter(|m| {
            let text = v.read(m.id).unwrap();
            found(&text) || found(&m.title) || found(&m.path.to_string_lossy())
        })
        .map(|m| m.path.to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn indexed_search_equals_naive_scan() {
    let dir = test_dir("property");
    let root = dir.join("vault");
    vaultgen::generate(&root, 400).unwrap();
    let (v, ix) = open(&dir, &root);
    let mut rng = vaultgen::Rng::new(7);
    let words: Vec<&str> = vaultgen::FR.iter().chain(vaultgen::EN).copied().collect();
    for round in 0..300 {
        let w = *rng.pick(&words);
        let chars: Vec<char> = w.chars().collect();
        let start = rng.below(chars.len());
        let len = 1 + rng.below(8.min(chars.len() - start));
        let mut term: String = chars[start..start + len].iter().collect();
        match round % 4 {
            0 => term = term.chars().map(focus_core::strip_accent).collect(),
            1 if rng.chance(30) => term = term.to_uppercase(),
            2 => term = format!("{term} {}", rng.pick(&words)),
            _ => {}
        }
        let query = if term.contains(' ') {
            format!("\"{term}\"")
        } else {
            term.clone()
        };
        let got = q(&v, &ix, &query);
        assert_eq!(got, naive(&v, &term), "query {query:?}");
    }
}

#[test]
fn applying_events_equals_rebuild() {
    let dir = test_dir("equivalence");
    let root = dir.join("vault");
    vaultgen::generate(&root, 200).unwrap();
    let (v, ix) = open(&dir, &root);
    let rx = v.subscribe();
    let first = v.notes()[0].id;
    let text = v.read(first).unwrap();
    v.write(first, &format!("{text}\nzythum quorvex élévation\n"), None)
        .unwrap();
    let created = v
        .create_note(Path::new("Neuf/Une note.md"), "# Neuve\nquorvex\n")
        .unwrap();
    let some = v
        .notes()
        .iter()
        .find(|m| m.id != first && m.id != created)
        .unwrap()
        .clone();
    v.rename(&some.path, Path::new("Ailleurs/Renommée.md"), true)
        .unwrap();
    let other = v
        .notes()
        .iter()
        .find(|m| m.id != first && m.id != created && m.id != some.id)
        .unwrap()
        .clone();
    v.trash(&other.path).unwrap();
    for e in rx.try_iter() {
        ix.apply(&e);
    }
    ix.flush();
    ix.persist().unwrap();

    let fresh = SearchIndex::open(v.clone(), &dir.join("fresh-cache"), &|_| {}).unwrap();
    let a = ix.postings.read();
    let b = fresh.postings.read();
    assert_eq!(a.docs, b.docs);
    assert!(
        a.materialize() == b.materialize(),
        "trigram lists differ: {} against {}",
        a.trigram_count(),
        b.trigram_count()
    );
    drop((a, b));
    let mut expected = vec![
        "Neuf/Une note.md".to_string(),
        v.note(first).unwrap().path.to_string_lossy().into_owned(),
    ];
    expected.sort();
    assert_eq!(q(&v, &ix, "quorvex"), expected);

    let reloaded = SearchIndex::open(v.clone(), &dir.join("cache"), &|_| {}).unwrap();
    assert_eq!(reloaded.stats().reindexed, 0);
    assert!(reloaded.postings.read().materialize() == fresh.postings.read().materialize());
}

#[test]
fn persisted_index_reloads_and_survives_corruption() {
    let (dir, root) = small_vault("persist");
    let (v, ix) = open(&dir, &root);
    assert_eq!(ix.stats().reindexed, 4);
    ix.persist().unwrap();
    let file = dir.join("cache").join(index::FILE_NAME);
    assert!(ix.stats().bytes_on_disk > 0);
    let again = SearchIndex::open(v.clone(), &dir.join("cache"), &|_| {}).unwrap();
    assert_eq!(again.stats().reindexed, 0);
    assert_eq!(q(&v, &again, "elevation").len(), 2);
    drop((ix, again));
    let bytes = std::fs::read(&file).unwrap();
    std::fs::write(&file, &bytes[..bytes.len() / 3]).unwrap();
    let rebuilt = SearchIndex::open(v.clone(), &dir.join("cache"), &|_| {}).unwrap();
    assert_eq!(rebuilt.stats().reindexed, 4);
    assert_eq!(q(&v, &rebuilt, "elevation").len(), 2);
}

#[test]
fn fuzzy_palette() {
    let (dir, root) = small_vault("fuzzy");
    let (v, ix) = open(&dir, &root);
    let hits = ix.fuzzy("krbrst", FuzzyScope::Notes, &[], 10);
    assert_eq!(hits[0].text, "Kerberoasting");
    assert_eq!(hits[0].positions, [0, 2, 3, 5, 8, 9]);
    let golden = v.note_by_path(Path::new("Red_Team/Golden.md")).unwrap();
    let logs = v.note_by_path(Path::new("Tools_Blue/Logs.md")).unwrap();
    let empty = ix.fuzzy("", FuzzyScope::Notes, &[logs, golden], 2);
    assert_eq!(
        empty.iter().map(|h| h.note.unwrap()).collect::<Vec<_>>(),
        [logs, golden]
    );
    let _ = ix.fuzzy("e", FuzzyScope::Notes, &[], 10);
    let incremental = ix.fuzzy("el", FuzzyScope::Notes, &[], 10);
    *ix.last_fuzzy.lock() = fuzzy::LastRun::default();
    let full = ix.fuzzy("el", FuzzyScope::Notes, &[], 10);
    assert_eq!(
        incremental
            .iter()
            .map(|h| (h.note, h.score))
            .collect::<Vec<_>>(),
        full.iter().map(|h| (h.note, h.score)).collect::<Vec<_>>()
    );
    let k = v
        .note_by_path(Path::new("Red_Team/Kerberoasting.md"))
        .unwrap();
    let hs = ix.fuzzy("enum", FuzzyScope::Headings(k), &[], 5);
    assert_eq!(hs[0].text, "Énumération des SPN");
    assert_eq!(hs[0].line, Some(6));
    let tags = ix.fuzzy("kerb", FuzzyScope::Tags, &[], 5);
    assert_eq!(tags[0].text, "ad/kerberos");

    let rx = v.subscribe();
    let added = v
        .create_note(Path::new("Zygomorphe.md"), "# Zygomorphe\n")
        .unwrap();
    for e in rx.try_iter() {
        ix.apply(&e);
    }
    let started = std::time::Instant::now();
    loop {
        let hits = ix.fuzzy("zygomor", FuzzyScope::Notes, &[], 5);
        if hits.first().and_then(|h| h.note) == Some(added) {
            break;
        }
        assert!(started.elapsed().as_secs() < 5, "catalog never rebuilt");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn private_vault_relevance() {
    let Some(root) = std::env::var_os("FOCUS_REAL_VAULT").map(PathBuf::from) else {
        return;
    };
    if !root.join(".obsidian").exists() {
        return;
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/focus-search-tests/private-relevance");
    let (v, ix) = open(&dir, &root);
    let cases: &[(&str, &[&str])] = &[
        (
            "kerberoast",
            &[
                "kerberoast - active-directory-methodology.md",
                "kerberoast - kerberos.md",
            ],
        ),
        ("mimikatz", &["credentials-mimikatz.md", "2.Mimikatz.md"]),
        (
            "bloodhound",
            &["recon/bloodhound/index.md", "BloodHound - CRTP.md"],
        ),
        (
            "asreproast",
            &[
                "asreproast - active-directory-methodology.md",
                "asreproast - kerberos.md",
            ],
        ),
        (
            "golden ticket",
            &["golden-ticket.md", "forged-tickets/golden.md"],
        ),
        (
            "silver ticket",
            &["silver-ticket.md", "forged-tickets/silver.md"],
        ),
        (
            "dcsync",
            &[
                "dcsync - dumping.md",
                "dcsync - active-directory-methodology.md",
            ],
        ),
        ("ntlm relay", &["ntlm/relay - ntlm.md"]),
        ("pass the hash", &["ntlm/pth.md", "3.Pass-the-hash.md"]),
        ("nmap", &["Nmap - CPTS.md", "Nmap - HTB Academy.md"]),
        (
            "sqlmap",
            &["sqlmap - sql-injection.md", "SQLmap - SQL Injection.md"],
        ),
        (
            "sql injection",
            &[
                "SQL Injection/README.md",
                "sqli - inputs.md",
                "sql-injection/README.md",
            ],
        ),
        ("ssrf", &["Server Side Request Forgery (SSRF).md"]),
        (
            "file upload",
            &["Web Exploitation/File Upload.md", "file-upload/README.md"],
        ),
        (
            "windows privesc",
            &["Windows Post Exploitation/Windows privesc.md"],
        ),
        (
            "linux privesc",
            &["Linux Post Exploitation/Linux privesc.md"],
        ),
        ("unconstrained delegation", &["unconstrained-delegation.md"]),
        (
            "constrained delegation",
            &["active-directory-methodology/constrained-delegation.md"],
        ),
        (
            "shadow credentials",
            &[
                "shadow-credentials - kerberos.md",
                "shadow-credentials - acl-persistence-abuse.md",
            ],
        ),
        ("port forwarding", &["pivoting/port-forwarding.md"]),
        ("web checklist", &["EXAM/Web checklist.md"]),
        ("AD checklist", &["EXAM/AD_Checklist.md"]),
        (
            "reverse shell",
            &[
                "Reverse Shell Cheatsheet.md",
                "reverse-shells/README.md",
                "CRTP/Reverse Shell.md",
            ],
        ),
    ];
    let options = SearchOptions {
        max_files: 3,
        budget_ms: u32::MAX,
        ..Default::default()
    };
    let mut failures = Vec::new();
    for (query, expected) in cases {
        let r = ix.search(&parse_query(query), &options, &CancelToken::new());
        let top: Vec<String> = r
            .hits
            .iter()
            .map(|h| v.note(h.note).unwrap().path.to_string_lossy().into_owned())
            .collect();
        if !top.iter().any(|p| expected.iter().any(|e| p.ends_with(e))) {
            failures.push(format!("{query:?} : {top:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "queries without an expected note in the top three:\n{}",
        failures.join("\n")
    );

    let r = ix.search(
        &parse_query("kerberoast"),
        &SearchOptions {
            budget_ms: u32::MAX,
            ..Default::default()
        },
        &CancelToken::new(),
    );
    let rank = |suffix: &str| {
        r.hits.iter().position(|h| {
            v.note(h.note)
                .unwrap()
                .path
                .to_string_lossy()
                .ends_with(suffix)
        })
    };
    assert!(rank("kerberoast - kerberos.md") < rank("Windows - LDAP User KerbeRoastable.md"));
    let r = ix.search(
        &parse_query("elevation de privileges"),
        &SearchOptions {
            budget_ms: u32::MAX,
            ..Default::default()
        },
        &CancelToken::new(),
    );
    assert!(r.total_files > 0);
    for h in &r.hits {
        for m in &h.matches {
            for range in &m.ranges {
                assert_ne!(m.text[range.clone()].to_lowercase(), "de");
            }
        }
    }
}

fn mention_vault(name: &str) -> (PathBuf, Arc<Vault>, Arc<SearchIndex>) {
    let dir = test_dir(name);
    let root = dir.join("vault");
    put(
        &root,
        "Red/Kerberos.md",
        "---\naliases: [krb5]\n---\n# Kerberos\n\nKerberos talks about itself.\n",
    );
    put(&root, "a.md", "Attack on kerberos here.\nAnd KRB5 there.\n");
    put(
        &root,
        "b.md",
        "---\ntopic: Kerberos\n---\n`Kerberos` [[Kerberos]] #Kerberos kerberoasting\n",
    );
    put(&root, "Private/c.md", "Kerberos in an excluded folder.\n");
    let vault = Vault::open(
        &root,
        OpenOptions {
            cache_dir: dir.join("cache"),
            ignore: vec!["Private".into()],
            watch: false,
        },
    )
    .unwrap();
    let index = SearchIndex::open(vault.clone(), &dir.join("cache"), &|_| {}).unwrap();
    (dir, vault, index)
}

fn mentions_of(vault: &Vault, index: &SearchIndex, path: &str) -> Mentions {
    let note = vault.note_by_path(Path::new(path)).unwrap();
    index.unlinked_mentions(
        note,
        &MentionOptions {
            budget_ms: u32::MAX,
            ..Default::default()
        },
        &CancelToken::new(),
    )
}

#[test]
fn unlinked_mentions_by_name_and_alias() {
    let (_dir, vault, index) = mention_vault("mentions");
    let found = mentions_of(&vault, &index, "Red/Kerberos.md");
    assert_eq!(found.total_notes, 1, "{found:?}");
    let hit = &found.hits[0];
    assert_eq!(
        vault.note(hit.note).unwrap().path.as_ref(),
        Path::new("a.md")
    );
    let words: Vec<(u32, &str)> = hit
        .mentions
        .iter()
        .map(|m| (m.line, &m.text[m.range.clone()]))
        .collect();
    assert_eq!(words, [(0, "kerberos"), (1, "KRB5")]);
    assert!(!found.partial);
}

#[test]
fn a_mention_turns_into_a_link_and_keeps_a_version() {
    let (dir, vault, index) = mention_vault("mention-link");
    vault
        .set_history(&dir.join("history"), focus_core::HistoryConfig::default())
        .unwrap();
    let found = mentions_of(&vault, &index, "Red/Kerberos.md");
    let hit = &found.hits[0];
    let mention = &hit.mentions[0];
    let source = vault.read(hit.note).unwrap();
    let words = &source[mention.span.clone()];
    let link = mention_link(
        words,
        Path::new("a.md"),
        Path::new("Red/Kerberos.md"),
        false,
        LinkPath::Shortest,
        true,
    );
    write_mention_link(&vault, hit.note, mention.span.clone(), words, &link).unwrap();
    assert_eq!(
        vault.read(hit.note).unwrap(),
        "Attack on [[Kerberos|kerberos]] here.\nAnd KRB5 there.\n"
    );
    let versions = vault.history().unwrap().list(Path::new("a.md")).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(
        vault
            .history()
            .unwrap()
            .read(Path::new("a.md"), &versions[0].id)
            .unwrap(),
        source
    );
    assert!(matches!(
        write_mention_link(&vault, hit.note, mention.span.clone(), words, &link),
        Err(LinkMentionError::Changed)
    ));
}

#[test]
fn a_common_name_is_bounded_by_the_budget() {
    let dir = test_dir("mentions-common");
    let root = dir.join("vault");
    put(&root, "the.md", "x\n");
    for i in 0..400 {
        put(&root, &format!("n{i}.md"), "the the the\n");
    }
    let (vault, index) = open(&dir, &root);
    let note = vault.note_by_path(Path::new("the.md")).unwrap();
    let full = index.unlinked_mentions(
        note,
        &MentionOptions {
            max_notes: 10,
            max_per_note: 2,
            budget_ms: u32::MAX,
        },
        &CancelToken::new(),
    );
    assert_eq!((full.total_notes, full.total_mentions), (400, 1200));
    assert_eq!(full.hits.len(), 10);
    assert!(
        full.hits
            .iter()
            .all(|h| h.mentions.len() == 2 && h.count == 3)
    );
    let cancel = CancelToken::new();
    cancel.cancel();
    let stopped = index.unlinked_mentions(note, &MentionOptions::default(), &cancel);
    assert!(stopped.partial && stopped.cancelled && stopped.total_notes == 0);
}
