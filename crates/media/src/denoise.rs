//! Background noise taken out of a sound track (a fan, a hum, a microphone's hiss), on this
//! computer: the sound in short windows half over each other, each split into its frequencies; in
//! each frequency, what is no louder than the track's steady noise is turned down, and what stands
//! above it (a voice) is kept (spectral gating). The noise is learnt from the track itself: how
//! loud each frequency is in its quieter moments. Nothing is sent anywhere and no trained model is
//! used.
//!
//! Any stretch is made from the sound around it alone (the windows sit at fixed places in the
//! recording), so playing from a moment and making the video's file come out the same.

use std::sync::{Arc, OnceLock};

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

/// Samples in a window (21 ms at 48 kHz); the windows are half over each other.
const WINDOW: usize = 1024;
const HOP: usize = WINDOW / 2;
/// The frequencies of a window that are kept (the rest mirror them).
pub const BINS: usize = WINDOW / 2 + 1;
/// The most windows looked at to learn a track's noise.
const LEARN_FROM: u64 = 600;
/// The most samples made in one go (longer asks are made a piece at a time).
const PIECE: usize = 48_000;

type Ffts = (Arc<dyn Fft<f32>>, Arc<dyn Fft<f32>>);

fn ffts() -> &'static Ffts {
    static FFTS: OnceLock<Ffts> = OnceLock::new();
    FFTS.get_or_init(|| {
        let mut planner = FftPlanner::new();
        (planner.plan_fft_forward(WINDOW), planner.plan_fft_inverse(WINDOW))
    })
}

/// The windows' shape, used on the way in and on the way out: the square root of a raised
/// cosine, so windows half over each other add up to the sound unchanged.
fn shape() -> &'static [f32] {
    static SHAPE: OnceLock<Vec<f32>> = OnceLock::new();
    SHAPE.get_or_init(|| (0..WINDOW).map(|n| (std::f32::consts::PI * n as f32 / WINDOW as f32).sin()).collect())
}

/// A track's steady noise, learnt from it: how loud each frequency is, on average, in the quietest
/// fifth of its moments (where nobody speaks). `read(start, n)` gives `n` samples (one channel)
/// of the track from `start`; `samples` is how long it is. Stretches of nothing at all (not
/// recorded) aren't counted.
pub fn learn(read: &dyn Fn(u64, usize) -> Vec<f32>, samples: u64) -> Vec<f32> {
    let (forward, _) = ffts();
    let w = shape();
    let count = (samples / WINDOW as u64).clamp(1, LEARN_FROM);
    let step = (samples / count).max(1);
    // Each moment looked at: how loud it is in all, and in each frequency.
    let mut moments: Vec<(f32, Vec<f32>)> = Vec::with_capacity(count as usize);
    let mut buf = vec![Complex32::default(); WINDOW];
    for i in 0..count {
        let x = read(i.saturating_mul(step), WINDOW);
        let energy = x.iter().map(|v| v * v).sum::<f32>();
        if !(energy >= 1e-9 && energy.is_finite()) {
            continue;
        }
        for ((b, s), wv) in buf.iter_mut().zip(x.iter().chain(std::iter::repeat(&0.0))).zip(w) {
            *b = Complex32::new(s * wv, 0.0);
        }
        forward.process(&mut buf);
        moments.push((energy, buf.iter().take(BINS).map(Complex32::norm_sqr).collect()));
    }
    moments.sort_by(|a, b| a.0.total_cmp(&b.0));
    let quiet = moments.get(..moments.len().div_ceil(5)).unwrap_or_default();
    (0..BINS).map(|b| quiet.iter().map(|(_, p)| p.get(b).copied().unwrap_or(0.0)).sum::<f32>() / quiet.len().max(1) as f32).collect()
}

