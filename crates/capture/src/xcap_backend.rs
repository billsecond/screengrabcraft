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
    let mut monitors = Vec::new();
    for m in xcap::Monitor::all().map_err(failed)? {
        let info = monitor_of(&m)?;
        match m.capture_image() {
            Ok(img) => monitors.push((info, raster_of(img)?)),
            Err(e) => log::warn!("monitor {} could not be captured: {e}", info.name),
        }
    }
    if monitors.is_empty() {
        return Err(CaptureError::NoMonitor);
    }
    Ok(Frozen { monitors, windows: windows().unwrap_or_default(), pointer })
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
