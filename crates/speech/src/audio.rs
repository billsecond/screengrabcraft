//! A recording's sound made ready for Whisper: one track at a time, as one channel at 16 kHz,
//! with only the stretches where something is said (silence costs nothing to hear).
//!
//! - [`levels`]: how loud a track is every 20 ms.
//! - [`speech`]: the stretches loud enough to be speech, from those levels.
//! - [`Compact`]: those stretches end to end, short pauses between, read from the recording
//!   when Whisper needs them, and the way back from a moment in them to the recording's time.

use screengrabcraft_media::compose::to_samples;
use screengrabcraft_media::recording::Recording;

/// Samples of the recording (48 kHz) in a level step: 20 ms.
pub const FRAME: usize = 960;
/// A level step in µs.
pub const FRAME_US: u64 = 20_000;
/// Samples at 16 kHz in a level step.
const FRAME_16K: usize = 320;
/// The longest stretch of a track heard (a damaged file can claim anything).
const MAX_TRACK_US: u64 = 12 * 3600 * 1_000_000;
/// The pause put between stretches: long enough that the model ends a line there.
const GAP: usize = 16_000 * 8 / 10;
/// How far into the next stretch a line's end may run and still be taken as its own stretch's.
const OVERRUN: usize = 16_000 * 6 / 10;
/// Below this a track is silent, whatever its noise.
const QUIET_DB: f32 = -55.0;

/// The level of silence: no sound at all.
pub const SILENT_DB: f32 = -100.0;

fn db(sum_sq: f64, n: usize) -> f32 {
    if n == 0 {
        return SILENT_DB;
    }
    (10.0 * (sum_sq / n as f64 + 1e-10).log10()).max(f64::from(SILENT_DB)) as f32
}

/// Seconds as 16 kHz samples (nothing for what isn't a time).
fn samples(secs: f32) -> usize {
    if secs.is_finite() { (f64::from(secs.max(0.0)) * 16_000.0).round() as usize } else { 0 }
}

fn channels(rec: &Recording, stream: u8) -> usize {
    rec.stream(stream).map(|s| usize::from(s.channels.clamp(1, 2))).unwrap_or(1)
}

/// One channel, -1 to 1: the channels' average.
fn mono(samples: &[i16], channels: usize) -> impl Iterator<Item = f32> + '_ {
    samples.chunks(channels.max(1)).map(|c| c.iter().map(|s| f32::from(*s)).sum::<f32>() / (32768.0 * c.len().max(1) as f32))
}

/// How loud the track is every 20 ms, in dB below full scale ([`SILENT_DB`] for nothing).
pub fn levels(rec: &Recording, stream: u8) -> Vec<f32> {
    let total = to_samples(rec.stream_duration(stream).min(MAX_TRACK_US));
    let ch = channels(rec, stream);
    let mut out = Vec::with_capacity((total / FRAME as u64) as usize + 1);
    let block = FRAME as u64 * 500;
    let mut at = 0u64;
    while at < total {
        let n = (total - at).min(block) as usize;
        let sound: Vec<f32> = mono(&rec.sound(stream, at, n), ch).collect();
        for f in sound.chunks(FRAME) {
            out.push(db(f.iter().map(|v| f64::from(*v) * f64::from(*v)).sum(), f.len()));
        }
        at += n as u64;
    }
    out
}

