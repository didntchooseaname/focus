use std::path::{Path, PathBuf};
use std::sync::Arc;

use focus_core::{OpenOptions, Vault};
use focus_search::SearchIndex;

pub fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(format!("mcp-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

pub fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

pub fn populate(root: &Path) {
    write(
        root,
        "Home.md",
        "# Home\n\nWelcome. See [[Red Team/Kerberoasting]] et [[Notes/Isolated note]].\n\n- [ ] review the home page\n- [x] create the vault\n",
    );
    write(
        root,
        "Red Team/Kerberoasting.md",
        "---\ntags: [red/ad]\n---\n# Kerberoasting\n\nAn attack on service accounts.\n\n## Detection\n\nWatch ticket requests. Read [[Defense]] et [[Missing]].\n\n![](assets/capture.png)\n\n- [ ] write the procedure\n",
    );
    write(
        root,
        "Red Team/Defense.md",
        "# Defense\n\nHarden the accounts. Back to [[Red Team/Kerberoasting]].\n\n#blue/ad\n",
    );
    write(
        root,
        "Notes/Isolated note.md",
        "# Isolated\n\nA note that nothing links to.\n",
    );
    write(
        root,
        "Notes/Bridge.md",
        "# Bridge\n\nLinks [[Home]] and [[Red Team/Defense]].\n",
    );
    std::fs::create_dir_all(root.join("Red Team/assets")).unwrap();
    std::fs::write(
        root.join("Red Team/assets/capture.png"),
        b"\x89PNG\r\n\x1a\nfake",
    )
    .unwrap();
    std::fs::write(root.join("orphan.png"), b"\x89PNG\r\n\x1a\nfake2").unwrap();
}

pub struct Fixture {
    pub root: PathBuf,
    pub vault: Arc<Vault>,
    pub index: Arc<SearchIndex>,
}

pub fn open(name: &str) -> Fixture {
    let base = scratch(name);
    let root = base.join("vault");
    std::fs::create_dir_all(&root).unwrap();
    populate(&root);
    open_at(&root, &base.join("cache"))
}

pub fn open_at(root: &Path, cache: &Path) -> Fixture {
    let vault = Vault::open(
        root,
        OpenOptions {
            cache_dir: cache.to_path_buf(),
            ignore: Vec::new(),
            watch: false,
        },
    )
    .unwrap();
    let index = SearchIndex::open(vault.clone(), cache, &|_| {}).unwrap();
    Fixture {
        root: root.to_path_buf(),
        vault,
        index,
    }
}

use focus_ai::agent::{ToolHost, ToolOutcome};
use focus_ai::{Cancel, ToolCall};

pub fn run(tools: &dyn ToolHost, name: &str, arguments: serde_json::Value) -> ToolOutcome {
    tools.call(
        &ToolCall {
            id: "t".into(),
            name: name.into(),
            arguments: arguments.to_string(),
        },
        &Cancel::new(),
    )
}

pub fn ok(outcome: ToolOutcome) -> String {
    match outcome {
        ToolOutcome::Ok(text) => text,
        other => panic!("expected Ok, got {other:?}"),
    }
}

pub fn error(outcome: ToolOutcome) -> String {
    match outcome {
        ToolOutcome::Error(text) => text,
        other => panic!("expected Error, got {other:?}"),
    }
}
