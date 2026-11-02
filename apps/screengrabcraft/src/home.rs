//! The service's window: capture buttons and the settings. Opened from the tray (Settings…), by
//! double-clicking the tray icon, or by starting ScreenGrabCraft again; closing it leaves the
//! service running in the tray.

use std::time::{Duration, Instant};

use egui::{Color32, RichText, Ui, ViewportBuilder, ViewportId, vec2};
use screengrabcraft_desktop::{launch, paths, service};
use screengrabcraft_model::settings::KEYS;
use screengrabcraft_model::settings::VideoOptions;
use screengrabcraft_model::{CaptureMode, Hotkey, Hotkeys, ImageFormat, Theme};
use screengrabcraft_ui::icons::{self, Icon};
use screengrabcraft_ui::theme::Tokens;
use screengrabcraft_ui::widgets::{self, FooterButton};

use crate::app::{APP_ID, ICON, Service};
use crate::hotkeys::logo_name;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Capture,
    After,
    Video,
    Shortcuts,
    General,
    About,
}

#[derive(Default)]
pub struct Home {
    pub focus: bool,
    tab: Tab,
    status: Option<service::Status>,
    message: Option<(String, bool)>,
    delay: u32,
    /// The microphones and cameras, once the Video page has looked.
    devices: Option<Devices>,
}

pub fn show(svc: &mut Service, ctx: &egui::Context) {
    let Some(mut home) = svc.home.take() else { return };
    let mut builder = ViewportBuilder::default()
        .with_title("ScreenGrabCraft")
        .with_app_id(APP_ID)
        .with_inner_size([760.0, 600.0])
        .with_min_inner_size([600.0, 460.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) {
        builder = builder.with_icon(icon);
    }
    let mut close = false;
    ctx.show_viewport_immediate(ViewportId::from_hash_of("home"), builder, |ui, _| {
        if home.focus {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            home.focus = false;
        }
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            close = true;
        }
        contents(svc, &mut home, ui, &mut close);
    });
    if close {
        // Seen once is enough: the welcome doesn't come back at the next sign-in.
        if !svc.settings.welcomed {
            svc.settings.welcomed = true;
            svc.apply_settings();
        }
    } else {
        svc.home = Some(home);
    }
}

/// What the buttons along the window's bottom do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Leave {
    Quit,
    Close,
}

/// The window's footer, across its bottom: Quit set apart at the left, Close at the right.
fn footer(ui: &mut Ui) -> Option<Leave> {
    let t = Tokens::get(ui.ctx());
    let mut leave = None;
    egui::Panel::bottom("home-footer")
        .resizable(false)
        .frame(egui::Frame::NONE.fill(t.footer).inner_margin(egui::Margin::symmetric(widgets::PADDING, 12)))
        .show(ui, |ui| {
            let buttons =
                [FooterButton::aside("Quit ScreenGrabCraft"), FooterButton::primary("Close").tip("ScreenGrabCraft keeps running in the tray")];
            leave = widgets::footer(ui, &buttons).map(|i| if i == 0 { Leave::Quit } else { Leave::Close });
        });
    leave
}

/// A page's entry in the list on the left: highlighted, with a mark of the accent colour, when it
/// is the page shown (as the system's settings do).
fn nav_item(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label));
    let p = ui.painter();
    if selected || resp.hovered() {
        p.rect_filled(rect, widgets::CONTROL_RADIUS, if selected { t.selected } else { t.hover });
    }
    if selected {
        let mark = egui::Rect::from_center_size(egui::pos2(rect.min.x + 2.5, rect.center().y), vec2(3.0, 16.0));
        p.rect_filled(mark, 1.5, t.accent);
    }
    p.text(egui::pos2(rect.min.x + 14.0, rect.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(14.0), t.text);
    resp
}

