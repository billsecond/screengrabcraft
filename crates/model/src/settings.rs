//! What the owner chose: the capture service's settings (shortcuts, what happens after a
//! capture, where captures go) and the editor's. Both programs read `settings.json`; the
//! capture service writes it, the editor writes `editor.json`.
//!
//! Everything has a default, and a file from another version (or hand-edited) is read with
//! whatever it gets wrong replaced by the default: a bad settings file never stops a program
//! from starting.

use serde::{Deserialize, Serialize};

use crate::geom::IRect;
use crate::object::{Kind, Style};

/// A keyboard shortcut that works anywhere on the desktop: modifiers and a key, written the way
/// people say it (`Ctrl+Shift+PrintScreen`, `Alt+F9`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The Windows key, Command on a Mac.
    pub logo: bool,
    /// The key's name, as [`KEYS`] spells it.
    pub key: String,
}

/// The keys a shortcut can be on, by the name shortcuts are written with.
pub const KEYS: &[&str] = &[
    "PrintScreen",
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Pause",
    "ScrollLock",
    "Space",
];

impl Hotkey {
    pub fn new(ctrl: bool, alt: bool, shift: bool, logo: bool, key: &str) -> Hotkey {
        Hotkey { ctrl, alt, shift, logo, key: key.to_string() }
    }

    /// Reads `Ctrl+Shift+X`. Modifier names are taken in any case and order (`Control`, `Cmd`,
    /// `Win`, `Super`, `Option` too); the key must be one of [`KEYS`].
    pub fn parse(text: &str) -> Option<Hotkey> {
        let mut h = Hotkey::default();
        let parts: Vec<&str> = text.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
        let (key, mods) = parts.split_last()?;
        for m in mods {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => h.ctrl = true,
                "alt" | "option" => h.alt = true,
                "shift" => h.shift = true,
                "win" | "super" | "cmd" | "command" | "meta" | "logo" => h.logo = true,
                _ => return None,
            }
        }
        let key = KEYS.iter().find(|k| k.eq_ignore_ascii_case(key)).or_else(|| match key.to_ascii_lowercase().as_str() {
            "prtsc" | "prtscn" | "print" | "printscr" => Some(&"PrintScreen"),
            _ => None,
        })?;
        h.key = (*key).to_string();
        Some(h)
    }

    /// `Ctrl+Alt+Shift+Win+Key`, in that order; `logo_name` says what the logo key is called
    /// on this system (`Win`, `Cmd`, `Super`).
    pub fn display(&self, logo_name: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.logo {
            parts.push(logo_name);
        }
        parts.push(&self.key);
        parts.join("+")
    }
}

impl Serialize for Hotkey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.display("Super"))
    }
}

impl<'de> Deserialize<'de> for Hotkey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Hotkey, D::Error> {
        let text = String::deserialize(d)?;
        Hotkey::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("not a shortcut: {text:?}")))
    }
}

/// What a capture is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureMode {
    /// Drag out an area (or click a window).
    Region,
    /// Click a window.
    Window,
    /// The monitor under the pointer.
    Screen,
    /// Every monitor in one picture.
    AllScreens,
    /// The same area as last time.
    Repeat,
    /// An area recorded as an animated GIF.
    Gif,
    /// An area captured while it scrolls, stitched into one long picture.
    Scrolling,
}

impl CaptureMode {
    pub const ALL: [CaptureMode; 7] = [
        CaptureMode::Region,
        CaptureMode::Window,
        CaptureMode::Screen,
        CaptureMode::AllScreens,
        CaptureMode::Repeat,
        CaptureMode::Scrolling,
        CaptureMode::Gif,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CaptureMode::Region => "Capture Region",
            CaptureMode::Window => "Capture Window",
            CaptureMode::Screen => "Capture Full Screen",
            CaptureMode::AllScreens => "Capture All Screens",
            CaptureMode::Repeat => "Repeat Last Capture",
            CaptureMode::Gif => "Record GIF",
            CaptureMode::Scrolling => "Scrolling Capture",
        }
    }

    /// The word used on the command line (`--capture region`).
    pub fn arg(self) -> &'static str {
        match self {
            CaptureMode::Region => "region",
            CaptureMode::Window => "window",
            CaptureMode::Screen => "screen",
            CaptureMode::AllScreens => "all",
            CaptureMode::Repeat => "repeat",
            CaptureMode::Gif => "gif",
            CaptureMode::Scrolling => "scrolling",
        }
    }

    pub fn from_arg(text: &str) -> Option<CaptureMode> {
        CaptureMode::ALL.into_iter().find(|m| m.arg().eq_ignore_ascii_case(text.trim()))
    }
}

