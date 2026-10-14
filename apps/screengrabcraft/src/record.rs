//! Captures that run for a while: an area recorded as an animated GIF, and an area captured as
//! its content scrolls and joined into one long picture. Each runs on a thread of its own and
//! is stopped from the tray, its small control window or its shortcut.

use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use screengrabcraft_model::{IRect, Raster};
use screengrabcraft_render::anim::GifWriter;
use screengrabcraft_render::stitch::{Step, Stitcher};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Gif,
    Scrolling,
}

/// What a finished recording made.
pub enum Output {
    /// A GIF written to this file (in the temporary folder), with this many frames.
    Gif(PathBuf, u32),
    /// The joined scrolling picture.
    Picture(Raster),
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
            };
            wake.request_repaint();
            out
        })
        .map_err(|e| e.to_string())?;
    Ok(Recording { kind, started: Instant::now(), desktop, stop, cancel, progress, lost, handle: Some(handle) })
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
    let tick = Duration::from_millis(1000 / u64::from(fps.clamp(1, 30)));
    let started = Instant::now();
    let mut last = Instant::now();
    let mut frames = 0u32;
    let limit = Duration::from_secs(u64::from(max_secs.clamp(1, 600)));
    while !stop.load(Ordering::SeqCst) && started.elapsed() < limit {
        let at = Instant::now();
        match screengrabcraft_capture::grab_on_monitor(monitor, area) {
            Ok(frame) if frame.width() == area.w && frame.height() == area.h => {
                let delay = at.duration_since(last).as_millis().min(u128::from(u32::MAX)) as u32;
                writer.push(frame, if frames == 0 { 0 } else { delay }).map_err(|e| e.to_string())?;
                last = at;
                frames += 1;
                progress.store(frames, Ordering::SeqCst);
                wake.request_repaint();
            }
            Ok(_) => log::warn!("a frame of the wrong size was left out"),
            Err(e) => log::warn!("a frame could not be captured: {e}"),
        }
        if let Some(rest) = tick.checked_sub(at.elapsed()) {
            std::thread::sleep(rest);
        }
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
