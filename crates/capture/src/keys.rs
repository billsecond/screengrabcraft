//! The shortcuts pressed while a video recording runs, to show on the video (a keys callout).
//!
//! Only shortcuts are kept, never what's typed: a key pressed with Ctrl, Alt or the Windows key,
//! or a key that doesn't type anything (Enter, Esc, Tab, Backspace, Delete, Insert, Home, End,
//! Page Up, Page Down, the arrows, F1 to F24), with Shift if it's held. Letters, digits, symbols
//! and Space pressed on their own or with Shift are left out, so a password typed while recording
//! isn't in it; so is AltGr with a key (the right Alt), which types a character. Keys are only
//! looked at while a recording runs, and nothing of them is logged.
//!
//! On Windows the keys are asked for through a safe binding (`GetAsyncKeyState`, the same as the
//! pointer's buttons), as the pointer is, every few milliseconds. Elsewhere there are none yet
//! ([`available`] says so).

use std::collections::BTreeSet;

/// The keys down at one moment: which modifiers, and the other keys by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Held {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The Windows key (Cmd on a Mac).
    pub win: bool,
    /// AltGr (the right Alt): what's pressed with it types a character.
    pub alt_gr: bool,
    pub keys: Vec<&'static str>,
}

/// The longest a shortcut's words are (`Ctrl+Alt+Shift+Win+Page Down` is 29).
pub const MAX_WORDS: usize = 48;

/// Whether a key types something (a letter, a digit, a symbol, a space).
pub fn types(key: &str) -> bool {
    let mut c = key.chars();
    matches!((c.next(), c.next()), (Some(_), None)) || key == "Space" || key.starts_with("Num ")
}

/// The shortcut a key makes with the modifiers held (`Ctrl+Shift+S`, `Enter`, `Shift+Tab`), or
/// none when it's typing (see the module's notes).
pub fn shortcut(held: &Held, key: &str) -> Option<String> {
    let (ctrl, alt) = if held.alt_gr { (false, false) } else { (held.ctrl, held.alt) };
    let command = ctrl || alt || held.win;
    if !command && types(key) {
        return None;
    }
    let mut words = Vec::with_capacity(5);
    for (on, name) in [(ctrl, "Ctrl"), (alt, "Alt"), (held.shift, "Shift"), (held.win, "Win")] {
        if on {
            words.push(name);
        }
    }
    words.push(key);
    let s = words.join("+");
    (s.len() <= MAX_WORDS).then_some(s)
}

/// Follows the keys from one look to the next: a key that has just gone down makes a shortcut
/// (if it is one); one held down makes it once.
#[derive(Debug, Default)]
pub struct Watch {
    down: BTreeSet<&'static str>,
}

impl Watch {
    /// The shortcuts made since the last look.
    pub fn step(&mut self, held: &Held) -> Vec<String> {
        let now: BTreeSet<&'static str> = held.keys.iter().copied().collect();
        let out = held.keys.iter().filter(|k| !self.down.contains(*k)).filter_map(|k| shortcut(held, k)).collect();
        self.down = now;
        out
    }
}

/// Whether the keys pressed can be kept on this system.
pub fn available() -> bool {
    cfg!(target_os = "windows")
}

/// The keys down now; none where the system doesn't say.
pub fn held() -> Option<Held> {
    imp::held()
}

#[cfg(target_os = "windows")]
mod imp {
    use winsafe::{self as w, co};

    use super::Held;

