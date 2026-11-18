pub fn civil_date(secs: u64) -> (i64, u32, u32) {
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

pub fn format_utc(secs: u64) -> String {
    let (y, m, d) = civil_date(secs);
    let minutes = secs % 86_400 / 60;
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        minutes / 60,
        minutes % 60
    )
}

pub fn weekday(secs: u64) -> u32 {
    (((secs / 86_400) + 3) % 7) as u32
}

pub fn hour_utc(secs: u64) -> u32 {
    (secs % 86_400 / 3600) as u32
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
