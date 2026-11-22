//! The service's window: capture buttons and the settings. Opened from the tray (Settings…), by
//! double-clicking the tray icon, or by starting ScreenGrabCraft again; closing it leaves the
//! service running in the tray.

use std::time::{Duration, Instant};

use egui::{Color32, RichText, Ui, ViewportBuilder, ViewportId, vec2};
use screengrabcraft_desktop::{launch, paths, service};
use screengrabcraft_model::settings::KEYS;
use screengrabcraft_model::settings::VideoOptions;
use screengrabcraft_model::settings::{FIXED_MAX, FIXED_MIN, PRESETS_MAX, SHAPES};
use screengrabcraft_model::{CaptureMode, CapturePreset, Hotkey, Hotkeys, ImageFormat, PresetArea, Theme};
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
    Presets,
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
    /// The microphones and cameras, once the Recording page has looked.
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
            Tab::Presets => presets_tab(svc, ui, close),
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
                (Tab::Presets, "Capture presets"),
                (Tab::Video, "Recording"),
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
    let chosen = capture_buttons(ui, &mut home.delay, &|mode| svc.keys.label(&svc.settings.hotkeys, mode).unwrap_or_default());
    if !svc.settings.presets.is_empty() {
        ui.add_space(16.0);
        if let Some(i) = preset_buttons(ui, &svc.settings.presets, &|p| svc.keys.preset_label(p)) {
            // The window goes first, so it isn't in the picture.
            *close = true;
            svc.run_preset_soon(i);
        }
    }
    ui.add_space(16.0);
    let waiting = svc.timed.at.map(|_| svc.settings.timed.at.clone());
    let status = svc.timed.problem.clone().unwrap_or_else(|| svc.timed.status());
    let (changed, action) = timed_section(ui, &mut svc.settings.timed, svc.timed.running(), &status, waiting.as_deref());
    if changed {
        svc.settings.timed = svc.settings.timed.clone().clamped();
        screengrabcraft_desktop::store::save_settings(&svc.settings);
    }
    match action {
        Some(TimerAction::Start) => crate::timed::start(svc),
        Some(TimerAction::Stop) => crate::timed::stop(svc),
        Some(TimerAction::Schedule) => {
            if let Err(e) = crate::timed::schedule(svc) {
                svc.notify("No capture was set", e, None, None, true);
            }
        }
        Some(TimerAction::Unschedule) => svc.timed.at = None,
        None => {}
    }
    ui.add_space(14.0);
    ui.separator();
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if widgets::button(ui, "Open the Image Editor").clicked() {
            let _ = launch::open_editor(&[]);
        }
        if widgets::button(ui, "Open the Video Editor").on_hover_text("Recordings open in it, in a window of its own").clicked() {
            let _ = launch::open_video_editor(&[]);
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
        if mode == CaptureMode::Production {
            // Its Recorder first, where the area and the inputs are chosen.
            svc.open_recorder();
        } else if delay > 0 {
            svc.start(mode, delay);
        } else {
            svc.preset = None;
            svc.after_countdown = Some((mode, Instant::now() + Duration::from_millis(300)));
        }
    }
}