/// `count` stereo samples of a track from its sample `first`, its noise (`noise`, from
/// [`learn`]) turned down by `amount` (0 not at all, 1 the most). `read(start, n)` gives `n`
/// stereo samples of the track from `start` (before its start, silence); `stereo` false: the
/// two sides are the same, and only one is worked on.
pub fn cleaned(read: &dyn Fn(i64, usize) -> Vec<[f32; 2]>, noise: &[f32], amount: f32, stereo: bool, first: i64, count: usize) -> Vec<[f32; 2]> {
    let amount = if amount.is_finite() { amount.clamp(0.0, 1.0) } else { 0.0 };
    // (Kept far from the ends of the numbers, so nothing added to it overflows.)
    let first = first.clamp(-(1 << 60), 1 << 60);
    if amount <= 0.0 || noise.len() != BINS {
        return read(first, count);
    }
    let mut out = Vec::with_capacity(count);
    let mut done = 0usize;
    while done < count {
        let n = PIECE.min(count - done);
        out.extend(piece(read, noise, amount, stereo, first + done as i64, n));
        done += n;
    }
    out
}

fn piece(read: &dyn Fn(i64, usize) -> Vec<[f32; 2]>, noise: &[f32], amount: f32, stereo: bool, first: i64, count: usize) -> Vec<[f32; 2]> {
    let (forward, inverse) = ffts();
    let w = shape();
    let hop = HOP as i64;
    // Window k covers samples k × HOP to k × HOP + WINDOW: the samples asked for are in windows
    // ka..=kb, and how loud a frequency is is judged with the windows either side too.
    let ka = first.div_euclid(hop) - 1;
    let kb = (first + count as i64 - 1).div_euclid(hop);
    let (k_lo, k_hi) = (ka - 1, kb + 1);
    let windows = (k_hi - k_lo + 1) as usize;
    let sound = read(k_lo * hop, (windows - 1) * HOP + WINDOW);
    let sides = if stereo { 2 } else { 1 };
    // Each window's frequencies (each side), and how loud each is (the sides together).
    let mut spectra: Vec<Vec<Vec<Complex32>>> = Vec::with_capacity(windows);
    let mut power: Vec<Vec<f32>> = Vec::with_capacity(windows);
    for i in 0..windows {
        let start = i * HOP;
        let mut sides_of: Vec<Vec<Complex32>> = (0..sides)
            .map(|c| {
                (0..WINDOW)
                    .map(|n| {
                        let s = sound.get(start + n).map_or(0.0, |s| s[c]);
                        Complex32::new(s * w.get(n).copied().unwrap_or(0.0), 0.0)
                    })
                    .collect()
            })
            .collect();
        for x in &mut sides_of {
            forward.process(x);
        }
        power.push((0..BINS).map(|b| sides_of.iter().map(|x| x.get(b).map_or(0.0, |c| c.norm_sqr())).sum::<f32>() / sides as f32).collect());
        spectra.push(sides_of);
    }
    // How far the noise is turned down at most (24 dB at the most), and how far above it a sound
    // must be to be kept.
    let floor = 10f32.powf(-amount * 24.0 / 20.0);
    let over = 1.0 + 1.5 * amount;
    let mut out = vec![[0.0f32; 2]; count];
    for i in 1..windows - 1 {
        let k = k_lo + i as i64;
        let gain: Vec<f32> = (0..BINS)
            .map(|b| {
                // How loud it is here: this window and those either side, this frequency and
                // those either side (so a voice's edges aren't chopped).
                let (mut p, mut n) = (0.0f32, 0.0f32);
                for row in power.get(i - 1..=i + 1).unwrap_or_default() {
                    for v in row.get(b.saturating_sub(1)..=(b + 1).min(BINS - 1)).unwrap_or_default() {
                        p += v;
                        n += 1.0;
                    }
                }
                let p = p / n.max(1.0);
                let nz = noise.get(b).copied().unwrap_or(0.0) * over;
                let g = if p > 0.0 { (1.0 - nz / p).max(0.0).sqrt() } else { 0.0 };
                g.max(floor)
            })
            .collect();
        let Some(sides_of) = spectra.get_mut(i) else { continue };
        for (c, x) in sides_of.iter_mut().enumerate() {
            for (b, v) in x.iter_mut().enumerate() {
                // The upper half mirrors the lower.
                let bin = if b < BINS { b } else { WINDOW - b };
                *v *= gain.get(bin).copied().unwrap_or(1.0);
            }
            inverse.process(x);
            let start = k * hop;
            for (n, v) in x.iter().enumerate() {
                let Ok(o) = usize::try_from(start + n as i64 - first) else { continue };
                if let Some(slot) = out.get_mut(o) {
                    slot[c] += v.re / WINDOW as f32 * w.get(n).copied().unwrap_or(0.0);
                }
            }
        }
    }
    if !stereo {
        for s in &mut out {
            s[1] = s[0];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone from 0.5 s to 1.5 s over a steady hiss, 3 s long, as recorded.
    fn noisy(s: i64) -> f32 {
        if s < 0 {
            return 0.0;
        }
        // The same hiss every time it's read (a little random-number maker of its own).
        let mut h = (s as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h ^= h >> 29;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 32;
        let hiss = (h % 2001) as f32 / 1000.0 - 1.0;
        let tone = if (24_000..72_000).contains(&s) { (std::f32::consts::TAU * 440.0 * s as f32 / 48_000.0).sin() * 0.3 } else { 0.0 };
        tone + hiss * 0.03
    }

    fn read(start: i64, n: usize) -> Vec<[f32; 2]> {
        (0..n as i64).map(|i| [noisy(start + i); 2]).collect()
    }

    fn loudness(s: &[[f32; 2]]) -> f32 {
        (s.iter().map(|v| v[0] * v[0]).sum::<f32>() / s.len().max(1) as f32).sqrt()
    }

    #[test]
    fn the_hiss_goes_and_the_tone_stays() {
        let noise = learn(&|start, n| (0..n as u64).map(|i| noisy((start + i) as i64)).collect(), 144_000);
        assert_eq!(noise.len(), BINS);
        let hiss = loudness(&read(100_000, 24_000));
        let cleaned_hiss = loudness(&cleaned(&read, &noise, 1.0, false, 100_000, 24_000));
        assert!(cleaned_hiss < hiss / 4.0, "the hiss {hiss} is {cleaned_hiss} after");
        let tone = loudness(&read(36_000, 24_000));
        let cleaned_tone = loudness(&cleaned(&read, &noise, 1.0, false, 36_000, 24_000));
        assert!((cleaned_tone / tone - 1.0).abs() < 0.1, "the tone {tone} is {cleaned_tone} after");
        // Nothing taken out: as it was.
        assert_eq!(cleaned(&read, &noise, 0.0, false, 5, 100), read(5, 100));
    }

    #[test]
    fn it_comes_out_the_same_made_whole_or_in_pieces_and_junk_never_panics() {
        let noise = learn(&|start, n| (0..n as u64).map(|i| noisy((start + i) as i64)).collect(), 144_000);
        let whole = cleaned(&read, &noise, 0.7, true, 10_000, 20_000);
        let mut parts = cleaned(&read, &noise, 0.7, true, 10_000, 7_777);
        parts.extend(cleaned(&read, &noise, 0.7, true, 17_777, 12_223));
        assert_eq!(whole.len(), parts.len());
        assert!(whole.iter().zip(&parts).all(|(a, b)| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6));
        // Before the start, a wrong noise, junk amounts, nothing at all.
        let _ = cleaned(&read, &noise, 1.0, true, -5_000, 3);
        let _ = cleaned(&read, &[1.0; 3], 1.0, true, 0, 3);
        for amount in [f32::NAN, -1.0, 9.0] {
            let _ = cleaned(&read, &noise, amount, false, i64::MAX, 3);
        }
        assert!(cleaned(&read, &noise, 1.0, true, 0, 0).is_empty());
        assert_eq!(learn(&|_, n| vec![0.0; n], 0).len(), BINS);
    }
}
