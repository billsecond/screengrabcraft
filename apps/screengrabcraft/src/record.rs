//! Captures that run for a while: an area recorded as an animated GIF, an area captured as its
//! content scrolls and joined into one long picture, and video: a small MP4 (Record Video) or
//! a recording with a track for each source to edit (Video Production). Each runs on a thread of
//! its own and is stopped from the tray, its small control window or its shortcut.

use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use screengrabcraft_capture::audio::{self, AudioCapture, AudioSource, SoundBlock};
use screengrabcraft_capture::camera::{self, CameraCapture};
use screengrabcraft_media::compose;
use screengrabcraft_media::export::{self, ExportSettings, Exporter};
use screengrabcraft_media::recording::{self, FileSource, Header, RecordingWriter, StreamInfo, StreamKind};
use screengrabcraft_media::timeline::Timeline;
use screengrabcraft_model::settings::VideoOptions;
use screengrabcraft_model::{IRect, Raster};
use screengrabcraft_render::anim::GifWriter;
use screengrabcraft_render::stitch::{Step, Stitcher};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Gif,
    Scrolling,
    Video,
    Production,
}

/// What a finished recording made.
pub enum Output {
    /// A GIF written to this file (in the temporary folder), with this many frames.
    Gif(PathBuf, u32),
    /// The joined scrolling picture.
    Picture(Raster),
    /// A video made: the MP4 and how long it is (µs).
    Video(PathBuf, u64),
    /// A Video Production recording, to open in the editor.
    Production(PathBuf),
    Cancelled,
}

pub struct Recording {
    pub kind: Kind,
    pub started: Instant,
    /// Where on the desktop (for placing the control window).
    pub desktop: IRect,
    stop: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    /// Frames so far (GIF), or the joined picture's height (scrolling).
    pub progress: Arc<AtomicU32>,
    /// Frames that could not be joined lately (scrolling): the owner scrolled too fast.
    pub lost: Arc<AtomicU32>,
    /// Making the video after the recording stopped: 0 not yet, then 1 to 1001 (its progress + 1).
    pub saving: Arc<AtomicU32>,
    /// How loud the computer's sound and the microphone are just now (`f32` bits, 0 to 1).
    pub levels: Arc<[AtomicU32; 2]>,
    /// What the recording takes in, or couldn't, for the owner (`Computer sound`, `No camera`…).
    pub sources: Arc<Mutex<Vec<String>>>,
    handle: Option<JoinHandle<Result<Output, String>>>,
}

impl Recording {
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Asks it to stop and keep what it has.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Asks it to stop and throw away what it has.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.stop.store(true, Ordering::SeqCst);
    }

    pub fn finished(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Its result, once it has finished.
    pub fn take(&mut self) -> Option<Result<Output, String>> {
        if !self.finished() {
            return None;
        }
        let h = self.handle.take()?;
        Some(h.join().unwrap_or_else(|_| Err("the recording stopped unexpectedly".to_string())))
    }
}

/// Starts recording part of a monitor (`area` in its pixels).
pub fn start(kind: Kind, monitor: u32, area: IRect, desktop: IRect, fps: u32, max_secs: u32, wake: egui::Context) -> Result<Recording, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(AtomicU32::new(0));
    let lost = Arc::new(AtomicU32::new(0));
    let (s, c, p, l) = (stop.clone(), cancel.clone(), progress.clone(), lost.clone());
    let handle = std::thread::Builder::new()
        .name(format!("{kind:?}").to_lowercase())
        .spawn(move || {
            let out = match kind {
                Kind::Gif => gif(monitor, area, fps, max_secs, &s, &c, &p, &wake),
                Kind::Scrolling => scrolling(monitor, area, &s, &c, &p, &l, &wake),
                Kind::Video | Kind::Production => Err("a video is started with start_video".to_string()),
            };
            wake.request_repaint();
            out
        })
        .map_err(|e| e.to_string())?;
    Ok(Recording {
        kind,
        started: Instant::now(),
        desktop,
        stop,
        cancel,
        progress,
        lost,
        saving: Arc::default(),
        levels: Arc::default(),
        sources: Arc::default(),
        handle: Some(handle),
    })
}

