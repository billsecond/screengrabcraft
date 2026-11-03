//! The icon in the system tray (the menu bar on macOS) and its menu.
//!
//! Windows and macOS draw it through the system; Linux and the BSDs through the
//! StatusNotifierItem protocol over D-Bus (KDE, Xfce, Cinnamon, MATE, LXQt and others show it;
//! GNOME needs the AppIndicator extension). When there is no tray at all the service still
//! runs: its shortcuts work, and starting it again opens its window.
//!
//! The menu ([`entries`]): the captures; then video in a section of its own, Video Production…
//! marked with a red record dot (it opens the Recorder) and Record Video; then the editor, the
//! folder, the settings and updates; then Quit.

use screengrabcraft_desktop::tray_icon::{Look, pixels, record_dot};
use screengrabcraft_model::{CaptureMode, Settings};
use tray_icon::menu::{IconMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::hotkeys::Registered;

/// What a menu item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Capture(CaptureMode),
    Delayed(u32),
    Editor,
    Folder,
    Settings,
    StopRecording,
    CheckUpdates,
    InstallUpdate,
    Quit,
}

/// One entry of the menu, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// An item: its words and what it does.
    Item(String, Item),
    /// An item marked with the red record dot (Video Production…).
    Marked(String, Item),
    /// The submenu of captures after a delay.
    Delays,
    Separator,
}

/// What Video Production's item says: it opens a window, hence the dots.
pub const PRODUCTION_ITEM: &str = "Video Production…";

/// The menu's entries, top to bottom: `shortcut` is a capture's shortcut as the menu shows it,
/// `update` the update waiting.
pub fn entries(shortcut: &dyn Fn(CaptureMode) -> Option<String>, update: Option<&str>) -> Vec<Entry> {
    let capture = |m: CaptureMode| Entry::Item(label(m.label(), shortcut(m)), Item::Capture(m));
    let mut e: Vec<Entry> = [
        CaptureMode::Region,
        CaptureMode::Window,
        CaptureMode::Screen,
        CaptureMode::AllScreens,
        CaptureMode::Scrolling,
        CaptureMode::Gif,
        CaptureMode::Repeat,
    ]
    .into_iter()
    .map(capture)
    .collect();
    e.push(Entry::Delays);
    // Video, a section of its own.
    e.push(Entry::Separator);
    e.push(Entry::Marked(label(PRODUCTION_ITEM, shortcut(CaptureMode::Production)), Item::Capture(CaptureMode::Production)));
    e.push(capture(CaptureMode::Video));
    e.push(Entry::Separator);
    e.push(Entry::Item("Open Editor".to_string(), Item::Editor));
    e.push(Entry::Item("Open Captures Folder".to_string(), Item::Folder));
    e.push(Entry::Item("Settings…".to_string(), Item::Settings));
    e.extend(crate::update::tray_items(update).into_iter().map(|(text, item)| Entry::Item(text, item)));
    e.push(Entry::Separator);
    e.push(Entry::Item("Quit ScreenGrabCraft".to_string(), Item::Quit));
    e
}

/// Video Production's item: with its red dot, or plain where the dot can't be made.
enum Marked {
    Icon(IconMenuItem),
    Plain(MenuItem),
}

impl Marked {
    fn set_enabled(&self, on: bool) {
        match self {
            Marked::Icon(i) => i.set_enabled(on),
            Marked::Plain(i) => i.set_enabled(on),
        }
    }
}

/// An item whose words and meaning change while a recording runs: it becomes Stop Recording.
struct Stopper {
    item: MenuItem,
    /// Its words, and what it does, when nothing is recording.
    text: String,
    meaning: Item,
}

pub struct Tray {
    icon: Option<TrayIcon>,
    items: Vec<(MenuId, Item)>,
    /// Record GIF and Record Video: each becomes Stop Recording while a recording runs.
    stoppers: Vec<Stopper>,
    /// Video Production…: not while a recording runs.
    production: Option<Marked>,
    recording: bool,
    /// The update waiting, as its item says it (`Install Update 0.1.8`); see `crate::update`.
    pub update: Option<String>,
}

fn icon(look: Look) -> Option<Icon> {
    let size = if cfg!(target_os = "macos") { 36 } else { 64 };
    Icon::from_rgba(pixels(size, look), size, size).ok()
}

