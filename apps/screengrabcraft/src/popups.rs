//! The service's small windows: the countdown before a delayed capture, the control of a running
//! recording, and the notice after a capture is kept.
//!
//! Each is a borderless window drawn as a standard popup (`screengrabcraft_ui::widgets`): the
//! panel colour of the light or dark look, a 1 px border all round, its buttons at the right.
//! What each shows is drawn by a function of its own (`notice_ui`, …) that only says what was
//! pressed, so it can be drawn and tested headless.

use std::path::Path;
use std::time::Instant;

use egui::{RichText, Ui, Vec2, ViewportBuilder, ViewportId, pos2, vec2};
use screengrabcraft_desktop::launch;
use screengrabcraft_model::IRect;
use screengrabcraft_ui::theme::Tokens;
use screengrabcraft_ui::widgets::{self, FooterButton};

use crate::app::{APP_ID, Service};
use crate::record::Kind;

fn popup(title: &str, at: egui::Pos2, size: egui::Vec2, active: bool) -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title(title)
        .with_app_id(APP_ID)
        .with_decorations(false)
        .with_resizable(false)
        .with_taskbar(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
        .with_active(active)
        .with_position(at)
        .with_inner_size(size)
}

/// A desktop point in egui points for a window placed on a monitor of this scale.
fn points(x: f32, y: f32, scale: f32) -> egui::Pos2 {
    if cfg!(target_os = "macos") { pos2(x, y) } else { pos2(x / scale.max(0.5), y / scale.max(0.5)) }
}

// ---- the countdown ----

const COUNTDOWN_SIZE: Vec2 = vec2(184.0, 196.0);

/// The countdown's face: the seconds left, and Cancel. True when Cancel was pressed.
fn countdown_ui(ui: &mut Ui, left: u32) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut cancel = false;
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(left.max(1).to_string()).font(widgets::strong_font(ui.ctx(), 84.0)).color(t.text));
        ui.label(RichText::new("Capturing…").color(t.muted));
        ui.add_space(10.0);
        cancel = widgets::button(ui, "Cancel").clicked();
    });
    cancel
}

pub fn countdown(svc: &mut Service, ctx: &egui::Context) {
    let Some(c) = &svc.countdown else { return };
    let left = c.until.saturating_duration_since(Instant::now()).as_secs_f32().ceil() as u32;
    let size = COUNTDOWN_SIZE;
    let at = match c.on {
        Some((r, s)) => {
            let center = points(r.x as f32 + r.w as f32 / 2.0, r.y as f32 + r.h as f32 / 2.0, s);
            center - size / 2.0
        }
        None => pos2(200.0, 200.0),
    };
    let mut cancel = false;
    ctx.show_viewport_immediate(ViewportId::from_hash_of("countdown"), popup("ScreenGrabCraft countdown", at, size, false), |ui, _| {
        egui::CentralPanel::default().frame(widgets::popup_frame(ui.ctx())).show(ui, |ui| {
            cancel = countdown_ui(ui, left);
        });
    });
    if cancel {
        svc.countdown = None;
    }
}

// ---- the recording control ----

/// What the recording control's buttons do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecordingChoice {
    /// Stop and keep it.
    Stop,
    /// Stop and throw it away.
    Discard,
    /// Pause it, or go on.
    Pause,
    /// Throw it away and record the same again.
    Restart,
    /// The red border round the area, off or on.
    Border,
    /// Put the controls away in the tray.
    Hide,
    /// The bar is being dragged somewhere else.
    Drag,
}

fn recording_size(kind: Kind) -> Vec2 {
    let w = match kind {
        Kind::Scrolling => 600.0,
        Kind::Video | Kind::Production => 680.0,
        Kind::Gif => 560.0,
    };
    vec2(w, 56.0)
}

/// What the recording control shows.
#[derive(Clone, Debug, Default, PartialEq)]
struct RecordingView {
    secs: u64,
    /// Frames (GIF, video) or the picture's height (scrolling).
    progress: u32,
    lost: u32,
    /// A video being made after the recording: 1 to 1001 (its progress + 1), else 0.
    saving: u32,
    /// Stop was pressed and it's finishing.
    stopping: bool,
    paused: bool,
    /// The red border round the area is shown.
    border: bool,
    /// The computer's sound and the microphone, if they're recorded, and how loud they are.
    sound: Option<f32>,
    mic: Option<f32>,
    /// What was asked for and couldn't be had (`No camera: …`).
    missing: Vec<String>,
    /// The shortcuts that pause and stop it, as the tips show them.
    pause_key: Option<String>,
    stop_key: Option<String>,
}

/// A red light that blinks every second and the time, or, paused, an amber pause sign.
fn red_light(ui: &mut Ui, secs: u64, paused: bool, text: egui::Color32) {
    let (dot, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
    if paused {
        screengrabcraft_ui::icons::paint(ui.painter(), dot, screengrabcraft_ui::icons::Icon::Pause, egui::Color32::from_rgb(0xF5, 0x9E, 0x0B));
    } else {
        let red = if secs.is_multiple_of(2) { egui::Color32::from_rgb(0xE5, 0x39, 0x35) } else { egui::Color32::from_rgb(0x99, 0x22, 0x22) };
        ui.painter().circle_filled(dot.center(), 6.0, red);
    }
    ui.label(RichText::new(format!("{:02}:{:02}", secs / 60, secs % 60)).monospace().size(16.0).color(text));
    if paused {
        ui.label(RichText::new("Paused").strong().color(egui::Color32::from_rgb(0xF5, 0x9E, 0x0B)));
    }
}

/// Six dots at the bar's left: it can be dragged.
fn grip(ui: &mut Ui, color: egui::Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(10.0, 22.0), egui::Sense::hover());
    for row in 0..3 {
        for col in 0..2 {
            ui.painter().circle_filled(pos2(r.min.x + 2.5 + col as f32 * 5.0, r.min.y + 4.0 + row as f32 * 7.0), 1.4, color);
        }
    }
}

