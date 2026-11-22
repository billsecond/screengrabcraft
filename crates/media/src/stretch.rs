//! Sound played faster or slower without going higher or lower: short windows of what was
//! recorded, half over each other, each one taken from where the faster (or slower) play has got
//! to and moved a little, to line up with the one before it (overlap-add with a similarity
//! search, WSOLA). Voices sped up sound like fast talk, not like cartoon voices.
//!
//! Any stretch of the sound is made from what's near it alone (the windows are lined up afresh
//! every [`CHAIN`] windows), so playing from a moment and making the video's file come out the
//! same, piece by piece.

/// Samples in a window (32 ms at 48 kHz); the windows are half over each other.
const WINDOW: usize = 1536;
const HOP: usize = WINDOW / 2;
/// How far either way (samples) a window may be moved to line up with the one before (8 ms).
const SEEK: i64 = 384;
/// Windows are lined up afresh every so many.
const CHAIN: u64 = 32;
/// The most samples made in one go (longer asks are made a piece at a time).
const PIECE: usize = 48_000;

/// `count` stereo samples of a clip's sound played at `rate` (1 as recorded, 2 twice as fast),
/// from sample `first` of it as played. `read(start, n)` gives `n` stereo samples of the clip's
/// sound as recorded from sample `start` of it (before its start, silence).
pub fn stretched(read: &dyn Fn(i64, usize) -> Vec<[f32; 2]>, rate: f64, first: u64, count: usize) -> Vec<[f32; 2]> {
    let rate = if rate.is_finite() { rate.clamp(0.05, 20.0) } else { 1.0 };
    let mut out = Vec::with_capacity(count);
    let mut done = 0usize;
    while done < count {
        let n = PIECE.min(count - done);
        out.extend(piece(read, rate, first.saturating_add(done as u64), n));
        done += n;
    }
    out
}

/// Where window `k` would start in what was recorded (its middle, in samples), without moving it.
fn nominal(k: u64, rate: f64) -> i64 {
    // (Kept far from the ends of the numbers, so nothing added to it overflows.)
    ((k as f64 * HOP as f64 * rate).round() as i64).clamp(0, 1 << 60)
}

fn piece(read: &dyn Fn(i64, usize) -> Vec<[f32; 2]>, rate: f64, first: u64, count: usize) -> Vec<[f32; 2]> {
    let hop = HOP as u64;
    // Window k is centred on sample k × HOP of the sound as played; the samples asked for are
    // made of windows ka..=kb, lined up from the last fresh start before them.
    let ka = first / hop;
    let kb = first.saturating_add(count as u64).saturating_sub(1) / hop + 1;
    let k0 = ka - ka % CHAIN;
    // What was recorded that they may be taken from (the last one may look at the one after it),
    // read once.
    let lo = nominal(k0, rate) - SEEK - HOP as i64;
    let hi = nominal(kb + 1, rate) + SEEK + HOP as i64 + 1;
    let Some(len) = usize::try_from(hi - lo).ok().filter(|n| *n <= PIECE * 64) else { return vec![[0.0; 2]; count] };
    let sound = read(lo, len);
    let mono: Vec<f32> = sound.iter().map(|[l, r]| l + r).collect();
    let at = |i: i64| -> [f32; 2] { usize::try_from(i - lo).ok().and_then(|j| sound.get(j)).copied().unwrap_or([0.0; 2]) };
    // Where each window is taken from (its middle).
    let mut centre: Vec<i64> = Vec::with_capacity((kb - k0 + 1) as usize);
    for k in k0..=kb {
        let q = nominal(k, rate);
        let p = if k % CHAIN == 0 {
            q
        } else {
            let prev = centre.last().copied().unwrap_or(q);
            // The last window before a fresh start lines up with it as well as with the one before.
            let next = if (k + 1) % CHAIN == 0 { Some(nominal(k + 1, rate)) } else { None };
            q + best_shift(&mono, lo, q, prev, next)
        };
        centre.push(p);
    }
    // Each sample: the two windows over it, faded across (their weights add up to 1).
    let fade: Vec<f32> = (0..HOP).map(|j| (std::f32::consts::PI * j as f32 / WINDOW as f32).cos().powi(2)).collect();
    (first..first.saturating_add(count as u64))
        .map(|y| {
            let k = y / hop;
            let j = (y % hop) as usize;
            let i = (k - k0) as usize;
            let (Some(&p1), Some(&p2)) = (centre.get(i), centre.get(i + 1)) else { return [0.0; 2] };
            let w = fade.get(j).copied().unwrap_or(0.0);
            let (a, b) = (at(p1 + j as i64), at(p2 + j as i64 - HOP as i64));
            [a[0] * w + b[0] * (1.0 - w), a[1] * w + b[1] * (1.0 - w)]
        })
        .collect()
}

