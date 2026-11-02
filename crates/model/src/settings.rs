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
    /// An area recorded as a small video (MP4).
    Video,
    /// An area recorded with the computer's sound, the microphone and the camera, each on a
    /// track of its own, to edit in the editor.
    Production,
}

impl CaptureMode {
    pub const ALL: [CaptureMode; 9] = [
        CaptureMode::Region,
        CaptureMode::Window,
        CaptureMode::Screen,
        CaptureMode::AllScreens,
        CaptureMode::Repeat,
        CaptureMode::Scrolling,
        CaptureMode::Gif,
        CaptureMode::Video,
        CaptureMode::Production,
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
            CaptureMode::Video => "Record Video",
            CaptureMode::Production => "Video Production",
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
            CaptureMode::Video => "video",
            CaptureMode::Production => "production",
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
    pub video: Option<Hotkey>,
    pub production: Option<Hotkey>,
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
                video: k("V"),
                production: k("P"),
                editor: k("E"),
            }
        } else {
            Hotkeys {
                region: Some(Hotkey::new(false, false, false, false, "PrintScreen")),
                window: Some(Hotkey::new(false, true, false, false, "PrintScreen")),
                screen: Some(Hotkey::new(true, false, false, false, "PrintScreen")),
                // Ctrl+Shift+Print Screen is Windows' own; every other Print Screen key is taken.
                all_screens: None,
                repeat: Some(Hotkey::new(false, false, true, false, "PrintScreen")),
                scrolling: Some(Hotkey::new(false, true, true, false, "PrintScreen")),
                gif: Some(Hotkey::new(true, true, false, false, "PrintScreen")),
                video: Some(Hotkey::new(true, true, true, false, "PrintScreen")),
                production: None,
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
            CaptureMode::Video => self.video.as_ref(),
            CaptureMode::Production => self.production.as_ref(),
        }
    }

    /// The 0.1.9 changes, each only where the shortcut still is what it was by default before,
    /// so nothing the owner chose is lost: Record GIF moved to Ctrl+Alt+Print Screen and
    /// Scrolling Capture to Alt+Shift+Print Screen (they swapped), and Capture All Screens lost
    /// Ctrl+Shift+Print Screen, which Windows uses itself.
    fn bring_up_to_0_1_9(&mut self) {
        let old_gif = Some(Hotkey::new(false, true, true, false, "PrintScreen"));
        let old_scrolling = Some(Hotkey::new(true, true, false, false, "PrintScreen"));
        if self.gif == old_gif && self.scrolling == old_scrolling {
            std::mem::swap(&mut self.gif, &mut self.scrolling);
        }
        if self.all_screens == Some(Hotkey::new(true, false, true, false, "PrintScreen")) {
            self.all_screens = None;
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
            CaptureMode::Video => &mut self.video,
            CaptureMode::Production => &mut self.production,
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

/// What a video recording takes in besides the screen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VideoOptions {
    /// What the computer plays (Windows only).
    pub system_sound: bool,
    pub microphone: bool,
    /// The microphone by name; `None` is the system's default one.
    pub microphone_name: Option<String>,
    pub camera: bool,
    /// The camera by number; `None` is the first one.
    pub camera_index: Option<u32>,
}

impl Default for VideoOptions {
    /// A quick video: the screen and what the computer plays.
    fn default() -> VideoOptions {
        VideoOptions { system_sound: true, microphone: false, microphone_name: None, camera: false, camera_index: None }
    }
}

impl VideoOptions {
    /// Video Production: everything, each on its own track.
    pub fn production() -> VideoOptions {
        VideoOptions { system_sound: true, microphone: true, microphone_name: None, camera: true, camera_index: None }
    }
}

/// What a Video Production recording shows of the screen: the Recorder's Area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordArea {
    /// An area dragged out on the overlay.
    #[default]
    Region,
    /// A window clicked on the overlay: where it was then (it isn't followed if it moves).
    Window,
    /// One monitor, whole ([`RecorderOptions::monitor`]).
    Screen,
    /// Every monitor in one picture.
    AllScreens,
    /// A frame of a set size ([`RecorderOptions::width`] × [`RecorderOptions::height`] pixels),
    /// placed with a click on the overlay.
    Fixed,
}