/// The Capture page's buttons, the pictures apart from the recordings, each with its shortcut
/// (`shortcut` says it, empty when it has none), and under the pictures the one taken after a
/// wait of `delay` seconds. What was pressed, and the seconds to wait first.
fn capture_buttons(ui: &mut Ui, delay: &mut u32, shortcut: &dyn Fn(CaptureMode) -> String) -> Option<(CaptureMode, u32)> {
    let t = Tokens::get(ui.ctx());
    let pictures = [
        (CaptureMode::Region, Icon::Region, "Drag out an area, or click a window"),
        (CaptureMode::Adjust, Icon::Crop, "Drag out an area, then adjust it"),
        (CaptureMode::Window, Icon::Window, "Click a window"),
        (CaptureMode::Screen, Icon::Screen, "The screen under the pointer"),
        (CaptureMode::AllScreens, Icon::AllScreens, "Every monitor in one picture"),
        (CaptureMode::Scrolling, Icon::Scrolling, "A long page, as you scroll it"),
        (CaptureMode::Repeat, Icon::Repeat, "The same area as last time"),
        (CaptureMode::Text, Icon::Text, "Copy the text in an area"),
    ];
    let recordings = [
        (CaptureMode::Production, Icon::Edit, "With the camera and sound, to edit"),
        (CaptureMode::Video, Icon::Play, "Record an area as a small MP4"),
        (CaptureMode::Gif, Icon::Gif, "Record an area as an animated GIF"),
    ];
    let mut chosen = None;
    let groups: [(&str, &str, &[(CaptureMode, Icon, &str)]); 2] = [
        ("Take a picture", "Of an area, a window or the whole screen.", &pictures),
        ("Record a video", "What happens on the screen, as it happens.", &recordings),
    ];
    for (g, (name, about, modes)) in groups.iter().enumerate() {
        if g > 0 {
            ui.add_space(16.0);
        }
        ui.label(RichText::new(*name).strong());
        ui.label(RichText::new(*about).size(12.0).color(t.muted));
        ui.add_space(6.0);
        egui::Grid::new(("capture-buttons", g)).num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
            for pair in modes.chunks(2) {
                // The two of a row are as tall as each other: with room for a shortcut if either
                // has one.
                let keys: Vec<String> = pair.iter().map(|(mode, ..)| shortcut(*mode)).collect();
                let tall = keys.iter().any(|k| !k.is_empty());
                for ((mode, icon, help), key) in pair.iter().zip(&keys) {
                    if big_button(ui, *icon, mode.label(), help, key, tall).clicked() {
                        chosen = Some((*mode, 0));
                    }
                }
                ui.end_row();
            }
        });
        if g == 0 {
            // For a menu or a tip that goes when anything is pressed: time to open it first.
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label("Or after a wait of");
                ui.add(egui::DragValue::new(delay).range(1..=60).suffix(" s"));
                if widgets::button(ui, "Capture After a Wait")
                    .on_hover_text("Time to open a menu first: the area is chosen when the wait is over")
                    .clicked()
                {
                    chosen = Some((CaptureMode::Region, *delay));
                }
            });
        }
    }
    chosen
}

/// One of the Capture page's buttons: its picture, its name, what it does and its shortcut on a
/// key cap. `tall` leaves room for the shortcut even when it has none (its row's other one has).
fn big_button(ui: &mut Ui, icon: Icon, label: &str, help: &str, shortcut: &str, tall: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let size = vec2(262.0, if tall || !shortcut.is_empty() { 70.0 } else { 54.0 });
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    // Screen readers (and tests) know it by its name.
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let bg = if resp.hovered() { t.hover } else { t.window };
    ui.painter().rect_filled(rect, 8.0, bg);
    ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    icons::paint(ui.painter(), egui::Rect::from_min_size(rect.min + vec2(12.0, (size.y - 28.0) / 2.0), vec2(28.0, 28.0)), icon, t.accent);
    // Its words never run past its edge onto the next one.
    let words = ui.painter().with_clip_rect(rect.shrink2(vec2(8.0, 0.0)));
    words.text(rect.min + vec2(52.0, 9.0), egui::Align2::LEFT_TOP, label, egui::FontId::proportional(14.5), t.text);
    words.text(rect.min + vec2(52.0, 29.0), egui::Align2::LEFT_TOP, help, egui::FontId::proportional(11.5), t.muted);
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
    ui.checkbox(&mut s.after_capture.open_editor, "Open it in the Image Editor");
    ui.checkbox(&mut s.after_capture.save, "Save it in the captures folder");
    ui.add_space(12.0);
    ui.checkbox(&mut s.after_capture.show_bar, "Stop first, with a bar to adjust the area or choose Edit, Copy or Save");
    ui.label(RichText::new("Enter (or a double-click) then does the default. Off, the default happens at once.").size(12.0).color(t.muted));
    ui.add_space(6.0);
    ui.checkbox(&mut s.hide_editor, "Hide the Editor while capturing");
    ui.label(RichText::new("So it isn't in the picture. It comes back with the new capture.").size(12.0).color(t.muted));
    if cfg!(target_os = "windows") {
        ui.add_space(6.0);
        ui.checkbox(&mut s.keep_editor_ready, "Keep the Editor ready in the background");
        ui.label(
            RichText::new(
                "A capture opens in it at once, instead of waiting seconds for it to start. Closing its window hides it. It uses some memory.",
            )
            .size(12.0)
            .color(t.muted),
        );
    }
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
    ui.add_space(12.0);
    scrolling_options(ui, &mut s, screengrabcraft_capture::scroll::supported());
    if s != svc.settings {
        svc.settings = s;
        svc.apply_settings();
    }
}

