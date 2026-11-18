use std::path::PathBuf;
use std::time::Instant;

use focus_view_media::pages::{GAP, Zoom};
use focus_view_media::pdf::Region;
use focus_view_media::pdf::worker::{Answer, Client, Request, WORKER_ARG, serve_stdio};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(WORKER_ARG) {
        std::process::exit(serve_stdio());
    }
    let path = PathBuf::from(
        args.get(1)
            .expect("usage: first-page <file.pdf> [width] [scale]"),
    );
    let width: f32 = args.get(2).and_then(|w| w.parse().ok()).unwrap_or(1000.);
    let scale: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.);
    let start = Instant::now();
    let (client, opened) = Client::spawn(&path).expect("starting the worker");
    let Ok(Ok(Answer::Opened { sizes })) = smol::block_on(opened.recv()) else {
        panic!("the document did not open");
    };
    let opened_ms = start.elapsed().as_secs_f64() * 1e3;
    let zoom = Zoom::FitWidth.resolve(&sizes, (width, 800.), 0);
    let render = |page: usize| {
        let region = Region::whole(sizes[page], zoom * scale).expect("page too large");
        match smol::block_on(client.send(Request::Render { page, region }).recv()) {
            Ok(Ok(Answer::Bitmap(bitmap))) => bitmap,
            other => panic!("page {page}: {other:?}"),
        }
    };
    let first = render(0);
    let first_ms = start.elapsed().as_secs_f64() * 1e3;
    let count = sizes.len().min(20);
    let pages = Instant::now();
    for page in 0..count {
        render(page);
    }
    let per_page = pages.elapsed().as_secs_f64() * 1e3 / count as f64;
    println!(
        "{} pages, open {opened_ms:.1} ms, first page {first_ms:.1} ms ({}x{} px, area {width} px less {GAP} px each side, scale {scale}), {per_page:.1} ms per page over {count} pages",
        sizes.len(),
        first.width,
        first.height,
    );
}
