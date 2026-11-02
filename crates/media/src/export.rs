//! Making the MP4: the edited video's pictures in AV1 and its sound in Opus.
//!
//! rav1e takes its time over a picture, so the video is made in two-second stretches, each
//! starting with a key frame, several at once (one encoder each, sharing the processor), and
//! written in order as they're done. The sound for each stretch goes in beside it.

use std::collections::BTreeMap;
use std::io::BufWriter;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

use crate::MediaError;
use crate::av1::{Av1Encoder, Av1Settings, Coded};
use crate::compose::{self, Composer};
use crate::mp4::{AudioTrack, Mp4Writer, Sample, VideoTrack};
use crate::opus::{self, OpusEncoder};
use crate::recording::{Recording, StreamKind};
use crate::timeline::Timeline;
use crate::yuv::I420;

/// How the video is made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportSettings {
    /// The video is made no larger than this (smaller recordings keep their size).
    pub max_width: u32,
    pub max_height: u32,
    pub fps: u32,
    /// AV1 quantizer: higher is smaller.
    pub quantizer: u8,
    /// Sound, bits a second.
    pub audio_bitrate: u32,
}

impl Default for ExportSettings {
    fn default() -> ExportSettings {
        ExportSettings { max_width: 1920, max_height: 1080, fps: 30, quantizer: 130, audio_bitrate: 96_000 }
    }
}

/// What was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exported {
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    pub bytes: u64,
    pub duration_us: u64,
}

/// Pictures in a stretch: two seconds.
const STRETCH_SECS: u64 = 2;

/// Encoders side by side, and threads for each, on this computer.
fn workers() -> (usize, usize) {
    let cores = std::thread::available_parallelism().map(usize::from).unwrap_or(4);
    let n = (cores / 6).clamp(1, 4);
    (n, (cores / n).max(1))
}

