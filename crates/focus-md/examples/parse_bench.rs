use std::time::Instant;

use focus_md::{ParseOptions, parse};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("expected <note.md> [runs]"))?;
    let runs: usize = args.next().map_or(Ok(200), |n| n.parse())?;
    let text = std::fs::read_to_string(&path)?;
    let options = ParseOptions::default();
    for _ in 0..20 {
        std::hint::black_box(parse(&text, &options));
    }
    let mut times: Vec<f64> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            std::hint::black_box(parse(&text, &options));
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    let at = |q: f64| times[((times.len() - 1) as f64 * q).round() as usize];
    println!(
        "{} bytes, {runs} runs: median {:.3} ms, p90 {:.3} ms",
        text.len(),
        at(0.5),
        at(0.9)
    );
    Ok(())
}
