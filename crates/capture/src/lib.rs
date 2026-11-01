//! Taking pictures of the screen: the monitors, the windows on them, and their pixels.
//!
//! One API, a backend per system:
//!
//! - **Windows and macOS** (`xcap_backend`): through `xcap` (GDI and DWM on Windows; Core
//!   Graphics on macOS, which needs the owner's Screen Recording permission, see
//!   [`permission`]).
//! - **Linux and the BSDs** (`unix`): an X11 session through `x11rb` (the protocol spoken in
//!   Rust, no C library); a Wayland session through the desktop's screenshot portal
//!   (`org.freedesktop.portal.Screenshot` over D-Bus), where the compositor decides what a
//!   program may see and windows can't be listed.
//!
//! Desktop coordinates are the system's own: physical pixels on Windows and X11 (where they may
//! start at negative coordinates, for a monitor left of the main one), points on macOS. A
//! monitor's picture is in its own pixels, which on a Retina display are twice as many a side as
//! its rectangle; [`Monitor::to_pixels`] maps between the two, and a capture of an area is cut
//! from one monitor's picture in that monitor's pixels, so nothing is lost to scaling.
//!
//! Layer L3: no UI toolkit. Not part of the browser build.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use screengrabcraft_model::{IRect, Raster};

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod xcap_backend;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use xcap_backend as backend;

#[cfg(all(unix, not(target_os = "macos")))]
mod unix;
#[cfg(all(unix, not(target_os = "macos")))]
use unix as backend;

#[cfg(not(any(target_os = "windows", unix)))]
mod none;
#[cfg(not(any(target_os = "windows", unix)))]
use none as backend;

pub mod compose;
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
mod decode;

/// One monitor.
#[derive(Clone, Debug, PartialEq)]
pub struct Monitor {
    /// The system's own id for it (stable while the monitor stays connected).
    pub id: u32,
    pub name: String,
    /// Where its pixels are on the desktop.
    pub rect: IRect,
    /// Its scale factor (1.0 at 96 dpi on Windows, 2.0 on a Retina display).
    pub scale: f32,
    pub primary: bool,
}

impl Monitor {
    /// A desktop rectangle in the pixels of this monitor's picture (`pic_w`x`pic_h`), clipped to
    /// it; none when it misses the monitor.
    pub fn to_pixels(&self, area: IRect, pic_w: u32, pic_h: u32) -> Option<IRect> {
        let part = self.rect.intersect(area)?;
        let sx = f64::from(pic_w) / f64::from(self.rect.w.max(1));
        let sy = f64::from(pic_h) / f64::from(self.rect.h.max(1));
        let x0 = ((f64::from(part.x) - f64::from(self.rect.x)) * sx).round();
        let y0 = ((f64::from(part.y) - f64::from(self.rect.y)) * sy).round();
        let x1 = ((part.right() as f64 - f64::from(self.rect.x)) * sx).round().min(f64::from(pic_w));
        let y1 = ((part.bottom() as f64 - f64::from(self.rect.y)) * sy).round().min(f64::from(pic_h));
        (x1 > x0 && y1 > y0).then(|| IRect::new(x0 as i32, y0 as i32, (x1 - x0) as u32, (y1 - y0) as u32))
    }
}

/// A window on the desktop, as the overlay offers it for a window capture.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowInfo {
    pub id: u64,
    pub title: String,
    pub app: String,
    /// Its visible frame on the desktop (without the invisible borders some systems add).
    pub rect: IRect,
}

/// The whole desktop at one moment: every monitor's picture and the windows on them, front to
/// back. The overlay shows this frozen picture while the owner chooses what to keep.
#[derive(Clone, Debug)]
pub struct Frozen {
    pub monitors: Vec<(Monitor, Raster)>,
    /// Front first. Empty where windows can't be listed (Wayland).
    pub windows: Vec<WindowInfo>,
    /// Where the pointer was, if the system says.
    pub pointer: Option<(i32, i32)>,
}

