//! Transitions between two clips of a track, in the video editor: set from the clip's panel or
//! the right button, or dropped from the Library's Between Clips section where two clips meet.
//! On the timeline each shows as a band across the cut, as long as it plays.

use egui::{Color32, Rect, RichText, Stroke, Ui, pos2, vec2};
use screengrabcraft_media::timeline::{Blend, MAX_TRANSITION_US, Track};
use screengrabcraft_model::Raster;
use serde_json::json;

use super::{LibDrop, VideoView, library};
use crate::editor::Editor;
use crate::theme::Tokens;

/// The Library's section of them.
pub const GROUP: &str = "Between Clips";

/// The lengths offered (µs).
const LENGTHS: [(u64, &str); 4] = [(500_000, "0.5 s"), (1_000_000, "1 s"), (2_000_000, "2 s"), (3_000_000, "3 s")];

/// The transitions whose names (or the section's) have these words in them.
pub fn matching(wanted: &str) -> Vec<Blend> {
    if wanted.is_empty() {
        return Vec::new();
    }
    let section = GROUP.to_lowercase().contains(wanted) || "transition".contains(wanted) || wanted.contains("transition");
    Blend::ALL.into_iter().filter(|b| section || b.label().to_lowercase().contains(wanted)).collect()
}

/// A transition's card: two made-up screens, the one going and the one coming, part of the way
/// through it.
pub fn card_picture(kind: Blend) -> Option<Raster> {
    let (w, h) = (256, 144);
    let going = library::backdrop(w, h)?;
    // The one coming: the same screen in another light, warm where the other is cool.
    let mut coming = going.clone();
    for p in coming.pixels_mut().as_chunks_mut::<4>().0 {
        let (r, g, b) = (p[0], p[1], p[2]);
        p[0] = b.saturating_add(70);
        p[1] = g.saturating_add(18);
        p[2] = r / 2;
    }
    let p = match kind {
        Blend::DipBlack => 0.35,
        Blend::CrossFade => 0.5,
        _ => 0.42,
    };
    Some(screengrabcraft_media::compose::blend_pictures(&going, &coming, kind, p))
}

/// Whether clip `ci` of a track meets the one before it (a transition can go there).
fn meets(track: &Track, ci: usize) -> bool {
    match (ci.checked_sub(1).and_then(|p| track.clips.get(p)), track.clips.get(ci)) {
        (Some(b), Some(c)) => b.end() == c.at,
        _ => false,
    }
}

/// The cuts of a track where two clips meet: the moment, and the clip coming in there.
pub(super) fn cuts(track: &Track) -> Vec<(u64, usize)> {
    (1..track.clips.len()).filter(|&ci| meets(track, ci)).filter_map(|ci| track.clips.get(ci).map(|c| (c.at, ci))).collect()
}

/// Draws a track's transitions on its row: a band across each cut as long as it plays, with a
/// bow (the two clips crossing) on it.
pub(super) fn draw(painter: &egui::Painter, timeline: &screengrabcraft_media::timeline::Timeline, i: usize, row: Rect, x_of: &dyn Fn(u64) -> f32) {
    let Some(track) = timeline.tracks.get(i) else { return };
    for ci in 1..track.clips.len() {
        let Some((a, b, _)) = timeline.transition_span(i, ci) else { continue };
        let r = Rect::from_min_max(pos2(x_of(a), row.min.y + 3.0), pos2(x_of(b).max(x_of(a) + 6.0), row.max.y - 3.0));
        painter.rect_filled(r, 3.0, Color32::from_white_alpha(70));
        let s = Stroke::new(1.5, Color32::WHITE);
        painter.line_segment([r.left_top(), r.right_bottom()], s);
        painter.line_segment([r.left_bottom(), r.right_top()], s);
        painter.rect_stroke(r, 3.0, s, egui::StrokeKind::Inside);
    }
}

/// Lights the cut a transition would go to under the pointer.
pub(super) fn draw_target(painter: &egui::Painter, row: Rect, x: f32, accent: Color32) {
    let r = Rect::from_center_size(pos2(x, row.center().y), vec2(18.0, row.height() - 4.0));
    painter.rect_filled(r, 4.0, accent.gamma_multiply(0.5));
    painter.rect_stroke(r, 4.0, Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Inside);
}

impl VideoView {
    /// Sets the transition into a clip (none: a cut).
    pub(super) fn set_transition(&mut self, ed: &mut Editor, ctx: &egui::Context, ti: usize, ci: usize, kind: Option<Blend>, len: Option<u64>) {
        let mut p = json!({"track": ti, "clip": ci, "kind": kind.map_or(json!("none"), |k| json!(k))});
        if let (Some(l), Some(o)) = (len, p.as_object_mut()) {
            o.insert("len".into(), json!(l));
        }
        self.run(ed, ctx, "video.transition", p);
    }

    /// A transition dropped (or added with +) at `at`: at the nearest cut where two clips meet,
    /// on `track` (else on the chosen clip's track, else any).
    pub fn drop_blend(&mut self, ed: &mut Editor, ctx: &egui::Context, kind: Blend, track: Option<usize>, at: u64) {
        let tl = &self.session.timeline;
        let which: Vec<usize> = match track.or(self.selected.map(|(t, _)| t)) {
            Some(t) => vec![t],
            None => (0..tl.tracks.len()).collect(),
        };
        let best = which
            .iter()
            .filter_map(|&ti| tl.tracks.get(ti).map(|t| (ti, t)))
            .flat_map(|(ti, t)| cuts(t).into_iter().map(move |(cut, ci)| (ti, ci, cut)))
            .min_by_key(|(_, _, cut)| cut.abs_diff(at));
        match best {
            Some((ti, ci, _)) => {
                self.set_transition(ed, ctx, ti, ci, Some(kind), None);
                self.selected = Some((ti, ci));
                self.item = None;
            }
            None => ed.error(ctx, "A transition goes where two clips meet: split the track first (S), then drop it on the cut.".to_string()),
        }
    }

