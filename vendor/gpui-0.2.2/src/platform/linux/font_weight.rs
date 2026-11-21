//! Focus patch: font matching by weight, for the fallback faces of `text_system.rs`.

/// Focus: the weight among `available` that CSS font matching picks for `desired`.
pub fn nearest_weight(desired: u16, available: &[u16]) -> Option<u16> {
    let at_most = |limit: u16| available.iter().copied().filter(|w| *w <= limit).max();
    let at_least = |limit: u16| available.iter().copied().filter(|w| *w >= limit).min();
    match desired {
        400..=500 => available
            .iter()
            .copied()
            .filter(|w| (desired..=500).contains(w))
            .min()
            .or_else(|| at_most(desired))
            .or_else(|| at_least(desired)),
        0..400 => at_most(desired).or_else(|| at_least(desired)),
        _ => at_least(desired).or_else(|| at_most(desired)),
    }
}
