//! The Recorder: Video Production's window. The owner chooses what the recording shows of the
//! screen (an area dragged out, a window, a whole monitor, every monitor, or a frame of a set
//! size) and what it takes in (the screen, the camera, the microphone, the computer's sound), then
//! presses Record. Every choice is one of the owner's settings ([`RecorderOptions`], and
//! `Settings::production` for the devices), so the window opens as it was left.
//!
//! [`recorder_ui`] draws the window's contents and says which button was pressed; the capture
//! service opens it in a window of its own, lists the devices and does what it answers. Nothing
//! here turns a device on: the microphone's meter is the service's, and only when asked.

use egui::{Align, Align2, Color32, FontId, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use screengrabcraft_model::settings::{FIXED_MAX, FIXED_MIN, FIXED_SIZES, VideoOptions};
use screengrabcraft_model::{MonitorChoice, RecordArea, RecorderOptions};

use crate::icons::{self, Icon};
use crate::theme::Tokens;
use crate::widgets::{self, CONTROL_RADIUS, StandardButton};

/// The window's size.
pub const SIZE: Vec2 = vec2(640.0, 540.0);
/// The size of the Record button: larger than the others, as the window's one big action.
const RECORD_SIZE: Vec2 = vec2(136.0, 40.0);
const TILE: Vec2 = vec2(106.0, 76.0);

/// A monitor the Recorder offers.
#[derive(Clone, Debug, PartialEq)]
pub struct RecorderMonitor {
    pub name: String,
    /// Where it is on the desktop.
    pub x: i32,
    pub y: i32,
    /// Its size, in pixels.
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

/// What this computer has to record with, as the service found it. Listing them turns nothing on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecorderDevices {
    pub monitors: Vec<RecorderMonitor>,
    /// The size an all-screens recording is made at.
    pub all_screens: Option<(u32, u32)>,
    pub microphones: Vec<String>,
    /// The cameras by number and name, once looked for (only while the camera is on: on a Mac,
    /// looking asks the owner).
    pub cameras: Option<Vec<(u32, String)>>,
    /// Whether this system can record a camera at all.
    pub camera_supported: bool,
    /// Whether the computer's sound can be recorded here, now.
    pub system_sound: bool,
    /// Whether this kind of system can record the computer's sound at all (Windows).
    pub system_sound_platform: bool,
}

/// How loud the microphone is, for the meter beside it.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum MicLevel {
    /// Not listened to.
    #[default]
    Off,
    Starting,
    /// 0 to 1.
    Level(f32),
    /// It couldn't be heard, and why.
    Failed(String),
}

/// The window's own state, what isn't a setting.
#[derive(Clone, Debug, Default)]
pub struct RecorderView {
    /// A fixed size typed in rather than one of [`FIXED_SIZES`]; none until first shown.
    custom: Option<bool>,
    /// The owner has asked to hear the microphone, by turning it on here or with Test: the
    /// service listens to it for the meter while this is on and the microphone is.
    pub listen: bool,
}

/// What the Recorder's buttons do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecorderChoice {
    Record,
    Cancel,
}

/// The red of the Record button: as it is, under the pointer, pressed.
fn reds(dark: bool) -> [Color32; 3] {
    if dark {
        [Color32::from_rgb(0xE5, 0x48, 0x4D), Color32::from_rgb(0xEE, 0x63, 0x67), Color32::from_rgb(0xC4, 0x3A, 0x3F)]
    } else {
        [Color32::from_rgb(0xD3, 0x2F, 0x2F), Color32::from_rgb(0xE5, 0x39, 0x35), Color32::from_rgb(0xB7, 0x1C, 0x1C)]
    }
}

/// The big red Record button: a white dot and the word.
fn record_button(ui: &mut Ui, enabled: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(RECORD_SIZE, if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, "Record"));
    let [red, hot, down] = reds(t.dark);
    let fill = if !enabled {
        t.control
    } else if resp.is_pointer_button_down_on() {
        down
    } else if resp.hovered() {
        hot
    } else {
        red
    };
    let p = ui.painter();
    p.rect(rect, CONTROL_RADIUS, fill, Stroke::new(1.0, if enabled { fill } else { t.border }), StrokeKind::Inside);
    let ink = if enabled { Color32::WHITE } else { t.muted };
    let galley = p.layout_no_wrap("Record".to_string(), widgets::strong_font(ui.ctx(), 15.0), ink);
    let dot = 12.0;
    let x = rect.center().x - (dot + 8.0 + galley.size().x) / 2.0;
    p.circle_filled(pos2(x + dot / 2.0, rect.center().y), dot / 2.0, ink);
    p.galley(pos2(x + dot + 8.0, rect.center().y - galley.size().y / 2.0), galley, ink);
    if resp.has_focus() {
        p.rect_stroke(rect.expand(2.0), CONTROL_RADIUS + 2, Stroke::new(1.5, t.text), StrokeKind::Inside);
    }
    if enabled { resp.on_hover_text("Start the recording") } else { resp.on_hover_text("Turn on something to record first") }
}

