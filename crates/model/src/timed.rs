//! Captures on a timer (one every so many seconds, a time-lapse) and at a set time of day: what's
//! captured, how often, and whether a capture is skipped when nothing has changed since the last
//! one. The capture service runs them; here is what they are and the arithmetic.

use serde::{Deserialize, Serialize};

use crate::{Raster, Stamp};

/// The shortest and longest wait between captures on a timer, in seconds.
pub const MIN_EVERY: u32 = 2;
pub const MAX_EVERY: u32 = 24 * 60 * 60;

/// What a capture on a timer is of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimedArea {
    /// The screen the pointer is on.
    #[default]
    Screen,
    AllScreens,
    /// The area last captured.
    LastArea,
}

impl TimedArea {
    pub const ALL: [TimedArea; 3] = [TimedArea::Screen, TimedArea::AllScreens, TimedArea::LastArea];

    pub fn label(self) -> &'static str {
        match self {
            TimedArea::Screen => "The screen with the pointer",
            TimedArea::AllScreens => "All screens",
            TimedArea::LastArea => "The last area captured",
        }
    }
}

/// Captures on a timer and at a set time, as the owner set them up.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TimedCaptures {
    /// One capture every so many seconds.
    pub every_secs: u32,
    pub area: TimedArea,
    /// No capture is kept when the screen looks as it did at the last one.
    pub skip_unchanged: bool,
    /// The time of day of a capture at a set time, as `14:30` (empty when none is set).
    pub at: String,
}

impl Default for TimedCaptures {
    fn default() -> TimedCaptures {
        TimedCaptures { every_secs: 60, area: TimedArea::Screen, skip_unchanged: true, at: String::new() }
    }
}

impl TimedCaptures {
    /// Every number in its range.
    pub fn clamped(mut self) -> TimedCaptures {
        self.every_secs = self.every_secs.clamp(MIN_EVERY, MAX_EVERY);
        if self.at.chars().count() > 8 {
            self.at.clear();
        }
        self
    }
}

/// A time of day written `14:30` (or `9:05`, `14.30`, `1430`): the hour and minute.
pub fn parse_clock(text: &str) -> Option<(u8, u8)> {
    let t = text.trim();
    let (h, m) = match t.split_once([':', '.', 'h']) {
        Some((h, m)) => (h, m),
        None if t.len() == 4 && t.chars().all(|c| c.is_ascii_digit()) => t.split_at(2),
        None => return None,
    };
    let (h, m): (u8, u8) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

/// Seconds from `now` until the next `hour:minute` (today if it's still to come, else tomorrow;
/// a whole day when it's this very minute).
pub fn secs_until(now: Stamp, (hour, minute): (u8, u8)) -> u32 {
    let at = u32::from(hour.min(23)) * 3600 + u32::from(minute.min(59)) * 60;
    let now = u32::from(now.hour.min(23)) * 3600 + u32::from(now.minute.min(59)) * 60 + u32::from(now.second.min(59));
    if at > now { at - now } else { at + 24 * 3600 - now }
}

/// A picture made small: the average colour of each of a grid of blocks, to tell whether the
/// screen has changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    width: u32,
    height: u32,
    blocks: Vec<[u8; 3]>,
}

/// The grid a fingerprint is made on.
const GRID: (u32, u32) = (32, 18);