/// Makes `out` (written beside it first, then put in place) from the recording as edited.
/// `progress` goes from 0 to 1000; setting `cancel` stops it and leaves nothing behind.
pub fn export(
    rec: Arc<Recording>,
    timeline: &Timeline,
    out: &Path,
    s: ExportSettings,
    progress: &AtomicU32,
    cancel: &AtomicBool,
) -> Result<Exported, MediaError> {
    let length = timeline.length();
    if length == 0 {
        return Err(MediaError::Encode("there's nothing left to make a video of".to_string()));
    }
    let fps = s.fps.clamp(1, 60);
    let (sw, sh) = rec
        .header
        .streams
        .iter()
        .find(|st| st.kind == StreamKind::Screen)
        .or_else(|| rec.header.streams.iter().find(|st| st.kind == StreamKind::Camera))
        .map(|st| (st.width, st.height))
        .unwrap_or((1280, 720));
    let (w, h) = compose::fit(sw, sh, s.max_width.max(16), s.max_height.max(16));
    let frames = length.saturating_mul(u64::from(fps)).div_ceil(1_000_000);
    let per = STRETCH_SECS * u64::from(fps);
    let stretches = frames.div_ceil(per) as usize;
    let with_sound = timeline.tracks.iter().any(|t| !t.kind.is_video() && !t.muted && !t.clips.is_empty());
    let part = out.with_extension("mp4.part");
    let result = (|| {
        let file = BufWriter::new(std::fs::File::create(&part)?);
        let video = VideoTrack { width: w, height: h, timescale: fps, av1c: Vec::new() };
        let audio = with_sound.then_some(AudioTrack { channels: 2, pre_skip: opus::PRE_SKIP });
        let mut mp4 = Mp4Writer::new(file, Some(video), audio)?;
        let mut sound = if with_sound { Some(OpusEncoder::new(2, s.audio_bitrate)?) } else { None };
        let (n, threads) = workers();
        let next = AtomicUsize::new(0);
        let settings = Av1Settings { width: w, height: h, fps, quantizer: s.quantizer, speed: 10, threads };
        std::thread::scope(|scope| -> Result<(), MediaError> {
            let (tx, rx) = mpsc::channel::<(usize, Result<Vec<Coded>, MediaError>)>();
            for _ in 0..n.min(stretches.max(1)) {
                let (tx, rec, next) = (tx.clone(), rec.clone(), &next);
                scope.spawn(move || {
                    let mut composer = Composer::new(rec, timeline, w, h);
                    loop {
                        let k = next.fetch_add(1, Ordering::SeqCst);
                        if k >= stretches || cancel.load(Ordering::SeqCst) {
                            break;
                        }
                        let made = (|| {
                            let mut enc = Av1Encoder::new(settings)?;
                            let mut coded = Vec::with_capacity(per as usize);
                            for f in (k as u64 * per)..((k as u64 + 1) * per).min(frames) {
                                if cancel.load(Ordering::SeqCst) {
                                    return Err(MediaError::Encode("cancelled".to_string()));
                                }
                                let pic = composer.frame(timeline, f * 1_000_000 / u64::from(fps));
                                let yuv = I420::from_rgba(pic.pixels(), pic.width(), pic.height())
                                    .ok_or_else(|| MediaError::Encode("a picture of the wrong size".to_string()))?;
                                coded.extend(enc.send(&yuv)?);
                            }
                            coded.extend(enc.finish()?);
                            Ok(coded)
                        })();
                        if tx.send((k, made)).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx);
            let mut waiting: BTreeMap<usize, Vec<Coded>> = BTreeMap::new();
            let mut written = 0usize;
            for (k, made) in rx {
                match made {
                    Ok(coded) => {
                        waiting.insert(k, coded);
                    }
                    Err(e) => {
                        cancel.store(true, Ordering::SeqCst);
                        return Err(e);
                    }
                }
                while let Some(coded) = waiting.remove(&written) {
                    if written == 0
                        && let Some(first) = coded.first()
                    {
                        let record = Av1Encoder::new(settings)?.config_record(first);
                        mp4.set_av1c(record);
                    }
                    let samples: Vec<Sample> = coded.into_iter().map(|c| Sample { data: c.data, duration: 1, sync: c.key }).collect();
                    mp4.write_video(&samples)?;
                    if let Some(enc) = &mut sound {
                        let from = written as u64 * per * 1_000_000 / u64::from(fps);
                        let to = (((written + 1) as u64 * per).min(frames) * 1_000_000 / u64::from(fps)).min(length);
                        let (a, b) = (compose::to_samples(from), compose::to_samples(to.max(from)));
                        let pcm = compose::mix(&rec, timeline, a, (b - a) as usize);
                        let mut packets = enc.push(&pcm)?;
                        if written + 1 == stretches {
                            packets.extend(enc.finish()?);
                        }
                        let samples: Vec<Sample> =
                            packets.into_iter().map(|p| Sample { data: p, duration: opus::FRAME as u32, sync: true }).collect();
                        mp4.write_audio(&samples)?;
                    }
                    written += 1;
                    progress.store((written * 1000 / stretches.max(1)) as u32, Ordering::SeqCst);
                }
            }
            if cancel.load(Ordering::SeqCst) {
                return Err(MediaError::Encode("cancelled".to_string()));
            }
            if written < stretches {
                return Err(MediaError::Encode("the video stopped being made partway".to_string()));
            }
            Ok(())
        })?;
        let file = mp4.finish()?;
        file.into_inner().map_err(|e| MediaError::Io(e.into_error()))?.sync_all()?;
        std::fs::rename(&part, out)?;
        Ok(Exported { width: w, height: h, frames, bytes: std::fs::metadata(out)?.len(), duration_us: length })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::{Color, Raster};

    use super::*;
    use crate::recording::{Header, RecordingWriter, StreamInfo};

    fn recording(secs: u64) -> Arc<Recording> {
        let streams = vec![
            StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 97, height: 61, channels: 0 },
            StreamInfo { id: 1, kind: StreamKind::Microphone, name: String::new(), width: 0, height: 0, channels: 1 },
        ];
        let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap();
        for t in 0..secs * 10 {
            let mut pic = Raster::filled(97, 61, Color::WHITE).unwrap();
            let x = (t * 3 % 80) as u32;
            for y in 10..30 {
                for xx in x..x + 15 {
                    let i = ((y * 97 + xx) * 4) as usize;
                    pic.pixels_mut()[i..i + 3].copy_from_slice(&[0x4E, 0x3C, 0xA8]);
                }
            }
            w.video_frame(0, t * 100_000, &pic).unwrap();
            let tone: Vec<i16> = (0..4800).map(|i| (((t * 4800 + i) as f32 * 0.0575).sin() * 8000.0) as i16).collect();
            w.audio(1, t * 4800, &tone).unwrap();
        }
        Arc::new(Recording::open(Box::new(w.finish(secs * 1_000_000).unwrap())).unwrap())
    }

    fn top_boxes(b: &[u8]) -> Vec<String> {
        let mut at = 0usize;
        let mut out = Vec::new();
        while at + 16 <= b.len() {
            let s32 = u32::from_be_bytes(b[at..at + 4].try_into().unwrap());
            let size = if s32 == 1 { u64::from_be_bytes(b[at + 8..at + 16].try_into().unwrap()) } else { u64::from(s32) };
            out.push(String::from_utf8_lossy(&b[at + 4..at + 8]).to_string());
            at += size as usize;
        }
        out
    }

    #[test]
    fn an_edited_recording_becomes_an_mp4_of_the_edited_length() {
        let rec = recording(5);
        let mut tl = Timeline::new(&rec);
        tl.cut(1_000_000, 2_000_000).unwrap();
        let dir = std::env::temp_dir().join(format!("sgc-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("video.mp4");
        let progress = AtomicU32::new(0);
        let s = ExportSettings { fps: 10, ..ExportSettings::default() };
        let done = export(rec, &tl, &out, s, &progress, &AtomicBool::new(false)).unwrap();
        assert_eq!((done.width, done.height), (96, 60));
        assert_eq!(done.frames, 40);
        assert_eq!(progress.load(Ordering::SeqCst), 1000);
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(top_boxes(&bytes), ["ftyp", "mdat", "moov"]);
        assert!(!out.with_extension("mp4.part").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cancelling_leaves_nothing_behind() {
        let rec = recording(3);
        let tl = Timeline::new(&rec);
        let dir = std::env::temp_dir().join(format!("sgc-cancel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("video.mp4");
        let r = export(rec, &tl, &out, ExportSettings::default(), &AtomicU32::new(0), &AtomicBool::new(true));
        assert!(r.is_err());
        assert!(!out.exists() && !out.with_extension("mp4.part").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nothing_left_is_an_error_not_an_empty_file() {
        let rec = recording(1);
        let mut tl = Timeline::new(&rec);
        tl.cut(0, 5_000_000).unwrap();
        let out = std::env::temp_dir().join("sgc-nothing.mp4");
        assert!(export(rec, &tl, &out, ExportSettings::default(), &AtomicU32::new(0), &AtomicBool::new(false)).is_err());
        assert!(!out.exists());
    }
}
