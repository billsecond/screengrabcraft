//! The capture service: the tray, the shortcuts, and every capture from the key press to the
//! saved file, the clipboard or the editor.
//!
//! The service's own (root) window is a one-pixel, see-through, click-through window that is
//! shown only while something is on screen (the capture overlay, the settings window, a
//! countdown, a notice), because eframe opens further windows only from a window that is shown.
//! Everything the owner sees is a window of its own.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use egui::{ViewportBuilder, ViewportId, pos2, vec2};
use screengrabcraft_capture::{Frozen, Monitor};
use screengrabcraft_desktop::{clipboard, instance, launch, paths, store};
use screengrabcraft_model::{CaptureMode, IRect, ImageFormat, Raster, Settings};
use screengrabcraft_ui::overlay::{Action, Outcome, Overlay, OverlayMonitor, OverlayWindow, Purpose};

use crate::hotkeys::{Registered, Target};
use crate::record::{self, Recording};
use crate::tray::{Item, Tray};

pub const APP_ID: &str = "io.github.billsecond.screengrabcraft";
pub static ICON: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/io.github.billsecond.screengrabcraft.png");

/// Things that happen elsewhere and are handled on the window's thread.
pub enum Msg {
    TrayClick,
    TrayDoubleClick,
    Menu(tray_icon::menu::MenuId),
    Hotkey(u32),
    Frozen(CaptureMode, Result<Frozen, String>),
    Picture(Raster, String),
    Saved(Result<Done, String>),
}

/// What became of a finished capture.
pub struct Done {
    pub saved: Option<PathBuf>,
    pub copied: bool,
    pub edited: bool,
    pub thumb: Option<Raster>,
}

pub struct OverlayRun {
    pub overlay: Overlay,
    pub mode: CaptureMode,
    pub monitors: Vec<Monitor>,
    /// Where the frozen pointer was (desktop units).
    pub pointer: Option<(i32, i32)>,
}

pub struct Countdown {
    pub mode: CaptureMode,
    pub until: Instant,
    /// The monitor the number is shown on (desktop units and scale).
    pub on: Option<(IRect, f32)>,
}

pub struct Notice {
    pub text: String,
    pub detail: String,
    pub path: Option<PathBuf>,
    pub until: Instant,
    pub thumb: Option<Raster>,
    pub texture: Option<egui::TextureHandle>,
    pub error: bool,
}

pub struct Service {
    pub settings: Settings,
    pub keys: Registered,
    pub tray: Option<Tray>,
    pub guard: instance::Guard,
    pub tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub ctx: egui::Context,
    pub home: Option<crate::home::Home>,
    pub overlay: Option<OverlayRun>,
    pub countdown: Option<Countdown>,
    /// A countdown just ended: the capture starts once its window has gone.
    pub after_countdown: Option<(CaptureMode, Instant)>,
    pub freezing: Option<CaptureMode>,
    pub notice: Option<Notice>,
    pub recording: Option<Recording>,
    /// The winit monitors, for placing the overlay on each: position, size and name.
    pub screens: Vec<((i32, i32), (u32, u32), String)>,
    root_shown: bool,
    pub quit: bool,
}

