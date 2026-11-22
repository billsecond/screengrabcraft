//! Narration recorded over a video in the video editor: the microphone the owner chose, turned on
//! only while Record Narration is pressed (through the capture crate's sound input, as the
//! Recorder records it), the take written as a WAV beside the recording.

use std::path::{Path, PathBuf};
use std::time::Instant;

use screengrabcraft_capture::audio::{AudioCapture, AudioSource, RATE, SoundBlock};
use screengrabcraft_ui::services::NarrationTake;

/// The longest take kept (an hour); what's past it is let go.
const MOST_FRAMES: u64 = RATE as u64 * 3600;

/// A take being recorded.
pub struct Take {
    capture: AudioCapture,
    /// Samples (a channel's) heard before the microphone was ready, and the video began playing:
    /// not kept, so the take starts where the playhead was.
    skip: u64,
    /// The recording it's for: the take is written beside it.
    beside: PathBuf,
}

/// Turns the microphone on (by name, or the system's default) for a take.
pub fn start(mic: Option<String>, beside: &Path) -> Result<Take, String> {
    let started = Instant::now();
    let capture = AudioCapture::start(AudioSource::Microphone(mic), started).map_err(|e| e.to_string())?;
    let skip = (started.elapsed().as_secs_f64() * f64::from(RATE)) as u64;
    log::info!("narration: the microphone ({}) is on", capture.name);
    Ok(Take { capture, skip, beside: beside.to_path_buf() })
}

/// The blocks a device handed over as one stretch of stereo sound (16-bit, interleaved), each where
/// it belongs (a gap silent, a mono one heard on both sides), the first `skip` samples left out, at
/// most `most` of them.
pub fn assemble(blocks: &[SoundBlock], channels: u8, skip: u64, most: u64) -> Vec<i16> {
    let ch = usize::from(channels.clamp(1, 2));
    let end = blocks.iter().map(|b| b.first.saturating_add((b.samples.len() / ch) as u64)).max().unwrap_or(0);
    let frames = end.saturating_sub(skip).min(most).min(MOST_FRAMES);
    let Ok(n) = usize::try_from(frames) else { return Vec::new() };
    let mut out = vec![0i16; n.saturating_mul(2)];
    for b in blocks {
        for (k, f) in b.samples.chunks_exact(ch).enumerate() {
            let at = b.first.saturating_add(k as u64);
            let Some(i) = at.checked_sub(skip).and_then(|i| usize::try_from(i).ok()) else { continue };
            if i >= n {
                break;
            }
            let (l, r) = match f {
                [m] => (*m, *m),
                [l, r] => (*l, *r),
                _ => continue,
            };
            if let Some(slot) = out.get_mut(i * 2..i * 2 + 2) {
                slot.copy_from_slice(&[l, r]);
            }
        }
    }
    out
}

/// A WAV file of stereo 48 kHz sound.
pub fn wav(samples: &[i16]) -> Vec<u8> {
    let frames = (samples.len() / 2) as u64;
    let mut bytes = screengrabcraft_media::export::wav_header(frames).to_vec();
    bytes.reserve(samples.len() * 2);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    bytes
}

/// Where a take for the recording at `beside` is written: `<its name> narration.wav` beside it
/// (a number added if that's taken).
fn take_path(beside: &Path) -> PathBuf {
    let stem = beside.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Video".to_string());
    crate::video::free(&beside.with_file_name(format!("{stem} narration.wav")))
}

impl NarrationTake for Take {
    fn level(&self) -> f32 {
        self.capture.level()
    }

    fn finish(self: Box<Self>) -> Result<(String, u64), String> {
        let Take { capture, skip, beside } = *self;
        let channels = capture.channels;
        let blocks = capture.stop();
        let samples = assemble(&blocks, channels, skip, MOST_FRAMES);
        if samples.is_empty() {
            return Err("the microphone heard nothing".to_string());
        }
        let path = take_path(&beside);
        screengrabcraft_desktop::store::write_atomic(&path, &wav(&samples)).map_err(|e| format!("{}: {e}", path.display()))?;
        let us = (samples.len() / 2) as u64 * 1_000_000 / u64::from(RATE);
        log::info!(
            "narration: {:.1} s written as {}",
            us as f64 / 1e6,
            path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
        );
        Ok((path.to_string_lossy().to_string(), us))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_take_s_blocks_are_put_where_they_belong_and_its_start_left_out() {
        let blocks = vec![
            SoundBlock { first: 0, samples: vec![1, 2, 3, 4] },
            // A gap of three, then more.
            SoundBlock { first: 7, samples: vec![9, 9] },
        ];
        // Mono, the first two left out: 2, 3, 4, then silence, then 9, 9 (both sides each).
        let s = assemble(&blocks, 1, 2, 1000);
        assert_eq!(s, vec![3, 3, 4, 4, 0, 0, 0, 0, 0, 0, 9, 9, 9, 9]);
        // Stereo stays as it is.
        let st = assemble(&[SoundBlock { first: 0, samples: vec![1, -1, 2, -2] }], 2, 0, 1000);
        assert_eq!(st, vec![1, -1, 2, -2]);
        // At most so many; nothing heard is nothing.
        assert_eq!(assemble(&blocks, 1, 0, 3).len(), 6);
        assert!(assemble(&[], 1, 0, 1000).is_empty());
        assert!(assemble(&blocks, 1, 1_000, 1000).is_empty());
        // Junk never panics.
        let _ = assemble(&[SoundBlock { first: u64::MAX - 2, samples: vec![1; 5] }], 0, u64::MAX - 10, 1000);
    }

    #[test]
    fn a_take_is_a_wav_beside_its_recording() {
        let bytes = wav(&[1, -1, 2, -2]);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + 8);
        let dir = std::env::temp_dir().join(format!("sgc-narration-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rec = dir.join("Talk.sgvideo");
        assert_eq!(take_path(&rec), dir.join("Talk narration.wav"));
        std::fs::write(dir.join("Talk narration.wav"), b"x").unwrap();
        assert_eq!(take_path(&rec), dir.join("Talk narration (2).wav"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
