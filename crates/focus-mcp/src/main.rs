use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use focus_core::{OpenOptions, Vault, settings};
use focus_mcp::VaultTools;
use focus_mcp::server::{ServerInfo, serve};
use focus_search::SearchIndex;

const HELP: &str =
    "focus-mcp: MCP (Model Context Protocol) server on stdio for a vault of Markdown notes

Usage: focus-mcp --vault <path> [--write] [--cache-dir <path>] [--ops-dir <path>]
               [--reveal-secrets] [--export-dir <path>]

Options:
  --vault <path>        Vault to expose (required).
  --write               Expose the write tools (create_note, append_to_note,
                        replace_in_note, toggle_task, rename_note, trash_note,
                        export_note, and the writing ops_* and app_* tools). They run
                        WITHOUT any confirmation: the calling agent carries the
                        authorization. Without this option the server is read-only and
                        the write tools do not exist.
  --cache-dir <path>    Cache of the vault and of the search index. Default: the Focus
                        cache for this vault (~/.cache/focus/<fingerprint of the path>).
  --ops-dir <path>      Folder of the Ops mode engagements, relative to the vault or
                        absolute. Default: the folder from the Focus settings
                        (Engagements). The ops_* tools exist only if this folder exists.
  --reveal-secrets      Show engagement secrets in clear text: in the ops_* tools
                        (credentials, sensitive variables, report, journal) and in the
                        notes of the engagements, including Loot/. Without it Loot/ is
                        not served at all and the other engagement notes are masked.
  --export-dir <path>   The only folder export_note writes to, relative to the vault
                        or absolute (Export by default). An export never overwrites.
  -h, --help            Print this help.
  -V, --version         Print the version.

When the Focus application is open on this vault, the app_* tools drive it (current note,
open a note, search, graph, insert text, notify) through a local socket in
$XDG_RUNTIME_DIR/focus. Otherwise they answer that it is not open.

The protocol goes through standard input and output, one JSON message per line. Standard
output carries nothing else; diagnostics go to standard error. The server stops when
standard input is closed. Changes made to the vault while it runs are picked up.
";

struct Args {
    vault: PathBuf,
    write: bool,
    cache_dir: Option<PathBuf>,
    ops_dir: Option<PathBuf>,
    reveal_secrets: bool,
    export_dir: Option<PathBuf>,
}

struct NoResolver;

impl focus_md::export::Resolver for NoResolver {
    fn image(&self, _: &str) -> Option<PathBuf> {
        None
    }
    fn link(&self, _: &str, _: Option<&str>) -> Option<String> {
        None
    }
    fn embed(&self, _: &str) -> Option<String> {
        None
    }
}

fn probe_export() {
    let options = focus_md::export::HtmlOptions {
        title: "probe".into(),
        embed_images: false,
        render_mermaid: false,
        embed_fonts: false,
        ..focus_md::export::HtmlOptions::default()
    };
    focus_md::export::to_standard_markdown("# probe\n", &NoResolver);
    focus_md::export::to_html("# probe\n", &options, &NoResolver);
}

fn export_available() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            std::process::Command::new(exe)
                .arg("--probe-export")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .ok()
        })
        .is_some_and(|s| s.success())
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut vault = None;
    let mut write = false;
    let mut cache_dir = None;
    let mut ops_dir = None;
    let mut reveal_secrets = false;
    let mut export_dir = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(None);
            }
            "--probe-export" => {
                probe_export();
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("focus-mcp {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--write" => write = true,
            "--vault" => vault = Some(PathBuf::from(args.next().ok_or("--vault needs a path")?)),
            "--cache-dir" => {
                cache_dir = Some(PathBuf::from(
                    args.next().ok_or("--cache-dir needs a path")?,
                ))
            }
            "--ops-dir" => {
                ops_dir = Some(PathBuf::from(args.next().ok_or("--ops-dir needs a path")?))
            }
            "--export-dir" => {
                export_dir = Some(PathBuf::from(
                    args.next().ok_or("--export-dir needs a path")?,
                ))
            }
            "--reveal-secrets" => reveal_secrets = true,
            other => return Err(format!("unknown option: {other}. See --help.")),
        }
    }
    let vault = vault.ok_or("--vault is required. See --help.")?;
    Ok(Some(Args {
        vault,
        write,
        cache_dir,
        ops_dir,
        reveal_secrets,
        export_dir,
    }))
}

