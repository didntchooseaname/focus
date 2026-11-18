use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub(crate) fn local_stamp(unix: u64) -> String {
    let seconds = unix as libc::time_t;
    // SAFETY: `localtime_r` writes into the zeroed `tm` it is given.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        #[cfg(unix)]
        libc::localtime_r(&seconds, &mut tm);
        #[cfg(windows)]
        libc::localtime_s(&mut tm, &seconds);
        format!(
            "{:04}-{:02}-{:02}-{:02}{:02}{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

pub(crate) fn local_iso(unix: u64) -> String {
    let stamp = local_stamp(unix);
    let (date, time) = stamp.rsplit_once('-').unwrap_or((&stamp, "000000"));
    format!("{date}T{}:{}:{}", &time[0..2], &time[2..4], &time[4..6])
}

pub(crate) fn clock_text(seconds: u64) -> String {
    format!(
        "{}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use focus_ops::timer::iso_to_unix as local_iso_to_unix;

    use super::*;

    #[test]
    fn clock_text_runs_past_a_day() {
        assert_eq!(clock_text(0), "0:00:00");
        assert_eq!(clock_text(3661), "1:01:01");
        assert_eq!(clock_text(24 * 3600 + 59), "24:00:59");
    }

    #[test]
    fn a_stamp_reads_back_to_the_same_second() {
        let now = 1_794_441_600;
        let stamp = local_stamp(now);
        let (date, time) = stamp.rsplit_once('-').unwrap();
        let iso = format!("{date}T{}:{}:{}", &time[..2], &time[2..4], &time[4..]);
        assert_eq!(local_iso_to_unix(&iso), Some(now));
    }

    #[test]
    fn iso_dates_and_times_parse() {
        let day = local_iso_to_unix("2026-11-12").unwrap();
        assert_eq!(
            local_iso_to_unix("2026-11-12T01:00:00").unwrap() - day,
            3600
        );
        assert_eq!(local_iso_to_unix("2026-11-12 00:00:30").unwrap() - day, 30);
        assert!(local_iso_to_unix("not a date").is_none());
    }
}