/// The stretches (in level steps, end not included) loud enough to be speech: louder than the
/// track's own quiet by 12 dB, a pause of less than half a second bridged, clicks left out, a
/// little kept either side.
pub fn speech(levels: &[f32]) -> Vec<(usize, usize)> {
    if levels.is_empty() {
        return Vec::new();
    }
    let levels: Vec<f32> = levels.iter().map(|l| if l.is_finite() { *l } else { SILENT_DB }).collect();
    let mut sorted = levels.clone();
    sorted.sort_by(f32::total_cmp);
    let at = |q: usize| sorted.get((sorted.len() - 1) * q / 100).copied().unwrap_or(SILENT_DB);
    let (quiet, loud) = (at(15), at(99));
    if loud < QUIET_DB {
        return Vec::new();
    }
    let threshold = (quiet + 12.0).max(QUIET_DB).min(loud - 10.0);
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();
    for (i, l) in levels.iter().enumerate() {
        if *l <= threshold {
            continue;
        }
        match runs.last_mut() {
            Some((_, end, n)) if i <= *end + 25 => {
                *end = i + 1;
                *n += 1;
            }
            _ => runs.push((i, i + 1, 1)),
        }
    }
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (start, end, n) in runs {
        if n < 8 {
            continue;
        }
        let (a, b) = (start.saturating_sub(12), (end + 18).min(levels.len()));
        match out.last_mut() {
            Some((_, e)) if a <= *e => *e = b.max(*e),
            _ => out.push((a, b)),
        }
    }
    out
}

/// A stretch: where it is among the stretches end to end, and in the track (16 kHz samples).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Piece {
    at: usize,
    from: usize,
    len: usize,
}

/// The speech of a track end to end, with a short pause between stretches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compact {
    pieces: Vec<Piece>,
    /// How long it is, in 16 kHz samples.
    pub len: usize,
}

/// The low-pass filter taken before keeping every third sample (48 kHz → 16 kHz): a windowed
/// sinc cutting off at 7.2 kHz.
fn taps() -> [f32; 63] {
    let mut h = [0f32; 63];
    let fc = 7200.0 / 48_000.0;
    for (k, v) in h.iter_mut().enumerate() {
        let x = k as f64 - 31.0;
        let sinc = if x == 0.0 { 2.0 * fc } else { (2.0 * std::f64::consts::PI * fc * x).sin() / (std::f64::consts::PI * x) };
        let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * k as f64 / 62.0).cos() + 0.08 * (4.0 * std::f64::consts::PI * k as f64 / 62.0).cos();
        *v = (sinc * w) as f32;
    }
    let sum: f32 = h.iter().sum();
    for v in &mut h {
        *v /= sum;
    }
    h
}

/// `n` samples at 16 kHz of a track from its 16 kHz sample `from`, one channel.
pub fn read_16k(rec: &Recording, stream: u8, from: usize, n: usize) -> Vec<f32> {
    let h = taps();
    let start = (from as u64 * 3).saturating_sub(31);
    let lead = (from * 3).min(31);
    let count = n * 3 + 31 + lead;
    let x: Vec<f32> = mono(&rec.sound(stream, start, count), channels(rec, stream)).collect();
    (0..n)
        .map(|i| {
            // Sample 3i of the stretch is x[lead + 3i]; the filter is centred on it.
            let centre = lead + 3 * i;
            h.iter().enumerate().map(|(k, w)| w * (centre + k).checked_sub(31).and_then(|j| x.get(j)).copied().unwrap_or(0.0)).sum()
        })
        .collect()
}

