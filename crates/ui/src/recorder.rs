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
use crate::widgets::{self, StandardButton};

/// The window's size: four cards side by side, Record under them.
pub const SIZE: Vec2 = vec2(1012.0, 418.0);
/// Each card, and the room between them.
const CARD: Vec2 = vec2(232.0, 250.0);
const CARD_GAP: f32 = 14.0;
/// The camera's picture in its card (16:9, as wide as the card).
const PREVIEW: Vec2 = vec2(204.0, 115.0);
/// The size of the Record button: larger than the others, as the window's one big action.
const RECORD_SIZE: Vec2 = vec2(170.0, 46.0);
/// A square choosing the area.
const CHIP: Vec2 = vec2(36.0, 34.0);

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
    /// The system's default microphone, by name.
    pub default_microphone: Option<String>,
    /// The cameras by number and name, once looked for (only while the camera is on: on a Mac,
    /// looking asks the owner).
    pub cameras: Option<Vec<(u32, String)>>,
    /// Whether this system can record a camera at all.
    pub camera_supported: bool,
    /// Whether the computer's sound can be recorded here, now.
    pub system_sound: bool,
    /// Whether this kind of system can record the computer's sound at all (Windows).
    pub system_sound_platform: bool,
    /// The speakers whose sound can be recorded, and which is the system's default.
    pub speakers: Vec<String>,
    pub default_speaker: Option<String>,
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
    /// Heard for a while, and nothing at all came: not even the hiss every microphone hears
    /// (Windows hands programs silence when it doesn't let them use the microphone).
    Silent,
    /// It couldn't be heard, and why.
    Failed(String),
}

/// Said of a microphone that hears only silence.
pub const SILENT_MICROPHONE: &str = "It hears only silence. In Windows Settings, Privacy & security, Microphone, let desktop apps use the microphone; check it isn't muted, and that a headset is switched on.";

/// The window's own state, what isn't a setting.
#[derive(Clone, Debug, Default)]
pub struct RecorderView {
    /// A fixed size typed in rather than one of [`FIXED_SIZES`]; none until first shown.
    custom: Option<bool>,
    /// The owner has asked to hear the microphone, by turning it on here or with Test: the
    /// service listens to it for the meter while this is on and the microphone is.
    pub listen: bool,
    /// The owner has asked to see the camera (its Test): the service shows what it sends while
    /// this is on and the camera is.
    pub watch: bool,
    /// How to pause and stop the recording, said beside Record (`F9 pauses, F10 stops`).
    pub hint: Option<String>,
}

/// The camera while it's tested, as the service has it.
#[derive(Clone, Default)]
pub enum CameraPreview {
    /// Not tested.
    #[default]
    Off,
    /// Turned on, its first picture not come yet.
    Starting,
    /// Its latest picture.
    Live(egui::TextureHandle),
    /// No pictures, and why, in plain words.
    Failed(String),
}

/// What the Recorder's buttons do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecorderChoice {
    Record,
    Cancel,
    /// Put the Recorder away in the tray (its icon brings it back).
    Hide,
}

/// The red of the Record button: as it is, under the pointer, pressed.
fn reds(dark: bool) -> [Color32; 3] {
    if dark {
        [Color32::from_rgb(0xE5, 0x48, 0x4D), Color32::from_rgb(0xEE, 0x63, 0x67), Color32::from_rgb(0xC4, 0x3A, 0x3F)]
    } else {
        [Color32::from_rgb(0xD3, 0x2F, 0x2F), Color32::from_rgb(0xE5, 0x39, 0x35), Color32::from_rgb(0xB7, 0x1C, 0x1C)]
    }
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

/// Said under the microphone when it's the camera's own and the camera is on.
pub const OWN_MICROPHONE_NOTE: &str =
    "That's the camera's own microphone. Some cameras send no video while it records: if the camera stays off, choose another microphone.";

/// Said under the microphone when there's none.
pub const NO_MICROPHONE_NOTE: &str = "No microphone was found. Over Remote Desktop, the Remote Desktop app has to share yours.";

/// Whether `microphone` is part of the camera `camera` (Windows names a webcam's microphone
/// "Microphone (<camera>)").
fn is_own_microphone(microphone: &str, camera: &str) -> bool {
    let camera = camera.trim().to_lowercase();
    !camera.is_empty() && microphone.to_lowercase().contains(&camera)
}

fn note(ui: &mut Ui, text: impl Into<String>) {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text.into()).size(12.0).color(t.muted)).wrap());
}

