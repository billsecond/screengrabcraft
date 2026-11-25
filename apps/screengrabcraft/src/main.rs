//! ScreenGrabCraft: the screen capture service in the tray.
//!
//! ```text
//! screengrabcraft                      start the service (or show the running one's window)
//! screengrabcraft --capture <mode>     capture now: region, window, screen, all, repeat,
//!                                      scrolling, gif or video (the running service does it);
//!                                      production opens the Recorder (the Start menu's
//!                                      ScreenGrabCraft Recorder)
//! screengrabcraft --preset <name>      take a capture preset (by its name or its number);
//!                                      also --capture preset:<name>
//! screengrabcraft --delay <seconds>    capture an area after a countdown
//! screengrabcraft --settings           show the settings
//! screengrabcraft --check-updates      look for a newer release now (and say what was found)
//! screengrabcraft --quit               stop the running service
//! screengrabcraft --background         the sign-in entry: run the service and start it again
//!                                      if it stops unexpectedly
//! screengrabcraft --tray               run the service itself (what launchd and systemd run)
//! screengrabcraft --transcribe <file>  make the captions of a Video Production recording (what
//!                                      the service starts, at low priority, after recording)
//! screengrabcraft service install|uninstall|status
//!                                      start at sign-in, or not
//! screengrabcraft --version
//! ```
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod app;
mod captions;
mod film;
#[cfg(test)]
mod fit_check;
mod home;
mod hotkeys;
mod ink;
mod popups;
mod record;
mod recorder;
mod steps;
mod timed;
mod tray;
mod update;

use std::process::ExitCode;

use screengrabcraft_desktop::{graphics, instance, logging, paths, service, store, version};

const USAGE: &str =
    "usage: screengrabcraft [--capture region|window|screen|all|repeat|scrolling|gif|video|production|preset:<name>] [--delay <s>] [--settings] [--quit]
       screengrabcraft --preset <name or number>
       screengrabcraft --check-updates
       screengrabcraft --background | --tray
       screengrabcraft --transcribe <recording.sgvideo>
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
        self.0.captions.stop();
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

/// The service's root window: an anchor eframe needs and nobody sees. As small as a window can be
/// ([`app::SMALLEST_WINDOW`]: at 1 point, Wayland desktops ended the service as it started), out of
/// sight where the system allows it (it may not be see-through), and never in the way.
fn anchor() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("ScreenGrabCraft")
        .with_app_id(app::APP_ID)
        .with_visible(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_inner_size(app::SMALLEST_WINDOW)
        .with_min_inner_size(app::SMALLEST_WINDOW)
        .with_position(if cfg!(target_os = "windows") { [-32000.0, -32000.0] } else { [0.0, 0.0] })
        .with_taskbar(false)
        .with_mouse_passthrough(true)
        .with_resizable(false)
        .with_active(false)
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
    // A recording's camera, run by the capture service in a program of its own.
    if let Some(code) = screengrabcraft_capture::camera::helper_main(&args) {
        return ExitCode::from(code);
    }
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
    logging::init();
    if let Some(i) = args.iter().position(|a| a == captions::ARG) {
        // Its own log: what the service started it for, and how long it took.
        logging::to_file("captions");
        let Some(path) = args.get(i + 1) else {
            eprintln!("usage: screengrabcraft --transcribe <recording, sound file or video> [--again]");
            return ExitCode::FAILURE;
        };
        let again = args.get(i + 2).is_some_and(|a| a == captions::AGAIN);
        return captions::run(path, again);
    }
    if args.iter().any(|a| a == "--background") {
        logging::to_file("supervisor");
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
    // The copy that stays: its log is the one kept.
    logging::to_file("service");
    log::info!("{} starting ({})", version::line("ScreenGrabCraft"), screengrabcraft_capture::backend_name());
    // A recording's camera runs in a copy of this program (`--camera-helper`): one that locks up
    // is let go of as soon as the recording ends.
    if let Ok(exe) = std::env::current_exe() {
        screengrabcraft_capture::camera::use_helper(exe);
    }

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

    let mut viewport = anchor();
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
            // eframe shows the anchor window after its first frame, made hidden or not, and Windows
            // keeps it on a monitor (at the top left), where it can't be see-through everywhere:
            // cloaked, it is never seen. Nor animated.
            screengrabcraft_desktop::cloak::calm_windows();
            if cfg!(target_os = "windows") && !screengrabcraft_desktop::cloak::cloak(cc, true) {
                log::warn!("the anchor window couldn't be cloaked; it may be seen at the top left of a monitor");
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The hidden root window was 1 point wide and couldn't be resized, so on Wayland winit asked
    /// for it to be at least 2 wide and at most 1, and the desktop ended the service every time
    /// it started ("Invalid min/max size"): no tray icon, ever.
    #[test]
    fn the_hidden_window_is_one_wayland_takes() {
        let old = egui::ViewportBuilder::default().with_inner_size([1.0, 1.0]).with_resizable(false);
        assert!(!app::wayland_accepts(&old), "what 0.1.60 asked for");
        let b = anchor();
        assert!(app::wayland_accepts(&b));
        assert_eq!(b.resizable, Some(false));
        assert!(b.inner_size.is_some_and(|s| s.x >= 2.0 && s.y >= 1.0));
    }

    /// The Start menu's ScreenGrabCraft Recorder (Windows) and the Record action (Linux) start a
    /// new recording the way the tray's red-dot item does: they open the Recorder.
    #[test]
    fn the_start_menus_recorder_opens_the_recorder() {
        use screengrabcraft_model::CaptureMode;
        let wxs = include_str!("../../../packaging/windows/screengrabcraft.wxs");
        let at = wxs.find(r#"Name="ScreenGrabCraft Recorder""#).unwrap();
        let tag = &wxs[wxs[..at].rfind("<Shortcut").unwrap()..at + wxs[at..].find('>').unwrap()];
        assert!(tag.contains(r#"Directory="ProgramMenuFolder""#), "{tag}");
        let windows = tag.split(" Arguments=\"").nth(1).and_then(|a| a.split('"').next()).unwrap();
        let desktop = include_str!("../../../packaging/linux/io.github.billsecond.screengrabcraft.desktop");
        let linux = desktop.lines().skip_while(|l| *l != "[Desktop Action record]").find_map(|l| l.strip_prefix("Exec=screengrabcraft ")).unwrap();
        for args in [windows, linux] {
            let words: Vec<&str> = args.split_whitespace().collect();
            assert_eq!(words.first(), Some(&"--capture"), "{args}");
            assert_eq!(words.get(1).and_then(|w| CaptureMode::from_arg(w)), Some(CaptureMode::Production), "{args}");
            assert_eq!(words.len(), 2, "{args}");
        }
        // Video Production is the tray's record item, the one with the red dot.
        let menu = tray::entries(&|_| None, &[], None, tray::Running::default());
        assert!(menu.iter().any(|e| matches!(e, tray::Entry::Marked(_, tray::Item::Capture(CaptureMode::Production)))));
    }
}
