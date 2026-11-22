//! Lining two sounds up: how far one must move to sound in step with the other (the same voice
//! recorded twice, by the computer's microphone and by another recorder, say). Both are listened
//! to at a low rate: first how loud they are every hundredth of a second, compared at every shift
//! within reach; then the waves themselves near the best of those, to the sample.

/// Samples a second the sounds are listened to at.
pub const RATE: u64 = 8_000;
/// The longest stretch of a sound listened to (µs): a minute.
pub const MOST_LISTENED: u64 = 60_000_000;
/// The farthest a sound is moved to line up (µs).
pub const MOST_WITHIN: u64 = 30_000_000;
/// How alike the two must sound, at the best shift, to be lined up (0 to 1).
pub const SURE: f32 = 0.35;
/// Samples in a step of loudness (10 ms).
const STEP: usize = 80;
/// Steps a sound's loudness is compared with its own average over (half a second).
const AVERAGE: usize = 50;
/// The most samples of the waves compared at each shift near the best (4 s).
const FINE: usize = 32_000;

/// What's listened to: a track of the recording, or an item of a layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Track(usize),
    Item { layer: u32, item: u32 },
}

/// µs as samples at [`RATE`].
pub fn to_samples(us: u64) -> u64 {
    us.min(u64::MAX / RATE) * RATE / 1_000_000
}

/// Samples at [`RATE`] as µs (either way).
pub fn to_us(samples: i64) -> i64 {
    samples.clamp(-(i64::MAX / 1_000_000), i64::MAX / 1_000_000) * 1_000_000 / RATE as i64
}

/// How loud a sound is, step by step, compared with its own average round it: where something
/// starts or gets louder stands out, a steady hum doesn't.
fn envelope(s: &[f32]) -> Vec<f32> {
    let level: Vec<f32> = s.chunks(STEP).map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len().max(1) as f32 + 1e-8).ln()).collect();
    let n = level.len();
    // A running sum, for the average round each step.
    let mut sum = Vec::with_capacity(n + 1);
    sum.push(0.0f64);
    for v in &level {
        let last = sum.last().copied().unwrap_or(0.0);
        sum.push(last + f64::from(*v));
    }
    (0..n)
        .map(|i| {
            let (a, b) = (i.saturating_sub(AVERAGE / 2), (i + AVERAGE / 2 + 1).min(n));
            let mean = (sum.get(b).copied().unwrap_or(0.0) - sum.get(a).copied().unwrap_or(0.0)) / (b - a).max(1) as f64;
            level.get(i).copied().unwrap_or(0.0) - mean as f32
        })
        .collect()
}

/// How alike `moving` sounds to `fixed` with its sample `k` over `fixed`'s `k + shift` (-1 to 1),
/// over at most `most` of the samples they share; none if they share fewer than `least`.
fn alike(fixed: &[f32], moving: &[f32], shift: i64, least: usize, most: usize) -> Option<f32> {
    // The samples of `moving` over `fixed`.
    let k0 = usize::try_from(shift.saturating_neg().max(0)).ok()?;
    let k1 = usize::try_from((fixed.len() as i64).saturating_sub(shift).max(0)).ok()?.min(moving.len());
    if k1 <= k0 || k1 - k0 < least {
        return None;
    }
    let k1 = k1.min(k0.saturating_add(most));
    let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
    for k in k0..k1 {
        let b = f64::from(moving.get(k).copied().unwrap_or(0.0));
        let a = f64::from(usize::try_from((k as i64).saturating_add(shift)).ok().and_then(|j| fixed.get(j)).copied().unwrap_or(0.0));
        ab += a * b;
        aa += a * a;
        bb += b * b;
    }
    if aa < 1e-9 || bb < 1e-9 {
        return None;
    }
    Some((ab / (aa * bb).sqrt()) as f32)
}

