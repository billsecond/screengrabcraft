//! The icon in the system tray (the menu bar on macOS) and its menu.
//!
//! Windows and macOS draw it through the system; Linux and the BSDs through the
//! StatusNotifierItem protocol over D-Bus (KDE, Xfce, Cinnamon, MATE, LXQt and others show it;
//! GNOME needs the AppIndicator extension). When there is no tray at all the service still
//! runs: its shortcuts work, and starting it again opens its window.

use screengrabcraft_desktop::tray_icon::{Look, pixels};
use screengrabcraft_model::{CaptureMode, Settings};
use tray_icon::menu::{Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};
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
    Quit,
}

pub struct Tray {
    icon: Option<TrayIcon>,
    items: Vec<(MenuId, Item)>,
    gif_item: Option<MenuItem>,
    recording: bool,
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
        let mut t = Tray { icon: None, items: Vec::new(), gif_item: None, recording: false };
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

    fn add(&mut self, m: &Menu, text: String, item: Item) -> MenuItem {
        let mi = MenuItem::new(text, true, None);
        self.items.push((mi.id().clone(), item));
        let _ = m.append(&mi);
        mi
    }

    fn menu(&mut self, settings: &Settings, keys: &Registered) -> Menu {
        let menu = Menu::new();
        for mode in [CaptureMode::Region, CaptureMode::Window, CaptureMode::Screen, CaptureMode::AllScreens, CaptureMode::Scrolling] {
            self.add(&menu, label(mode.label(), keys.label(&settings.hotkeys, mode)), Item::Capture(mode));
        }
        let gif = self.add(&menu, label(CaptureMode::Gif.label(), keys.label(&settings.hotkeys, CaptureMode::Gif)), Item::Capture(CaptureMode::Gif));
        self.gif_item = Some(gif);
        self.add(&menu, label(CaptureMode::Repeat.label(), keys.label(&settings.hotkeys, CaptureMode::Repeat)), Item::Capture(CaptureMode::Repeat));
        let delayed = Submenu::new("Capture Region After a Delay", true);
        for secs in [3u32, 5, 10] {
            let mi = MenuItem::new(format!("{secs} seconds"), true, None);
            self.items.push((mi.id().clone(), Item::Delayed(secs)));
            let _ = delayed.append(&mi);
        }
        let _ = menu.append(&delayed);
        let _ = menu.append(&PredefinedMenuItem::separator());
        self.add(&menu, "Open Editor".to_string(), Item::Editor);
        self.add(&menu, "Open Captures Folder".to_string(), Item::Folder);
        self.add(&menu, "Settings…".to_string(), Item::Settings);
        let _ = menu.append(&PredefinedMenuItem::separator());
        self.add(&menu, "Quit ScreenGrabCraft".to_string(), Item::Quit);
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

    /// Shows that a recording is running (a red dot, and the GIF item stops it).
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
        if let Some(g) = &self.gif_item {
            g.set_text(if on { "Stop Recording" } else { CaptureMode::Gif.label() });
        }
        // The GIF item's meaning follows.
        for (_, it) in self.items.iter_mut() {
            if on && *it == Item::Capture(CaptureMode::Gif) {
                *it = Item::StopRecording;
            } else if !on && *it == Item::StopRecording {
                *it = Item::Capture(CaptureMode::Gif);
            }
        }
    }
}
