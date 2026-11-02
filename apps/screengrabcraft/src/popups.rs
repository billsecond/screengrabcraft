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
    Stop,
    Cancel,
}

fn recording_size(kind: Kind) -> Vec2 {
    let w = match kind {
        Kind::Scrolling => 470.0,
        Kind::Video | Kind::Production => 520.0,
        Kind::Gif => 390.0,
    };
    vec2(w, 54.0)
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
    /// The computer's sound and the microphone, if they're recorded, and how loud they are.
    sound: Option<f32>,
    mic: Option<f32>,
    /// What was asked for and couldn't be had (`No camera: …`).
    missing: Vec<String>,
}

/// A red light that blinks every second, and the time.
fn red_light(ui: &mut Ui, secs: u64, text: egui::Color32) {
    let (dot, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
    let red = if secs.is_multiple_of(2) { egui::Color32::from_rgb(0xE5, 0x39, 0x35) } else { egui::Color32::from_rgb(0x99, 0x22, 0x22) };
    ui.painter().circle_filled(dot.center(), 6.0, red);
    ui.label(RichText::new(format!("{:02}:{:02}", secs / 60, secs % 60)).monospace().size(16.0).color(text));
}

/// The recording control's face: how far it has got, then [Stop] [Cancel] at the right.
fn recording_ui(ui: &mut Ui, kind: Kind, v: &RecordingView) -> Option<RecordingChoice> {
    let t = Tokens::get(ui.ctx());
    let mut choice = None;
    let making = matches!(kind, Kind::Video | Kind::Production) && v.saving > 0;
    ui.horizontal_centered(|ui| {
        match kind {
            Kind::Gif => {
                red_light(ui, v.secs, t.text);
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
                red_light(ui, v.secs, t.text);
                // A meter for each sound, so the owner sees it's being heard.
                for (name, level) in [("Sound", v.sound), ("Mic", v.mic)] {
                    if let Some(l) = level {
                        ui.label(RichText::new(name).size(12.0).color(t.muted));
                        ui.add(egui::ProgressBar::new(l.clamp(0.0, 1.0)).desired_width(44.0).desired_height(6.0));
                    }
                }
                if !v.missing.is_empty() {
                    ui.label(RichText::new("⚠").color(egui::Color32::from_rgb(0xF5, 0x9E, 0x0B))).on_hover_text(v.missing.join("\n"));
                }
            }
        }
        let (stop, tip) = match kind {
            Kind::Scrolling => ("Done", "Stop scrolling and keep the picture"),
            _ => ("Stop", "Stop and save"),
        };
        let buttons: Vec<FooterButton<'_>> =
            if making { vec![FooterButton::cancel("Cancel")] } else { vec![FooterButton::primary(stop).tip(tip), FooterButton::cancel("Cancel")] };
        choice = widgets::footer(ui, &buttons).map(|i| if i == 0 && !making { RecordingChoice::Stop } else { RecordingChoice::Cancel });
    });
    choice
}

pub fn recording(svc: &mut Service, ctx: &egui::Context) {
    let Some(rec) = &svc.recording else { return };
    let (kind, d): (Kind, IRect) = (rec.kind, rec.desktop);
    let secs = rec.elapsed().as_secs();
    let progress = rec.progress.load(std::sync::atomic::Ordering::SeqCst);
    let lost = rec.lost.load(std::sync::atomic::Ordering::SeqCst);
    let monitors: Vec<(IRect, f32)> =
        screengrabcraft_capture::monitors().map(|ms| ms.into_iter().map(|m| (m.rect, m.scale)).collect()).unwrap_or_default();
    let level = |i: usize| rec.levels.get(i).map(|l| f32::from_bits(l.load(std::sync::atomic::Ordering::Relaxed))).unwrap_or(0.0);
    let sources = rec.sources.lock().map(|s| s.clone()).unwrap_or_default();
    let has = |prefix: &str| sources.iter().any(|s| s.starts_with(prefix));
    let view = RecordingView {
        secs,
        progress,
        lost,
        saving: rec.saving.load(std::sync::atomic::Ordering::SeqCst),
        sound: has("Computer sound").then(|| level(0)),
        mic: has("Microphone:").then(|| level(1)),
        missing: sources.iter().filter(|s| s.starts_with("No ")).cloned().collect(),
    };
    let size = recording_size(kind);
    // Never in the video: beside the area, else on a screen that isn't recorded, else nowhere
    // (the tray icon stops it). Once it's making the video, the area doesn't matter.
    let place = control_place(d, &monitors, size).or_else(|| (view.saving > 0).then_some((d.x as f32 + 10.0, d.y as f32 + 10.0, 1.0)));
    let Some((x, y, s)) = place else {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        return;
    };
    let at = points(x, y, s);
    let mut choice = None;
    ctx.show_viewport_immediate(ViewportId::from_hash_of("recording"), popup("ScreenGrabCraft recording", at, size, false), |ui, _| {
        egui::CentralPanel::default().frame(widgets::popup_frame(ui.ctx()).inner_margin(egui::Margin::symmetric(14, 8))).show(ui, |ui| {
            choice = recording_ui(ui, kind, &view);
        });
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    match choice {
        Some(RecordingChoice::Stop) => svc.stop_recording(false),
        Some(RecordingChoice::Cancel) => svc.stop_recording(true),
        None => {}
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
    fn the_recording_control_stops_and_cancels_from_the_right() {
        for kind in [Kind::Gif, Kind::Scrolling, Kind::Video, Kind::Production] {
            let stop = if kind == Kind::Scrolling { "Done" } else { "Stop" };
            for (label, wanted) in [(stop, RecordingChoice::Stop), ("Cancel", RecordingChoice::Cancel)] {
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