impl RecordArea {
    pub const ALL: [RecordArea; 5] = [RecordArea::Region, RecordArea::Window, RecordArea::Screen, RecordArea::AllScreens, RecordArea::Fixed];

    pub fn label(self) -> &'static str {
        match self {
            RecordArea::Region => "Region",
            RecordArea::Window => "Window",
            RecordArea::Screen => "Full screen",
            RecordArea::AllScreens => "All screens",
            RecordArea::Fixed => "Fixed size",
        }
    }

    /// Whether the area is chosen on the overlay once Record is pressed.
    pub fn chosen_on_screen(self) -> bool {
        matches!(self, RecordArea::Region | RecordArea::Window | RecordArea::Fixed)
    }
}

/// A monitor as the Recorder remembers it: its name, and where its top left corner is on the
/// desktop (two monitors of the same model have the same name).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MonitorChoice {
    pub name: String,
    pub x: i32,
    pub y: i32,
}

impl MonitorChoice {
    /// Which of `monitors` (each its name and top left corner) this is: the one with its name
    /// where it was, else one with its name (moved), else one where it was (renamed); none when
    /// no monitor is like it.
    pub fn pick(&self, monitors: &[(&str, i32, i32)]) -> Option<usize> {
        let at = |m: &(&str, i32, i32)| m.1 == self.x && m.2 == self.y;
        let named = |m: &(&str, i32, i32)| m.0 == self.name;
        monitors.iter().position(|m| named(m) && at(m)).or_else(|| monitors.iter().position(named)).or_else(|| monitors.iter().position(at))
    }
}

/// The fixed sizes the Recorder offers, in pixels, and what each is for.
pub const FIXED_SIZES: [(u32, u32, &str); 4] =
    [(1920, 1080, "Full HD"), (1280, 720, "HD"), (1080, 1920, "vertical, for phones"), (1080, 1080, "square")];

/// The smallest and largest side of a fixed-size recording.
pub const FIXED_MIN: u32 = 64;
pub const FIXED_MAX: u32 = 7680;

/// The Recorder's choices for a Video Production recording: the area, and whether the screen is
/// recorded at all. What it takes in besides is [`Settings::production`] (the computer's sound,
/// the microphone and the camera).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RecorderOptions {
    pub area: RecordArea,
    /// Full screen: the monitor; none is the main one.
    pub monitor: Option<MonitorChoice>,
    /// Fixed size, in the monitor's pixels.
    pub width: u32,
    pub height: u32,
    /// Record the screen (off: only the camera and the sound).
    pub screen: bool,
}

impl Default for RecorderOptions {
    fn default() -> RecorderOptions {
        RecorderOptions { area: RecordArea::Region, monitor: None, width: 1920, height: 1080, screen: true }
    }
}

impl RecorderOptions {
    /// Every number in its range.
    pub fn clamped(mut self) -> RecorderOptions {
        self.width = self.width.clamp(FIXED_MIN, FIXED_MAX);
        self.height = self.height.clamp(FIXED_MIN, FIXED_MAX);
        self
    }

    /// Whether anything at all is recorded: the screen, or one of `sources`. What this system
    /// can't record (the computer's sound where `system_sound` is false, the camera where `camera`
    /// is) doesn't count.
    pub fn records_anything(&self, sources: &VideoOptions, system_sound: bool, camera: bool) -> bool {
        self.screen || (sources.camera && camera) || sources.microphone || (sources.system_sound && system_sound)
    }
}

/// The shortcuts' defaults as of this version: 1 is 0.1.9 (Record GIF on Ctrl+Alt+Print Screen, no
/// default for Capture All Screens).
pub const HOTKEYS_VERSION: u32 = 1;