/// `words` and the shortcut that does the same, for a tip.
fn with_key(words: &str, key: &Option<String>) -> String {
    match key {
        Some(k) => format!("{words} ({k})"),
        None => words.to_string(),
    }
}

/// The recording control's face: a grip, how far it has got, then Pause, Restart, Border and
/// Minimize, and [Stop] [Discard] at the right. Anywhere that isn't a button drags it. Once Stop is
/// pressed it says so at once, and only Discard is left.
fn recording_ui(ui: &mut Ui, kind: Kind, v: &RecordingView) -> Option<RecordingChoice> {
    use screengrabcraft_ui::icons::{self, Icon};
    let t = Tokens::get(ui.ctx());
    let mut choice = None;
    let making = matches!(kind, Kind::Video | Kind::Production) && v.saving > 0;
    let finishing = making || v.stopping;
    // Made first, so the buttons on top of it take their clicks.
    let bar = ui.interact(ui.max_rect(), ui.id().with("drag the bar"), egui::Sense::drag());
    if bar.drag_started() {
        choice = Some(RecordingChoice::Drag);
    }
    if bar.hovered() && !finishing {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        grip(ui, t.muted);
        match kind {
            _ if v.stopping && !making => {
                ui.add(egui::Spinner::new().size(16.0).color(t.text));
                let secs = v.secs;
                ui.label(RichText::new(format!("Stopping… ({:02}:{:02} recorded)", secs / 60, secs % 60)).color(t.text));
            }
            Kind::Gif => {
                red_light(ui, v.secs, v.paused, t.text);
                ui.label(RichText::new(format!("{} frames", v.progress)).color(t.muted));
            }
            Kind::Scrolling => {
                let text = if v.lost > 2 { "Scroll more slowly…".to_string() } else { format!("Scroll the area — {} px", v.progress) };
                ui.label(RichText::new(text).color(if v.lost > 2 { t.danger } else { t.text }));
            }
            Kind::Video | Kind::Production if making => {
                let done = v.saving.saturating_sub(1).min(1000) as f32 / 1000.0;
                ui.label(RichText::new("Making the video…").color(t.text));
                ui.add(egui::ProgressBar::new(done).desired_width(150.0).show_percentage());
            }
            Kind::Video | Kind::Production => {
                red_light(ui, v.secs, v.paused, t.text);
                // A meter for each sound, so the owner sees it's being heard.
                for (name, level) in [("Sound", v.sound), ("Mic", v.mic)] {
                    if let Some(l) = level {
                        ui.label(RichText::new(name).size(12.0).color(t.muted));
                        ui.add(egui::ProgressBar::new(l.clamp(0.0, 1.0)).desired_width(40.0).desired_height(6.0));
                    }
                }
                if !v.missing.is_empty() {
                    ui.label(RichText::new("⚠").color(egui::Color32::from_rgb(0xF5, 0x9E, 0x0B))).on_hover_text(v.missing.join("\n"));
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if finishing {
                if widgets::button(ui, "Discard").on_hover_text("Stop waiting and throw the recording away").clicked() {
                    choice = Some(RecordingChoice::Discard);
                }
                return;
            }
            if widgets::button(ui, "Discard").on_hover_text("Stop and throw the recording away").clicked() {
                choice = Some(RecordingChoice::Discard);
            }
            let (stop, tip) = match kind {
                Kind::Scrolling => ("Done", "Stop scrolling and keep the picture".to_string()),
                _ => ("Stop", with_key("Stop and save", &v.stop_key)),
            };
            if widgets::primary_button(ui, stop).on_hover_text(tip).clicked() {
                choice = Some(RecordingChoice::Stop);
            }
            ui.add_space(4.0);
            ui.separator();
            if icons::button(ui, Icon::Minimize, "Put the controls away in the tray (its icon has them)", false, true).clicked() {
                choice = Some(RecordingChoice::Hide);
            }
            let border = if v.border { "Hide the red border" } else { "Show the red border" };
            if icons::button(ui, Icon::Marquee, border, v.border, true).clicked() {
                choice = Some(RecordingChoice::Border);
            }
            if icons::button(ui, Icon::Restart, "Restart: throw this away and record again", false, true).clicked() {
                choice = Some(RecordingChoice::Restart);
            }
            if kind != Kind::Scrolling {
                let (icon, words) = if v.paused { (Icon::Play, "Resume") } else { (Icon::Pause, "Pause") };
                if icons::button(ui, icon, &with_key(words, &v.pause_key), v.paused, true).clicked() {
                    choice = Some(RecordingChoice::Pause);
                }
            }
        });
    });
    choice
}

