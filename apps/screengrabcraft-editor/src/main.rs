//! ScreenGrabCraft Editor: annotate, mark up and share screenshots.
//!
//! Usage: `screengrabcraft-editor [--video] [--version] [pictures, .sgcraft projects or recordings…]`
//!
//! One editor window at a time: started again (a capture handed over by the capture service, a
//! file opened from the file manager), it hands the files to the window already open and exits.
//!
//! With `--video` it is the video editor: the same program, a window of its own for recordings
//! beside the picture editor's (its own name, place on the screen, log and taskbar button). Each
//! hands the other kind of file to the other, so a capture never takes the place of the video
//! being edited. The video editor never steps aside for a capture.
//!
//! The capture service also has the window step aside before a capture ([`Aside`]): on Windows
//! it is cloaked, there but not seen, at once and without Windows' animations; elsewhere hidden.
//! It never outlasts the service that sent it aside, and a program asking for the editor (the
//! Start menu, the tray) brings it back too.
//!
//! On Windows the service keeps an editor ready out of sight (`--standby`, see
//! [`launch::ready_editor`]): started hidden soon after the service, and hidden again rather than
//! closed when its window is closed, so a capture never waits for the editor to start. It ends
//! with the service, or when the owner turns that off.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod narration;
mod place;
mod services;
mod video;

use std::time::Duration;

use screengrabcraft_desktop::{cloak, instance, launch, logging, paths, store, version};
use screengrabcraft_ui::{Editor, EditorKind};

const APP_ID: &str = "io.github.billsecond.screengrabcraft.editor";
/// The window's place, size and state, in the settings folder (written by eframe).
const WINDOW_FILE: &str = "editor-window.ron";
/// The video editor's: its own window, its own place.
const VIDEO_WINDOW_FILE: &str = "video-editor-window.ron";
/// The video editor's taskbar button, apart from the picture editor's (Windows).
const VIDEO_APP_ID: &str = "io.github.billsecond.screengrabcraft.video";
static ICON: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/io.github.billsecond.screengrabcraft.editor.png");

/// How often a thread looks for the capture service, to wake a window out of sight (hidden for a
/// capture, or kept ready) once the service has gone. The window itself asks for no frames while
/// out of sight: eframe draws a hidden window ten times a second for as long as one is asked for.
const WATCH: Duration = Duration::from_secs(3);

/// What to do with the window.
#[derive(Debug, PartialEq)]
enum Move {
    /// Out of the picture for a capture.
    Park,
    /// Back as it was once the capture is over (no picture came of it for the editor).
    Unpark {
        focus: bool,
    },
    Stay,
}

/// The window out of the picture for a capture, and whether the owner was working in it. A
/// window the owner minimized isn't in the picture, and stays minimized; one kept ready out of
/// sight steps aside too, shown but cloaked, so that it appears at once if the capture comes to it.
#[derive(Default)]
struct Aside {
    parked: bool,
    focused: bool,
}

impl Aside {
    /// The capture service's word.
    fn hear(&mut self, word: &str, minimized: bool, focused: bool, standby: bool) -> Move {
        match word {
            launch::EDITOR_HIDE if (standby || !minimized) && !self.parked => {
                *self = Aside { parked: true, focused: focused && !standby };
                Move::Park
            }
            launch::EDITOR_SHOW if self.parked => self.back(),
            _ => Move::Stay,
        }
    }

    /// Back where it was, with the keyboard only if it had it: over another program it would
    /// take it just as the capture is about to be pasted there.
    fn back(&mut self) -> Move {
        let focus = self.focused;
        *self = Aside::default();
        Move::Unpark { focus }
    }
}

/// Whether what is handed to the window (`args`) brings it back to the owner: shown, restored and
/// with the keyboard. It always does, but for a capture or a recording from the capture service
/// while the owner has the window minimized: that is opened in it, and it stays minimized. Kept
/// ready out of sight it's hidden, not minimized, and comes back with the capture.
fn brought_back(args: &[String], minimized: bool, standby: bool) -> bool {
    standby || !minimized || !args.iter().any(|a| a == launch::FROM_CAPTURE)
}