    /// The clip panel's transition into the chosen clip from the one before: what, and how long.
    pub(super) fn transition_section(&mut self, ed: &mut Editor, ui: &mut Ui, ti: usize, ci: usize) {
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        let Some(track) = self.session.timeline.tracks.get(ti).cloned() else { return };
        if !meets(&track, ci) {
            ui.label(RichText::new("It doesn't meet the clip before it: a transition goes where two clips meet.").size(12.0).color(t.muted));
            return;
        }
        let now = track.clips.get(ci).and_then(|c| c.transition);
        let sound = !track.kind.is_video();
        if sound {
            let on = now.is_some();
            if ui
                .add(egui::Button::new("Cross-fade from the Clip Before").selected(on).min_size(vec2(ui.available_width(), 28.0)))
                .on_hover_text("One clip's sound goes down as the next comes up")
                .clicked()
            {
                self.set_transition(ed, &ctx, ti, ci, (!on).then_some(Blend::CrossFade), None);
            }
        } else {
            let label = now.map_or("None (a cut)", |x| x.kind.label());
            let mut chosen: Option<Option<Blend>> = None;
            egui::ComboBox::from_id_salt(("clip-transition", ti, ci)).width(ui.available_width() - 8.0).selected_text(label).show_ui(ui, |ui| {
                if ui.selectable_label(now.is_none(), "None (a cut)").clicked() {
                    chosen = Some(None);
                }
                for k in Blend::ALL {
                    if ui.selectable_label(now.is_some_and(|x| x.kind == k), k.label()).clicked() {
                        chosen = Some(Some(k));
                    }
                }
            });
            if let Some(k) = chosen {
                self.set_transition(ed, &ctx, ti, ci, k, None);
            }
        }
        if let Some(x) = now {
            ui.add_space(6.0);
            let w = ((ui.available_width() - 18.0) / 4.0).floor();
            let mut len = None;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (l, words) in LENGTHS {
                    if ui.add(egui::Button::new(words).selected(x.len == l).min_size(vec2(w, 26.0))).on_hover_text("How long it takes").clicked() {
                        len = Some(l.min(MAX_TRANSITION_US));
                    }
                }
            });
            if let Some(l) = len {
                self.set_transition(ed, &ctx, ti, ci, Some(x.kind), Some(l));
            }
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new("It plays across the cut, half before it and half after. Or drop one from the Library's Between Clips.")
                .size(12.0)
                .color(t.muted),
        );
    }

    /// The right button's transitions for the chosen clip, from the one before it.
    pub(super) fn transition_menu(&mut self, ed: &mut Editor, ui: &mut Ui, ctx: &egui::Context, ti: usize, ci: usize) {
        let Some(track) = self.session.timeline.tracks.get(ti).cloned() else { return };
        let ok = meets(&track, ci);
        let now = track.clips.get(ci).and_then(|c| c.transition).map(|x| x.kind);
        ui.add_enabled_ui(ok, |ui| {
            ui.menu_button("Transition from the Clip Before", |ui| {
                if ui.add(egui::Button::new("None (a Cut)").selected(now.is_none())).clicked() {
                    self.set_transition(ed, ctx, ti, ci, None, None);
                    ui.close();
                }
                let kinds: &[Blend] = if track.kind.is_video() { &Blend::ALL } else { &[Blend::CrossFade] };
                for &k in kinds {
                    if ui.add(egui::Button::new(k.label()).selected(now == Some(k))).clicked() {
                        self.set_transition(ed, ctx, ti, ci, Some(k), None);
                        ui.close();
                    }
                }
            })
            .response
            .on_disabled_hover_text("It doesn't meet the clip before it");
        });
    }

    /// The Library's cards of transitions between clips (each dragged onto a cut, or + to put it
    /// at the cut nearest the playhead).
    pub(super) fn blend_cards(&mut self, ed: &mut Editor, ui: &mut Ui, ctx: &egui::Context, kinds: &[Blend], detail: &str, budget: &mut usize) {
        for row in kinds.chunks(2) {
            ui.horizontal(|ui| {
                for &kind in row {
                    let key = format!("blend-{kind:?}");
                    if !self.thumbs.contains_key(&key) {
                        if *budget > 0 {
                            *budget -= 1;
                            if let Some(pic) = card_picture(kind) {
                                self.thumbs.insert(key.clone(), library::texture(ctx, &key, &pic));
                            }
                        } else {
                            ctx.request_repaint();
                        }
                    }
                    let tex = self.thumbs.get(&key).cloned();
                    let (added, resp) = self.card(ui, LibDrop::Blend(kind), tex.as_ref(), None, kind.label(), detail, None);
                    resp.on_hover_text("Drop it where two clips of a track meet");
                    if added {
                        let at = self.playhead;
                        self.drop_blend(ed, ctx, kind, None, at);
                    }
                }
            });
        }
    }
}
