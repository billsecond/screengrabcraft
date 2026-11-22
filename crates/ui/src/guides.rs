//! Rulers, guides and snapping. View › Rulers and Guides shows a ruler along the top and the left
//! of the picture's window, in the picture's pixels. A guide is dragged out of a ruler (out of the
//! top one, a line across; out of the left one, a line down), moved by dragging it, and taken away
//! by dragging it back onto its ruler or off the picture, or from its right-click menu. While
//! annotations are moved, their edges and middles snap to the guides and to the picture's edges
//! and middle (View › Snap to Guides).

use egui::{Align2, Color32, CursorIcon, FontId, Sense, Stroke, Ui, pos2};
use screengrabcraft_model::{Pt, Rect};

use crate::editor::Editor;
use crate::theme::Tokens;

/// A guide: a line across the picture at a height, or down it at a distance from the left, in
/// the picture's pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub across: bool,
    pub at: f32,
}

/// How thick the rulers are, in points.
pub const RULER: f32 = 18.0;
/// How near (in points on the screen) an edge comes to a line before it snaps to it.
pub const SNAP: f32 = 6.0;
/// The most guides kept.
pub const MAX_GUIDES: usize = 64;
/// What the rulers are called (their tooltips and names).
pub const TOP_RULER: &str = "Ruler across the top: drag a guide out of it";
pub const LEFT_RULER: &str = "Ruler down the left: drag a guide out of it";

/// The colour of guides, and of the lines something moved has snapped to.
const GUIDE: Color32 = Color32::from_rgb(0x00, 0xB8, 0xD4);
const SNAPPED: Color32 = Color32::from_rgb(0xE0, 0x40, 0xFB);

/// A move of annotations under way, for snapping: their box when it began, how far the pointer
/// has gone since, and how far they have been moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapState {
    pub origin: Rect,
    pub raw: Pt,
    pub applied: Pt,
}

/// Where something moved to `moved` snaps: how much further to move it, across and down, for one
/// of its edges or its middle to lie on one of the lines (`xs` down, `ys` across) within `reach`,
/// the nearest winning; and the lines it then lies on.
pub fn snap(moved: Rect, xs: &[f32], ys: &[f32], reach: f32) -> (Pt, Vec<Guide>) {
    let pick = |edges: [f32; 3], lines: &[f32]| -> Option<(f32, f32)> {
        let mut best: Option<(f32, f32)> = None;
        for e in edges {
            for l in lines {
                let d = l - e;
                if d.is_finite() && d.abs() <= reach && best.is_none_or(|(b, _)| d.abs() < b.abs()) {
                    best = Some((d, *l));
                }
            }
        }
        best
    };
    let x = pick([moved.x, moved.center().x, moved.right()], xs);
    let y = pick([moved.y, moved.center().y, moved.bottom()], ys);
    let mut lines = Vec::new();
    if let Some((_, at)) = x {
        lines.push(Guide { across: false, at });
    }
    if let Some((_, at)) = y {
        lines.push(Guide { across: true, at });
    }
    (Pt::new(x.map_or(0.0, |v| v.0), y.map_or(0.0, |v| v.0)), lines)
}

/// The lines annotations snap to: the picture's edges and middle, and the guides when they are
/// shown. Down the picture first, then across it.
pub fn lines(ed: &Editor) -> (Vec<f32>, Vec<f32>) {
    let b = ed.session.doc.bounds();
    let mut xs = vec![b.x, b.center().x, b.right()];
    let mut ys = vec![b.y, b.center().y, b.bottom()];
    if ed.settings.show_rulers {
        for g in &ed.guides {
            if g.across {
                ys.push(g.at);
            } else {
                xs.push(g.at);
            }
        }
    }
    (xs, ys)
}

/// The step between a ruler's numbered ticks, in picture pixels (1, 2 or 5 times a power of ten),
/// for them to be at least `gap` points apart at `scale` points a pixel.
pub fn tick_step(scale: f32, gap: f32) -> f32 {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let mut step = 1.0;
    for _ in 0..10 {
        for m in [1.0, 2.0, 5.0] {
            if step * m * scale >= gap {
                return step * m;
            }
        }
        step *= 10.0;
    }
    step
}

/// Adds a guide (none past [`MAX_GUIDES`]); its place in the list.
pub fn add(ed: &mut Editor, g: Guide) -> Option<usize> {
    if ed.guides.len() >= MAX_GUIDES || !g.at.is_finite() {
        return None;
    }
    ed.guides.push(g);
    Some(ed.guides.len() - 1)
}