fn contents(svc: &mut Service, home: &mut Home, ui: &mut Ui, close: &mut bool) {
    let t = Tokens::get(ui.ctx());
    if home.status.is_none() {
        home.status = Some(service::status());
        home.delay = svc.settings.delay_secs.max(1);
    }
    match footer(ui) {
        Some(Leave::Quit) => svc.quit = true,
        Some(Leave::Close) => *close = true,
        None => {}
    }
    nav(ui, &mut home.tab);
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::same(18))).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match home.tab {
            Tab::Capture => capture_tab(svc, home, ui, close),
            Tab::After => after_tab(svc, ui),
            Tab::Video => video_tab(svc, home, ui),
            Tab::Shortcuts => shortcuts_tab(svc, ui),
            Tab::General => general_tab(svc, home, ui),
            Tab::About => about_tab(svc, ui),
        });
    });
}

/// The list of pages on the left.
fn nav(ui: &mut Ui, tab: &mut Tab) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::left("home-nav")
        .exact_size(170.0)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(egui::Margin::same(10)))
        .show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                icons::paint(ui.painter(), egui::Rect::from_min_size(ui.cursor().min, vec2(22.0, 22.0)), Icon::Capture, t.accent);
                ui.add_space(26.0);
                ui.label(RichText::new("ScreenGrabCraft").strong().size(15.0));
            });
            ui.add_space(14.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            for (page, label) in [
                (Tab::Capture, "Capture"),
                (Tab::After, "After a capture"),
                (Tab::Video, "Video"),
                (Tab::Shortcuts, "Shortcuts"),
                (Tab::General, "General"),
                (Tab::About, "About"),
            ] {
                if nav_item(ui, label, *tab == page).clicked() {
                    *tab = page;
                }
            }
        });
}

/// The first-run hint: what ScreenGrabCraft does from now on, and [Got it] at its bottom right.
/// True when Got it was pressed.
fn welcome(ui: &mut Ui, region_key: Option<String>) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut seen = false;
    egui::Frame::NONE
        .fill(t.hover)
        .stroke(egui::Stroke::new(1.0, t.border))
        .corner_radius(widgets::WINDOW_RADIUS)
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.label(RichText::new("ScreenGrabCraft is running").font(widgets::strong_font(ui.ctx(), 15.0)));
            ui.add_space(2.0);
            ui.label("It lives in the tray (the menu bar on a Mac) and waits for your shortcut. Closing this window keeps it running; it starts again when you sign in, and if it ever stops, it is started again.");
            if let Some(k) = region_key {
                ui.label(format!("Press {k} to capture an area."));
            }
            ui.add_space(6.0);
            seen = widgets::footer(ui, &[FooterButton::primary("Got it")]).is_some();
        });
    seen
}

fn capture_tab(svc: &mut Service, home: &mut Home, ui: &mut Ui, close: &mut bool) {
    if !svc.settings.welcomed {
        if welcome(ui, svc.settings.hotkeys.region.as_ref().map(|k| k.display(logo_name()))) {
            svc.settings.welcomed = true;
            svc.apply_settings();
        }
        ui.add_space(12.0);
    }
    ui.heading("Capture");
    ui.add_space(8.0);
    let modes = [
        (CaptureMode::Region, Icon::Region, "Drag out an area, or click a window"),
        (CaptureMode::Window, Icon::Window, "Click a window"),
        (CaptureMode::Screen, Icon::Screen, "The screen under the pointer"),
        (CaptureMode::AllScreens, Icon::AllScreens, "Every monitor in one picture"),
        (CaptureMode::Scrolling, Icon::Scrolling, "A long page, as you scroll it"),
        (CaptureMode::Gif, Icon::Gif, "Record an area as an animated GIF"),
        (CaptureMode::Video, Icon::Play, "Record an area as a small MP4"),
        (CaptureMode::Production, Icon::Edit, "Screen, sound, microphone and camera, to edit"),
        (CaptureMode::Repeat, Icon::Repeat, "The same area as last time"),
    ];
    let mut chosen: Option<(CaptureMode, u32)> = None;
    egui::Grid::new("capture-buttons").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
        for (i, (mode, icon, help)) in modes.iter().enumerate() {
            let shortcut = svc.keys.label(&svc.settings.hotkeys, *mode).unwrap_or_default();
            if big_button(ui, *icon, mode.label(), help, &shortcut).clicked() {
                chosen = Some((*mode, 0));
            }
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.label("After a delay of");
        ui.add(egui::DragValue::new(&mut home.delay).range(1..=60).suffix(" s"));
        if widgets::button(ui, "Capture Region").clicked() {
            chosen = Some((CaptureMode::Region, home.delay));
        }
    });
    ui.add_space(14.0);
    ui.separator();
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if widgets::button(ui, "Open the Editor").clicked() {
            let _ = launch::open_editor(&[]);
        }
        if widgets::button(ui, "Open the captures folder").clicked() {
            let d = paths::captures_dir(&svc.settings);
            let _ = std::fs::create_dir_all(&d);
            let _ = launch::reveal(&d);
        }
    });
    if let Some((mode, delay)) = chosen {
        // The window goes first, so it isn't in the picture.
        *close = true;
        if delay > 0 {
            svc.start(mode, delay);
        } else {
            svc.after_countdown = Some((mode, Instant::now() + Duration::from_millis(300)));
        }
    }
}

