//! The strip of recent captures along the bottom of the editor: click one to open it, or the
//! bin on one under the pointer to move it to the Recycle Bin. The mouse wheel over it goes back
//! through older captures (all of them, a thumbnail made only for those in view), and All
//! captures shows every one bigger, newest first, by day, with a search. A video has a play
//! mark on its thumbnail.

use egui::{Color32, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::autosave::TRASH;
use crate::editor::Editor;
use crate::icons::{self, Icon};
use crate::services::RecentItem;
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
/// The most captures the strip and All captures show (the newest).
pub const MAX_SHOWN: usize = 2000;
/// A thumbnail's size in All captures.
const BIG: egui::Vec2 = egui::Vec2 { x: 196.0, y: 132.0 };
/// All captures' width: three thumbnails a row.
const ALL_WIDTH: f32 = 3.0 * BIG.x + 2.0 * 12.0 + 18.0;

/// What the delete button says for a capture (its tooltip says it shorter).
pub fn delete_label(name: &str) -> String {
    format!("Move {name} to the {TRASH}")
}

/// Whether a capture is a video (a recording or an MP4).
pub fn is_video(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| x.eq_ignore_ascii_case(screengrabcraft_media::recording::EXT) || x.eq_ignore_ascii_case("mp4"))
}

/// The heading for a day (`2026-11-07`) in All captures: Today, Yesterday, else the weekday and
/// date. A day that can't be read is shown as it is.
pub fn day_heading(day: &str, today: &str) -> String {
    let Some(d) = parse_day(day) else { return if day.is_empty() { "Earlier".to_string() } else { day.to_string() } };
    if let Some(t) = parse_day(today) {
        let (n, m) = (days(d), days(t));
        if n == m {
            return "Today".to_string();
        }
        if n + 1 == m {
            return "Yesterday".to_string();
        }
    }
    const WEEKDAYS: [&str; 7] = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let weekday = WEEKDAYS.get(days(d).rem_euclid(7) as usize).copied().unwrap_or("");
    let month = MONTHS.get(usize::from(d.1.saturating_sub(1))).copied().unwrap_or("");
    format!("{weekday}, {} {month} {}", d.2, d.0)
}

/// `2026-11-07` as (year, month, day).
fn parse_day(s: &str) -> Option<(i64, u8, u8)> {
    let mut parts = s.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: u8 = parts.next()?.parse().ok()?;
    let d: u8 = parts.next()?.parse().ok()?;
    ((1..=12).contains(&m) && (1..=31).contains(&d) && (1..=9999).contains(&y) && parts.next().is_none()).then_some((y, m, d))
}

/// Days since 1970-01-01 (a Thursday), by the civil calendar.
fn days((y, m, d): (i64, u8, u8)) -> i64 {
    let (m, d) = (i64::from(m), i64::from(d));
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Draws a capture's thumbnail in `rect` (asking for it to be made when it isn't, or is out of
/// date), with a play mark on a video.
fn thumbnail(ed: &mut Editor, ui: &Ui, item: &RecentItem, rect: egui::Rect) {
    let ctx = ui.ctx().clone();
    // A picture written again (by the editor or anyone) gets a new thumbnail; the old one stays
    // up until it's ready.
    let stale = ed.thumbs.get(&item.path).is_none_or(|(m, _)| *m != item.modified);
    if stale {
        match ed.services.thumbnail(&item.path, item.modified) {
            Some(r) => {
                let img = egui::ColorImage::from_rgba_unmultiplied([r.width() as usize, r.height() as usize], r.pixels());
                let tex = ctx.load_texture(format!("thumb-{}", item.path), img, egui::TextureOptions::LINEAR);
                ed.thumbs.insert(item.path.clone(), (item.modified, tex));
            }
            None => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
        }
    }
    let inner = rect.shrink(6.0);
    if let Some((_, tex)) = ed.thumbs.get(&item.path) {
        let size = tex.size_vec2();
        if size.x > 0.0 && size.y > 0.0 {
            let f = (inner.width() / size.x).min(inner.height() / size.y);
            let r = egui::Rect::from_center_size(inner.center(), size * f);
            ui.painter().image(tex.id(), r, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
    }
    if is_video(&item.path) {
        video_mark(ui.painter(), inner);
    }
}

/// A round play mark at the lower left of a video's thumbnail.
fn video_mark(p: &egui::Painter, inner: egui::Rect) {
    let r = (inner.height() * 0.16).clamp(9.0, 18.0);
    let c = pos2(inner.min.x + r + 4.0, inner.max.y - r - 4.0);
    p.circle_filled(c, r, Color32::from_black_alpha(170));
    p.circle_stroke(c, r, Stroke::new(1.5, Color32::WHITE));
    let k = r * 0.45;
    p.add(egui::Shape::convex_polygon(
        vec![pos2(c.x - k * 0.7, c.y - k), pos2(c.x + k, c.y), pos2(c.x - k * 0.7, c.y + k)],
        Color32::WHITE,
        Stroke::NONE,
    ));
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
        let items = ed.recent.clone();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Recent captures").size(12.0).color(t.muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, Icon::Folder, "Show the captures folder", false, true).clicked() {
                    ed.services.open_folder(None);
                }
                all_captures(ed, ui, &items);
            });
        });
        ui.add_space(4.0);
        if items.is_empty() {
            ui.label(
                egui::RichText::new("Captures you take appear here. Press Print Screen, or use the ScreenGrabCraft icon in the tray.").color(t.muted),
            );
            return;
        }
        // The mouse wheel over the strip goes along it: down for older captures, up for newer.
        let wheel = if ui.rect_contains_pointer(ui.max_rect()) {
            ctx.input_mut(|i| {
                let dy = i.smooth_scroll_delta.y;
                i.smooth_scroll_delta.y = 0.0;
                dy
            })
        } else {
            0.0
        };
        egui::ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
            if wheel != 0.0 {
                ui.scroll_with_delta(vec2(wheel, 0.0));
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(GAP, 0.0);
                for item in items.iter().take(MAX_SHOWN) {
                    let (rect, resp) = ui.allocate_exact_size(vec2(THUMB * 1.4, THUMB), Sense::click());
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &item.name));
                    // Only those in view are drawn, and get a thumbnail made.
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let current = ed.picture_path.as_deref() == Some(item.path.as_str()) || ed.project_path.as_deref() == Some(item.path.as_str());
                    let pointed = ui.rect_contains_pointer(rect);
                    let bg = if pointed { t.hover } else { t.panel };
                    ui.painter().rect_filled(rect, 5.0, bg);
                    thumbnail(ed, ui, item, rect);
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

