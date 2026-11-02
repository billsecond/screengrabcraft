//! The strip of recent captures along the bottom of the editor: click one to open it, or the
//! bin on one under the pointer to move it to the Recycle Bin.

use egui::{Color32, Sense, Stroke, StrokeKind, Ui, vec2};

use crate::autosave::TRASH;
use crate::editor::Editor;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

/// A thumbnail's height (it is 1.4 times as wide).
pub const THUMB: f32 = 90.0;
/// The room between thumbnails.
const GAP: f32 = 14.0;
/// The room round the strip's edges.
const MARGIN: egui::Margin = egui::Margin { left: 16, right: 16, top: 10, bottom: 12 };
/// The delete button in a thumbnail's corner.
const BIN: f32 = 28.0;
/// Below the thumbnails, where the scroll bar goes rather than over them.
const SCROLL_ROOM: f32 = 10.0;

/// What the delete button says for a capture (its tooltip says it shorter).
pub fn delete_label(name: &str) -> String {
    format!("Move {name} to the {TRASH}")
}

pub fn show(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    if !ed.services.has_paths() {
        return;
    }
    // The heading row (with the folder button), a gap, the thumbnails, room for the scroll bar
    // under them, and the margins.
    let height = 30.0 + 8.0 + THUMB + SCROLL_ROOM + f32::from(MARGIN.top) + f32::from(MARGIN.bottom);
    egui::Panel::bottom("recent_strip").exact_size(height).frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(MARGIN)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Recent captures").size(12.0).color(t.muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, Icon::Folder, "Show the captures folder", false, true).clicked() {
                    ed.services.open_folder(None);
                }
            });
        });
        ui.add_space(4.0);
        let items = ed.recent.clone();
        if items.is_empty() {
            ui.label(
                egui::RichText::new("Captures you take appear here. Press Print Screen, or use the ScreenGrabCraft icon in the tray.").color(t.muted),
            );
            return;
        }
        egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(GAP, 0.0);
                for item in items.iter().take(40) {
                    let (rect, resp) = ui.allocate_exact_size(vec2(THUMB * 1.4, THUMB), Sense::click());
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &item.name));
                    let current = ed.picture_path.as_deref() == Some(item.path.as_str()) || ed.project_path.as_deref() == Some(item.path.as_str());
                    let pointed = ui.rect_contains_pointer(rect);
                    let bg = if pointed { t.hover } else { t.panel };
                    ui.painter().rect_filled(rect, 5.0, bg);
                    // A picture written again (by the editor or anyone) gets a new thumbnail;
                    // the old one stays up until it's ready.
                    let stale = ed.thumbs.get(&item.path).is_none_or(|(m, _)| *m != item.modified);
                    if stale {
                        match ed.services.thumbnail(&item.path, item.modified) {
                            Some(r) => {
                                let img = egui::ColorImage::from_rgba_unmultiplied([r.width() as usize, r.height() as usize], r.pixels());
                                let tex = ctx.load_texture(format!("thumb-{}", item.path), img, egui::TextureOptions::LINEAR);
                                ed.thumbs.insert(item.path.clone(), (item.modified, tex));
                            }
                            None => ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)),
                        }
                    }
                    if let Some((_, tex)) = ed.thumbs.get(&item.path) {
                        let size = tex.size_vec2();
                        let inner = rect.shrink(6.0);
                        let f = (inner.width() / size.x).min(inner.height() / size.y);
                        let r = egui::Rect::from_center_size(inner.center(), size * f);
                        ui.painter().image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    let ring = if current { Stroke::new(2.0, t.accent) } else { Stroke::new(1.0, t.border) };
                    ui.painter().rect_stroke(rect, 5.0, ring, StrokeKind::Inside);
                    let resp = resp.on_hover_text(&item.name);
                    // Over the thumbnail, so it takes the click, not the thumbnail.
                    let delete = pointed && delete_button(ui, rect, &item.name).clicked();
                    if delete {
                        ed.delete_capture(&item.path);
                    } else if resp.clicked() && !current {
                        ed.open_path(&ctx, &item.path);
                    }
                }
            });
            ui.add_space(SCROLL_ROOM);
        });
    });
    // Thumbnails of captures no longer listed are let go.
    let listed: std::collections::HashSet<&str> = ed.recent.iter().map(|r| r.path.as_str()).collect();
    ed.thumbs.retain(|k, _| listed.contains(k.as_str()));
}

/// The bin in the top right corner of a thumbnail.
fn delete_button(ui: &mut Ui, thumb: egui::Rect, name: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let rect = egui::Rect::from_min_size(egui::pos2(thumb.max.x - BIN - 3.0, thumb.min.y + 3.0), vec2(BIN, BIN));
    let resp = ui.interact(rect, ui.id().with(("delete", name)), Sense::click());
    let label = delete_label(name);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    let (fill, ink) = if resp.hovered() { (t.danger, Color32::WHITE) } else { (t.panel, t.text) };
    ui.painter().rect_filled(rect, 5.0, fill);
    ui.painter().rect_stroke(rect, 5.0, Stroke::new(1.0, t.border), StrokeKind::Inside);
    icons::paint(ui.painter(), rect.shrink(5.0), Icon::Delete, ink);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("Move to the {TRASH}"))
}
