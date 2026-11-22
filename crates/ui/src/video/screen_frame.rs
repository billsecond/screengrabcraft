//! The recording's own screen framed, in the side panel: a device round it, made smaller on a
//! background colour, its corners rounded, a border, a shadow.

use egui::{Color32, RichText, Ui};
use screengrabcraft_media::frame::Device;
use serde_json::json;

use super::VideoView;
use crate::editor::Editor;
use crate::theme::Tokens;

/// A colour as `#rrggbb`.
fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

impl VideoView {
    /// The screen's frame: a device round it, its size in the video, the background round it,
    /// rounded corners, a border, a shadow.
    pub(super) fn screen_frame_section(&mut self, ed: &mut Editor, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let t = Tokens::get(ui.ctx());
        let f = self.session.timeline.frame.clone().checked();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Device").size(12.0).color(t.muted));
            egui::ComboBox::from_id_salt("screen-device").width(150.0).selected_text(f.device.label()).show_ui(ui, |ui| {
                for d in Device::ALL {
                    if ui.selectable_label(f.device == d, d.label()).clicked() {
                        self.run(ed, &ctx, "video.screenFrame", json!({ "device": d }));
                    }
                }
            });
        });
        ui.add_space(4.0);
        let mut size = f.size * 100.0;
        let r = super::named_slider(ui, "Size, of the video", &mut size, 50.0..=100.0, "%")
            .on_hover_text("The screen made smaller, the background round it");
        self.frame_change(ed, &ctx, &r, (size - f.size * 100.0).abs() > 0.4, json!({"size": size / 100.0}));
        let mut radius = f.radius * 100.0;
        let r = super::named_slider(ui, "Rounded corners", &mut radius, 0.0..=20.0, "%");
        self.frame_change(ed, &ctx, &r, (radius - f.radius * 100.0).abs() > 0.4, json!({"radius": radius / 100.0}));
        let mut border = f.border * 100.0;
        let r = super::named_slider(ui, "Border, of its width", &mut border, 0.0..=5.0, "%");
        self.frame_change(ed, &ctx, &r, (border - f.border * 100.0).abs() > 0.04, json!({"border": border / 100.0}));
        ui.add_space(2.0);
        egui::Grid::new("screen-frame-colours").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            for (label, key, value, or) in
                [("Background", "background", &f.background, [0x1e, 0x22, 0x30]), ("Border colour", "borderColour", &f.border_colour, [0xff; 3])]
            {
                ui.label(RichText::new(label).size(12.0).color(t.muted));
                let [r, g, b] = screengrabcraft_media::layers::color(value, or);
                let mut c = Color32::from_rgb(r, g, b);
                if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                    self.run(ed, &ctx, "video.screenFrame", json!({ key: hex(c) }));
                }
                ui.end_row();
            }
        });
        let mut shadow = f.shadow;
        if ui.checkbox(&mut shadow, "Shadow").on_hover_text("A soft shadow under the screen (and its device)").changed() {
            self.run(ed, &ctx, "video.screenFrame", json!({ "shadow": shadow }));
        }
        if !f.is_plain() {
            ui.add_space(4.0);
            if ui.button("Fill the Video Again").on_hover_text("No frame: the screen fills the video as recorded").clicked() {
                self.run(ed, &ctx, "video.screenFrame", json!({"size": 1, "radius": 0, "border": 0, "shadow": false, "device": "none"}));
            }
        }
        ui.add_space(2.0);
        ui.label(RichText::new("The camera, the layers and the captions stay where they are on the video.").size(12.0).color(t.muted));
    }

    /// A change of the screen's frame from a slider: a drag is one undo step.
    fn frame_change(&mut self, ed: &mut Editor, ctx: &egui::Context, r: &egui::Response, moved: bool, p: serde_json::Value) {
        if r.drag_started() {
            self.session.begin("Screen Frame");
        }
        if r.changed() && moved {
            if r.dragged() {
                let _ = self.session.drag("video.screenFrame", &p);
            } else {
                self.run(ed, ctx, "video.screenFrame", p);
            }
        }
        if r.drag_stopped() {
            self.session.commit();
        }
    }
}
