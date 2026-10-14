//! The global shortcuts: the owner's settings registered with the system, and the presses
//! turned back into capture modes.
//!
//! Registration works on Windows, macOS and X11. A Wayland session has no way for a program to
//! take a key for itself; there the owner binds a key in the desktop's own settings to
//! `screengrabcraft --capture region` (and the like), which reaches the running service.

use global_hotkey::GlobalHotKeyManager;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use screengrabcraft_model::{CaptureMode, Hotkey, Hotkeys};

/// What a registered shortcut does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Capture(CaptureMode),
    Editor,
}

/// The system's key code for a key name of [`screengrabcraft_model::settings::KEYS`].
pub fn code(name: &str) -> Option<Code> {
    let c = match name {
        "PrintScreen" => Code::PrintScreen,
        "Insert" => Code::Insert,
        "Home" => Code::Home,
        "End" => Code::End,
        "PageUp" => Code::PageUp,
        "PageDown" => Code::PageDown,
        "Pause" => Code::Pause,
        "ScrollLock" => Code::ScrollLock,
        "Space" => Code::Space,
        "F1" => Code::F1,
        "F2" => Code::F2,
        "F3" => Code::F3,
        "F4" => Code::F4,
        "F5" => Code::F5,
        "F6" => Code::F6,
        "F7" => Code::F7,
        "F8" => Code::F8,
        "F9" => Code::F9,
        "F10" => Code::F10,
        "F11" => Code::F11,
        "F12" => Code::F12,
        other => {
            let mut chars = other.chars();
            let (Some(ch), None) = (chars.next(), chars.next()) else { return None };
            match ch {
                'A' => Code::KeyA,
                'B' => Code::KeyB,
                'C' => Code::KeyC,
                'D' => Code::KeyD,
                'E' => Code::KeyE,
                'F' => Code::KeyF,
                'G' => Code::KeyG,
                'H' => Code::KeyH,
                'I' => Code::KeyI,
                'J' => Code::KeyJ,
                'K' => Code::KeyK,
                'L' => Code::KeyL,
                'M' => Code::KeyM,
                'N' => Code::KeyN,
                'O' => Code::KeyO,
                'P' => Code::KeyP,
                'Q' => Code::KeyQ,
                'R' => Code::KeyR,
                'S' => Code::KeyS,
                'T' => Code::KeyT,
                'U' => Code::KeyU,
                'V' => Code::KeyV,
                'W' => Code::KeyW,
                'X' => Code::KeyX,
                'Y' => Code::KeyY,
                'Z' => Code::KeyZ,
                '0' => Code::Digit0,
                '1' => Code::Digit1,
                '2' => Code::Digit2,
                '3' => Code::Digit3,
                '4' => Code::Digit4,
                '5' => Code::Digit5,
                '6' => Code::Digit6,
                '7' => Code::Digit7,
                '8' => Code::Digit8,
                '9' => Code::Digit9,
                _ => return None,
            }
        }
    };
    Some(c)
}

pub fn to_system(h: &Hotkey) -> Option<HotKey> {
    let mut m = Modifiers::empty();
    if h.ctrl {
        m |= Modifiers::CONTROL;
    }
    if h.alt {
        m |= Modifiers::ALT;
    }
    if h.shift {
        m |= Modifiers::SHIFT;
    }
    if h.logo {
        m |= Modifiers::SUPER;
    }
    Some(HotKey::new(if m.is_empty() { None } else { Some(m) }, code(&h.key)?))
}

/// The name of the logo key here.
pub fn logo_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else if cfg!(target_os = "windows") {
        "Win"
    } else {
        "Super"
    }
}

/// The registered shortcuts and what went wrong registering the others.
pub struct Registered {
    manager: Option<GlobalHotKeyManager>,
    active: Vec<(HotKey, Target)>,
    pub problems: Vec<String>,
}

impl Registered {
    pub fn new() -> Registered {
        let manager = match GlobalHotKeyManager::new() {
            Ok(m) => Some(m),
            Err(e) => {
                log::warn!("global shortcuts are not available: {e}");
                None
            }
        };
        let problems = if manager.is_none() { vec![no_shortcuts_text()] } else { Vec::new() };
        Registered { manager, active: Vec::new(), problems }
    }

    /// Whether this system lets a program have global shortcuts at all.
    pub fn available(&self) -> bool {
        self.manager.is_some()
    }

    /// Registers the shortcuts of `keys`, replacing those registered before.
    pub fn apply(&mut self, keys: &Hotkeys) {
        let Some(manager) = &self.manager else { return };
        for (k, _) in self.active.drain(..) {
            let _ = manager.unregister(k);
        }
        self.problems.clear();
        let mut wanted: Vec<(Option<&Hotkey>, Target)> = CaptureMode::ALL.iter().map(|m| (keys.get(*m), Target::Capture(*m))).collect();
        wanted.push((keys.editor.as_ref(), Target::Editor));
        for (key, target) in wanted {
            let Some(key) = key else { continue };
            let label = key.display(logo_name());
            let Some(hk) = to_system(key) else {
                self.problems.push(format!("{label}: not a key this system knows"));
                continue;
            };
            if self.active.iter().any(|(k, _)| *k == hk) {
                self.problems.push(format!("{label} is set for two actions; only the first has it"));
                continue;
            }
            match manager.register(hk) {
                Ok(()) => self.active.push((hk, target)),
                Err(e) => {
                    log::warn!("registering {label}: {e}");
                    self.problems.push(format!("{label} is taken by another program ({e})"));
                }
            }
        }
        log::info!("{} global shortcut(s) registered", self.active.len());
    }

    /// What a pressed shortcut is for.
    pub fn target(&self, id: u32) -> Option<Target> {
        self.active.iter().find(|(k, _)| k.id() == id).map(|(_, t)| *t)
    }

    /// The shortcut registered for a mode, for menus.
    pub fn label(&self, keys: &Hotkeys, mode: CaptureMode) -> Option<String> {
        let k = keys.get(mode)?;
        let hk = to_system(k)?;
        self.active.iter().any(|(a, _)| *a == hk).then(|| k.display(logo_name()))
    }
}

fn no_shortcuts_text() -> String {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        "This Wayland desktop doesn't let programs have global shortcuts. Bind keys in your desktop's keyboard settings to commands like \"screengrabcraft --capture region\".".to_string()
    } else {
        "Global shortcuts are not available on this system; use the tray menu.".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::settings::KEYS;

    #[test]
    fn every_offered_key_has_a_code() {
        for k in KEYS {
            assert!(code(k).is_some(), "{k}");
        }
        assert!(code("Banana").is_none());
        let h = Hotkey::new(true, false, true, false, "PrintScreen");
        let s = to_system(&h).unwrap();
        assert_eq!(s, HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::PrintScreen));
        for mac in [false, true] {
            let keys = Hotkeys::defaults_for(mac);
            for m in CaptureMode::ALL {
                if let Some(k) = keys.get(m) {
                    assert!(to_system(k).is_some(), "{m:?}");
                }
            }
        }
    }
}