/// Whether the window opens again what it last had open as it comes to the owner with `args`:
/// when it's given nothing to open (the Start menu, the tray's Open Editor, a desktop icon) and
/// nothing is open in it (`fresh`: it has just started, or was kept ready out of sight). A
/// window already on the screen just comes forward; a capture or a recording from the service
/// is what opens instead; and an editor started out of sight to be kept ready opens nothing
/// until it's brought to the owner, so that it never shows an old file as a capture comes.
fn opens_the_last_file(args: &[String], fresh: bool) -> bool {
    fresh && !args.iter().any(|a| !a.starts_with("--") || a == launch::FROM_CAPTURE || a == launch::EDITOR_STANDBY)
}

/// The window's taskbar button, there or not (Windows; elsewhere the taskbar has its own ways).
#[cfg(target_os = "windows")]
fn taskbar_button(frame: &eframe::Frame, shown: bool) {
    use winit::platform::windows::WindowExtWindows as _;
    if let Some(w) = frame.winit_window() {
        w.set_skip_taskbar(!shown);
    }
}

#[cfg(not(target_os = "windows"))]
fn taskbar_button(_frame: &eframe::Frame, _shown: bool) {}

struct App {
    editor: Editor,
    guard: instance::Guard,
    /// The editor's settings, shared with the other editor's window.
    settings_file: std::sync::Arc<store::SharedEditorSettings>,
    aside: Aside,
    /// The window is cloaked, having stepped aside.
    cloaked: bool,
    /// The window is to be uncloaked on the next frame, once a capture for it is drawn.
    reveal: bool,
    /// The window's taskbar button was taken away while it was kept ready.
    off_taskbar: bool,
    /// Out of sight, kept ready for the next capture (`--standby`, or its window closed).
    standby: bool,
    /// A frame has been drawn: the fonts and the drawing are ready.
    warm: bool,
    /// When the program started, until its first frame is drawn (for the log).
    started: Option<std::time::Instant>,
    /// The window is being brought to the owner: its size is looked at once it's no longer
    /// minimized (frames left to look in).
    size_check: u8,
    /// The last place the owner left the window in (shown, not minimized, its usual size or
    /// bigger), written over eframe's whenever eframe writes the window's place (`place.rs`).
    place: Option<place::Place>,
    /// The window's place is kept in the settings folder (not with `SCREENGRABCRAFT_NO_PREFS`).
    keep_place: bool,
}

/// The smallest the window is ever shown at for the owner, and the size given to one found
/// smaller (points).
const SHOWN_MIN: (f64, f64) = (760.0, 480.0);
const SHOWN_USUAL: (f64, f64) = (1320.0, 860.0);
/// The same, as the window's place is kept.
const PLACE_MIN: (f32, f32) = (SHOWN_MIN.0 as f32, SHOWN_MIN.1 as f32);

/// The size to give a window of `inner` (physical pixels, at `scale`) that's being shown, when
/// it's smaller than the editor is ever shown at; None when it's fine. Windows can leave a window
/// kept out of sight squeezed to a sliver at the top left of the screen.
fn rescued_size(inner: (u32, u32), scale: f64) -> Option<(f64, f64)> {
    let scale = if scale.is_finite() { scale.max(0.5) } else { 1.0 };
    let (w, h) = (f64::from(inner.0) / scale, f64::from(inner.1) / scale);
    (w < SHOWN_MIN.0 - 1.0 || h < SHOWN_MIN.1 - 1.0).then_some(SHOWN_USUAL)
}

impl App {
    /// Whether a closed window goes out of sight instead of ending the program: while the
    /// capture service runs and the owner hasn't turned it off.
    fn keep_ready(&self) -> bool {
        cfg!(target_os = "windows") && instance::running("service") && store::load_settings().keep_editor_ready
    }

