//! Windows shown and hidden at once, without the animations Windows plays.
//!
//! Windows animates a window that is shown, hidden, closed, minimized or maximized (the desktop
//! window manager's "transitions"): the capture overlay faded in and out, and the editor faded
//! away for a capture. [`calm_windows`] turns that off for the calling thread's windows, and
//! [`cloak`] makes a window invisible, or visible again, at once: it stays where it is, maximized
//! or not, and is still drawn, so it comes back as it was. The editor steps aside for a capture
//! that way, and the capture service's anchor window is kept out of sight that way wherever
//! Windows moves it (Windows brings windows back onto a monitor when the monitors change).
//!
//! Elsewhere these do nothing.

use raw_window_handle::HasWindowHandle;

/// Turns off Windows' animations for every window of the calling thread that doesn't have them
/// off yet. Call it on the thread that made the windows (eframe's), once a window is made and
/// before it is shown.
pub fn calm_windows() {
    imp::calm_windows();
}

/// Cloaks the window (`true`: it isn't seen, although it's there and drawn) or uncloaks it, at
/// once. False where that couldn't be done (not Windows, or Windows refused). Call it on the
/// thread that made the window.
pub fn cloak(window: &impl HasWindowHandle, cloaked: bool) -> bool {
    imp::cloak(window, cloaked)
}

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::Mutex;

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winsafe::{DwmAttr, EnumThreadWindows, GetCurrentThreadId, HWND};

    /// The windows whose animations are off already, so that each is told once.
    static CALMED: Mutex<Vec<isize>> = Mutex::new(Vec::new());

    fn id(h: &HWND) -> isize {
        h.ptr() as isize
    }

    pub fn calm_windows() {
        let mut calmed = CALMED.lock().unwrap_or_else(|e| e.into_inner());
        let mut there = Vec::new();
        let _ = EnumThreadWindows(GetCurrentThreadId(), |h: HWND| {
            let i = id(&h);
            there.push(i);
            if !calmed.contains(&i) && h.DwmSetWindowAttribute(DwmAttr::TransitionsForceDisabled(true)).is_ok() {
                calmed.push(i);
            }
            true
        });
        // A window gone: its handle may come again, for another window.
        calmed.retain(|i| there.contains(i));
    }

    pub fn cloak(window: &impl HasWindowHandle, cloaked: bool) -> bool {
        let Some(want) = window.window_handle().ok().and_then(|h| match h.as_raw() {
            RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
            _ => None,
        }) else {
            return false;
        };
        let mut done = false;
        let _ = EnumThreadWindows(GetCurrentThreadId(), |h: HWND| {
            if id(&h) != want {
                return true;
            }
            done = h.DwmSetWindowAttribute(DwmAttr::Cloak(cloaked)).is_ok();
            false
        });
        done
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use raw_window_handle::HasWindowHandle;

    pub fn calm_windows() {}

    pub fn cloak(_window: &impl HasWindowHandle, _cloaked: bool) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thread_without_windows_is_calmed_without_fuss() {
        calm_windows();
        calm_windows();
    }
}