/// One shortcut per kind of capture, and one for the editor; `None` is no shortcut.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Hotkeys {
    pub region: Option<Hotkey>,
    pub window: Option<Hotkey>,
    pub screen: Option<Hotkey>,
    pub all_screens: Option<Hotkey>,
    pub repeat: Option<Hotkey>,
    pub scrolling: Option<Hotkey>,
    pub gif: Option<Hotkey>,
    pub editor: Option<Hotkey>,
}

impl Hotkeys {
    /// The shortcuts a new installation starts with. Print Screen where keyboards have it;
    /// Control+Shift and a letter on a Mac, whose keyboards don't (and whose own screenshot
    /// shortcuts are Command+Shift and a number).
    pub fn defaults_for(mac: bool) -> Hotkeys {
        if mac {
            let k = |key: &str| Some(Hotkey::new(true, false, true, false, key));
            Hotkeys {
                region: k("C"),
                window: k("W"),
                screen: k("F"),
                all_screens: None,
                repeat: k("R"),
                scrolling: k("S"),
                gif: k("G"),
                editor: k("E"),
            }
        } else {
            Hotkeys {
                region: Some(Hotkey::new(false, false, false, false, "PrintScreen")),
                window: Some(Hotkey::new(false, true, false, false, "PrintScreen")),
                screen: Some(Hotkey::new(true, false, false, false, "PrintScreen")),
                all_screens: Some(Hotkey::new(true, false, true, false, "PrintScreen")),
                repeat: Some(Hotkey::new(false, false, true, false, "PrintScreen")),
                scrolling: Some(Hotkey::new(true, true, false, false, "PrintScreen")),
                gif: Some(Hotkey::new(false, true, true, false, "PrintScreen")),
                editor: None,
            }
        }
    }

    pub fn get(&self, mode: CaptureMode) -> Option<&Hotkey> {
        match mode {
            CaptureMode::Region => self.region.as_ref(),
            CaptureMode::Window => self.window.as_ref(),
            CaptureMode::Screen => self.screen.as_ref(),
            CaptureMode::AllScreens => self.all_screens.as_ref(),
            CaptureMode::Repeat => self.repeat.as_ref(),
            CaptureMode::Gif => self.gif.as_ref(),
            CaptureMode::Scrolling => self.scrolling.as_ref(),
        }
    }

    pub fn set(&mut self, mode: CaptureMode, key: Option<Hotkey>) {
        let slot = match mode {
            CaptureMode::Region => &mut self.region,
            CaptureMode::Window => &mut self.window,
            CaptureMode::Screen => &mut self.screen,
            CaptureMode::AllScreens => &mut self.all_screens,
            CaptureMode::Repeat => &mut self.repeat,
            CaptureMode::Gif => &mut self.gif,
            CaptureMode::Scrolling => &mut self.scrolling,
        };
        *slot = key;
    }

    /// Pairs of actions that share a shortcut (only one of each pair can have it).
    pub fn clashes(&self) -> Vec<(String, String)> {
        let mut named: Vec<(&str, &Hotkey)> = CaptureMode::ALL.iter().filter_map(|m| self.get(*m).map(|k| (m.label(), k))).collect();
        if let Some(k) = &self.editor {
            named.push(("Open Editor", k));
        }
        let mut out = Vec::new();
        for (i, (a, ka)) in named.iter().enumerate() {
            for (b, kb) in named.iter().skip(i + 1) {
                if ka == kb {
                    out.push(((*a).to_string(), (*b).to_string()));
                }
            }
        }
        out
    }
}

impl Default for Hotkeys {
    fn default() -> Hotkeys {
        Hotkeys::defaults_for(cfg!(target_os = "macos"))
    }
}