impl Compact {
    /// The stretches (in level steps) end to end.
    pub fn new(stretches: &[(usize, usize)]) -> Compact {
        let mut pieces = Vec::with_capacity(stretches.len());
        let mut at = 0usize;
        for &(a, b) in stretches {
            let len = b.saturating_sub(a) * FRAME_16K;
            if len == 0 {
                continue;
            }
            if !pieces.is_empty() {
                at += GAP;
            }
            pieces.push(Piece { at, from: a * FRAME_16K, len });
            at += len;
        }
        Compact { pieces, len: at }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// `n` samples from `start` of the stretches end to end (silence in the pauses).
    pub fn read(&self, rec: &Recording, stream: u8, start: usize, n: usize) -> Vec<f32> {
        let mut out = vec![0f32; n];
        let end = start.saturating_add(n);
        let first = self.pieces.partition_point(|p| p.at + p.len <= start);
        for p in self.pieces.iter().skip(first).take_while(|p| p.at < end) {
            let (a, b) = (start.max(p.at), end.min(p.at + p.len));
            if a >= b {
                continue;
            }
            let sound = read_16k(rec, stream, p.from + (a - p.at), b - a);
            if let Some(dst) = out.get_mut(a - start..b - start) {
                for (d, s) in dst.iter_mut().zip(sound) {
                    *d = s;
                }
            }
        }
        out
    }

    /// The recording's times (µs) of a line `start..end` seconds into the stretches end to end.
    /// Its end may run a little past its stretch (the model's times are rough): less than
    /// [`OVERRUN`] into the next one is taken as the end of its own, else a moment in a pause
    /// would be a stretch of the recording later.
    pub fn span(&self, start: f32, end: f32) -> (u64, u64) {
        let a = self.to_recording(start, false);
        let e = samples(end);
        let j = self.pieces.iter().rposition(|p| p.at <= e);
        let i = self.pieces.iter().position(|p| samples(start) < p.at + p.len);
        let b = match (i, j) {
            (Some(i), Some(j)) if j > i && self.pieces.get(j).is_some_and(|p| e < p.at + OVERRUN) => {
                self.pieces.get(j - 1).map_or(0, |p| (p.from + p.len) as u64 * 125 / 2)
            }
            _ => self.to_recording(end, true),
        };
        (a, b.max(a))
    }

    /// The recording's time (µs) of `secs` into the stretches end to end. A moment in a pause is
    /// the start of the stretch after it (`end` false) or the end of the one before (`end` true).
    pub fn to_recording(&self, secs: f32, end: bool) -> u64 {
        let s = samples(secs);
        let sample = match self.pieces.iter().position(|p| s < p.at + p.len) {
            Some(i) => match self.pieces.get(i) {
                Some(p) if s >= p.at => p.from + (s - p.at),
                Some(p) if !end || i == 0 => p.from,
                _ => self.pieces.get(i - 1).map_or(0, |q| q.from + q.len),
            },
            None => self.pieces.last().map_or(0, |p| p.from + p.len),
        };
        sample as u64 * 125 / 2
    }
}

#[cfg(test)]
mod tests {
    use screengrabcraft_media::recording::{Header, RecordingWriter, StreamInfo, StreamKind};

    use super::*;