/// An on/off switch with its words after it; the whole of it can be clicked.
fn switch(ui: &mut Ui, on: &mut bool, label: &str, enabled: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(14.0), if enabled { t.text } else { t.muted });
    let (rect, mut resp) = ui.allocate_exact_size(vec2(40.0 + 10.0 + galley.size().x, 26.0), if enabled { Sense::click() } else { Sense::hover() });
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let shown = *on;
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, shown, label));
    let track = Rect::from_min_size(pos2(rect.min.x, rect.center().y - 10.0), vec2(40.0, 20.0));
    let (fill, edge, knob) = match (enabled, shown) {
        (true, true) => (if resp.hovered() { t.accent_hover } else { t.accent }, t.accent, t.accent_text),
        (true, false) => (if resp.hovered() { t.control_hover } else { t.control }, t.control_border, t.text),
        (false, true) => (t.control, t.border, t.muted),
        (false, false) => (t.control, t.border, t.border),
    };
    let p = ui.painter();
    p.rect(track, 10.0, fill, Stroke::new(1.0, edge), StrokeKind::Inside);
    let kx = if shown { track.max.x - 10.0 } else { track.min.x + 10.0 };
    p.circle_filled(pos2(kx, track.center().y), 6.0, knob);
    p.galley(pos2(track.max.x + 10.0, rect.center().y - galley.size().y / 2.0), galley, t.text);
    if resp.has_focus() {
        p.rect_stroke(track.expand(2.0), 12.0, Stroke::new(1.5, t.text), StrokeKind::Inside);
    }
    resp
}

/// The fixed-size area's picture: a frame with its width marked under it.
fn paint_fixed(p: &egui::Painter, rect: Rect, color: Color32) {
    let s = rect.width().min(rect.height()) / 20.0;
    let c = rect.center();
    let at = |x: f32, y: f32| pos2(c.x + (x - 10.0) * s, c.y + (y - 10.0) * s);
    let stroke = Stroke::new((1.6 * s).max(1.2), color);
    p.rect_stroke(Rect::from_min_max(at(2.5, 4.0), at(17.5, 12.5)), 1.0 * s, stroke, StrokeKind::Middle);
    p.line_segment([at(2.5, 16.0), at(17.5, 16.0)], stroke);
    for x in [2.5, 17.5] {
        p.line_segment([at(x, 14.5), at(x, 17.5)], stroke);
    }
}

/// A choice of area: its picture and its name, outlined in the accent colour when chosen.
fn area_tile(ui: &mut Ui, area: RecordArea, selected: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let enabled = ui.is_enabled();
    let (rect, resp) = ui.allocate_exact_size(TILE, Sense::click());
    let label = area.label();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, selected, label));
    let fill = if selected {
        t.selected
    } else if resp.hovered() && enabled {
        t.hover
    } else {
        t.window
    };
    let edge = if selected { Stroke::new(2.0, t.accent) } else { Stroke::new(1.0, t.border) };
    let p = ui.painter();
    p.rect(rect, 8.0, fill, edge, StrokeKind::Inside);
    let ink = if !enabled {
        t.muted
    } else if selected {
        t.accent
    } else {
        t.text
    };
    let pic = Rect::from_center_size(pos2(rect.center().x, rect.min.y + 27.0), vec2(28.0, 28.0));
    match area {
        RecordArea::Region => icons::paint(p, pic, Icon::Region, ink),
        RecordArea::Window => icons::paint(p, pic, Icon::Window, ink),
        RecordArea::Screen => icons::paint(p, pic, Icon::Screen, ink),
        RecordArea::AllScreens => icons::paint(p, pic, Icon::AllScreens, ink),
        RecordArea::Fixed => paint_fixed(p, pic, ink),
    }
    p.text(
        pos2(rect.center().x, rect.max.y - 16.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(13.0),
        if enabled { t.text } else { t.muted },
    );
    if resp.has_focus() {
        p.rect_stroke(rect.expand(2.0), 10.0, Stroke::new(1.5, t.text), StrokeKind::Inside);
    }
    resp
}

