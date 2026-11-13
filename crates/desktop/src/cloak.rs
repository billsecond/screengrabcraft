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

/// Makes the calling thread's windows titled `title` plain, each once: no shadow, square corners,
/// and Windows' thin border in the window's own colour `rgb`, so that nothing but the window's
/// colour is seen (the frame around an area being recorded: one colour, no outline). Call it on
/// the thread that made the windows, once they're made.
pub fn plain_windows(title: &str, rgb: [u8; 3]) {
    imp::plain_windows(title, rgb);
}

/// Keeps the calling thread's windows titled `title` out of every screen capture and recording,
/// this program's own included, each once: they're seen on the screen and nowhere else (the
/// controls of a recording, wherever they're dragged). Windows 10 2004 and later; elsewhere it
/// does nothing. Call it on the thread that made the windows, once they're made.
pub fn keep_out_of_captures(title: &str) {
    imp::keep_out_of_captures(title);
}

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::Mutex;

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winsafe::{DwmAttr, EnumThreadWindows, GetCurrentThreadId, HWND};

    /// The windows whose animations are off already, so that each is told once.
    static CALMED: Mutex<Vec<isize>> = Mutex::new(Vec::new());
    /// The windows made plain already.
    static PLAIN: Mutex<Vec<isize>> = Mutex::new(Vec::new());
    /// The windows kept out of captures already.
    static UNSEEN: Mutex<Vec<isize>> = Mutex::new(Vec::new());

    pub fn keep_out_of_captures(title: &str) {
        use winsafe::co;
        let mut unseen = UNSEEN.lock().unwrap_or_else(|e| e.into_inner());
        let mut there = Vec::new();
        let _ = EnumThreadWindows(GetCurrentThreadId(), |h: HWND| {
            let i = id(&h);
            there.push(i);
            if !unseen.contains(&i) && h.GetWindowText().is_ok_and(|t| t == title) {
                match h.SetWindowDisplayAffinity(co::WDA::EXCLUDEFROMCAPTURE) {
                    Ok(()) => unseen.push(i),
                    Err(e) => {
                        log::warn!("a window couldn't be kept out of captures: {e}");
                        unseen.push(i);
                    }
                }
            }
            true
        });
        unseen.retain(|i| there.contains(i));
    }

    pub fn plain_windows(title: &str, rgb: [u8; 3]) {
        use winsafe::{COLORREF, co};
        let mut plain = PLAIN.lock().unwrap_or_else(|e| e.into_inner());
        let mut there = Vec::new();
        let _ = EnumThreadWindows(GetCurrentThreadId(), |h: HWND| {
            let i = id(&h);
            there.push(i);
            if !plain.contains(&i) && h.GetWindowText().is_ok_and(|t| t == title) {
                // No shadow (the window manager draws nothing of its own around it), square
                // corners, and the border Windows 11 draws in the window's own colour.
                let _ = h.DwmSetWindowAttribute(DwmAttr::NcRenderingPolicy(co::DWMNCRENDERINGPOLICY::DISABLED));
                let _ = h.DwmSetWindowAttribute(DwmAttr::WindowCornerPreference(co::DWMWCP::DONOTROUND));
                let _ = h.DwmSetWindowAttribute(DwmAttr::BorderColor(COLORREF::from_rgb(rgb[0], rgb[1], rgb[2])));
                plain.push(i);
            }
            true
        });
        plain.retain(|i| there.contains(i));
    }

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

    pub fn plain_windows(_title: &str, _rgb: [u8; 3]) {}

    pub fn keep_out_of_captures(_title: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thread_without_windows_is_calmed_without_fuss() {
        calm_windows();
        calm_windows();
        plain_windows("Nothing of that name", [229, 57, 53]);
    }
}
