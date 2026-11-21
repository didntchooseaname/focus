use super::*;
use std::time::Duration;

fn put(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

fn get(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap()
}

fn options(dir: &Path, watch: bool) -> OpenOptions {
    OpenOptions {
        cache_dir: dir.join("cache"),
        ignore: vec!["Private".into()],
        watch,
    }
}

fn sample(name: &str) -> (PathBuf, PathBuf) {
    let dir = test_dir(name);
    let root = dir.join("vault");
    put(
        &root,
        ".obsidian/app.json",
        r#"{"attachmentFolderPath":"./assets","useMarkdownLinks":true,"newLinkFormat":"relative"}"#,
    );
    put(
        &root,
        "Home.md",
        "# Accueil\n\n- [Red](Red/00_Index.md)\n- [Kerb](Red/AD/Kerberoasting.md#Énumération)\n- [[Kerberoasting|alias]]\n",
    );
    put(
        &root,
        "Red/00_Index.md",
        "---\ntags: [red-team, index]\n---\n# Index Red\n\n[Accueil](../Home.md)\n[Kerb](AD/Kerberoasting.md)\n[[AD/Kerberoasting#Outils|outils]]\n",
    );
    put(
        &root,
        "Red/AD/Kerberoasting.md",
        "---\naliases: [Roasting]\ntags: attack/T1558\n---\n# Kerberoasting\n\n## Énumération\n![capture](assets/Kerberoasting/kerberoasting-enumeration-01.png)\n- [ ] tester #ad/kerberos\n- [x] fini\n\n## Outils\nVoir [index](../00_Index.md) et [[Inexistante]].\n",
    );
    put(
        &root,
        "Red/AD/assets/Kerberoasting/kerberoasting-enumeration-01.png",
        "PNG1",
    );
    put(
        &root,
        "Labs/Box.md",
        "# Box\n[[Roasting]] [k](../Red/AD/Kerberoasting.md) ![[shared.png]]\n",
    );
    put(&root, "Labs/shared.png", "PNG2");
    put(&root, "Private/secret.md", "# Secret\n");
    put(&root, ".trash/old.md", "# Vieux\n");
    (dir, root)
}

fn id(v: &Vault, p: &str) -> NoteId {
    v.note_by_path(Path::new(p))
        .unwrap_or_else(|| panic!("{p} missing"))
}

#[test]
fn open_reads_metadata_tree_and_links() {
    let (dir, root) = sample("open");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let stats = v.stats();
    assert_eq!(stats.notes, 4, "Private and .trash excluded");
    assert_eq!(stats.attachments, 2);
    assert_eq!(stats.reparsed, 4);
    let cfg = v.obsidian();
    assert!(cfg.use_markdown_links);

    let k = id(&v, "Red/AD/Kerberoasting.md");
    let meta = v.note(k).unwrap();
    assert_eq!(&*meta.title, "Kerberoasting");
    assert_eq!(meta.aliases, ["Roasting"]);
    assert_eq!(meta.tags, ["attack/T1558", "ad/kerberos"]);
    assert_eq!((meta.tasks_open, meta.tasks_done), (1, 1));

    let tree = v.tree();
    let names: Vec<_> = tree
        .nodes
        .iter()
        .map(|n| (n.depth, n.name.as_ref()))
        .collect();
    assert_eq!(
        names,
        [
            (0, "Labs"),
            (1, "Box.md"),
            (1, "shared.png"),
            (0, "Red"),
            (1, "AD"),
            (2, "assets"),
            (3, "Kerberoasting"),
            (4, "kerberoasting-enumeration-01.png"),
            (2, "Kerberoasting.md"),
            (1, "00_Index.md"),
            (0, "Home.md"),
        ]
    );
    assert_eq!(tree.nodes[3].note_count, 2);

    let home = id(&v, "Home.md");
    let index = id(&v, "Red/00_Index.md");
    let bx = id(&v, "Labs/Box.md");
    assert_eq!(
        v.resolve_link(index, "AD/Kerberoasting.md"),
        LinkTarget::Note {
            id: k,
            anchor: None
        }
    );
    assert_eq!(
        v.resolve_link(home, "Red/AD/Kerberoasting.md#Énumération"),
        LinkTarget::Note {
            id: k,
            anchor: Some("Énumération".into())
        }
    );
    assert_eq!(
        v.resolve_link(home, "Kerberoasting"),
        LinkTarget::Note {
            id: k,
            anchor: None
        }
    );
    assert_eq!(
        v.resolve_link(home, "kerberoasting"),
        LinkTarget::Note {
            id: k,
            anchor: None
        },
        "case-insensitive"
    );
    assert_eq!(
        v.resolve_link(bx, "Roasting"),
        LinkTarget::Note {
            id: k,
            anchor: None
        },
        "alias"
    );
    assert_eq!(
        v.resolve_link(bx, "Roasting|texte"),
        LinkTarget::Note {
            id: k,
            anchor: None
        }
    );
    assert_eq!(
        v.resolve_link(home, "Red/00_Index"),
        LinkTarget::Note {
            id: index,
            anchor: None
        }
    );
    assert_eq!(
        v.resolve_link(home, "Shoulder%20surfing.md"),
        LinkTarget::Unresolved
    );
    assert_eq!(
        v.resolve_link(home, "https://x.org"),
        LinkTarget::External("https://x.org".into())
    );
    assert_eq!(
        v.resolve_link(home, "#Accueil"),
        LinkTarget::Note {
            id: home,
            anchor: Some("Accueil".into())
        }
    );
    assert_eq!(
        v.resolve_link(k, "shared.png"),
        LinkTarget::Attachment("Labs/shared.png".into()),
        "unique file name"
    );
    assert_eq!(
        v.resolve_link(k, "../../../outside.md"),
        LinkTarget::Unresolved
    );

    let mut back: Vec<_> = v
        .backlinks(k)
        .into_iter()
        .map(|b| (b.source, b.line, b.context[b.range.clone()].to_string()))
        .collect();
    back.sort();
    let mut expected = vec![
        (
            home,
            3,
            "[Kerb](Red/AD/Kerberoasting.md#Énumération)".to_string(),
        ),
        (home, 4, "[[Kerberoasting|alias]]".to_string()),
        (index, 6, "[Kerb](AD/Kerberoasting.md)".to_string()),
        (index, 7, "[[AD/Kerberoasting#Outils|outils]]".to_string()),
        (bx, 1, "[[Roasting]]".to_string()),
        (bx, 1, "[k](../Red/AD/Kerberoasting.md)".to_string()),
    ];
    expected.sort();
    assert_eq!(back, expected);

    let un: Vec<_> = v.unresolved(k).into_iter().map(|l| l.target).collect();
    assert_eq!(un, ["Inexistante"]);
    assert_eq!(v.all_unresolved().len(), 1);
    assert_eq!(
        v.attachments_of(k),
        [PathBuf::from(
            "Red/AD/assets/Kerberoasting/kerberoasting-enumeration-01.png"
        )]
    );
    assert!(v.orphan_attachments().is_empty());

    let tags: Vec<_> = v.tags().into_iter().map(|t| (t.tag, t.count)).collect();
    assert_eq!(
        tags,
        [
            ("ad".into(), 1),
            ("ad/kerberos".into(), 1),
            ("attack".into(), 1),
            ("attack/T1558".into(), 1),
            ("index".into(), 1),
            ("red-team".into(), 1)
        ]
    );
    assert_eq!(v.notes_with_tag("attack"), [k]);
    assert_eq!(v.notes_with_tag("#AD/Kerberos"), [k]);
    assert!(v.notes_with_tag("att").is_empty());
    let open_tasks: Vec<_> = v.tasks(false).into_iter().map(|t| t.text).collect();
    assert_eq!(open_tasks, ["tester #ad/kerberos"]);
    assert_eq!(v.tasks(true).len(), 2);
}

#[test]
fn cache_is_reused_and_rebuilt_when_corrupt() {
    let (dir, root) = sample("cache");
    let first = Vault::open(&root, options(&dir, false)).unwrap();
    let ids: Vec<_> = ["Home.md", "Labs/Box.md"]
        .iter()
        .map(|p| id(&first, p))
        .collect();
    drop(first);
    let cache = dir.join("cache").join(cache::FILE_NAME);
    assert!(cache.exists());

    let warm = Vault::open(&root, options(&dir, false)).unwrap();
    assert_eq!(warm.stats().reparsed, 0);
    assert_eq!(
        ids,
        ["Home.md", "Labs/Box.md"]
            .iter()
            .map(|p| id(&warm, p))
            .collect::<Vec<_>>(),
        "stable ids"
    );
    assert_eq!(
        warm.backlinks(id(&warm, "Red/AD/Kerberoasting.md")).len(),
        6
    );
    drop(warm);

    std::thread::sleep(Duration::from_millis(10));
    put(&root, "Labs/Box.md", "# Box modifiée\n");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    assert_eq!(v.stats().reparsed, 1);
    assert_eq!(
        &*v.note(id(&v, "Labs/Box.md")).unwrap().title,
        "Box modifiée"
    );
    drop(v);

    let mut bytes = std::fs::read(&cache).unwrap();
    let mid = bytes.len() / 2;
    bytes.truncate(mid);
    std::fs::write(&cache, &bytes).unwrap();
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    assert_eq!(v.stats().reparsed, 4);
    drop(v);
    std::fs::write(&cache, b"n'importe quoi").unwrap();
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    assert_eq!(v.stats().notes, 4);
}

#[test]
fn rename_note_rewrites_links_in_their_style() {
    let (dir, root) = sample("rename-note");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let k = id(&v, "Red/AD/Kerberoasting.md");
    let events = v.subscribe();
    let report = v
        .rename(
            Path::new("Red/AD/Kerberoasting.md"),
            Path::new("Red/Kerberos/Kerberoast SPN.md"),
            true,
        )
        .unwrap();

    assert_eq!(
        id(&v, "Red/Kerberos/Kerberoast SPN.md"),
        k,
        "the id follows the note"
    );
    assert!(
        v.note_by_path(Path::new("Red/AD/Kerberoasting.md"))
            .is_none()
    );
    assert_eq!(
        get(&root, "Home.md"),
        "# Accueil\n\n- [Red](Red/00_Index.md)\n- [Kerb](Red/Kerberos/Kerberoast%20SPN.md#Énumération)\n- [[Kerberoast SPN|alias]]\n"
    );
    assert_eq!(
        get(&root, "Red/00_Index.md"),
        "---\ntags: [red-team, index]\n---\n# Index Red\n\n[Accueil](../Home.md)\n[Kerb](Kerberos/Kerberoast%20SPN.md)\n[[Kerberos/Kerberoast SPN#Outils|outils]]\n"
    );
    assert_eq!(
        get(&root, "Labs/Box.md"),
        "# Box\n[[Roasting]] [k](../Red/Kerberos/Kerberoast%20SPN.md) ![[shared.png]]\n"
    );
    let moved = get(&root, "Red/Kerberos/Kerberoast SPN.md");
    assert!(
        moved.contains("![capture](assets/Kerberoast%20SPN/kerberoasting-enumeration-01.png)"),
        "{moved}"
    );
    assert!(moved.contains("Voir [index](../00_Index.md)"), "{moved}");
    assert!(
        root.join("Red/Kerberos/assets/Kerberoast SPN/kerberoasting-enumeration-01.png")
            .exists(),
        "the subfolder follows"
    );
    assert!(!root.join("Red/AD/assets/Kerberoasting").exists());
    assert_eq!(report.updated_links, 6);
    assert_eq!(report.updated_notes.len(), 4);

    assert_eq!(v.all_unresolved().len(), 1);
    assert_eq!(v.backlinks(k).len(), 6);
    let evs: Vec<_> = events.try_iter().collect();
    assert!(
        evs.iter()
            .any(|e| matches!(e, VaultEvent::NoteMoved { id, .. } if *id == k))
    );
    assert!(evs.iter().any(|e| matches!(e, VaultEvent::TreeChanged)));
}

#[test]
fn rename_folder_and_attachment() {
    let (dir, root) = sample("rename-folder");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let report = v
        .rename(Path::new("Red"), Path::new("Offensive/Red"), true)
        .unwrap();
    assert!(report.updated_links >= 4);
    assert_eq!(
        get(&root, "Home.md"),
        "# Accueil\n\n- [Red](Offensive/Red/00_Index.md)\n- [Kerb](Offensive/Red/AD/Kerberoasting.md#Énumération)\n- [[Kerberoasting|alias]]\n"
    );
    assert_eq!(
        get(&root, "Offensive/Red/00_Index.md").lines().nth(5),
        Some("[Accueil](../../Home.md)")
    );
    assert!(
        v.note_by_path(Path::new("Offensive/Red/AD/Kerberoasting.md"))
            .is_some()
    );
    assert_eq!(v.all_unresolved().len(), 1);
    assert!(
        v.tree()
            .nodes
            .iter()
            .any(|n| &*n.path == Path::new("Offensive/Red/AD/assets/Kerberoasting"))
    );

    v.rename(
        Path::new("Labs/shared.png"),
        Path::new("Labs/carte réseau.png"),
        true,
    )
    .unwrap();
    assert_eq!(
        get(&root, "Labs/Box.md"),
        "# Box\n[[Roasting]] [k](../Offensive/Red/AD/Kerberoasting.md) ![[carte réseau.png]]\n"
    );
    assert!(matches!(
        v.rename(Path::new("Labs/Box.md"), Path::new("Home.md"), true),
        Err(VaultError::AlreadyExists(_))
    ));
    assert!(matches!(
        v.rename(Path::new("../x.md"), Path::new("y.md"), true),
        Err(VaultError::OutsideVault(_))
    ));
}

#[test]
fn write_conflict_create_trash_and_tasks() {
    let (dir, root) = sample("write");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let k = id(&v, "Red/AD/Kerberoasting.md");
    let before = v.note(k).unwrap().modified;
    let text = v.read(k).unwrap();
    let t1 = v
        .write(
            k,
            &text.replace("# Kerberoasting", "# Kerberoasting v2"),
            Some(before),
        )
        .unwrap();
    assert_eq!(&*v.note(k).unwrap().title, "Kerberoasting v2");
    assert!(
        matches!(v.write(k, "x", Some(before)), Err(VaultError::Conflict(_))),
        "stale mtime"
    );
    assert!(v.write(k, &v.read(k).unwrap(), Some(t1)).is_ok());

    assert!(v.toggle_task(k, 8).unwrap());
    assert!(v.read(k).unwrap().contains("- [x] tester"));
    assert!(!v.toggle_task(k, 8).unwrap());
    assert!(v.toggle_task(k, 0).is_err());

    let n = v
        .create_note(Path::new("Nouveau/Inexistante"), "# Nouvelle\n")
        .unwrap();
    assert_eq!(
        &*v.note(n).unwrap().path,
        Path::new("Nouveau/Inexistante.md")
    );
    assert!(v.all_unresolved().is_empty(), "the broken link resolves");
    assert!(matches!(
        v.create_note(Path::new("Nouveau/Inexistante.md"), ""),
        Err(VaultError::AlreadyExists(_))
    ));
    v.create_folder(Path::new("Vide/Sous")).unwrap();
    assert!(
        v.tree()
            .nodes
            .iter()
            .any(|n| &*n.path == Path::new("Vide/Sous"))
    );

    v.trash(Path::new("Nouveau/Inexistante.md")).unwrap();
    assert!(v.note(n).is_none());
    assert!(root.join(".trash/Inexistante.md").exists());
    assert_eq!(v.all_unresolved().len(), 1);
    let again = v.create_note(Path::new("Inexistante.md"), "").unwrap();
    assert!(again > n, "id never reused");
    v.trash(Path::new("Inexistante.md")).unwrap();
    assert!(root.join(".trash/Inexistante 1.md").exists());
}

#[test]
fn attachments_saved_deduplicated_and_tidied() {
    let (dir, root) = sample("attach");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let bx = id(&v, "Labs/Box.md");
    let policy = AttachmentPolicy::default();
    let req = |bytes: &'static [u8], heading| AttachmentRequest {
        note: bx,
        bytes,
        extension: "png",
        original_name: None,
        heading,
        directory: None,
        name_prefix: None,
        policy: &policy,
    };
    let a = v.attach(&req(b"IMG-A", Some("Énumération SPN"))).unwrap();
    assert_eq!(
        a.path,
        PathBuf::from("Labs/assets/Box/box-enumeration-spn-01.png")
    );
    assert_eq!(
        a.link,
        "![Énumération SPN](assets/Box/box-enumeration-spn-01.png)"
    );
    assert!(!a.reused);
    let b = v.attach(&req(b"IMG-B", Some("Énumération SPN"))).unwrap();
    assert_eq!(
        b.path,
        PathBuf::from("Labs/assets/Box/box-enumeration-spn-02.png")
    );
    let c = v.attach(&req(b"IMG-A", None)).unwrap();
    assert!(c.reused);
    assert_eq!(c.path, a.path);

    assert_eq!(v.orphan_attachments(), [a.path.clone(), b.path.clone()]);

    put(
        &root,
        "30/Note rangée.md",
        "# Note rangée\n## Preuves\n![](../loose.png) ![](../copy.png)\n![[ailleurs.png]]\n",
    );
    put(&root, "loose.png", "SAME");
    put(&root, "copy.png", "SAME");
    put(&root, "30/x/ailleurs.png", "OTHER");
    drop(v);
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let n = id(&v, "30/Note rangée.md");
    let plan = v.plan_tidy_attachments(Some(n), &policy);
    assert_eq!(
        plan.moves,
        [
            (
                PathBuf::from("loose.png"),
                PathBuf::from("30/assets/Note rangée/note-rangee-preuves-01.png")
            ),
            (
                PathBuf::from("30/x/ailleurs.png"),
                PathBuf::from("30/assets/Note rangée/note-rangee-preuves-02.png")
            ),
        ]
    );
    assert_eq!(
        plan.duplicates,
        [(PathBuf::from("copy.png"), PathBuf::from("loose.png"))]
    );
    assert_eq!(plan.link_updates, 3);
    let report = v.apply_tidy(&plan).unwrap();
    assert!(report.skipped.is_empty());
    assert_eq!(
        get(&root, "30/Note rangée.md"),
        "# Note rangée\n## Preuves\n![](assets/Note%20rangée/note-rangee-preuves-01.png) ![](assets/Note%20rangée/note-rangee-preuves-01.png)\n![[note-rangee-preuves-02.png]]\n"
    );
    assert!(root.join(".trash/copy.png").exists());
    assert!(v.unresolved(n).is_empty());
    assert!(
        v.plan_tidy_attachments(Some(n), &policy).moves.is_empty(),
        "a second tidy changes nothing"
    );
    let _ = dir;
}