/// The title of the recording control's window (how it's found to be kept out of captures).
const CONTROL_TITLE: &str = "ScreenGrabCraft recording";

/// Whether the recording control can be on the screen: always on Windows, which keeps it out of
/// the recording; elsewhere when it has a place off the area (else it waits in the tray until
/// it's asked for).
pub fn controls_fit(svc: &Service) -> bool {
    if cfg!(target_os = "windows") {
        return svc.recording.is_some();
    }
    let Some(rec) = &svc.recording else { return false };
    let monitors: Vec<(IRect, f32)> =
        screengrabcraft_capture::monitors().map(|ms| ms.into_iter().map(|m| (m.rect, m.scale)).collect()).unwrap_or_default();
    control_place(rec.desktop, &monitors, recording_size(rec.kind)).is_some()
}

pub fn recording(svc: &mut Service, ctx: &egui::Context) {
    let Some(rec) = &svc.recording else { return };
    if svc.controls_hidden {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        return;
    }
    let (kind, d): (Kind, IRect) = (rec.kind, rec.desktop);
    let secs = rec.elapsed().as_secs();
    let progress = rec.progress.load(std::sync::atomic::Ordering::SeqCst);
    let lost = rec.lost.load(std::sync::atomic::Ordering::SeqCst);
    let monitors: Vec<(IRect, f32)> =
        screengrabcraft_capture::monitors().map(|ms| ms.into_iter().map(|m| (m.rect, m.scale)).collect()).unwrap_or_default();
    let level = |i: usize| rec.levels.get(i).map(|l| f32::from_bits(l.load(std::sync::atomic::Ordering::Relaxed))).unwrap_or(0.0);
    let sources = rec.sources.lock().map(|s| s.clone()).unwrap_or_default();
    let has = |prefix: &str| sources.iter().any(|s| s.starts_with(prefix));
    let keys = &svc.settings.hotkeys;
    let view = RecordingView {
        secs,
        progress,
        lost,
        saving: rec.saving.load(std::sync::atomic::Ordering::SeqCst),
        stopping: rec.stopping(),
        paused: rec.is_paused(),
        border: svc.settings.recording_border,
        sound: has("Computer sound").then(|| level(0)),
        mic: has("Microphone:").then(|| level(1)),
        missing: sources.iter().filter(|s| s.starts_with("No ")).cloned().collect(),
        pause_key: crate::hotkeys::Registered::recording_label(keys, crate::hotkeys::Target::Pause),
        stop_key: crate::hotkeys::Registered::recording_label(keys, crate::hotkeys::Target::Stop),
    };
    let size = recording_size(kind);
    // Where the owner dragged it; else beside the area, else on a screen that isn't recorded,
    // else (asked for from the tray) at the bottom of the area's screen. Windows keeps it out of
    // the recording wherever it is.
    let at = match svc.controls_at {
        Some(p) => p,
        None => {
            let place = control_place(d, &monitors, size)
                .or_else(|| (view.saving > 0).then_some((d.x as f32 + 10.0, d.y as f32 + 10.0, 1.0)))
                .or_else(|| bottom_of(d, &monitors, size));
            let Some((x, y, s)) = place else {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
                return;
            };
            points(x, y, s)
        }
    };
    let mut choice = None;
    // Where its window is (points), the points' size in pixels, and whether the button is down.
    let mut seen: Option<(egui::Pos2, f32, bool)> = None;
    ctx.show_viewport_immediate(ViewportId::from_hash_of("recording"), popup(CONTROL_TITLE, at, size, false), |ui, _| {
        egui::CentralPanel::default().frame(widgets::popup_frame(ui.ctx()).inner_margin(egui::Margin::symmetric(12, 8))).show(ui, |ui| {
            choice = recording_ui(ui, kind, &view);
        });
        let ppp = ui.ctx().pixels_per_point();
        let (outer, down) = ui.ctx().input(|i| (i.viewport().outer_rect.map(|r| r.min), i.pointer.primary_down()));
        seen = outer.map(|o| (o, ppp, down));
        // Elsewhere the system moves it (on Windows its move loop would hold up the service, the
        // tray and the shortcuts, for as long as it runs: there it follows the pointer, below).
        if choice == Some(RecordingChoice::Drag) && !cfg!(target_os = "windows") {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    });
    // Out of every capture, this one's included (Windows 10 2004 and later; elsewhere it's kept
    // off the area instead).
    screengrabcraft_desktop::cloak::keep_out_of_captures(CONTROL_TITLE);
    if let Some((outer, ppp, down)) = seen {
        if choice == Some(RecordingChoice::Drag) {
            // Where on it the pointer took hold (pixels), and from now on it stays where it's put.
            svc.controls_drag = screengrabcraft_desktop::cloak::cursor_position().map(|(x, y)| (x as f32 - outer.x * ppp, y as f32 - outer.y * ppp));
            svc.controls_at = Some(outer);
        }
        match svc.controls_drag {
            // Dragged: it follows the pointer on the desktop, nothing else moving it meanwhile.
            Some((gx, gy)) if down => {
                if let Some((x, y)) = screengrabcraft_desktop::cloak::cursor_position() {
                    svc.controls_at = Some(pos2((x as f32 - gx) / ppp.max(0.5), (y as f32 - gy) / ppp.max(0.5)));
                }
                ctx.request_repaint();
            }
            Some(_) => svc.controls_drag = None,
            None => {}
        }
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    match choice {
        Some(RecordingChoice::Stop) => svc.stop_recording(false),
        Some(RecordingChoice::Discard) => svc.stop_recording(true),
        Some(RecordingChoice::Pause) => svc.toggle_pause(),
        Some(RecordingChoice::Restart) => svc.restart_recording(),
        Some(RecordingChoice::Border) => svc.toggle_border(),
        Some(RecordingChoice::Hide) => svc.controls_hidden = true,
        Some(RecordingChoice::Drag) | None => {}
    }
}

/// The bottom middle of the screen the area is on (the area's middle), for controls asked for
/// when there's no room off the area.
fn bottom_of(area: IRect, monitors: &[(IRect, f32)], size: Vec2) -> Option<(f32, f32, f32)> {
    let (cx, cy) = (i64::from(area.x) + i64::from(area.w / 2), i64::from(area.y) + i64::from(area.h / 2));
    let (m, s) = monitors.iter().find(|(m, _)| m.contains(cx, cy)).or_else(|| monitors.first()).copied()?;
    let x = m.x as f32 + (m.w as f32 - size.x * s) / 2.0;
    let y = m.bottom() as f32 - size.y * s - 80.0 * s;
    Some((x, y.max(m.y as f32), s))
}

/// How thick the outline round an area being recorded is, in desktop pixels.
const FRAME: i32 = 4;
/// Its one colour, a bright red.
const FRAME_RGB: [u8; 3] = [0xFF, 0x3B, 0x30];
/// Its windows' title (how they're found to be made plain).
const FRAME_TITLE: &str = "ScreenGrabCraft recording area";

/// The area being recorded outlined for as long as it's recorded: four thin strips just outside
/// it (so in none of the video), on top of everything, that the pointer goes through. A whole
/// screen isn't outlined, nor anything once the video is being made. The strips are made hidden
/// and shown on the next frame, without Windows' animation.
pub fn recording_frame(svc: &mut Service, ctx: &egui::Context) {
    // None while there's no recording, or the owner turned the border off.
    let Some(rec) = svc.recording.as_ref().filter(|_| svc.settings.recording_border) else {
        svc.frame_frames = 0;
        return;
    };
    if rec.saving.load(std::sync::atomic::Ordering::SeqCst) > 0 {
        return;
    }
    let d = rec.desktop;
    let monitors: Vec<(IRect, f32)> =
        screengrabcraft_capture::monitors().map(|ms| ms.into_iter().map(|m| (m.rect, m.scale)).collect()).unwrap_or_default();
    if monitors.iter().any(|(m, _)| *m == d) || d.w == 0 || d.h == 0 {
        return;
    }
    let (cx, cy) = (d.x.saturating_add((d.w / 2) as i32), d.y.saturating_add((d.h / 2) as i32));
    let s = monitors.iter().find(|(m, _)| cx >= m.x && cy >= m.y && cx < m.x + m.w as i32 && cy < m.y + m.h as i32).map_or(1.0, |(_, s)| *s);
    let (w, h) = (d.w as i32, d.h as i32);
    let strips = [
        (d.x - FRAME, d.y - FRAME, w + 2 * FRAME, FRAME),
        (d.x - FRAME, d.y + h, w + 2 * FRAME, FRAME),
        (d.x - FRAME, d.y, FRAME, h),
        (d.x + w, d.y, FRAME, h),
    ];
    let shown = svc.frame_frames > 0;
    let red = egui::Color32::from_rgb(FRAME_RGB[0], FRAME_RGB[1], FRAME_RGB[2]);
    for (i, (x, y, sw, sh)) in strips.into_iter().enumerate() {
        let size = vec2(sw.max(1) as f32 / s.max(0.5), sh.max(1) as f32 / s.max(0.5));
        let builder = ViewportBuilder::default()
            .with_title(FRAME_TITLE)
            .with_app_id(APP_ID)
            .with_decorations(false)
            .with_resizable(false)
            .with_taskbar(false)
            .with_window_level(egui::WindowLevel::AlwaysOnTop)
            .with_active(false)
            .with_mouse_passthrough(true)
            .with_min_inner_size([1.0, 1.0])
            .with_position(points(x as f32, y as f32, s))
            .with_inner_size(size)
            .with_visible(shown);
        ctx.show_viewport_immediate(ViewportId::from_hash_of(("recording-frame", i)), builder, |ui, _| {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(red)).show(ui, |_| {});
        });
    }
    screengrabcraft_desktop::cloak::calm_windows();
    // One colour all round: no shadow, border or rounded corners of Windows' own on the strips.
    screengrabcraft_desktop::cloak::plain_windows(FRAME_TITLE, FRAME_RGB);
    svc.frame_frames = svc.frame_frames.saturating_add(1);
    if !shown {
        ctx.request_repaint();
    }
}

/// Where the recording control goes (desktop pixels, and that monitor's scale) so it isn't in
/// what's recorded: below the area, else above it, on the area's monitor; else at the bottom of
/// a monitor the area doesn't touch. None when every place is in the recording.
fn control_place(area: IRect, monitors: &[(IRect, f32)], size: Vec2) -> Option<(f32, f32, f32)> {
    let overlaps =
        |r: IRect| i64::from(r.x) < area.right() && i64::from(area.x) < r.right() && i64::from(r.y) < area.bottom() && i64::from(area.y) < r.bottom();
    let (w, h) = (|s: f32| size.x * s, |s: f32| size.y * s);
    let home = monitors.iter().find(|(m, _)| m.contains(i64::from(area.x), i64::from(area.y))).copied();
    if let Some((m, s)) = home {
        let x = (area.x as f32).clamp(m.x as f32, (m.right() as f32 - w(s)).max(m.x as f32));
        let below = area.bottom() as f32 + 10.0;
        if below + h(s) <= m.bottom() as f32 {
            return Some((x, below, s));
        }
        let above = area.y as f32 - 10.0 - h(s);
        if above >= m.y as f32 {
            return Some((x, above, s));
        }
    }
    monitors.iter().find(|(m, _)| !overlaps(*m)).map(|(m, s)| {
        let x = m.x as f32 + (m.w as f32 - w(*s)) / 2.0;
        let y = m.bottom() as f32 - h(*s) - 60.0 * s;
        (x, y.max(m.y as f32), *s)
    })
}

// ---- the notice after a capture ----

/// An MP4: it plays in the system's own player.
fn is_video(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mp4"))
}

/// The button that takes a saved file on: a GIF plays in the editor and a video in the system's
/// player, a picture is edited, and a PDF doesn't open.
fn editor_button(path: &std::path::Path) -> Option<&'static str> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    if ext.eq_ignore_ascii_case("gif") || is_video(path) {
        Some("Play")
    } else if ext.eq_ignore_ascii_case("pdf") {
        None
    } else {
        Some("Edit")
    }
}

