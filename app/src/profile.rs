use std::sync::OnceLock;
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();
static ENABLED: OnceLock<bool> = OnceLock::new();

pub fn start() {
    START.get_or_init(Instant::now);
    ENABLED.get_or_init(|| std::env::var("FOCUS_PROFILE").is_ok_and(|v| v == "1"));
}

pub fn elapsed_ms() -> f32 {
    START
        .get()
        .map_or(0., |start| start.elapsed().as_secs_f32() * 1000.)
}

pub fn mark(label: &str) -> f32 {
    let ms = elapsed_ms();
    if ENABLED.get().copied().unwrap_or(false) {
        let mib = |b: u64| b as f64 / (1024. * 1024.);
        eprintln!(
            "focus: {ms:7.1} ms  heap {:6.1} MiB  anon {:6.1} MiB  {label}",
            mib(live_heap() as u64),
            mib(anonymous().unwrap_or(0))
        );
    }
    ms
}

pub struct Counting;

static LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

// SAFETY: every call is forwarded to the system allocator unchanged; only a counter is kept.
unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size(), std::sync::atomic::Ordering::Relaxed);
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        LIVE.fetch_sub(layout.size(), std::sync::atomic::Ordering::Relaxed);
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        LIVE.fetch_add(new_size, std::sync::atomic::Ordering::Relaxed);
        LIVE.fetch_sub(layout.size(), std::sync::atomic::Ordering::Relaxed);
        unsafe { std::alloc::System.realloc(ptr, layout, new_size) }
    }
}

pub fn live_heap() -> usize {
    LIVE.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn anonymous() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/smaps_rollup").ok()?;
    let line = text.lines().find(|l| l.starts_with("Anonymous:"))?;
    Some(line.split_whitespace().nth(1)?.parse::<u64>().ok()? * 1024)
}