/// A guide at the picture's middle, across and down.
pub fn add_middle(ed: &mut Editor) {
    let c = ed.session.doc.bounds().center();
    for g in [Guide { across: true, at: c.y }, Guide { across: false, at: c.x }] {
        if !ed.guides.contains(&g) {
            add(ed, g);
        }
    }
    ed.settings.show_rulers = true;
}

/// The lines something being moved has snapped to, drawn across the picture.
pub fn paint_snapped(ed: &Editor, ui: &Ui, clip: egui::Rect) {
    if ed.snap_lines.is_empty() {
        return;
    }
    let painter = ui.painter_at(clip);
    let img = ed.view.rect_to_screen(ed.session.doc.bounds());
    for g in &ed.snap_lines {
        let s = ed.view.to_screen(Pt::new(g.at, g.at));
        let (a, b) = if g.across { (pos2(img.left(), s.y), pos2(img.right(), s.y)) } else { (pos2(s.x, img.top()), pos2(s.x, img.bottom())) };
        painter.line_segment([a, b], Stroke::new(1.0, SNAPPED));
    }
}

/// The rulers and the guides over the canvas `rect` (when shown), and their dragging.
pub fn show(ed: &mut Editor, ui: &mut Ui, rect: egui::Rect) {
    if !ed.settings.show_rulers {
        ed.guide_drag = None;
        return;
    }
    let ctx = ui.ctx().clone();
    let t = Tokens::get(&ctx);
    let top = egui::Rect::from_min_max(pos2(rect.left() + RULER, rect.top()), pos2(rect.right(), rect.top() + RULER));
    let left = egui::Rect::from_min_max(pos2(rect.left(), rect.top() + RULER), pos2(rect.left() + RULER, rect.bottom()));
    let inside = egui::Rect::from_min_max(pos2(rect.left() + RULER, rect.top() + RULER), rect.max);

    // ---- the guides: each a thin strip to take hold of, under the rulers ----
    let mut remove = None;
    for i in 0..ed.guides.len() {
        let Some(g) = ed.guides.get(i).copied() else { continue };
        let s = ed.view.to_screen(Pt::new(g.at, g.at));
        let strip = if g.across {
            egui::Rect::from_min_max(pos2(inside.left(), s.y - 3.0), pos2(inside.right(), s.y + 3.0))
        } else {
            egui::Rect::from_min_max(pos2(s.x - 3.0, inside.top()), pos2(s.x + 3.0, inside.bottom()))
        };
        let Some(strip) = Some(strip.intersect(inside)).filter(|r| r.is_positive()) else { continue };
        let resp = ui.interact(strip, ui.id().with(("guide", i)), Sense::click_and_drag());
        let name = if g.across { format!("Guide across at {:.0} px", g.at) } else { format!("Guide down at {:.0} px", g.at) };
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &name));
        if resp.hovered() || ed.guide_drag == Some(i) {
            ctx.set_cursor_icon(if g.across { CursorIcon::ResizeVertical } else { CursorIcon::ResizeHorizontal });
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            ed.guide_drag = Some(i);
        }
        resp.context_menu(|ui| {
            if ui.button("Remove Guide").clicked() {
                remove = Some(i);
                ui.close();
            }
        });
    }
    if let Some(i) = remove {
        ed.guides.remove(i);
        ed.guide_drag = None;
    }

    // ---- the rulers: a drag out of one makes a guide ----
    for (r, across, name) in [(top, true, TOP_RULER), (left, false, LEFT_RULER)] {
        let resp = ui.interact(r, ui.id().with(("ruler", across)), Sense::drag());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, name));
        if resp.hovered() {
            ctx.set_cursor_icon(if across { CursorIcon::ResizeVertical } else { CursorIcon::ResizeHorizontal });
        }
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = ctx.input(|i| i.pointer.latest_pos())
        {
            let q = ed.view.to_image(p);
            ed.guide_drag = add(ed, Guide { across, at: if across { q.y.round() } else { q.x.round() } });
        }
    }

    // ---- a guide being dragged follows the pointer; let go on its ruler or off the picture, it goes ----
    if let Some(i) = ed.guide_drag {
        let pointer = ctx.input(|i| i.pointer.latest_pos());
        let down = ctx.input(|i| i.pointer.primary_down());
        if let (Some(p), Some(g)) = (pointer, ed.guides.get_mut(i)) {
            let q = ed.view.to_image(p);
            g.at = if g.across { q.y.round() } else { q.x.round() };
        }
        if !down {
            ed.guide_drag = None;
            let b = ed.session.doc.bounds();
            let gone = ed.guides.get(i).is_some_and(|g| {
                let on_ruler = pointer.is_some_and(|p| if g.across { p.y < top.bottom() } else { p.x < left.right() });
                let off = if g.across { g.at < b.y || g.at > b.bottom() } else { g.at < b.x || g.at > b.right() };
                on_ruler || off
            });
            if gone {
                ed.guides.remove(i);
            }
        }
    }

    // ---- drawn: the guides, then the rulers over them ----
    let painter = ui.painter_at(rect);
    for g in &ed.guides {
        let s = ed.view.to_screen(Pt::new(g.at, g.at));
        let (a, b) =
            if g.across { (pos2(inside.left(), s.y), pos2(inside.right(), s.y)) } else { (pos2(s.x, inside.top()), pos2(s.x, inside.bottom())) };
        painter.line_segment([a, b], Stroke::new(1.0, GUIDE));
    }
    let scale = ed.view.scale();
    let step = tick_step(scale, 56.0);
    let small = if step >= 5.0 { step / 5.0 } else { step };
    let font = FontId::proportional(9.5);
    for (r, across) in [(top, true), (left, false)] {
        painter.rect_filled(r, 0.0, t.toolbar);
        // From the first tick in sight to the last, in picture pixels.
        let (from, to) = if across {
            (ed.view.to_image(r.left_top()).x, ed.view.to_image(r.right_top()).x)
        } else {
            (ed.view.to_image(r.left_top()).y, ed.view.to_image(r.left_bottom()).y)
        };
        if !(from.is_finite() && to.is_finite()) {
            continue;
        }
        let first = (from / small).floor() as i64;
        let last = (to / small).ceil() as i64;
        for n in first..=last.min(first.saturating_add(2000)) {
            let v = n as f32 * small;
            let s = ed.view.to_screen(Pt::new(v, v));
            let major = (v / step).round() * step == v;
            let len = if major { RULER * 0.6 } else { RULER * 0.25 };
            if across {
                painter.line_segment([pos2(s.x, r.bottom() - len), pos2(s.x, r.bottom())], Stroke::new(1.0, t.muted));
                if major {
                    painter.text(pos2(s.x + 2.0, r.top() + 1.0), Align2::LEFT_TOP, format!("{v:.0}"), font.clone(), t.muted);
                }
            } else {
                painter.line_segment([pos2(r.right() - len, s.y), pos2(r.right(), s.y)], Stroke::new(1.0, t.muted));
                if major {
                    painter.text(pos2(r.left() + 1.0, s.y + 2.0), Align2::LEFT_TOP, format!("{v:.0}"), font.clone(), t.muted);
                }
            }
        }
        // Where each guide meets its ruler.
        for g in ed.guides.iter().filter(|g| g.across != across) {
            let s = ed.view.to_screen(Pt::new(g.at, g.at));
            if across {
                painter.line_segment([pos2(s.x, r.top()), pos2(s.x, r.bottom())], Stroke::new(2.0, GUIDE));
            } else {
                painter.line_segment([pos2(r.left(), s.y), pos2(r.right(), s.y)], Stroke::new(2.0, GUIDE));
            }
        }
        let edge = if across { [r.left_bottom(), r.right_bottom()] } else { [r.right_top(), r.right_bottom()] };
        painter.line_segment(edge, Stroke::new(1.0, t.border));
    }
    painter.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(RULER, RULER)), 0.0, t.toolbar);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_line_wins_within_reach() {
        let moved = Rect::new(97.0, 48.0, 20.0, 10.0);
        let (d, lines) = snap(moved, &[0.0, 100.0, 200.0], &[0.0, 50.0, 100.0], 5.0);
        assert_eq!(d, Pt::new(3.0, 2.0), "its left edge to 100, its top to 50");
        assert_eq!(lines, vec![Guide { across: false, at: 100.0 }, Guide { across: true, at: 50.0 }]);
        // Its middle snaps too; nothing in reach, nothing moves.
        let (d, _) = snap(Rect::new(88.0, 0.0, 20.0, 10.0), &[100.0], &[], 3.0);
        assert_eq!(d, Pt::new(2.0, 0.0));
        let (d, lines) = snap(Rect::new(10.0, 10.0, 20.0, 10.0), &[100.0], &[100.0], 3.0);
        assert_eq!((d, lines.len()), (Pt::ZERO, 0));
        let (d, _) = snap(Rect::new(f32::NAN, 0.0, 1.0, 1.0), &[f32::INFINITY], &[], 3.0);
        assert_eq!(d, Pt::ZERO);
    }

    #[test]
    fn ruler_ticks_are_far_enough_apart() {
        assert_eq!(tick_step(1.0, 50.0), 50.0);
        assert_eq!(tick_step(4.0, 50.0), 20.0);
        assert_eq!(tick_step(0.1, 50.0), 500.0);
        assert_eq!(tick_step(f32::NAN, 50.0), 50.0);
        assert!(tick_step(1e-30, 50.0) >= 1.0);
    }
}