/// What the notice after a capture shows.
struct NoticeView<'a> {
    text: &'a str,
    detail: &'a str,
    path: Option<&'a Path>,
    /// The capture, small, and its size.
    thumb: Option<(egui::TextureId, Vec2)>,
    error: bool,
}

/// What the notice's buttons do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoticeChoice {
    /// Edit it, or play it (a GIF).
    Open,
    Reveal,
    CopyPath,
    Close,
}

/// The notice's buttons, left to right on Windows, and what each does: the editor's (the default
/// one), Show in folder and Copy path. A PDF, which doesn't open in the editor, has Show in folder
/// as its default.
fn notice_buttons(path: Option<&Path>) -> Vec<(FooterButton<'static>, NoticeChoice)> {
    let Some(p) = path else { return Vec::new() };
    let mut buttons = Vec::with_capacity(3);
    match editor_button(p) {
        Some(label) => {
            buttons.push((FooterButton::primary(label), NoticeChoice::Open));
            buttons.push((FooterButton::plain("Show in folder"), NoticeChoice::Reveal));
        }
        None => buttons.push((FooterButton::primary("Show in folder"), NoticeChoice::Reveal)),
    }
    buttons.push((FooterButton::plain("Copy path"), NoticeChoice::CopyPath));
    buttons
}

