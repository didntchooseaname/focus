use crate::{Usage, timefmt};

const PRICES: &[(&str, [f64; 3], [f64; 3])] = &[
    ("deepseek-flash", [0.006, 0.30, 1.20], [0.003, 0.15, 0.60]),
    (
        "deepseek-v4-flash",
        [0.006, 0.30, 1.20],
        [0.003, 0.15, 0.60],
    ),
    ("deepseek-v4-pro", [0.044, 1.32, 3.96], [0.022, 0.66, 1.98]),
];

const ANTHROPIC_PRICES: &[(&str, [f64; 3])] = &[
    ("claude-fable-5-2", [0.20, 8.0, 40.0]),
    ("claude-opus-5-5", [0.20, 4.0, 20.0]),
    ("claude-sonnet-5-5", [0.10, 2.0, 10.0]),
    ("claude-haiku-5-5", [0.01, 0.10, 0.50]),
];

pub fn estimate_cost_usd(model: &str, usage: &Usage) -> Option<f64> {
    estimate_cost_usd_at(model, usage, timefmt::now_secs())
}

pub fn estimate_cost_usd_at(model: &str, usage: &Usage, unix_secs: u64) -> Option<f64> {
    if let Some((_, [hit, miss, out])) = ANTHROPIC_PRICES.iter().find(|(name, _)| *name == model) {
        return Some(
            (f64::from(usage.cache_hit_tokens) * hit
                + f64::from(usage.cache_miss_tokens) * miss
                + f64::from(usage.completion_tokens) * out)
                / 1e6,
        );
    }
    let (_, peak, off_peak) = PRICES.iter().find(|(name, _, _)| *name == model)?;
    let hour = timefmt::hour_utc(unix_secs);
    let is_peak =
        timefmt::weekday(unix_secs) < 5 && ((1..4).contains(&hour) || (6..10).contains(&hour));
    let [hit, miss, out] = if is_peak { peak } else { off_peak };
    let miss_tokens = if usage.cache_hit_tokens + usage.cache_miss_tokens == 0 {
        usage.prompt_tokens
    } else {
        usage.cache_miss_tokens
    };
    Some(
        (f64::from(usage.cache_hit_tokens) * hit
            + f64::from(miss_tokens) * miss
            + f64::from(usage.completion_tokens) * out)
            / 1e6,
    )
}

pub fn estimate_tokens(text: &str) -> u32 {
    let mut tokens = 0.0f64;
    for c in text.chars() {
        tokens += if c.is_ascii()
            || ('\u{00C0}'..='\u{024F}').contains(&c)
            || matches!(c, '’' | '‘' | '“' | '”' | '…' | '€' | '«' | '»')
        {
            0.3
        } else if c.is_whitespace() {
            0.0
        } else {
            0.6
        };
    }
    tokens.ceil() as u32
}