fn label(text: &str, shortcut: Option<String>) -> String {
    match shortcut {
        Some(s) if cfg!(target_os = "windows") => format!("{text}\t{s}"),
        Some(s) => format!("{text}  ({s})"),
        None => text.to_string(),
    }
}

impl Tray {
    /// Builds the tray icon and menu (on the main thread, with the event loop running).
    pub fn new(settings: &Settings, keys: &Registered) -> Tray {
        let mut t = Tray { icon: None, items: Vec::new(), stoppers: Vec::new(), production: None, recording: false, update: None };
        let menu = t.menu(settings, keys);
        let look = if cfg!(target_os = "macos") { Look::Template } else { Look::Normal };
        let mut b = TrayIconBuilder::new()
            .with_tooltip("ScreenGrabCraft — capture the screen")
            .with_menu(Box::new(menu))
            // A click on the icon captures an area (the menu is a right-click away), except on
            // macOS, where a menu-bar icon opens its menu.
            .with_menu_on_left_click(cfg!(target_os = "macos"));
        if let Some(i) = icon(look) {
            #[cfg(target_os = "macos")]
            {
                b = b.with_icon_templated(i);
            }
            #[cfg(not(target_os = "macos"))]
            {
                b = b.with_icon(i);
            }
        }
        match b.build() {
            Ok(i) => t.icon = Some(i),
            Err(e) => log::warn!("no tray icon: {e}"),
        }
        t
    }

    pub fn available(&self) -> bool {
        self.icon.is_some()
    }

    fn menu(&mut self, settings: &Settings, keys: &Registered) -> Menu {
        let menu = Menu::new();
        self.stoppers.clear();
        self.production = None;
        let shortcut = |m: CaptureMode| keys.label(&settings.hotkeys, m);
        for entry in entries(&shortcut, self.update.as_deref()) {
            match entry {
                Entry::Item(text, item) => {
                    let mi = MenuItem::new(&text, true, None);
                    self.items.push((mi.id().clone(), item));
                    let _ = menu.append(&mi);
                    if matches!(item, Item::Capture(CaptureMode::Gif | CaptureMode::Video)) {
                        self.stoppers.push(Stopper { item: mi, text, meaning: item });
                    }
                }
                Entry::Marked(text, item) => {
                    // The dot at the size menus draw their icons (the system scales it).
                    let size = 32;
                    let marked = match tray_icon::menu::Icon::from_rgba(record_dot(size), size, size) {
                        Ok(dot) => {
                            let mi = IconMenuItem::new(&text, true, Some(dot), None);
                            self.items.push((mi.id().clone(), item));
                            let _ = menu.append(&mi);
                            Marked::Icon(mi)
                        }
                        Err(e) => {
                            log::warn!("the video item has no dot: {e}");
                            let mi = MenuItem::new(&text, true, None);
                            self.items.push((mi.id().clone(), item));
                            let _ = menu.append(&mi);
                            Marked::Plain(mi)
                        }
                    };
                    self.production = Some(marked);
                }
                Entry::Delays => {
                    let delayed = Submenu::new("Capture Region After a Delay", true);
                    for secs in [3u32, 5, 10] {
                        let mi = MenuItem::new(format!("{secs} seconds"), true, None);
                        self.items.push((mi.id().clone(), Item::Delayed(secs)));
                        let _ = delayed.append(&mi);
                    }
                    let _ = menu.append(&delayed);
                }
                Entry::Separator => {
                    let _ = menu.append(&PredefinedMenuItem::separator());
                }
            }
        }
        menu
    }

    /// Builds the menu again (the shortcuts changed).
    pub fn rebuild(&mut self, settings: &Settings, keys: &Registered) {
        self.items.clear();
        let menu = self.menu(settings, keys);
        if let Some(i) = &self.icon {
            i.set_menu(Some(Box::new(menu)));
        }
        let r = self.recording;
        self.recording = !r;
        self.set_recording(r);
    }

    pub fn item(&self, id: &MenuId) -> Option<Item> {
        self.items.iter().find(|(i, _)| i == id).map(|(_, it)| *it)
    }