fn big_button(ui: &mut Ui, icon: Icon, label: &str, help: &str, shortcut: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let size = vec2(262.0, if shortcut.is_empty() { 54.0 } else { 70.0 });
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let bg = if resp.hovered() { t.hover } else { t.window };
    ui.painter().rect_filled(rect, 8.0, bg);
    ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    icons::paint(ui.painter(), egui::Rect::from_min_size(rect.min + vec2(12.0, (size.y - 28.0) / 2.0), vec2(28.0, 28.0)), icon, t.accent);
    ui.painter().text(rect.min + vec2(52.0, 9.0), egui::Align2::LEFT_TOP, label, egui::FontId::proportional(14.5), t.text);
    ui.painter().text(rect.min + vec2(52.0, 29.0), egui::Align2::LEFT_TOP, help, egui::FontId::proportional(11.5), t.muted);
    if !shortcut.is_empty() {
        // The shortcut on a line of its own, like a key cap.
        let galley = ui.painter().layout_no_wrap(shortcut.to_string(), egui::FontId::monospace(10.5), t.text);
        let cap = egui::Rect::from_min_size(rect.min + vec2(52.0, 47.0), galley.size() + vec2(10.0, 4.0));
        ui.painter().rect_filled(cap, 4.0, t.panel);
        ui.painter().rect_stroke(cap, 4.0, egui::Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        ui.painter().galley(cap.min + vec2(5.0, 2.0), galley, t.text);
    }
    resp
}

fn after_tab(svc: &mut Service, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let mut s = svc.settings.clone();
    ui.heading("After a capture");
    ui.add_space(8.0);
    ui.label(RichText::new("The default").strong());
    ui.label(RichText::new("What happens to a capture once you have chosen the area.").size(12.0).color(t.muted));
    ui.add_space(4.0);
    ui.checkbox(&mut s.after_capture.copy, "Copy it to the clipboard");
    ui.checkbox(&mut s.after_capture.open_editor, "Open it in the Editor");
    ui.checkbox(&mut s.after_capture.save, "Save it in the captures folder");
    ui.add_space(12.0);
    ui.checkbox(&mut s.after_capture.show_bar, "Stop first, with a bar to adjust the area or choose Edit, Copy or Save");
    ui.label(RichText::new("Enter (or a double-click) then does the default. Off, the default happens at once.").size(12.0).color(t.muted));
    ui.add_space(6.0);
    ui.checkbox(&mut s.hide_editor, "Hide the Editor while capturing");
    ui.label(RichText::new("So it isn't in the picture. It comes back with the new capture.").size(12.0).color(t.muted));
    ui.add_space(12.0);
    ui.label(RichText::new("Captures folder").strong());
    let dir = paths::captures_dir(&s);
    ui.horizontal(|ui| {
        ui.label(RichText::new(dir.to_string_lossy()).monospace().size(12.0));
    });
    ui.horizontal(|ui| {
        if widgets::button(ui, "Change…").clicked()
            && let Some(p) = rfd::FileDialog::new().set_directory(&dir).pick_folder()
        {
            s.folder = Some(p.to_string_lossy().to_string());
        }
        if s.folder.is_some() && widgets::button(ui, "Use Pictures/ScreenGrabCraft").clicked() {
            s.folder = None;
        }
        if widgets::button(ui, "Open").clicked() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = launch::reveal(&dir);
        }
    });
    ui.add_space(12.0);
    egui::Grid::new("naming").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("File name");
        ui.add(egui::TextEdit::singleline(&mut s.file_name).desired_width(280.0));
        ui.end_row();
        ui.label("");
        let example = screengrabcraft_model::settings::expand_name(&s.file_name, screengrabcraft_desktop::time::now(), "Notepad", 1);
        ui.label(RichText::new(format!("{example}.{}   ·   {{date}} {{time}} {{source}} {{n}}", s.format.extension())).size(12.0).color(t.muted));
        ui.end_row();
        ui.label("Format");
        egui::ComboBox::from_id_salt("format").selected_text(s.format.label()).show_ui(ui, |ui| {
            for f in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Webp, ImageFormat::Bmp, ImageFormat::Pdf] {
                ui.selectable_value(&mut s.format, f, f.label());
            }
        });
        ui.end_row();
        if s.format == ImageFormat::Jpeg {
            ui.label("JPEG quality");
            ui.add(egui::Slider::new(&mut s.jpeg_quality, 10..=100));
            ui.end_row();
        }
    });
    ui.add_space(12.0);
    ui.label(RichText::new("While choosing").strong());
    ui.checkbox(&mut s.magnifier, "Show the magnifier next to the pointer");
    ui.checkbox(&mut s.crosshair, "Show crosshair lines");
    ui.checkbox(&mut s.include_cursor, "Include the mouse pointer in captures");
    if s != svc.settings {
        svc.settings = s;
        svc.apply_settings();
    }
}

