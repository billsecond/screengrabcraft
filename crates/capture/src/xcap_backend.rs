//! Windows and macOS: through `xcap`.
//!
//! On Windows the process is per-monitor DPI aware (winit makes it so before any window opens,
//! and the command-line program asks for it itself), so `xcap` reports physical pixels.
//! On macOS a program sees only its own windows and the desktop picture until the owner allows
//! Screen Recording for it; `permission` asks Core Graphics, and a capture without it is refused
//! rather than handed over blank.

use screengrabcraft_model::{IRect, Raster};

use crate::{CaptureError, Frozen, Monitor, Permission, WindowInfo};

pub fn name() -> String {
    if cfg!(target_os = "macos") { "macOS Core Graphics (xcap)".to_string() } else { "Windows GDI (xcap)".to_string() }
}

fn failed(e: impl std::fmt::Display) -> CaptureError {
    CaptureError::Failed(e.to_string())
}

fn monitor_of(m: &xcap::Monitor) -> Result<Monitor, CaptureError> {
    let (x, y) = (m.x().map_err(failed)?, m.y().map_err(failed)?);
    let (w, h) = (m.width().map_err(failed)?, m.height().map_err(failed)?);
    // On macOS the rectangle is in points and the picture in pixels (twice as many a side on a
    // Retina display); desktop coordinates stay in points there.
    let scale = m.scale_factor().unwrap_or(1.0);
    Ok(Monitor {
        id: m.id().unwrap_or(0),
        name: m.friendly_name().or_else(|_| m.name()).unwrap_or_default(),
        rect: IRect::new(x, y, w, h),
        scale,
        primary: m.is_primary().unwrap_or(false),
    })
}

fn raster_of(img: image::RgbaImage) -> Result<Raster, CaptureError> {
    let (w, h) = img.dimensions();
    Ok(Raster::new(w, h, img.into_raw())?)
}

pub fn monitors() -> Result<Vec<Monitor>, CaptureError> {
    xcap::Monitor::all().map_err(failed)?.iter().map(monitor_of).collect()
}

pub fn windows() -> Result<Vec<WindowInfo>, CaptureError> {
    let own = std::process::id();
    let mut all: Vec<(i32, WindowInfo)> = Vec::new();
    for w in xcap::Window::all().map_err(failed)? {
        if w.is_minimized().unwrap_or(false) || w.pid().ok() == Some(own) {
            continue;
        }
        let (Ok(x), Ok(y), Ok(width), Ok(height)) = (w.x(), w.y(), w.width(), w.height()) else { continue };
        if width < 8 || height < 8 {
            continue;
        }
        let title = w.title().unwrap_or_default();
        let app = w.app_name().unwrap_or_default();
        // Menu bars, docks and other furniture have no title on macOS.
        if cfg!(target_os = "macos") && title.is_empty() {
            continue;
        }
        let rect = IRect::new(x, y, width, height);
        all.push((w.z().unwrap_or(0), WindowInfo { id: u64::from(w.id().unwrap_or(0)), title, app, rect }));
    }
    // Front first: xcap's z is larger for windows nearer the front.
    all.sort_by_key(|(z, _)| std::cmp::Reverse(*z));
    Ok(all.into_iter().map(|(_, w)| w).collect())
}