/// How a scrolling capture goes: by itself (where the system lets it, `can`), and how long its
/// picture may get.
fn scrolling_options(ui: &mut Ui, s: &mut screengrabcraft_model::Settings, can: bool) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new("Scrolling Capture").strong());
    ui.add_enabled_ui(can, |ui| {
        let mut on = s.scroll_by_itself && can;
        if ui.checkbox(&mut on, "Scroll the page by itself").on_disabled_hover_text(screengrabcraft_capture::scroll::UNSUPPORTED).changed() {
            s.scroll_by_itself = on;
        }
    });
    let how = if can && s.scroll_by_itself {
        "It turns the mouse wheel over the area and stops at the end of the page. Where the page doesn't move, you scroll it and press Done."
    } else {
        "You scroll the page yourself and press Done."
    };
    ui.label(RichText::new(how).size(12.0).color(t.muted));
    ui.horizontal(|ui| {
        ui.label("Longest picture");
        let range = screengrabcraft_model::settings::SCROLLING_SHORTEST..=screengrabcraft_model::MAX_SIDE;
        ui.add(egui::DragValue::new(&mut s.scrolling_longest).range(range).speed(100.0).suffix(" px"));
    });
}

/// What the Capture page's timer section asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerAction {
    Start,
    Stop,
    /// A capture at the time written.
    Schedule,
    /// Not that one after all.
    Unschedule,
}

/// Captures on a timer and at a set time, on the Capture page: how often and of what, whether
/// a capture that would look as the last one did is skipped, Start or Stop with how it's going
/// (`status`), and one capture at a time of day (`waiting` is the time one waits for). Whether a
/// setting changed, and what was pressed.
fn timed_section(
    ui: &mut Ui,
    t: &mut screengrabcraft_model::TimedCaptures,
    running: bool,
    status: &str,
    waiting: Option<&str>,
) -> (bool, Option<TimerAction>) {
    use screengrabcraft_model::timed::{MAX_EVERY, MIN_EVERY, TimedArea};
    let tokens = Tokens::get(ui.ctx());
    let mut changed = false;
    let mut action = None;
    ui.label(RichText::new("Captures on a Timer").strong());
    ui.add_enabled_ui(!running, |ui| {
        ui.horizontal(|ui| {
            ui.label("One every");
            changed |= ui.add(egui::DragValue::new(&mut t.every_secs).range(MIN_EVERY..=MAX_EVERY).suffix(" s")).changed();
            ui.label("of");
            egui::ComboBox::from_id_salt("timed-area").selected_text(t.area.label()).show_ui(ui, |ui| {
                for a in TimedArea::ALL {
                    changed |= ui.selectable_value(&mut t.area, a, a.label()).changed();
                }
            });
        });
        changed |= ui.checkbox(&mut t.skip_unchanged, "Skip a capture when nothing has changed").changed();
    });
    ui.horizontal(|ui| {
        if running {
            if widgets::button(ui, "Stop").clicked() {
                action = Some(TimerAction::Stop);
            }
            ui.label(RichText::new(status).color(tokens.muted));
        } else if widgets::button(ui, "Start").on_hover_text("Each is saved to the captures folder, nothing coming up over the screen").clicked() {
            action = Some(TimerAction::Start);
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("One at");
        match waiting {
            Some(at) => {
                ui.label(RichText::new(format!("{at}, waiting")).color(tokens.muted));
                if widgets::button(ui, "Cancel").clicked() {
                    action = Some(TimerAction::Unschedule);
                }
            }
            None => {
                changed |= ui.add(egui::TextEdit::singleline(&mut t.at).hint_text("14:30").desired_width(56.0)).changed();
                let ok = screengrabcraft_model::timed::parse_clock(&t.at).is_some();
                if ui
                    .add_enabled(ok, egui::Button::new("Take One Then"))
                    .on_disabled_hover_text("Write the time as hours and minutes, as 14:30")
                    .clicked()
                {
                    action = Some(TimerAction::Schedule);
                }
            }
        }
    });
    (changed, action)
}

/// The capture presets as buttons, each with its shortcut (`key` says it, if it has one). The one
/// pressed.
fn preset_buttons(ui: &mut Ui, presets: &[CapturePreset], key: &dyn Fn(&CapturePreset) -> Option<String>) -> Option<usize> {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new("Your presets").strong());
    ui.label(RichText::new("Made on the Capture presets page.").size(12.0).color(t.muted));
    ui.add_space(4.0);
    let mut pressed = None;
    ui.horizontal_wrapped(|ui| {
        for (i, p) in presets.iter().enumerate() {
            let words = match key(p) {
                Some(k) => format!("{}  ({k})", p.title(i + 1)),
                None => p.title(i + 1),
            };
            if widgets::button(ui, &words).on_hover_text(p.area.describe()).clicked() {
                pressed = Some(i);
            }
        }
    });
    pressed
}

