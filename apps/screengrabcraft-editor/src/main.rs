//! ScreenGrabCraft Editor: annotate, mark up and share screenshots.
//!
//! Usage: `screengrabcraft-editor [--version] [pictures or .sgcraft projects…]`
//!
//! One editor window at a time: started again (a capture handed over by the capture service, a
//! file opened from the file manager), it hands the files to the window already open and exits.
//!
//! The capture service also takes the window out of the picture before a capture ([`Aside`]). It's
//! minimized, never hidden: whatever happens to the service, it's still in the taskbar.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod services;

use screengrabcraft_desktop::{instance, launch, logging, store, version};
use screengrabcraft_ui::Editor;

const APP_ID: &str = "io.github.billsecond.screengrabcraft.editor";
static ICON: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/io.github.billsecond.screengrabcraft.editor.png");

/// What to do with the window at the capture service's word.
#[derive(Debug, PartialEq)]
enum Move {
    Minimize,
    Restore,
    Stay,
}

/// Whether the window went down for a capture and comes back up after it: only if the owner was
/// working in it. Coming back over another program would take the keyboard from it, just as the
/// capture is about to be pasted there, and a window the owner minimized stays that way.
#[derive(Default)]
struct Aside(bool);

impl Aside {
    fn hear(&mut self, word: &str, minimized: bool, focused: bool) -> Move {
        match word {
            launch::EDITOR_HIDE if !minimized => {
                self.0 = focused;
                Move::Minimize
            }
            launch::EDITOR_SHOW if self.0 => {
                self.0 = false;
                Move::Restore
            }
            _ => Move::Stay,
        }
    }
}

struct App {
    editor: Editor,
    guard: instance::Guard,
    aside: Aside,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for args in self.guard.handed() {
            if let [word] = args.as_slice()
                && word.starts_with("--")
            {
                let (minimized, focused) = ctx.input(|i| (i.viewport().minimized.unwrap_or(false), i.viewport().focused.unwrap_or(false)));
                match self.aside.hear(word, minimized, focused) {
                    Move::Minimize => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                    Move::Restore => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    Move::Stay => {}
                }
                continue;
            }
            self.aside = Aside::default();
            for a in args.iter().filter(|a| !a.starts_with("--")) {
                self.editor.open_path(ctx, a);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.editor.logic(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.editor.close_requested() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.editor.quit_requested {
            self.editor.quit_requested = false;
            if self.editor.close_requested() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.editor.ui(ui);
    }

    fn on_exit(&mut self) {
        store::save_editor_settings(&self.editor.settings);
        self.guard.close();
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
    logging::init("editor");
    log::info!("{}", version::line("ScreenGrabCraft Editor"));
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
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(screengrabcraft_model::EDITOR_NAME)
        .with_app_id(APP_ID)
        .with_inner_size([1320.0, 860.0])
        .with_min_inner_size([760.0, 480.0])
        .with_drag_and_drop(true);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    screengrabcraft_desktop::graphics::configure(&mut options, eframe::wgpu::Backends::from_env());
    let result = eframe::run_native(
        screengrabcraft_model::EDITOR_NAME,
        options,
        Box::new(move |cc| {
            screengrabcraft_desktop::graphics::log_errors(cc);
            let ctx = cc.egui_ctx.clone();
            let settings = store::load_editor_settings();
            let mut editor = Editor::new(Box::new(services::DesktopServices::new(ctx.clone())), settings);
            let wake = ctx.clone();
            guard.set_waker(Box::new(move || wake.request_repaint()));
            for f in &files {
                editor.open_path(&ctx, f);
            }
            Ok(Box::new(App { editor, guard, aside: Aside::default() }))
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

/// A canonical path without Windows' `\\?\` prefix.
fn simplify(p: &std::path::Path) -> String {
    let s = p.to_string_lossy().to_string();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_in_use_goes_down_for_a_capture_and_comes_back() {
        let mut a = Aside::default();
        assert_eq!(a.hear(launch::EDITOR_HIDE, false, true), Move::Minimize);
        assert_eq!(a.hear(launch::EDITOR_SHOW, true, false), Move::Restore);
        assert_eq!(a.hear(launch::EDITOR_SHOW, false, true), Move::Stay, "once");
    }

    #[test]
    fn a_window_behind_another_program_leaves_the_keyboard_alone() {
        let mut a = Aside::default();
        assert_eq!(a.hear(launch::EDITOR_HIDE, false, false), Move::Minimize, "still out of the picture");
        assert_eq!(a.hear(launch::EDITOR_SHOW, true, false), Move::Stay);
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
