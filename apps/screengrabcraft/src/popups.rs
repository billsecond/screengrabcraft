//! The service's small windows: the countdown before a delayed capture, the control of a running
//! recording, and the notice after a capture is kept.

use std::time::Instant;

use egui::{Color32, RichText, ViewportBuilder, ViewportId, pos2, vec2};
use screengrabcraft_desktop::launch;
use screengrabcraft_model::IRect;
use screengrabcraft_ui::icons::{self, Icon};

use crate::app::{APP_ID, Service};
use crate::record::Kind;

const DARK: Color32 = Color32::from_rgb(0x1F, 0x1F, 0x25);

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

fn frame() -> egui::Frame {
    egui::Frame::NONE.fill(DARK).inner_margin(egui::Margin::same(12))
}

pub fn countdown(svc: &mut Service, ctx: &egui::Context) {
    let Some(c) = &svc.countdown else { return };
    let left = c.until.saturating_duration_since(Instant::now()).as_secs_f32().ceil() as u32;
    let size = vec2(180.0, 180.0);
    let at = match c.on {
        Some((r, s)) => {
            let center = points(r.x as f32 + r.w as f32 / 2.0, r.y as f32 + r.h as f32 / 2.0, s);
            center - size / 2.0
        }
        None => pos2(200.0, 200.0),
    };
    let mut cancel = false;
    ctx.show_viewport_immediate(ViewportId::from_hash_of("countdown"), popup("ScreenGrabCraft countdown", at, size, false), |ui, _| {
        egui::CentralPanel::default().frame(frame().corner_radius(16)).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new(left.max(1).to_string()).size(84.0).color(Color32::WHITE).strong());
                ui.label(RichText::new("Capturing…").color(Color32::from_gray(200)));
                ui.add_space(4.0);
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    });
    if cancel {
        svc.countdown = None;
    }
}

