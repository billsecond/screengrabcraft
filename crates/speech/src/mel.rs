//! What Whisper hears: the log-mel spectrogram of 30 seconds of 16 kHz sound, as OpenAI's
//! `log_mel_spectrogram` makes it (a Hann window of 400 samples every 160, 80 or 128 mel bands
//! on the Slaney scale, log10, the top 8 decades kept, scaled to about -1..1).

use rustfft::num_complex::Complex32;

pub const RATE: usize = 16_000;
pub const N_FFT: usize = 400;
pub const HOP: usize = 160;
/// A window: 30 seconds.
pub const WINDOW: usize = 30 * RATE;
/// Spectrogram frames in a window.
pub const FRAMES: usize = WINDOW / HOP;

fn hz_to_mel(f: f64) -> f64 {
    let f_sp = 200.0 / 3.0;
    if f >= 1000.0 { 15.0 + (f / 1000.0).ln() / (6.4f64.ln() / 27.0) } else { f / f_sp }
}

fn mel_to_hz(m: f64) -> f64 {
    let f_sp = 200.0 / 3.0;
    if m >= 15.0 { 1000.0 * ((m - 15.0) * (6.4f64.ln() / 27.0)).exp() } else { m * f_sp }
}

/// The mel filter bank (`librosa.filters.mel(sr=16000, n_fft=400, n_mels)`): `n_mels` rows of
/// 201 weights.
pub fn filters(n_mels: usize) -> Vec<f32> {
    let bins = N_FFT / 2 + 1;
    let fft: Vec<f64> = (0..bins).map(|i| i as f64 * (RATE as f64 / 2.0) / (bins - 1) as f64).collect();
    let (lo, hi) = (hz_to_mel(0.0), hz_to_mel(RATE as f64 / 2.0));
    let edges: Vec<f64> = (0..n_mels + 2).map(|i| mel_to_hz(lo + (hi - lo) * i as f64 / (n_mels + 1) as f64)).collect();
    let mut w = vec![0f32; n_mels * bins];
    for (i, row) in w.chunks_exact_mut(bins).enumerate() {
        let (Some(&a), Some(&b), Some(&c)) = (edges.get(i), edges.get(i + 1), edges.get(i + 2)) else { break };
        let norm = 2.0 / (c - a);
        for (v, f) in row.iter_mut().zip(&fft) {
            let up = (f - a) / (b - a);
            let down = (c - f) / (c - b);
            *v = (up.min(down).max(0.0) * norm) as f32;
        }
    }
    w
}

/// Makes spectrograms; keeps its FFT and window between them.
pub struct Mel {
    n_mels: usize,
    filters: Vec<f32>,
    hann: Vec<f32>,
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
}

impl Mel {
    pub fn new(n_mels: usize) -> Mel {
        let hann = (0..N_FFT).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N_FFT as f32).cos()).collect();
        let fft = rustfft::FftPlanner::<f32>::new().plan_fft_forward(N_FFT);
        Mel { n_mels, filters: filters(n_mels), hann, fft }
    }

    /// The spectrogram of up to 30 seconds of sound (silence after it): `n_mels × 3000`, a row
    /// per band.
    pub fn window(&self, samples: &[f32]) -> Vec<f32> {
        let mut audio = vec![0f32; WINDOW];
        let n = samples.len().min(WINDOW);
        if let (Some(dst), Some(src)) = (audio.get_mut(..n), samples.get(..n)) {
            dst.copy_from_slice(src);
        }
        // Centred frames: the sound is mirrored at both ends.
        let pad = N_FFT / 2;
        let mut padded = Vec::with_capacity(WINDOW + 2 * pad);
        padded.extend((1..=pad).rev().map(|i| audio.get(i).copied().unwrap_or(0.0)));
        padded.extend_from_slice(&audio);
        padded.extend((0..pad).map(|i| audio.get(WINDOW - 2 - i).copied().unwrap_or(0.0)));
        let bins = N_FFT / 2 + 1;
        let mut mel = vec![0f32; self.n_mels * FRAMES];
        let mut buf = vec![Complex32::default(); N_FFT];
        let mut power = vec![0f32; bins];
        for t in 0..FRAMES {
            let frame = padded.get(t * HOP..t * HOP + N_FFT).unwrap_or_default();
            for ((b, s), w) in buf.iter_mut().zip(frame).zip(&self.hann) {
                *b = Complex32::new(s * w, 0.0);
            }
            self.fft.process(&mut buf);
            for (p, b) in power.iter_mut().zip(&buf) {
                *p = b.norm_sqr();
            }
            for (m, row) in self.filters.chunks_exact(bins).enumerate() {
                let s: f32 = row.iter().zip(&power).map(|(a, b)| a * b).sum();
                if let Some(v) = mel.get_mut(m * FRAMES + t) {
                    *v = s.max(1e-10).log10();
                }
            }
        }
        let floor = mel.iter().copied().fold(f32::MIN, f32::max) - 8.0;
        for v in &mut mel {
            *v = (v.max(floor) + 4.0) / 4.0;
        }
        mel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_bank_is_librosas() {
        let f = filters(80);
        assert_eq!(f.len(), 80 * 201);
        // Values of librosa.filters.mel(sr=16000, n_fft=400, n_mels=80) (as in Whisper's mel_filters.npz).
        assert!((f[1] - 0.024_862_6).abs() < 1e-5, "{}", f[1]);
        assert_eq!(f[2], 0.0);
        assert!((f[201 + 2] - 0.022_872).abs() < 1e-5, "{}", f[203]);
        // Each band covers its own stretch, low bands narrow and high ones wide.
        let width = |m: usize| f[m * 201..(m + 1) * 201].iter().filter(|v| **v > 0.0).count();
        assert!(width(2) <= 2 && width(79) > 8);
    }

    #[test]
    fn a_tone_lights_up_its_band_and_silence_is_flat() {
        let mel = Mel::new(80);
        let tone: Vec<f32> = (0..RATE).map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 1000.0 / RATE as f32).sin() * 0.5).collect();
        let m = mel.window(&tone);
        assert_eq!(m.len(), 80 * FRAMES);
        // In the first second, the loudest band is the one around 1 kHz (band 26 of 80: 1 kHz
        // is 15 mels of 45.2).
        let at = |band: usize, t: usize| m[band * FRAMES + t];
        let loudest = (0..80).max_by(|a, b| at(*a, 50).total_cmp(&at(*b, 50))).unwrap();
        assert!((25..=27).contains(&loudest), "{loudest}");
        // After it, silence: the floor, everywhere the same.
        assert!((at(5, 2000) - at(70, 2900)).abs() < 1e-6);
        let silent = mel.window(&[]);
        assert!(silent.iter().all(|v| (v - silent[0]).abs() < 1e-6));
    }
}
