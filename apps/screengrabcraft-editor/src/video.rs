//! The desktop's part of the video editor: making the MP4 on a thread of its own, playing the
//! sound through the speakers, and the strip's small pictures of videos.

#![cfg_attr(not(any(target_os = "windows", target_os = "macos", target_os = "linux")), allow(unused_imports))]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use screengrabcraft_media::export::{self, ExportSettings};
use screengrabcraft_media::recording::{FileSource, Recording, ScreenDecoder, StreamKind};
use screengrabcraft_media::timeline::Timeline;
use screengrabcraft_media::{captions, compose};
use screengrabcraft_model::{Color, Raster};
use screengrabcraft_ui::services::{SoundOut, VideoExport, VideoJob};

/// `path`, or `name (2).ext` and on beside it if that is taken.
fn free(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let (dir, stem, ext) = (
        path.parent().map(Path::to_path_buf).unwrap_or_default(),
        path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Video".to_string()),
        path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_else(|| "mp4".to_string()),
    );
    (2..10_000)
        .map(|n| dir.join(format!("{stem} ({n}).{ext}")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("{stem} {}.{ext}", std::process::id())))
}

/// Starts making the MP4; the answer comes back through the job's `done`.
pub fn export(job: VideoJob, wake: egui::Context) -> Result<VideoExport, String> {
    let x = VideoExport::default();
    let out = free(Path::new(&job.out));
    let (progress, cancel, done) = (x.progress.clone(), x.cancel.clone(), x.done.clone());
    std::thread::Builder::new()
        .name("export".into())
        .spawn(move || {
            let settings = ExportSettings { max_width: job.max_size.0, max_height: job.max_size.1, fps: job.fps, ..ExportSettings::default() };
            let captions = job.captions.filter(|c| !c.is_empty());
            let burned = captions.clone().filter(|_| job.burn).map(Arc::new);
            let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                export::export_with_captions(job.recording, &job.timeline, burned, &out, settings, &progress, &cancel)
            }));
            let result = match made {
                Ok(Ok(e)) => {
                    log::info!("made a video: {} × {}, {} frames, {} bytes", e.width, e.height, e.frames, e.bytes);
                    // The captions beside it, as they fall on it (players pick up a .srt of the same name).
                    if let Some(c) = &captions {
                        for (ext, text) in [("txt", captions::to_text(c)), ("srt", captions::to_srt(c))] {
                            let side = out.with_extension(ext);
                            if let Err(e) = screengrabcraft_desktop::store::write_atomic(&side, text.as_bytes()) {
                                log::warn!("the video's captions could not be written beside it ({}): {e}", side.display());
                            }
                        }
                    }
                    Ok(out.to_string_lossy().to_string())
                }
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err("something went wrong while making the video".to_string()),
            };
            *done.lock().unwrap_or_else(PoisonError::into_inner) = Some(result);
            wake.request_repaint();
        })
        .map_err(|e| e.to_string())?;
    Ok(x)
}

/// A small picture of a video for the strip: a recording's first picture, or a play sign for
/// an MP4 (it isn't decoded here).
pub fn thumbnail(path: &Path) -> Option<Raster> {
    let is_mp4 = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mp4"));
    if is_mp4 {
        return play_sign();
    }
    let rec = Recording::open(Box::new(FileSource::open(path).ok()?)).ok()?;
    let screen = rec.header.streams.iter().find(|s| s.kind == StreamKind::Screen)?.id;
    let first = rec.frames(screen).first()?.pts;
    ScreenDecoder::new(screen).frame_at(&rec, first)
}

