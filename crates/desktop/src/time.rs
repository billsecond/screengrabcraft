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

/// Now as `2026-10-14T09:30:05` (local time, to the second).
pub fn rfc3339_now() -> String {
    let s = now();
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", s.year, s.month, s.day, s.hour, s.minute, s.second)
}

#[cfg(test)]
mod tests {
    #[test]
    fn now_is_plausible() {
        let s = super::now();
        assert!(s.year >= 2024 && (1..=12).contains(&s.month) && (1..=31).contains(&s.day) && s.hour < 24);
        assert_eq!(super::rfc3339_now().len(), 19);
    }
}
