//! ScreenGrabCraft Editor: annotate, mark up and share screenshots.
//!
//! Usage: `screengrabcraft-editor [--version] [pictures or .sgcraft projects…]`
//!
//! One editor window at a time: started again (a capture handed over by the capture service, a
//! file opened from the file manager), it hands the files to the window already open and exits.
//!
//! The capture service also hides the window before a capture ([`Aside`]). A hidden window has no
//! taskbar button to be found by, so it never outlasts the service that hid it, and a program
//! asking for the editor (the Start menu, the tray) brings it back too.
//!
//! On Windows the service keeps an editor ready out of sight (`--standby`, see
//! [`launch::ready_editor`]): started hidden soon after the service, and hidden again rather than
//! closed when its window is closed, so a capture never waits for the editor to start. It ends
//! with the service, or when the owner turns that off.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod services;
mod video;

use std::time::Duration;

use screengrabcraft_desktop::{instance, launch, logging, paths, store, version};
use screengrabcraft_ui::Editor;

const APP_ID: &str = "io.github.billsecond.screengrabcraft.editor";
/// The window's place, size and state, in the settings folder (written by eframe).
const WINDOW_FILE: &str = "editor-window.ron";
static ICON: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/io.github.billsecond.screengrabcraft.editor.png");

/// How often a thread looks for the capture service, to wake a window out of sight (hidden for a
/// capture, or kept ready) once the service has gone. The window itself asks for no frames while
/// out of sight: eframe draws a hidden window ten times a second for as long as one is asked for.
const WATCH: Duration = Duration::from_secs(3);

/// What to do with the window.
#[derive(Debug, PartialEq)]
enum Move {
    Hide,
    Show { focus: bool },
    Stay,
}

impl Move {
    fn apply(self, ctx: &egui::Context) {
        match self {
            Move::Hide => ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false)),
            Move::Show { focus } => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                if focus {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
            }
            Move::Stay => {}
        }
    }
}

/// The window hidden for a capture, and whether the owner was working in it. A window the owner
/// minimized isn't in the picture, and stays minimized.
#[derive(Default)]
struct Aside {
    hidden: bool,
    focused: bool,
}

impl Aside {
    /// The capture service's word.
    fn hear(&mut self, word: &str, minimized: bool, focused: bool) -> Move {
        match word {
            launch::EDITOR_HIDE if !minimized && !self.hidden => {
                *self = Aside { hidden: true, focused };
                Move::Hide
            }
            launch::EDITOR_SHOW if self.hidden => self.back(),
            _ => Move::Stay,
        }
    }

    /// Back on screen, with the keyboard only if it had it: over another program it would take
    /// it just as the capture is about to be pasted there.
    fn back(&mut self) -> Move {
        let focus = self.focused;
        *self = Aside::default();
        Move::Show { focus }
    }
}

struct App {
    editor: Editor,
    guard: instance::Guard,
    aside: Aside,
    /// Out of sight, kept ready for the next capture (`--standby`, or its window closed).
    standby: bool,
    /// A frame has been drawn: the fonts and the drawing are ready.
    warm: bool,
    /// When the program started, until its first frame is drawn (for the log).
    started: Option<std::time::Instant>,
}

impl App {
    /// Whether a closed window goes out of sight instead of ending the program: while the
    /// capture service runs and the owner hasn't turned it off.
    fn keep_ready(&self) -> bool {
        cfg!(target_os = "windows") && instance::running("service") && store::load_settings().keep_editor_ready
    }

