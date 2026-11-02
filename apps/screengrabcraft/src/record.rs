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

use screengrabcraft_capture::Monitor;
use screengrabcraft_capture::audio::{self, AudioCapture, AudioSource, SoundBlock};
use screengrabcraft_capture::camera::{self, CameraCapture};
use screengrabcraft_media::compose;
use screengrabcraft_media::export::{self, ExportSettings, Exporter};
use screengrabcraft_media::recording::{self, FileSource, Header, RecordingWriter, StreamInfo, StreamKind};
use screengrabcraft_media::timeline::Timeline;
use screengrabcraft_model::settings::VideoOptions;
use screengrabcraft_model::{IRect, MonitorChoice, Raster, RecordArea, RecorderOptions};
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
    /// When it was asked to stop.
    stopped: Option<Instant>,
    handle: Option<JoinHandle<Result<Output, String>>>,
}

impl Recording {
    /// How long it has recorded: up to now, or to when it was asked to stop.
    pub fn elapsed(&self) -> Duration {
        self.stopped.map_or_else(|| self.started.elapsed(), |at| at.saturating_duration_since(self.started))
    }

    /// Whether it has been asked to stop and is finishing.
    pub fn stopping(&self) -> bool {
        self.stopped.is_some()
    }

    /// Asks it to stop and keep what it has.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.stopped.get_or_insert_with(Instant::now);
    }

    /// Asks it to stop and throw away what it has.
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.stop();
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
        stopped: None,
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

/// What of the screen a video recording shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Part of one monitor, in its pixels (an area dragged out, a window, a fixed size).
    Part { monitor: u32, area: IRect },
    /// One monitor, whole.
    Monitor { monitor: u32 },
    /// Every monitor (by id, and where it is on the desktop), put together in one picture of
    /// `size`.
    All { monitors: Vec<(u32, IRect)>, size: (u32, u32) },
    /// None: the camera and the sound only.
    Off,
}

/// The largest an all-screens recording is: the monitors' pictures are made smaller to fit, so
/// each frame is put together and written in time.
pub const ALL_SCREENS_MAX: (u32, u32) = (3840, 2160);

/// The desktop that holds every monitor, and the size an all-screens recording of it is made at.
pub fn all_screens_size(rects: &[IRect]) -> Option<(IRect, (u32, u32))> {
    let desktop = rects.iter().copied().reduce(IRect::union)?;
    if desktop.is_empty() {
        return None;
    }
    Some((desktop, compose::fit(desktop.w, desktop.h, ALL_SCREENS_MAX.0, ALL_SCREENS_MAX.1)))
}

/// Which of the monitors the Recorder means: see [`MonitorChoice::pick`]; the main one when it
/// names none or none like it is connected.
pub fn chosen_monitor<'a>(choice: Option<&MonitorChoice>, monitors: &'a [Monitor]) -> Option<&'a Monitor> {
    let main = monitors.iter().find(|m| m.primary).or(monitors.first());
    let Some(c) = choice else { return main };
    let list: Vec<(&str, i32, i32)> = monitors.iter().map(|m| (m.name.as_str(), m.rect.x, m.rect.y)).collect();
    c.pick(&list).and_then(|i| monitors.get(i)).or(main)
}