    /// Every key looked at, by its name on the video.
    const KEYS: &[(co::VK, &str)] = &[
        (co::VK::CHAR_A, "A"),
        (co::VK::CHAR_B, "B"),
        (co::VK::CHAR_C, "C"),
        (co::VK::CHAR_D, "D"),
        (co::VK::CHAR_E, "E"),
        (co::VK::CHAR_F, "F"),
        (co::VK::CHAR_G, "G"),
        (co::VK::CHAR_H, "H"),
        (co::VK::CHAR_I, "I"),
        (co::VK::CHAR_J, "J"),
        (co::VK::CHAR_K, "K"),
        (co::VK::CHAR_L, "L"),
        (co::VK::CHAR_M, "M"),
        (co::VK::CHAR_N, "N"),
        (co::VK::CHAR_O, "O"),
        (co::VK::CHAR_P, "P"),
        (co::VK::CHAR_Q, "Q"),
        (co::VK::CHAR_R, "R"),
        (co::VK::CHAR_S, "S"),
        (co::VK::CHAR_T, "T"),
        (co::VK::CHAR_U, "U"),
        (co::VK::CHAR_V, "V"),
        (co::VK::CHAR_W, "W"),
        (co::VK::CHAR_X, "X"),
        (co::VK::CHAR_Y, "Y"),
        (co::VK::CHAR_Z, "Z"),
        (co::VK::CHAR_0, "0"),
        (co::VK::CHAR_1, "1"),
        (co::VK::CHAR_2, "2"),
        (co::VK::CHAR_3, "3"),
        (co::VK::CHAR_4, "4"),
        (co::VK::CHAR_5, "5"),
        (co::VK::CHAR_6, "6"),
        (co::VK::CHAR_7, "7"),
        (co::VK::CHAR_8, "8"),
        (co::VK::CHAR_9, "9"),
        (co::VK::OEM_PLUS, "="),
        (co::VK::OEM_MINUS, "-"),
        (co::VK::OEM_COMMA, ","),
        (co::VK::OEM_PERIOD, "."),
        (co::VK::OEM_1, ";"),
        (co::VK::OEM_2, "/"),
        (co::VK::OEM_3, "`"),
        (co::VK::OEM_4, "["),
        (co::VK::OEM_5, "\\"),
        (co::VK::OEM_6, "]"),
        (co::VK::OEM_7, "'"),
        (co::VK::NUMPAD0, "Num 0"),
        (co::VK::NUMPAD1, "Num 1"),
        (co::VK::NUMPAD2, "Num 2"),
        (co::VK::NUMPAD3, "Num 3"),
        (co::VK::NUMPAD4, "Num 4"),
        (co::VK::NUMPAD5, "Num 5"),
        (co::VK::NUMPAD6, "Num 6"),
        (co::VK::NUMPAD7, "Num 7"),
        (co::VK::NUMPAD8, "Num 8"),
        (co::VK::NUMPAD9, "Num 9"),
        (co::VK::MULTIPLY, "Num *"),
        (co::VK::ADD, "Num Plus"),
        (co::VK::SUBTRACT, "Num -"),
        (co::VK::DECIMAL, "Num ."),
        (co::VK::DIVIDE, "Num /"),
        (co::VK::SPACE, "Space"),
        (co::VK::RETURN, "Enter"),
        (co::VK::ESCAPE, "Esc"),
        (co::VK::TAB, "Tab"),
        (co::VK::BACK, "Backspace"),
        (co::VK::DELETE, "Delete"),
        (co::VK::INSERT, "Insert"),
        (co::VK::HOME, "Home"),
        (co::VK::END, "End"),
        (co::VK::PRIOR, "Page Up"),
        (co::VK::NEXT, "Page Down"),
        (co::VK::LEFT, "Left"),
        (co::VK::RIGHT, "Right"),
        (co::VK::UP, "Up"),
        (co::VK::DOWN, "Down"),
        (co::VK::F1, "F1"),
        (co::VK::F2, "F2"),
        (co::VK::F3, "F3"),
        (co::VK::F4, "F4"),
        (co::VK::F5, "F5"),
        (co::VK::F6, "F6"),
        (co::VK::F7, "F7"),
        (co::VK::F8, "F8"),
        (co::VK::F9, "F9"),
        (co::VK::F10, "F10"),
        (co::VK::F11, "F11"),
        (co::VK::F12, "F12"),
        (co::VK::F13, "F13"),
        (co::VK::F14, "F14"),
        (co::VK::F15, "F15"),
        (co::VK::F16, "F16"),
        (co::VK::F17, "F17"),
        (co::VK::F18, "F18"),
        (co::VK::F19, "F19"),
        (co::VK::F20, "F20"),
        (co::VK::F21, "F21"),
        (co::VK::F22, "F22"),
        (co::VK::F23, "F23"),
        (co::VK::F24, "F24"),
    ];