fn gif(
    monitor: u32,
    area: IRect,
    fps: u32,
    max_secs: u32,
    stop: &AtomicBool,
    cancel: &AtomicBool,
    progress: &AtomicU32,
    wake: &egui::Context,
) -> Result<Output, String> {
    let dir = std::env::temp_dir().join("ScreenGrabCraft");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("recording-{}.gif", std::process::id()));
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut writer = GifWriter::new(BufWriter::new(file), area.w, area.h).map_err(|e| e.to_string())?;
    let mut grab = screengrabcraft_capture::grabber(monitor, area).map_err(|e| e.to_string())?;
    let tick = Duration::from_secs_f64(1.0 / f64::from(fps.clamp(1, 30)));
    let started = Instant::now();
    let mut last = Instant::now();
    let mut due = Instant::now();
    let mut frames = 0u32;
    let limit = Duration::from_secs(u64::from(max_secs.clamp(1, 600)));
    while !stop.load(Ordering::SeqCst) && started.elapsed() < limit {
        let at = Instant::now();
        match grab() {
            Ok(frame) if frame.width() == area.w && frame.height() == area.h => {
                let delay = at.duration_since(last).as_millis().min(u128::from(u32::MAX)) as u32;
                // A frame left out only makes the one before last longer.
                if writer.offer(frame, if frames == 0 { 0 } else { delay }).map_err(|e| e.to_string())? {
                    last = at;
                    frames += 1;
                    progress.store(frames, Ordering::SeqCst);
                    wake.request_repaint();
                }
            }
            Ok(_) => log::warn!("a frame of the wrong size was left out"),
            Err(e) => log::warn!("a frame could not be captured: {e}"),
        }
        // On a steady beat: what a frame took comes off the wait for the next.
        due = (due + tick).max(Instant::now());
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
    }
    let (_, n) = writer.finish(at_least(last.elapsed()).as_millis().min(u128::from(u32::MAX)) as u32).map_err(|e| e.to_string())?;
    if cancel.load(Ordering::SeqCst) || n == 0 {
        let _ = std::fs::remove_file(&path);
        return Ok(Output::Cancelled);
    }
    Ok(Output::Gif(path, n))
}

fn at_least(d: Duration) -> Duration {
    d.max(Duration::from_millis(100))
}

fn scrolling(
    monitor: u32,
    area: IRect,
    stop: &AtomicBool,
    cancel: &AtomicBool,
    progress: &AtomicU32,
    lost: &AtomicU32,
    wake: &egui::Context,
) -> Result<Output, String> {
    let mut s = Stitcher::new();
    let tick = Duration::from_millis(120);
    while !stop.load(Ordering::SeqCst) {
        let at = Instant::now();
        match screengrabcraft_capture::grab_on_monitor(monitor, area) {
            Ok(frame) => match s.push(frame) {
                Step::Scrolled(_) | Step::Started => {
                    progress.store(s.length(), Ordering::SeqCst);
                    lost.store(0, Ordering::SeqCst);
                    wake.request_repaint();
                }
                Step::Lost => {
                    lost.fetch_add(1, Ordering::SeqCst);
                    wake.request_repaint();
                }
                Step::Full => break,
                Step::Unchanged | Step::WrongSize => {}
            },
            Err(e) => log::warn!("a frame could not be captured: {e}"),
        }
        if let Some(rest) = tick.checked_sub(at.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    if cancel.load(Ordering::SeqCst) {
        return Ok(Output::Cancelled);
    }
    s.finish().map(Output::Picture).ok_or_else(|| "nothing was captured".to_string())
}

/// What a video recording is to do.
pub struct VideoPlan {
    pub kind: Kind,
    pub monitor: u32,
    /// In the monitor's pixels.
    pub area: IRect,
    pub desktop: IRect,
    pub fps: u32,
    pub max_secs: u32,
    pub options: VideoOptions,
    /// Record Video: the MP4 to make (the recording itself goes to the temporary folder).
    /// Video Production: the recording file, which the editor opens.
    pub out: PathBuf,
    /// Record Video: the largest the MP4 is made.
    pub max_size: (u32, u32),
    /// When it was started, in local time, for the recording file.
    pub created: String,
}

/// Starts a video recording.
pub fn start_video(plan: VideoPlan, wake: egui::Context) -> Result<Recording, String> {
    let r = Recording {
        kind: plan.kind,
        started: Instant::now(),
        desktop: plan.desktop,
        stop: Arc::default(),
        cancel: Arc::default(),
        progress: Arc::default(),
        lost: Arc::default(),
        saving: Arc::default(),
        levels: Arc::default(),
        sources: Arc::default(),
        handle: None,
    };
    let (stop, cancel, progress, saving, levels, sources) =
        (r.stop.clone(), r.cancel.clone(), r.progress.clone(), r.saving.clone(), r.levels.clone(), r.sources.clone());
    let handle = std::thread::Builder::new()
        .name("video".into())
        .spawn(move || {
            let shared =
                Shared { stop: &stop, cancel: &cancel, progress: &progress, saving: &saving, levels: &levels, sources: &sources, wake: &wake };
            let out = video(&plan, &shared);
            wake.request_repaint();
            out
        })
        .map_err(|e| e.to_string())?;
    Ok(Recording { handle: Some(handle), ..r })
}

struct Shared<'a> {
    stop: &'a AtomicBool,
    cancel: &'a AtomicBool,
    progress: &'a AtomicU32,
    saving: &'a AtomicU32,
    levels: &'a [AtomicU32; 2],
    sources: &'a Mutex<Vec<String>>,
    wake: &'a egui::Context,
}

impl Shared<'_> {
    fn note(&self, text: String) {
        if let Ok(mut s) = self.sources.lock() {
            s.push(text);
        }
    }
}