fn hotkey_row(ui: &mut Ui, label: &str, key: &mut Option<Hotkey>, default: Option<Hotkey>) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.horizontal(|ui| {
        let mut on = key.is_some();
        let mut k = key.clone().unwrap_or_else(|| default.clone().unwrap_or_else(|| Hotkey::new(true, false, true, false, "A")));
        if ui.checkbox(&mut on, "").on_hover_text("Use a shortcut for this").changed() {
            changed = true;
        }
        ui.add_enabled_ui(on, |ui| {
            changed |= ui.toggle_value(&mut k.ctrl, "Ctrl").changed();
            changed |= ui.toggle_value(&mut k.alt, if cfg!(target_os = "macos") { "Option" } else { "Alt" }).changed();
            changed |= ui.toggle_value(&mut k.shift, "Shift").changed();
            changed |= ui.toggle_value(&mut k.logo, logo_name()).changed();
            egui::ComboBox::from_id_salt(("key", label)).width(110.0).selected_text(k.key.clone()).show_ui(ui, |ui| {
                for name in KEYS {
                    if ui.selectable_label(k.key == *name, *name).clicked() {
                        k.key = (*name).to_string();
                        changed = true;
                    }
                }
            });
        });
        if default.is_some() && *key != default && ui.small_button("Default").clicked() {
            k = default.clone().unwrap_or(k.clone());
            on = true;
            changed = true;
        }
        if changed {
            *key = on.then_some(k);
        }
    });
    ui.end_row();
    changed
}