fn warning(ui: &mut Ui, text: impl Into<String>) {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text.into()).size(12.0).color(t.danger)).wrap());
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

/// What happens once Record is pressed.
fn next_step(rec: &RecorderOptions) -> &'static str {
    if !rec.screen {
        return "Recording starts as soon as you press Record.";
    }
    match rec.area {
        RecordArea::Region => "Press Record, then drag out the area to record.",
        RecordArea::Window => "Press Record, then click the window to record.",
        RecordArea::Fixed => "Press Record, then click where the frame goes.",
        RecordArea::Screen | RecordArea::AllScreens => "Recording starts as soon as you press Record.",
    }
}

/// An on/off switch with no words of its own (its card says what it is): `label` names it for
/// screen readers and tips.
fn toggle(ui: &mut Ui, on: &mut bool, label: &str, enabled: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, mut resp) = ui.allocate_exact_size(vec2(42.0, 24.0), if enabled { Sense::click() } else { Sense::hover() });
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let shown = *on && enabled;
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, shown, label));
    let track = Rect::from_center_size(rect.center(), vec2(40.0, 22.0));
    let (fill, edge, knob) = match (enabled, shown) {
        (true, true) => (if resp.hovered() { t.accent_hover } else { t.accent }, t.accent, t.accent_text),
        (true, false) => (if resp.hovered() { t.control_hover } else { t.control }, t.control_border, t.text),
        (false, _) => (t.control, t.border, t.border),
    };
    let p = ui.painter();
    p.rect(track, 11.0, fill, Stroke::new(1.0, edge), StrokeKind::Inside);
    let kx = if shown { track.max.x - 11.0 } else { track.min.x + 11.0 };
    p.circle_filled(pos2(kx, track.center().y), 7.0, knob);
    if resp.has_focus() {
        p.rect_stroke(track.expand(2.0), 13.0, Stroke::new(1.5, t.text), StrokeKind::Inside);
    }
    resp.on_hover_text(if shown { format!("Turn the {} off", label.to_lowercase()) } else { format!("Record the {}", label.to_lowercase()) })
}

/// One of the Recorder's cards: its picture, its name and its switch along the top, then what's
/// in it (greyed while it's off). True when the switch was turned.
fn card(ui: &mut Ui, icon: Icon, title: &str, on: &mut bool, enabled: bool, add: impl FnOnce(&mut Ui, bool)) -> bool {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(CARD, Sense::hover());
    let lit = *on && enabled;
    let (fill, edge) = if lit { (t.window, Stroke::new(1.5, t.accent)) } else { (t.panel, Stroke::new(1.0, t.border)) };
    ui.painter().rect(rect, 12.0, fill, edge, StrokeKind::Inside);
    let inner = rect.shrink2(vec2(14.0, 12.0));
    // Drawn inside the card's own place (a child that takes no more room in the row).
    let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
    let (top, _) = ui.allocate_exact_size(vec2(inner.width(), 26.0), Sense::hover());
    let p = ui.painter();
    let badge = Rect::from_min_size(pos2(top.min.x, top.center().y - 14.0), vec2(28.0, 28.0));
    p.rect_filled(badge, 7.0, if lit { t.selected } else { t.control });
    icons::paint(p, badge.shrink(5.0), icon, if lit { t.accent } else { t.muted });
    p.text(
        pos2(badge.max.x + 10.0, top.center().y),
        Align2::LEFT_CENTER,
        title,
        widgets::strong_font(ui.ctx(), 15.0),
        if enabled { t.text } else { t.muted },
    );
    let at = Rect::from_min_max(pos2(top.max.x - 44.0, top.min.y), top.max);
    let changed = toggle(&mut ui.new_child(egui::UiBuilder::new().max_rect(at)), on, title, enabled).changed();
    ui.add_space(12.0);
    let lit = *on && enabled;
    ui.add_enabled_ui(lit, |ui| add(ui, lit));
    changed
}

