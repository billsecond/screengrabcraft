//! Clips together and apart in the video editor: groups of clips that move together, clip
//! colours, clips played backwards, and tracks locked or played alone (the toggles in their
//! headers).

use egui::{Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use screengrabcraft_media::timeline::Track;
use serde_json::json;

use super::VideoView;
use crate::editor::Editor;
use crate::theme::Tokens;

/// The colours a clip can be given, by name.
pub const COLOURS: [(&str, [u8; 3]); 8] = [
    ("Red", [0xE5, 0x48, 0x4D]),
    ("Orange", [0xF0, 0x8C, 0x2E]),
    ("Yellow", [0xE8, 0xC5, 0x2A]),
    ("Green", [0x3F, 0xB9, 0x50]),
    ("Teal", [0x1F, 0xA8, 0xA0]),
    ("Blue", [0x3B, 0x82, 0xF6]),
    ("Purple", [0x8B, 0x5C, 0xF6]),
    ("Pink", [0xE0, 0x5C, 0xB0]),
];

/// A clip's colour on the timeline: its own if it was given one, else `track`'s.
pub(super) fn colour(colour: Option<u8>, track: Color32) -> Color32 {
    colour.and_then(|k| COLOURS.get(usize::from(k))).map_or(track, |(_, [r, g, b])| Color32::from_rgb(*r, *g, *b))
}

/// A grouped clip's mark: two links of a chain at its bottom left.
pub(super) fn draw_group_mark(painter: &egui::Painter, r: Rect) {
    if r.width() < 30.0 {
        return;
    }
    let base = pos2(r.min.x + 7.0, r.max.y - 9.0);
    let s = Stroke::new(1.5, Color32::WHITE);
    for dx in [0.0, 7.0] {
        painter.rect_stroke(Rect::from_min_size(base + vec2(dx, 0.0), vec2(10.0, 6.0)), 3.0, s, egui::StrokeKind::Middle);
    }
}

/// A locked track's row: stripes over it.
pub(super) fn draw_locked(painter: &egui::Painter, row: Rect) {
    let p = painter.with_clip_rect(row);
    let stroke = Stroke::new(1.0, Color32::from_black_alpha(70));
    let mut x = row.min.x - row.height();
    while x < row.max.x {
        p.line_segment([pos2(x, row.max.y), pos2(x + row.height(), row.min.y)], stroke);
        x += 9.0;
    }
}

/// A padlock, shut or open.
fn padlock(painter: &egui::Painter, r: Rect, shut: bool, c: Color32) {
    let body = Rect::from_min_max(pos2(r.min.x + 2.0, r.center().y - 1.0), pos2(r.max.x - 2.0, r.max.y - 2.0));
    painter.rect_filled(body, 2.0, c);
    let (left, right) = (body.min.x + 2.5, body.max.x - 2.5);
    let top = r.min.y + 2.5;
    let s = Stroke::new(1.6, c);
    // The shackle: closed into the body, or swung up and open on its right.
    let right_end = if shut { body.min.y } else { body.min.y - 3.0 };
    painter.line_segment([pos2(left, body.min.y), pos2(left, top + 2.0)], s);
    painter.line_segment([pos2(left, top + 2.0), pos2(right, top + 2.0)], s);
    painter.line_segment([pos2(right, top + 2.0), pos2(right, right_end)], s);
}

/// A small toggle in a header: true when it's clicked.
fn toggle(ui: &mut Ui, id: (&str, usize), rect: Rect, on: bool, tip: &str, draw: impl FnOnce(&egui::Painter, Rect, Color32)) -> bool {
    let t = Tokens::get(ui.ctx());
    let resp = ui.interact(rect, ui.id().with(id), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, tip));
    if on {
        ui.painter().rect_filled(rect, 4.0, t.accent.gamma_multiply(0.85));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, t.hover);
    }
    draw(ui.painter(), rect.shrink(3.0), if on { Color32::WHITE } else { t.muted });
    resp.on_hover_text(tip).clicked()
}