fn wait_for(
    rx: &Receiver<VaultEvent>,
    mut pred: impl FnMut(&VaultEvent) -> bool,
) -> Vec<VaultEvent> {
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if let Ok(e) = rx.recv_timeout(Duration::from_millis(50)) {
            let done = pred(&e);
            seen.push(e);
            if done {
                return seen;
            }
        }
    }
    panic!("expected event not received; got {seen:?}");
}

#[test]
fn watcher_applies_external_changes_in_batches() {
    let (dir, root) = sample("watch");
    let v = Vault::open(&root, options(&dir, true)).unwrap();
    let rx = v.subscribe();
    assert!(
        v.inner
            .watcher
            .lock()
            .as_ref()
            .is_some_and(|w| w.wait_ready(Duration::from_secs(60)))
    );

    let bx = id(&v, "Labs/Box.md");
    put(&root, "Labs/Box.md", "# Box externe\n[[Nouvelle]]\n");
    wait_for(&rx, |e| matches!(e, VaultEvent::NoteChanged(i) if *i == bx));
    assert_eq!(&*v.note(bx).unwrap().title, "Box externe");

    let burst = root.join("burst");
    std::fs::create_dir_all(&burst).unwrap();
    for i in 0..1000 {
        std::fs::write(
            burst.join(format!("n{i}.md")),
            format!("# N{i}\n[[n{}]]\n", (i + 1) % 1000),
        )
        .unwrap();
    }
    let started = Instant::now();
    while v.stats().notes < 1004 && started.elapsed() < Duration::from_secs(60) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(v.stats().notes, 1004);
    assert_eq!(v.backlinks(id(&v, "burst/n0.md")).len(), 1);

    let n5 = id(&v, "burst/n5.md");
    std::fs::rename(burst.join("n5.md"), root.join("n5-déplacée.md")).unwrap();
    wait_for(
        &rx,
        |e| matches!(e, VaultEvent::NoteMoved { id, .. } if *id == n5),
    );
    assert_eq!(id(&v, "n5-déplacée.md"), n5);

    put(&root, "Nouvelle.md", "# N\n");
    wait_for(&rx, |e| matches!(e, VaultEvent::NoteAdded(_)));
    assert!(v.unresolved(bx).is_empty());
    std::fs::remove_dir_all(&burst).unwrap();
    let started = Instant::now();
    while v.stats().notes != 6 && started.elapsed() < Duration::from_secs(60) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(v.stats().notes, 6);
    assert!(!v.tree().nodes.iter().any(|n| n.path.starts_with("burst")));

    while rx.try_recv().is_ok() {}
    let t = v.read(bx).unwrap();
    v.write(bx, &format!("{t}\nligne"), None).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    let evs: Vec<_> = rx.try_iter().collect();
    assert_eq!(
        evs.iter()
            .filter(|e| matches!(e, VaultEvent::NoteChanged(_)))
            .count(),
        1,
        "{evs:?}"
    );
}