fn heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(widgets::strong_font(ui.ctx(), 15.0)));
    ui.add_space(6.0);
}

fn note(ui: &mut Ui, text: impl Into<String>) {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text.into()).size(12.5).color(t.muted)).wrap());
}

fn monitor_label(m: &RecorderMonitor, i: usize) -> String {
    let name = if m.name.trim().is_empty() { format!("Monitor {}", i + 1) } else { m.name.clone() };
    format!("{name} — {} × {}{}", m.width, m.height, if m.primary { " (main)" } else { "" })
}

/// The monitor a full-screen recording is of: the one remembered, else the main one.
pub fn chosen_monitor(rec: &RecorderOptions, monitors: &[RecorderMonitor]) -> Option<usize> {
    let main = monitors.iter().position(|m| m.primary).or((!monitors.is_empty()).then_some(0));
    let list: Vec<(&str, i32, i32)> = monitors.iter().map(|m| (m.name.as_str(), m.x, m.y)).collect();
    rec.monitor.as_ref().and_then(|c| c.pick(&list)).or(main)
}

/// What happens once Record is pressed, said in the footer.
fn next_step(rec: &RecorderOptions) -> &'static str {
    if !rec.screen {
        return "Recording starts at once. Stop it from its control or the tray.";
    }
    match rec.area {
        RecordArea::Region => "Next, drag out the area to record.",
        RecordArea::Window => "Next, click the window to record.",
        RecordArea::Fixed => "Next, click where the frame goes.",
        RecordArea::Screen | RecordArea::AllScreens => {
            "Recording starts at once. Its control goes on a screen that isn't recorded; if there's none, stop it from the tray icon."
        }
    }
}

fn area_section(ui: &mut Ui, view: &mut RecorderView, rec: &mut RecorderOptions, devices: &RecorderDevices) {
    heading(ui, "Area");
    ui.add_enabled_ui(rec.screen, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for area in RecordArea::ALL {
                if area_tile(ui, area, rec.area == area).clicked() {
                    rec.area = area;
                }
            }
        });
        ui.add_space(10.0);
        if !rec.screen {
            return;
        }
        match rec.area {
            RecordArea::Region => note(ui, "Drag out the area after you press Record. Its size is shown as you drag."),
            RecordArea::Window => {
                note(ui, "Click the window after you press Record. The video keeps to where the window was then; it doesn't follow it if it moves.")
            }
            RecordArea::Screen => monitor_choice(ui, rec, devices),
            RecordArea::AllScreens => note(
                ui,
                match (devices.monitors.len(), devices.all_screens) {
                    (0, _) => "No monitor was found.".to_string(),
                    (1, _) => "There's one monitor: it's recorded whole.".to_string(),
                    (n, Some((w, h))) => format!("All {n} monitors in one video, {w} × {h}, as they are placed side by side."),
                    (n, None) => format!("All {n} monitors in one video."),
                },
            ),
            RecordArea::Fixed => fixed_choice(ui, view, rec, devices),
        }
    });
    if !rec.screen {
        note(ui, "The screen is off, so there's no area to choose.");
    }
}

fn monitor_choice(ui: &mut Ui, rec: &mut RecorderOptions, devices: &RecorderDevices) {
    if devices.monitors.is_empty() {
        note(ui, "No monitor was found.");
        return;
    }
    let chosen = chosen_monitor(rec, &devices.monitors);
    let shown = chosen.and_then(|i| devices.monitors.get(i).map(|m| monitor_label(m, i))).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("Monitor");
        egui::ComboBox::from_id_salt("recorder-monitor").width(340.0).selected_text(shown).show_ui(ui, |ui| {
            for (i, m) in devices.monitors.iter().enumerate() {
                if ui.selectable_label(chosen == Some(i), monitor_label(m, i)).clicked() {
                    rec.monitor = Some(MonitorChoice { name: m.name.clone(), x: m.x, y: m.y });
                }
            }
        });
    });
}

/// Whether a frame of this size fits on one of the monitors (on any, when none was found).
fn fits(w: u32, h: u32, monitors: &[RecorderMonitor]) -> bool {
    monitors.is_empty() || monitors.iter().any(|m| w <= m.width && h <= m.height)
}