/// A choice of area: a small square with its picture, outlined in the accent colour when chosen.
fn area_chip(ui: &mut Ui, area: RecordArea, selected: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let enabled = ui.is_enabled();
    let (rect, resp) = ui.allocate_exact_size(CHIP, Sense::click());
    let label = area.label();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, selected, label));
    let fill = if selected {
        t.selected
    } else if resp.hovered() && enabled {
        t.hover
    } else {
        t.control
    };
    let edge = if selected { Stroke::new(1.5, t.accent) } else { Stroke::new(1.0, t.border) };
    let p = ui.painter();
    p.rect(rect, 7.0, fill, edge, StrokeKind::Inside);
    let ink = if !enabled {
        t.muted
    } else if selected {
        t.accent
    } else {
        t.text
    };
    let pic = Rect::from_center_size(rect.center(), vec2(22.0, 22.0));
    match area {
        RecordArea::Region => icons::paint(p, pic, Icon::Region, ink),
        RecordArea::Window => icons::paint(p, pic, Icon::Window, ink),
        RecordArea::Screen => icons::paint(p, pic, Icon::Screen, ink),
        RecordArea::AllScreens => icons::paint(p, pic, Icon::AllScreens, ink),
        RecordArea::Fixed => paint_fixed(p, pic, ink),
    }
    if resp.has_focus() {
        p.rect_stroke(rect.expand(2.0), 9.0, Stroke::new(1.5, t.text), StrokeKind::Inside);
    }
    resp.on_hover_text(label)
}

/// The Screen card's contents: the area chosen from its squares, and what goes with it.
fn screen_card(ui: &mut Ui, view: &mut RecorderView, rec: &mut RecorderOptions, devices: &RecorderDevices) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        for area in RecordArea::ALL {
            if area_chip(ui, area, rec.area == area).clicked() {
                rec.area = area;
            }
        }
    });
    ui.add_space(10.0);
    ui.label(RichText::new(area_name(rec.area)).font(widgets::strong_font(ui.ctx(), 14.0)).color(t.text));
    ui.add_space(4.0);
    match rec.area {
        RecordArea::Region => note(ui, "Drag out the area after you press Record. Its size is shown as you drag."),
        RecordArea::Window => note(ui, "Click the window after you press Record. The video keeps to where the window was then."),
        RecordArea::Screen => monitor_choice(ui, rec, devices),
        RecordArea::AllScreens => note(
            ui,
            match (devices.monitors.len(), devices.all_screens) {
                (0, _) => "No monitor was found.".to_string(),
                (1, _) => "There's one monitor: it's recorded whole.".to_string(),
                (n, Some((w, h))) => format!("All {n} monitors in one video, {w} × {h}, as they're placed."),
                (n, None) => format!("All {n} monitors in one video."),
            },
        ),
        RecordArea::Fixed => fixed_choice(ui, view, rec, devices),
    }
}

/// The area's name as the card heads its details (the squares themselves are named for screen
/// readers and in their tips).
fn area_name(area: RecordArea) -> &'static str {
    match area {
        RecordArea::Region => "An area you drag out",
        RecordArea::Window => "One window",
        RecordArea::Screen => "One whole monitor",
        RecordArea::AllScreens => "Every monitor",
        RecordArea::Fixed => "A frame of a set size",
    }
}

fn monitor_choice(ui: &mut Ui, rec: &mut RecorderOptions, devices: &RecorderDevices) {
    if devices.monitors.is_empty() {
        note(ui, "No monitor was found.");
        return;
    }
    let chosen = chosen_monitor(rec, &devices.monitors);
    let shown = chosen.and_then(|i| devices.monitors.get(i).map(|m| monitor_label(m, i))).unwrap_or_default();
    egui::ComboBox::from_id_salt("recorder-monitor").width(ui.available_width()).truncate().selected_text(shown).show_ui(ui, |ui| {
        for (i, m) in devices.monitors.iter().enumerate() {
            if ui.selectable_label(chosen == Some(i), monitor_label(m, i)).clicked() {
                rec.monitor = Some(MonitorChoice { name: m.name.clone(), x: m.x, y: m.y });
            }
        }
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
    // Short enough to stay in the window: the list scrolls.
    egui::ComboBox::from_id_salt("recorder-size").width(ui.available_width()).height(260.0).truncate().selected_text(shown).show_ui(ui, |ui| {
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
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut rec.width).range(FIXED_MIN..=max_w).speed(2.0));
            ui.label("×");
            ui.add(egui::DragValue::new(&mut rec.height).range(FIXED_MIN..=max_h).speed(2.0));
            ui.label("pixels");
        });
    }
    ui.add_space(4.0);
    if fits(rec.width, rec.height, &devices.monitors) {
        note(ui, "Click where the frame goes after you press Record.");
    } else {
        note(ui, "This is bigger than your monitors, so it's made smaller, keeping its shape.");
    }
}