#[test]
fn rename_on_private_vault_copy() {
    let Some(src) = std::env::var_os("FOCUS_REAL_VAULT").map(PathBuf::from) else {
        return;
    };
    if !src.join(".obsidian").exists() {
        return;
    }
    let dir = test_dir("private-copy");
    let root = dir.join("vault");
    let status = std::process::Command::new("rsync")
        .args([
            "-a",
            "--include=*/",
            "--include=*.md",
            "--include=.obsidian/*.json",
            "--exclude=*",
            "--exclude=.git/",
        ])
        .arg(format!("{}/", src.display()))
        .arg(&root)
        .status()
        .expect("rsync");
    assert!(status.success());
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let before = v.all_unresolved().len();
    let t = v
        .summaries()
        .iter()
        .map(|s| s.id)
        .max_by_key(|&id| v.backlinks(id).len())
        .expect("a note");
    let target = v.note(t).unwrap().path.to_path_buf();
    let backlinks = v.backlinks(t).len();
    assert!(backlinks > 0);
    let report = v
        .rename(&target, Path::new("Index/Renamed index.md"), true)
        .unwrap();
    assert!(
        report.updated_links >= backlinks,
        "{} < {backlinks}",
        report.updated_links
    );
    assert_eq!(v.backlinks(t).len(), backlinks);
    assert_eq!(
        v.all_unresolved().len(),
        before,
        "no link broken by the rename"
    );
    drop(v);
    std::fs::remove_dir_all(dir.join("cache")).unwrap();
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    assert_eq!(v.all_unresolved().len(), before);
    assert_eq!(
        v.backlinks(id(&v, "Index/Renamed index.md")).len(),
        backlinks
    );
}