/// The capture service's settings (`settings.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub hotkeys: Hotkeys,
    /// Which defaults the shortcuts were last brought up to (see [`Settings::from_json`]).
    pub hotkeys_version: u32,
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
    /// Frames a second of a GIF recording. It's `gifFrameRate` in the file: the old `gifFps` was
    /// written at every save, so it says nothing of what the owner chose, and is left behind.
    pub gif_frame_rate: u32,
    pub gif_max_secs: u32,
    /// A quick video (Record Video).
    pub video: VideoOptions,
    /// A Video Production recording: what it takes in besides the screen.
    pub production: VideoOptions,
    /// A Video Production recording: the Recorder's area, and whether the screen is in it.
    pub recorder: RecorderOptions,
    /// Frames a second of a video recording.
    pub video_frame_rate: u32,
    /// The longest a video recording runs, in minutes.
    pub video_max_mins: u32,
    /// Keep a quick video at the size recorded (else it is made no larger than 1920 × 1080).
    pub video_full_size: bool,
    pub theme: Theme,
    /// The area of the last region capture, for Repeat Last Capture.
    pub last_region: Option<IRect>,
    /// Whether the first-run window has been seen.
    pub welcomed: bool,
    /// Play a short sound when a capture is taken.
    pub sound: bool,
    /// Take an open editor out of the picture before a capture.
    pub hide_editor: bool,
    /// Ask GitHub for a newer release now and then (the only network request ScreenGrabCraft
    /// makes). A file from before it existed gets the default, on.
    #[serde(default = "on")]
    pub check_updates: bool,
}

fn on() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            hotkeys: Hotkeys::default(),
            hotkeys_version: HOTKEYS_VERSION,
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
            gif_frame_rate: 30,
            gif_max_secs: 60,
            video: VideoOptions::default(),
            production: VideoOptions::production(),
            recorder: RecorderOptions::default(),
            video_frame_rate: 30,
            video_max_mins: 60,
            video_full_size: false,
            theme: Theme::System,
            last_region: None,
            welcomed: false,
            sound: false,
            hide_editor: true,
            check_updates: true,
        }
    }
}

impl Settings {
    /// Reads settings, replacing what can't be read with defaults (field by field where the
    /// JSON is an object, wholesale where it isn't JSON at all).
    ///
    /// Every shortcut is written at every save, so a changed default wouldn't reach anyone who
    /// has a settings file. A file from before the defaults changed is brought up to them where
    /// it still has the old ones; a shortcut the owner chose is left as it is.
    pub fn from_json(text: &str) -> Settings {
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text) else {
            return Settings::default();
        };
        let version = map.get("hotkeysVersion").and_then(serde_json::Value::as_u64).unwrap_or(0);
        let mut s = Settings::from_map(map);
        if version < 1 {
            s.hotkeys.bring_up_to_0_1_9();
        }
        s.hotkeys_version = HOTKEYS_VERSION;
        s
    }

    fn from_map(map: serde_json::Map<String, serde_json::Value>) -> Settings {
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
        self.gif_frame_rate = self.gif_frame_rate.clamp(1, 30);
        self.gif_max_secs = self.gif_max_secs.clamp(1, 600);
        self.video_frame_rate = self.video_frame_rate.clamp(5, 30);
        self.video_max_mins = self.video_max_mins.clamp(1, 240);
        self.recorder = self.recorder.clamped();
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
    /// The drawing tool last picked, which the editor starts with. One that can't be read is
    /// let go, not the rest of the settings with it.
    #[serde(deserialize_with = "or_none")]
    pub tool: Option<Kind>,
    /// The stamp last picked.
    #[serde(deserialize_with = "or_none")]
    pub stamp: Option<crate::object::StampKind>,
}