pub fn freeze() -> Result<Frozen, CaptureError> {
    if permission() == Permission::Denied {
        return Err(CaptureError::NotPermitted);
    }
    let pointer = pointer();
    let count = xcap::Monitor::all().map_err(failed)?.len();
    // Every monitor at once, each on a thread of its own, and the windows on another: the freeze
    // takes as long as the slowest of them rather than all of them, and the pictures are of the
    // same moment. (A monitor can't be handed to another thread, so each looks up its own.)
    let (grabs, windows) = std::thread::scope(|s| {
        let threads: Vec<_> =
            (0..count).map(|i| std::thread::Builder::new().name(format!("freeze-{i}")).spawn_scoped(s, move || grab_nth(i))).collect();
        let listing = std::thread::Builder::new().name("freeze-windows".into()).spawn_scoped(s, windows);
        let grabs: Vec<_> = threads
            .into_iter()
            .enumerate()
            .map(|(i, t)| match t {
                Ok(t) => t.join().unwrap_or_else(|_| Err(CaptureError::Failed(format!("capturing monitor {i} failed")))),
                // No thread to spare: this one does it.
                Err(_) => grab_nth(i),
            })
            .collect();
        let windows = match listing {
            Ok(t) => t.join().unwrap_or_else(|_| Ok(Vec::new())),
            Err(_) => windows(),
        };
        (grabs, windows)
    });
    let mut monitors = Vec::new();
    for g in grabs {
        match g {
            Ok(m) => monitors.push(m),
            Err(e) => log::warn!("a monitor could not be captured: {e}"),
        }
    }
    if monitors.is_empty() {
        return Err(CaptureError::NoMonitor);
    }
    Ok(Frozen { monitors, windows: windows.unwrap_or_default(), pointer })
}

/// The `i`th monitor and a picture of it.
fn grab_nth(i: usize) -> Result<(Monitor, Raster), CaptureError> {
    let m = xcap::Monitor::all().map_err(failed)?.into_iter().nth(i).ok_or(CaptureError::NoMonitor)?;
    let info = monitor_of(&m)?;
    let img = m.capture_image().map_err(|e| CaptureError::Failed(format!("monitor {}: {e}", info.name)))?;
    Ok((info, raster_of(img)?))
}

pub fn grab_monitor(id: u32) -> Result<(Monitor, Raster), CaptureError> {
    if permission() == Permission::Denied {
        return Err(CaptureError::NotPermitted);
    }
    for m in xcap::Monitor::all().map_err(failed)? {
        let info = monitor_of(&m)?;
        if info.id == id {
            return Ok((info, raster_of(m.capture_image().map_err(failed)?)?));
        }
    }
    Err(CaptureError::NoMonitor)
}

/// Keeps the monitor, so a frame neither lists the monitors nor copies more than the area.
#[cfg(target_os = "windows")]
pub fn grabber(id: u32, pixels: IRect) -> Result<crate::Grabber, CaptureError> {
    let monitor = xcap::Monitor::all().map_err(failed)?.into_iter().find(|m| m.id().ok() == Some(id)).ok_or(CaptureError::NoMonitor)?;
    let (x, y) = (u32::try_from(pixels.x.max(0)).unwrap_or(0), u32::try_from(pixels.y.max(0)).unwrap_or(0));
    Ok(Box::new(move || raster_of(monitor.capture_region(x, y, pixels.w, pixels.h).map_err(failed)?)))
}

/// Keeps the monitor, so a frame doesn't list the monitors first.
#[cfg(target_os = "windows")]
pub fn monitor_grabber(id: u32) -> Result<crate::Grabber, CaptureError> {
    let monitor = xcap::Monitor::all().map_err(failed)?.into_iter().find(|m| m.id().ok() == Some(id)).ok_or(CaptureError::NoMonitor)?;
    Ok(Box::new(move || raster_of(monitor.capture_image().map_err(failed)?)))
}

pub fn pointer() -> Option<(i32, i32)> {
    match mouse_position::mouse_position::Mouse::get_mouse_position() {
        mouse_position::mouse_position::Mouse::Position { x, y } => Some((x, y)),
        mouse_position::mouse_position::Mouse::Error => None,
    }
}

#[cfg(target_os = "macos")]
pub fn permission() -> Permission {
    if objc2_core_graphics::CGPreflightScreenCaptureAccess() { Permission::Granted } else { Permission::Denied }
}

#[cfg(target_os = "macos")]
pub fn request_permission() -> bool {
    objc2_core_graphics::CGRequestScreenCaptureAccess()
}

#[cfg(not(target_os = "macos"))]
pub fn permission() -> Permission {
    Permission::Granted
}

#[cfg(not(target_os = "macos"))]
pub fn request_permission() -> bool {
    true
}