fn shortcuts_tab(svc: &mut Service, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.heading("Shortcuts");
    ui.label(RichText::new("They work in every program, while ScreenGrabCraft runs.").color(t.muted));
    ui.add_space(10.0);
    let mut keys: Hotkeys = svc.settings.hotkeys.clone();
    let defaults = Hotkeys::default();
    let mut changed = false;
    egui::Grid::new("hotkeys").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        for mode in CaptureMode::ALL {
            let mut k = keys.get(mode).cloned();
            if hotkey_row(ui, mode.label(), &mut k, defaults.get(mode).cloned()) {
                keys.set(mode, k);
                changed = true;
            }
        }
        let mut e = keys.editor.clone();
        if hotkey_row(ui, "Open the Editor", &mut e, defaults.editor.clone()) {
            keys.editor = e;
            changed = true;
        }
    });
    if changed {
        svc.settings.hotkeys = keys;
        svc.apply_settings();
    }
    ui.add_space(10.0);
    for (a, b) in svc.settings.hotkeys.clashes() {
        ui.label(RichText::new(format!("{a} and {b} have the same shortcut.")).color(t.danger));
    }
    for p in &svc.keys.problems {
        ui.label(RichText::new(p).color(t.danger));
    }
    if cfg!(target_os = "windows") {
        ui.add_space(10.0);
        ui.label(RichText::new("If Print Screen opens the Snipping Tool instead, turn off “Use the Print screen key to open screen capture” in Windows' keyboard settings.").size(12.0).color(t.muted));
        if widgets::button(ui, "Open Windows keyboard settings").clicked() {
            let _ = launch::open_url("ms-settings:easeofaccess-keyboard");
        }
    }
    if !svc.keys.available() || std::env::var_os("WAYLAND_DISPLAY").is_some() {
        ui.add_space(10.0);
        ui.label(RichText::new("On Wayland, bind keys in your desktop's keyboard settings to these commands:").size(12.0).color(t.muted));
        for m in CaptureMode::ALL {
            ui.label(RichText::new(format!("screengrabcraft --capture {}", m.arg())).monospace().size(12.0));
        }
    }
}

fn general_tab(svc: &mut Service, home: &mut Home, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.heading("General");
    ui.add_space(8.0);
    let status = home.status.clone().unwrap_or_else(service::status);
    let mut on = status.enabled;
    if ui.checkbox(&mut on, "Start ScreenGrabCraft when I sign in").changed() {
        let result = if on { service::enable() } else { service::disable() };
        home.message = Some(match result {
            Ok(m) => (m, false),
            Err(e) => (e, true),
        });
        svc.settings.start_at_login = Some(on);
        svc.apply_settings();
        home.status = Some(service::status());
    }
    ui.label(
        RichText::new(format!("It runs in the background and is started again if it stops. Arranged through {}.", status.how))
            .size(12.0)
            .color(t.muted),
    );
    if let Some((m, bad)) = &home.message {
        ui.label(RichText::new(m).color(if *bad { t.danger } else { t.muted }));
    }
    ui.add_space(14.0);
    let mut s = svc.settings.clone();
    ui.label(RichText::new("Look").strong());
    ui.horizontal(|ui| {
        for (th, label) in [(Theme::System, "Follow the system"), (Theme::Light, "Light"), (Theme::Dark, "Dark")] {
            ui.radio_value(&mut s.theme, th, label);
        }
    });
    ui.add_space(14.0);
    ui.label(RichText::new("GIF recordings").strong());
    egui::Grid::new("gif").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Frames a second");
        ui.add(egui::Slider::new(&mut s.gif_frame_rate, 1..=30));
        ui.end_row();
        ui.label("Longest recording");
        ui.add(egui::Slider::new(&mut s.gif_max_secs, 5..=600).suffix(" s"));
        ui.end_row();
    });
    if s != svc.settings {
        let theme_changed = s.theme != svc.settings.theme;
        svc.settings = s;
        svc.apply_settings();
        if theme_changed {
            screengrabcraft_ui::theme::apply(ui.ctx(), svc.settings.theme);
        }
    }
    ui.add_space(14.0);
    if widgets::button(ui, "Open the log folder").clicked()
        && let Some(d) = paths::log_dir()
    {
        let _ = std::fs::create_dir_all(&d);
        let _ = launch::reveal(&d);
    }
    let _ = Color32::WHITE;
}