/// The file format captures are saved in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
    Webp,
    Bmp,
    Gif,
    Pdf,
}

impl ImageFormat {
    pub const ALL: [ImageFormat; 6] = [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Webp, ImageFormat::Bmp, ImageFormat::Gif, ImageFormat::Pdf];

    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Webp => "webp",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Gif => "gif",
            ImageFormat::Pdf => "pdf",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ImageFormat::Png => "PNG",
            ImageFormat::Jpeg => "JPEG",
            ImageFormat::Webp => "WebP (lossless)",
            ImageFormat::Bmp => "BMP",
            ImageFormat::Gif => "GIF",
            ImageFormat::Pdf => "PDF",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Webp => "image/webp",
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Pdf => "application/pdf",
        }
    }

    /// The format a file name's extension names.
    pub fn from_extension(ext: &str) -> Option<ImageFormat> {
        match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "png" => Some(ImageFormat::Png),
            "jpg" | "jpeg" | "jpe" => Some(ImageFormat::Jpeg),
            "webp" => Some(ImageFormat::Webp),
            "bmp" => Some(ImageFormat::Bmp),
            "gif" => Some(ImageFormat::Gif),
            "pdf" => Some(ImageFormat::Pdf),
            _ => None,
        }
    }
}

/// What happens once a picture is captured: the default action, of which several parts may be
/// on; with none, the editor opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AfterCapture {
    /// Stop with the capture's handles and its bar of actions on screen, so the area can be
    /// adjusted and an action picked (Edit, Copy, Save); Enter takes the default. Off, the default
    /// runs as soon as the area is chosen.
    pub show_bar: bool,
    pub open_editor: bool,
    pub copy: bool,
    pub save: bool,
}

impl Default for AfterCapture {
    fn default() -> AfterCapture {
        AfterCapture { show_bar: false, open_editor: true, copy: true, save: true }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// The capture service's settings (`settings.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub hotkeys: Hotkeys,
    pub after_capture: AfterCapture,
    /// Where captures are saved; `None` is `Pictures/ScreenGrabCraft`.
    pub folder: Option<String>,
    /// The name of a saved capture, before its extension; see [`expand_name`].
    pub file_name: String,
    pub format: ImageFormat,
    pub jpeg_quality: u8,
    /// Draw the mouse pointer into captures of a screen or a window.
    pub include_cursor: bool,
    /// Seconds to wait before a capture starts when it is asked for with a delay.
    pub delay_secs: u32,
    /// The magnifying loupe next to the pointer while choosing an area.
    pub magnifier: bool,
    /// Lines across the screen through the pointer while choosing an area.
    pub crosshair: bool,
    /// Start the capture service when the owner signs in (and restart it if it stops). `None`
    /// until the first run has decided (it turns it on and says so).
    pub start_at_login: Option<bool>,
    pub gif_fps: u32,
    pub gif_max_secs: u32,
    pub theme: Theme,
    /// The area of the last region capture, for Repeat Last Capture.
    pub last_region: Option<IRect>,
    /// Whether the first-run window has been seen.
    pub welcomed: bool,
    /// Play a short sound when a capture is taken.
    pub sound: bool,
    /// Take an open editor out of the picture before a capture.
    pub hide_editor: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            hotkeys: Hotkeys::default(),
            after_capture: AfterCapture::default(),
            folder: None,
            file_name: "Capture {date} {time}".to_string(),
            format: ImageFormat::Png,
            jpeg_quality: 90,
            include_cursor: false,
            delay_secs: 3,
            magnifier: true,
            crosshair: true,
            start_at_login: None,
            gif_fps: 12,
            gif_max_secs: 60,
            theme: Theme::System,
            last_region: None,
            welcomed: false,
            sound: false,
            hide_editor: true,
        }
    }
}