/// A sound's live level, as wide as its card (or why it can't be heard).
fn level_meter(ui: &mut Ui, level: &MicLevel, name: &str) {
    let t = Tokens::get(ui.ctx());
    match level {
        MicLevel::Failed(why) => {
            ui.label(RichText::new("Can't hear it").size(12.5).color(t.danger)).on_hover_text(why.as_str());
        }
        MicLevel::Silent => {
            ui.label(RichText::new("Hears only silence").size(12.5).color(t.danger)).on_hover_text(SILENT_MICROPHONE);
        }
        MicLevel::Off | MicLevel::Starting | MicLevel::Level(_) => {
            let v = if let MicLevel::Level(l) = level { l.clamp(0.0, 1.0) } else { 0.0 };
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::hover());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ProgressIndicator, true, name));
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

/// The camera's card: its live picture (or why there's none) and which camera.
fn camera_card(ui: &mut Ui, on: bool, o: &mut VideoOptions, d: &RecorderDevices, camera_view: &CameraPreview) {
    let t = Tokens::get(ui.ctx());
    if !d.camera_supported {
        note(ui, "Cameras can't be recorded on this system yet.");
        return;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), PREVIEW.y), Sense::hover());
    let p = ui.painter();
    match (on, camera_view) {
        (true, CameraPreview::Live(texture)) => {
            let s = texture.size_vec2();
            let f = (rect.width() / s.x.max(1.0)).min(rect.height() / s.y.max(1.0));
            let shown = Rect::from_center_size(rect.center(), s * f);
            p.rect_filled(rect, 8.0, Color32::BLACK);
            let resp = ui.put(shown, egui::Image::new((texture.id(), s * f)).corner_radius(8));
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, "The camera's picture"));
        }
        (true, CameraPreview::Failed(why)) => {
            p.rect(rect, 8.0, t.control, Stroke::new(1.0, t.border), StrokeKind::Inside);
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(8.0)), |ui| warning(ui, why.as_str()));
        }
        (true, _) => {
            p.rect(rect, 8.0, t.control, Stroke::new(1.0, t.border), StrokeKind::Inside);
            p.text(rect.center(), Align2::CENTER_CENTER, "Starting the camera…", FontId::proportional(12.5), t.muted);
        }
        (false, _) => {
            p.rect(rect, 8.0, t.control, Stroke::new(1.0, t.border), StrokeKind::Inside);
            icons::paint(p, Rect::from_center_size(rect.center() - vec2(0.0, 10.0), vec2(30.0, 30.0)), Icon::Camera, t.muted);
            p.text(rect.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, "Camera off", FontId::proportional(12.5), t.muted);
        }
    }
    ui.add_space(10.0);
    let list = d.cameras.as_deref().unwrap_or_default();
    let shown = o
        .camera_index
        .and_then(|i| list.iter().find(|c| c.0 == i).map(|c| c.1.clone()))
        .or_else(|| list.first().map(|c| c.1.clone()))
        .unwrap_or_else(|| if d.cameras.is_some() { "No camera was found".to_string() } else { "The first camera".to_string() });
    egui::ComboBox::from_id_salt("recorder-camera").width(ui.available_width()).truncate().selected_text(shown).show_ui(ui, |ui| {
        for (i, name) in list {
            ui.selectable_value(&mut o.camera_index, Some(*i), name);
        }
    });
}