/// The fixed sizes the Recorder lists: those of [`FIXED_SIZES`] that fit on one of the monitors,
/// under Landscape, Portrait and Square.
pub fn listed_sizes(monitors: &[RecorderMonitor]) -> [(&'static str, Vec<(u32, u32, &'static str)>); 3] {
    let shape = |keep: fn(u32, u32) -> bool| -> Vec<(u32, u32, &'static str)> {
        FIXED_SIZES.iter().copied().filter(|&(w, h, _)| keep(w, h) && fits(w, h, monitors)).collect()
    };
    [("Landscape", shape(|w, h| w > h)), ("Portrait", shape(|w, h| w < h)), ("Square", shape(|w, h| w == h))]
}

/// The largest size of one's own: as wide as the widest monitor and as tall as the tallest.
fn largest(monitors: &[RecorderMonitor]) -> (u32, u32) {
    let side = |v: Option<u32>| v.unwrap_or(FIXED_MAX).clamp(FIXED_MIN, FIXED_MAX);
    (side(monitors.iter().map(|m| m.width).max()), side(monitors.iter().map(|m| m.height).max()))
}

fn fixed_choice(ui: &mut Ui, view: &mut RecorderView, rec: &mut RecorderOptions, devices: &RecorderDevices) {
    let t = Tokens::get(ui.ctx());
    let preset = FIXED_SIZES.iter().find(|(w, h, _)| (*w, *h) == (rec.width, rec.height));
    let custom = view.custom.get_or_insert(preset.is_none());
    let shown = match (preset, *custom) {
        (Some((w, h, what)), false) => format!("{w} × {h} — {what}"),
        _ => "A size of your own".to_string(),
    };
    let (max_w, max_h) = largest(&devices.monitors);
    ui.horizontal(|ui| {
        ui.label("Size");
        // Short enough to stay in the window: the list scrolls.
        egui::ComboBox::from_id_salt("recorder-size").width(250.0).height(300.0).selected_text(shown).show_ui(ui, |ui| {
            for (group, sizes) in listed_sizes(&devices.monitors) {
                if sizes.is_empty() {
                    continue;
                }
                ui.label(RichText::new(group).size(12.5).color(t.muted));
                for (w, h, what) in sizes {
                    if ui.selectable_label(!*custom && (w, h) == (rec.width, rec.height), format!("{w} × {h} — {what}")).clicked() {
                        (rec.width, rec.height) = (w, h);
                        *custom = false;
                    }
                }
                ui.separator();
            }
            if ui.selectable_label(*custom, "A size of your own").clicked() {
                *custom = true;
            }
        });
        if *custom {
            ui.add(egui::DragValue::new(&mut rec.width).range(FIXED_MIN..=max_w).speed(2.0));
            ui.label("×");
            ui.add(egui::DragValue::new(&mut rec.height).range(FIXED_MIN..=max_h).speed(2.0));
            ui.label("pixels");
        }
    });
    ui.add_space(4.0);
    if fits(rec.width, rec.height, &devices.monitors) {
        note(ui, "Click where the frame goes after you press Record. Only the sizes that fit on your monitors are listed.");
    } else {
        note(ui, "This is bigger than your monitors, so it's made smaller, keeping its shape. Click where the frame goes after you press Record.");
    }
}

/// The microphone's meter, or a Test button that asks for it.
fn mic_meter(ui: &mut Ui, view: &mut RecorderView, level: &MicLevel) {
    let t = Tokens::get(ui.ctx());
    if !view.listen {
        if ui.add(StandardButton::new("Test")).on_hover_text("Listen to the microphone, to see that it hears you").clicked() {
            view.listen = true;
        }
        return;
    }
    match level {
        MicLevel::Failed(why) => {
            ui.label(RichText::new("Can't hear it").size(12.5).color(t.danger)).on_hover_text(why.as_str());
        }
        MicLevel::Off | MicLevel::Starting | MicLevel::Level(_) => {
            let v = if let MicLevel::Level(l) = level { l.clamp(0.0, 1.0) } else { 0.0 };
            let (rect, resp) = ui.allocate_exact_size(vec2(96.0, 10.0), Sense::hover());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ProgressIndicator, true, "Microphone level"));
            let p = ui.painter();
            p.rect(rect, 5.0, t.control, Stroke::new(1.0, t.border), StrokeKind::Inside);
            // Loudness as it sounds: quiet sounds still move it.
            let lit = rect.width() * v.sqrt();
            if lit > 1.0 {
                let green = Color32::from_rgb(0x2F, 0xA8, 0x4F);
                let colour = if v > 0.9 { reds(t.dark)[0] } else { green };
                p.rect_filled(Rect::from_min_size(rect.min, vec2(lit, rect.height())), 5.0, colour);
            }
        }
    }
}

