//! ScreenGrabCraft: the screen capture service in the tray.
//!
//! ```text
//! screengrabcraft                      start the service (or show the running one's window)
//! screengrabcraft --capture <mode>     capture now: region, window, screen, all, repeat,
//!                                      scrolling or gif (the running service does it)
//! screengrabcraft --delay <seconds>    capture an area after a countdown
//! screengrabcraft --settings           show the settings
//! screengrabcraft --quit               stop the running service
//! screengrabcraft --background         the sign-in entry: run the service and start it again
//!                                      if it stops unexpectedly
//! screengrabcraft --tray               run the service itself (what launchd and systemd run)
//! screengrabcraft service install|uninstall|status
//!                                      start at sign-in, or not
//! screengrabcraft --version
//! ```
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod app;
mod home;
mod hotkeys;
mod popups;
mod record;
mod tray;

use std::process::ExitCode;

use screengrabcraft_desktop::{graphics, instance, logging, paths, service, store, version};

const USAGE: &str = "usage: screengrabcraft [--capture region|window|screen|all|repeat|scrolling|gif] [--delay <s>] [--settings] [--quit]
       screengrabcraft --background | --tray
       screengrabcraft service install|uninstall|status
       screengrabcraft --version";

struct App(app::Service);

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.0.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // The anchor window is see-through.
        [0.0; 4]
    }

    fn on_exit(&mut self) {
        self.0.guard.close();
        log::info!("the capture service stopped");
    }
}

/// Whether this is a build in the source tree (it never registers itself to start at sign-in).
fn development_build() -> bool {
    std::env::current_exe().is_ok_and(|p| {
        let s = p.to_string_lossy().replace('\\', "/");
        s.contains("/target/") || s.contains("/target-")
    })
}

fn service_command(what: Option<&str>) -> ExitCode {
    let result = match what {
        Some("install") => service::enable(),
        Some("uninstall") => service::disable(),
        Some("status") => {
            let s = service::status();
            Ok(format!("{} ({})", if s.enabled { "starts when you sign in" } else { "does not start when you sign in" }, s.how))
        }
        _ => Err("usage: screengrabcraft service install|uninstall|status".to_string()),
    };
    match result {
        Ok(m) => {
            println!("{m}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", version::line("ScreenGrabCraft"));
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if args.first().map(String::as_str) == Some("service") {
        return service_command(args.get(1).map(String::as_str));
    }
    logging::init("service");
    if args.iter().any(|a| a == "--background") {
        let Ok(exe) = std::env::current_exe() else { return ExitCode::FAILURE };
        log::info!("{}: supervising the capture service", version::line("ScreenGrabCraft"));
        let code = service::supervise(&exe, &[]);
        return ExitCode::from(u8::try_from(code.clamp(0, 255)).unwrap_or(1));
    }
    let started_by_system = args.iter().any(|a| a == "--tray");
    // Started by the system while it runs: nothing to show. Started by the owner: show it.
    let handover = if started_by_system { vec!["--noop".to_string()] } else { args.clone() };
    let guard = match instance::start("service", &handover) {
        instance::Start::First(g) => g,
        instance::Start::Handed => return ExitCode::SUCCESS,
        instance::Start::Unreachable(e) => {
            log::error!("another ScreenGrabCraft is running but couldn't be reached: {e}");
            return ExitCode::FAILURE;
        }
    };
    if args.iter().any(|a| a == "--quit") {
        // Nothing was running to quit.
        return ExitCode::SUCCESS;
    }
    log::info!("{} starting ({})", version::line("ScreenGrabCraft"), screengrabcraft_capture::backend_name());

    let mut settings = store::load_settings();
    let first_run = settings.start_at_login.is_none();
    if first_run && !paths::no_prefs() && !development_build() {
        match service::enable() {
            Ok(m) => log::info!("{m}"),
            Err(e) => log::warn!("starting at sign-in could not be arranged: {e}"),
        }
        settings.start_at_login = Some(true);
        store::save_settings(&settings);
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ScreenGrabCraft")
        .with_app_id(app::APP_ID)
        .with_visible(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_inner_size([1.0, 1.0])
        .with_min_inner_size([1.0, 1.0])
        // Out of sight where the system allows it (a 1-pixel window that may not be see-through).
        .with_position(if cfg!(target_os = "windows") { [-32000.0, -32000.0] } else { [0.0, 0.0] })
        .with_taskbar(false)
        .with_mouse_passthrough(true)
        .with_resizable(false)
        .with_active(false);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(app::ICON) {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        // A menu-bar program: no Dock icon.
        options.event_loop_builder = Some(Box::new(|b| {
            b.with_activation_policy(ActivationPolicy::Accessory);
        }));
    }
    let backends = eframe::wgpu::Backends::from_env().or_else(graphics::fallback_backends);
    graphics::configure(&mut options, backends);
    let theme = settings.theme;
    let welcomed = settings.welcomed;
    let result = eframe::run_native(
        "ScreenGrabCraft",
        options,
        Box::new(move |cc| {
            graphics::log_errors(cc);
            screengrabcraft_ui::setup(&cc.egui_ctx, theme);
            let mut svc = app::Service::new(cc.egui_ctx.clone(), guard, settings);
            let tray_missing = svc.tray.as_ref().is_none_or(|t| !t.available());
            if !started_by_system || !welcomed || tray_missing {
                // Started by the owner, for the first time, or with no tray to reach it by.
                let wanted: Vec<String> = args.iter().filter(|a| a.as_str() != "--tray").cloned().collect();
                svc.guard_args(&wanted);
            }
            Ok(Box::new(App(svc)))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("the capture service could not start: {e}");
            ExitCode::FAILURE
        }
    }
}
