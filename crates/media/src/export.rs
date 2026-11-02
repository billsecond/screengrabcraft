//! Making the MP4: the edited video's pictures in AV1 and its sound in Opus.
//!
//! rav1e takes its time over a picture, so the video is made in two-second stretches, each
//! starting with a key frame, several at once (one encoder each, sharing the processor), and
//! written in order as they're done. The sound for each stretch goes in beside it.
//!
//! An [`Exporter`] can be fed while a recording is still going: each stretch is made as soon as
//! the recording has got past it, so a video is ready moments after the recording stops.
//! [`export`] makes one from a finished recording in one go.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread::JoinHandle;

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

/// The size a recording's video is made at.
pub fn video_size(rec: &Recording, s: &ExportSettings) -> (u32, u32) {
    let (sw, sh) = rec
        .header
        .streams
        .iter()
        .find(|st| st.kind == StreamKind::Screen)
        .or_else(|| rec.header.streams.iter().find(|st| st.kind == StreamKind::Camera))
        .map(|st| (st.width, st.height))
        .unwrap_or((1280, 720));
    compose::fit(sw, sh, s.max_width.max(16), s.max_height.max(16))
}

/// A stretch to encode: which, up to which picture, from what.
struct Job {
    k: usize,
    end: u64,
    rec: Arc<Recording>,
    timeline: Arc<Timeline>,
}

type Done = (usize, Result<Vec<Coded>, MediaError>);

/// Makes one MP4, stretch by stretch, on a pool of encoders of its own.
pub struct Exporter {
    out: PathBuf,
    part: PathBuf,
    settings: ExportSettings,
    av1: Av1Settings,
    per: u64,
    mp4: Option<Mp4Writer<BufWriter<File>>>,
    sound: Option<OpusEncoder>,
    jobs: Option<mpsc::Sender<Job>>,
    done: mpsc::Receiver<Done>,
    pool: Vec<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
    waiting: BTreeMap<usize, Vec<Coded>>,
    sent: usize,
    written: usize,
    /// The newest recording and edits it was handed (the sound is mixed from them).
    latest: Option<(Arc<Recording>, Arc<Timeline>)>,
    finished: bool,
}