fn notice_size(n: &NoticeView<'_>) -> Vec2 {
    vec2(400.0, if n.path.is_some() { 134.0 } else { 92.0 })
}

/// The notice's face: the capture, what became of it, an X at the top right, and its buttons at
/// the bottom right.
fn notice_ui(ui: &mut Ui, n: &NoticeView<'_>) -> Option<NoticeChoice> {
    let t = Tokens::get(ui.ctx());
    let area = ui.max_rect();
    let mut choice = None;
    let x = egui::Rect::from_min_size(pos2(area.max.x - 22.0, area.min.y - 6.0), vec2(28.0, 26.0));
    if widgets::close_button(ui, x).clicked() {
        choice = Some(NoticeChoice::Close);
    }
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        if let Some((id, s)) = n.thumb {
            let f = (96.0 / s.x.max(1.0)).min(64.0 / s.y.max(1.0)).min(1.0);
            let r = ui.add(egui::Image::new((id, s * f))).rect;
            ui.painter().rect_stroke(r, 0.0, egui::Stroke::new(1.0, t.border), egui::StrokeKind::Outside);
        } else if n.error {
            let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 13.0, t.danger);
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "!", widgets::strong_font(ui.ctx(), 17.0), t.panel);
        }
        ui.vertical(|ui| {
            // Clear of the X.
            ui.set_max_width((ui.available_width() - 30.0).max(40.0));
            let title = RichText::new(n.text).font(widgets::strong_font(ui.ctx(), 14.5)).color(if n.error { t.danger } else { t.text });
            ui.add(egui::Label::new(title).truncate());
            if !n.detail.is_empty() {
                ui.add(egui::Label::new(RichText::new(n.detail).size(12.0).color(t.muted)).truncate());
            }
        });
    });
    let buttons = notice_buttons(n.path);
    if !buttons.is_empty() {
        let room = ui.available_height() - widgets::BUTTON_SIZE.y;
        if room > 0.0 {
            ui.add_space(room);
        }
        let row: Vec<FooterButton<'_>> = buttons.iter().map(|(b, _)| *b).collect();
        if let Some(i) = widgets::footer(ui, &row) {
            choice = buttons.get(i).map(|(_, c)| *c);
        }
    }
    choice
}