impl Settings {
    /// Reads settings, replacing what can't be read with defaults (field by field where the
    /// JSON is an object, wholesale where it isn't JSON at all).
    pub fn from_json(text: &str) -> Settings {
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text) else {
            return Settings::default();
        };
        let mut out = serde_json::Map::new();
        let defaults = serde_json::to_value(Settings::default()).unwrap_or_default();
        for (k, v) in map {
            // A field that doesn't read on its own is dropped, so the rest still count.
            let mut probe = serde_json::Map::new();
            probe.insert(k.clone(), v.clone());
            if serde_json::from_value::<Settings>(serde_json::Value::Object(probe)).is_ok() && defaults.get(&k).is_some() {
                out.insert(k, v);
            }
        }
        serde_json::from_value::<Settings>(serde_json::Value::Object(out)).unwrap_or_default().clamped()
    }

    /// Every number in its range.
    pub fn clamped(mut self) -> Settings {
        self.jpeg_quality = self.jpeg_quality.clamp(10, 100);
        self.delay_secs = self.delay_secs.min(60);
        self.gif_fps = self.gif_fps.clamp(1, 30);
        self.gif_max_secs = self.gif_max_secs.clamp(1, 600);
        if self.file_name.trim().is_empty() || self.file_name.len() > 200 {
            self.file_name = Settings::default().file_name;
        }
        self
    }
}

/// The editor's settings (`editor.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EditorSettings {
    /// The style each tool last drew with.
    pub styles: std::collections::BTreeMap<Kind, Style>,
    /// Files opened or saved lately, newest first.
    pub recent: Vec<String>,
    pub theme: Theme,
    /// Show the strip of recent captures along the bottom.
    pub show_tray: bool,
    /// The format Export proposes.
    pub export_format: ImageFormat,
}

impl EditorSettings {
    pub const MAX_RECENT: usize = 20;

    pub fn from_json(text: &str) -> EditorSettings {
        serde_json::from_str::<EditorSettings>(text).unwrap_or_default().clamped()
    }

    pub fn clamped(mut self) -> EditorSettings {
        for s in self.styles.values_mut() {
            *s = s.clamped();
        }
        self.recent.truncate(EditorSettings::MAX_RECENT);
        self
    }

    /// The style a tool draws with: the last one used, or the kind's default.
    pub fn style(&self, kind: Kind) -> Style {
        self.styles.get(&kind).copied().unwrap_or_else(|| kind.default_style())
    }

    /// Puts `path` first in the recent files.
    pub fn remember(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(EditorSettings::MAX_RECENT);
    }
}