fn run(args: Args) -> anyhow::Result<()> {
    let started = Instant::now();
    let export_probe = std::thread::spawn(export_available);
    anyhow::ensure!(
        args.vault.is_dir(),
        "the vault does not exist or is not a folder: {}",
        args.vault.display()
    );
    let root = dunce::canonicalize(&args.vault)?;
    let cache_dir = args
        .cache_dir
        .unwrap_or_else(|| settings::cache_dir_for(&root));
    let vault = Vault::open(
        &root,
        OpenOptions {
            cache_dir: cache_dir.clone(),
            ignore: Vec::new(),
            watch: true,
        },
    )?;
    let index = SearchIndex::open(vault.clone(), &cache_dir, &|_| {})?;
    let events = vault.subscribe();
    let watcher_index = index.clone();
    std::thread::Builder::new()
        .name("focus-mcp-watch".into())
        .spawn(move || {
            for event in events {
                watcher_index.apply(&event);
            }
        })?;
    let stats = vault.stats();
    eprintln!(
        "focus-mcp: {} notes, ready in {:.0} ms ({})",
        stats.notes,
        started.elapsed().as_secs_f32() * 1000.0,
        if args.write {
            "writes enabled"
        } else {
            "read-only"
        }
    );

    let settings = match settings::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("focus-mcp: unreadable Focus settings, using defaults ({e})");
            settings::Settings::default()
        }
    };
    if args.write {
        let history_dir = settings::history_dir_for(vault.root());
        vault.set_history(&history_dir, settings.history)?;
        if let Some(history) = vault.history() {
            focus_ops::attach_history(history, vault.root());
        }
    }
    let saved: Vec<focus_ai::prompts::Template> = settings
        .ai_actions
        .iter()
        .filter_map(|v| match serde_json::from_value(v.clone()) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("focus-mcp: AI action ignored in the settings ({e})");
                None
            }
        })
        .collect();
    let ops_dir = args.ops_dir.map_or_else(
        || root.join(&settings.ops_folder),
        |d| if d.is_absolute() { d } else { root.join(d) },
    );
    let mut tools = VaultTools::new(vault, index.clone())
        .with_extended_tools()
        .with_structured_errors()
        .with_export_probe(export_probe)
        .with_ops(ops_dir, args.reveal_secrets)
        .with_app(focus_mcp::ipc::Client::for_vault(&root))
        .with_prompt_templates(focus_ai::prompts::merged(&saved));
    if let Some(dir) = args.export_dir {
        tools = tools.with_export_dir(dir);
    }
    if args.write {
        tools = tools.with_unconfirmed_writes();
    }
    let info = ServerInfo {
        name: "focus-mcp".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        instructions: "Tools to read a vault of Markdown notes (search, read, outline, links, tags, graph, tasks, attachments, export), Ops mode when the engagements folder exists, and to drive the Focus application when it is open".to_string()
            + if args.write { ", and to write to it. Paths are relative to the vault root. Line numbers start at 1." } else { ". Paths are relative to the vault root. Line numbers start at 1. This server is read-only." },
    };
    let stdin = std::io::stdin();
    serve(stdin.lock(), std::io::stdout(), Arc::new(tools), &info)?;
    if let Err(e) = index.persist() {
        eprintln!("focus-mcp: {e}");
    }
    Ok(())
}

fn main() {
    match parse_args() {
        Ok(None) => {}
        Ok(Some(args)) => {
            if let Err(e) = run(args) {
                eprintln!("focus-mcp: {e}");
                std::process::exit(1);
            }
        }
        Err(message) => {
            eprintln!("focus-mcp: {message}");
            std::process::exit(2);
        }
    }
}