fn about_tab(svc: &mut Service, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.heading("ScreenGrabCraft");
    ui.label(screengrabcraft_desktop::version::line("ScreenGrabCraft"));
    ui.add_space(8.0);
    ui.label("Capture the screen, mark it up, share it. An open-source screen capture suite written in Rust: this capture service, and ScreenGrabCraft Editor.");
    ui.add_space(8.0);
    ui.label(RichText::new(format!("Screen capture: {}", screengrabcraft_capture::backend_name())).size(12.0).color(t.muted));
    ui.label(
        RichText::new(format!("Settings: {}", paths::settings_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()))
            .size(12.0)
            .color(t.muted),
    );
    ui.add_space(8.0);
    ui.hyperlink_to("github.com/billsecond/screengrabcraft", screengrabcraft_model::HOMEPAGE);
    ui.add_space(8.0);
    ui.label(RichText::new("MIT OR Apache-2.0. Fonts: PatroneOS Sans and PatroneOS Mono (SIL Open Font Licence).").size(12.0).color(t.muted));
    crate::update::about(svc, ui);
}

/// The microphones and cameras a recording can use (looked up when the Video page is first
/// shown: nothing is turned on).
pub struct Devices {
    microphones: Vec<String>,
    cameras: Vec<(u32, String)>,
    system_sound: bool,
}

impl Devices {
    fn find() -> Devices {
        Devices {
            microphones: screengrabcraft_capture::audio::microphones(),
            cameras: screengrabcraft_capture::camera::cameras(),
            system_sound: screengrabcraft_capture::audio::system_sound_supported(),
        }
    }
}

/// What a kind of video recording takes in: the computer's sound, a microphone, a camera.
fn sources(ui: &mut Ui, id: &str, o: &mut VideoOptions, d: &Devices, camera: bool) {
    let t = Tokens::get(ui.ctx());
    ui.add_enabled_ui(d.system_sound, |ui| {
        ui.checkbox(&mut o.system_sound, "The computer's sound").on_disabled_hover_text("This system has no way to record what it plays");
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut o.microphone, "The microphone");
        ui.add_enabled_ui(o.microphone, |ui| {
            let shown = o.microphone_name.clone().unwrap_or_else(|| "The system's default".to_string());
            egui::ComboBox::from_id_salt((id, "mic")).width(240.0).selected_text(shown).show_ui(ui, |ui| {
                ui.selectable_value(&mut o.microphone_name, None, "The system's default");
                for m in &d.microphones {
                    ui.selectable_value(&mut o.microphone_name, Some(m.clone()), m);
                }
            });
        });
    });
    if d.microphones.is_empty() && o.microphone {
        ui.label(RichText::new("No microphone was found just now.").size(12.0).color(t.muted));
    }
    if camera {
        ui.horizontal(|ui| {
            ui.checkbox(&mut o.camera, "The camera");
            ui.add_enabled_ui(o.camera, |ui| {
                let shown = o
                    .camera_index
                    .and_then(|i| d.cameras.iter().find(|c| c.0 == i).map(|c| c.1.clone()))
                    .unwrap_or_else(|| d.cameras.first().map(|c| c.1.clone()).unwrap_or_else(|| "The first one".to_string()));
                egui::ComboBox::from_id_salt((id, "camera")).width(240.0).selected_text(shown).show_ui(ui, |ui| {
                    for (i, name) in &d.cameras {
                        ui.selectable_value(&mut o.camera_index, Some(*i), name);
                    }
                });
            });
        });
        if d.cameras.is_empty() && o.camera {
            ui.label(RichText::new("No camera was found just now.").size(12.0).color(t.muted));
        }
    }
}