#[test]
fn rescan_catches_up_with_unwatched_changes() {
    let (dir, root) = sample("rescan");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let rx = v.subscribe();
    put(&root, "Hors surveillance.md", "# Hors\n[[Kerberoasting]]\n");
    std::fs::remove_file(root.join("Labs/Box.md")).unwrap();
    v.inner.apply_batch(HashSet::new(), true);
    assert!(v.note_by_path(Path::new("Hors surveillance.md")).is_some());
    assert!(v.note_by_path(Path::new("Labs/Box.md")).is_none());
    let evs: Vec<_> = rx.try_iter().collect();
    assert!(evs.iter().any(|e| matches!(e, VaultEvent::NoteAdded(_))));
    assert!(
        evs.iter()
            .any(|e| matches!(e, VaultEvent::NoteRemoved { .. }))
    );
}

#[test]
fn tidy_skips_files_changed_since_plan() {
    let (dir, root) = sample("tidy-stamps");
    put(&root, "30/Note.md", "# Note\n![](../a.png) ![](../b.png)\n");
    put(&root, "a.png", "A");
    put(&root, "b.png", "B");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let plan = v.plan_tidy_attachments(Some(id(&v, "30/Note.md")), &AttachmentPolicy::default());
    assert_eq!(plan.moves.len(), 2);
    assert_eq!(plan.stamps.len(), 2);
    std::thread::sleep(Duration::from_millis(10));
    put(&root, "b.png", "B modifié");
    let report = v.apply_tidy(&plan).unwrap();
    assert_eq!(report.skipped, [PathBuf::from("b.png")]);
    assert!(root.join("b.png").exists());
    assert!(!root.join("a.png").exists());
    assert_eq!(
        get(&root, "30/Note.md"),
        "# Note\n![](assets/Note/note-note-01.png) ![](../b.png)\n"
    );
}