fn inputs_section(ui: &mut Ui, view: &mut RecorderView, rec: &mut RecorderOptions, o: &mut VideoOptions, d: &RecorderDevices, mic: &MicLevel) {
    heading(ui, "Inputs");
    egui::Grid::new("recorder-inputs").num_columns(2).spacing([18.0, 8.0]).min_col_width(170.0).min_row_height(widgets::BUTTON_SIZE.y).show(
        ui,
        |ui| {
            switch(ui, &mut rec.screen, "Screen", true);
            note(ui, if rec.screen { "What the area shows." } else { "Not recorded." });
            ui.end_row();

            let mut camera = o.camera && d.camera_supported;
            if switch(ui, &mut camera, "Camera", d.camera_supported).changed() {
                o.camera = camera;
            }
            ui.horizontal(|ui| {
                if !d.camera_supported {
                    note(ui, "Cameras can't be recorded on this system yet.");
                    return;
                }
                let list = d.cameras.as_deref().unwrap_or_default();
                ui.add_enabled_ui(o.camera, |ui| {
                    let shown = o
                        .camera_index
                        .and_then(|i| list.iter().find(|c| c.0 == i).map(|c| c.1.clone()))
                        .or_else(|| list.first().map(|c| c.1.clone()))
                        .unwrap_or_else(|| if d.cameras.is_some() { "No camera was found".to_string() } else { "The first camera".to_string() });
                    egui::ComboBox::from_id_salt("recorder-camera").width(260.0).selected_text(shown).show_ui(ui, |ui| {
                        for (i, name) in list {
                            ui.selectable_value(&mut o.camera_index, Some(*i), name);
                        }
                    });
                });
            });
            ui.end_row();

            let was = o.microphone;
            switch(ui, &mut o.microphone, "Microphone", true);
            // Turning it on here is asking to hear it; turning it off stops the listening.
            if o.microphone != was {
                view.listen = o.microphone;
            }
            ui.horizontal(|ui| {
                ui.add_enabled_ui(o.microphone, |ui| {
                    let shown = o.microphone_name.clone().unwrap_or_else(|| "The system's default".to_string());
                    egui::ComboBox::from_id_salt("recorder-mic").width(260.0).selected_text(shown).show_ui(ui, |ui| {
                        ui.selectable_value(&mut o.microphone_name, None, "The system's default");
                        for m in &d.microphones {
                            ui.selectable_value(&mut o.microphone_name, Some(m.clone()), m);
                        }
                    });
                });
                if o.microphone {
                    mic_meter(ui, view, mic);
                }
            });
            ui.end_row();

            let mut sound = o.system_sound && d.system_sound;
            if switch(ui, &mut sound, "Computer sound", d.system_sound).changed() {
                o.system_sound = sound;
            }
            if !d.system_sound_platform {
                note(ui, "Only on Windows: this system has no way to record what it plays.");
            } else if !d.system_sound {
                note(ui, "No sound output was found.");
            } else {
                note(ui, "What the computer plays.");
            }
            ui.end_row();
        },
    );
    if o.microphone && d.microphones.is_empty() {
        note(ui, "No microphone was found just now.");
    }
}