    /// Shows that a recording is running: a red dot, Record GIF and Record Video stop it, and
    /// Video Production… waits until it's over.
    pub fn set_recording(&mut self, on: bool) {
        if self.recording == on {
            return;
        }
        self.recording = on;
        if let Some(i) = &self.icon {
            #[cfg(target_os = "macos")]
            let _ = if on { i.set_icon(icon(Look::Recording)) } else { i.set_icon_templated(icon(Look::Template)) };
            #[cfg(not(target_os = "macos"))]
            let _ = i.set_icon(icon(if on { Look::Recording } else { Look::Normal }));
            let _ = i.set_tooltip(Some(if on { "ScreenGrabCraft — recording (click to stop)" } else { "ScreenGrabCraft — capture the screen" }));
        }
        for s in &self.stoppers {
            s.item.set_text(if on { "Stop Recording" } else { s.text.as_str() });
            // Its meaning follows its words.
            let meaning = if on { Item::StopRecording } else { s.meaning };
            if let Some((_, it)) = self.items.iter_mut().find(|(id, _)| id == s.item.id()) {
                *it = meaning;
            }
        }
        if let Some(p) = &self.production {
            p.set_enabled(!on);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Vec<Entry> {
        let shortcut = |m: CaptureMode| (m == CaptureMode::Video).then(|| "Ctrl+Alt+Shift+PrintScreen".to_string());
        entries(&shortcut, None)
    }

    fn words(e: &Entry) -> Option<&str> {
        match e {
            Entry::Item(t, _) | Entry::Marked(t, _) => Some(t.as_str()),
            Entry::Delays | Entry::Separator => None,
        }
    }

    #[test]
    fn video_has_a_section_of_its_own_with_production_marked() {
        let m = menu();
        let at = |what: Item| m.iter().position(|e| matches!(e, Entry::Item(_, i) | Entry::Marked(_, i) if *i == what));
        let production = at(Item::Capture(CaptureMode::Production)).unwrap();
        let video = at(Item::Capture(CaptureMode::Video)).unwrap();
        // [separator] Video Production… / Record Video [separator]
        assert_eq!(video, production + 1);
        assert_eq!(m.get(production - 1), Some(&Entry::Separator));
        assert_eq!(m.get(video + 1), Some(&Entry::Separator));
        assert_eq!(m.get(production), Some(&Entry::Marked(PRODUCTION_ITEM.to_string(), Item::Capture(CaptureMode::Production))));
        assert!(words(&m[video]).unwrap().starts_with("Record Video") && words(&m[video]).unwrap().contains("Ctrl+Alt+Shift+PrintScreen"));
        // Only Video Production is marked, and the captures come before the video section.
        assert_eq!(m.iter().filter(|e| matches!(e, Entry::Marked(..))).count(), 1);
        let gif = at(Item::Capture(CaptureMode::Gif)).unwrap();
        let region = at(Item::Capture(CaptureMode::Region)).unwrap();
        assert!(region == 0 && gif < production && at(Item::Capture(CaptureMode::Repeat)).unwrap() < production);
        assert!(m.iter().position(|e| *e == Entry::Delays).unwrap() < production);
        // Every capture has its item, once.
        for mode in CaptureMode::ALL {
            assert_eq!(m.iter().filter(|e| matches!(e, Entry::Item(_, i) | Entry::Marked(_, i) if *i == Item::Capture(mode))).count(), 1, "{mode:?}");
        }
        // Quit last, set apart.
        assert_eq!(m.last(), Some(&Entry::Item("Quit ScreenGrabCraft".to_string(), Item::Quit)));
        assert_eq!(m.get(m.len() - 2), Some(&Entry::Separator));
        // No two separators together.
        assert!(m.windows(2).all(|w| !(w[0] == Entry::Separator && w[1] == Entry::Separator)));
    }

    #[test]
    fn an_update_waiting_has_its_item_with_the_settings() {
        let m = entries(&|_| None, Some("Install Update 9.9.9"));
        let settings = m.iter().position(|e| words(e) == Some("Settings…")).unwrap();
        assert_eq!(m.get(settings + 1), Some(&Entry::Item("Install Update 9.9.9".to_string(), Item::InstallUpdate)));
    }
}
