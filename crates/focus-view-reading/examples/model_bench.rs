use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;

use anyhow::Context as _;
use focus_core::{OpenOptions, Vault};

struct Counting;

static LIVE: AtomicIsize = AtomicIsize::new(0);

// SAFETY: forwards to the system allocator and only counts the sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: same layout as received.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: the pointer came from `alloc` with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LIVE.fetch_add(
            new_size as isize - layout.size() as isize,
            Ordering::Relaxed,
        );
        // SAFETY: the pointer came from `alloc` with this layout.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().context("expected <vault> <note path>...")?);
    focus_i18n::init(None);
    let cache =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/demo-cache/model-bench");
    let vault = Vault::open(
        &root,
        OpenOptions {
            cache_dir: cache,
            ignore: Vec::new(),
            watch: false,
        },
    )?;
    for path in args {
        let id = vault
            .note_by_path(Path::new(&path))
            .with_context(|| format!("no note {path}"))?;
        let text = vault.read(id)?;
        let meta = vault.note(id);
        let (mut parse, mut build) = (Vec::new(), Vec::new());
        let mut kept = 0;
        for _ in 0..9 {
            let t = Instant::now();
            let document = focus_md::parse(&text, &focus_md::ParseOptions::default());
            parse.push(t.elapsed().as_secs_f64() * 1000.);
            let before = LIVE.load(Ordering::Relaxed);
            let t = Instant::now();
            let built = focus_view_reading::build(&document, meta.as_deref(), vault.as_ref());
            build.push(t.elapsed().as_secs_f64() * 1000.);
            kept = LIVE.load(Ordering::Relaxed) - before;
            drop(built);
        }
        println!(
            "{path} ({} bytes): parse {:.2} ms, model {:.2} ms (median of 9), model kept on the heap {:.1} KiB",
            text.len(),
            median(parse),
            median(build),
            kept as f64 / 1024.
        );
    }
    Ok(())
}