    /// Out of the picture for a capture, at once: cloaked, so that Windows shows nothing of it
    /// although it stays where it is, maximized or not, and is drawn (moving it would un-maximize
    /// it, and hiding it is a taskbar button gone). Kept ready out of sight, it is shown cloaked,
    /// off the taskbar, so that it appears at once if the capture comes to it. Where cloaking isn't
    /// to be had it is hidden (without Windows' animation, see `cloak::calm_windows`).
    fn park(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        cloak::calm_windows();
        self.cloaked = cloak::cloak(frame, true);
        if !self.cloaked {
            if !self.standby {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            return;
        }
        if self.standby {
            taskbar_button(frame, false);
            self.off_taskbar = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
    }

    /// Back as it was before the capture. Kept ready, it is hidden again first, at once (a command
    /// would hide it only after this frame), so that uncloaking it shows nothing.
    fn unpark(&mut self, ctx: &egui::Context, frame: &eframe::Frame, focus: bool) {
        if std::mem::take(&mut self.cloaked) {
            if self.standby
                && let Some(w) = frame.winit_window()
            {
                w.set_visible(false);
            }
            cloak::cloak(frame, false);
        } else if !self.standby {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        if focus && !self.standby {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    /// Out of sight, the window is never seen: if Windows shows it anyway (as a sliver at the top
    /// left of the screen), it's hidden again at once.
    fn keep_out_of_sight(&self, frame: &eframe::Frame) {
        if let Some(w) = frame.winit_window()
            && w.is_visible() == Some(true)
        {
            w.set_visible(false);
            log::warn!("the window kept out of sight was shown by something else; hidden again");
        }
    }

    /// Being brought to the owner: never as a sliver. Once it's no longer minimized, a window
    /// smaller than the editor is ever shown at gets its usual size back.
    fn check_size(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.size_check == 0 {
            return;
        }
        let Some(w) = frame.winit_window() else {
            self.size_check = 0;
            return;
        };
        if w.is_minimized() == Some(true) {
            self.size_check -= 1;
            return;
        }
        self.size_check = 0;
        let s = w.inner_size();
        if let Some((x, y)) = rescued_size((s.width, s.height), w.scale_factor()) {
            log::warn!("the window was {} × {} as it was shown; given its usual size", s.width, s.height);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(x as f32, y as f32)));
            // Near the top left of its monitor, whole on it.
            if let Some(m) = w.current_monitor().or_else(|| w.primary_monitor()) {
                let (p, scale) = (m.position(), m.scale_factor().max(0.5));
                let at = egui::pos2((f64::from(p.x) / scale) as f32 + 40.0, (f64::from(p.y) / scale) as f32 + 40.0);
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(at));
            }
        }
    }

    /// Notes where the window is while the owner can see it and it's a place they left it in, to
    /// be kept instead of whatever eframe sees as it writes the window's place (minimized, out of
    /// sight or squeezed, it would open next time as a sliver at the top left of the first screen).
    fn note_place(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if !self.keep_place || self.standby || self.aside.parked || self.cloaked || self.reveal || self.size_check > 0 {
            return;
        }
        let Some(w) = frame.winit_window() else { return };
        if w.is_visible() != Some(true) || w.is_minimized() == Some(true) {
            return;
        }
        // As eframe measures it: the inside's size in points (pixels over the screen's scale and
        // egui's zoom), the positions in pixels.
        let scale = w.scale_factor() * f64::from(ctx.zoom_factor());
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        let size = w.inner_size();
        let at = |x: i32, y: i32| place::Xy { x: x as f32, y: y as f32 };
        let here = place::Place {
            inner_position_pixels: w.inner_position().ok().map(|p| at(p.x, p.y)),
            outer_position_pixels: w.outer_position().ok().map(|p| at(p.x, p.y)),
            fullscreen: w.fullscreen().is_some(),
            maximized: w.is_maximized(),
            inner_size_points: Some(place::Xy { x: (f64::from(size.width) / scale) as f32, y: (f64::from(size.height) / scale) as f32 }),
        };
        if here.is_good(PLACE_MIN) {
            self.place = Some(here);
        }
    }

    /// The window was closed (and nothing is left unsaved): out of sight, with an empty picture.
    fn go_out_of_sight(&mut self, ctx: &egui::Context) {
        self.settings_file.save(&self.editor.settings);
        self.editor.start_over();
        self.aside = Aside::default();
        self.standby = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        log::info!("the window was closed; the editor stays ready out of sight");
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // A capture for the editor was drawn on the last frame, cloaked: now it's seen.
        if std::mem::take(&mut self.reveal) {
            cloak::cloak(frame, false);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.check_size(ctx, frame);
        self.note_place(ctx, frame);
        for args in self.guard.handed() {
            if let [word] = args.as_slice()
                && word.starts_with("--")
            {
                // An editor kept ready ends when the owner no longer wants one.
                if self.standby && word == launch::EDITOR_UNREADY {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    continue;
                }
                let (minimized, focused) = ctx.input(|i| (i.viewport().minimized.unwrap_or(false), i.viewport().focused.unwrap_or(false)));
                match self.aside.hear(word, minimized, focused, self.standby) {
                    Move::Park => self.park(ctx, frame),
                    Move::Unpark { focus } => self.unpark(ctx, frame, focus),
                    Move::Stay => {}
                }
                continue;
            }
            // Minimized by the owner, a capture leaves it so (looked at before it's no longer kept
            // ready: a window out of sight may say it's minimized).
            let minimized =
                frame.winit_window().and_then(|w| w.is_minimized()).unwrap_or_else(|| ctx.input(|i| i.viewport().minimized.unwrap_or(false)));
            let back = brought_back(&args, minimized, self.standby);
            // A capture for the editor: drawn this frame while still cloaked (kept ready, it's
            // already shown, cloaked), and uncloaked on the next, so that it appears at once and
            // with the capture; then it has the keyboard. Hidden instead, it's shown.
            if self.aside.parked && std::mem::take(&mut self.cloaked) {
                self.reveal = true;
                ctx.request_repaint();
            }
            if std::mem::take(&mut self.off_taskbar) {
                taskbar_button(frame, true);
            }
            let was_ready = std::mem::take(&mut self.standby);
            self.aside = Aside::default();
            let t = std::time::Instant::now();
            for a in args.iter().filter(|a| !a.starts_with("--")) {
                self.editor.open_path(ctx, a);
            }
            if args.iter().any(|a| a == launch::CAPTIONS_COMING) {
                self.editor.expect_captions(ctx.input(|i| i.time));
            }
            if args.iter().any(|a| !a.starts_with("--")) {
                log::info!("opened what was handed over in {} ms", t.elapsed().as_millis());
            }
            // Kept ready and now asked for with nothing to open: what it last had open, once the
            // window is up.
            if opens_the_last_file(&args, was_ready) {
                self.editor.reopen_last();
            }
            if back {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                // Its size is looked at once it's back (a few frames, while it's restored).
                self.size_check = 5;
            } else {
                log::info!("a capture came while the window is minimized: opened in it, and it stays minimized");
            }
            ctx.request_repaint();
        }
        // Out of the picture for a capture by a service that has gone since (the watch thread
        // wakes it then).
        if self.aside.parked && !instance::running("service") {
            let Move::Unpark { focus } = self.aside.back() else { return };
            self.unpark(ctx, frame, focus);
        }
        if self.standby && !self.aside.parked {
            // Asked to close (an installer, signing out): there is nothing to keep, so it does.
            if ctx.input(|i| i.viewport().close_requested()) {
                return;
            }
            // eframe shows its window once the first frame is drawn, even one made hidden, and it
            // runs this frame's commands just after: so it's hidden on that frame (out of sight
            // nothing is drawn, and it would be a black window). Only then: a command asks for
            // another frame, and a hidden window's frames come ten a second while they're asked for.
            if !self.warm {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            // Ready for as long as the service runs; once it has gone (quit, or updating), so does
            // this (the watch thread wakes it then).
            if !instance::running("service") {
                log::info!("the capture service has gone; the editor kept ready ends too");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            // Nothing to do out of sight once the first frame has readied the fonts and the drawing,
            // but to see that it stays out of sight.
            if self.warm {
                self.keep_out_of_sight(frame);
                return;
            }
        }
        self.editor.logic(ctx);
        if ctx.input(|i| i.viewport().close_requested()) {
            if !self.editor.close_requested() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else if !self.standby && self.keep_ready() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.go_out_of_sight(ctx);
            }
        }
        if self.editor.quit_requested {
            self.editor.quit_requested = false;
            if self.editor.close_requested() {
                if self.keep_ready() {
                    self.go_out_of_sight(ctx);
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.standby && self.warm {
            return;
        }
        self.editor.ui(ui);
        self.warm = true;
        if let Some(s) = self.started.take() {
            log::info!("start: first frame after {} ms", s.elapsed().as_millis());
        }
    }

    /// The window's place is written now and then, not only when the program ends (a program
    /// kept ready may end with Windows). Stepping aside doesn't move it.
    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(10)
    }

    fn on_exit(&mut self) {
        self.settings_file.save(&self.editor.settings);
        self.guard.close();
    }

    /// eframe has just put down the window's place as it is now, whatever that is; it's written
    /// over with the last place the owner left it in, so that it never opens minimized, squeezed
    /// or out of sight. With none seen yet (kept ready out of sight since it started, from a bad
    /// place), nothing is kept: eframe then opens it at its usual size.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if !self.keep_place {
            return;
        }
        let text = self.place.and_then(|p| p.write()).unwrap_or_else(|| "()".to_string());
        storage.set_string(place::KEY, text);
    }

    /// Only the window's place and size are kept, not egui's state (which holds, among other
    /// things, the undo history of text being typed).
    fn persist_egui_memory(&self) -> bool {
        false
    }
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", version::line(screengrabcraft_model::EDITOR_NAME));
        return std::process::ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("usage: screengrabcraft-editor [--video] [--version] [pictures, .sgcraft projects or recordings...]");
        return std::process::ExitCode::SUCCESS;
    }
    let started = std::time::Instant::now();
    logging::init();
    let standby = args.iter().any(|a| a == launch::EDITOR_STANDBY);
    // The video editor: a window of its own for recordings.
    let video = args.iter().any(|a| a == launch::VIDEO);
    let (instance_name, kind) = if video { (launch::VIDEO_INSTANCE, EditorKind::Video) } else { ("editor", EditorKind::Pictures) };
    // A recording from the capture service whose captions it is making.
    let coming = args.iter().any(|a| a == launch::CAPTIONS_COMING);
    // Kept ready by the service: one running already is enough (and isn't to be shown).
    if standby && instance::running(instance_name) {
        return std::process::ExitCode::SUCCESS;
    }
    // Paths as the file manager gave them, made whole before they are handed to another copy.
    let files: Vec<String> =
        args.iter().filter(|a| !a.starts_with("--")).map(|a| std::fs::canonicalize(a).map(|p| simplify(&p)).unwrap_or_else(|_| a.clone())).collect();
    let guard = match instance::start(instance_name, &files) {
        instance::Start::First(g) => g,
        instance::Start::Handed => return std::process::ExitCode::SUCCESS,
        instance::Start::Unreachable(e) => {
            log::error!("another editor is running but couldn't be reached: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    // The copy that stays: its log is the one kept.
    logging::to_file(if video { "video-editor" } else { "editor" });
    log::info!("{}", version::line(kind.name()));
    // Text and callouts can use the fonts on the computer (looked for now, out of the way).
    screengrabcraft_desktop::fonts::install();
    if video {
        // Before any window is made: its taskbar button is its own, not the picture editor's.
        cloak::own_taskbar_button(VIDEO_APP_ID);
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(kind.name())
        .with_app_id(APP_ID)
        .with_inner_size([1320.0, 860.0])
        .with_min_inner_size([760.0, 480.0])
        .with_drag_and_drop(true)
        .with_visible(!standby);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) {
        viewport = viewport.with_icon(if video { video_icon(icon) } else { icon });
    }
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    // The window opens where it was left: the same place (and so the same monitor), the same
    // size, maximized or not. eframe keeps that (and nothing else, see `persist_egui_memory`) in
    // the settings folder, and brings a window whose monitor is gone back onto one that's there.
    // Only ever a place the owner left it in is kept (see `place.rs`).
    let keep_place = match paths::ensure_settings_dir().filter(|_| !paths::no_prefs()) {
        Some(dir) => {
            options.persistence_path = Some(dir.join(if video { VIDEO_WINDOW_FILE } else { WINDOW_FILE }));
            true
        }
        None => {
            options.persist_window = false;
            false
        }
    };
    screengrabcraft_desktop::graphics::configure(&mut options, eframe::wgpu::Backends::from_env());
    log::info!("start: graphics chosen after {} ms", started.elapsed().as_millis());
    let result = eframe::run_native(
        kind.name(),
        options,
        Box::new(move |cc| {
            log::info!("start: window and graphics ready after {} ms", started.elapsed().as_millis());
            screengrabcraft_desktop::graphics::log_errors(cc);
            // The window is made, not shown yet (eframe shows it after the first frame): Windows
            // won't animate it opening, closing or stepping aside.
            cloak::calm_windows();
            let ctx = cc.egui_ctx.clone();
            let (settings, settings_file) = store::SharedEditorSettings::load();
            let settings_file = std::sync::Arc::new(settings_file);
            let mut editor = Editor::new(Box::new(services::DesktopServices::new(ctx.clone(), video, settings_file.clone())), settings);
            editor.kind = kind;
            let wake = ctx.clone();
            guard.set_waker(Box::new(move || wake.request_repaint()));
            watch_the_service(ctx.clone());
            for f in &files {
                editor.open_path(&ctx, f);
            }
            // Started with nothing to open: what it last had open, once the window is up (and
            // what there is to start with offered, if there's none).
            if files.is_empty() {
                editor.offer_welcome();
            }
            if opens_the_last_file(&args, !standby) {
                editor.reopen_last();
            }
            if coming {
                editor.expect_captions(0.0);
            }
            log::info!("start: editor ready after {} ms", started.elapsed().as_millis());
            if standby {
                log::info!("kept ready out of sight for the next capture");
            }
            // Where the window was left, if that's a place to open it in; the window was opened
            // there (or, from a bad place, eframe put it on the first screen, squeezed).
            let place = if keep_place { cc.storage.and_then(|s| s.get_string(place::KEY)).and_then(|t| place::Place::read(&t)) } else { None };
            let place = place.filter(|p| p.is_good(PLACE_MIN));
            if keep_place && place.is_none() {
                log::info!("start: no good place kept for the window");
            }
            Ok(Box::new(App {
                editor,
                guard,
                settings_file,
                aside: Aside::default(),
                cloaked: false,
                reveal: false,
                off_taskbar: false,
                standby,
                warm: false,
                started: Some(started),
                // Opened for the owner: its size is looked at at once, so that a window opened
                // from a bad place (minimized or squeezed when it was last kept) is never left a
                // sliver at the top left of the first screen.
                size_check: if standby { 0 } else { 5 },
                place,
                keep_place,
            }))
        }),
    );
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            log::error!("the editor window could not start: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Wakes the window when the capture service goes (quits, crashes, is updated), so a window out
/// of sight comes back or, kept ready, ends. A thread that sleeps costs nothing; a hidden window
/// asking for frames would be drawn ten times a second.
fn watch_the_service(ctx: egui::Context) {
    if paths::no_prefs() {
        return;
    }
    let spawned = std::thread::Builder::new().name("service-watch".into()).spawn(move || {
        let mut was = instance::running("service");
        loop {
            std::thread::sleep(WATCH);
            let now = instance::running("service");
            // A frame now and then also sees that a window out of sight stays so (one frame:
            // nothing is drawn out of sight).
            let _ = was && !now;
            ctx.request_repaint();
            was = now;
        }
    });
    if let Err(e) = spawned {
        log::warn!("no watch for the capture service: {e}");
    }
}

/// The editor's icon with a play mark at its bottom right: the video editor's, told apart from the
/// picture editor's in the taskbar and when switching windows (drawn here, over the editor's own).
fn video_icon(mut icon: egui::IconData) -> egui::IconData {
    let w = icon.width.max(1);
    let (cx, cy, r) = (w as f32 * 0.74, icon.height as f32 * 0.74, w as f32 * 0.25);
    let (ax, ay, bx, by, tx, ty) = (cx - r * 0.3, cy - r * 0.42, cx - r * 0.3, cy + r * 0.42, cx + r * 0.46, cy);
    let side = |x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32| (x2 - x1) * (y - y1) - (y2 - y1) * (x - x1);
    let in_triangle = |x: f32, y: f32| {
        let d = [side(ax, ay, bx, by, x, y), side(bx, by, tx, ty, x, y), side(tx, ty, ax, ay, x, y)];
        !(d.iter().any(|v| *v < 0.0) && d.iter().any(|v| *v > 0.0))
    };
    for (i, px) in icon.rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x0, y0) = ((i as u32 % w) as f32, (i as u32 / w) as f32);
        // Four by four samples in each pixel: the edges smooth.
        let (mut disc, mut light) = (0u32, 0u32);
        for k in 0..16 {
            let (x, y) = (x0 + ((k % 4) as f32 + 0.5) / 4.0, y0 + ((k / 4) as f32 + 0.5) / 4.0);
            let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            if d <= r {
                disc += 1;
                // A white rim, and a white triangle on red.
                if d > r * 0.86 || in_triangle(x, y) {
                    light += 1;
                }
            }
        }
        if disc == 0 {
            continue;
        }
        let cover = disc as f32 / 16.0;
        let white = light as f32 / disc as f32;
        let colour = [229.0 + 26.0 * white, 57.0 + 198.0 * white, 53.0 + 202.0 * white];
        for (c, v) in px.iter_mut().zip(colour) {
            *c = (f32::from(*c) * (1.0 - cover) + v * cover).round() as u8;
        }
        if let Some(a) = px.get_mut(3) {
            *a = (*a).max((cover * 255.0).round() as u8);
        }
    }
    icon
}

/// A canonical path without Windows' `\\?\` prefix.
fn simplify(p: &std::path::Path) -> String {
    let s = p.to_string_lossy().to_string();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_video_editors_icon_is_the_editors_with_a_play_mark() {
        let editor = eframe::icon_data::from_png_bytes(ICON).unwrap();
        let video = video_icon(editor.clone());
        assert_eq!((video.width, video.height, video.rgba.len()), (editor.width, editor.height, editor.rgba.len()));
        let at = |icon: &egui::IconData, x: f32, y: f32| {
            let i = ((y * icon.height as f32) as usize * icon.width as usize + (x * icon.width as f32) as usize) * 4;
            icon.rgba[i..i + 4].to_vec()
        };
        // The top left is the editor's own; the mark's red is at the bottom right, its white
        // triangle in the middle of it.
        assert_eq!(at(&video, 0.2, 0.2), at(&editor, 0.2, 0.2));
        assert_eq!(at(&video, 0.74, 0.88)[..3], [229, 57, 53]);
        assert_eq!(at(&video, 0.75, 0.74)[..3], [255, 255, 255]);
        // Any size, and none: no panic.
        let _ = video_icon(egui::IconData { rgba: vec![0; 4 * 3 * 5], width: 3, height: 5 });
        let _ = video_icon(egui::IconData { rgba: Vec::new(), width: 0, height: 0 });
    }

    #[test]
    fn a_window_squeezed_to_a_sliver_gets_its_usual_size_back() {
        // As Windows left it: a sliver at the top left.
        assert_eq!(rescued_size((70, 60), 1.0), Some(SHOWN_USUAL));
        assert_eq!(rescued_size((160, 28), 1.5), Some(SHOWN_USUAL));
        // Narrow or short only, at any scale.
        assert_eq!(rescued_size((2000, 300), 1.0), Some(SHOWN_USUAL));
        assert_eq!(rescued_size((1000, 1000), 2.0), Some(SHOWN_USUAL), "500 × 500 points");
        // A window the owner sized is left as it is, down to the smallest the editor allows.
        assert_eq!(rescued_size((2579, 1839), 1.5), None);
        assert_eq!(rescued_size((760, 480), 1.0), None);
        assert_eq!(rescued_size((1140, 720), 1.5), None);
        // Nonsense scales don't panic.
        assert_eq!(rescued_size((100, 100), f64::NAN), Some(SHOWN_USUAL));
        assert_eq!(rescued_size((u32::MAX, u32::MAX), 0.0), None);
    }

    #[test]
    fn a_window_in_use_steps_aside_for_a_capture_and_comes_back_with_the_keyboard() {
        let mut a = Aside::default();
        assert_eq!(a.hear(launch::EDITOR_HIDE, false, true, false), Move::Park);
        assert!(a.parked);
        assert_eq!(a.hear(launch::EDITOR_SHOW, false, false, false), Move::Unpark { focus: true });
        assert_eq!(a.hear(launch::EDITOR_SHOW, false, true, false), Move::Stay, "once");
    }

    #[test]
    fn a_window_behind_another_program_comes_back_and_leaves_the_keyboard_alone() {
        let mut a = Aside::default();
        assert_eq!(a.hear(launch::EDITOR_HIDE, false, false, false), Move::Park, "still out of the picture");
        assert_eq!(a.hear(launch::EDITOR_SHOW, false, false, false), Move::Unpark { focus: false }, "it has no taskbar button to come back by");
    }

    #[test]
    fn a_second_capture_while_aside_remembers_the_first() {
        let mut a = Aside::default();
        assert_eq!(a.hear(launch::EDITOR_HIDE, false, true, false), Move::Park);
        assert_eq!(a.hear(launch::EDITOR_HIDE, false, false, false), Move::Stay);
        assert_eq!(a.hear(launch::EDITOR_SHOW, false, false, false), Move::Unpark { focus: true });
    }

    #[test]
    fn a_window_aside_can_come_back_without_a_word() {
        // What the window does when the service that hid it has gone.
        let mut a = Aside::default();
        a.hear(launch::EDITOR_HIDE, false, true, false);
        assert_eq!(a.back(), Move::Unpark { focus: true });
        assert!(!a.parked);
    }

    #[test]
    fn a_window_the_owner_minimized_stays_minimized() {
        let mut a = Aside::default();
        assert_eq!(a.hear(launch::EDITOR_HIDE, true, false, false), Move::Stay);
        assert_eq!(a.hear(launch::EDITOR_SHOW, true, false, false), Move::Stay);
    }

    #[test]
    fn a_capture_for_a_window_the_owner_minimized_leaves_it_minimized() {
        let s = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        let capture = launch::capture_args(&s(&["shot.png"]));
        assert!(!brought_back(&capture, true, false), "opened in it, it stays minimized");
        assert!(brought_back(&capture, false, false), "on the screen, it comes forward with the capture");
        // The video editor, minimized, as a recording comes with its captions to be made.
        let recording = launch::capture_args(&s(&[launch::CAPTIONS_COMING, "a.sgvideo"]));
        assert!(!brought_back(&recording, true, false));
        // Kept ready out of sight it's hidden (whatever the system says of minimized): it comes back.
        assert!(brought_back(&capture, true, true));
        assert!(brought_back(&capture, false, true));
    }

    #[test]
    fn what_the_owner_opens_brings_a_minimized_window_back() {
        // A file from the file manager or a notice, and the tray's Open Editor (nothing to open).
        assert!(brought_back(&["shot.png".to_string()], true, false));
        assert!(brought_back(&[], true, false));
        assert!(brought_back(&[launch::CAPTIONS_COMING.to_string(), "a.sgvideo".to_string()], true, false));
    }

    #[test]
    fn an_editor_kept_ready_steps_aside_too_and_never_takes_the_keyboard() {
        // Hidden (and so "minimized" or not, as the system says): shown off the screens, ready.
        let mut a = Aside::default();
        assert_eq!(a.hear(launch::EDITOR_HIDE, true, false, true), Move::Park);
        assert_eq!(a.hear(launch::EDITOR_SHOW, true, true, true), Move::Unpark { focus: false });
        let mut a = Aside::default();
        assert_eq!(a.hear(launch::EDITOR_HIDE, false, true, true), Move::Park);
        assert_eq!(a.back(), Move::Unpark { focus: false });
    }

    #[test]
    fn the_last_file_opens_only_when_the_editor_is_opened_with_nothing() {
        let s = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        // Started from the Start menu, a desktop icon or the tray's Open Editor, either editor.
        assert!(opens_the_last_file(&[], true));
        assert!(opens_the_last_file(&s(&[launch::VIDEO]), true));
        // A file given on the command line, or handed over, is what opens.
        assert!(!opens_the_last_file(&s(&["shot.png"]), true));
        assert!(!opens_the_last_file(&s(&[launch::VIDEO, "C:/Captures/Week.sgvideo"]), true));
        // A capture or a recording from the service, even one with no file named.
        assert!(!opens_the_last_file(&launch::capture_args(&s(&["shot.png"])), true));
        assert!(!opens_the_last_file(&launch::capture_args(&[]), true));
        assert!(!opens_the_last_file(&launch::capture_args(&s(&[launch::CAPTIONS_COMING, "a.sgvideo"])), true));
        // Started out of sight to be kept ready: nothing until it's brought to the owner.
        assert!(!opens_the_last_file(&s(&[launch::EDITOR_STANDBY]), true));
        assert!(!opens_the_last_file(&s(&[launch::VIDEO, launch::EDITOR_STANDBY]), false));
        // A window already on the screen just comes forward with what it has.
        assert!(!opens_the_last_file(&[], false));
    }

    #[test]
    fn other_words_do_nothing() {
        let mut a = Aside::default();
        assert_eq!(a.hear("--anything", false, true, false), Move::Stay);
        assert_eq!(a.hear("--anything", true, false, false), Move::Stay);
    }
}