fn presets_tab(svc: &mut Service, ui: &mut Ui, close: &mut bool) {
    let t = Tokens::get(ui.ctx());
    ui.heading("Capture presets");
    ui.label(
        RichText::new(
            "A capture of your own: what it takes, after how long, what happens to it, and a shortcut of its own. They're on the Capture page and in the tray menu too.",
        )
        .color(t.muted),
    );
    ui.add_space(10.0);
    let mut presets = svc.settings.presets.clone();
    let run = presets_editor(ui, &mut presets);
    if presets != svc.settings.presets {
        svc.settings.presets = presets;
        svc.apply_settings();
    }
    ui.add_space(6.0);
    let named: Vec<String> = svc.settings.presets.iter().enumerate().map(|(i, p)| p.title(i + 1)).collect();
    for (a, b) in svc.settings.clashes() {
        if named.contains(&a) || named.contains(&b) {
            ui.label(RichText::new(format!("{a} and {b} have the same shortcut.")).color(t.danger));
        }
    }
    if let Some(i) = run {
        // The window goes first, so it isn't in the picture.
        *close = true;
        svc.run_preset_soon(i);
    }
}

/// Each preset on a card of its own (its name, what it takes, the wait, what happens after, its
/// shortcut, Capture and Remove), and a button that adds one. The preset whose Capture was pressed.
fn presets_editor(ui: &mut Ui, presets: &mut Vec<CapturePreset>) -> Option<usize> {
    let t = Tokens::get(ui.ctx());
    let mut run = None;
    let mut remove = None;
    for (i, p) in presets.iter_mut().enumerate() {
        egui::Frame::NONE
            .fill(t.window)
            .stroke(egui::Stroke::new(1.0, t.border))
            .corner_radius(widgets::CONTROL_RADIUS)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut p.name)
                            .id_salt(("preset-name", i))
                            .desired_width(260.0)
                            .hint_text(format!("Preset {}", i + 1)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::button(ui, "Remove").on_hover_text("Remove this preset").clicked() {
                            remove = Some(i);
                        }
                        if widgets::primary_button(ui, "Capture").on_hover_text("Take this capture now").clicked() {
                            run = Some(i);
                        }
                    });
                });
                ui.add_space(6.0);
                egui::Grid::new(("preset", i)).num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label("Area");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("preset-area", i)).width(190.0).selected_text(p.area.label()).show_ui(ui, |ui| {
                            for kind in PresetArea::KINDS {
                                if ui.selectable_label(p.area.same_kind(kind), kind.label()).clicked() && !p.area.same_kind(kind) {
                                    p.area = kind;
                                }
                            }
                        });
                        match &mut p.area {
                            PresetArea::Shape { w, h } => {
                                egui::ComboBox::from_id_salt(("preset-shape", i)).width(80.0).selected_text(format!("{w}:{h}")).show_ui(ui, |ui| {
                                    for (sw, sh) in SHAPES {
                                        if ui.selectable_label((*w, *h) == (sw, sh), format!("{sw}:{sh}")).clicked() {
                                            (*w, *h) = (sw, sh);
                                        }
                                    }
                                });
                            }
                            PresetArea::Fixed { w, h } => {
                                ui.add(egui::DragValue::new(w).range(FIXED_MIN..=FIXED_MAX).suffix(" px"));
                                ui.label("×");
                                ui.add(egui::DragValue::new(h).range(FIXED_MIN..=FIXED_MAX).suffix(" px"));
                            }
                            _ => {}
                        }
                    });
                    ui.end_row();
                    ui.label("Wait first");
                    ui.add(egui::DragValue::new(&mut p.delay_secs).range(0..=60).suffix(" s"));
                    ui.end_row();
                    ui.label("Then");
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut p.after.copy, "Copy it");
                        ui.checkbox(&mut p.after.open_editor, "Open it in the Image Editor");
                        ui.checkbox(&mut p.after.save, "Save it");
                        if p.area.mode() == CaptureMode::Region || p.area.mode() == CaptureMode::Window {
                            ui.checkbox(&mut p.after.show_bar, "Stop first, with the bar");
                        }
                    });
                    ui.end_row();
                    hotkey_row_in(ui, "Shortcut", ("preset-key", i), &mut p.hotkey, None);
                });
            });
        ui.add_space(8.0);
    }
    if let Some(i) = remove
        && i < presets.len()
    {
        presets.remove(i);
    }
    let full = presets.len() >= PRESETS_MAX;
    let add = ui.add_enabled_ui(!full, |ui| widgets::button(ui, "Add a preset")).inner;
    if add.on_disabled_hover_text(format!("{PRESETS_MAX} presets at most")).clicked() {
        presets.push(CapturePreset { name: format!("Preset {}", presets.len() + 1), ..CapturePreset::default() });
    }
    if presets.is_empty() {
        ui.add_space(4.0);
        ui.label(
            RichText::new("For example: a 1280 × 720 area copied straight to the clipboard, or a window saved after 3 seconds.")
                .size(12.0)
                .color(t.muted),
        );
    }
    run
}

