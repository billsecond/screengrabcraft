//! Local time, for naming captures and stamping the log.

use screengrabcraft_model::Stamp;

pub fn now() -> Stamp {
    let z = jiff::Zoned::now();
    Stamp {
        year: i32::from(z.year()),
        month: u8::try_from(z.month()).unwrap_or(1),
        day: u8::try_from(z.day()).unwrap_or(1),
        hour: u8::try_from(z.hour()).unwrap_or(0),
        minute: u8::try_from(z.minute()).unwrap_or(0),
        second: u8::try_from(z.second()).unwrap_or(0),
    }
}

/// Now as `2026-10-14T09:30:05.123` (local time, to the millisecond, so the log shows how long
/// each step of a capture takes).
pub fn rfc3339_now() -> String {
    let z = jiff::Zoned::now();
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}", z.year(), z.month(), z.day(), z.hour(), z.minute(), z.second(), z.millisecond())
}

/// The local day and time of a moment given in milliseconds since 1970: `("2026-11-07",
/// "14:05")`; empty for a moment that can't be (junk from a file's times).
pub fn local_day_time(ms: u64) -> (String, String) {
    let Ok(ms) = i64::try_from(ms) else { return (String::new(), String::new()) };
    let Ok(ts) = jiff::Timestamp::from_millisecond(ms) else { return (String::new(), String::new()) };
    let z = ts.to_zoned(jiff::tz::TimeZone::system());
    (format!("{:04}-{:02}-{:02}", z.year(), z.month(), z.day()), format!("{:02}:{:02}", z.hour(), z.minute()))
}

/// Today, locally, as `2026-11-07`.
pub fn today() -> String {
    let z = jiff::Zoned::now();
    format!("{:04}-{:02}-{:02}", z.year(), z.month(), z.day())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_moment_has_a_local_day_and_time_and_junk_has_none() {
        let (day, time) = super::local_day_time(1_762_500_000_000);
        assert_eq!(day.len(), 10, "{day}");
        assert_eq!(time.len(), 5, "{time}");
        assert_eq!(super::local_day_time(u64::MAX), (String::new(), String::new()));
        assert_eq!(super::today().len(), 10);
    }

    #[test]
    fn now_is_plausible() {
        let s = super::now();
        assert!(s.year >= 2024 && (1..=12).contains(&s.month) && (1..=31).contains(&s.day) && s.hour < 24);
        let text = super::rfc3339_now();
        assert_eq!(text.len(), 23, "{text}");
        assert_eq!(text.as_bytes().get(19), Some(&b'.'));
    }
}