#[test]
fn links_in_properties_and_diagrams_count_and_follow_renames() {
    let dir = test_dir("links-everywhere");
    let root = dir.join("vault");
    put(
        &root,
        "Hosts/dc01.md",
        "# DC01\n\n## Services\nLDAP ^svc-ldap\n",
    );
    put(
        &root,
        "Journal.md",
        "# Journal\n- 14:03 scan done ^j-20261112-140327\n",
    );
    put(
        &root,
        "Finding.md",
        "---\nrelated: \"[[dc01]]\"\nassets:\n  - \"[[Hosts/dc01|DC]]\"\n  - \"[[Missing host]]\"\n# note: [[not a link]]\n---\n# Finding\nSee [[Journal#^j-20261112-140327]].\n\n```mermaid\ngraph TD\n  A[\"[[dc01]]\"] --> B[[Subroutine]]\n  click A \"Hosts/dc01.md#Services\"\n  click B \"https://example.org\"\n  link C \"Journal.md\"\n```\n",
    );
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let (finding, dc) = (id(&v, "Finding.md"), id(&v, "Hosts/dc01.md"));
    let kinds: Vec<(LinkKind, String)> = v
        .note(finding)
        .unwrap()
        .links
        .iter()
        .map(|l| (l.kind, l.target.clone()))
        .collect();
    assert_eq!(
        kinds,
        [
            (LinkKind::Property, "dc01".into()),
            (LinkKind::Property, "Hosts/dc01".into()),
            (LinkKind::Property, "Missing host".into()),
            (LinkKind::Wiki, "Journal".into()),
            (LinkKind::Diagram, "dc01".into()),
            (LinkKind::Diagram, "Hosts/dc01.md".into()),
            (LinkKind::Diagram, "Journal.md".into()),
        ]
    );
    let sources: Vec<NoteId> = v.backlinks(dc).iter().map(|b| b.source).collect();
    assert_eq!(
        sources.len(),
        4,
        "two properties and two diagram links: {sources:?}"
    );
    assert_eq!(v.linked_from(dc), [finding]);
    assert_eq!(
        v.unresolved(finding)
            .iter()
            .map(|l| l.target.as_str())
            .collect::<Vec<_>>(),
        ["Missing host"]
    );

    let journal = id(&v, "Journal.md");
    assert_eq!(v.anchor_line(journal, "^j-20261112-140327"), Some(1));
    assert_eq!(v.anchor_line(dc, "^svc-ldap"), Some(3));
    assert_eq!(v.anchor_line(dc, "services"), Some(2));
    assert_eq!(v.anchor_line(dc, "DC01#Services"), Some(2));
    assert_eq!(v.anchor_line(dc, "^nope"), None);

    v.rename(
        Path::new("Hosts/dc01.md"),
        Path::new("Hosts/Domain controller.md"),
        true,
    )
    .unwrap();
    let text = get(&root, "Finding.md");
    assert!(
        text.contains("related: \"[[Domain controller]]\""),
        "{text}"
    );
    assert!(
        text.contains("\"[[Hosts/Domain controller|DC]]\""),
        "{text}"
    );
    assert!(text.contains("A[\"[[Domain controller]]\"]"), "{text}");
    assert!(
        text.contains("click A \"Hosts/Domain controller.md#Services\""),
        "{text}"
    );
    assert!(
        text.contains("B[[Subroutine]]") && text.contains("# note: [[not a link]]"),
        "{text}"
    );
}