impl Service {
    pub fn new(ctx: egui::Context, guard: instance::Guard, settings: Settings) -> Service {
        let (tx, rx) = channel();
        let mut keys = Registered::new();
        keys.apply(&settings.hotkeys);
        let tray = Tray::new(&settings, &keys);
        // The tray, the menu and the shortcuts are answered here, and wake the window's loop.
        let (t, c) = (tx.clone(), ctx.clone());
        tray_icon::TrayIconEvent::set_event_handler(Some(move |e: tray_icon::TrayIconEvent| {
            let msg = match e {
                tray_icon::TrayIconEvent::Click { button: tray_icon::MouseButton::Left, button_state: tray_icon::MouseButtonState::Up, .. } => {
                    Some(Msg::TrayClick)
                }
                tray_icon::TrayIconEvent::DoubleClick { button: tray_icon::MouseButton::Left, .. } => Some(Msg::TrayDoubleClick),
                _ => None,
            };
            if let Some(m) = msg {
                let _ = t.send(m);
                c.request_repaint();
            }
        }));
        let (t, c) = (tx.clone(), ctx.clone());
        tray_icon::menu::MenuEvent::set_event_handler(Some(move |e: tray_icon::menu::MenuEvent| {
            let _ = t.send(Msg::Menu(e.id));
            c.request_repaint();
        }));
        let (t, c) = (tx.clone(), ctx.clone());
        global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(move |e: global_hotkey::GlobalHotKeyEvent| {
            if e.state == global_hotkey::HotKeyState::Pressed {
                let _ = t.send(Msg::Hotkey(e.id));
                c.request_repaint();
            }
        }));
        let wake = ctx.clone();
        guard.set_waker(Box::new(move || wake.request_repaint()));
        Service {
            settings,
            keys,
            tray: Some(tray),
            guard,
            tx,
            rx,
            ctx,
            home: None,
            overlay: None,
            countdown: None,
            after_countdown: None,
            freezing: None,
            notice: None,
            recording: None,
            screens: Vec::new(),
            root_shown: false,
            quit: false,
        }
    }

    /// Saves the settings and puts them to work (shortcuts, the tray menu's labels).
    pub fn apply_settings(&mut self) {
        self.settings = self.settings.clone().clamped();
        store::save_settings(&self.settings);
        self.keys.apply(&self.settings.hotkeys);
        if let Some(t) = &mut self.tray {
            t.rebuild(&self.settings, &self.keys);
        }
    }

    pub fn open_home(&mut self) {
        if self.home.is_none() {
            self.home = Some(crate::home::Home::default());
        }
        if let Some(h) = &mut self.home {
            h.focus = true;
        }
    }

    pub fn notify(&mut self, text: impl Into<String>, detail: impl Into<String>, path: Option<PathBuf>, thumb: Option<Raster>, error: bool) {
        let secs = if error { 8 } else { 5 };
        self.notice = Some(Notice {
            text: text.into(),
            detail: detail.into(),
            path,
            until: Instant::now() + Duration::from_secs(secs),
            thumb,
            texture: None,
            error,
        });
    }

    // ---- what arrives ----

    /// The arguments this start itself was given (the same as a hand-over).
    pub fn guard_args(&mut self, args: &[String]) {
        self.handle_args(args);
    }

    /// Arguments handed over by a later start (`--capture region`, `--settings`…).
    fn handle_args(&mut self, args: &[String]) {
        // `--wait <ms>`: the program asking hides itself first (the editor), so wait for it.
        let wait = args.iter().position(|a| a == "--wait").and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<u64>().ok()).map(|ms| ms.min(5_000));
        let mut it = args.iter();
        let mut acted = false;
        while let Some(a) = it.next() {
            match a.as_str() {
                "--capture" => {
                    acted = true;
                    match (it.next().and_then(|m| CaptureMode::from_arg(m)), wait) {
                        (Some(m), Some(ms)) if self.overlay.is_none() && self.after_countdown.is_none() => {
                            self.after_countdown = Some((m, Instant::now() + Duration::from_millis(ms)));
                        }
                        (Some(m), _) => self.start(m, 0),
                        (None, _) => self.notify("Unknown capture", "Use region, window, screen, all, repeat, scrolling or gif.", None, None, true),
                    }
                }
                "--wait" => {
                    let _ = it.next();
                }
                "--delay" => {
                    acted = true;
                    let secs = it.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(self.settings.delay_secs);
                    self.start(CaptureMode::Region, secs.min(60));
                }
                "--settings" => {
                    acted = true;
                    self.open_home();
                }
                "--quit" => {
                    acted = true;
                    self.quit = true;
                }
                "--noop" | "--tray" | "--background" => acted = true,
                _ => {}
            }
        }
        // Started again with nothing to do: the owner wants to see it.
        if !acted {
            self.open_home();
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::TrayClick => {
                if self.recording.is_some() {
                    self.stop_recording(false);
                } else {
                    self.start(CaptureMode::Region, 0);
                }
            }
            Msg::TrayDoubleClick => self.open_home(),
            Msg::Menu(id) => {
                let item = self.tray.as_ref().and_then(|t| t.item(&id));
                match item {
                    Some(Item::Capture(m)) => self.start(m, 0),
                    Some(Item::Delayed(s)) => self.start(CaptureMode::Region, s),
                    Some(Item::StopRecording) => self.stop_recording(false),
                    Some(Item::Editor) => {
                        if let Err(e) = launch::open_editor(&[]) {
                            self.notify("The editor could not be started", e, None, None, true);
                        }
                    }
                    Some(Item::Folder) => {
                        let d = paths::captures_dir(&self.settings);
                        let _ = std::fs::create_dir_all(&d);
                        let _ = launch::reveal(&d);
                    }
                    Some(Item::Settings) => self.open_home(),
                    Some(Item::Quit) => self.quit = true,
                    None => {}
                }
            }
            Msg::Hotkey(id) => match self.keys.target(id) {
                Some(Target::Capture(CaptureMode::Gif)) if self.recording.is_some() => self.stop_recording(false),
                Some(Target::Capture(CaptureMode::Scrolling)) if self.recording.is_some() => self.stop_recording(false),
                Some(Target::Capture(m)) => self.start(m, 0),
                Some(Target::Editor) => {
                    let _ = launch::open_editor(&[]);
                }
                None => {}
            },
            Msg::Frozen(mode, result) => {
                self.freezing = None;
                match result {
                    Ok(frozen) => self.frozen(mode, frozen),
                    Err(e) => self.notify("The screen could not be captured", e, None, None, true),
                }
            }
            Msg::Picture(pic, source) => self.finish(pic, &source, Action::Default),
            Msg::Saved(result) => match result {
                Ok(done) => {
                    if !done.edited {
                        let text = match (&done.saved, done.copied) {
                            (Some(_), true) => "Saved and copied",
                            (Some(_), false) => "Saved",
                            (None, true) => "Copied to the clipboard",
                            (None, false) => "Captured",
                        };
                        let detail = done.saved.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        self.notify(text, detail, done.saved.clone(), done.thumb, false);
                    }
                }
                Err(e) => self.notify("The capture could not be kept", e, None, None, true),
            },
        }
    }

    // ---- capturing ----

    /// Starts a capture of `mode`, after `delay` seconds.
    pub fn start(&mut self, mode: CaptureMode, delay: u32) {
        if self.overlay.is_some() || self.freezing.is_some() || self.countdown.is_some() || self.after_countdown.is_some() {
            return;
        }
        if self.recording.is_some() {
            self.notify("A recording is running", "Stop it first (the tray icon, or its shortcut).", None, None, true);
            return;
        }
        if screengrabcraft_capture::permission() == screengrabcraft_capture::Permission::Denied {
            screengrabcraft_capture::request_permission();
            self.notify(
                "ScreenGrabCraft may not record the screen yet",
                "Allow it in System Settings › Privacy & Security › Screen & System Audio Recording, then capture again.",
                None,
                None,
                true,
            );
            return;
        }
        if mode == CaptureMode::Repeat {
            match self.settings.last_region {
                Some(r) => self.repeat(r),
                None => self.start(CaptureMode::Region, delay),
            }
            return;
        }
        if delay > 0 {
            let on = screengrabcraft_capture::monitors().ok().and_then(|ms| {
                let p = screengrabcraft_capture::pointer();
                ms.iter().find(|m| p.is_some_and(|(x, y)| m.rect.contains(i64::from(x), i64::from(y)))).or(ms.first()).map(|m| (m.rect, m.scale))
            });
            self.countdown = Some(Countdown { mode, until: Instant::now() + Duration::from_secs(u64::from(delay)), on });
            return;
        }
        // The service's own windows go first, so they aren't in the picture.
        if self.home.is_some() || self.notice.is_some() {
            self.home = None;
            self.notice = None;
            self.after_countdown = Some((mode, Instant::now() + Duration::from_millis(300)));
            return;
        }
        self.freeze(mode);
    }

    fn freeze(&mut self, mode: CaptureMode) {
        self.freezing = Some(mode);
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let spawned = std::thread::Builder::new().name("freeze".into()).spawn(move || {
            let r = screengrabcraft_capture::freeze().map_err(|e| e.to_string());
            let _ = tx.send(Msg::Frozen(mode, r));
            ctx.request_repaint();
        });
        if let Err(e) = spawned {
            self.freezing = None;
            self.notify("The screen could not be captured", e.to_string(), None, None, true);
        }
    }

    fn repeat(&mut self, desktop: IRect) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let _ = std::thread::Builder::new().name("repeat".into()).spawn(move || {
            let result = (|| -> Result<Raster, String> {
                let monitors = screengrabcraft_capture::monitors().map_err(|e| e.to_string())?;
                let cx = i64::from(desktop.x) + i64::from(desktop.w / 2);
                let cy = i64::from(desktop.y) + i64::from(desktop.h / 2);
                let m = monitors.iter().find(|m| m.rect.contains(cx, cy)).ok_or("the last area is on no monitor now")?;
                let (info, pic) = screengrabcraft_capture::grab_monitor(m.id).map_err(|e| e.to_string())?;
                let px = info.to_pixels(desktop, pic.width(), pic.height()).ok_or("the last area is on no monitor now")?;
                pic.crop(px.x.max(0) as u32, px.y.max(0) as u32, px.w, px.h).ok_or_else(|| "the area is empty".to_string())
            })();
            match result {
                Ok(pic) => {
                    let _ = tx.send(Msg::Picture(pic, "Repeat".to_string()));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Saved(Err(e)));
                }
            }
            ctx.request_repaint();
        });
    }

    /// The screen is frozen: show the overlay, or keep a whole screen at once.
    fn frozen(&mut self, mode: CaptureMode, frozen: Frozen) {
        match mode {
            CaptureMode::Screen => {
                let Some((m, pic)) = frozen.current_monitor() else { return };
                let pic = self.with_pointer(pic, m, (pic.width(), pic.height()), (0, 0), frozen.pointer);
                self.settings.last_region = Some(m.rect);
                store::save_settings(&self.settings);
                self.finish(pic, &m.name, Action::Default);
            }
            CaptureMode::AllScreens => {
                let desktop = frozen.desktop();
                let pic = if frozen.monitors.len() == 1 {
                    frozen.monitors.first().map(|(m, p)| self.with_pointer(p, m, (p.width(), p.height()), (0, 0), frozen.pointer))
                } else {
                    frozen.crop(desktop)
                };
                match pic {
                    Some(p) => self.finish(p, "All screens", Action::Default),
                    None => self.notify("The screens could not be put together", "", None, None, true),
                }
            }
            _ => {
                let purpose = match mode {
                    CaptureMode::Window => Purpose::Window,
                    CaptureMode::Gif => Purpose::Gif,
                    CaptureMode::Scrolling => Purpose::Scrolling,
                    _ => Purpose::Picture,
                };
                let monitors: Vec<Monitor> = frozen.monitors.iter().map(|(m, _)| m.clone()).collect();
                let pointer = frozen.pointer;
                let om = frozen.monitors.into_iter().map(|(m, picture)| OverlayMonitor { rect: m.rect, picture, name: m.name }).collect();
                let ow = frozen
                    .windows
                    .into_iter()
                    .map(|w| OverlayWindow { rect: w.rect, title: if w.title.is_empty() { w.app } else { w.title } })
                    .collect();
                let mut overlay = Overlay::new(om, ow, purpose);
                overlay.preview = self.settings.after_capture.preview;
                overlay.magnifier = self.settings.magnifier;
                overlay.crosshair = self.settings.crosshair;
                self.overlay = Some(OverlayRun { overlay, mode, monitors, pointer });
            }
        }
    }

    /// The mouse pointer drawn into a capture of (part of) a monitor, when the settings ask for
    /// it. `pic` is cut from the monitor's picture (`full` its size) at `origin`.
    fn with_pointer(&self, pic: &Raster, m: &Monitor, full: (u32, u32), origin: (u32, u32), pointer: Option<(i32, i32)>) -> Raster {
        let (Some((x, y)), true) = (pointer, self.settings.include_cursor) else { return pic.clone() };
        if !m.rect.contains(i64::from(x), i64::from(y)) {
            return pic.clone();
        }
        let rx = full.0 as f32 / m.rect.w.max(1) as f32;
        let ry = full.1 as f32 / m.rect.h.max(1) as f32;
        let px = (x - m.rect.x) as f32 * rx - origin.0 as f32;
        let py = (y - m.rect.y) as f32 * ry - origin.1 as f32;
        // Pointers grow with the display's scale (on a Retina display the picture's own pixels say so).
        let size = if rx > 1.01 { rx } else { m.scale.max(1.0) };
        screengrabcraft_render::pointer::draw(pic, px, py, size).unwrap_or_else(|| pic.clone())
    }

    /// The overlay has an answer.
    fn overlay_done(&mut self) {
        let Some(run) = self.overlay.take() else { return };
        let Outcome::Chosen { monitor, area, desktop, action, title } = run.overlay.outcome().clone() else { return };
        let Some(m) = run.monitors.get(monitor).cloned() else { return };
        match run.mode {
            CaptureMode::Gif | CaptureMode::Scrolling => {
                let kind = if run.mode == CaptureMode::Gif { record::Kind::Gif } else { record::Kind::Scrolling };
                match record::start(kind, m.id, area, desktop, self.settings.gif_fps, self.settings.gif_max_secs, self.ctx.clone()) {
                    Ok(r) => {
                        self.recording = Some(r);
                        if let Some(t) = &mut self.tray {
                            t.set_recording(true);
                        }
                    }
                    Err(e) => self.notify("The recording could not start", e, None, None, true),
                }
            }
            _ => {
                let Some(mon) = run.overlay.monitors.get(monitor) else { return };
                let origin = (area.x.max(0) as u32, area.y.max(0) as u32);
                let Some(pic) = mon.picture.crop(origin.0, origin.1, area.w, area.h) else { return };
                let pic = self.with_pointer(&pic, &m, (mon.picture.width(), mon.picture.height()), origin, run.pointer);
                self.settings.last_region = Some(desktop);
                store::save_settings(&self.settings);
                self.finish(pic, title.as_deref().unwrap_or("Region"), action);
            }
        }
    }

    /// Keeps a capture: saved, copied and/or opened in the editor, as the action and the
    /// settings say. The file work is done on a worker thread.
    pub fn finish(&mut self, pic: Raster, source: &str, action: Action) {
        let a = self.settings.after_capture;
        let (save, copy, edit) = match action {
            Action::Default | Action::Start => {
                let none = !a.save && !a.copy && !a.open_editor;
                (a.save, a.copy, a.open_editor || none)
            }
            Action::Edit => (a.save, false, true),
            Action::Copy => (a.save, true, false),
            Action::Save => (true, false, false),
        };
        let settings = self.settings.clone();
        let source = source.to_string();
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        // The clipboard is served from this thread's handle.
        let copied = copy
            && match clipboard::copy_image(&pic) {
                Ok(()) => true,
                Err(e) => {
                    log::warn!("copying the capture: {e}");
                    false
                }
            };
        let _ = std::thread::Builder::new().name("keep".into()).spawn(move || {
            let r = keep(&pic, &source, &settings, save, edit, copied);
            let _ = tx.send(Msg::Saved(r));
            ctx.request_repaint();
        });
    }

    pub fn stop_recording(&mut self, cancel: bool) {
        if let Some(r) = &self.recording {
            if cancel {
                r.cancel();
            } else {
                r.stop();
            }
        }
    }

    fn poll_recording(&mut self) {
        let Some(rec) = self.recording.as_mut() else { return };
        let Some(result) = rec.take() else { return };
        self.recording = None;
        if let Some(t) = &mut self.tray {
            t.set_recording(false);
        }
        match result {
            Ok(record::Output::Gif(tmp, frames)) => {
                let dir = paths::captures_dir(&self.settings);
                let name =
                    screengrabcraft_model::settings::expand_name(&self.settings.file_name, screengrabcraft_desktop::time::now(), "Recording", 1);
                match place(&tmp, &dir, &name, "gif") {
                    Ok(path) => self.notify("GIF saved", format!("{frames} frames — {}", file_name(&path)), Some(path), None, false),
                    Err(e) => self.notify("The GIF could not be saved", e, None, None, true),
                }
            }
            Ok(record::Output::Picture(p)) => self.finish(p, "Scrolling capture", Action::Default),
            Ok(record::Output::Cancelled) => {}
            Err(e) => self.notify("The recording failed", e, None, None, true),
        }
    }

    // ---- the frame ----

    pub fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        for args in self.guard.handed() {
            self.handle_args(&args);
        }
        while let Ok(msg) = self.rx.try_recv() {
            self.handle(msg);
        }
        if let Some(c) = &self.countdown
            && Instant::now() >= c.until
        {
            let mode = c.mode;
            self.countdown = None;
            self.after_countdown = Some((mode, Instant::now() + Duration::from_millis(250)));
        }
        if let Some((mode, at)) = self.after_countdown
            && Instant::now() >= at
        {
            self.after_countdown = None;
            self.start(mode, 0);
        }
        if self.overlay.as_ref().is_some_and(|o| *o.overlay.outcome() != Outcome::Pending) {
            self.overlay_done();
        }
        self.poll_recording();
        if self.notice.as_ref().is_some_and(|n| Instant::now() >= n.until) {
            self.notice = None;
        }
        if self.overlay.is_some() && self.screens.is_empty() {
            self.screens = winit_screens(frame);
        }
        if self.overlay.is_none() {
            self.screens.clear();
        }
        // The anchor window is shown while anything else is.
        let need = self.home.is_some() || self.overlay.is_some() || self.countdown.is_some() || self.notice.is_some() || self.recording.is_some();
        if need != self.root_shown {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(need));
            self.root_shown = need;
        }
        // Timers: countdown seconds, notices and recordings tick without input.
        if self.countdown.is_some() || self.recording.is_some() || self.after_countdown.is_some() || self.freezing.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if let Some(n) = &self.notice {
            ctx.request_repaint_after(n.until.saturating_duration_since(Instant::now()));
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            // The system asks the service to close (signing out, an installer updating it).
            self.quit = true;
        }
        if self.quit {
            if let Some(r) = &self.recording {
                r.stop();
            }
            self.guard.close();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if let Some(run) = self.overlay.as_mut() {
            for i in 0..run.monitors.len() {
                let Some(m) = run.monitors.get(i) else { continue };
                let builder = overlay_builder(m, &self.screens);
                ctx.show_viewport_immediate(ViewportId::from_hash_of(("overlay", i)), builder, |ui, _| run.overlay.show_monitor(ui, i));
            }
        }
        crate::popups::countdown(self, &ctx);
        crate::popups::recording(self, &ctx);
        crate::popups::notice(self, &ctx);
        crate::home::show(self, &ctx);
    }
}

