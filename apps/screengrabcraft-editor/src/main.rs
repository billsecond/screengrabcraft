//! ScreenGrabCraft Editor: annotate, mark up and share screenshots.
//!
//! Usage: `screengrabcraft-editor [--version] [pictures or .sgcraft projects…]`
//!
//! One editor window at a time: started again (a capture handed over by the capture service, a
//! file opened from the file manager), it hands the files to the window already open and exits.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod services;

use screengrabcraft_desktop::{instance, logging, store, version};
use screengrabcraft_ui::Editor;

const APP_ID: &str = "io.github.billsecond.screengrabcraft.editor";
static ICON: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/io.github.billsecond.screengrabcraft.editor.png");

struct App {
    editor: Editor,
    guard: instance::Guard,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for args in self.guard.handed() {
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
            let ctx = cc.egui_ctx.clone();
            let settings = store::load_editor_settings();
            let mut editor = Editor::new(Box::new(services::DesktopServices::new(ctx.clone())), settings);
            let wake = ctx.clone();
            guard.set_waker(Box::new(move || wake.request_repaint()));
            for f in &files {
                editor.open_path(&ctx, f);
            }
            Ok(Box::new(App { editor, guard }))
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