/// How far to move a window meant to be centred on `q` so that its first half sounds like the
/// second half of the window before (centred on `prev`), and, given `next`, its second half like
/// the first half of the window after. Within [`SEEK`] either way.
fn best_shift(mono: &[f32], lo: i64, q: i64, prev: i64, next: Option<i64>) -> i64 {
    let get = |i: i64| -> f32 { usize::try_from(i - lo).ok().and_then(|j| mono.get(j)).copied().unwrap_or(0.0) };
    let half = HOP as i64;
    let score = |d: i64, step: usize| -> f32 {
        let (mut dot, mut energy) = (0.0f32, 1e-6f32);
        for j in (0..half).step_by(step) {
            let c = get(q + d - half + j);
            dot += c * get(prev + j);
            energy += c * c;
        }
        let mut s = dot / energy.sqrt();
        if let Some(n) = next {
            let (mut dot, mut energy) = (0.0f32, 1e-6f32);
            for j in (0..half).step_by(step) {
                let c = get(q + d + j);
                dot += c * get(n - half + j);
                energy += c * c;
            }
            s += dot / energy.sqrt();
        }
        s
    };
    // Roughly first (every fourth shift, every fourth sample), then exactly near the best.
    let mut best = (0i64, score(0, 4));
    for d in (-SEEK..=SEEK).step_by(4) {
        let s = score(d, 4);
        if s > best.1 + 1e-6 || (s >= best.1 - 1e-6 && d.abs() < best.0.abs()) {
            best = (d, s);
        }
    }
    let rough = best.0;
    let mut best = (rough, score(rough, 2));
    for d in (rough - 3).max(-SEEK)..=(rough + 3).min(SEEK) {
        let s = score(d, 2);
        if s > best.1 + 1e-6 {
            best = (d, s);
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone of `hz`, as recorded: silence before its start.
    fn tone(hz: f32) -> impl Fn(i64, usize) -> Vec<[f32; 2]> {
        move |start, n| {
            (0..n as i64)
                .map(|i| {
                    let s = start + i;
                    let v = if s < 0 { 0.0 } else { (std::f32::consts::TAU * hz * s as f32 / 48_000.0).sin() * 0.5 };
                    [v, v]
                })
                .collect()
        }
    }

    /// How often a sound crosses zero going up, a second.
    fn pitch(s: &[[f32; 2]]) -> f32 {
        let ups = s.windows(2).filter(|w| w[0][0] < 0.0 && w[1][0] >= 0.0).count();
        ups as f32 * 48_000.0 / s.len() as f32
    }

    #[test]
    fn faster_and_slower_keep_the_pitch() {
        let read = tone(220.0);
        for rate in [0.5, 1.5, 2.0, 4.0] {
            let out = stretched(&read, rate, 4800, 48_000);
            assert_eq!(out.len(), 48_000);
            let hz = pitch(&out);
            assert!((hz - 220.0).abs() < 12.0, "{rate}× sounds at {hz} Hz");
            assert!(out.iter().all(|[l, r]| l.is_finite() && l.abs() <= 1.0 && l == r));
        }
    }

    #[test]
    fn it_comes_out_the_same_made_whole_or_in_pieces() {
        let read = tone(330.0);
        let whole = stretched(&read, 1.75, 1000, 30_000);
        let mut parts = stretched(&read, 1.75, 1000, 4800);
        parts.extend(stretched(&read, 1.75, 5800, 12_345));
        parts.extend(stretched(&read, 1.75, 18_145, 30_000 - 4800 - 12_345));
        assert_eq!(whole, parts);
    }

    #[test]
    fn it_follows_the_faster_play_through_the_recording() {
        // A tone starting 1 s into the recording is heard from half a second played twice as fast.
        let read = |start: i64, n: usize| -> Vec<[f32; 2]> {
            (0..n as i64)
                .map(|i| {
                    let s = start + i;
                    let v = if s >= 48_000 { (std::f32::consts::TAU * 300.0 * s as f32 / 48_000.0).sin() * 0.9 } else { 0.0 };
                    [v, v]
                })
                .collect()
        };
        let out = stretched(&read, 2.0, 0, 48_000);
        let loud = out.iter().position(|s| s[0].abs() > 0.3).unwrap();
        assert!((22_500..26_000).contains(&loud), "{loud}");
        // Junk asks never panic.
        for rate in [f64::NAN, 0.0, -3.0, 1e9] {
            let _ = stretched(&read, rate, u64::MAX / 4, 3);
        }
        assert!(stretched(&read, 2.0, 0, 0).is_empty());
    }
}
