//! Open menus, drop-downs and tooltips, to snap to like windows. A capture taken while one is
//! open has it in its frozen picture, but Windows doesn't list these with the programs' windows
//! (they're "tool windows"), so they're looked for here: the system's own menus and tooltips, and
//! the pop-ups of the program in front (most programs draw their own menus and drop-downs).
//! Elsewhere there are none to add.

use screengrabcraft_model::IRect;

use crate::WindowInfo;

/// The largest a drop-down of the program in front is taken to be, on either side: anything
/// larger is something else (a full-screen pane, a notification layer).
const MAX_SIDE: u32 = 2400;
/// The most looked for.
const MAX_FOUND: usize = 64;

/// What's known of a top-level window, to tell an open menu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Popup {
    /// Its window class.
    pub class: String,
    /// A tool window (no taskbar button, not listed with the others).
    pub tool: bool,
    /// A pop-up window (no frame of its own).
    pub popup: bool,
    /// The program it belongs to.
    pub pid: u32,
    pub rect: IRect,
}

/// The words a menu-like window is known by, or none when it isn't one: the system's menus and
/// tooltips, and the tool pop-ups of the program in front (`front`); never this program's (`own`)
/// or one too small to see.
pub fn kind_of(w: &Popup, own: u32, front: Option<u32>) -> Option<&'static str> {
    if w.pid == own || w.rect.w < 8 || w.rect.h < 8 {
        return None;
    }
    match w.class.as_str() {
        "#32768" => Some("Menu"),
        "tooltips_class32" => Some("Tooltip"),
        _ if w.tool && w.popup && front == Some(w.pid) && w.rect.w <= MAX_SIDE && w.rect.h <= MAX_SIDE => Some("Menu"),
        _ => None,
    }
}

/// The open menus, drop-downs and tooltips, front first (they're in front of every window).
#[cfg(target_os = "windows")]
pub fn open() -> Vec<WindowInfo> {
    use winsafe::{HWND, co};
    let own = std::process::id();
    let front = HWND::GetForegroundWindow().map(|h| h.GetWindowThreadProcessId().1);
    let mut found = Vec::new();
    let listed = winsafe::EnumWindows(|h: HWND| {
        if found.len() >= MAX_FOUND {
            return false;
        }
        // Shown, and not cloaked (there and drawn, but not seen: not in the picture either).
        let cloaked = matches!(h.DwmGetWindowAttribute(co::DWMWA::CLOAKED), Ok(winsafe::DwmAttr::Cloaked(c))
            if [co::DWM_CLOAKED::APP, co::DWM_CLOAKED::SHELL, co::DWM_CLOAKED::INHERITED].iter().any(|f| c.has(*f)));
        if !h.IsWindowVisible() || cloaked {
            return true;
        }
        let Ok(r) = h.GetWindowRect() else { return true };
        let (Ok(w), Ok(hgt)) = (u32::try_from(r.right.saturating_sub(r.left)), u32::try_from(r.bottom.saturating_sub(r.top))) else { return true };
        let style = h.GetWindowLongPtr(co::GWLP::STYLE);
        let ex = h.GetWindowLongPtr(co::GWLP::EXSTYLE);
        let p = Popup {
            class: h.GetClassName().unwrap_or_default(),
            tool: ex & 0x80 != 0,
            popup: (style as u32) & 0x8000_0000 != 0,
            pid: h.GetWindowThreadProcessId().1,
            rect: IRect::new(r.left, r.top, w, hgt),
        };
        if let Some(kind) = kind_of(&p, own, front) {
            found.push(WindowInfo { id: 0, title: kind.to_string(), app: String::new(), rect: p.rect });
        }
        true
    });
    if let Err(e) = listed {
        log::warn!("looking for open menus: {e}");
    }
    found
}

#[cfg(not(target_os = "windows"))]
pub fn open() -> Vec<WindowInfo> {
    Vec::new()
}

/// `menus` in front of `windows` (front first), each window once: a menu that is a listed window
/// (the same place) isn't listed twice.
pub fn in_front(menus: Vec<WindowInfo>, windows: Vec<WindowInfo>) -> Vec<WindowInfo> {
    let mut out: Vec<WindowInfo> = menus.into_iter().filter(|m| !windows.iter().any(|w| w.rect == m.rect)).collect();
    out.extend(windows);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn popup(class: &str, tool: bool, popup: bool, pid: u32, w: u32, h: u32) -> Popup {
        Popup { class: class.into(), tool, popup, pid, rect: IRect::new(10, 20, w, h) }
    }

    #[test]
    fn menus_tooltips_and_the_front_programs_drop_downs_are_known() {
        let (own, front) = (1, Some(7));
        assert_eq!(kind_of(&popup("#32768", true, true, 5, 200, 300), own, front), Some("Menu"));
        assert_eq!(kind_of(&popup("tooltips_class32", true, true, 5, 120, 24), own, front), Some("Tooltip"));
        // The program in front draws its own: a tool pop-up of it is a menu, another program's isn't.
        assert_eq!(kind_of(&popup("Chrome_WidgetWin_1", true, true, 7, 260, 400), own, front), Some("Menu"));
        assert_eq!(kind_of(&popup("Chrome_WidgetWin_1", true, true, 9, 260, 400), own, front), None);
        assert_eq!(kind_of(&popup("Chrome_WidgetWin_1", true, true, 7, 260, 400), own, None), None);
        // Not a tool pop-up, too large, too small, or this program's own: none.
        assert_eq!(kind_of(&popup("Notepad", false, false, 7, 800, 600), own, front), None);
        assert_eq!(kind_of(&popup("Overlay", true, true, 7, 3840, 2160), own, front), None);
        assert_eq!(kind_of(&popup("#32768", true, true, 5, 4, 300), own, front), None);
        assert_eq!(kind_of(&popup("#32768", true, true, own, 200, 300), own, front), None);
    }

    #[test]
    fn menus_go_in_front_of_the_windows_once() {
        let win = |title: &str, rect: IRect| WindowInfo { id: 0, title: title.into(), app: String::new(), rect };
        let menu = win("Menu", IRect::new(100, 100, 200, 300));
        let page = win("Notes", IRect::new(0, 0, 1000, 800));
        let both = in_front(vec![menu.clone()], vec![page.clone()]);
        assert_eq!(both.iter().map(|w| w.title.as_str()).collect::<Vec<_>>(), ["Menu", "Notes"]);
        // A menu already listed as a window, the same place: once.
        let listed = win("Listed", menu.rect);
        assert_eq!(in_front(vec![menu], vec![listed, page]).len(), 2);
        assert!(in_front(Vec::new(), Vec::new()).is_empty());
    }
}