#[test]
fn a_link_resolves_from_the_root_without_a_note() {
    let (dir, root) = sample("root-links");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    assert!(
        matches!(v.resolve_link_from_root("Kerberoasting"), LinkTarget::Note { id, .. } if id == self::id(&v, "Red/AD/Kerberoasting.md"))
    );
    assert!(
        matches!(v.resolve_link_from_root("Red/00_Index.md#Index Red"), LinkTarget::Note { anchor: Some(a), .. } if a == "Index Red")
    );
    assert!(matches!(
        v.resolve_link_from_root("Nothing"),
        LinkTarget::Unresolved
    ));
}

#[test]
fn duplicate_takes_the_first_free_number_and_tasks_of_one_note() {
    let (dir, root) = sample("duplicate");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let kerb = id(&v, "Red/AD/Kerberoasting.md");
    let copy = v.duplicate(kerb).unwrap();
    assert_eq!(
        &*v.note(copy).unwrap().path,
        Path::new("Red/AD/Kerberoasting 1.md")
    );
    let again = v.duplicate(kerb).unwrap();
    assert_eq!(
        &*v.note(again).unwrap().path,
        Path::new("Red/AD/Kerberoasting 2.md")
    );
    let third = v.duplicate(copy).unwrap();
    assert_eq!(
        &*v.note(third).unwrap().path,
        Path::new("Red/AD/Kerberoasting 3.md")
    );
    assert_eq!(v.read(copy).unwrap(), v.read(kerb).unwrap());
    let tasks = v.tasks_of(kerb);
    assert_eq!(
        tasks
            .iter()
            .map(|t| (t.text.as_str(), t.done))
            .collect::<Vec<_>>(),
        [("tester #ad/kerberos", false), ("fini", true)]
    );
}

#[test]
fn an_h1_that_is_the_file_name_gives_the_stem() {
    let path = Path::new("Wiki/SUMMARY.md");
    assert_eq!(
        title_for(path, Some("SUMMARY.md"), &[]),
        (Arc::from("SUMMARY"), true)
    );
    assert_eq!(
        title_for(path, Some("Summary of the wiki"), &[]),
        (Arc::from("Summary of the wiki"), false)
    );
}