/// What a Video Production recording is of, from the Recorder's choices, and the part of the
/// desktop its control window is placed by. `chosen` is the overlay's answer for an area chosen on
/// the screen (Region, Window, Fixed size): the monitor, the area in its pixels and on the desktop.
pub fn production_screen(r: &RecorderOptions, monitors: &[Monitor], chosen: Option<(u32, IRect, IRect)>) -> Result<(Screen, IRect), String> {
    let main = monitors.iter().find(|m| m.primary).or(monitors.first()).map(|m| m.rect).unwrap_or_default();
    if !r.screen {
        return Ok((Screen::Off, main));
    }
    match r.area {
        RecordArea::Region | RecordArea::Window | RecordArea::Fixed => {
            let (monitor, area, desktop) = chosen.ok_or("no area was chosen")?;
            if area.is_empty() {
                return Err("the area is empty".to_string());
            }
            Ok((Screen::Part { monitor, area }, desktop))
        }
        RecordArea::Screen => {
            let m = chosen_monitor(r.monitor.as_ref(), monitors).ok_or("no monitor was found")?;
            Ok((Screen::Monitor { monitor: m.id }, m.rect))
        }
        RecordArea::AllScreens => match monitors {
            [] => Err("no monitor was found".to_string()),
            [one] => Ok((Screen::Monitor { monitor: one.id }, one.rect)),
            _ => {
                let rects: Vec<IRect> = monitors.iter().map(|m| m.rect).collect();
                let (_, size) = all_screens_size(&rects).ok_or("the monitors cover nothing")?;
                Ok((Screen::All { monitors: monitors.iter().map(|m| (m.id, m.rect)).collect(), size }, main))
            }
        },
    }
}

/// Every monitor's picture put together in one picture of `size`, each where it is on `desktop`
/// (made smaller or larger to fit its place). What no monitor covers is black. None for an
/// empty or too large size.
pub fn compose_screens(parts: &[(IRect, &Raster)], desktop: IRect, size: (u32, u32)) -> Option<Raster> {
    let (w, h) = size;
    if w == 0 || h == 0 || desktop.is_empty() {
        return None;
    }
    let len = screengrabcraft_model::raster::checked_len(w, h).ok()?;
    let mut out = [0u8, 0, 0, 255].repeat(len / 4);
    let fx = f64::from(w) / f64::from(desktop.w.max(1));
    let fy = f64::from(h) / f64::from(desktop.h.max(1));
    let place = |v: f64, from: i32, f: f64, max: u32| ((v - f64::from(from)) * f).round().clamp(0.0, f64::from(max));
    let row_bytes = w as usize * 4;
    for (rect, pic) in parts {
        let x0 = place(f64::from(rect.x), desktop.x, fx, w);
        let y0 = place(f64::from(rect.y), desktop.y, fy, h);
        let x1 = place(rect.right() as f64, desktop.x, fx, w);
        let y1 = place(rect.bottom() as f64, desktop.y, fy, h);
        if x1 <= x0 || y1 <= y0 || pic.width() == 0 || pic.height() == 0 {
            continue;
        }
        let (tw, th) = ((x1 - x0) as u32, (y1 - y0) as u32);
        let resized;
        let src: &Raster = if (pic.width(), pic.height()) == (tw, th) {
            pic
        } else {
            let Some(r) = compose::scale(pic, tw, th) else { continue };
            resized = r;
            &resized
        };
        let (x0, y0) = (x0 as usize, y0 as usize);
        for (y, row) in src.pixels().chunks_exact(tw as usize * 4).enumerate() {
            let at = (y0 + y) * row_bytes + x0 * 4;
            if let Some(dst) = out.get_mut(at..at + row.len()) {
                dst.copy_from_slice(row);
            }
        }
    }
    Raster::new(w, h, out).ok()
}