/// What the writer thread is handed.
enum Item {
    Screen(u64, Raster),
    Camera(u64, Vec<u8>),
    Sound(u8, SoundBlock),
}

/// Screen pictures waiting to be written, at most (a slow disk loses pictures, not memory).
const WAITING: usize = 8;

/// The sound devices and the camera a recording asked for, those that started.
struct Devices {
    system: Option<AudioCapture>,
    mic: Option<AudioCapture>,
    camera: Option<CameraCapture>,
}

fn open_devices(o: &VideoOptions, started: Instant, s: &Shared) -> Devices {
    let system = if o.system_sound && audio::system_sound_supported() {
        AudioCapture::start(AudioSource::SystemSound, started).map_err(|e| s.note(format!("No computer sound: {e}"))).ok()
    } else {
        None
    };
    let mic = if o.microphone {
        AudioCapture::start(AudioSource::Microphone(o.microphone_name.clone()), started).map_err(|e| s.note(format!("No microphone: {e}"))).ok()
    } else {
        None
    };
    let camera = if o.camera {
        match o.camera_index.or_else(|| camera::cameras().first().map(|c| c.0)) {
            Some(i) => CameraCapture::start(i, started).map_err(|e| s.note(format!("No camera: {e}"))).ok(),
            None => {
                s.note("No camera was found".to_string());
                None
            }
        }
    } else {
        None
    };
    Devices { system, mic, camera }
}