/// How far (samples, later if above 0) `moving` must move to sound in step with `fixed`, and how
/// alike they then sound (0 to 1). Both are mono at [`RATE`]; `moving`'s first sample is now heard
/// `lead` samples after `fixed`'s first. Shifts within `reach` samples either way are tried. None
/// when either is silent, or they share too little within reach.
pub fn offset(fixed: &[f32], moving: &[f32], lead: i64, reach: i64) -> Option<(i64, f32)> {
    let reach = reach.clamp(0, to_samples(MOST_WITHIN) as i64);
    // (Kept far from the ends of the numbers, so nothing added to it overflows.)
    let lead = lead.clamp(-(1 << 40), 1 << 40);
    let (ef, em) = (envelope(fixed), envelope(moving));
    // Half a second shared at least (or all of the shorter, if it's shorter).
    let least = em.len().min(ef.len()).clamp(1, AVERAGE);
    let (step, lead_steps) = (STEP as i64, lead.div_euclid(STEP as i64));
    let mut best: Option<(i64, f32)> = None;
    for d in -(reach / step)..=(reach / step) {
        let Some(score) = alike(&ef, &em, lead_steps + d, least, usize::MAX) else { continue };
        if best.is_none_or(|(_, b)| score > b) {
            best = Some((d, score));
        }
    }
    let (coarse, sure) = best?;
    // Then the waves, near it, to the sample.
    let near = lead_steps * step + coarse * step - lead;
    let mut fine: Option<(i64, f32)> = None;
    for d in (near - 2 * step).max(-reach)..=(near + 2 * step).min(reach) {
        let Some(score) = alike(fixed, moving, lead + d, STEP, FINE) else { continue };
        if fine.is_none_or(|(_, b)| score > b) {
            fine = Some((d, score));
        }
    }
    // The waves may not match (another microphone, another room): the loudness's shift then.
    let shift = match fine {
        Some((d, s)) if s > 0.2 => d,
        _ => near,
    };
    Some((shift, sure.clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up voice: bursts of a few tones at uneven times, with quiet between.
    fn voice(n: usize) -> Vec<f32> {
        let mut seed = 12_345u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (seed >> 8) as f32 / (1u32 << 24) as f32
        };
        let mut out = vec![0.0f32; n];
        let mut at = 0usize;
        while at < n {
            let len = 800 + (next() * 3_000.0) as usize;
            let hz = 150.0 + next() * 300.0;
            for k in 0..len.min(n - at) {
                let t = k as f32 / RATE as f32;
                if let Some(o) = out.get_mut(at + k) {
                    *o = (std::f32::consts::TAU * hz * t).sin() * 0.5 + (next() - 0.5) * 0.05;
                }
            }
            at += len + 400 + (next() * 4_000.0) as usize;
        }
        out
    }

    #[test]
    fn a_sound_heard_late_is_found_and_moved_back_to_the_sample() {
        let v = voice(8_000 * 20);
        // The fixed sound from 0; the moving one is the same voice from 1.25 s on, now heard at
        // 4 s of the fixed one's time: it must move back by 2.75 s.
        let late = 10_000usize;
        let moving: Vec<f32> = v.get(late..late + 8_000 * 8).unwrap().to_vec();
        let (d, sure) = offset(&v, &moving, 32_000, 40_000).unwrap();
        assert_eq!(d, late as i64 - 32_000);
        assert!(sure > 0.8, "{sure}");
        // Quieter and with a hiss of its own (another recorder): still found, near enough.
        let mut other = moving.iter().map(|x| x * 0.3).collect::<Vec<_>>();
        for (k, x) in other.iter_mut().enumerate() {
            *x += ((k * 7_919) % 97) as f32 / 97.0 * 0.01 - 0.005;
        }
        let (d2, sure2) = offset(&v, &other, 32_000, 40_000).unwrap();
        assert!((d2 - d).abs() <= 2, "{d2} {d}");
        assert!(sure2 > SURE);
    }

    #[test]
    fn silence_and_sounds_that_share_nothing_are_not_lined_up() {
        let v = voice(8_000 * 10);
        assert!(offset(&v, &vec![0.0; 8_000 * 3], 0, 16_000).is_none(), "silence");
        assert!(offset(&[], &v, 0, 16_000).is_none());
        // Another voice altogether: found nowhere sure.
        let mut seedy = voice(8_000 * 30);
        let other: Vec<f32> = seedy.drain(8_000 * 20..).collect();
        if let Some((_, sure)) = offset(&v, &other, 0, 16_000) {
            assert!(sure < 0.9, "{sure}");
        }
        // Junk never panics.
        let _ = offset(&v, &v, i64::MIN, i64::MAX);
        let _ = offset(&v, &v, i64::MAX, -5);
        assert_eq!((to_samples(1_000_000), to_us(8_000), to_us(-4_000)), (8_000, 1_000_000, -500_000));
    }
}