/// A picture's fingerprint (its size, and the average colour of each block of a 32 × 18 grid).
/// Every pixel counts (every other one on a very large picture), so a few changed pixels move a
/// block's average only a little.
pub fn fingerprint(r: &Raster) -> Fingerprint {
    let (w, h) = (r.width().max(1), r.height().max(1));
    let (gw, gh) = (GRID.0.min(w), GRID.1.min(h));
    let cells = (gw * gh) as usize;
    let mut sums = vec![[0u64; 4]; cells];
    let step: usize = if u64::from(w) * u64::from(h) > 8_000_000 { 2 } else { 1 };
    for (y, row) in r.pixels().chunks_exact((w as usize).saturating_mul(4)).enumerate().step_by(step) {
        let by = (y as u64 * u64::from(gh) / u64::from(h)) as usize;
        for (x, p) in row.as_chunks::<4>().0.iter().enumerate().step_by(step) {
            let bx = (x as u64 * u64::from(gw) / u64::from(w)) as usize;
            if let (Some(s), Some(px)) = (sums.get_mut(by * gw as usize + bx), p.get(..3)) {
                for (acc, v) in s.iter_mut().zip(px) {
                    *acc += u64::from(*v);
                }
                s[3] += 1;
            }
        }
    }
    let blocks = sums.iter().map(|s| {
        let n = s[3].max(1);
        [(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8]
    });
    Fingerprint { width: r.width(), height: r.height(), blocks: blocks.collect() }
}

/// Whether two fingerprints are of the same screen, give or take a little (a clock ticking over,
/// the pointer's blink): the same size, and no block's colour more than a little different.
pub fn same_scene(a: &Fingerprint, b: &Fingerprint) -> bool {
    const LITTLE: u8 = 6;
    a.width == b.width
        && a.height == b.height
        && a.blocks.len() == b.blocks.len()
        && a.blocks.iter().zip(&b.blocks).all(|(p, q)| p.iter().zip(q).all(|(x, y)| x.abs_diff(*y) <= LITTLE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Color;

    #[test]
    fn a_time_of_day_is_read_however_it_is_written() {
        assert_eq!(parse_clock("14:30"), Some((14, 30)));
        assert_eq!(parse_clock(" 9:05 "), Some((9, 5)));
        assert_eq!(parse_clock("14.30"), Some((14, 30)));
        assert_eq!(parse_clock("0730"), Some((7, 30)));
        for junk in ["", "24:00", "12:60", "noon", "1:2:3", "-1:30", "99999", "１２:30"] {
            assert_eq!(parse_clock(junk), None, "{junk}");
        }
    }

    #[test]
    fn the_wait_for_a_set_time_is_today_or_else_tomorrow() {
        let now = Stamp { hour: 14, minute: 0, second: 30, ..Stamp::default() };
        assert_eq!(secs_until(now, (14, 30)), 29 * 60 + 30);
        assert_eq!(secs_until(now, (13, 0)), 23 * 3600 - 30);
        assert_eq!(secs_until(now, (14, 0)), 24 * 3600 - 30, "this minute: tomorrow's");
        assert_eq!(secs_until(Stamp { hour: 99, ..Stamp::default() }, (0, 0)), 3600, "nonsense is clamped");
    }

    #[test]
    fn a_screen_that_hardly_changed_is_the_same_scene() {
        let mut a = Raster::filled(1920, 1080, Color::rgb(30, 60, 90)).unwrap();
        let b = a.clone();
        assert!(same_scene(&fingerprint(&a), &fingerprint(&b)));
        // A clock's digit changing in a corner: a stroke of a few pixels.
        for y in 1050..1070 {
            for x in 1880..1882 {
                if let Some(p) = a.pixels_mut().get_mut(((y * 1920 + x) * 4) as usize..((y * 1920 + x) * 4 + 4) as usize) {
                    p.copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
        assert!(same_scene(&fingerprint(&a), &fingerprint(&b)), "a clock ticking over is no change");
        // A window opened over a quarter of it.
        for y in 0..540 {
            if let Some(row) = a.pixels_mut().get_mut((y * 1920 * 4) as usize..((y * 1920 + 960) * 4) as usize) {
                row.fill(255);
            }
        }
        assert!(!same_scene(&fingerprint(&a), &fingerprint(&b)));
        let small = Raster::filled(800, 600, Color::rgb(30, 60, 90)).unwrap();
        assert!(!same_scene(&fingerprint(&small), &fingerprint(&b)), "another size is another scene");
        let tiny = Raster::filled(1, 1, Color::WHITE).unwrap();
        assert!(same_scene(&fingerprint(&tiny), &fingerprint(&tiny)));
    }

    #[test]
    fn the_settings_are_kept_in_range() {
        let t = TimedCaptures { every_secs: 0, at: "a long time from now".into(), ..TimedCaptures::default() }.clamped();
        assert_eq!(t.every_secs, MIN_EVERY);
        assert!(t.at.is_empty());
        assert_eq!(TimedCaptures { every_secs: u32::MAX, ..TimedCaptures::default() }.clamped().every_secs, MAX_EVERY);
    }
}