#[test]
fn a_template_placeholder_title_gives_the_stem() {
    let path = Path::new("Templates/Finding draft.md");
    assert_eq!(
        title_for(path, Some("{{title}}"), &[]),
        (Arc::from("Finding draft"), true)
    );
    let props = [("title".to_string(), PropValue::Text("{{title}}".into()))];
    assert_eq!(
        title_for(path, None, &props),
        (Arc::from("Finding draft"), true)
    );
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_vault_is_known_from_the_walk() {
    let dir = test_dir("outside");
    let root = dir.join("vault");
    put(&root, "a.md", "# A\n");
    put(&dir, "elsewhere/secret.md", "# Secret\n");
    put(&dir, "elsewhere/plan.png", "png");
    std::os::unix::fs::symlink(dir.join("elsewhere/secret.md"), root.join("link.md")).unwrap();
    std::os::unix::fs::symlink(dir.join("elsewhere/plan.png"), root.join("plan.png")).unwrap();
    std::os::unix::fs::symlink(root.join("a.md"), root.join("inner.md")).unwrap();
    let v = Vault::open_with(&root, options(&dir, false), ExternalLinks::Follow).unwrap();
    assert_eq!(v.read(id(&v, "link.md")).unwrap(), "# Secret\n");
    assert!(v.is_outside(Path::new("link.md")));
    assert!(v.is_outside(Path::new("plan.png")));
    assert!(!v.is_outside(Path::new("inner.md")));
    assert!(!v.is_outside(Path::new("a.md")));
    let marked: Vec<String> = v
        .tree()
        .nodes
        .iter()
        .filter(|n| n.link_out == Some(LinkOut::Followed))
        .map(|n| n.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(marked, ["link.md", "plan.png"]);
    v.rename(Path::new("link.md"), Path::new("moved.md"), true)
        .unwrap();
    assert!(v.is_outside(Path::new("moved.md")) && !v.is_outside(Path::new("link.md")));
    v.trash(Path::new("moved.md")).unwrap();
    assert!(!v.is_outside(Path::new("moved.md")));
    assert!(dir.join("elsewhere/secret.md").exists());
}

#[test]
fn renaming_rewrites_mdbook_refs_and_includes() {
    let dir = test_dir("mdbook-rename");
    let root = dir.join("vault");
    put(
        &root,
        "src/ad/README.md",
        "# AD\n\n{{#include ../banners/banner.md}}\n\n{{#ref}}\nntlm/NTLM relay.md\n{{#endref}}\n\nvoir [relay]({{#ref}}ntlm/NTLM relay.md{{#endref}})\n",
    );
    put(&root, "src/ad/ntlm/NTLM relay.md", "# Relay\n");
    put(&root, "src/banners/banner.md", "banner\n");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let readme = id(&v, "src/ad/README.md");
    assert!(
        v.unresolved(readme).is_empty(),
        "{:?}",
        v.unresolved(readme)
    );
    assert_eq!(v.backlinks(id(&v, "src/ad/ntlm/NTLM relay.md")).len(), 2);
    v.rename(
        Path::new("src/ad/ntlm/NTLM relay.md"),
        Path::new("src/ad/relay/Relay.md"),
        true,
    )
    .unwrap();
    v.rename(
        Path::new("src/banners/banner.md"),
        Path::new("src/shared/banner.md"),
        true,
    )
    .unwrap();
    assert_eq!(
        get(&root, "src/ad/README.md"),
        "# AD\n\n{{#include ../shared/banner.md}}\n\n{{#ref}}\nrelay/Relay.md\n{{#endref}}\n\nvoir [relay]({{#ref}}relay/Relay.md{{#endref}})\n"
    );
    assert!(v.unresolved(readme).is_empty());
}

#[test]
fn the_fold_table_covers_latin_1_and_extended_a() {
    assert_eq!(fold_char('\u{e9}'), 'e');
    assert_eq!(fold_char('\u{141}'), 'L');
    assert_eq!(fold_char('\u{c6}'), '\u{c6}');
    assert_eq!(fold_char('z'), 'z');
}

#[test]
fn a_missing_include_is_no_broken_link_but_a_missing_ref_is() {
    let dir = test_dir("mdbook-missing");
    let root = dir.join("vault");
    put(
        &root,
        "book/page.md",
        "# Page\n\n{{#include ../banners/banner.md}}\n\n{{#ref}}\nabsent.md\n{{#endref}}\n",
    );
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let page = id(&v, "book/page.md");
    let broken: Vec<(LinkKind, String)> = v
        .unresolved(page)
        .into_iter()
        .map(|l| (l.kind, l.target))
        .collect();
    assert_eq!(broken, [(LinkKind::Markdown, "absent.md".to_string())]);
    assert_eq!(v.all_unresolved().len(), 1);
    assert!(
        v.note(page)
            .unwrap()
            .links
            .iter()
            .any(|l| l.kind == LinkKind::Include)
    );
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_vault_is_not_followed_by_default() {
    let dir = test_dir("outside-skipped");
    let root = dir.join("vault");
    put(&root, "a.md", "# A\n[[link]]\n");
    put(&dir, "elsewhere/secret.md", "# Secret\n");
    std::os::unix::fs::symlink(dir.join("elsewhere/secret.md"), root.join("link.md")).unwrap();
    std::os::unix::fs::symlink(root.join("a.md"), root.join("inner.md")).unwrap();
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    assert!(v.note_by_path(Path::new("link.md")).is_none());
    assert!(v.note_by_path(Path::new("inner.md")).is_some());
    assert_eq!(v.unresolved(id(&v, "a.md")).len(), 1);
    let tree = v.tree();
    let link = tree
        .nodes
        .iter()
        .find(|n| &*n.path == Path::new("link.md"))
        .unwrap();
    assert_eq!(
        (link.kind, link.link_out, link.note),
        (NodeKind::Other, Some(LinkOut::NotFollowed), None)
    );
    assert!(!v.is_outside(Path::new("link.md")));
    v.trash(Path::new("link.md")).unwrap();
    assert!(
        !v.tree()
            .nodes
            .iter()
            .any(|n| &*n.path == Path::new("link.md"))
    );
    assert!(dir.join("elsewhere/secret.md").exists());
}

fn import_all(v: &Vault, sources: &[PathBuf], into: &str, mode: ImportMode) -> ImportReport {
    v.import(
        sources,
        Path::new(into),
        mode,
        &std::sync::atomic::AtomicBool::new(false),
        &|_| {},
    )
    .unwrap()
}

#[test]
fn import_never_overwrites_and_copies_folders_whole() {
    let (dir, root) = sample("import-names");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let outside = dir.join("outside");
    put(&outside, "Box.md", "# Other box\n");
    put(&outside, "Notes/a.md", "# A\n");
    put(&outside, "Notes/deep/b.md", "# B\n[[a]]\n");
    let report = import_all(
        &v,
        &[outside.join("Box.md"), outside.join("Notes")],
        "Labs",
        ImportMode::Copy,
    );
    assert_eq!(
        report.created,
        [PathBuf::from("Labs/Box 2.md"), PathBuf::from("Labs/Notes")]
    );
    assert_eq!(
        report.renamed,
        [(outside.join("Box.md"), PathBuf::from("Labs/Box 2.md"))]
    );
    assert_eq!(
        (report.files, report.skipped.len(), report.cancelled),
        (3, 0, false)
    );
    assert!(
        v.note_by_path(Path::new("Labs/Notes/deep/b.md")).is_some(),
        "indexed at once"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("Labs/Box.md"))
            .unwrap()
            .lines()
            .next(),
        Some("# Box"),
        "the note in place is kept"
    );
    let again = import_all(&v, &[outside.join("Notes")], "Labs", ImportMode::Copy);
    assert_eq!(again.created, [PathBuf::from("Labs/Notes 2")]);
    let duplicate = import_all(&v, &[root.join("Labs/Box.md")], "Labs", ImportMode::Copy);
    assert_eq!(duplicate.created, [PathBuf::from("Labs/Box 3.md")]);
    assert!(
        v.import(
            &[outside.join("Box.md")],
            Path::new(".obsidian"),
            ImportMode::Copy,
            &std::sync::atomic::AtomicBool::new(false),
            &|_| {}
        )
        .is_err()
    );
}

#[test]
#[cfg(unix)]
fn import_follows_no_link_out_and_skips_special_files() {
    let (dir, root) = sample("import-links");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let given = dir.join("given");
    put(&given, "a.md", "# A\n");
    put(&dir, "secret.txt", "out of reach\n");
    std::os::unix::fs::symlink(dir.join("secret.txt"), given.join("out.txt")).unwrap();
    std::os::unix::fs::symlink(given.join("a.md"), given.join("again.md")).unwrap();
    std::os::unix::fs::symlink(&given, given.join("loop")).unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(given.join("socket")).unwrap();
    let report = import_all(&v, std::slice::from_ref(&given), "", ImportMode::Copy);
    let skipped: Vec<(String, SkipReason)> = report
        .skipped
        .iter()
        .map(|(p, r)| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                r.clone(),
            )
        })
        .collect();
    assert_eq!(
        skipped,
        [
            ("loop".into(), SkipReason::SymbolicLink),
            ("out.txt".into(), SkipReason::SymbolicLink),
            ("socket".into(), SkipReason::Special)
        ]
    );
    assert_eq!(
        std::fs::read_to_string(root.join("given/again.md")).unwrap(),
        "# A\n",
        "a link to a file of what was given is copied"
    );
    assert!(!root.join("given/out.txt").exists() && !root.join("given/loop").exists());
}