/// A dark tile with a white triangle.
fn play_sign() -> Option<Raster> {
    let (w, h) = (320u32, 180u32);
    let mut r = Raster::filled(w, h, Color::rgb(0x26, 0x26, 0x2B)).ok()?;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let px = r.pixels_mut();
    for y in 0..h {
        for x in 0..w {
            // The triangle: from (cx-22, cy-28) and (cx-22, cy+28) to (cx+30, cy).
            let (fx, fy) = (x as f32 - (cx - 22.0), (y as f32 - cy).abs());
            if (0.0..=52.0).contains(&fx) && fy <= 28.0 * (1.0 - fx / 52.0) {
                let i = ((y * w + x) * 4) as usize;
                if let Some(p) = px.get_mut(i..i + 3) {
                    p.copy_from_slice(&[0xF2, 0xF2, 0xF5]);
                }
            }
        }
    }
    Some(r)
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
/// The edited sound, playing through the speakers until it's dropped.
pub struct Sound {
    from_us: u64,
    rate: u64,
    /// Frames the device has played so far.
    played: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
impl SoundOut for Sound {
    fn position_us(&self) -> Option<u64> {
        let played = self.played.load(Ordering::Relaxed);
        (played > 0).then(|| self.from_us + played.saturating_mul(1_000_000) / self.rate.max(1))
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
impl Drop for Sound {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Sound is mixed this far ahead of what's heard.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
const AHEAD_SECS: f64 = 0.25;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
impl Sound {
    pub fn start(rec: Arc<Recording>, timeline: Timeline, from_us: u64) -> Result<Sound, String> {
        let device = cpal::default_host().default_output_device().ok_or("there are no speakers")?;
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let rate = u64::from(config.sample_rate().max(1));
        let ch = usize::from(config.channels().max(1));
        let format = config.sample_format();
        let played = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (p, s) = (played.clone(), stop.clone());
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);
        let thread = std::thread::Builder::new()
            .name("sound-out".into())
            .spawn(move || {
                // Stereo frames at the device's rate, waiting to be played.
                let queue: Arc<Mutex<VecDeque<[f32; 2]>>> = Arc::default();
                let q = queue.clone();
                let counter = p.clone();
                let stream = device.build_output_stream_raw(
                    config.config(),
                    format,
                    move |data: &mut cpal::Data, _: &cpal::OutputCallbackInfo| {
                        let mut queue = q.lock().unwrap_or_else(PoisonError::into_inner);
                        let frames = data.len() / ch;
                        let mut next = || queue.pop_front().unwrap_or([0.0, 0.0]);
                        let value = |frame: [f32; 2], c: usize| match (ch, c) {
                            (1, _) => (frame[0] + frame[1]) / 2.0,
                            (_, 0) => frame[0],
                            (_, 1) => frame[1],
                            _ => 0.0,
                        };
                        if let Some(out) = data.as_slice_mut::<f32>() {
                            for f in out.chunks_mut(ch) {
                                let frame = next();
                                for (c, v) in f.iter_mut().enumerate() {
                                    *v = value(frame, c);
                                }
                            }
                        } else if let Some(out) = data.as_slice_mut::<i16>() {
                            for f in out.chunks_mut(ch) {
                                let frame = next();
                                for (c, v) in f.iter_mut().enumerate() {
                                    *v = (value(frame, c).clamp(-1.0, 1.0) * 32767.0) as i16;
                                }
                            }
                        }
                        counter.fetch_add(frames as u64, Ordering::Relaxed);
                    },
                    |e| log::warn!("speakers: {e}"),
                    None,
                );
                let stream = match stream.map_err(|e| e.to_string()).and_then(|st| st.play().map(|()| st).map_err(|e| e.to_string())) {
                    Ok(st) => st,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                let end = compose::to_samples(timeline.length());
                let mut at48 = compose::to_samples(from_us);
                // Where between two 48 kHz samples the device's next one falls.
                let mut pos = 0.0f64;
                let mut last = [0.0f32; 2];
                let step = 48_000.0 / rate as f64;
                while !s.load(Ordering::SeqCst) {
                    let waiting = queue.lock().unwrap_or_else(PoisonError::into_inner).len();
                    if (waiting as f64) < rate as f64 * AHEAD_SECS && at48 < end {
                        let n = 4800usize;
                        let mixed = compose::mix(&rec, &timeline, at48, n);
                        at48 += n as u64;
                        let frames: Vec<[f32; 2]> = mixed.as_chunks::<2>().0.to_vec();
                        let mut out = Vec::with_capacity((n as f64 / step) as usize + 2);
                        while pos < frames.len() as f64 {
                            let i = pos.floor() as usize;
                            let t = (pos - pos.floor()) as f32;
                            let a = if i == 0 { last } else { frames.get(i - 1).copied().unwrap_or(last) };
                            let b = frames.get(i).copied().unwrap_or(a);
                            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
                            pos += step;
                        }
                        pos -= frames.len() as f64;
                        last = frames.last().copied().unwrap_or(last);
                        queue.lock().unwrap_or_else(PoisonError::into_inner).extend(out);
                    } else {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                drop(stream);
            })
            .map_err(|e| e.to_string())?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Sound { from_us, rate, played, stop, thread: Some(thread) }),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                stop.store(true, Ordering::SeqCst);
                Err("the speakers didn't start".to_string())
            }
        }
    }
}

/// The BSDs: no speakers yet (the video plays without its sound).
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub struct Sound;

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
impl Sound {
    pub fn start(_rec: Arc<Recording>, _timeline: Timeline, _from_us: u64) -> Result<Sound, String> {
        Err("sound can't be played on this system yet".to_string())
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
impl SoundOut for Sound {
    fn position_us(&self) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_name_gets_a_number() {
        let dir = std::env::temp_dir().join(format!("sgc-free-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Video.mp4");
        assert_eq!(free(&p), p);
        std::fs::write(&p, b"x").unwrap();
        assert_eq!(free(&p), dir.join("Video (2).mp4"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_mp4_has_a_play_sign_for_its_picture() {
        let r = thumbnail(Path::new("C:/nowhere/Video.mp4")).unwrap();
        assert_eq!((r.width(), r.height()), (320, 180));
        assert_eq!(r.pixel(165, 90).unwrap().r, 0xF2);
        assert_eq!(r.pixel(10, 10).unwrap().r, 0x26);
        assert!(thumbnail(Path::new("C:/nowhere/Missing.sgvideo")).is_none());
    }
}