    pub fn held() -> Option<Held> {
        let down = |k: co::VK| w::GetAsyncKeyState(k);
        let alt_gr = down(co::VK::RMENU);
        Some(Held {
            ctrl: down(co::VK::LCONTROL) || down(co::VK::RCONTROL),
            alt: down(co::VK::LMENU),
            shift: down(co::VK::LSHIFT) || down(co::VK::RSHIFT),
            win: down(co::VK::LWIN) || down(co::VK::RWIN),
            alt_gr,
            keys: KEYS.iter().filter(|(k, _)| down(*k)).map(|(_, name)| *name).take(8).collect(),
        })
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::Held;

    pub fn held() -> Option<Held> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(ctrl: bool, alt: bool, shift: bool, win: bool, keys: &[&'static str]) -> Held {
        Held { ctrl, alt, shift, win, alt_gr: false, keys: keys.to_vec() }
    }

    #[test]
    fn shortcuts_are_kept_and_typing_is_not() {
        let none = with(false, false, false, false, &[]);
        let shift = with(false, false, true, false, &[]);
        let ctrl_shift = with(true, false, true, false, &[]);
        // Typing: letters, digits, symbols, Space, the number pad, alone or with Shift.
        for key in ["a", "S", "7", "=", "Space", "Num 4"] {
            assert_eq!(shortcut(&none, key), None, "{key}");
            assert_eq!(shortcut(&shift, key), None, "Shift+{key}");
        }
        // Shortcuts: with Ctrl, Alt or Win; keys that don't type, with Shift if it's held.
        assert_eq!(shortcut(&ctrl_shift, "S").as_deref(), Some("Ctrl+Shift+S"));
        assert_eq!(shortcut(&with(false, true, false, false, &[]), "F4").as_deref(), Some("Alt+F4"));
        assert_eq!(shortcut(&with(false, false, false, true, &[]), "D").as_deref(), Some("Win+D"));
        assert_eq!(shortcut(&none, "Enter").as_deref(), Some("Enter"));
        assert_eq!(shortcut(&shift, "Tab").as_deref(), Some("Shift+Tab"));
        assert_eq!(shortcut(&none, "F5").as_deref(), Some("F5"));
        assert_eq!(shortcut(&none, "Page Down").as_deref(), Some("Page Down"));
        // AltGr (Ctrl and the right Alt, as Windows reports it) types a character: left out.
        let alt_gr = Held { ctrl: true, alt_gr: true, ..Held::default() };
        assert_eq!(shortcut(&alt_gr, "Q"), None);
        assert_eq!(shortcut(&Held { win: true, ..alt_gr }, "Q").as_deref(), Some("Win+Q"));
    }

    #[test]
    fn a_key_held_down_is_one_shortcut_and_a_new_one_another() {
        let mut w = Watch::default();
        assert_eq!(w.step(&with(true, false, false, false, &[])), Vec::<String>::new(), "Ctrl alone is nothing");
        assert_eq!(w.step(&with(true, false, false, false, &["C"])), vec!["Ctrl+C".to_string()]);
        assert!(w.step(&with(true, false, false, false, &["C"])).is_empty(), "still held: once");
        assert_eq!(w.step(&with(true, false, false, false, &["C", "V"])), vec!["Ctrl+V".to_string()]);
        assert!(w.step(&with(false, false, false, false, &[])).is_empty());
        // Typed: nothing at all.
        for k in ["h", "u", "n", "t", "e", "r", "2"] {
            assert!(w.step(&with(false, false, false, false, &[k])).is_empty());
            w.step(&with(false, false, false, false, &[]));
        }
        assert_eq!(w.step(&with(false, false, false, false, &["Enter"])), vec!["Enter".to_string()]);
    }

    #[test]
    fn the_keys_can_be_asked_for_where_the_system_says() {
        assert_eq!(available(), cfg!(target_os = "windows"));
        // Asking never fails or panics (the test's own keyboard is whatever it is).
        let _ = held();
    }
}