/// A setting that can't be read is none.
fn or_none<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).ok())
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
    fn gif_is_ctrl_alt_print_screen_and_all_screens_leaves_windows_its_key() {
        let k = Hotkeys::defaults_for(false);
        assert_eq!(k.gif, Some(Hotkey::new(true, true, false, false, "PrintScreen")));
        assert_eq!(k.scrolling, Some(Hotkey::new(false, true, true, false, "PrintScreen")));
        assert_eq!(k.all_screens, None);
        assert_eq!(Settings::default().hotkeys_version, HOTKEYS_VERSION);
    }

    /// A settings file written before 0.1.9, with every shortcut as it was by default then.
    fn file_before_0_1_9(gif: &str) -> String {
        format!(
            r#"{{"hotkeys":{{"region":"PrintScreen","window":"Alt+PrintScreen","screen":"Ctrl+PrintScreen","allScreens":"Ctrl+Shift+PrintScreen","repeat":"Shift+PrintScreen","scrolling":"Ctrl+Alt+PrintScreen","gif":"{gif}"}},"delaySecs":5}}"#
        )
    }

    #[test]
    fn a_file_from_before_gets_the_new_shortcuts_where_they_were_the_old_defaults() {
        let s = Settings::from_json(&file_before_0_1_9("Alt+Shift+PrintScreen"));
        if cfg!(target_os = "macos") {
            return;
        }
        assert_eq!(s.hotkeys.gif, Some(Hotkey::new(true, true, false, false, "PrintScreen")));
        assert_eq!(s.hotkeys.scrolling, Some(Hotkey::new(false, true, true, false, "PrintScreen")));
        assert_eq!(s.hotkeys.all_screens, None);
        assert_eq!(s.delay_secs, 5);
        assert_eq!(s.hotkeys_version, HOTKEYS_VERSION);
        assert!(s.hotkeys.clashes().is_empty());
    }

    #[test]
    fn a_shortcut_the_owner_chose_is_kept() {
        // GIF on a key of the owner's own: neither GIF nor Scrolling moves.
        let s = Settings::from_json(&file_before_0_1_9("Ctrl+Shift+G"));
        assert_eq!(s.hotkeys.gif, Some(Hotkey::new(true, false, true, false, "G")));
        assert_eq!(s.hotkeys.scrolling, Some(Hotkey::new(true, true, false, false, "PrintScreen")));
        // Once brought up to date, the file's shortcuts are the owner's: nothing moves again.
        let text = serde_json::to_string(&Settings { hotkeys: Hotkeys::defaults_for(false), ..Settings::default() }).unwrap();
        let mut again = Settings::from_json(&text);
        again.hotkeys.all_screens = Some(Hotkey::new(true, false, true, false, "PrintScreen"));
        let text = serde_json::to_string(&again).unwrap();
        assert_eq!(Settings::from_json(&text).hotkeys.all_screens, Some(Hotkey::new(true, false, true, false, "PrintScreen")));
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
    fn a_file_from_before_smooth_recordings_gets_them() {
        assert_eq!(Settings::default().gif_frame_rate, 30);
        assert_eq!(Settings::from_json(r#"{"gifFps":12}"#).gif_frame_rate, 30);
        let s = Settings::from_json(r#"{"gifFrameRate":15}"#);
        assert_eq!(s.gif_frame_rate, 15, "a rate of its own is kept");
        assert_eq!(Settings::from_json(&serde_json::to_string(&s).unwrap()), s);
        assert_eq!(Settings::from_json(r#"{"gifFrameRate":500}"#).gif_frame_rate, 30);
    }

    #[test]
    fn update_checks_are_on_unless_turned_off() {
        assert!(Settings::default().check_updates);
        // A file from before the setting existed.
        assert!(Settings::from_json(r#"{"format":"jpeg","welcomed":true}"#).check_updates);
        assert!(serde_json::from_str::<Settings>(r#"{"welcomed":true}"#).unwrap().check_updates);
        let off = Settings::from_json(r#"{"checkUpdates":false}"#);
        assert!(!off.check_updates);
        assert_eq!(Settings::from_json(&serde_json::to_string(&off).unwrap()), off);
        assert!(Settings::from_json(r#"{"checkUpdates":"sometimes"}"#).check_updates, "unreadable: the default");
    }

    #[test]
    fn the_recorders_choices_are_kept_and_a_file_from_before_gets_the_defaults() {
        // A file from before the Recorder: the screen, an area dragged out, 1920 × 1080 for a fixed size.
        let old = Settings::from_json(r#"{"production":{"systemSound":false,"microphone":true,"camera":false}}"#);
        assert_eq!(old.recorder, RecorderOptions::default());
        assert!(old.recorder.screen && old.recorder.area == RecordArea::Region);
        assert!(old.production.microphone && !old.production.system_sound, "the rest is as chosen");
        let recorder = RecorderOptions {
            area: RecordArea::Fixed,
            monitor: Some(MonitorChoice { name: "DELL U2720Q".into(), x: -2560, y: 0 }),
            width: 1080,
            height: 1920,
            screen: false,
        };
        let s = Settings { recorder, ..Settings::default() };
        let back = Settings::from_json(&serde_json::to_string(&s).unwrap());
        assert_eq!(back.recorder, s.recorder);
        // Sizes are kept in range; an area this version doesn't know leaves the Recorder at its defaults only.
        let s = Settings::from_json(r#"{"recorder":{"area":"fixed","width":0,"height":99999},"delaySecs":7}"#);
        assert_eq!((s.recorder.width, s.recorder.height), (FIXED_MIN, FIXED_MAX));
        let s = Settings::from_json(r#"{"recorder":{"area":"hologram"},"delaySecs":7}"#);
        assert_eq!((s.recorder.clone(), s.delay_secs), (RecorderOptions::default(), 7));
        for a in RecordArea::ALL {
            assert!(!a.label().is_empty());
            assert_eq!(a.chosen_on_screen(), !matches!(a, RecordArea::Screen | RecordArea::AllScreens));
        }
    }

    #[test]
    fn a_remembered_monitor_is_found_again() {
        let c = MonitorChoice { name: "DELL".into(), x: 1920, y: 0 };
        let two_dells = [("DELL", 0, 0), ("DELL", 1920, 0), ("LG", 3840, 0)];
        assert_eq!(c.pick(&two_dells), Some(1), "the one where it was");
        assert_eq!(c.pick(&[("LG", 0, 0), ("DELL", -1920, 0)]), Some(1), "moved");
        assert_eq!(c.pick(&[("LG", 0, 0), ("Generic", 1920, 0)]), Some(1), "renamed");
        assert_eq!(c.pick(&[("LG", 0, 0)]), None);
        assert_eq!(c.pick(&[]), None);
    }

    #[test]
    fn something_must_be_recorded() {
        let none = VideoOptions { system_sound: false, microphone: false, microphone_name: None, camera: false, camera_index: None };
        let off = RecorderOptions { screen: false, ..RecorderOptions::default() };
        assert!(!off.records_anything(&none, true, true));
        assert!(RecordArea::ALL.iter().all(|a| RecorderOptions { area: *a, ..RecorderOptions::default() }.records_anything(&none, true, true)));
        assert!(off.records_anything(&VideoOptions { camera: true, ..none.clone() }, false, true));
        assert!(off.records_anything(&VideoOptions { microphone: true, ..none.clone() }, false, false));
        // The computer's sound and the camera count only where they can be recorded.
        let sound = VideoOptions { system_sound: true, ..none.clone() };
        assert!(off.records_anything(&sound, true, false));
        assert!(!off.records_anything(&sound, false, true));
        assert!(!off.records_anything(&VideoOptions { camera: true, ..none }, true, false));
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

    #[test]
    fn the_last_tool_is_remembered_and_a_bad_one_spoils_nothing() {
        assert_eq!(EditorSettings::from_json("{}").tool, None, "a file from before has none");
        let e =
            EditorSettings { tool: Some(Kind::Callout), stamp: Some(crate::object::StampKind::Cross), show_tray: true, ..EditorSettings::default() };
        assert_eq!(EditorSettings::from_json(&serde_json::to_string(&e).unwrap()), e);
        let e = EditorSettings::from_json(r#"{"tool":"teapot","stamp":7,"showTray":true}"#);
        assert_eq!((e.tool, e.stamp), (None, None));
        assert!(e.show_tray, "the rest is still read");
    }
}