/// A moment in local time, for naming captures.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// The name a capture is saved under: `template` with `{date}` (2026-10-14), `{time}` (14.05.09),
/// `{year}`, `{month}`, `{day}`, `{hour}`, `{minute}`, `{second}`, `{source}` (the window or
/// screen captured) and `{n}` (a number that makes the name unique, given by the caller)
/// replaced. Characters no file system takes are replaced by `-`, and the result is never empty.
pub fn expand_name(template: &str, at: Stamp, source: &str, n: u32) -> String {
    let date = format!("{:04}-{:02}-{:02}", at.year, at.month, at.day);
    let time = format!("{:02}.{:02}.{:02}", at.hour, at.minute, at.second);
    let mut out = template.to_string();
    for (key, value) in [
        ("{date}", date),
        ("{time}", time),
        ("{year}", format!("{:04}", at.year)),
        ("{month}", format!("{:02}", at.month)),
        ("{day}", format!("{:02}", at.day)),
        ("{hour}", format!("{:02}", at.hour)),
        ("{minute}", format!("{:02}", at.minute)),
        ("{second}", format!("{:02}", at.second)),
        ("{source}", source.chars().take(60).collect()),
        ("{n}", n.to_string()),
    ] {
        out = out.replace(key, &value);
    }
    let cleaned: String =
        out.chars().map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c }).collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim().chars().take(180).collect::<String>();
    if cleaned.is_empty() { "Capture".to_string() } else { cleaned }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkeys_parse_and_print() {
        let h = Hotkey::parse("ctrl + shift + prtsc").unwrap();
        assert_eq!(h, Hotkey::new(true, false, true, false, "PrintScreen"));
        assert_eq!(h.display("Win"), "Ctrl+Shift+PrintScreen");
        assert_eq!(Hotkey::parse("Cmd+Option+f9").unwrap().display("Cmd"), "Alt+Cmd+F9");
        for bad in ["", "Ctrl+", "Hyper+A", "Ctrl+Banana", "+"] {
            assert_eq!(Hotkey::parse(bad), None, "{bad}");
        }
        let json = serde_json::to_string(&h).unwrap();
        assert_eq!(serde_json::from_str::<Hotkey>(&json).unwrap(), h);
    }

    #[test]
    fn default_hotkeys_do_not_clash() {
        for mac in [false, true] {
            assert!(Hotkeys::defaults_for(mac).clashes().is_empty());
        }
        let mut h = Hotkeys::defaults_for(false);
        h.set(CaptureMode::Gif, h.region.clone());
        assert_eq!(h.clashes(), vec![("Capture Region".to_string(), "Record GIF".to_string())]);
    }

    #[test]
    fn bad_settings_fall_back_field_by_field() {
        assert_eq!(Settings::from_json("not json"), Settings::default());
        assert_eq!(Settings::from_json("[1,2]"), Settings::default());
        let s = Settings::from_json(r#"{"format":"jpeg","jpegQuality":5,"delaySecs":"soon","hotkeys":{"region":"Nope+Q"},"unknown":1}"#);
        assert_eq!(s.format, ImageFormat::Jpeg);
        assert_eq!(s.jpeg_quality, 10, "clamped");
        assert_eq!(s.delay_secs, 3, "unreadable: the default");
        assert_eq!(s.hotkeys, Hotkeys::default(), "unreadable: the default");
        let round = Settings::from_json(&serde_json::to_string(&s).unwrap());
        assert_eq!(round, s);
    }

    #[test]
    fn the_default_copies_and_opens_the_editor_without_stopping() {
        let s = Settings::default();
        assert!(s.after_capture.copy && s.after_capture.open_editor);
        assert!(!s.after_capture.show_bar);
        assert!(s.hide_editor);
    }

    #[test]
    fn a_file_from_before_the_default_ran_at_once_gets_it() {
        // Every start used to write `preview: true`, the old default, so it says nothing of what
        // the owner chose.
        let s = Settings::from_json(r#"{"afterCapture":{"preview":true,"openEditor":true,"copy":false,"save":true}}"#);
        assert!(!s.after_capture.show_bar);
        assert!(!s.after_capture.copy, "the rest stays as chosen");
        let s = Settings::from_json(r#"{"afterCapture":{"showBar":true}}"#);
        assert!(s.after_capture.show_bar);
        assert_eq!(Settings::from_json(&serde_json::to_string(&s).unwrap()), s);
    }

    #[test]
    fn names_are_expanded_and_safe() {
        let at = Stamp { year: 2026, month: 10, day: 4, hour: 9, minute: 5, second: 7 };
        assert_eq!(expand_name("Capture {date} {time}", at, "", 1), "Capture 2026-10-04 09.05.07");
        assert_eq!(expand_name("{source} {n}", at, "C:\\Notes: <draft>?", 3), "C--Notes- -draft-- 3");
        assert_eq!(expand_name("  ...  ", at, "", 1), "Capture");
        assert_eq!(expand_name("{year}{month}{day}-{hour}{minute}{second}", at, "", 1), "20261004-090507");
    }

    #[test]
    fn formats_and_modes() {
        for f in ImageFormat::ALL {
            assert_eq!(ImageFormat::from_extension(f.extension()), Some(f));
        }
        assert_eq!(ImageFormat::from_extension(".JPEG"), Some(ImageFormat::Jpeg));
        for m in CaptureMode::ALL {
            assert_eq!(CaptureMode::from_arg(m.arg()), Some(m));
        }
        assert_eq!(CaptureMode::from_arg("nope"), None);
    }

    #[test]
    fn editor_settings() {
        let mut e = EditorSettings::from_json("{}");
        assert_eq!(e.style(Kind::Arrow), Kind::Arrow.default_style());
        for i in 0..30 {
            e.remember(&format!("f{i}.png"));
        }
        e.remember("f25.png");
        assert_eq!(e.recent.len(), EditorSettings::MAX_RECENT);
        assert_eq!(e.recent.first().map(String::as_str), Some("f25.png"));
    }
}