pub fn notice(svc: &mut Service, ctx: &egui::Context) {
    let Some(n) = svc.notice.as_mut() else { return };
    if n.texture.is_none()
        && let Some(t) = &n.thumb
    {
        let img = egui::ColorImage::from_rgba_unmultiplied([t.width() as usize, t.height() as usize], t.pixels());
        n.texture = Some(ctx.load_texture("notice-thumb", img, egui::TextureOptions::LINEAR));
    }
    let (text, detail, path, tex, error) =
        (n.text.clone(), n.detail.clone(), n.path.clone(), n.texture.as_ref().map(|t| (t.id(), t.size_vec2())), n.error);
    let view = NoticeView { text: &text, detail: &detail, path: path.as_deref(), thumb: tex, error };
    let size = notice_size(&view);
    let at = screengrabcraft_capture::monitors()
        .ok()
        .and_then(|ms| ms.into_iter().next())
        .map(|m| points(m.rect.right() as f32 - 24.0 * m.scale, m.rect.bottom() as f32 - 72.0 * m.scale, m.scale) - size)
        .unwrap_or(pos2(40.0, 40.0));
    let mut choice = None;
    let mut hold = false;
    ctx.show_viewport_immediate(ViewportId::from_hash_of("notice"), popup("ScreenGrabCraft", at, size, false), |ui, _| {
        egui::CentralPanel::default().frame(widgets::popup_frame(ui.ctx())).show(ui, |ui| {
            hold = ui.ui_contains_pointer();
            choice = notice_ui(ui, &view);
        });
    });
    let close = match (choice, &path) {
        (Some(NoticeChoice::Open), Some(p)) => {
            let _ = if is_video(p) { launch::open_file(p) } else { launch::open_editor(&[p.to_string_lossy().to_string()]) };
            true
        }
        (Some(NoticeChoice::Reveal), Some(p)) => {
            let _ = launch::reveal(p);
            true
        }
        (Some(NoticeChoice::CopyPath), Some(p)) => {
            let _ = screengrabcraft_desktop::clipboard::copy_text(&p.to_string_lossy());
            false
        }
        (Some(NoticeChoice::Close), _) => true,
        _ => false,
    };
    if close {
        svc.notice = None;
    } else if hold && let Some(n) = svc.notice.as_mut() {
        // Kept while the pointer is on it.
        n.until = n.until.max(Instant::now() + std::time::Duration::from_secs(2));
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;
    use screengrabcraft_model::Theme;

    use super::*;

    #[test]
    fn the_recording_control_stays_out_of_the_recording() {
        let size = vec2(520.0, 54.0);
        let left = IRect { x: 0, y: 0, w: 3840, h: 2160 };
        let right = IRect { x: 3840, y: 0, w: 1920, h: 1080 };
        let screens = [(left, 1.5), (right, 1.0)];
        let outside = |(x, y, s): (f32, f32, f32), area: IRect| {
            let (r, b) = (x + size.x * s, y + size.y * s);
            r <= area.x as f32 || x >= area.right() as f32 || b <= area.y as f32 || y >= area.bottom() as f32
        };
        // A region: just below it.
        let region = IRect { x: 100, y: 100, w: 800, h: 600 };
        let p = control_place(region, &screens, size).unwrap();
        assert_eq!((p.0, p.1), (100.0, 710.0));
        // A region at the bottom of its screen: above it.
        let low = IRect { x: 100, y: 1800, w: 800, h: 360 };
        let p = control_place(low, &screens, size).unwrap();
        assert!(outside(p, low) && p.1 < 1800.0);
        // The whole left screen: on the right one.
        let p = control_place(left, &screens, size).unwrap();
        assert!(p.0 >= 3840.0 && outside(p, left));
        // Every screen: nowhere (the tray icon stops it).
        let all = IRect { x: 0, y: 0, w: 5760, h: 2160 };
        assert_eq!(control_place(all, &screens, size), None);
        assert_eq!(control_place(left, &[(left, 1.0)], size), None);
    }

    #[test]
    fn a_gif_is_played_a_picture_is_edited_and_a_pdf_stays_out() {
        assert_eq!(editor_button(Path::new("Recording.gif")), Some("Play"));
        assert_eq!(editor_button(Path::new("RECORDING.GIF")), Some("Play"));
        assert_eq!(editor_button(Path::new("Capture.png")), Some("Edit"));
        assert_eq!(editor_button(Path::new("Capture.pdf")), None);
        assert_eq!(editor_button(Path::new("Video.MP4")), Some("Play"));
        assert!(is_video(Path::new("Video.mp4")) && !is_video(Path::new("Capture.png")));
    }

    /// Where the harness puts its contents.
    const ROOT: f32 = 8.0;

    /// A popup's face drawn headless the way the service draws it, and what was pressed.
    fn popup_harness<T: 'static>(size: Vec2, theme: Theme, mut face: impl FnMut(&mut Ui) -> Option<T> + 'static) -> Harness<'static, Option<T>> {
        Harness::builder().with_size(size + vec2(2.0 * ROOT, 2.0 * ROOT)).build_ui_state(
            move |ui, pressed: &mut Option<T>| {
                screengrabcraft_ui::theme::apply(ui.ctx(), theme);
                egui::CentralPanel::default().frame(widgets::popup_frame(ui.ctx())).show(ui, |ui| {
                    if let Some(p) = face(ui) {
                        *pressed = Some(p);
                    }
                });
            },
            None,
        )
    }

    fn notice_harness(path: Option<&'static str>) -> Harness<'static, Option<NoticeChoice>> {
        let view = move || NoticeView { text: "Saved and copied", detail: "Capture.png", path: path.map(Path::new), thumb: None, error: false };
        popup_harness(notice_size(&view()), Theme::Light, move |ui| notice_ui(ui, &view()))
    }

    #[test]
    fn the_notice_has_its_actions_at_the_bottom_right_and_an_x_to_close_it() {
        let mut h = notice_harness(Some("Capture.png"));
        h.run_steps(2);
        let (edit, show, copy, x) = (
            h.get_by_label("Edit").rect(),
            h.get_by_label("Show in folder").rect(),
            h.get_by_label("Copy path").rect(),
            h.get_by_label("Close window").rect(),
        );
        let size = notice_size(&NoticeView { text: "", detail: "", path: Some(Path::new("a.png")), thumb: None, error: false });
        // The last of the row ends at the right, as far in as the X.
        let last = if cfg!(target_os = "macos") { edit } else { copy };
        assert!(size.x + ROOT - last.max.x < 16.0 && size.y + ROOT - last.max.y < 16.0, "{last:?}");
        assert!(x.min.y - ROOT < 12.0 && size.x + ROOT - x.max.x < 12.0, "the X at the top right: {x:?}");
        if cfg!(target_os = "macos") {
            assert!(show.max.x < copy.min.x && copy.max.x < edit.min.x);
        } else {
            assert!(edit.max.x < show.min.x && show.max.x < copy.min.x);
        }
        for r in [edit, show, copy] {
            assert!(r.width() >= widgets::BUTTON_SIZE.x && r.height() >= widgets::BUTTON_SIZE.y - 0.5, "{r:?}");
        }
        h.get_by_label("Close window").click();
        h.run_steps(1);
        assert_eq!(*h.state(), Some(NoticeChoice::Close));
    }

    #[test]
    fn the_notice_buttons_say_what_they_do() {
        for (label, wanted) in [("Edit", NoticeChoice::Open), ("Show in folder", NoticeChoice::Reveal), ("Copy path", NoticeChoice::CopyPath)] {
            let mut h = notice_harness(Some("Capture.png"));
            h.run_steps(2);
            h.get_by_label(label).click();
            h.run_steps(1);
            assert_eq!(*h.state(), Some(wanted), "{label}");
        }
        // A PDF isn't opened in the editor; an error has nothing to do but close.
        let mut h = notice_harness(Some("Capture.pdf"));
        h.run_steps(2);
        assert!(h.query_by_label("Edit").is_none());
        h.get_by_label("Show in folder");
        let mut h = notice_harness(None);
        h.run_steps(2);
        assert!(h.query_by_label("Show in folder").is_none());
        h.get_by_label("Close window");
    }

    #[test]
    fn the_recording_control_stops_and_discards_from_the_right() {
        for kind in [Kind::Gif, Kind::Scrolling, Kind::Video, Kind::Production] {
            let stop = if kind == Kind::Scrolling { "Done" } else { "Stop" };
            for (label, wanted) in [(stop, RecordingChoice::Stop), ("Discard", RecordingChoice::Discard)] {
                let view = RecordingView { secs: 7, progress: 212, ..RecordingView::default() };
                let mut h = popup_harness(recording_size(kind), Theme::Dark, move |ui| recording_ui(ui, kind, &view));
                h.run_steps(2);
                let r = h.get_by_label(label).rect();
                assert!(r.max.x <= recording_size(kind).x + ROOT && r.min.x > recording_size(kind).x * 0.4, "{label} {r:?}");
                h.get_by_label(label).click();
                h.run_steps(1);
                assert_eq!(*h.state(), Some(wanted), "{label}");
            }
        }
    }

    #[test]
    fn once_stop_is_pressed_the_control_says_so_and_only_discard_is_left() {
        for kind in [Kind::Gif, Kind::Scrolling, Kind::Video, Kind::Production] {
            let view = RecordingView { secs: 14, stopping: true, ..RecordingView::default() };
            let mut h = popup_harness(recording_size(kind), Theme::Light, move |ui| recording_ui(ui, kind, &view));
            h.run_steps(2);
            h.get_by_label_contains("Stopping… (00:14 recorded)");
            assert!(h.query_by_label("Stop").is_none() && h.query_by_label("Done").is_none(), "{kind:?}");
            h.get_by_label("Discard").click();
            h.run_steps(1);
            assert_eq!(*h.state(), Some(RecordingChoice::Discard), "{kind:?}");
        }
        // Making the video after: its progress, not "Stopping".
        let view = RecordingView { stopping: true, saving: 501, ..RecordingView::default() };
        let mut h = popup_harness(recording_size(Kind::Video), Theme::Light, move |ui| recording_ui(ui, Kind::Video, &view));
        h.run_steps(2);
        h.get_by_label("Making the video…");
        assert!(h.query_by_label_contains("Stopping").is_none());
    }

    #[test]
    fn the_recording_control_pauses_restarts_hides_and_turns_the_border_off() {
        let keys = (Some("F9".to_string()), Some("F10".to_string()));
        for kind in [Kind::Gif, Kind::Video, Kind::Production, Kind::Scrolling] {
            let view = RecordingView {
                secs: 65,
                border: true,
                sound: Some(0.4),
                mic: Some(0.2),
                pause_key: keys.0.clone(),
                stop_key: keys.1.clone(),
                ..RecordingView::default()
            };
            let buttons: Vec<(&str, RecordingChoice)> = vec![
                ("Pause (F9)", RecordingChoice::Pause),
                ("Restart: throw this away and record again", RecordingChoice::Restart),
                ("Hide the red border", RecordingChoice::Border),
                ("Put the controls away in the tray (its icon has them)", RecordingChoice::Hide),
            ];
            for (label, wanted) in buttons {
                let v = view.clone();
                let mut h = popup_harness(recording_size(kind), Theme::Dark, move |ui| recording_ui(ui, kind, &v));
                h.run_steps(2);
                if kind == Kind::Scrolling && wanted == RecordingChoice::Pause {
                    assert!(h.query_by_label(label).is_none(), "a scrolling capture can't be paused");
                    continue;
                }
                let r = h.get_by_label(label).rect();
                assert!(r.max.x <= recording_size(kind).x + ROOT, "{kind:?} {label} {r:?}");
                h.get_by_label(label).click();
                h.run_steps(1);
                assert_eq!(*h.state(), Some(wanted), "{kind:?} {label}");
            }
        }
        // Paused: it says so, and the button goes on with it.
        let view = RecordingView { secs: 65, paused: true, pause_key: keys.0.clone(), ..RecordingView::default() };
        let mut h = popup_harness(recording_size(Kind::Video), Theme::Dark, move |ui| recording_ui(ui, Kind::Video, &view));
        h.run_steps(2);
        h.get_by_label("Paused");
        h.get_by_label("Resume (F9)").click();
        h.run_steps(1);
        assert_eq!(*h.state(), Some(RecordingChoice::Pause));
        if let Ok(dir) = std::env::var("SGC_SHOTS") {
            for (name, paused) in [("recording-bar", false), ("recording-bar-paused", true)] {
                let view = RecordingView {
                    secs: 65,
                    paused,
                    border: true,
                    sound: Some(0.5),
                    mic: Some(0.3),
                    pause_key: keys.0.clone(),
                    stop_key: keys.1.clone(),
                    ..RecordingView::default()
                };
                let mut h = popup_harness(recording_size(Kind::Production), Theme::Dark, move |ui| recording_ui(ui, Kind::Production, &view));
                h.run_steps(2);
                if let Ok(img) = h.render() {
                    let _ = img.save(format!("{dir}/{name}.png"));
                }
            }
        }
    }

    #[test]
    fn the_recording_menu_pauses_stops_restarts_and_says_whats_going_on() {
        use crate::tray::{Item, RecordingEntry as E, RecordingState, recording_entries};
        let s = RecordingState {
            can_pause: true,
            controls: true,
            border: true,
            pause_key: Some("F9".into()),
            stop_key: Some("F10".into()),
            ..RecordingState::default()
        };
        let m = recording_entries(&s);
        assert_eq!(m.first(), Some(&E::Heading("Recording…".to_string())));
        let item = |m: &[E], what: Item| {
            m.iter().find_map(|e| match e {
                E::Item(t, i) | E::Check(t, i, _) if *i == what => Some(t.clone()),
                _ => None,
            })
        };
        assert!(item(&m, Item::PauseRecording).unwrap().starts_with("Pause Recording") && item(&m, Item::PauseRecording).unwrap().contains("F9"));
        assert!(item(&m, Item::StopRecording).unwrap().contains("F10"));
        assert!(item(&m, Item::RestartRecording).is_some() && item(&m, Item::DiscardRecording).is_some());
        assert_eq!(item(&m, Item::ToggleControls).as_deref(), Some("Hide the Controls in the Tray"));
        assert!(m.contains(&E::Check("Red Border Round the Area".to_string(), Item::ToggleBorder, true)));
        // Discard last, set apart.
        assert_eq!(m.last(), Some(&E::Item("Discard Recording".to_string(), Item::DiscardRecording)));
        assert_eq!(m.get(m.len() - 2), Some(&E::Separator));
        // Paused, controls put away, border off; a scrolling capture can't be paused.
        let paused = recording_entries(&RecordingState { paused: true, controls: false, border: false, ..s.clone() });
        assert_eq!(paused.first(), Some(&E::Heading("Recording paused".to_string())));
        assert!(item(&paused, Item::PauseRecording).unwrap().starts_with("Resume Recording"));
        assert_eq!(item(&paused, Item::ToggleControls).as_deref(), Some("Show the Controls"));
        assert!(paused.contains(&E::Check("Red Border Round the Area".to_string(), Item::ToggleBorder, false)));
        assert!(item(&recording_entries(&RecordingState { can_pause: false, ..s }), Item::PauseRecording).is_none());
    }

    #[test]
    fn the_countdown_can_be_cancelled() {
        let mut h = popup_harness(COUNTDOWN_SIZE, Theme::Light, |ui| countdown_ui(ui, 3).then_some(()));
        h.run_steps(2);
        let r = h.get_by_label("Cancel").rect();
        assert!(r.max.y <= COUNTDOWN_SIZE.y + ROOT && r.width() >= widgets::BUTTON_SIZE.x, "{r:?}");
        h.get_by_label("Cancel").click();
        h.run_steps(1);
        assert_eq!(*h.state(), Some(()));
    }
}