/// The monitors as winit has them (for borderless full screen on the right one).
fn winit_screens(frame: &eframe::Frame) -> Vec<((i32, i32), (u32, u32), String)> {
    let Some(w) = frame.winit_window() else { return Vec::new() };
    w.available_monitors().map(|m| ((m.position().x, m.position().y), (m.size().width, m.size().height), m.name().unwrap_or_default())).collect()
}

/// A window covering one monitor exactly, above everything.
fn overlay_builder(m: &Monitor, screens: &[((i32, i32), (u32, u32), String)]) -> ViewportBuilder {
    let b = ViewportBuilder::default()
        .with_title("ScreenGrabCraft capture")
        .with_app_id(APP_ID)
        .with_decorations(false)
        .with_resizable(false)
        .with_taskbar(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
        .with_active(true);
    if cfg!(target_os = "macos") {
        // Desktop units are points there, as egui's are; full screen would slide to a Space.
        return b.with_position(pos2(m.rect.x as f32, m.rect.y as f32)).with_inner_size(vec2(m.rect.w as f32, m.rect.h as f32));
    }
    let by_place = screens.iter().position(|((x, y), (w, h), _)| *x == m.rect.x && *y == m.rect.y && *w == m.rect.w && *h == m.rect.h);
    let by_name = screens.iter().position(|(_, _, n)| !n.is_empty() && *n == m.name);
    match by_place.or(by_name) {
        Some(i) => b.with_monitor(i),
        None => {
            let s = m.scale.max(0.5);
            b.with_position(pos2(m.rect.x as f32 / s, m.rect.y as f32 / s)).with_inner_size(vec2(m.rect.w as f32 / s, m.rect.h as f32 / s))
        }
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// A free name in `dir`: `name.ext`, else `name (2).ext`, …
pub fn free_path(dir: &Path, name: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{name}.{ext}"));
    if !first.exists() {
        return first;
    }
    for n in 2..10_000 {
        let p = dir.join(format!("{name} ({n}).{ext}"));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{name} {}.{ext}", std::process::id()))
}

/// Moves a finished temporary file into the captures folder.
fn place(tmp: &Path, dir: &Path, name: &str, ext: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dest = free_path(dir, name, ext);
    if std::fs::rename(tmp, &dest).is_err() {
        std::fs::copy(tmp, &dest).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(tmp);
    }
    Ok(dest)
}

/// The file work of keeping a capture (on a worker thread).
fn keep(pic: &Raster, source: &str, s: &Settings, save: bool, edit: bool, copied: bool) -> Result<Done, String> {
    let name = screengrabcraft_model::settings::expand_name(&s.file_name, screengrabcraft_desktop::time::now(), source, 1);
    let format = if s.format == ImageFormat::Gif || s.format == ImageFormat::Pdf && edit { ImageFormat::Png } else { s.format };
    let mut saved = None;
    let mut path_for_editor = None;
    if save || edit {
        let dir = if save { paths::captures_dir(s) } else { std::env::temp_dir().join("ScreenGrabCraft") };
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        // The editor opens pictures, not PDFs.
        let fmt = if edit && format == ImageFormat::Pdf { ImageFormat::Png } else { format };
        let bytes = screengrabcraft_render::encode(pic, fmt, s.jpeg_quality).map_err(|e| e.to_string())?;
        let path = free_path(&dir, &name, fmt.extension());
        store::write_atomic(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        log::info!("capture {}x{} kept as {}", pic.width(), pic.height(), path.display());
        if save {
            saved = Some(path.clone());
        }
        path_for_editor = Some(path);
    }
    let mut edited = false;
    if edit && let Some(p) = &path_for_editor {
        match launch::open_editor(&[p.to_string_lossy().to_string()]) {
            Ok(()) => edited = true,
            Err(e) => log::warn!("opening the editor: {e}"),
        }
    }
    let thumb = {
        let f = (96.0 / pic.width() as f32).min(64.0 / pic.height() as f32).min(1.0);
        screengrabcraft_render::effects::resize(pic, ((pic.width() as f32 * f) as u32).max(1), ((pic.height() as f32 * f) as u32).max(1)).ok()
    };
    Ok(Done { saved, copied, edited, thumb })
}