fn video(plan: &VideoPlan, s: &Shared) -> Result<Output, String> {
    let file_path = match plan.kind {
        Kind::Production => plan.out.clone(),
        _ => {
            let dir = std::env::temp_dir().join("ScreenGrabCraft");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            dir.join(format!("recording-{}.{}", std::process::id(), recording::EXT))
        }
    };
    let started = Instant::now();
    // The devices first: they take a moment to start, and their sound is placed by the clock.
    let d = open_devices(&plan.options, started, s);
    let mut streams = vec![StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: plan.area.w, height: plan.area.h, channels: 0 }];
    if let Some(a) = &d.system {
        streams.push(StreamInfo { id: 1, kind: StreamKind::SystemSound, name: a.name.clone(), width: 0, height: 0, channels: a.channels });
        s.note("Computer sound".to_string());
    }
    if let Some(a) = &d.mic {
        streams.push(StreamInfo { id: 2, kind: StreamKind::Microphone, name: a.name.clone(), width: 0, height: 0, channels: a.channels });
        s.note(format!("Microphone: {}", a.name));
    }
    if let Some(c) = &d.camera {
        streams.push(StreamInfo { id: 3, kind: StreamKind::Camera, name: c.name.clone(), width: 640, height: 480, channels: 0 });
        s.note(format!("Camera: {}", c.name));
    }
    let header = Header { version: 1, created: plan.created.clone(), fps: plan.fps, streams };
    let file = std::fs::File::create(&file_path).map_err(|e| format!("{}: {e}", file_path.display()))?;
    let writer = RecordingWriter::new(BufWriter::new(file), &header).map_err(|e| e.to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Item>();
    let waiting = Arc::new(AtomicUsize::new(0));
    let w = waiting.clone();
    let write_thread = std::thread::Builder::new()
        .name("video-writer".into())
        .spawn(move || -> Result<RecordingWriter<BufWriter<std::fs::File>>, String> {
            let mut writer = writer;
            for item in rx {
                let r = match item {
                    Item::Screen(pts, pic) => {
                        w.fetch_sub(1, Ordering::SeqCst);
                        writer.video_frame(0, pts, &pic)
                    }
                    Item::Camera(pts, jpeg) => writer.jpeg_frame(3, pts, &jpeg),
                    Item::Sound(stream, b) => writer.audio(stream, b.first, &b.samples),
                };
                r.map_err(|e| e.to_string())?;
            }
            Ok(writer)
        })
        .map_err(|e| e.to_string())?;
    // Record Video: the MP4 is made as the recording goes, two seconds at a time.
    let live = (plan.kind == Kind::Video).then(|| start_live(plan, &d)).flatten();
    let feeding = AtomicBool::new(true);
    let (frames, live) = std::thread::scope(|scope| {
        let feeder = live.map(|(ex, cancel)| scope.spawn(|| feed_live(ex, cancel, &file_path, &feeding)));
        let frames = record_screen(plan, s, &d, &tx, &waiting, started);
        feeding.store(false, Ordering::SeqCst);
        let live = feeder.and_then(|h| h.join().ok()).flatten();
        (frames, live)
    });
    let duration = started.elapsed().as_micros() as u64;
    // What the devices still hold, then they're let go (the camera's light goes off).
    if let Some(a) = d.system {
        send_sound(&tx, 1, a.stop());
    }
    if let Some(a) = d.mic {
        send_sound(&tx, 2, a.stop());
    }
    if let Some(c) = d.camera {
        for f in c.stop() {
            let _ = tx.send(Item::Camera(f.at.as_micros() as u64, f.jpeg));
        }
    }
    drop(tx);
    let writer = write_thread.join().map_err(|_| "the recording stopped unexpectedly".to_string())??;
    writer.finish(duration).map_err(|e| e.to_string())?;
    if s.cancel.load(Ordering::SeqCst) || frames.is_err() {
        let _ = std::fs::remove_file(&file_path);
        return frames.map(|_| Output::Cancelled);
    }
    if plan.kind == Kind::Production {
        return Ok(Output::Production(file_path));
    }
    // Record Video: the rest of the MP4 is made, then the recording goes.
    s.saving.store(1, Ordering::SeqCst);
    s.wake.request_repaint();
    let made = match live {
        Some((ex, cancel)) => finish_live(ex, &cancel, &file_path, s),
        None => make_mp4(plan, &file_path, s),
    };
    let _ = std::fs::remove_file(&file_path);
    if s.cancel.load(Ordering::SeqCst) {
        return Ok(Output::Cancelled);
    }
    made.map(|d| Output::Video(plan.out.clone(), d))
}

fn send_sound(tx: &std::sync::mpsc::Sender<Item>, id: u8, blocks: Vec<SoundBlock>) {
    for b in blocks {
        let _ = tx.send(Item::Sound(id, b));
    }
}