impl Exporter {
    /// Starts one: `out` is written beside it (`.mp4.part`) and moved into place at the end.
    /// `width × height` is the video's size (see [`video_size`]); `with_sound` whether it has a
    /// sound track.
    pub fn new(
        out: &Path,
        width: u32,
        height: u32,
        with_sound: bool,
        settings: ExportSettings,
        cancel: Arc<AtomicBool>,
    ) -> Result<Exporter, MediaError> {
        let fps = settings.fps.clamp(1, 60);
        let part = out.with_extension("mp4.part");
        let file = BufWriter::new(File::create(&part)?);
        let video = VideoTrack { width, height, timescale: fps, av1c: Vec::new() };
        let audio = with_sound.then_some(AudioTrack { channels: 2, pre_skip: opus::PRE_SKIP });
        let mp4 = Mp4Writer::new(file, Some(video), audio)?;
        let sound = if with_sound { Some(OpusEncoder::new(2, settings.audio_bitrate)?) } else { None };
        let (n, threads) = workers();
        let av1 = Av1Settings { width, height, fps, quantizer: settings.quantizer, speed: 10, threads };
        let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();
        let jobs_rx = Arc::new(Mutex::new(jobs_rx));
        let (done_tx, done) = mpsc::channel::<Done>();
        let mut pool = Vec::with_capacity(n);
        for i in 0..n {
            let (jobs, done, cancel) = (jobs_rx.clone(), done_tx.clone(), cancel.clone());
            let handle = std::thread::Builder::new().name(format!("encoder-{i}")).spawn(move || {
                loop {
                    let job = { jobs.lock().unwrap_or_else(PoisonError::into_inner).recv() };
                    let Ok(job) = job else { break };
                    let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| encode(&job, av1, fps, &cancel)))
                        .unwrap_or_else(|_| Err(MediaError::Encode("an encoder stopped unexpectedly".to_string())));
                    if done.send((job.k, made)).is_err() {
                        break;
                    }
                }
            });
            match handle {
                Ok(h) => pool.push(h),
                Err(e) => log::warn!("an encoder could not start: {e}"),
            }
        }
        if pool.is_empty() {
            let _ = std::fs::remove_file(&part);
            return Err(MediaError::Encode("no encoder could start".to_string()));
        }
        Ok(Exporter {
            out: out.to_path_buf(),
            part,
            settings,
            av1,
            per: STRETCH_SECS * u64::from(fps),
            mp4: Some(mp4),
            sound,
            jobs: Some(jobs_tx),
            done,
            pool,
            cancel,
            waiting: BTreeMap::new(),
            sent: 0,
            written: 0,
            latest: None,
            finished: false,
        })
    }

    fn fps(&self) -> u64 {
        u64::from(self.av1.fps)
    }

    /// Pictures in `length` µs.
    fn frames_in(&self, length: u64) -> u64 {
        length.saturating_mul(self.fps()).div_ceil(1_000_000)
    }

    /// Hands over what's been recorded so far: every whole stretch before `ready` µs is started.
    pub fn feed(&mut self, rec: Arc<Recording>, timeline: Arc<Timeline>, ready: u64) -> Result<(), MediaError> {
        let whole = ready.saturating_mul(self.fps()) / 1_000_000 / self.per;
        while (self.sent as u64) < whole {
            self.send(self.sent, (self.sent as u64 + 1) * self.per, &rec, &timeline)?;
        }
        self.latest = Some((rec, timeline));
        self.collect(false, false)
    }

    fn send(&mut self, k: usize, end: u64, rec: &Arc<Recording>, timeline: &Arc<Timeline>) -> Result<(), MediaError> {
        let job = Job { k, end, rec: rec.clone(), timeline: timeline.clone() };
        let sent = self.jobs.as_ref().map(|j| j.send(job).is_ok()).unwrap_or(false);
        if !sent {
            return Err(MediaError::Encode("the encoders stopped".to_string()));
        }
        self.sent = k + 1;
        Ok(())
    }

    /// Writes what's done, in order. `wait`: until everything sent is written.
    fn collect(&mut self, wait: bool, last: bool) -> Result<(), MediaError> {
        loop {
            while let Some(coded) = self.waiting.remove(&self.written) {
                self.write(coded, last && self.written + 1 == self.sent)?;
            }
            if self.written >= self.sent {
                return Ok(());
            }
            let next = if wait {
                self.done.recv().map_err(|_| MediaError::Encode("the encoders stopped".to_string()))?
            } else {
                match self.done.try_recv() {
                    Ok(d) => d,
                    Err(_) => return Ok(()),
                }
            };
            match next {
                (k, Ok(coded)) => {
                    self.waiting.insert(k, coded);
                }
                (_, Err(e)) => {
                    self.cancel.store(true, Ordering::SeqCst);
                    return Err(e);
                }
            }
        }
    }

    /// Stretch `self.written` and its sound.
    fn write(&mut self, coded: Vec<Coded>, last: bool) -> Result<(), MediaError> {
        let Some(mp4) = self.mp4.as_mut() else { return Err(MediaError::Encode("the video is already finished".to_string())) };
        if self.written == 0
            && let Some(first) = coded.first()
        {
            mp4.set_av1c(Av1Encoder::new(self.av1)?.config_record(first));
        }
        let samples: Vec<Sample> = coded.into_iter().map(|c| Sample { data: c.data, duration: 1, sync: c.key }).collect();
        mp4.write_video(&samples)?;
        if let (Some(enc), Some((rec, timeline))) = (self.sound.as_mut(), self.latest.as_ref()) {
            let fps = u64::from(self.av1.fps);
            let from = self.written as u64 * self.per * 1_000_000 / fps;
            let mut to = (self.written as u64 + 1) * self.per * 1_000_000 / fps;
            if last {
                to = to.min(timeline.length());
            }
            let (a, b) = (compose::to_samples(from), compose::to_samples(to.max(from)));
            let pcm = compose::mix(rec, timeline, a, (b - a) as usize);
            let mut packets = enc.push(&pcm)?;
            if last {
                packets.extend(enc.finish()?);
            }
            let samples: Vec<Sample> = packets.into_iter().map(|p| Sample { data: p, duration: opus::FRAME as u32, sync: true }).collect();
            mp4.write_audio(&samples)?;
        }
        self.written += 1;
        Ok(())
    }

    /// The recording is over: the rest is made, the file finished and put in place.
    /// `progress` (0 to 1000) is how much of the whole is written as it goes.
    pub fn finish(mut self, rec: Arc<Recording>, timeline: Arc<Timeline>, progress: &AtomicU32) -> Result<Exported, MediaError> {
        let length = timeline.length();
        if length == 0 {
            return Err(MediaError::Encode("there's nothing left to make a video of".to_string()));
        }
        let frames = self.frames_in(length);
        let stretches = frames.div_ceil(self.per) as usize;
        if self.sent > stretches {
            return Err(MediaError::Encode("the recording got shorter while it was being made".to_string()));
        }
        // The last whole stretch sent may have been made before the end was known: the
        // stretches not sent yet are sent now, the last one cut at the end.
        while self.sent < stretches {
            let k = self.sent;
            let end = ((k as u64 + 1) * self.per).min(frames);
            self.send(k, end, &rec, &timeline)?;
        }
        self.latest = Some((rec, timeline));
        self.jobs = None;
        while self.written < self.sent {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(MediaError::Encode("cancelled".to_string()));
            }
            self.collect_one()?;
            progress.store((self.written * 1000 / stretches.max(1)) as u32, Ordering::SeqCst);
        }
        let mp4 = self.mp4.take().ok_or_else(|| MediaError::Encode("the video is already finished".to_string()))?;
        let file = mp4.finish()?;
        file.into_inner().map_err(|e| MediaError::Io(e.into_error()))?.sync_all()?;
        std::fs::rename(&self.part, &self.out)?;
        self.finished = true;
        progress.store(1000, Ordering::SeqCst);
        let (width, height) = (self.av1.width, self.av1.height);
        let _ = self.settings;
        Ok(Exported { width, height, frames, bytes: std::fs::metadata(&self.out)?.len(), duration_us: length })
    }

    /// Waits for one more stretch and writes what's in order.
    fn collect_one(&mut self) -> Result<(), MediaError> {
        let before = self.written;
        while self.written == before {
            let last = self.written + 1 == self.sent;
            if let Some(coded) = self.waiting.remove(&self.written) {
                self.write(coded, last)?;
                continue;
            }
            match self.done.recv() {
                Ok((k, Ok(coded))) => {
                    self.waiting.insert(k, coded);
                }
                Ok((_, Err(e))) => {
                    self.cancel.store(true, Ordering::SeqCst);
                    return Err(e);
                }
                Err(_) => return Err(MediaError::Encode("the encoders stopped".to_string())),
            }
        }
        Ok(())
    }
}