impl Frozen {
    /// The part of the desktop inside `area` (desktop pixels), from the monitors' pictures;
    /// what no monitor covers is transparent.
    pub fn crop(&self, area: IRect) -> Option<Raster> {
        compose::compose(&self.monitors.iter().map(|(m, r)| (m.rect, r)).collect::<Vec<_>>(), area)
    }

    /// The rectangle holding every monitor.
    pub fn desktop(&self) -> IRect {
        self.monitors.iter().fold(IRect::default(), |acc, (m, _)| acc.union(m.rect))
    }

    /// The monitor the pointer is on, else the primary one, else the first.
    pub fn current_monitor(&self) -> Option<&(Monitor, Raster)> {
        let under = self.pointer.and_then(|(x, y)| self.monitors.iter().find(|(m, _)| m.rect.contains(i64::from(x), i64::from(y))));
        under.or_else(|| self.monitors.iter().find(|(m, _)| m.primary)).or_else(|| self.monitors.first())
    }

    /// The frontmost window under a desktop point.
    pub fn window_at(&self, x: i64, y: i64) -> Option<&WindowInfo> {
        self.windows.iter().find(|w| w.rect.contains(x, y))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("no monitor was found")]
    NoMonitor,
    #[error("the screen could not be captured: {0}")]
    Failed(String),
    #[error(
        "ScreenGrabCraft is not allowed to record the screen. Allow it in System Settings › Privacy & Security › Screen Recording, then capture again."
    )]
    NotPermitted,
    #[error("the screenshot was cancelled")]
    Cancelled,
    #[error("this system has no way for ScreenGrabCraft to capture the screen")]
    Unsupported,
    #[error(transparent)]
    Picture(#[from] screengrabcraft_model::RasterError),
}

/// Whether the system lets this program see the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Granted,
    /// The owner has to allow it (macOS Screen Recording).
    Denied,
    /// The system asks each time, or can't say (a Wayland portal).
    Ask,
}

/// Which way this build captures, for the About box and logs.
pub fn backend_name() -> String {
    backend::name()
}

/// The monitors, the primary one first.
pub fn monitors() -> Result<Vec<Monitor>, CaptureError> {
    let mut all = backend::monitors()?;
    if all.is_empty() {
        return Err(CaptureError::NoMonitor);
    }
    all.sort_by_key(|m| (!m.primary, m.rect.x, m.rect.y));
    Ok(all)
}

/// The visible windows, front first. Empty where the system won't list them.
pub fn windows() -> Vec<WindowInfo> {
    backend::windows().unwrap_or_else(|e| {
        log::warn!("listing windows failed: {e}");
        Vec::new()
    })
}

/// Every monitor's picture, the windows on them and the pointer, at one moment.
pub fn freeze() -> Result<Frozen, CaptureError> {
    let started = std::time::Instant::now();
    let frozen = backend::freeze()?;
    if frozen.monitors.is_empty() {
        return Err(CaptureError::NoMonitor);
    }
    log::info!(
        "froze {} monitor(s) and {} window(s) in {} ms ({})",
        frozen.monitors.len(),
        frozen.windows.len(),
        started.elapsed().as_millis(),
        backend::name()
    );
    Ok(frozen)
}

/// A picture of one monitor, now.
pub fn grab_monitor(id: u32) -> Result<(Monitor, Raster), CaptureError> {
    backend::grab_monitor(id)
}

/// A picture of part of one monitor, now, cut in that monitor's own pixels (for Repeat Last
/// Capture, scrolling captures and GIF frames).
pub fn grab_on_monitor(id: u32, pixels: IRect) -> Result<Raster, CaptureError> {
    let (_, pic) = backend::grab_monitor(id)?;
    let (x, y) = (u32::try_from(pixels.x.max(0)).unwrap_or(0), u32::try_from(pixels.y.max(0)).unwrap_or(0));
    pic.crop(x, y, pixels.w, pixels.h).ok_or(CaptureError::Failed("the area is not on that monitor any more".to_string()))
}

/// The same part of one monitor, grabbed anew at each call: the frames of a recording.
pub type Grabber = Box<dyn FnMut() -> Result<Raster, CaptureError>>;

