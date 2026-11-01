//! The service's window: capture buttons and the settings. Opened from the tray (Settings…), by
//! double-clicking the tray icon, or by starting ScreenGrabCraft again; closing it leaves the
//! service running in the tray.

use std::time::{Duration, Instant};

use egui::{Color32, RichText, Ui, ViewportBuilder, ViewportId, vec2};
use screengrabcraft_desktop::{launch, paths, service};
use screengrabcraft_model::settings::KEYS;
use screengrabcraft_model::{CaptureMode, Hotkey, Hotkeys, ImageFormat, Theme};
use screengrabcraft_ui::icons::{self, Icon};
use screengrabcraft_ui::theme::Tokens;

use crate::app::{APP_ID, ICON, Service};
use crate::hotkeys::logo_name;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Capture,
    After,
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

fn contents(svc: &mut Service, home: &mut Home, ui: &mut Ui, close: &mut bool) {
    let t = Tokens::get(ui.ctx());
    if home.status.is_none() {
        home.status = Some(service::status());
        home.delay = svc.settings.delay_secs.max(1);
    }
    egui::Panel::left("home-nav")
        .exact_size(170.0)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(egui::Margin::same(10)))
        .show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                icons::paint(ui.painter(), egui::Rect::from_min_size(ui.cursor().min, vec2(22.0, 22.0)), Icon::Capture, t.accent);
                ui.add_space(26.0);
                ui.label(RichText::new("ScreenGrabCraft").strong().size(15.0));
            });
            ui.add_space(14.0);
            for (tab, label) in [
                (Tab::Capture, "Capture"),
                (Tab::After, "After a capture"),
                (Tab::Shortcuts, "Shortcuts"),
                (Tab::General, "General"),
                (Tab::About, "About"),
            ] {
                let selected = home.tab == tab;
                let b = egui::Button::new(RichText::new(label).size(14.0)).selected(selected).frame(selected).min_size(vec2(150.0, 30.0));
                if ui.add(b).clicked() {
                    home.tab = tab;
                }
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                if ui.button("Quit ScreenGrabCraft").clicked() {
                    svc.quit = true;
                }
                if ui.button("Close this window").on_hover_text("ScreenGrabCraft keeps running in the tray").clicked() {
                    *close = true;
                }
            });
        });
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::same(18))).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match home.tab {
            Tab::Capture => capture_tab(svc, home, ui, close),
            Tab::After => after_tab(svc, ui),
            Tab::Shortcuts => shortcuts_tab(svc, ui),
            Tab::General => general_tab(svc, home, ui),
            Tab::About => about_tab(svc, ui),
        });
    });
}

fn capture_tab(svc: &mut Service, home: &mut Home, ui: &mut Ui, close: &mut bool) {
    let t = Tokens::get(ui.ctx());
    if !svc.settings.welcomed {
        egui::Frame::NONE.fill(t.selected).corner_radius(8).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.label(RichText::new("ScreenGrabCraft is running").strong().size(16.0));
            ui.label("It lives in the tray (the menu bar on a Mac) and waits for your shortcut. Closing this window keeps it running; it starts again when you sign in, and if it ever stops, it is started again.");
            if let Some(k) = svc.settings.hotkeys.region.as_ref() {
                ui.label(format!("Press {} to capture an area.", k.display(logo_name())));
            }
            if ui.button("Got it").clicked() {
                svc.settings.welcomed = true;
                svc.apply_settings();
            }
        });
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
        if ui.button("Capture Region").clicked() {
            chosen = Some((CaptureMode::Region, home.delay));
        }
    });
    ui.add_space(14.0);
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Open the Editor").clicked() {
            let _ = launch::open_editor(&[]);
        }
        if ui.button("Open the captures folder").clicked() {
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
        if ui.button("Change…").clicked()
            && let Some(p) = rfd::FileDialog::new().set_directory(&dir).pick_folder()
        {
            s.folder = Some(p.to_string_lossy().to_string());
        }
        if s.folder.is_some() && ui.button("Use Pictures/ScreenGrabCraft").clicked() {
            s.folder = None;
        }
        if ui.button("Open").clicked() {
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
        if ui.button("Open Windows keyboard settings").clicked() {
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
        ui.add(egui::Slider::new(&mut s.gif_fps, 1..=30));
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
    if ui.button("Open the log folder").clicked()
        && let Some(d) = paths::log_dir()
    {
        let _ = std::fs::create_dir_all(&d);
        let _ = launch::reveal(&d);
    }
    let _ = Color32::WHITE;
}

fn about_tab(_svc: &mut Service, ui: &mut Ui) {
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
}