/// All captures: every capture as a bigger thumbnail, newest first, under the day it was taken
/// (Today, Yesterday, then the date), with a search by name; a click opens it.
fn all_captures(ed: &mut Editor, ui: &mut Ui, items: &[RecentItem]) {
    let t = Tokens::get(ui.ctx());
    let shown = items.len().min(MAX_SHOWN);
    let resp = ui.button(format!("All captures ({shown})")).on_hover_text("Every capture in the captures folder, newest first, by day");
    egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(ALL_WIDTH);
        let search = egui::TextEdit::singleline(&mut ed.capture_search).hint_text("Search by name").desired_width(f32::INFINITY);
        ui.add(search);
        ui.add_space(6.0);
        let words: Vec<String> = ed.capture_search.to_lowercase().split_whitespace().map(str::to_string).collect();
        let mut matching: Vec<&RecentItem> =
            items.iter().take(MAX_SHOWN).filter(|i| words.iter().all(|w| i.name.to_lowercase().contains(w.as_str()))).collect();
        if matching.is_empty() {
            ui.label(egui::RichText::new(if items.is_empty() { "No captures yet." } else { "No capture has that name." }).color(t.muted));
            return;
        }
        // Newest first, whatever order they came in.
        matching.sort_by_key(|i| std::cmp::Reverse(i.modified));
        let today = ed.services.today();
        egui::ScrollArea::vertical().max_height(560.0).auto_shrink([false, true]).show(ui, |ui| {
            let mut start = 0;
            while let Some(first) = matching.get(start) {
                let end = matching.iter().skip(start).position(|i| i.day != first.day).map_or(matching.len(), |n| start + n);
                ui.add_space(4.0);
                ui.label(egui::RichText::new(day_heading(&first.day, &today)).strong());
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                    for item in matching.get(start..end).unwrap_or_default() {
                        if tile(ed, ui, item) {
                            ui.close();
                        }
                    }
                });
                start = end;
            }
        });
    });
}

/// A capture in All captures: its thumbnail and the time (or name) under it. True when clicked
/// (it's opened).
fn tile(ed: &mut Editor, ui: &mut Ui, item: &RecentItem) -> bool {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let (rect, resp) = ui.allocate_exact_size(vec2(BIG.x, BIG.y + 20.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &item.name));
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let current = ed.picture_path.as_deref() == Some(item.path.as_str()) || ed.project_path.as_deref() == Some(item.path.as_str());
    let pic = egui::Rect::from_min_size(rect.min, BIG);
    let bg = if resp.hovered() { t.hover } else { t.panel };
    ui.painter().rect_filled(pic, 5.0, bg);
    thumbnail(ed, ui, item, pic);
    let ring = if current { Stroke::new(2.0, t.accent) } else { Stroke::new(1.0, t.border) };
    ui.painter().rect_stroke(pic, 5.0, ring, StrokeKind::Inside);
    let caption =
        if item.time.is_empty() { item.name.clone() } else { format!("{}  {}", item.time, if is_video(&item.path) { "Video" } else { "" }) };
    ui.painter().text(pos2(pic.min.x + 2.0, pic.max.y + 3.0), egui::Align2::LEFT_TOP, caption.trim_end(), egui::FontId::proportional(12.0), t.muted);
    let clicked = resp.on_hover_text(&item.name).clicked();
    if clicked && !current {
        ed.open_path(&ctx, &item.path);
    }
    clicked
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_are_headed_today_yesterday_or_by_their_date() {
        assert_eq!(day_heading("2026-11-07", "2026-11-07"), "Today");
        assert_eq!(day_heading("2026-11-06", "2026-11-07"), "Yesterday");
        assert_eq!(day_heading("2026-10-31", "2026-11-01"), "Yesterday", "across a month");
        assert_eq!(day_heading("2026-11-05", "2026-11-07"), "Thursday, 5 November 2026");
        assert_eq!(day_heading("1970-01-01", ""), "Thursday, 1 January 1970");
        assert_eq!(day_heading("2024-02-29", "2026-11-07"), "Thursday, 29 February 2024");
        assert_eq!(day_heading("", "2026-11-07"), "Earlier");
        for junk in ["2026-13-01", "26-11", "x", "2026-11-07-1", "99999-01-01"] {
            assert_eq!(day_heading(junk, "2026-11-07"), junk, "shown as it is");
        }
    }

    #[test]
    fn recordings_and_mp4s_are_videos() {
        assert!(is_video(&format!("a/Recording.{}", screengrabcraft_media::recording::EXT)));
        assert!(is_video("b/clip.MP4"));
        assert!(!is_video("c/Capture.png"));
        assert!(!is_video("noext"));
    }
}