fn video_tab(svc: &mut Service, home: &mut Home, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let d = home.devices.get_or_insert_with(Devices::find);
    let mut s = svc.settings.clone();
    ui.heading("Video");
    ui.add_space(8.0);
    ui.label(RichText::new("Record Video").strong());
    ui.label(RichText::new("A small MP4 of an area, made as soon as you stop.").size(12.0).color(t.muted));
    ui.add_space(4.0);
    sources(ui, "video", &mut s.video, d, false);
    ui.checkbox(&mut s.video_full_size, "Keep the full size (else no larger than 1920 × 1080, for smaller files)");
    ui.add_space(14.0);
    ui.label(RichText::new("Video Production").strong());
    ui.label(RichText::new("Each on a track of its own, to cut and edit in the Editor before you make the MP4.").size(12.0).color(t.muted));
    ui.add_space(4.0);
    sources(ui, "production", &mut s.production, d, true);
    ui.add_space(14.0);
    ui.label(RichText::new("Both").strong());
    egui::Grid::new("video-both").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Frames a second");
        ui.add(egui::Slider::new(&mut s.video_frame_rate, 5..=30));
        ui.end_row();
        ui.label("Longest recording");
        ui.add(egui::DragValue::new(&mut s.video_max_mins).range(1..=240).suffix(" minutes"));
        ui.end_row();
    });
    ui.add_space(6.0);
    if widgets::button(ui, "Look for devices again").clicked() {
        home.devices = None;
    }
    if s != svc.settings {
        svc.settings = s;
        svc.apply_settings();
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;
    use screengrabcraft_model::Theme;

    use super::*;

    /// The settings window's frame: its footer, its pages, and what was pressed.
    fn window() -> Harness<'static, (Option<Leave>, Tab)> {
        Harness::builder().with_size(vec2(760.0, 600.0)).build_ui_state(
            |ui, (leave, tab): &mut (Option<Leave>, Tab)| {
                screengrabcraft_ui::theme::apply(ui.ctx(), Theme::Light);
                if let Some(l) = footer(ui) {
                    *leave = Some(l);
                }
                nav(ui, tab);
                egui::CentralPanel::default().show(ui, |_| {});
            },
            (None, Tab::Capture),
        )
    }

    #[test]
    fn quit_sits_apart_at_the_bottom_left_and_close_at_the_bottom_right() {
        let mut h = window();
        h.run_steps(2);
        let (quit, close) = (h.get_by_label("Quit ScreenGrabCraft").rect(), h.get_by_label("Close").rect());
        assert!(quit.min.x < 30.0 && close.max.x > 730.0, "{quit:?} {close:?}");
        assert!(quit.max.y > 560.0 && (quit.center().y - close.center().y).abs() < 0.5, "both along the bottom: {quit:?} {close:?}");
        assert!(close.width() >= widgets::BUTTON_SIZE.x);
        h.get_by_label("Close").click();
        h.run_steps(1);
        assert_eq!(h.state().0, Some(Leave::Close));
        h.get_by_label("Quit ScreenGrabCraft").click();
        h.run_steps(1);
        assert_eq!(h.state().0, Some(Leave::Quit));
    }

    #[test]
    fn the_pages_are_chosen_from_the_left() {
        let mut h = window();
        h.run_steps(2);
        h.get_by_label("Shortcuts").click();
        h.run_steps(1);
        assert_eq!(h.state().1, Tab::Shortcuts);
        assert!(h.get_by_label("About").rect().max.x <= 170.0);
    }

    #[test]
    fn got_it_is_at_the_bottom_right_of_the_welcome() {
        let mut h = Harness::builder().with_size(vec2(560.0, 300.0)).build_ui_state(
            |ui, seen: &mut bool| {
                screengrabcraft_ui::theme::apply(ui.ctx(), Theme::Dark);
                *seen |= welcome(ui, Some("Print Screen".into()));
            },
            false,
        );
        h.run_steps(2);
        let got = h.get_by_label("Got it").rect();
        assert!(got.max.x > 520.0 && got.width() >= widgets::BUTTON_SIZE.x, "{got:?}");
        h.get_by_label("Got it").click();
        h.run_steps(1);
        assert!(*h.state());
    }
}
