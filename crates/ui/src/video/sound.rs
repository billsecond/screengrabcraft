//! The video editor's sound mix: a sound track's volume line (points on its clips' waves,
//! dragged up and down), the rest lowered while the voice is heard, and levels evened out.

use egui::{Color32, Pos2, Rect, RichText, Stroke, Ui, pos2, vec2};
use screengrabcraft_media::recording::StreamKind;
use screengrabcraft_media::timeline::{Clip, Track};
use serde_json::json;

use super::VideoView;
use crate::editor::Editor;
use crate::theme::Tokens;

/// How near (points) the pointer must be to a volume point to take hold of it.
pub(super) const POINT_REACH: f32 = 7.0;

/// The height in a clip's box `r` of a level (0 at its bottom, 1 in its middle, 2 at its top).
pub(super) fn level_y(r: Rect, level: f32) -> f32 {
    r.max.y - 3.0 - (level / 2.0).clamp(0.0, 1.0) * (r.height() - 6.0)
}

/// The level a height in a clip's box `r` stands for.
pub(super) fn level_of_y(r: Rect, y: f32) -> f32 {
    ((r.max.y - 3.0 - y) / (r.height() - 6.0).max(1.0) * 2.0).clamp(0.0, 2.0)
}

/// A sound clip's volume line over its wave, and its points (each where it's heard: `points`
/// are the track's, with their places on the timeline).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_line(
    painter: &egui::Painter,
    track: &Track,
    clip: &Clip,
    r: Rect,
    lane: Rect,
    points: &[(usize, u64)],
    x_of: &dyn Fn(u64) -> f32,
    us_of: &dyn Fn(f32) -> u64,
) {
    let colour = Color32::from_rgb(0xFF, 0xD5, 0x4F);
    let (x0, x1) = (r.min.x.max(lane.min.x), r.max.x.min(lane.max.x));
    if x1 <= x0 {
        return;
    }
    let level_at_x = |x: f32| track.level_at(clip.source_at(us_of(x).saturating_sub(clip.at).min(clip.len)));
    let mut line = Vec::new();
    let mut x = x0;
    while x < x1 {
        line.push(pos2(x, level_y(r, level_at_x(x))));
        x += 2.0;
    }
    line.push(pos2(x1, level_y(r, level_at_x(x1))));
    painter.add(egui::Shape::line(line.clone(), Stroke::new(3.5, Color32::from_black_alpha(150))));
    painter.add(egui::Shape::line(line, Stroke::new(1.5, colour)));
    for (k, at) in points {
        let px = x_of(*at);
        if px < x0 - 1.0 || px > x1 + 1.0 {
            continue;
        }
        let level = track.points.get(*k).map_or(1.0, |p| p.level);
        let c = pos2(px, level_y(r, level));
        painter.circle_filled(c, 4.0, colour);
        painter.circle_stroke(c, 4.0, Stroke::new(1.0, Color32::BLACK));
    }
}

/// The volume point of a track's clips near `p`, if one is: its place among the track's points.
pub(super) fn point_near(points: &[(usize, u64)], track: &Track, row: Rect, p: Pos2, x_of: &dyn Fn(u64) -> f32) -> Option<usize> {
    let r = Rect::from_min_max(pos2(row.min.x, row.min.y + 3.0), pos2(row.max.x, row.max.y - 3.0));
    points
        .iter()
        .filter_map(|(k, at)| {
            let level = track.points.get(*k)?.level;
            let d = pos2(x_of(*at), level_y(r, level)).distance(p);
            (d <= POINT_REACH).then_some((*k, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(k, _)| k)
}

impl VideoView {
    /// Every sound track's level evened out (or one's).
    pub(super) fn even_out(&mut self, ed: &mut Editor, ctx: &egui::Context, track: Option<usize>) {
        let p = match track {
            Some(t) => json!({"track": t}),
            None => json!({}),
        };
        self.run(ed, ctx, "video.levels", p);
    }

    /// The sound mix: the rest lowered under the voice, and every track's level evened out.
    pub(super) fn mix_section(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        let tl = &self.session.timeline;
        let voice = tl.find(StreamKind::Microphone).is_some();
        let others = tl.tracks.iter().any(|x| !x.kind.is_video() && x.kind != StreamKind::Microphone)
            || tl.layers.iter().any(|l| l.kind == screengrabcraft_media::layers::LayerKind::Sound);
        let duck = tl.ducking;
        if voice {
            let mut on = duck.on;
            let r = ui
                .add_enabled(others, egui::Checkbox::new(&mut on, "Lower the rest under the voice"))
                .on_hover_text("The computer's sound, music and media go down while the microphone is heard, and back up after")
                .on_disabled_hover_text("There's nothing but the voice to lower");
            if r.changed() {
                self.run(ed, &ctx, "video.ducking", json!({"on": on}));
            }
            if duck.on {
                let mut level = duck.level * 100.0;
                let r = super::named_slider(ui, "How loud under the voice", &mut level, 0.0..=100.0, "%");
                if r.drag_started() {
                    self.session.begin("Lower Under the Voice");
                }
                if r.changed() {
                    let p = json!({"level": level / 100.0});
                    if r.dragged() {
                        let _ = self.session.drag("video.ducking", &p);
                    } else {
                        self.run(ed, &ctx, "video.ducking", p);
                    }
                }
                if r.drag_stopped() {
                    self.session.commit();
                }
            }
            ui.add_space(6.0);
        }
        let any = self.session.timeline.tracks.iter().any(|x| !x.kind.is_video());
        if ui
            .add_enabled(any, egui::Button::new("Even Out the Levels").min_size(vec2(ui.available_width(), 28.0)))
            .on_hover_text("Each sound track's volume set so they're all about as loud")
            .clicked()
        {
            self.even_out(ed, &ctx, None);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new("A sound clip's volume line: double-click its wave to put a point there, drag points up and down.")
                .size(12.0)
                .color(t.muted),
        );
    }

    /// A sound clip's volume line, in its panel: how many points, and a way to take them away.
    pub(super) fn line_section(&mut self, ed: &mut Editor, ui: &mut Ui, track: usize) {
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        let n = self.session.timeline.tracks.get(track).map_or(0, |x| x.points.len());
        let words = match n {
            0 => "Double-click its wave to put a volume point there; drag a point up or down, or along.".to_string(),
            1 => "One volume point: the whole track is as loud as it says.".to_string(),
            n => format!("{n} volume points: the sound goes straight from one to the next."),
        };
        ui.label(RichText::new(words).size(12.0).color(t.muted));
        ui.add_space(4.0);
        let w = ((ui.available_width() - 6.0) / 2.0).floor();
        let (mut clear, mut even) = (false, false);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            clear = ui
                .add_enabled(n > 0, egui::Button::new("Clear the Line").min_size(vec2(w, 26.0)))
                .on_hover_text("Take every volume point away: as loud all along")
                .clicked();
            even = ui.add(egui::Button::new("Even Out").min_size(vec2(w, 26.0))).on_hover_text("Set this track's volume to an even level").clicked();
        });
        if clear {
            self.run(ed, &ctx, "video.removeVolumePoint", json!({"track": track, "all": true}));
        }
        if even {
            self.even_out(ed, &ctx, Some(track));
        }
        ui.add_space(6.0);
    }
}