/// A grabber for part of one monitor, in that monitor's own pixels. On Windows a frame costs only
/// the area; elsewhere it's the whole monitor, cut down.
pub fn grabber(id: u32, pixels: IRect) -> Result<Grabber, CaptureError> {
    #[cfg(target_os = "windows")]
    return backend::grabber(id, pixels);
    #[cfg(not(target_os = "windows"))]
    Ok(Box::new(move || grab_on_monitor(id, pixels)))
}

/// A picture of an area of the desktop, now, put together from the monitors it lies on (in
/// desktop units: a Retina monitor's part is reduced to its points).
pub fn grab_area(area: IRect) -> Result<Raster, CaptureError> {
    let mut parts = Vec::new();
    for m in monitors()? {
        if m.rect.intersect(area).is_some() {
            parts.push(backend::grab_monitor(m.id)?);
        }
    }
    let refs: Vec<(IRect, &Raster)> = parts.iter().map(|(m, p)| (m.rect, p)).collect();
    compose::compose(&refs, area).ok_or(CaptureError::NoMonitor)
}

/// Where the pointer is on the desktop, if the system says.
pub fn pointer() -> Option<(i32, i32)> {
    backend::pointer()
}

/// Whether this program may see the screen.
pub fn permission() -> Permission {
    backend::permission()
}

/// Asks the system for permission to see the screen (macOS shows its dialog once); true when
/// it is granted now.
pub fn request_permission() -> bool {
    backend::request_permission()
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Color;

    fn mon(id: u32, rect: IRect, primary: bool) -> Monitor {
        Monitor { id, name: format!("m{id}"), rect, scale: 1.0, primary }
    }

    #[test]
    fn desktop_areas_map_to_monitor_pixels() {
        let retina = Monitor { id: 1, name: "r".into(), rect: IRect::new(0, 0, 1440, 900), scale: 2.0, primary: true };
        assert_eq!(retina.to_pixels(IRect::new(100, 50, 200, 100), 2880, 1800), Some(IRect::new(200, 100, 400, 200)));
        let left = Monitor { id: 2, name: "l".into(), rect: IRect::new(-1920, 0, 1920, 1080), scale: 1.0, primary: false };
        assert_eq!(left.to_pixels(IRect::new(-100, -10, 300, 20), 1920, 1080), Some(IRect::new(1820, 0, 100, 10)));
        assert_eq!(left.to_pixels(IRect::new(10, 10, 5, 5), 1920, 1080), None);
    }

    #[test]
    fn frozen_desktop_lookups() {
        let left = IRect::new(-100, 0, 100, 50);
        let main = IRect::new(0, 0, 200, 100);
        let frozen = Frozen {
            monitors: vec![
                (mon(1, main, true), Raster::filled(200, 100, Color::WHITE).unwrap()),
                (mon(2, left, false), Raster::filled(100, 50, Color::BLACK).unwrap()),
            ],
            windows: vec![
                WindowInfo { id: 1, title: "front".into(), app: "a".into(), rect: IRect::new(10, 10, 50, 50) },
                WindowInfo { id: 2, title: "back".into(), app: "b".into(), rect: IRect::new(0, 0, 150, 90) },
            ],
            pointer: Some((-50, 10)),
        };
        assert_eq!(frozen.desktop(), IRect::new(-100, 0, 300, 100));
        assert_eq!(frozen.current_monitor().map(|(m, _)| m.id), Some(2));
        assert_eq!(frozen.window_at(20, 20).map(|w| w.id), Some(1));
        assert_eq!(frozen.window_at(100, 20).map(|w| w.id), Some(2));
        assert!(frozen.window_at(190, 95).is_none());
        // A crop across both monitors takes from each.
        let both = frozen.crop(IRect::new(-10, 0, 20, 10)).unwrap();
        assert_eq!(both.pixel(0, 0), Some(Color::BLACK));
        assert_eq!(both.pixel(15, 0), Some(Color::WHITE));
        let nowhere = Frozen { pointer: None, ..frozen };
        assert_eq!(nowhere.current_monitor().map(|(m, _)| m.id), Some(1));
    }
}