pub fn recording(svc: &mut Service, ctx: &egui::Context) {
    let Some(rec) = &svc.recording else { return };
    let (kind, d): (Kind, IRect) = (rec.kind, rec.desktop);
    let secs = rec.elapsed().as_secs();
    let progress = rec.progress.load(std::sync::atomic::Ordering::SeqCst);
    let lost = rec.lost.load(std::sync::atomic::Ordering::SeqCst);
    let scale = screengrabcraft_capture::monitors()
        .ok()
        .and_then(|ms| ms.into_iter().find(|m| m.rect.contains(i64::from(d.x), i64::from(d.y))).map(|m| (m.scale, m.rect)))
        .unwrap_or((1.0, d));
    let saving = rec.saving.load(std::sync::atomic::Ordering::SeqCst);
    let level = |i: usize| rec.levels.get(i).map(|l| f32::from_bits(l.load(std::sync::atomic::Ordering::Relaxed))).unwrap_or(0.0);
    let (sound, mic) = (level(0), level(1));
    let sources = rec.sources.lock().map(|s| s.clone()).unwrap_or_default();
    let has = |prefix: &str| sources.iter().any(|s| s.starts_with(prefix));
    let (with_sound, with_mic) = (has("Computer sound"), has("Microphone:"));
    let missing: Vec<String> = sources.iter().filter(|s| s.starts_with("No ")).cloned().collect();
    let video = matches!(kind, Kind::Video | Kind::Production);
    let size = vec2(
        match kind {
            Kind::Scrolling => 420.0,
            Kind::Video | Kind::Production => 460.0,
            Kind::Gif => 300.0,
        },
        52.0,
    );
    // Below the area, else above it, else inside its top.
    let (s, mon) = scale;
    let below = d.bottom() as f32 + 10.0;
    let y = if below + size.y * s < mon.bottom() as f32 {
        below
    } else if d.y as f32 - 10.0 - size.y * s > mon.y as f32 {
        d.y as f32 - 10.0 - size.y * s
    } else {
        d.y as f32 + 10.0
    };
    let at = points(d.x as f32, y, s);
    let mut stop = false;
    let mut cancel = false;
    ctx.show_viewport_immediate(ViewportId::from_hash_of("recording"), popup("ScreenGrabCraft recording", at, size, false), |ui, _| {
        egui::CentralPanel::default().frame(frame().corner_radius(10).inner_margin(egui::Margin::symmetric(12, 8))).show(ui, |ui| {
            ui.style_mut().visuals = egui::Visuals::dark();
            ui.horizontal_centered(|ui| {
                match kind {
                    Kind::Gif => {
                        let blink = (secs % 2 == 0) as u8;
                        ui.label(RichText::new("●").size(18.0).color(if blink == 1 {
                            Color32::from_rgb(0xEF, 0x44, 0x44)
                        } else {
                            Color32::from_rgb(0x99, 0x22, 0x22)
                        }));
                        ui.label(RichText::new(format!("{:02}:{:02}", secs / 60, secs % 60)).monospace().size(16.0).color(Color32::WHITE));
                        ui.label(RichText::new(format!("{progress} frames")).color(Color32::from_gray(170)));
                    }
                    Kind::Scrolling => {
                        let text = if lost > 2 { "Scroll more slowly…".to_string() } else { format!("Scroll the area — {progress} px") };
                        ui.label(RichText::new(text).color(Color32::WHITE));
                    }
                    Kind::Video | Kind::Production if saving > 0 => {
                        let done = saving.saturating_sub(1).min(1000) as f32 / 1000.0;
                        ui.label(RichText::new("Making the video…").color(Color32::WHITE));
                        ui.add(egui::ProgressBar::new(done).desired_width(140.0).show_percentage());
                    }
                    Kind::Video | Kind::Production => {
                        let blink = secs % 2 == 0;
                        let red = if blink { Color32::from_rgb(0xEF, 0x44, 0x44) } else { Color32::from_rgb(0x99, 0x22, 0x22) };
                        ui.label(RichText::new("●").size(18.0).color(red));
                        ui.label(RichText::new(format!("{:02}:{:02}", secs / 60, secs % 60)).monospace().size(16.0).color(Color32::WHITE));
                        // A meter for each sound, so the owner sees it's being heard.
                        for (on, name, v) in [(with_sound, "Sound", sound), (with_mic, "Mic", mic)] {
                            if on {
                                ui.label(RichText::new(name).size(12.0).color(Color32::from_gray(170)));
                                ui.add(egui::ProgressBar::new(v.clamp(0.0, 1.0)).desired_width(44.0).desired_height(6.0));
                            }
                        }
                        if !missing.is_empty() {
                            ui.label(RichText::new("⚠").color(Color32::from_rgb(0xF5, 0x9E, 0x0B))).on_hover_text(missing.join("\n"));
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::button(ui, Icon::Close, "Cancel", false, true).clicked() {
                        cancel = true;
                    }
                    let tip = if kind == Kind::Scrolling { "Done" } else { "Stop and save" };
                    if !(video && saving > 0) && icons::button(ui, Icon::Check, tip, false, true).clicked() {
                        stop = true;
                    }
                });
            });
        });
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    if stop {
        svc.stop_recording(false);
    }
    if cancel {
        svc.stop_recording(true);
    }
}

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

pub fn notice(svc: &mut Service, ctx: &egui::Context) {
    let Some(n) = svc.notice.as_mut() else { return };
    if n.texture.is_none()
        && let Some(t) = &n.thumb
    {
        let img = egui::ColorImage::from_rgba_unmultiplied([t.width() as usize, t.height() as usize], t.pixels());
        n.texture = Some(ctx.load_texture("notice-thumb", img, egui::TextureOptions::LINEAR));
    }
    let size = vec2(380.0, 96.0);
    let at = screengrabcraft_capture::monitors()
        .ok()
        .and_then(|ms| ms.into_iter().next())
        .map(|m| points(m.rect.right() as f32 - 24.0 * m.scale, m.rect.bottom() as f32 - 72.0 * m.scale, m.scale) - size)
        .unwrap_or(pos2(40.0, 40.0));
    let (text, detail, path, tex, error) =
        (n.text.clone(), n.detail.clone(), n.path.clone(), n.texture.as_ref().map(|t| (t.id(), t.size_vec2())), n.error);
    let mut close = false;
    let mut hold = false;
    ctx.show_viewport_immediate(ViewportId::from_hash_of("notice"), popup("ScreenGrabCraft", at, size, false), |ui, _| {
        let fill = if error { Color32::from_rgb(0x5A, 0x1D, 0x22) } else { DARK };
        egui::CentralPanel::default().frame(frame().fill(fill).corner_radius(10)).show(ui, |ui| {
            ui.style_mut().visuals = egui::Visuals::dark();
            hold = ui.ui_contains_pointer();
            ui.horizontal(|ui| {
                if let Some((id, s)) = tex {
                    let f = (96.0 / s.x).min(64.0 / s.y).min(1.0);
                    ui.add(egui::Image::new((id, s * f)));
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new(&text).strong().size(15.0).color(Color32::WHITE));
                    if !detail.is_empty() {
                        ui.add(egui::Label::new(RichText::new(&detail).size(12.0).color(Color32::from_gray(190))).truncate());
                    }
                    ui.horizontal(|ui| {
                        if let Some(p) = &path {
                            if let Some(label) = editor_button(p)
                                && ui.small_button(label).clicked()
                            {
                                let _ = if is_video(p) { launch::open_file(p) } else { launch::open_editor(&[p.to_string_lossy().to_string()]) };
                                close = true;
                            }
                            if ui.small_button("Show in folder").clicked() {
                                let _ = launch::reveal(p);
                                close = true;
                            }
                            if ui.small_button("Copy path").clicked() {
                                let _ = screengrabcraft_desktop::clipboard::copy_text(&p.to_string_lossy());
                            }
                        }
                        if ui.small_button("Close").clicked() {
                            close = true;
                        }
                    });
                });
            });
        });
    });
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

    use super::*;

    #[test]
    fn a_gif_is_played_a_picture_is_edited_and_a_pdf_stays_out() {
        assert_eq!(editor_button(Path::new("Recording.gif")), Some("Play"));
        assert_eq!(editor_button(Path::new("RECORDING.GIF")), Some("Play"));
        assert_eq!(editor_button(Path::new("Capture.png")), Some("Edit"));
        assert_eq!(editor_button(Path::new("Capture.pdf")), None);
        assert_eq!(editor_button(Path::new("Video.MP4")), Some("Play"));
        assert!(is_video(Path::new("Video.mp4")) && !is_video(Path::new("Capture.png")));
    }
}
