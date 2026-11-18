use std::hint::black_box;
use std::time::Instant;

fn main() {
    const ROUNDS: u32 = 2_000_000;
    for code in ["en", "fr", "ar", "zh-CN"] {
        focus_i18n::set_language(code).expect("embedded language");
        for _ in 0..10_000 {
            black_box(focus_i18n::t("ui-cmd-open-note"));
        }
        let started = Instant::now();
        for _ in 0..ROUNDS {
            black_box(focus_i18n::t("ui-cmd-open-note"));
        }
        let plain = started.elapsed().as_nanos() as f64 / f64::from(ROUNDS);
        let started = Instant::now();
        for i in 0..ROUNDS {
            black_box(focus_i18n::t_args(
                "ui-status-words",
                &[("count", focus_i18n::Arg::Int(i64::from(i % 50)))],
            ));
        }
        let with_args = started.elapsed().as_nanos() as f64 / f64::from(ROUNDS);
        println!("{code}: t {plain:.0} ns, t_args with plural {with_args:.0} ns");
    }
}
