//! Turning the mouse wheel over a place on the screen, for a scrolling capture that scrolls by
//! itself.
//!
//! On Windows the turn is sent as if it came from the mouse (`SendInput`, through `winsafe`'s safe
//! wrapper): it goes to the window under the pointer, so the pointer is put over the place for it
//! and put back where it was straight after. Nothing else is sent: no clicks, no keys. Elsewhere it
//! can't yet, and a scrolling capture is scrolled by hand.

/// Whether this system can turn the wheel for a scrolling capture.
pub fn supported() -> bool {
    cfg!(target_os = "windows")
}

/// Why it can't, for the setting that's greyed out where [`supported`] is false.
pub const UNSUPPORTED: &str = "On Windows for now: elsewhere you scroll the page by hand";

/// The largest turn sent at once (ten notches).
const MAX_TURN: i32 = 1200;

/// Turns the wheel `turn` (120 is one notch; more than 0 goes down the page) over `at`, a place on
/// the desktop in its pixels.
pub fn wheel(at: (i32, i32), turn: i32) -> Result<(), String> {
    imp::wheel(at, turn.clamp(-MAX_TURN, MAX_TURN))
}

#[cfg(target_os = "windows")]
mod imp {
    use winsafe::{self as w, co};

    /// How long the pointer stays over the place, for the system to send the turn there.
    const HOLD: std::time::Duration = std::time::Duration::from_millis(40);

    pub fn wheel(at: (i32, i32), turn: i32) -> Result<(), String> {
        let back = w::GetCursorPos().map_err(|e| format!("where the pointer is: {e}"))?;
        w::SetCursorPos(at.0, at.1).map_err(|e| format!("the pointer could not be moved: {e}"))?;
        // The wheel's own count goes up turning it away from you, which goes up the page.
        let input = w::MOUSEINPUT { mouseData: turn.saturating_neg().cast_unsigned(), dwFlags: co::MOUSEEVENTF::WHEEL, ..Default::default() };
        let sent = w::SendInput(&[w::HwKbMouse::Mouse(input)]);
        std::thread::sleep(HOLD);
        let _ = w::SetCursorPos(back.x, back.y);
        sent.map(|_| ()).map_err(|e| format!("the wheel could not be turned: {e}"))
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    pub fn wheel(_at: (i32, i32), _turn: i32) -> Result<(), String> {
        Err(super::UNSUPPORTED.to_string())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(not(target_os = "windows"))]
    fn elsewhere_the_wheel_is_not_turned() {
        assert!(!super::supported());
        assert!(super::wheel((10, 10), 120).is_err());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_can_scroll_by_itself() {
        // Only asked: a test never turns the wheel over the owner's windows.
        assert!(super::supported());
    }
}