/// A sound's card: which device, and its live level.
#[allow(clippy::too_many_arguments)]
fn sound_card(
    ui: &mut Ui,
    id: &str,
    chosen: &mut Option<String>,
    default: &Option<String>,
    devices: &[String],
    level: &MicLevel,
    meter: &str,
    on: bool,
) {
    let label = match default {
        Some(n) => format!("The system's default ({n})"),
        None => "The system's default".to_string(),
    };
    let shown = chosen.clone().unwrap_or_else(|| label.clone());
    egui::ComboBox::from_id_salt(id).width(ui.available_width()).truncate().selected_text(shown).show_ui(ui, |ui| {
        ui.selectable_value(chosen, None, label);
        for m in devices {
            ui.selectable_value(chosen, Some(m.clone()), m);
        }
    });
    ui.add_space(14.0);
    if on {
        level_meter(ui, level, meter);
    } else {
        level_meter(ui, &MicLevel::Off, meter);
    }
    ui.add_space(8.0);
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
    p.rect(rect, RECORD_SIZE.y / 2.0, fill, Stroke::new(1.0, if enabled { fill } else { t.border }), StrokeKind::Inside);
    let ink = if enabled { Color32::WHITE } else { t.muted };
    let galley = p.layout_no_wrap("Record".to_string(), widgets::strong_font(ui.ctx(), 16.0), ink);
    let dot = 14.0;
    let x = rect.center().x - (dot + 10.0 + galley.size().x) / 2.0;
    p.circle_filled(pos2(x + dot / 2.0, rect.center().y), dot / 2.0, ink);
    p.galley(pos2(x + dot + 10.0, rect.center().y - galley.size().y / 2.0), galley, ink);
    if resp.has_focus() {
        p.rect_stroke(rect.expand(2.0), RECORD_SIZE.y / 2.0 + 2.0, Stroke::new(1.5, t.text), StrokeKind::Inside);
    }
    if enabled { resp.on_hover_text("Start the recording") } else { resp.on_hover_text("Turn on something to record first") }
}

