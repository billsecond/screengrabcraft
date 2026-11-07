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

#[cfg(test)]
mod tests {
    #[test]
    fn now_is_plausible() {
        let s = super::now();
        assert!(s.year >= 2024 && (1..=12).contains(&s.month) && (1..=31).contains(&s.day) && s.hour < 24);
        let text = super::rfc3339_now();
        assert_eq!(text.len(), 23, "{text}");
        assert_eq!(text.as_bytes().get(19), Some(&b'.'));
    }
}