impl Drop for Exporter {
    fn drop(&mut self) {
        if !self.finished {
            self.cancel.store(true, Ordering::SeqCst);
        }
        self.jobs = None;
        for h in self.pool.drain(..) {
            let _ = h.join();
        }
        if !self.finished {
            self.mp4 = None;
            let _ = std::fs::remove_file(&self.part);
        }
    }
}

/// One stretch, from its key frame.
fn encode(job: &Job, av1: Av1Settings, fps: u32, cancel: &AtomicBool) -> Result<Vec<Coded>, MediaError> {
    let per = STRETCH_SECS * u64::from(fps);
    let mut composer = Composer::new(job.rec.clone(), &job.timeline, av1.width, av1.height);
    let mut enc = Av1Encoder::new(av1)?;
    let mut coded = Vec::with_capacity(per as usize);
    for f in (job.k as u64 * per)..job.end {
        if cancel.load(Ordering::SeqCst) {
            return Err(MediaError::Encode("cancelled".to_string()));
        }
        let pic = composer.frame(&job.timeline, f * 1_000_000 / u64::from(fps));
        let yuv =
            I420::from_rgba(pic.pixels(), pic.width(), pic.height()).ok_or_else(|| MediaError::Encode("a picture of the wrong size".to_string()))?;
        coded.extend(enc.send(&yuv)?);
    }
    coded.extend(enc.finish()?);
    Ok(coded)
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
    if timeline.length() == 0 {
        return Err(MediaError::Encode("there's nothing left to make a video of".to_string()));
    }
    if cancel.load(Ordering::SeqCst) {
        return Err(MediaError::Encode("cancelled".to_string()));
    }
    let (w, h) = video_size(&rec, &s);
    let with_sound = timeline.tracks.iter().any(|t| !t.kind.is_video() && !t.muted && !t.clips.is_empty());
    // The caller's cancel is watched here and passed on to the encoders.
    let stop = Arc::new(AtomicBool::new(false));
    let exporter = Exporter::new(out, w, h, with_sound, s, stop.clone())?;
    std::thread::scope(|scope| {
        let watcher = scope.spawn(|| {
            while !stop.load(Ordering::SeqCst) {
                if cancel.load(Ordering::SeqCst) {
                    stop.store(true, Ordering::SeqCst);
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        });
        let r = exporter.finish(rec, Arc::new(timeline.clone()), progress);
        stop.store(true, Ordering::SeqCst);
        let _ = watcher.join();
        r
    })
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::{Color, Raster};

    use super::*;
    use crate::recording::{Header, RecordingWriter, StreamInfo};

    fn writer() -> RecordingWriter<Vec<u8>> {
        let streams = vec![
            StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: 97, height: 61, channels: 0 },
            StreamInfo { id: 1, kind: StreamKind::Microphone, name: String::new(), width: 0, height: 0, channels: 1 },
        ];
        RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).unwrap()
    }

    fn tenth(w: &mut RecordingWriter<Vec<u8>>, t: u64) {
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

    fn recording(secs: u64) -> Arc<Recording> {
        let mut w = writer();
        for t in 0..secs * 10 {
            tenth(&mut w, t);
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

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sgc-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn an_edited_recording_becomes_an_mp4_of_the_edited_length() {
        let rec = recording(5);
        let mut tl = Timeline::new(&rec);
        tl.cut(1_000_000, 2_000_000).unwrap();
        let dir = dir("export");
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
    fn made_while_recording_it_comes_out_the_same_as_made_after() {
        let s = ExportSettings { fps: 10, ..ExportSettings::default() };
        let dir = dir("live");
        let live = dir.join("live.mp4");
        let mut w = writer();
        let mut ex = Exporter::new(&live, 96, 60, true, s, Arc::default()).unwrap();
        // Fed every second as the recording grows, as the recorder does.
        for t in 0..55u64 {
            tenth(&mut w, t);
            if t % 10 == 9 {
                let rec = Arc::new(Recording::open(Box::new(w.out_bytes())).unwrap());
                let tl = Arc::new(Timeline::new(&rec));
                ex.feed(rec.clone(), tl, rec.duration_us.saturating_sub(500_000)).unwrap();
            }
        }
        let rec = Arc::new(Recording::open(Box::new(w.finish(5_500_000).unwrap())).unwrap());
        let tl = Arc::new(Timeline::new(&rec));
        let done = ex.finish(rec.clone(), tl.clone(), &AtomicU32::new(0)).unwrap();
        assert_eq!(done.frames, 55);
        let after = dir.join("after.mp4");
        export(rec, &tl, &after, s, &AtomicU32::new(0), &AtomicBool::new(false)).unwrap();
        assert_eq!(std::fs::read(&live).unwrap(), std::fs::read(&after).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cancelling_leaves_nothing_behind() {
        let rec = recording(3);
        let tl = Timeline::new(&rec);
        let dir = dir("cancel");
        let out = dir.join("video.mp4");
        let r = export(rec, &tl, &out, ExportSettings::default(), &AtomicU32::new(0), &AtomicBool::new(true));
        assert!(r.is_err());
        assert!(!out.exists() && !out.with_extension("mp4.part").exists());
        // An exporter dropped halfway leaves nothing either.
        let ex = Exporter::new(&out, 96, 60, false, ExportSettings::default(), Arc::default()).unwrap();
        assert!(out.with_extension("mp4.part").exists());
        drop(ex);
        assert!(!out.with_extension("mp4.part").exists());
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