fn hotkey_row(ui: &mut Ui, label: &str, key: &mut Option<Hotkey>, default: Option<Hotkey>) -> bool {
    hotkey_row_in(ui, label, label, key, default)
}

/// A shortcut's row of a grid: its label, on or off, the keys held with it and the key; `salt` keeps
/// its list of keys apart from another row's with the same label.
fn hotkey_row_in(ui: &mut Ui, label: &str, salt: impl std::hash::Hash + std::fmt::Debug, key: &mut Option<Hotkey>, default: Option<Hotkey>) -> bool {
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
            egui::ComboBox::from_id_salt(("key", salt)).width(110.0).selected_text(k.key.clone()).show_ui(ui, |ui| {
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
        if hotkey_row(ui, "Open the Image Editor", &mut e, defaults.editor.clone()) {
            keys.editor = e;
            changed = true;
        }
        let mut p = keys.pause.clone();
        if hotkey_row(ui, "Pause or Resume Recording", &mut p, defaults.pause.clone()) {
            keys.pause = p;
            changed = true;
        }
        let mut s = keys.stop.clone();
        if hotkey_row(ui, "Stop Recording", &mut s, defaults.stop.clone()) {
            keys.stop = s;
            changed = true;
        }
        let mut d = keys.draw.clone();
        if hotkey_row(ui, "Draw on the Screen While Recording", &mut d, defaults.draw.clone()) {
            keys.draw = d;
            changed = true;
        }
    });
    ui.label(RichText::new("Pause, Stop and Draw are only taken from other programs while a recording runs.").size(12.0).color(t.muted));
    if changed {
        svc.settings.hotkeys = keys;
        svc.apply_settings();
    }
    ui.add_space(10.0);
    for (a, b) in svc.settings.clashes() {
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

/// The microphones and cameras a recording can use (looked up when the Recording page is first
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
        ui.label(RichText::new("No microphone was found. Over Remote Desktop, the Remote Desktop app has to share yours.").size(12.0).color(t.muted));
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
    ui.heading("Recording");
    ui.add_space(8.0);
    ui.label(RichText::new("Record Video").strong());
    ui.label(RichText::new("A small MP4 of an area, made as soon as you stop.").size(12.0).color(t.muted));
    ui.add_space(4.0);
    sources(ui, "video", &mut s.video, d, false);
    ui.checkbox(&mut s.video_full_size, "Keep the full size (else no larger than 1920 × 1080, for smaller files)");
    ui.add_space(14.0);
    ui.label(RichText::new("Video Production").strong());
    ui.label(RichText::new("Each on a track of its own, to cut and edit in the Video Editor before you make the MP4.").size(12.0).color(t.muted));
    ui.add_space(4.0);
    sources(ui, "production", &mut s.production, d, true);
    ui.add_space(4.0);
    ui.checkbox(&mut s.production.captions, "Transcribe recordings automatically");
    ui.label(
        RichText::new(
            "After each recording, what was said is written down in the background, on this computer: the Video Editor shows its progress, then the captions. You can also transcribe again in the Video Editor.",
        )
        .size(12.0)
        .color(t.muted),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new("Video Production opens the Recorder, where you choose the area (a region, a window, a whole screen, all screens or a fixed size), turn the screen and each of these on or off, and press Record. These are the same choices.")
            .size(12.0)
            .color(t.muted),
    );
    let mut open_recorder = false;
    if widgets::button(ui, "Open the Recorder").clicked() {
        open_recorder = true;
    }
    ui.add_space(14.0);
    ui.label(RichText::new("Both kinds of video").strong());
    egui::Grid::new("video-both").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Frames a second");
        ui.add(egui::Slider::new(&mut s.video_frame_rate, 5..=30));
        ui.end_row();
        ui.label("Longest recording");
        ui.add(egui::DragValue::new(&mut s.video_max_mins).range(1..=240).suffix(" minutes"));
        ui.end_row();
    });
    ui.add_space(14.0);
    ui.label(RichText::new("Record GIF").strong());
    ui.label(RichText::new("A few seconds of something moving, as a picture that plays anywhere. It has no sound.").size(12.0).color(t.muted));
    ui.add_space(4.0);
    egui::Grid::new("gif").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Frames a second");
        ui.add(egui::Slider::new(&mut s.gif_frame_rate, 1..=30));
        ui.end_row();
        ui.label("Longest recording");
        ui.add(egui::Slider::new(&mut s.gif_max_secs, 5..=600).suffix(" s"));
        ui.end_row();
    });
    ui.add_space(10.0);
    if widgets::button(ui, "Look for devices again").clicked() {
        home.devices = None;
    }
    if s != svc.settings {
        svc.settings = s;
        svc.apply_settings();
    }
    if open_recorder {
        svc.open_recorder();
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;
    use screengrabcraft_model::{AfterCapture, Theme};

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

    /// The Capture page's buttons: the pictures first, the recordings under a heading of their
    /// own, the capture after a wait with the pictures.
    #[test]
    fn the_capture_page_keeps_pictures_and_recordings_apart() {
        for (theme, name) in [(Theme::Dark, "home-capture"), (Theme::Light, "home-capture-light")] {
            let mut h = Harness::builder().with_size(vec2(590.0, 720.0)).build_ui_state(
                move |ui, (delay, chosen): &mut (u32, Option<(CaptureMode, u32)>)| {
                    screengrabcraft_ui::theme::apply(ui.ctx(), theme);
                    let shortcut = |m: CaptureMode| if m == CaptureMode::Region { "Print Screen".to_string() } else { String::new() };
                    egui::CentralPanel::default().show(ui, |ui| {
                        if let Some(c) = capture_buttons(ui, delay, &shortcut) {
                            *chosen = Some(c);
                        }
                    });
                },
                (5, None),
            );
            h.run_steps(3);
            let (pictures, recordings) = (h.get_by_label("Take a picture").rect(), h.get_by_label("Record a video").rect());
            let (region, repeat) = (h.get_by_label(CaptureMode::Region.label()).rect(), h.get_by_label(CaptureMode::Repeat.label()).rect());
            let (video, gif) = (h.get_by_label(CaptureMode::Production.label()).rect(), h.get_by_label(CaptureMode::Gif.label()).rect());
            assert!(
                pictures.max.y <= region.min.y && repeat.max.y < recordings.min.y,
                "pictures under their heading: {pictures:?} {region:?} {repeat:?}"
            );
            assert!(recordings.max.y <= video.min.y && video.min.y <= gif.min.y, "recordings under theirs: {recordings:?} {video:?} {gif:?}");
            // The two of a row are as tall as each other, though only one has a shortcut.
            let adjust = h.get_by_label(CaptureMode::Adjust.label()).rect();
            assert!((region.height() - adjust.height()).abs() < 0.5 && (region.min.y - adjust.min.y).abs() < 0.5, "{region:?} {adjust:?}");
            // The capture after a wait belongs to the pictures.
            let wait = h.get_by_label("Capture After a Wait").rect();
            assert!(wait.min.y > repeat.max.y && wait.max.y < recordings.min.y, "{wait:?}");
            if let Ok(dir) = std::env::var("SGC_SHOTS")
                && let Ok(img) = h.render()
            {
                let _ = img.save(format!("{dir}/{name}.png"));
            }
            h.get_by_label(CaptureMode::Gif.label()).click();
            h.run_steps(2);
            assert_eq!(h.state().1, Some((CaptureMode::Gif, 0)));
            h.get_by_label("Capture After a Wait").click();
            h.run_steps(2);
            assert_eq!(h.state().1, Some((CaptureMode::Region, 5)));
        }
    }

    /// The presets' page: one is added, made a picture of a set size copied straight away, given
    /// a wait, and captured; another is removed.
    #[test]
    fn a_preset_is_added_set_up_captured_and_removed() {
        let mut h = Harness::builder().with_size(vec2(620.0, 760.0)).build_ui_state(
            |ui, (presets, run): &mut (Vec<CapturePreset>, Option<usize>)| {
                screengrabcraft_ui::theme::apply(ui.ctx(), Theme::Dark);
                egui::CentralPanel::default().show(ui, |ui| {
                    if let Some(i) = presets_editor(ui, presets) {
                        *run = Some(i);
                    }
                });
            },
            (Vec::new(), None),
        );
        h.run_steps(2);
        h.get_by_label_contains("For example");
        h.get_by_label("Add a preset").click();
        h.run_steps(2);
        assert_eq!(h.state().0.len(), 1);
        assert_eq!(h.state().0[0].name, "Preset 1");
        // A picture of a set size, copied and nothing else.
        h.get_by_value("Region").click();
        h.run_steps(2);
        h.get_by_label("Region of a set size").click();
        h.run_steps(2);
        assert_eq!(h.state().0[0].area, PresetArea::Fixed { w: 1280, h: 720 });
        for what in ["Open it in the Image Editor", "Save it"] {
            h.get_by_label(what).click();
            h.run_steps(1);
        }
        assert_eq!(h.state().0[0].after, AfterCapture { show_bar: false, open_editor: false, copy: true, save: false });
        h.get_by_label("Add a preset").click();
        h.run_steps(2);
        assert_eq!(h.state().0.len(), 2);
        h.run_steps(3);
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/home-presets.png"));
        }
        // Capture the first; remove the second.
        let captures = h.get_all_by_label("Capture").map(|n| n.rect()).collect::<Vec<_>>();
        assert_eq!(captures.len(), 2);
        h.get_all_by_label("Capture").next().unwrap().click();
        h.run_steps(2);
        assert_eq!(h.state().1, Some(0));
        h.get_all_by_label("Remove").nth(1).unwrap().click();
        h.run_steps(2);
        assert_eq!(h.state().0.len(), 1);
        assert_eq!(h.state().0[0].area, PresetArea::Fixed { w: 1280, h: 720 }, "the first is left");
    }

    #[test]
    fn the_capture_page_shows_the_presets_with_their_shortcuts() {
        let presets = vec![
            CapturePreset { name: "Docs shot".into(), area: PresetArea::Fixed { w: 800, h: 600 }, ..CapturePreset::default() },
            CapturePreset { name: " ".into(), area: PresetArea::Window, ..CapturePreset::default() },
        ];
        let mut h = Harness::builder().with_size(vec2(560.0, 200.0)).build_ui_state(
            move |ui, pressed: &mut Option<usize>| {
                screengrabcraft_ui::theme::apply(ui.ctx(), Theme::Light);
                egui::CentralPanel::default().show(ui, |ui| {
                    let key = |p: &CapturePreset| (p.name == "Docs shot").then(|| "Ctrl+Alt+D".to_string());
                    if let Some(i) = preset_buttons(ui, &presets, &key) {
                        *pressed = Some(i);
                    }
                });
            },
            None,
        );
        h.run_steps(2);
        h.get_by_label("Docs shot  (Ctrl+Alt+D)");
        h.get_by_label("Preset 2").click();
        h.run_steps(1);
        assert_eq!(*h.state(), Some(1));
    }

    /// The timer section with its settings, whether it runs, and what it last asked for.
    type TimerState = (screengrabcraft_model::TimedCaptures, bool, Option<TimerAction>, Option<String>);

    #[test]
    fn the_timer_starts_and_stops_and_one_capture_waits_for_a_set_time() {
        let mut h = Harness::builder().with_size(vec2(560.0, 220.0)).build_ui_state(
            |ui, s: &mut TimerState| {
                screengrabcraft_ui::theme::apply(ui.ctx(), Theme::Light);
                egui::CentralPanel::default().show(ui, |ui| {
                    let (_, action) = timed_section(ui, &mut s.0, s.1, "3 captures kept", s.3.as_deref());
                    if action.is_some() {
                        s.2 = action;
                    }
                });
            },
            (screengrabcraft_model::TimedCaptures::default(), false, None, None),
        );
        h.run_steps(2);
        assert!(h.state().0.skip_unchanged, "unchanged ones are skipped by default");
        assert!(egui_kittest::kittest::NodeT::accesskit_node(&h.get_by_label("Take One Then")).is_disabled(), "no time written yet");
        h.get_by_label("Start").click();
        h.run_steps(2);
        assert_eq!(h.state().2, Some(TimerAction::Start));
        h.state_mut().1 = true;
        h.run_steps(2);
        h.get_by_label("3 captures kept");
        h.get_by_label("Stop").click();
        h.run_steps(2);
        assert_eq!(h.state().2, Some(TimerAction::Stop));
        h.state_mut().0.at = "7:30".into();
        h.run_steps(2);
        h.get_by_label("Take One Then").click();
        h.run_steps(2);
        assert_eq!(h.state().2, Some(TimerAction::Schedule));
        h.state_mut().3 = Some("7:30".into());
        h.run_steps(2);
        h.get_by_label("7:30, waiting");
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/home-timer.png"));
        }
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        assert_eq!(h.state().2, Some(TimerAction::Unschedule));
    }

    /// Scrolling by itself is turned off and on where the system can do it, and greyed out with
    /// the reason where it can't.
    #[test]
    fn scrolling_by_itself_is_a_setting_greyed_out_where_it_cant_be() {
        for can in [true, false] {
            let mut h = Harness::builder().with_size(vec2(560.0, 200.0)).build_ui_state(
                move |ui, s: &mut screengrabcraft_model::Settings| {
                    screengrabcraft_ui::theme::apply(ui.ctx(), Theme::Light);
                    egui::CentralPanel::default().show(ui, |ui| scrolling_options(ui, s, can));
                },
                screengrabcraft_model::Settings::default(),
            );
            h.run_steps(2);
            assert!(h.state().scroll_by_itself, "on by default");
            let shown = h.get_by_label("Scroll the page by itself");
            assert_eq!(egui_kittest::kittest::NodeT::accesskit_node(&shown).is_disabled(), !can);
            if can {
                h.get_by_label_contains("stops at the end of the page");
                h.get_by_label("Scroll the page by itself").click();
                h.run_steps(2);
                assert!(!h.state().scroll_by_itself);
                h.get_by_label("You scroll the page yourself and press Done.");
            } else {
                h.get_by_label("You scroll the page yourself and press Done.");
            }
            if let Ok(dir) = std::env::var("SGC_SHOTS")
                && let Ok(img) = h.render()
            {
                let _ = img.save(format!("{dir}/home-scrolling-{can}.png"));
            }
        }
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