/// What a video recording is to do.
pub struct VideoPlan {
    pub kind: Kind,
    pub screen: Screen,
    /// The part of the desktop the control window is placed by.
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
        stopped: None,
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

/// How long a camera may send nothing before the owner is told.
const CAMERA_SILENT: Duration = Duration::from_secs(5);

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
    let mut screen = ScreenSource::open(&plan.screen)?;
    if screen.is_none() && d.system.is_none() && d.mic.is_none() && d.camera.is_none() {
        let why = s.sources.lock().map(|v| v.join("; ")).unwrap_or_default();
        return Err(if why.is_empty() { "nothing was chosen to record".to_string() } else { format!("nothing could be recorded: {why}") });
    }
    let mut streams = Vec::with_capacity(4);
    if let Some(src) = &screen {
        let (width, height) = src.size;
        streams.push(StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width, height, channels: 0 });
    }
    if let Some(a) = &d.system {
        streams.push(StreamInfo { id: 1, kind: StreamKind::SystemSound, name: a.name.clone(), width: 0, height: 0, channels: a.channels });
        s.note("Computer sound".to_string());
    }
    if let Some(a) = &d.mic {
        streams.push(StreamInfo { id: 2, kind: StreamKind::Microphone, name: a.name.clone(), width: 0, height: 0, channels: a.channels });
        s.note(format!("Microphone: {}", a.name));
    }
    let mut first_camera = Vec::new();
    if let Some(c) = &d.camera {
        let (mut width, mut height) = (640, 480);
        if screen.is_none() {
            // The camera is the picture: the video is the size of the camera's pictures.
            first_camera = first_frames(c, s.stop, Duration::from_secs(3));
            if let Some(f) = first_camera.first() {
                (width, height) = (f.width.max(2), f.height.max(2));
            }
        }
        streams.push(StreamInfo { id: 3, kind: StreamKind::Camera, name: c.name.clone(), width, height, channels: 0 });
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
    for f in first_camera {
        let _ = tx.send(Item::Camera(f.at.as_micros() as u64, f.jpeg));
    }
    // Record Video: the MP4 is made as the recording goes, two seconds at a time.
    let live = match (&screen, plan.kind) {
        (Some(src), Kind::Video) => start_live(plan, src.size, &d),
        _ => None,
    };
    let feeding = AtomicBool::new(true);
    let (frames, live) = std::thread::scope(|scope| {
        let feeder = live.map(|(ex, cancel)| scope.spawn(|| feed_live(ex, cancel, &file_path, &feeding)));
        let frames = record_screen(plan, s, &d, &tx, &waiting, started, screen.as_mut());
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

/// The camera's first pictures, waited for up to `wait` (so its size is known before the
/// recording file is begun).
fn first_frames(c: &CameraCapture, stop: &AtomicBool, wait: Duration) -> Vec<camera::CameraFrame> {
    let until = Instant::now() + wait;
    loop {
        let got = c.take();
        if !got.is_empty() || Instant::now() >= until || stop.load(Ordering::SeqCst) {
            return got;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Where a recording's screen pictures come from, and their size.
struct ScreenSource {
    grab: Grab,
    size: (u32, u32),
    /// Pictures that couldn't be had, lately (so the log isn't flooded).
    failures: u32,
}

enum Grab {
    /// Part of a monitor, or all of one.
    One(screengrabcraft_capture::Grabber),
    /// Every monitor: each one's grabber, where it is on the desktop and its last picture, and
    /// the desktop they make up.
    All(Vec<(screengrabcraft_capture::Grabber, IRect, Option<Raster>)>, IRect),
}

impl ScreenSource {
    /// None when the screen isn't recorded.
    fn open(screen: &Screen) -> Result<Option<ScreenSource>, String> {
        let source = |grab, size| Some(ScreenSource { grab, size, failures: 0 });
        match screen {
            Screen::Off => Ok(None),
            Screen::Part { monitor, area } => {
                let g = screengrabcraft_capture::grabber(*monitor, *area).map_err(|e| e.to_string())?;
                Ok(source(Grab::One(g), (area.w, area.h)))
            }
            Screen::Monitor { monitor } => {
                let mut g = screengrabcraft_capture::monitor_grabber(*monitor).map_err(|e| e.to_string())?;
                // Its size is its picture's (a Retina display has more pixels than points).
                let first = g().map_err(|e| e.to_string())?;
                Ok(source(Grab::One(g), (first.width(), first.height())))
            }
            Screen::All { monitors, size } => {
                let rects: Vec<IRect> = monitors.iter().map(|(_, r)| *r).collect();
                let (desktop, _) = all_screens_size(&rects).ok_or("the monitors cover nothing")?;
                let mut parts = Vec::with_capacity(monitors.len());
                for (id, rect) in monitors {
                    match screengrabcraft_capture::monitor_grabber(*id) {
                        Ok(g) => parts.push((g, *rect, None)),
                        Err(e) => log::warn!("a monitor is left out of the recording: {e}"),
                    }
                }
                if parts.is_empty() {
                    return Err("no monitor could be recorded".to_string());
                }
                Ok(source(Grab::All(parts, desktop), *size))
            }
        }
    }

    /// The next picture, at its size; none when there's none this time.
    fn grab(&mut self) -> Option<Raster> {
        let size = self.size;
        let mut failed = Vec::new();
        let pic = match &mut self.grab {
            Grab::One(g) => match g() {
                Ok(pic) if (pic.width(), pic.height()) == size => Some(pic),
                // The monitor changed its resolution: the picture is fitted to the video.
                Ok(pic) => compose::scale(&pic, size.0, size.1),
                Err(e) => {
                    failed.push(e.to_string());
                    None
                }
            },
            Grab::All(parts, desktop) => {
                // A monitor that can't be had this time shows what it showed last.
                for (g, _, last) in parts.iter_mut() {
                    match g() {
                        Ok(pic) => *last = Some(pic),
                        Err(e) => failed.push(e.to_string()),
                    }
                }
                let pics: Vec<(IRect, &Raster)> = parts.iter().filter_map(|(_, r, last)| last.as_ref().map(|p| (*r, p))).collect();
                if pics.is_empty() { None } else { compose_screens(&pics, *desktop, size) }
            }
        };
        if let Some(e) = failed.first() {
            if self.failures.is_multiple_of(100) {
                log::warn!("a picture could not be captured: {e}");
            }
            self.failures = self.failures.saturating_add(1);
        } else {
            self.failures = 0;
        }
        pic
    }
}

/// The screen on a steady beat (if it's recorded), with the devices' sound and pictures passed on
/// as they come. The number of screen pictures taken, or why there were none.
fn record_screen(
    plan: &VideoPlan,
    s: &Shared,
    d: &Devices,
    tx: &std::sync::mpsc::Sender<Item>,
    waiting: &AtomicUsize,
    started: Instant,
    mut screen: Option<&mut ScreenSource>,
) -> Result<u32, String> {
    let tick = Duration::from_secs_f64(1.0 / f64::from(plan.fps.clamp(5, 30)));
    let limit = Duration::from_secs(u64::from(plan.max_secs.max(1)));
    let mut due = Instant::now();
    let mut frames = 0u32;
    // A camera that started but sends nothing is said so, once.
    let mut camera_pictures = 0u32;
    let mut camera_told = false;
    while !s.stop.load(Ordering::SeqCst) && started.elapsed() < limit {
        let at = started.elapsed();
        if let Some(pic) = screen.as_deref_mut().and_then(ScreenSource::grab) {
            if waiting.load(Ordering::SeqCst) < WAITING {
                waiting.fetch_add(1, Ordering::SeqCst);
                let _ = tx.send(Item::Screen(at.as_micros() as u64, pic));
                frames += 1;
                s.progress.store(frames, Ordering::SeqCst);
            } else {
                log::warn!("a video picture was left out: the disk is behind");
            }
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
                camera_pictures = camera_pictures.saturating_add(1);
                let _ = tx.send(Item::Camera(f.at.as_micros() as u64, f.jpeg));
            }
            if camera_pictures == 0 && !camera_told && at >= CAMERA_SILENT {
                camera_told = true;
                log::warn!("the camera has sent no pictures");
                s.note(format!("No camera pictures: {} has sent none (is another program using it?)", c.name));
            }
        }
        s.wake.request_repaint_after(Duration::from_millis(250));
        due = (due + tick).max(Instant::now());
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
    }
    if screen.is_some() && frames == 0 {
        return Err("nothing could be recorded".to_string());
    }
    Ok(frames)
}

/// The MP4 started as the recording starts (none if it can't be: it's then made after).
fn start_live(plan: &VideoPlan, size: (u32, u32), d: &Devices) -> Option<(Exporter, Arc<AtomicBool>)> {
    let settings = ExportSettings { max_width: plan.max_size.0, max_height: plan.max_size.1, fps: plan.fps, ..ExportSettings::default() };
    let (w, h) = compose::fit(size.0, size.1, settings.max_width, settings.max_height);
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
    use screengrabcraft_model::Color;

    use super::*;

    fn mon(id: u32, name: &str, rect: IRect, primary: bool) -> Monitor {
        Monitor { id, name: name.into(), rect, scale: 1.0, primary }
    }

    /// A laptop on the left of a main monitor, lower down, and a second one of the same make.
    fn desk() -> Vec<Monitor> {
        vec![
            mon(7, "DELL", IRect::new(0, 0, 2560, 1440), true),
            mon(8, "Laptop", IRect::new(-1920, 360, 1920, 1080), false),
            mon(9, "DELL", IRect::new(2560, 0, 2560, 1440), false),
        ]
    }

    #[test]
    fn each_area_comes_to_its_own_plan() {
        let m = desk();
        let r = |area| RecorderOptions { area, ..RecorderOptions::default() };
        let chosen = Some((8, IRect::new(10, 20, 300, 200), IRect::new(-1910, 380, 300, 200)));
        // Chosen on the overlay: that part of that monitor.
        for area in [RecordArea::Region, RecordArea::Window, RecordArea::Fixed] {
            assert_eq!(
                production_screen(&r(area), &m, chosen),
                Ok((Screen::Part { monitor: 8, area: IRect::new(10, 20, 300, 200) }, IRect::new(-1910, 380, 300, 200))),
                "{area:?}"
            );
            assert!(production_screen(&r(area), &m, None).is_err(), "{area:?} without an area");
        }
        assert!(production_screen(&r(RecordArea::Region), &m, Some((8, IRect::new(0, 0, 0, 5), IRect::default()))).is_err());
        // Full screen: the monitor remembered, else the main one.
        let main = production_screen(&r(RecordArea::Screen), &m, None);
        assert_eq!(main, Ok((Screen::Monitor { monitor: 7 }, IRect::new(0, 0, 2560, 1440))));
        let second = RecorderOptions { monitor: Some(MonitorChoice { name: "DELL".into(), x: 2560, y: 0 }), ..r(RecordArea::Screen) };
        assert_eq!(production_screen(&second, &m, None).map(|p| p.0), Ok(Screen::Monitor { monitor: 9 }));
        let gone = RecorderOptions { monitor: Some(MonitorChoice { name: "Projector".into(), x: 9000, y: 0 }), ..r(RecordArea::Screen) };
        assert_eq!(production_screen(&gone, &m, None).map(|p| p.0), Ok(Screen::Monitor { monitor: 7 }));
        // All screens: every monitor, the desktop made to fit 3840 × 2160; its control on the main one.
        let (screen, place) = production_screen(&r(RecordArea::AllScreens), &m, None).unwrap();
        let Screen::All { monitors, size } = screen else { panic!("{screen:?}") };
        assert_eq!(monitors.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![7, 8, 9]);
        // 7040 × 1440 made 3840 × 784 (even sides).
        assert_eq!(size, (3840, 784));
        assert_eq!(place, IRect::new(0, 0, 2560, 1440));
        // One monitor: all screens is that one, whole.
        let one = vec![mon(3, "Only", IRect::new(0, 0, 1366, 768), false)];
        assert_eq!(production_screen(&r(RecordArea::AllScreens), &one, None).map(|p| p.0), Ok(Screen::Monitor { monitor: 3 }));
        assert!(production_screen(&r(RecordArea::AllScreens), &[], None).is_err());
        assert!(production_screen(&r(RecordArea::Screen), &[], None).is_err());
        // The screen off: no screen, whatever the area; the control on the main monitor.
        for area in RecordArea::ALL {
            let off = RecorderOptions { screen: false, ..r(area) };
            assert_eq!(production_screen(&off, &m, None), Ok((Screen::Off, IRect::new(0, 0, 2560, 1440))), "{area:?}");
        }
        assert_eq!(production_screen(&RecorderOptions { screen: false, ..RecorderOptions::default() }, &[], None).map(|p| p.0), Ok(Screen::Off));
    }

    #[test]
    fn all_screens_are_put_together_where_they_are() {
        let left = Raster::filled(4, 2, Color::RED).unwrap();
        let main = Raster::filled(4, 4, Color::WHITE).unwrap();
        let (l, m) = (IRect::new(-4, 2, 4, 2), IRect::new(0, 0, 4, 4));
        let (desktop, size) = all_screens_size(&[l, m]).unwrap();
        assert_eq!((desktop, size), (IRect::new(-4, 0, 8, 4), (16, 16)), "never smaller than 16 a side");
        let out = compose_screens(&[(l, &left), (m, &main)], desktop, (8, 4)).unwrap();
        assert_eq!(out.pixel(0, 2), Some(Color::RED));
        assert_eq!(out.pixel(7, 0), Some(Color::WHITE));
        assert_eq!(out.pixel(0, 0), Some(Color::BLACK), "no monitor there");
        // Made smaller: half the size, each monitor in its half.
        let out = compose_screens(&[(l, &left), (m, &main)], desktop, (4, 2)).unwrap();
        assert_eq!(out.pixel(0, 1), Some(Color::RED));
        assert_eq!(out.pixel(3, 0), Some(Color::WHITE));
        // A Retina monitor's picture (twice its points) is fitted to its place.
        let retina = Raster::filled(8, 8, Color::WHITE).unwrap();
        let out = compose_screens(&[(m, &retina)], m, (4, 4)).unwrap();
        assert!(out.pixels().as_chunks::<4>().0.iter().all(|p| *p == [255, 255, 255, 255]));
        // Nothing to make, or nonsense: none, never a panic.
        assert!(compose_screens(&[(m, &main)], m, (0, 4)).is_none());
        assert!(compose_screens(&[(m, &main)], IRect::new(0, 0, 0, 0), (4, 4)).is_none());
        assert!(compose_screens(&[(m, &main)], m, (70_000, 70_000)).is_none());
        let far = compose_screens(&[(IRect::new(9_000, 9_000, 4, 4), &main), (IRect::new(i32::MIN, 0, u32::MAX, 4), &left)], m, (4, 4));
        assert!(far.is_some());
        assert!(all_screens_size(&[]).is_none());
        let big = all_screens_size(&[IRect::new(0, 0, 3840, 2160), IRect::new(3840, 0, 3840, 2160)]).unwrap().1;
        assert!(big.0 <= ALL_SCREENS_MAX.0 && big.1 <= ALL_SCREENS_MAX.1, "{big:?}");
    }

    #[test]
    fn stopping_stops_the_clock_at_once() {
        let off = VideoOptions { system_sound: false, microphone: false, microphone_name: None, camera: false, camera_index: None };
        let plan = VideoPlan {
            kind: Kind::Production,
            screen: Screen::Off,
            desktop: IRect::new(0, 0, 10, 10),
            fps: 30,
            max_secs: 60,
            options: off,
            out: std::env::temp_dir().join(format!("sgc-nothing-{}.sgvideo", std::process::id())),
            max_size: (1920, 1080),
            created: String::new(),
        };
        let mut rec = start_video(plan, egui::Context::default()).unwrap();
        assert!(!rec.stopping());
        rec.stop();
        let at = rec.elapsed();
        std::thread::sleep(Duration::from_millis(30));
        assert!(rec.stopping());
        assert_eq!(rec.elapsed(), at, "the time shown stays where Stop was pressed");
        // Nothing was chosen, so it ends with that.
        let result = loop {
            if let Some(r) = rec.take() {
                break r;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(result.is_err());
    }

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
            screen: Screen::Part { monitor: monitor.id, area },
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