/// The Recorder's contents: a card for each thing it records (the screen, the camera, the
/// microphone, the computer's sound) with its switch, its choices and what it sees or hears live;
/// then Captions, and Record and Cancel along the bottom. Changes the choices in place; returns
/// the button pressed (Enter is Record, Esc is Cancel).
#[allow(clippy::too_many_arguments)]
pub fn recorder_ui(
    ui: &mut Ui,
    view: &mut RecorderView,
    rec: &mut RecorderOptions,
    sources: &mut VideoOptions,
    devices: &RecorderDevices,
    mic: &MicLevel,
    sound: &MicLevel,
    camera: &CameraPreview,
) -> Option<RecorderChoice> {
    let t = Tokens::get(ui.ctx());
    // Read before anything is drawn: Esc closing a list isn't for the window, and Enter on a
    // focused control (a number being typed, a switch) is that control's.
    let popup = egui::Popup::is_any_open(ui.ctx());
    let typing = ui.memory(|m| m.focused().is_some());
    // What's on is seen and heard live (the camera's picture, the sounds' meters), as the owner
    // turned it on to record it.
    view.listen = true;
    view.watch = true;
    let can = rec.records_anything(sources, devices.system_sound, devices.camera_supported);
    let mut choice = None;
    let footer = egui::Panel::bottom("recorder-footer")
        .resizable(false)
        .frame(Frame::NONE.fill(t.footer).inner_margin(Margin::symmetric(widgets::PADDING + 4, 14)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // (Transcribing after the recording is a setting: Settings, Video.)
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = widgets::GAP + 2.0;
                    // The default first and Cancel last, or the other way round on a Mac.
                    let cancel = |ui: &mut Ui| ui.add(StandardButton::new("Cancel")).clicked();
                    if cfg!(target_os = "macos") {
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
                    if let Some(hint) = &view.hint {
                        ui.add_space(6.0);
                        ui.label(RichText::new(hint).size(12.0).color(t.muted));
                    }
                });
            });
        });
    let line = footer.response.rect;
    ui.painter().hline(line.x_range(), line.top(), Stroke::new(1.0, t.border));
    egui::CentralPanel::default().frame(Frame::NONE.fill(t.panel).inner_margin(Margin::symmetric(20, 16))).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("New Recording").font(widgets::strong_font(ui.ctx(), 20.0)).color(t.text));
                ui.label(RichText::new(next_step(rec)).size(12.5).color(t.muted));
            });
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if icons::button(ui, Icon::Minimize, "Put the Recorder away in the tray (click the tray icon to bring it back)", false, true)
                    .clicked()
                {
                    choice = Some(RecorderChoice::Hide);
                }
            });
        });
        ui.add_space(14.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = CARD_GAP;
            let mut screen = rec.screen;
            if card(ui, Icon::Screen, "Screen", &mut screen, true, |ui, _| screen_card(ui, view, rec, devices)) {
                rec.screen = screen;
            }
            let mut camera_on = sources.camera && devices.camera_supported;
            let shown = camera_on;
            if card(ui, Icon::Camera, "Camera", &mut camera_on, devices.camera_supported, |ui, _| camera_card(ui, shown, sources, devices, camera)) {
                sources.camera = camera_on;
            }
            let mut mic_on = sources.microphone;
            let mic_shown = mic_on;
            if card(ui, Icon::Microphone, "Microphone", &mut mic_on, true, |ui, _| {
                sound_card(
                    ui,
                    "recorder-mic",
                    &mut sources.microphone_name,
                    &devices.default_microphone,
                    &devices.microphones,
                    mic,
                    "Microphone level",
                    mic_shown,
                );
                // The camera's own microphone, with the camera on: some cameras send no video
                // while theirs records (the recording then lets the camera go).
                let camera_name = sources
                    .camera_index
                    .and_then(|i| devices.cameras.as_deref().unwrap_or_default().iter().find(|c| c.0 == i).map(|c| c.1.clone()))
                    .or_else(|| devices.cameras.as_deref().and_then(|l| l.first()).map(|c| c.1.clone()));
                let mic_name = sources.microphone_name.clone().or_else(|| devices.default_microphone.clone());
                if sources.camera
                    && let (Some(cam), Some(m)) = (camera_name, mic_name)
                    && is_own_microphone(&m, &cam)
                {
                    note(ui, OWN_MICROPHONE_NOTE);
                }
                if devices.microphones.is_empty() {
                    note(ui, NO_MICROPHONE_NOTE);
                }
                if matches!(mic, MicLevel::Silent) {
                    warning(ui, SILENT_MICROPHONE);
                }
            }) {
                sources.microphone = mic_on;
            }
            let mut sound_on = sources.system_sound && devices.system_sound;
            let sound_shown = sound_on;
            if card(ui, Icon::Speaker, "Computer sound", &mut sound_on, devices.system_sound, |ui, _| {
                if !devices.system_sound_platform {
                    note(ui, "Only on Windows: this system has no way to record what it plays.");
                } else if !devices.system_sound {
                    note(ui, "No sound output was found.");
                } else {
                    sound_card(
                        ui,
                        "recorder-speaker",
                        &mut sources.system_sound_name,
                        &devices.default_speaker,
                        &devices.speakers,
                        sound,
                        "Computer sound level",
                        sound_shown,
                    );
                    note(ui, "What plays through these speakers, like the others in a call.");
                }
            }) {
                sources.system_sound = sound_on;
            }
        });
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
        camera: CameraPreview,
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
            default_microphone: Some("Headset".into()),
            cameras: Some(vec![(0, "Webcam".into())]),
            camera_supported: true,
            system_sound: true,
            system_sound_platform: true,
            speakers: vec!["Speakers".to_string()],
            default_speaker: Some("Speakers".to_string()),
        }
    }

    fn harness(settings: Settings, devices: RecorderDevices) -> Harness<'static, State> {
        Harness::builder().with_size(SIZE).build_ui_state(
            |ui, st: &mut State| {
                crate::theme::apply(ui.ctx(), Theme::Light);
                if let Some(c) = recorder_ui(
                    ui,
                    &mut st.view,
                    &mut st.settings.recorder,
                    &mut st.settings.production,
                    &st.devices,
                    &st.mic,
                    &MicLevel::Off,
                    &st.camera,
                ) {
                    st.pressed = Some(c);
                }
            },
            State { settings, view: RecorderView::default(), devices, mic: MicLevel::Off, camera: CameraPreview::Off, pressed: None },
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
    fn transcribing_is_a_setting_not_a_switch_of_the_recorder() {
        let mut h = harness(Settings::default(), devices());
        h.run_steps(2);
        assert!(h.query_by_label("Captions").is_none());
        assert!(h.state().settings.production.captions, "on unless turned off in Settings");
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
    fn the_cameras_own_microphone_with_the_camera_on_is_pointed_out() {
        let mut s = Settings::default();
        s.production.camera = true;
        s.production.microphone = true;
        s.production.microphone_name = None;
        // The system's default microphone is the camera's own.
        let d = RecorderDevices {
            microphones: vec!["Headset".into(), "Microphone (Studio Webcam 2)".into()],
            default_microphone: Some("Microphone (Studio Webcam 2)".into()),
            cameras: Some(vec![(0, "Studio Webcam 2".into())]),
            ..devices()
        };
        let mut h = harness(s.clone(), d.clone());
        h.run_steps(2);
        h.get_by_value("The system's default (Microphone (Studio Webcam 2))");
        h.get_by_label_contains("That's the camera's own microphone.");
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/recorder-own-microphone.png"));
        }
        // Another microphone, or the camera off: nothing to say.
        s.production.microphone_name = Some("Headset".into());
        let mut h = harness(s.clone(), d.clone());
        h.run_steps(2);
        assert!(h.query_by_label_contains("That's the camera's own microphone.").is_none());
        s.production.microphone_name = None;
        s.production.camera = false;
        let mut h = harness(s, d);
        h.run_steps(2);
        assert!(h.query_by_label_contains("That's the camera's own microphone.").is_none());
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
        h.get_by_label_contains("Click where the frame goes after you press Record.");
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
    fn no_microphone_at_all_is_said_under_it() {
        let mut s = Settings::default();
        s.production.microphone = true;
        let d = RecorderDevices { microphones: Vec::new(), default_microphone: None, ..devices() };
        let mut h = harness(s.clone(), d);
        h.run_steps(2);
        h.get_by_label(NO_MICROPHONE_NOTE);
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/recorder-no-microphone.png"));
        }
        // With one, nothing is said.
        let mut h = harness(s, devices());
        h.run_steps(2);
        assert!(h.query_by_label(NO_MICROPHONE_NOTE).is_none());
    }

    #[test]
    fn a_microphone_that_hears_only_silence_is_said_so() {
        let mut s = Settings::default();
        s.production.microphone = true;
        let mut h = harness(s, devices());
        h.state_mut().view.listen = true;
        h.state_mut().mic = MicLevel::Silent;
        h.run_steps(2);
        h.get_by_label("Hears only silence");
        h.get_by_label_contains("let desktop apps use the microphone");
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/recorder-silent-microphone.png"));
        }
        // Heard: no such note.
        h.state_mut().mic = MicLevel::Level(0.2);
        h.run_steps(2);
        assert!(h.query_by_label_contains("let desktop apps use the microphone").is_none());
    }

    #[test]
    fn the_camera_is_seen_live_while_its_on_or_why_there_is_none() {
        let mut s = Settings::default();
        s.production.camera = true;
        s.production.microphone = true;
        let mut h = harness(s, devices());
        h.run_steps(2);
        // On: watched at once (the service then says how the camera is).
        assert!(h.state().view.watch);
        let grey = egui::ColorImage::new([64, 48], vec![egui::Color32::from_gray(128); 64 * 48]);
        let texture = h.ctx.load_texture("camera-test", grey, egui::TextureOptions::LINEAR);
        h.state_mut().camera = CameraPreview::Live(texture.clone());
        h.state_mut().mic = MicLevel::Level(0.35);
        h.run_steps(2);
        h.get_by_label("The camera's picture");
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/recorder.png"));
        }
        // None: why, in plain words.
        h.state_mut().camera = CameraPreview::Failed("Webcam sends no video while its own microphone is on.".into());
        h.run_steps(2);
        h.get_by_label_contains("sends no video while its own microphone is on");
        // Off: no picture, whatever the service still has.
        h.state_mut().camera = CameraPreview::Live(texture);
        press(&mut h, "Camera");
        assert!(!h.state().settings.production.camera);
        assert!(h.query_by_label("The camera's picture").is_none());
    }

    #[test]
    fn the_sounds_are_heard_live_while_theyre_on() {
        let mut s = Settings::default();
        s.production.microphone = true;
        let mut h = harness(s, devices());
        h.state_mut().mic = MicLevel::Level(0.4);
        h.run_steps(2);
        assert!(h.state().view.listen);
        h.get_by_label("Microphone level");
        h.state_mut().mic = MicLevel::Failed("no microphone".into());
        h.run_steps(2);
        h.get_by_label("Can't hear it");
        // Off: nothing said about hearing it.
        press(&mut h, "Microphone");
        assert!(!h.state().settings.production.microphone);
        assert!(h.query_by_label("Can't hear it").is_none());
    }

    #[test]
    fn the_recorder_can_be_put_away_in_the_tray() {
        let mut h = harness(Settings::default(), devices());
        h.run_steps(2);
        h.get_by_label("Put the Recorder away in the tray (click the tray icon to bring it back)").click();
        h.run_steps(1);
        assert_eq!(h.state().pressed, Some(RecorderChoice::Hide));
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
