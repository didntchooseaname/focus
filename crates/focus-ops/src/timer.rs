use crate::Engagement;

pub fn iso_to_unix(text: &str) -> Option<u64> {
    let text = text.trim();
    let (date, time) = text.split_once(['T', ' ']).unwrap_or((text, "00:00:00"));
    let mut d = date.split('-').map(str::parse::<i32>);
    let (year, month, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.trim_end_matches('Z').split(':').map(str::parse::<i32>);
    let (hour, minute, second) = (
        t.next().transpose().ok()?.unwrap_or(0),
        t.next().transpose().ok()?.unwrap_or(0),
        t.next().transpose().ok()?.unwrap_or(0),
    );
    // SAFETY: `tm` is zero-initialised plain data, filled before `mktime` reads it.
    let seconds = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        tm.tm_year = year - 1900;
        tm.tm_mon = month - 1;
        tm.tm_mday = day;
        tm.tm_hour = hour;
        tm.tm_min = minute;
        tm.tm_sec = second;
        tm.tm_isdst = -1;
        #[cfg(unix)]
        let seconds = libc::mktime(&mut tm);
        #[cfg(windows)]
        let seconds = _mktime64(&mut tm);
        seconds
    };
    u64::try_from(seconds).ok()
}

pub fn unix_to_iso(unix: u64) -> String {
    let (year, month, day, hour, minute, second) = crate::fsutil::local_parts(unix as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    NotStarted,
    Running,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Timer {
    pub started: Option<u64>,
    pub pauses: Vec<(u64, Option<u64>)>,
    pub stopped: Option<u64>,
}

impl Timer {
    pub fn of(engagement: &Engagement) -> Timer {
        let pauses = engagement
            .pauses
            .iter()
            .filter_map(|pair| {
                let (start, end) = pair.split_once('/')?;
                Some((iso_to_unix(start)?, iso_to_unix(end)))
            })
            .collect();
        Timer {
            started: engagement.started.as_deref().and_then(iso_to_unix),
            pauses,
            stopped: engagement.stopped.as_deref().and_then(iso_to_unix),
        }
    }

    pub fn state(&self) -> State {
        match (self.started, self.stopped) {
            (None, _) => State::NotStarted,
            (Some(_), Some(_)) => State::Stopped,
            (Some(_), None) if self.pauses.last().is_some_and(|(_, end)| end.is_none()) => {
                State::Paused
            }
            _ => State::Running,
        }
    }

    pub fn worked(&self, now: u64) -> u64 {
        let Some(started) = self.started else {
            return 0;
        };
        let end = self.stopped.unwrap_or(now).max(started);
        let paused: u64 = self
            .pauses
            .iter()
            .map(|(from, to)| {
                to.unwrap_or(end)
                    .min(end)
                    .saturating_sub((*from).max(started))
            })
            .sum();
        (end - started).saturating_sub(paused)
    }

    pub fn clock_now(&self, now: u64) -> u64 {
        self.started
            .map_or(now, |started| started + self.worked(now))
    }
}

fn write(engagement: &mut Engagement, timer: &Timer) {
    engagement.pauses = timer
        .pauses
        .iter()
        .map(|(from, to)| {
            format!(
                "{}/{}",
                unix_to_iso(*from),
                to.map(unix_to_iso).unwrap_or_default()
            )
        })
        .collect();
    engagement.stopped = timer.stopped.map(unix_to_iso);
}

pub fn start(engagement: &mut Engagement, now: u64) {
    if engagement.started.is_none() {
        engagement.started = Some(unix_to_iso(now));
    }
}

pub fn pause(engagement: &mut Engagement, now: u64) -> bool {
    let mut timer = Timer::of(engagement);
    if timer.state() != State::Running {
        return false;
    }
    timer.pauses.push((now, None));
    write(engagement, &timer);
    true
}

pub fn resume(engagement: &mut Engagement, now: u64) -> bool {
    let mut timer = Timer::of(engagement);
    if timer.state() != State::Paused {
        return false;
    }
    if let Some(last) = timer.pauses.last_mut() {
        last.1 = Some(now);
    }
    write(engagement, &timer);
    true
}

pub fn stop(engagement: &mut Engagement, now: u64) -> bool {
    let mut timer = Timer::of(engagement);
    if matches!(timer.state(), State::NotStarted | State::Stopped) {
        return false;
    }
    if let Some(last) = timer.pauses.last_mut().filter(|(_, end)| end.is_none()) {
        last.1 = Some(now);
    }
    timer.stopped = Some(now);
    write(engagement, &timer);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngagementKind, EngagementStatus};

    fn engagement() -> Engagement {
        Engagement {
            dir: Default::default(),
            name: "x".into(),
            kind: EngagementKind::Exam,
            status: EngagementStatus::InProgress,
            client: None,
            started: None,
            deadline: None,
            scope: vec![],
            out_of_scope: vec![],
            vars: Default::default(),
            var_links: Default::default(),
            pauses: vec![],
            stopped: None,
            exam: None,
            exam_done: vec![],
            focus: None,
        }
    }

    const T0: u64 = 1_794_441_600;

    #[test]
    fn an_iso_time_reads_back_to_the_same_second() {
        assert_eq!(iso_to_unix(&unix_to_iso(T0 + 12_345)), Some(T0 + 12_345));
    }

    #[test]
    fn the_time_worked_leaves_the_pauses_out() {
        let mut e = engagement();
        assert_eq!(Timer::of(&e).state(), State::NotStarted);
        start(&mut e, T0);
        assert_eq!(Timer::of(&e).worked(T0 + 600), 600);
        assert!(pause(&mut e, T0 + 600));
        assert_eq!(Timer::of(&e).state(), State::Paused);
        assert_eq!(
            Timer::of(&e).worked(T0 + 5000),
            600,
            "the clock stands still while paused"
        );
        assert!(resume(&mut e, T0 + 1800));
        assert_eq!(Timer::of(&e).worked(T0 + 2400), 600 + 600);
        assert!(pause(&mut e, T0 + 3000) && resume(&mut e, T0 + 3300));
        assert_eq!(Timer::of(&e).worked(T0 + 4000), 4000 - 1200 - 300);
        assert_eq!(e.pauses.len(), 2);
    }

    #[test]
    fn stopping_ends_the_clock_even_during_a_pause() {
        let mut e = engagement();
        start(&mut e, T0);
        pause(&mut e, T0 + 100);
        assert!(stop(&mut e, T0 + 700));
        let timer = Timer::of(&e);
        assert_eq!(
            (timer.state(), timer.worked(T0 + 99_999)),
            (State::Stopped, 100)
        );
        assert!(!pause(&mut e, T0 + 800) && !resume(&mut e, T0 + 800) && !stop(&mut e, T0 + 800));
    }

    #[test]
    fn only_the_right_moves_are_allowed() {
        let mut e = engagement();
        assert!(!pause(&mut e, T0) && !stop(&mut e, T0));
        start(&mut e, T0);
        assert!(!resume(&mut e, T0 + 1));
        start(&mut e, T0 + 50);
        assert_eq!(
            Timer::of(&e).started,
            Some(T0),
            "starting twice keeps the first start"
        );
    }

    #[test]
    fn pauses_and_the_stop_survive_a_save_and_a_load() {
        let ops = crate::fsutil::test_dir("timer-save");
        let created = crate::create(
            &ops,
            &crate::NewEngagement {
                name: "Timer".into(),
                kind: EngagementKind::Lab,
                client: None,
                scope: vec![],
                exam: None,
                checklist: None,
            },
        )
        .unwrap();
        let mut e = crate::load(&created.dir).unwrap();
        start(&mut e, T0);
        pause(&mut e, T0 + 60);
        resume(&mut e, T0 + 120);
        pause(&mut e, T0 + 200);
        crate::save(&e).unwrap();
        let again = crate::load(&created.dir).unwrap();
        assert_eq!(Timer::of(&again), Timer::of(&e));
        assert_eq!(Timer::of(&again).state(), State::Paused);
        let mut stopped = again;
        stop(&mut stopped, T0 + 300);
        crate::save(&stopped).unwrap();
        assert_eq!(
            Timer::of(&crate::load(&created.dir).unwrap()).worked(T0 + 9999),
            60 + 80
        );
    }

    #[test]
    fn the_exam_clock_runs_on_worked_time() {
        let mut e = engagement();
        start(&mut e, T0);
        pause(&mut e, T0 + 3600);
        assert_eq!(Timer::of(&e).clock_now(T0 + 7200), T0 + 3600);
    }
}

#[cfg(windows)]
unsafe extern "C" {
    fn _mktime64(tm: *mut libc::tm) -> i64;
}