impl VideoView {
    /// A picture track's clip's colours (brightness, contrast, colour) and crop (each side).
    pub(super) fn clip_look_section(&mut self, ed: &mut Editor, ui: &mut Ui, ti: usize, ci: usize) {
        let ctx = ui.ctx().clone();
        let Some(look) = self.session.timeline.tracks.get(ti).and_then(|t| t.clips.get(ci)).map(|c| c.look) else { return };
        // A ready-made look: every colour at once (the sliders then change it further).
        let t = Tokens::get(ui.ctx());
        let chosen = match look.named() {
            Some(n) => n.label,
            None if look.colours_plain() => "As recorded",
            None => "Your own",
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Look").size(12.0).color(t.muted));
            egui::ComboBox::from_id_salt(("clip-look", ti, ci)).width(150.0).selected_text(chosen).show_ui(ui, |ui| {
                if ui.selectable_label(look.colours_plain(), "As recorded").clicked() {
                    self.run(ed, &ctx, "video.clipLook", json!({"track": ti, "clip": ci, "look": "plain"}));
                }
                for n in screengrabcraft_media::timeline::LOOKS {
                    if ui.selectable_label(look.named().is_some_and(|m| m.key == n.key), n.label).clicked() {
                        self.run(ed, &ctx, "video.clipLook", json!({"track": ti, "clip": ci, "look": n.key}));
                    }
                }
            });
        });
        ui.add_space(4.0);
        for (name, key, value) in
            [("Brightness", "brightness", look.brightness), ("Contrast", "contrast", look.contrast), ("Colour", "saturation", look.saturation)]
        {
            let mut v = f32::from(value);
            let r = super::named_slider(ui, name, &mut v, -100.0..=100.0, "");
            self.clip_look_change(ed, &ctx, &r, (v - f32::from(value)).abs() > 0.4, json!({"track": ti, "clip": ci, key: v.round()}));
        }
        // The tint: a colour, and how far towards it (a colour picked with none yet gives some).
        let mut amount = f32::from(look.tint_amount);
        let r = super::named_slider(ui, "Tint", &mut amount, 0.0..=100.0, "%");
        let tint = if look.tint_amount == 0 { [0xFF, 0x96, 0x3C] } else { look.tint };
        let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
        self.clip_look_change(
            ed,
            &ctx,
            &r,
            (amount - f32::from(look.tint_amount)).abs() > 0.4,
            json!({"track": ti, "clip": ci, "tint": hex(tint), "tintAmount": amount.round()}),
        );
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Tint colour").size(12.0).color(t.muted));
            let mut c = Color32::from_rgb(tint[0], tint[1], tint[2]);
            if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                let amount = if look.tint_amount == 0 { 30 } else { look.tint_amount };
                self.run(ed, &ctx, "video.clipLook", json!({"track": ti, "clip": ci, "tint": hex([c.r(), c.g(), c.b()]), "tintAmount": amount}));
            }
        });
        for (k, name) in ["Crop the left", "Crop the top", "Crop the right", "Crop the bottom"].into_iter().enumerate() {
            let was = f32::from(look.crop.get(k).copied().unwrap_or(0)) / 10.0;
            let mut v = was;
            let r = super::named_slider(ui, name, &mut v, 0.0..=45.0, "%");
            let mut crop = look.crop;
            if let Some(c) = crop.get_mut(k) {
                *c = (v * 10.0).round().clamp(0.0, 450.0) as u16;
            }
            self.clip_look_change(ed, &ctx, &r, (v - was).abs() > 0.04, json!({"track": ti, "clip": ci, "crop": crop}));
        }
        if !look.is_plain() {
            ui.add_space(4.0);
            if ui.button("As Recorded").on_hover_text("Its colours and its whole picture again").clicked() {
                self.run(ed, &ctx, "video.clipLook", json!({"track": ti, "clip": ci, "look": "plain", "crop": [0, 0, 0, 0]}));
            }
        }
    }

    /// A change of a clip's look from a slider: a drag is one undo step.
    fn clip_look_change(&mut self, ed: &mut Editor, ctx: &egui::Context, r: &egui::Response, moved: bool, p: serde_json::Value) {
        if r.drag_started() {
            self.session.begin("Colour and Crop");
        }
        if r.changed() && moved {
            if r.dragged() {
                let _ = self.session.drag("video.clipLook", &p);
            } else {
                self.run(ed, ctx, "video.clipLook", p);
            }
        }
        if r.drag_stopped() {
            self.session.commit();
        }
    }

    /// A track header's lock and (a sound track's) play-alone toggles, in `rect` (right to left).
    pub(super) fn track_toggles(&mut self, ed: &mut Editor, ui: &mut Ui, rect: Rect, i: usize, track: &Track) {
        let ctx = ui.ctx().clone();
        let lock = Rect::from_min_size(pos2(rect.max.x - 22.0, rect.min.y), vec2(20.0, 20.0));
        let tip = if track.locked { "Locked: its clips stay as they are. Click to unlock it." } else { "Lock it: its clips stay as they are" };
        if toggle(ui, ("lock", i), lock, track.locked, tip, |p, r, c| padlock(p, r, track.locked, c)) {
            self.run(ed, &ctx, "video.lock", json!({"track": i, "locked": !track.locked}));
        }
        if !track.kind.is_video() {
            let solo = Rect::from_min_size(pos2(lock.min.x - 23.0, rect.min.y), vec2(20.0, 20.0));
            let tip = if track.solo { "Played alone. Click to hear the others too." } else { "Play it alone: the other sound isn't heard" };
            let draw = |p: &egui::Painter, r: Rect, c: Color32| {
                p.text(r.center(), egui::Align2::CENTER_CENTER, "S", egui::FontId::proportional(12.0), c);
            };
            if toggle(ui, ("solo", i), solo, track.solo, tip, draw) {
                self.run(ed, &ctx, "video.solo", json!({"track": i, "solo": !track.solo}));
            }
        }
    }

    /// The right button's clip items: its colour, and its group.
    pub(super) fn clip_menu(&mut self, ed: &mut Editor, ui: &mut Ui, ctx: &egui::Context, ti: usize, ci: usize) {
        let Some(clip) = self.session.timeline.tracks.get(ti).and_then(|t| t.clips.get(ci)).copied() else { return };
        if !clip.hold {
            let words = if clip.reverse { "Play Forwards Again" } else { "Play Backwards" };
            if ui
                .button(words)
                .on_hover_text("From its end to its start, the picture and the sound turned round (the clips grouped with it too)")
                .clicked()
            {
                self.run(ed, ctx, "video.reverse", json!({"track": ti, "clip": ci, "reverse": !clip.reverse}));
                ui.close();
            }
        }
        ui.menu_button("Clip Colour", |ui| {
            if ui.add(egui::Button::new("As Its Track").selected(clip.colour.is_none())).clicked() {
                self.run(ed, ctx, "video.clipColour", json!({"track": ti, "clip": ci, "colour": null}));
                ui.close();
            }
            for (k, (name, [r, g, b])) in COLOURS.iter().enumerate() {
                let words = egui::RichText::new(*name).color(Color32::from_rgb(*r, *g, *b)).strong();
                if ui.add(egui::Button::new(words).selected(clip.colour == Some(k as u8))).clicked() {
                    self.run(ed, ctx, "video.clipColour", json!({"track": ti, "clip": ci, "colour": k}));
                    ui.close();
                }
            }
        });
        // The clips of the other tracks over the same time: grouped with it, they move with it.
        let others: Vec<(usize, usize)> = self
            .session
            .timeline
            .tracks
            .iter()
            .enumerate()
            .filter(|(t, _)| *t != ti)
            .flat_map(|(t, tr)| tr.clips.iter().enumerate().filter(|(_, c)| c.at < clip.end() && c.end() > clip.at).map(move |(c, _)| (t, c)))
            .collect();
        if clip.group.is_some() {
            if ui.button("Ungroup").on_hover_text("Its group's clips move on their own again").clicked() {
                self.run(ed, ctx, "video.ungroup", json!({"track": ti, "clip": ci}));
                ui.close();
            }
        } else if ui
            .add_enabled(!others.is_empty(), egui::Button::new("Group with the Clips Above and Below"))
            .on_hover_text("It and the other tracks' clips over the same time move together")
            .on_disabled_hover_text("No other track has a clip at this time")
            .clicked()
        {
            let mut all = vec![(ti, ci)];
            all.extend(others);
            let list: Vec<[usize; 2]> = all.into_iter().map(|(t, c)| [t, c]).collect();
            self.run(ed, ctx, "video.group", json!({"clips": list}));
            ui.close();
        }
    }
}
