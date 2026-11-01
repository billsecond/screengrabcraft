//! Playing a GIF: a Play button over the canvas while an animated GIF is open.
//!
//! The document is the GIF's first frame. The frames only take its place on the screen, and any
//! change to the document goes back to it, so the file itself is never touched.

use std::time::Duration;

use egui::{Align2, Color32, CursorIcon, FontId, Rect, Sense, Stroke, TextureOptions, pos2, vec2};
use screengrabcraft_render::anim::Player;

use crate::editor::Editor;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

const BUTTON: egui::Vec2 = vec2(104.0, 38.0);
/// How far the bar stays above the canvas's lower edge.
const LIFT: f32 = 14.0;

pub struct Playback {
    player: Player,
    /// The frame on the screen in place of the picture, while there is one.
    frame: Option<egui::TextureHandle>,
    playing: bool,
    /// When the next frame is due, on the context's clock.
    due: f64,
    /// The document's revision the frames were started on.
    revision: u64,
}

impl Playback {
    pub(crate) fn new(player: Player, revision: u64) -> Playback {
        Playback { player, frame: None, playing: false, due: 0.0, revision }
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    /// How far it has played.
    pub fn elapsed_ms(&self) -> u64 {
        self.player.elapsed_ms()
    }

    /// What is on the canvas in place of the picture.
    pub(crate) fn frame(&self) -> Option<egui::TextureId> {
        self.frame.as_ref().map(egui::TextureHandle::id)
    }

    /// Back to the picture, ready to play from the start.
    fn stop(&mut self) {
        if self.playing || self.frame.is_some() {
            self.playing = false;
            self.frame = None;
            self.due = 0.0;
            self.player.rewind();
        }
    }

    /// Puts up the next frame once it is due, and asks for a repaint when the one after is.
    fn advance(&mut self, ctx: &egui::Context) {
        if !self.playing {
            return;
        }
        let now = ctx.input(|i| i.time);
        if now >= self.due {
            let Some(next) = self.player.next_frame() else {
                self.stop();
                return;
            };
            let size = [next.picture.width() as usize, next.picture.height() as usize];
            let image = egui::ColorImage::from_rgba_unmultiplied(size, next.picture.pixels());
            let options = TextureOptions {
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Linear,
                ..TextureOptions::default()
            };
            match &mut self.frame {
                Some(frame) => frame.set(image, options),
                None => self.frame = Some(ctx.load_texture("gif-frame", image, options)),
            }
            // A slow frame makes the next one later rather than rushing to catch up.
            self.due = now + f64::from(next.delay_ms) / 1000.0;
        }
        ctx.request_repaint_after(Duration::from_millis(((self.due - now) * 1000.0).max(0.0) as u64));
    }

    fn bar(&mut self, ctx: &egui::Context, canvas: Rect) {
        let t = Tokens::get(ctx);
        let (icon, label) = if self.playing { (Icon::Pause, "Pause") } else { (Icon::Play, "Play") };
        let time = format!("{:.1} / {:.1} s", self.player.elapsed_ms() as f64 / 1000.0, self.player.total_ms() as f64 / 1000.0);
        let mut pressed = false;
        egui::Area::new(egui::Id::new("gif-play"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(pos2(canvas.center().x, canvas.max.y - LIFT))
            .show(ctx, |ui| {
                egui::Frame::NONE.fill(t.panel).stroke(Stroke::new(1.0, t.border)).corner_radius(10).inner_margin(egui::Margin::same(6)).show(
                    ui,
                    |ui| {
                        ui.horizontal(|ui| {
                            pressed = button(ui, icon, label).clicked();
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(time).monospace().color(t.muted));
                            ui.add_space(4.0);
                        });
                    },
                );
            });
        if pressed {
            self.playing = !self.playing;
        }
    }
}

fn button(ui: &mut egui::Ui, icon: Icon, label: &'static str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(BUTTON, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let fill = if resp.hovered() { t.accent.gamma_multiply(0.85) } else { t.accent };
    let p = ui.painter();
    p.rect_filled(rect, 8.0, fill);
    let at = Rect::from_center_size(pos2(rect.min.x + 22.0, rect.center().y), vec2(20.0, 20.0));
    icons::paint(p, at, icon, t.accent_text);
    p.text(pos2(rect.min.x + 40.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(16.0), t.accent_text);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Shows the bar and moves the frames on, while there is a GIF that still matches the document.
pub fn show(ed: &mut Editor, ctx: &egui::Context, canvas: Rect) {
    let revision = ed.session.revision();
    let size = (ed.session.doc.width(), ed.session.doc.height());
    let Some(p) = ed.playback.as_mut() else { return };
    let side = ctx.input(|i| i.max_texture_side);
    // Cropped or resized, it's no longer the GIF's picture; too big for a texture, it can't be shown.
    if size != p.player.size() || size.0.max(size.1) as usize > side {
        p.stop();
        return;
    }
    if revision != p.revision {
        p.stop();
        p.revision = revision;
    }
    p.bar(ctx, canvas);
    p.advance(ctx);
}

/// The frame on the canvas over the picture, if one is up.
pub fn paint(ed: &Editor, painter: &egui::Painter, picture: Rect) {
    if let Some(frame) = ed.playback.as_ref().and_then(Playback::frame) {
        painter.image(frame, picture, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
}
