//! Linux and the BSDs.
//!
//! - **X11** (`x11`): the X server hands out its pixels (`GetImage` on the root window), lists
//!   its monitors (RandR) and, through the window manager's EWMH properties, its windows.
//! - **Wayland** (`portal`, `wayland`): the compositor gives a program no pixels of its own
//!   accord. The desktop's screenshot portal takes a picture of the whole desktop (asking the
//!   owner the first time, on some desktops); the outputs, listed over the Wayland connection,
//!   say how to cut it into monitors. Windows can't be listed and the pointer isn't told.
//!
//! The session is worked out from the environment on every call (a program started from an X
//! terminal inside a Wayland session still sees `WAYLAND_DISPLAY`), and logged when it changes.

mod portal;
mod wayland;
mod x11;

use std::sync::atomic::{AtomicU8, Ordering};

use screengrabcraft_model::{IRect, Raster};

use crate::{CaptureError, Frozen, Monitor, Permission, WindowInfo, decode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Session {
    X11,
    Wayland,
    /// Neither: a console, a service, a test runner.
    Headless,
}

/// The session last logged (0 for none yet).
static LOGGED: AtomicU8 = AtomicU8::new(0);

fn session() -> Session {
    let set = |key: &str| std::env::var_os(key).is_some_and(|v| !v.is_empty());
    let wayland = std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t.eq_ignore_ascii_case("wayland")) || set("WAYLAND_DISPLAY");
    let session = if wayland {
        Session::Wayland
    } else if set("DISPLAY") {
        Session::X11
    } else {
        Session::Headless
    };
    let code = match session {
        Session::X11 => 1,
        Session::Wayland => 2,
        Session::Headless => 3,
    };
    if LOGGED.swap(code, Ordering::Relaxed) != code {
        match session {
            Session::X11 => log::info!("capture: an X11 session, through the X server (DISPLAY)"),
            Session::Wayland => log::info!("capture: a Wayland session, through the desktop's screenshot portal"),
            Session::Headless => log::info!("capture: neither an X11 nor a Wayland session (no DISPLAY or WAYLAND_DISPLAY)"),
        }
    }
    session
}

pub fn name() -> String {
    match session() {
        Session::X11 => "X11 (x11rb)".to_string(),
        Session::Wayland => "Wayland screenshot portal (xdg-desktop-portal)".to_string(),
        Session::Headless => "none (no X11 or Wayland display)".to_string(),
    }
}

pub fn monitors() -> Result<Vec<Monitor>, CaptureError> {
    match session() {
        Session::X11 => x11::monitors(),
        Session::Wayland => match wayland::outputs() {
            Ok(outputs) if !outputs.is_empty() => Ok(outputs),
            Ok(_) => whole_picture_monitor(),
            Err(e) => {
                log::warn!("the Wayland outputs could not be listed ({e}); asking the portal for the desktop's size");
                whole_picture_monitor()
            }
        },
        Session::Headless => Err(CaptureError::Unsupported),
    }
}

pub fn windows() -> Result<Vec<WindowInfo>, CaptureError> {
    match session() {
        Session::X11 => x11::windows(),
        Session::Wayland | Session::Headless => Ok(Vec::new()),
    }
}

pub fn freeze() -> Result<Frozen, CaptureError> {
    match session() {
        Session::X11 => x11::freeze(),
        Session::Wayland => Ok(Frozen { monitors: portal_monitors()?, windows: Vec::new(), pointer: None }),
        Session::Headless => Err(CaptureError::Unsupported),
    }
}

pub fn grab_monitor(id: u32) -> Result<(Monitor, Raster), CaptureError> {
    match session() {
        Session::X11 => x11::grab_monitor(id),
        Session::Wayland => portal_monitors()?.into_iter().find(|(m, _)| m.id == id).ok_or(CaptureError::NoMonitor),
        Session::Headless => Err(CaptureError::Unsupported),
    }
}

pub fn pointer() -> Option<(i32, i32)> {
    match session() {
        Session::X11 => x11::pointer(),
        Session::Wayland | Session::Headless => None,
    }
}

pub fn permission() -> Permission {
    match session() {
        Session::X11 => Permission::Granted,
        // The portal decides at each capture (and may ask the owner the first time).
        Session::Wayland => Permission::Ask,
        Session::Headless => Permission::Denied,
    }
}