/// The Recorder's contents: the area, the inputs, and Record and Cancel along the bottom. Changes
/// the choices in place; returns the button pressed (Enter is Record, Esc is Cancel).
pub fn recorder_ui(
    ui: &mut Ui,
    view: &mut RecorderView,
    rec: &mut RecorderOptions,
    sources: &mut VideoOptions,
    devices: &RecorderDevices,
    mic: &MicLevel,
) -> Option<RecorderChoice> {
    let t = Tokens::get(ui.ctx());
    // Read before anything is drawn: Esc closing a list isn't for the window, and Enter on a
    // focused control (a number being typed, a switch) is that control's.
    let popup = egui::Popup::is_any_open(ui.ctx());
    let typing = ui.memory(|m| m.focused().is_some());
    let can = rec.records_anything(sources, devices.system_sound, devices.camera_supported);
    let mut choice = None;
    let footer = egui::Panel::bottom("recorder-footer")
        .resizable(false)
        .frame(Frame::NONE.fill(t.footer).inner_margin(Margin::symmetric(widgets::PADDING, 14)))
        .show(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = widgets::GAP;
                // The default first and Cancel last, or the other way round on a Mac.
                let mac = cfg!(target_os = "macos");
                let cancel = |ui: &mut Ui| ui.add(StandardButton::new("Cancel")).clicked();
                if mac {
                    if record_button(ui, can).clicked() {
                        choice = Some(RecorderChoice::Record);
                    }
                    if cancel(ui) {
                        choice = Some(RecorderChoice::Cancel);
                    }
                } else {
                    if cancel(ui) {
                        choice = Some(RecorderChoice::Cancel);
                    }
                    if record_button(ui, can).clicked() {
                        choice = Some(RecorderChoice::Record);
                    }
                }
                ui.add_space(12.0);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(next_step(rec)).size(12.5).color(t.muted)).truncate());
                });
            });
        });
    let line = footer.response.rect;
    ui.painter().hline(line.x_range(), line.top(), Stroke::new(1.0, t.border));
    egui::CentralPanel::default().frame(Frame::NONE.fill(t.panel).inner_margin(Margin::symmetric(22, 18))).show(ui, |ui| {
        area_section(ui, view, rec, devices);
        ui.add_space(20.0);
        inputs_section(ui, view, rec, sources, devices, mic);
        ui.add_space(10.0);
        if !rec.records_anything(sources, devices.system_sound, devices.camera_supported) {
            ui.label(RichText::new("Turn on at least one of these to record.").color(t.danger));
        } else if !rec.screen && !(sources.camera && devices.camera_supported) {
            note(ui, "Sound only: the video's picture is black.");
        } else if !rec.screen {
            note(ui, "No screen: the camera's picture is the video.");
        }
    });
    if choice.is_none() && !popup {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            choice = Some(RecorderChoice::Cancel);
        } else if !typing && can && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            choice = Some(RecorderChoice::Record);
        }
    }
    choice
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT as _, Queryable as _};
    use screengrabcraft_model::{Settings, Theme};

    use super::*;

    struct State {
        settings: Settings,
        view: RecorderView,
        devices: RecorderDevices,
        mic: MicLevel,
        pressed: Option<RecorderChoice>,
    }

    fn devices() -> RecorderDevices {
        RecorderDevices {
            monitors: vec![
                RecorderMonitor { name: "DELL U2720Q".into(), x: 0, y: 0, width: 3840, height: 2160, primary: true },
                RecorderMonitor { name: "Laptop".into(), x: -1920, y: 0, width: 1920, height: 1080, primary: false },
            ],
            all_screens: Some((3840, 1440)),
            microphones: vec!["Headset".into(), "Webcam microphone".into()],
            cameras: Some(vec![(0, "Webcam".into())]),
            camera_supported: true,
            system_sound: true,
            system_sound_platform: true,
        }
    }

    fn harness(settings: Settings, devices: RecorderDevices) -> Harness<'static, State> {
        Harness::builder().with_size(SIZE).build_ui_state(
            |ui, st: &mut State| {
                crate::theme::apply(ui.ctx(), Theme::Light);
                if let Some(c) = recorder_ui(ui, &mut st.view, &mut st.settings.recorder, &mut st.settings.production, &st.devices, &st.mic) {
                    st.pressed = Some(c);
                }
            },
            State { settings, view: RecorderView::default(), devices, mic: MicLevel::Off, pressed: None },
        )
    }

    fn press(h: &mut Harness<'_, State>, label: &str) {
        h.get_by_label(label).click();
        h.run_steps(2);
    }

    fn disabled(h: &Harness<'_, State>, label: &str) -> bool {
        h.get_by_label(label).accesskit_node().is_disabled()
    }

    /// Whether a switch shows on.
    fn on(h: &Harness<'_, State>, label: &str) -> bool {
        h.get_by_label(label).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
    }

    #[test]
    fn the_area_is_chosen_from_its_tiles_and_kept() {
        let mut h = harness(Settings::default(), devices());
        h.run_steps(2);
        assert_eq!(h.state().settings.recorder.area, RecordArea::Region);
        press(&mut h, "Window");
        assert_eq!(h.state().settings.recorder.area, RecordArea::Window);
        press(&mut h, "Full screen");
        assert_eq!(h.state().settings.recorder.area, RecordArea::Screen);
        h.get_by_value("DELL U2720Q — 3840 × 2160 (main)");
        press(&mut h, "All screens");
        h.get_by_label_contains("All 2 monitors in one video, 3840 × 1440");
        press(&mut h, "Fixed size");
        assert_eq!(h.state().settings.recorder.area, RecordArea::Fixed);
        h.get_by_value("1920 × 1080 — Full HD");
        // The choices are settings: they come back the same from the file.
        let s = &h.state().settings;
        let back = Settings::from_json(&serde_json::to_string(s).unwrap_or_default());
        assert_eq!((back.recorder, back.production), (s.recorder.clone(), s.production.clone()));
        assert!(h.state().pressed.is_none(), "choosing records nothing");
    }

    fn monitor(width: u32, height: u32) -> RecorderMonitor {
        RecorderMonitor { name: "Screen".into(), x: 0, y: 0, width, height, primary: true }
    }

    fn listed(monitors: &[RecorderMonitor]) -> Vec<(u32, u32)> {
        listed_sizes(monitors).into_iter().flat_map(|(_, sizes)| sizes).map(|(w, h, _)| (w, h)).collect()
    }

    #[test]
    fn the_sizes_are_each_once_and_in_range() {
        let mut seen = std::collections::HashSet::new();
        for &(w, h, what) in FIXED_SIZES {
            assert!((FIXED_MIN..=FIXED_MAX).contains(&w) && (FIXED_MIN..=FIXED_MAX).contains(&h), "{w} × {h}");
            assert!(seen.insert((w, h)), "{w} × {h} twice");
            assert!(!what.is_empty());
        }
        // Each size is listed once, under its shape.
        let [land, tall, square] = listed_sizes(&[]);
        assert!(land.1.iter().all(|s| s.0 > s.1) && tall.1.iter().all(|s| s.0 < s.1) && square.1.iter().all(|s| s.0 == s.1));
        assert_eq!(listed(&[]).len(), FIXED_SIZES.len(), "no monitor found: every size");
    }

    #[test]
    fn only_the_sizes_that_fit_on_a_monitor_are_listed() {
        // A Full HD monitor: nothing wider or taller, but some landscape, portrait and square.
        let hd = listed(&[monitor(1920, 1080)]);
        assert!(hd.iter().all(|&(w, h)| w <= 1920 && h <= 1080), "{hd:?}");
        for size in [(1920, 1080), (1366, 768), (1280, 720), (540, 960), (1080, 1080)] {
            assert!(hd.contains(&size), "{size:?}");
        }
        // With a second one turned on end, its tall sizes too.
        let both = listed(&[monitor(1920, 1080), RecorderMonitor { x: 1920, ..monitor(1080, 1920) }]);
        assert!(both.contains(&(1080, 1920)) && both.contains(&(1080, 1350)) && !both.contains(&(1200, 1920)));
        // A 4K monitor: up to 4K, not 5K.
        let uhd = listed(&[monitor(3840, 2160)]);
        assert!(uhd.contains(&(3840, 2160)) && uhd.contains(&(2160, 2160)) && !uhd.contains(&(5120, 2880)));
        assert_eq!(largest(&[monitor(1920, 1080), monitor(1080, 1920)]), (1920, 1920));
        assert_eq!(largest(&[]), (FIXED_MAX, FIXED_MAX));
    }

    #[test]
    fn a_size_is_picked_from_the_list() {
        let mut s = Settings::default();
        s.recorder.area = RecordArea::Fixed;
        let d = RecorderDevices { monitors: vec![monitor(1920, 1080)], ..devices() };
        let mut h = harness(s, d);
        h.run_steps(2);
        h.get_by_value("1920 × 1080 — Full HD").click();
        h.run_steps(2);
        for group in ["Landscape", "Portrait", "Square"] {
            h.get_by_label(group);
        }
        assert!(h.query_by_label("2560 × 1440 — 1440p").is_none(), "too big for the monitor");
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/recorder-sizes.png"));
        }
        h.get_by_label("540 × 960 — 540p, for phones").click();
        h.run_steps(2);
        let r = &h.state().settings.recorder;
        assert_eq!((r.width, r.height), (540, 960));
        h.get_by_label_contains("Only the sizes that fit on your monitors are listed.");
    }

    #[test]
    fn a_size_bigger_than_the_monitors_says_so() {
        let mut s = Settings::default();
        s.recorder.area = RecordArea::Fixed;
        (s.recorder.width, s.recorder.height) = (2560, 1440);
        let d = RecorderDevices { monitors: vec![monitor(1920, 1080)], ..devices() };
        let mut h = harness(s, d);
        h.run_steps(2);
        h.get_by_value("2560 × 1440 — 1440p");
        h.get_by_label_contains("bigger than your monitors");
    }

    #[test]
    fn a_size_of_ones_own_is_typed_in() {
        let mut s = Settings::default();
        s.recorder.area = RecordArea::Fixed;
        (s.recorder.width, s.recorder.height) = (1000, 500);
        let mut h = harness(s, devices());
        h.run_steps(2);
        h.get_by_value("A size of your own");
        h.get_by_label("pixels");
    }

    #[test]
    fn the_inputs_switch_on_and_off_and_record_needs_one() {
        let mut h = harness(Settings::default(), devices());
        h.run_steps(2);
        // Video Production takes in everything unless told otherwise.
        let o = h.state().settings.production.clone();
        assert!(h.state().settings.recorder.screen && o.camera && o.microphone && o.system_sound);
        for label in ["Screen", "Camera", "Microphone", "Computer sound"] {
            press(&mut h, label);
        }
        let s = &h.state().settings;
        assert!(!s.recorder.screen && !s.production.camera && !s.production.microphone && !s.production.system_sound);
        assert!(disabled(&h, "Record"));
        h.get_by_label("Turn on at least one of these to record.");
        h.get_by_label("Record").click();
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state().pressed, None, "nothing to record, nothing recorded");
        // The camera alone is enough: its picture is the video.
        press(&mut h, "Camera");
        assert!(!disabled(&h, "Record"));
        h.get_by_label("No screen: the camera's picture is the video.");
        press(&mut h, "Record");
        assert_eq!(h.state().pressed, Some(RecorderChoice::Record));
    }

    #[test]
    fn enter_records_and_esc_cancels() {
        for (key, wanted) in [(egui::Key::Enter, RecorderChoice::Record), (egui::Key::Escape, RecorderChoice::Cancel)] {
            let mut h = harness(Settings::default(), devices());
            h.run_steps(2);
            h.key_press(key);
            h.run_steps(2);
            assert_eq!(h.state().pressed, Some(wanted), "{key:?}");
        }
        let mut h = harness(Settings::default(), devices());
        h.run_steps(2);
        press(&mut h, "Cancel");
        assert_eq!(h.state().pressed, Some(RecorderChoice::Cancel));
    }

    #[test]
    fn what_this_system_cant_record_is_shown_off_and_doesnt_count() {
        let d = RecorderDevices { camera_supported: false, system_sound: false, system_sound_platform: false, cameras: None, ..devices() };
        let mut s = Settings::default();
        s.recorder.screen = false;
        s.production.microphone = false;
        // The camera and the computer's sound are on in the settings, but can't be had here.
        assert!(s.production.camera && s.production.system_sound);
        let mut h = harness(s, d);
        h.run_steps(2);
        for label in ["Camera", "Computer sound"] {
            assert!(disabled(&h, label), "{label}");
            assert!(!on(&h, label), "{label} is shown off");
        }
        h.get_by_label("Only on Windows: this system has no way to record what it plays.");
        h.get_by_label("Cameras can't be recorded on this system yet.");
        assert!(disabled(&h, "Record"));
    }

    #[test]
    fn the_microphone_is_listened_to_only_when_asked() {
        let mut s = Settings::default();
        s.production.microphone = false;
        let mut h = harness(s, devices());
        h.run_steps(2);
        assert!(!h.state().view.listen);
        // Turned on here: that's asking to hear it.
        press(&mut h, "Microphone");
        assert!(h.state().view.listen);
        h.state_mut().mic = MicLevel::Level(0.4);
        h.run_steps(2);
        h.get_by_label("Microphone level");
        press(&mut h, "Microphone");
        assert!(!h.state().view.listen, "off: no longer listened to");
        // On from before: a Test button, and nothing heard until it's pressed.
        let mut h = harness(Settings::default(), devices());
        h.run_steps(2);
        assert!(h.query_by_label("Microphone level").is_none());
        press(&mut h, "Test");
        assert!(h.state().view.listen);
        h.state_mut().mic = MicLevel::Failed("no microphone".into());
        h.run_steps(2);
        h.get_by_label("Can't hear it");
    }

    #[test]
    fn record_is_the_big_red_button_at_the_bottom_right() {
        let mut h = harness(Settings::default(), devices());
        h.run_steps(2);
        let (record, cancel) = (h.get_by_label("Record").rect(), h.get_by_label("Cancel").rect());
        assert!(record.width() > widgets::BUTTON_SIZE.x && record.height() > widgets::BUTTON_SIZE.y, "{record:?}");
        // The last button of the row (Cancel on Windows, Record on macOS) ends at the right.
        let right = record.max.x.max(cancel.max.x);
        assert!(record.max.y > SIZE.y - 40.0 && right > SIZE.x - 40.0, "{record:?} {cancel:?}");
        if cfg!(target_os = "macos") {
            assert!(cancel.max.x < record.min.x);
        } else {
            assert!(record.max.x < cancel.min.x);
        }
    }
}