    /// The window was closed (and nothing is left unsaved): out of sight, with an empty picture.
    fn go_out_of_sight(&mut self, ctx: &egui::Context) {
        store::save_editor_settings(&self.editor.settings);
        self.editor.start_over();
        self.aside = Aside::default();
        self.standby = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        log::info!("the window was closed; the editor stays ready out of sight");
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for args in self.guard.handed() {
            if let [word] = args.as_slice()
                && word.starts_with("--")
            {
                // An editor kept ready is out of the picture already, and stays out of sight, or
                // ends when the owner no longer wants one.
                if self.standby {
                    if word == launch::EDITOR_UNREADY {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    continue;
                }
                let (minimized, focused) = ctx.input(|i| (i.viewport().minimized.unwrap_or(false), i.viewport().focused.unwrap_or(false)));
                self.aside.hear(word, minimized, focused).apply(ctx);
                continue;
            }
            self.standby = false;
            self.aside = Aside::default();
            let t = std::time::Instant::now();
            for a in args.iter().filter(|a| !a.starts_with("--")) {
                self.editor.open_path(ctx, a);
            }
            if args.iter().any(|a| !a.starts_with("--")) {
                log::info!("opened what was handed over in {} ms", t.elapsed().as_millis());
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if self.standby {
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
            // Nothing to do out of sight once the first frame has readied the fonts and the drawing.
            if self.warm {
                return;
            }
        }
        // Hidden for a capture by a service that has gone since (the watch thread wakes it then).
        if self.aside.hidden && !instance::running("service") {
            self.aside.back().apply(ctx);
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
    /// kept ready may end with Windows).
    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(10)
    }

    fn on_exit(&mut self) {
        store::save_editor_settings(&self.editor.settings);
        self.guard.close();
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
        println!("{}", version::line("ScreenGrabCraft Editor"));
        return std::process::ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("usage: screengrabcraft-editor [--version] [pictures or .sgcraft projects...]");
        return std::process::ExitCode::SUCCESS;
    }
    let started = std::time::Instant::now();
    logging::init();
    let standby = args.iter().any(|a| a == launch::EDITOR_STANDBY);
    // Kept ready by the service: one running already is enough (and isn't to be shown).
    if standby && instance::running("editor") {
        return std::process::ExitCode::SUCCESS;
    }
    // Paths as the file manager gave them, made whole before they are handed to another copy.
    let files: Vec<String> =
        args.iter().filter(|a| !a.starts_with("--")).map(|a| std::fs::canonicalize(a).map(|p| simplify(&p)).unwrap_or_else(|_| a.clone())).collect();
    let guard = match instance::start("editor", &files) {
        instance::Start::First(g) => g,
        instance::Start::Handed => return std::process::ExitCode::SUCCESS,
        instance::Start::Unreachable(e) => {
            log::error!("another editor is running but couldn't be reached: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    // The copy that stays: its log is the one kept.
    logging::to_file("editor");
    log::info!("{}", version::line("ScreenGrabCraft Editor"));
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(screengrabcraft_model::EDITOR_NAME)
        .with_app_id(APP_ID)
        .with_inner_size([1320.0, 860.0])
        .with_min_inner_size([760.0, 480.0])
        .with_drag_and_drop(true)
        .with_visible(!standby);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    // The window opens where it was left: the same place (and so the same monitor), the same
    // size, maximized or not. eframe keeps that (and nothing else, see `persist_egui_memory`) in
    // the settings folder, and brings a window whose monitor is gone back onto one that's there.
    match paths::ensure_settings_dir().filter(|_| !paths::no_prefs()) {
        Some(dir) => options.persistence_path = Some(dir.join(WINDOW_FILE)),
        None => options.persist_window = false,
    }
    screengrabcraft_desktop::graphics::configure(&mut options, eframe::wgpu::Backends::from_env());
    log::info!("start: graphics chosen after {} ms", started.elapsed().as_millis());
    let result = eframe::run_native(
        screengrabcraft_model::EDITOR_NAME,
        options,
        Box::new(move |cc| {
            log::info!("start: window and graphics ready after {} ms", started.elapsed().as_millis());
            screengrabcraft_desktop::graphics::log_errors(cc);
            let ctx = cc.egui_ctx.clone();
            let settings = store::load_editor_settings();
            let mut editor = Editor::new(Box::new(services::DesktopServices::new(ctx.clone())), settings);
            let wake = ctx.clone();
            guard.set_waker(Box::new(move || wake.request_repaint()));
            watch_the_service(ctx.clone());
            for f in &files {
                editor.open_path(&ctx, f);
            }
            log::info!("start: editor ready after {} ms", started.elapsed().as_millis());
            if standby {
                log::info!("kept ready out of sight for the next capture");
            }
            Ok(Box::new(App { editor, guard, aside: Aside::default(), standby, warm: false, started: Some(started) }))
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
            if was && !now {
                ctx.request_repaint();
            }
            was = now;
        }
    });
    if let Err(e) = spawned {
        log::warn!("no watch for the capture service: {e}");
    }
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
    fn a_window_in_use_hides_for_a_capture_and_comes_back_with_the_keyboard() {
        let mut a = Aside::default();
        assert_eq!(a.hear(launch::EDITOR_HIDE, false, true), Move::Hide);
        assert!(a.hidden);
        assert_eq!(a.hear(launch::EDITOR_SHOW, false, false), Move::Show { focus: true });
        assert_eq!(a.hear(launch::EDITOR_SHOW, false, true), Move::Stay, "once");
    }

    #[test]
    fn a_window_behind_another_program_comes_back_and_leaves_the_keyboard_alone() {
        let mut a = Aside::default();
        assert_eq!(a.hear(launch::EDITOR_HIDE, false, false), Move::Hide, "still out of the picture");
        assert_eq!(a.hear(launch::EDITOR_SHOW, false, false), Move::Show { focus: false }, "it has no taskbar button to come back by");
    }

    #[test]
    fn a_second_capture_while_hidden_remembers_the_first() {
        let mut a = Aside::default();
        assert_eq!(a.hear(launch::EDITOR_HIDE, false, true), Move::Hide);
        assert_eq!(a.hear(launch::EDITOR_HIDE, false, false), Move::Stay);
        assert_eq!(a.hear(launch::EDITOR_SHOW, false, false), Move::Show { focus: true });
    }

    #[test]
    fn a_hidden_window_can_come_back_without_a_word() {
        // What the window does when the service that hid it has gone.
        let mut a = Aside::default();
        a.hear(launch::EDITOR_HIDE, false, true);
        assert_eq!(a.back(), Move::Show { focus: true });
        assert!(!a.hidden);
    }

    #[test]
    fn a_window_the_owner_minimized_stays_minimized() {
        let mut a = Aside::default();
        assert_eq!(a.hear(launch::EDITOR_HIDE, true, false), Move::Stay);
        assert_eq!(a.hear(launch::EDITOR_SHOW, true, false), Move::Stay);
    }

    #[test]
    fn other_words_do_nothing() {
        let mut a = Aside::default();
        assert_eq!(a.hear("--anything", false, true), Move::Stay);
        assert_eq!(a.hear("--anything", true, false), Move::Stay);
    }
}
