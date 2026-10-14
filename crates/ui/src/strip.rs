//! The strip of recent captures along the bottom of the editor: click one to open it.

use egui::{Color32, Sense, Stroke, StrokeKind, Ui, vec2};

use crate::editor::Editor;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

const THUMB: f32 = 72.0;

pub fn show(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    if !ed.services.has_paths() {
        return;
    }
    egui::Panel::bottom("recent_strip")
        .exact_size(THUMB + 34.0)
        .frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(egui::Margin::symmetric(10, 6)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Recent captures").size(12.0).color(t.muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::button(ui, Icon::Folder, "Show the captures folder", false, true).clicked() {
                        ed.services.open_folder(None);
                    }
                });
            });
            let items = ed.recent.clone();
            if items.is_empty() {
                ui.label(
                    egui::RichText::new("Captures you take appear here. Press Print Screen, or use the ScreenGrabCraft icon in the tray.")
                        .color(t.muted),
                );
                return;
            }
            egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                    for item in items.iter().take(40) {
                        let (rect, resp) = ui.allocate_exact_size(vec2(THUMB * 1.4, THUMB), Sense::click());
                        let current =
                            ed.picture_path.as_deref() == Some(item.path.as_str()) || ed.project_path.as_deref() == Some(item.path.as_str());
                        let bg = if resp.hovered() { t.hover } else { t.panel };
                        ui.painter().rect_filled(rect, 5.0, bg);
                        if !ed.thumbs.contains_key(&item.path)
                            && let Some(r) = ed.services.thumbnail(&item.path)
                        {
                            let img = egui::ColorImage::from_rgba_unmultiplied([r.width() as usize, r.height() as usize], r.pixels());
                            let tex = ctx.load_texture(format!("thumb-{}", item.path), img, egui::TextureOptions::LINEAR);
                            ed.thumbs.insert(item.path.clone(), tex);
                        }
                        match ed.thumbs.get(&item.path) {
                            Some(tex) => {
                                let size = tex.size_vec2();
                                let inner = rect.shrink(4.0);
                                let f = (inner.width() / size.x).min(inner.height() / size.y);
                                let r = egui::Rect::from_center_size(inner.center(), size * f);
                                ui.painter().image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                            }
                            None => {
                                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                            }
                        }
                        let ring = if current { Stroke::new(2.0, t.accent) } else { Stroke::new(1.0, t.border) };
                        ui.painter().rect_stroke(rect, 5.0, ring, StrokeKind::Inside);
                        let resp = resp.on_hover_text(&item.name);
                        if resp.clicked() && !current {
                            ed.open_path(&ctx, &item.path);
                        }
                    }
                });
            });
        });
    // Thumbnails of captures no longer listed are let go.
    let listed: std::collections::HashSet<&str> = ed.recent.iter().map(|r| r.path.as_str()).collect();
    ed.thumbs.retain(|k, _| listed.contains(k.as_str()));
}