    /// A recording with one stereo track: `f(sample)` for its 48 kHz samples, `secs` long.
    fn recording(secs: u64, f: impl Fn(u64) -> f32) -> Recording {
        let streams = vec![StreamInfo { id: 1, kind: StreamKind::SystemSound, name: String::new(), width: 0, height: 0, channels: 2 }];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 30, streams }).unwrap();
        for block in 0..secs * 10 {
            let samples: Vec<i16> = (0..4800u64)
                .flat_map(|i| {
                    let v = (f(block * 4800 + i) * 32767.0) as i16;
                    [v, v]
                })
                .collect();
            w.audio(1, block * 4800, &samples).unwrap();
        }
        Recording::open(Box::new(w.finish(secs * 1_000_000).unwrap())).unwrap()
    }

    fn tone(hz: f32, amp: f32) -> impl Fn(u64) -> f32 {
        move |i| (i as f32 * 2.0 * std::f32::consts::PI * hz / 48_000.0).sin() * amp
    }

    #[test]
    fn levels_are_the_loudness_every_20_ms() {
        let rec = recording(2, |i| if i < 48_000 { tone(440.0, 0.5)(i) } else { 0.0 });
        let l = levels(&rec, 1);
        assert_eq!(l.len(), 100);
        // A sine of amplitude 0.5: its power is 0.125, -9 dB.
        assert!((l[10] + 9.03).abs() < 0.1, "{}", l[10]);
        assert_eq!(l[80], SILENT_DB);
        assert!(levels(&rec, 9).is_empty());
    }

    #[test]
    fn speech_is_found_with_a_little_either_side_and_clicks_are_not() {
        let mut l = vec![-80.0f32; 1000];
        // Two words a fifth of a second apart (one stretch), a click, a long pause, a sentence.
        l[100..130].fill(-20.0);
        l[140..170].fill(-25.0);
        l[400..402].fill(-10.0);
        l[600..800].fill(-22.0);
        assert_eq!(speech(&l), vec![(88, 188), (588, 818)]);
        // A muted microphone (only hiss) has nothing to hear.
        assert!(speech(&[-70.0; 500]).is_empty());
        assert!(speech(&[]).is_empty());
        assert!(speech(&[f32::NAN; 10]).is_empty());
    }

    #[test]
    fn the_stretches_end_to_end_lead_back_to_the_recording() {
        // 1 s to 2 s and 5 s to 6 s, in level steps.
        let c = Compact::new(&[(50, 100), (250, 300)]);
        assert_eq!(c.len, 16_000 + GAP + 16_000);
        assert_eq!(c.to_recording(0.5, false), 1_500_000);
        // In the pause: the next stretch's start, or the last one's end.
        assert_eq!(c.to_recording(1.1, false), 5_000_000);
        assert_eq!(c.to_recording(1.1, true), 2_000_000);
        let secs = |samples: usize| samples as f32 / 16_000.0;
        assert_eq!(c.to_recording(secs(16_000 + GAP + 4_000), false), 5_250_000);
        // A line ending a little into the next stretch ends with its own; well into it, there.
        assert_eq!(c.span(0.2, secs(16_000 + GAP + 3_000)), (1_200_000, 2_000_000));
        assert_eq!(c.span(0.2, secs(16_000 + GAP + 12_000)), (1_200_000, 5_750_000));
        assert_eq!(c.span(0.2, 0.9), (1_200_000, 1_900_000));
        assert_eq!(c.span(0.9, 0.2), (1_900_000, 1_900_000));
        assert_eq!(c.to_recording(999.0, true), 6_000_000);
        assert_eq!(c.to_recording(f32::NAN, false), 1_000_000);
        assert_eq!(Compact::new(&[]).to_recording(3.0, true), 0);
    }

    #[test]
    fn sound_is_read_at_16_khz_without_what_it_cant_hold() {
        // 1 kHz passes, 12 kHz (above what 16 kHz can hold) is taken out rather than folded down.
        let rec = recording(1, |i| tone(1000.0, 0.4)(i) + tone(12_000.0, 0.4)(i));
        let s = read_16k(&rec, 1, 4000, 4000);
        assert_eq!(s.len(), 4000);
        let power = |hz: f32| {
            let (mut re, mut im) = (0f32, 0f32);
            for (i, v) in s.iter().enumerate() {
                let a = i as f32 * 2.0 * std::f32::consts::PI * hz / 16_000.0;
                re += v * a.cos();
                im += v * a.sin();
            }
            (re * re + im * im).sqrt() / s.len() as f32 * 2.0
        };
        assert!((power(1000.0) - 0.4).abs() < 0.02, "{}", power(1000.0));
        // 12 kHz would fold to 4 kHz.
        assert!(power(4000.0) < 0.01, "{}", power(4000.0));
        // The stretches end to end: sound where they are, silence in the pause.
        let c = Compact::new(&[(5, 10), (20, 25)]);
        let all = c.read(&rec, 1, 0, c.len + 100);
        assert!(all[..1600].iter().any(|v| v.abs() > 0.2));
        assert!(all[1600 + 10..1600 + GAP - 10].iter().all(|v| *v == 0.0));
        assert!(all[c.len..].iter().all(|v| *v == 0.0));
        // Past the end of the track: silence, no panic.
        assert!(read_16k(&rec, 1, 1_000_000, 50).iter().all(|v| *v == 0.0));
    }
}