/// The screen on a steady beat, with the devices' sound and pictures passed on as they come.
/// The number of screen pictures taken, or why there were none.
fn record_screen(
    plan: &VideoPlan,
    s: &Shared,
    d: &Devices,
    tx: &std::sync::mpsc::Sender<Item>,
    waiting: &AtomicUsize,
    started: Instant,
) -> Result<u32, String> {
    let mut grab = screengrabcraft_capture::grabber(plan.monitor, plan.area).map_err(|e| e.to_string())?;
    let tick = Duration::from_secs_f64(1.0 / f64::from(plan.fps.clamp(5, 30)));
    let limit = Duration::from_secs(u64::from(plan.max_secs.max(1)));
    let mut due = Instant::now();
    let mut frames = 0u32;
    while !s.stop.load(Ordering::SeqCst) && started.elapsed() < limit {
        let at = started.elapsed();
        match grab() {
            Ok(pic) if pic.width() == plan.area.w && pic.height() == plan.area.h => {
                if waiting.load(Ordering::SeqCst) < WAITING {
                    waiting.fetch_add(1, Ordering::SeqCst);
                    let _ = tx.send(Item::Screen(at.as_micros() as u64, pic));
                    frames += 1;
                    s.progress.store(frames, Ordering::SeqCst);
                } else {
                    log::warn!("a video picture was left out: the disk is behind");
                }
            }
            Ok(_) => log::warn!("a picture of the wrong size was left out"),
            Err(e) => log::warn!("a picture could not be captured: {e}"),
        }
        if let Some(a) = &d.system {
            send_sound(tx, 1, a.take());
            s.levels[0].store(a.level().to_bits(), Ordering::Relaxed);
        }
        if let Some(a) = &d.mic {
            send_sound(tx, 2, a.take());
            s.levels[1].store(a.level().to_bits(), Ordering::Relaxed);
        }
        if let Some(c) = &d.camera {
            for f in c.take() {
                let _ = tx.send(Item::Camera(f.at.as_micros() as u64, f.jpeg));
            }
        }
        s.wake.request_repaint_after(Duration::from_millis(250));
        due = (due + tick).max(Instant::now());
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
    }
    if frames == 0 {
        return Err("nothing could be recorded".to_string());
    }
    Ok(frames)
}

/// The MP4 started as the recording starts (none if it can't be: it's then made after).
fn start_live(plan: &VideoPlan, d: &Devices) -> Option<(Exporter, Arc<AtomicBool>)> {
    let settings = ExportSettings { max_width: plan.max_size.0, max_height: plan.max_size.1, fps: plan.fps, ..ExportSettings::default() };
    let (w, h) = compose::fit(plan.area.w, plan.area.h, settings.max_width, settings.max_height);
    let with_sound = d.system.is_some() || d.mic.is_some();
    let cancel = Arc::new(AtomicBool::new(false));
    match Exporter::new(&plan.out, w, h, with_sound, settings, cancel.clone()) {
        Ok(ex) => Some((ex, cancel)),
        Err(e) => {
            log::warn!("the video will be made after the recording: {e}");
            None
        }
    }
}