#[test]
fn import_stops_when_cancelled_and_a_cut_within_keeps_links() {
    let (dir, root) = sample("import-cancel");
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let outside = dir.join("many");
    for i in 0..5 {
        put(&outside, &format!("n{i}.md"), "x\n");
    }
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let report = v
        .import(
            std::slice::from_ref(&outside),
            Path::new(""),
            ImportMode::Copy,
            &cancel,
            &|p| {
                if p.files == 2 {
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            },
        )
        .unwrap();
    assert!(report.cancelled);
    assert_eq!(report.files, 2);
    let box_id = id(&v, "Labs/Box.md");
    let backlinks = v.backlinks(box_id).len();
    let moved = import_all(&v, &[root.join("Labs/Box.md")], "Red", ImportMode::Move);
    assert_eq!(moved.created, [PathBuf::from("Red/Box.md")]);
    assert_eq!(
        v.backlinks(box_id).len(),
        backlinks,
        "links follow the move"
    );
    assert!(!root.join("Labs/Box.md").exists());
}

#[test]
fn a_note_moved_outside_focus_takes_its_versions() {
    let (dir, root) = sample("watch-history");
    let v = Vault::open(&root, options(&dir, true)).unwrap();
    v.set_history(&dir.join("history"), HistoryConfig::default())
        .unwrap();
    let rx = v.subscribe();
    assert!(
        v.inner
            .watcher
            .lock()
            .as_ref()
            .is_some_and(|w| w.wait_ready(Duration::from_secs(60)))
    );
    let box_id = id(&v, "Labs/Box.md");
    let before = get(&root, "Labs/Box.md");
    v.write(box_id, "# Box edited\n", None).unwrap();
    std::fs::rename(root.join("Labs/Box.md"), root.join("Boxed.md")).unwrap();
    wait_for(
        &rx,
        |e| matches!(e, VaultEvent::NoteMoved { id, .. } if *id == box_id),
    );
    let history = v.history().unwrap();
    let versions = history.list(Path::new("Boxed.md")).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(
        history
            .read(Path::new("Boxed.md"), &versions[0].id)
            .unwrap(),
        before
    );
    assert!(v.take_history_error().is_none());
}

#[test]
fn property_types_follow_types_json_on_disk() {
    let dir = test_dir("types-json");
    let root = dir.join("vault");
    put(&root, "Note.md", "---\ndue: 2020-08-21\n---\n# Note\n");
    let v = Vault::open(&root, options(&dir, true)).unwrap();
    let config = v.subscribe_config();
    assert!(
        v.inner
            .watcher
            .lock()
            .as_ref()
            .is_some_and(|w| w.wait_ready(Duration::from_secs(60)))
    );
    let changed = || {
        config
            .recv_timeout(Duration::from_secs(60))
            .expect("a configuration event")
    };
    let date = PropValue::Text("2020-08-21".into());
    let kind = || {
        let k = v.property_type("due", &date);
        (k.kind, k.origin)
    };
    assert_eq!(kind(), (PropertyType::Date, TypeOrigin::Value));
    assert!(v.obsidian().property_types.is_empty());

    put(
        &root,
        ".obsidian/types.json",
        r#"{"types":{"due":"datetime"}}"#,
    );
    assert_eq!(changed(), ConfigEvent::PropertyTypes);
    assert_eq!(kind(), (PropertyType::Datetime, TypeOrigin::Obsidian));

    put(&root, ".obsidian/types.json", r#"{"types":{"due":"text"}}"#);
    changed();
    assert_eq!(kind(), (PropertyType::Text, TypeOrigin::Obsidian));

    put(&root, ".obsidian/types.json", "{\"types\": {\"due\": ");
    changed();
    assert_eq!(kind(), (PropertyType::Date, TypeOrigin::Value));

    put(
        &root,
        ".obsidian/types.json",
        r#"{"types":{"due":"number"}}"#,
    );
    changed();
    assert_eq!(kind(), (PropertyType::Number, TypeOrigin::Obsidian));

    put(&root, ".obsidian/types.json", r#"{"due":"number"}"#);
    changed();
    assert_eq!(kind(), (PropertyType::Date, TypeOrigin::Value));

    put(
        &root,
        ".obsidian/types.json",
        r#"{"types":{"due":"datetime"}}"#,
    );
    changed();
    v.set_chosen_property_types([("due".to_string(), PropertyType::Text)].into());
    assert_eq!(changed(), ConfigEvent::PropertyTypes);
    let k = v.property_type("due", &date);
    assert_eq!(
        (k.kind, k.origin, k.obsidian),
        (
            PropertyType::Text,
            TypeOrigin::Focus,
            Some(PropertyType::Datetime)
        )
    );
    assert_eq!(
        get(&root, ".obsidian/types.json"),
        r#"{"types":{"due":"datetime"}}"#
    );
    v.set_chosen_property_types(v.chosen_property_types());
    assert!(config.try_recv().is_err());

    v.set_chosen_property_types(BTreeMap::new());
    changed();
    std::fs::remove_file(root.join(".obsidian/types.json")).unwrap();
    changed();
    assert_eq!(kind(), (PropertyType::Date, TypeOrigin::Value));
}

#[test]
fn types_json_is_read_at_the_opening() {
    let dir = test_dir("types-json-open");
    let root = dir.join("vault");
    put(&root, "Note.md", "# Note\n");
    put(
        &root,
        ".obsidian/types.json",
        r#"{"types":{"aliases":"aliases","cssclasses":"multitext","tags":"tags","due":"date","done":"checkbox","weird":"widget"}}"#,
    );
    let v = Vault::open(&root, options(&dir, false)).unwrap();
    let types = v.obsidian().property_types;
    assert_eq!(types.get("done"), Some(&PropertyType::Checkbox));
    assert!(!types.contains_key("weird"));
    let k = v.property_type("Done", &PropValue::Null);
    assert_eq!(
        (k.kind, k.origin),
        (PropertyType::Checkbox, TypeOrigin::Obsidian)
    );
}