/// Nothing can be asked ahead on Wayland: the portal asks, when it asks, at the capture itself.
pub fn request_permission() -> bool {
    !matches!(session(), Session::Headless)
}

/// The id of the one monitor made of the portal's whole picture, when the outputs can't be
/// listed (Wayland output ids are the compositor's global names, which start at 1).
const WHOLE_PICTURE: u32 = 0;

fn whole_monitor(pic: &Raster) -> Monitor {
    Monitor { id: WHOLE_PICTURE, name: "Desktop".to_string(), rect: IRect::new(0, 0, pic.width(), pic.height()), scale: 1.0, primary: true }
}

fn whole_picture_monitor() -> Result<Vec<Monitor>, CaptureError> {
    Ok(vec![whole_monitor(&portal::screenshot()?)])
}

/// One portal screenshot, cut into the outputs.
fn portal_monitors() -> Result<Vec<(Monitor, Raster)>, CaptureError> {
    let pic = portal::screenshot()?;
    let outputs = wayland::outputs().unwrap_or_else(|e| {
        log::warn!("the Wayland outputs could not be listed ({e}); the screenshot stays one picture");
        Vec::new()
    });
    Ok(split(pic, outputs))
}

/// The portal's picture of the whole desktop cut into the outputs it shows, or kept whole as one
/// monitor when it doesn't match them.
fn split(pic: Raster, outputs: Vec<Monitor>) -> Vec<(Monitor, Raster)> {
    let rects: Vec<IRect> = outputs.iter().map(|m| m.rect).collect();
    let whole = IRect::new(0, 0, pic.width(), pic.height());
    match decode::split_desktop(pic.width(), pic.height(), &rects) {
        Some(parts) => {
            let mut out = Vec::new();
            for (m, part) in outputs.into_iter().zip(parts) {
                let Some(p) = part else {
                    log::warn!("output {} is not in the portal's picture", m.name);
                    continue;
                };
                let cut =
                    if p == whole { Some(pic.clone()) } else { pic.crop(u32::try_from(p.x).unwrap_or(0), u32::try_from(p.y).unwrap_or(0), p.w, p.h) };
                if let Some(cut) = cut {
                    out.push((m, cut));
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
        None if !outputs.is_empty() => {
            log::warn!("the portal's picture ({}×{}) doesn't match the outputs ({:?}); keeping it whole", pic.width(), pic.height(), rects)
        }
        None => {}
    }
    vec![(whole_monitor(&pic), pic)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Color;

    fn output(id: u32, rect: IRect) -> Monitor {
        Monitor { id, name: format!("o{id}"), rect, scale: 2.0, primary: id == 1 }
    }

    #[test]
    fn the_portal_picture_is_cut_into_outputs_or_kept_whole() {
        // Two outputs of 100×50 logical pixels at scale 2: a picture of 400×100.
        let mut pic = Raster::filled(400, 100, Color::WHITE).unwrap();
        pic.pixels_mut()[250 * 4 + 1] = 0;
        let parts = split(pic.clone(), vec![output(1, IRect::new(0, 0, 100, 50)), output(7, IRect::new(100, 0, 100, 50))]);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].0.id, 7);
        assert_eq!((parts[1].1.width(), parts[1].1.height()), (200, 100));
        assert_eq!(parts[1].1.pixel(50, 0), Some(Color::rgb(255, 0, 255)));
        assert_eq!(parts[1].0.to_pixels(IRect::new(125, 0, 10, 10), 200, 100), Some(IRect::new(50, 0, 20, 20)));
        // One output that is the whole picture keeps the same pixels.
        let one = split(pic.clone(), vec![output(1, IRect::new(0, 0, 200, 50))]);
        assert!(one[0].1.same_pixels(&pic));
        // Outputs the picture doesn't match, or none: one monitor of the whole picture.
        for outputs in [vec![output(1, IRect::new(0, 0, 100, 100))], vec![]] {
            let whole = split(pic.clone(), outputs);
            assert_eq!(whole.len(), 1);
            assert_eq!((whole[0].0.id, whole[0].0.rect), (WHOLE_PICTURE, IRect::new(0, 0, 400, 100)));
        }
    }
}