/// While recording: every second, what's been written is read again and every stretch it
/// has got past is made. None if that went wrong (the video is then made after).
fn feed_live(mut ex: Exporter, cancel: Arc<AtomicBool>, file: &std::path::Path, feeding: &AtomicBool) -> Option<(Exporter, Arc<AtomicBool>)> {
    let mut next = Instant::now() + Duration::from_secs(1);
    while feeding.load(Ordering::SeqCst) {
        if Instant::now() < next {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        next = Instant::now() + Duration::from_secs(1);
        let Ok(rec) =
            FileSource::open(file).map_err(|e| e.to_string()).and_then(|f| recording::Recording::open(Box::new(f)).map_err(|e| e.to_string()))
        else {
            continue;
        };
        let rec = Arc::new(rec);
        let tl = Arc::new(Timeline::new(&rec));
        // Half a second's margin: the sound comes in a little after the pictures.
        let ready = rec.duration_us.saturating_sub(500_000);
        if let Err(e) = ex.feed(rec, tl, ready) {
            log::warn!("the video will be made after the recording: {e}");
            return None;
        }
    }
    Some((ex, cancel))
}

/// The recording is over: the stretches not made yet are made, and the MP4 is put in place.
fn finish_live(ex: Exporter, cancel: &AtomicBool, file: &std::path::Path, s: &Shared) -> Result<u64, String> {
    let source = FileSource::open(file).map_err(|e| e.to_string())?;
    let rec = Arc::new(recording::Recording::open(Box::new(source)).map_err(|e| e.to_string())?);
    let tl = Arc::new(Timeline::new(&rec));
    let progress = AtomicU32::new(0);
    let done = std::thread::scope(|scope| {
        let relay = scope.spawn(|| {
            while s.saving.load(Ordering::SeqCst) != 0 {
                if s.cancel.load(Ordering::SeqCst) {
                    cancel.store(true, Ordering::SeqCst);
                    break;
                }
                let p = progress.load(Ordering::SeqCst);
                if p >= 1000 {
                    break;
                }
                s.saving.store(p + 1, Ordering::SeqCst);
                s.wake.request_repaint();
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        let r = ex.finish(rec, tl, &progress);
        progress.store(1000, Ordering::SeqCst);
        let _ = relay.join();
        r
    })
    .map_err(|e| e.to_string())?;
    Ok(done.duration_us)
}

/// Record Video's MP4, made from the recording file. How long it is, in µs.
fn make_mp4(plan: &VideoPlan, file_path: &std::path::Path, s: &Shared) -> Result<u64, String> {
    let source = FileSource::open(file_path).map_err(|e| e.to_string())?;
    let rec = Arc::new(recording::Recording::open(Box::new(source)).map_err(|e| e.to_string())?);
    let tl = Timeline::new(&rec);
    let settings = ExportSettings { max_width: plan.max_size.0, max_height: plan.max_size.1, fps: plan.fps, ..ExportSettings::default() };
    // The export counts 0 to 1000; the control window reads it + 1 (0 is "not saving").
    let progress = AtomicU32::new(0);
    let done = std::thread::scope(|scope| {
        let relay = scope.spawn(|| {
            while s.saving.load(Ordering::SeqCst) != 0 && !s.cancel.load(Ordering::SeqCst) {
                let p = progress.load(Ordering::SeqCst);
                if p >= 1000 {
                    break;
                }
                s.saving.store(p + 1, Ordering::SeqCst);
                s.wake.request_repaint();
                std::thread::sleep(Duration::from_millis(200));
            }
        });
        let r = export::export(rec, &tl, &plan.out, settings, &progress, s.cancel);
        progress.store(1000, Ordering::SeqCst);
        let _ = relay.join();
        r
    })
    .map_err(|e| e.to_string())?;
    Ok(done.duration_us)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real path on the real screen: 3 s of a small area and the computer's sound, made into
    /// an MP4, then deleted unopened. Run by hand only: `cargo test -p screengrabcraft live_video
    /// -- --ignored`.
    #[test]
    #[ignore]
    fn live_video() {
        let monitor = screengrabcraft_capture::monitors().unwrap().into_iter().next().unwrap();
        let size = |k: &str, d: u32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let area = IRect { x: 0, y: 0, w: size("SGC_W", 320), h: size("SGC_H", 200) };
        let secs = u64::from(size("SGC_SECS", 3));
        let out = std::env::temp_dir().join(format!("sgc-live-{}.mp4", std::process::id()));
        let plan = VideoPlan {
            kind: Kind::Video,
            monitor: monitor.id,
            area,
            desktop: area,
            fps: 30,
            max_secs: 60,
            options: VideoOptions { system_sound: true, microphone: false, microphone_name: None, camera: false, camera_index: None },
            out: out.clone(),
            max_size: (1920, 1080),
            created: String::new(),
        };
        let mut rec = start_video(plan, egui::Context::default()).unwrap();
        std::thread::sleep(Duration::from_secs(secs));
        rec.stop();
        let started = Instant::now();
        let result = loop {
            if let Some(r) = rec.take() {
                break r;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let frames = rec.progress.load(Ordering::SeqCst);
        let sources = rec.sources.lock().unwrap().clone();
        match result {
            Ok(Output::Video(path, us)) => {
                let size = std::fs::metadata(&path).unwrap().len();
                println!(
                    "{frames} frames in 3 s, {} µs long, {size} bytes, made {:.2} s after stopping, sources {sources:?}",
                    us,
                    started.elapsed().as_secs_f64()
                );
                std::fs::remove_file(&path).unwrap();
                assert!(u64::from(frames) >= secs * 25, "{frames} frames");
            }
            other => panic!("{:?}", other.err()),
        }
    }
}
