//! The interface's icons, drawn in code (original; no icon font or picture files).

use egui::epaint::{CubicBezierShape, PathShape};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Select,
    Arrow,
    Line,
    Rect,
    Ellipse,
    Text,
    Callout,
    Step,
    Pen,
    Highlight,
    Blur,
    Pixelate,
    Spotlight,
    Magnify,
    Stamp,
    Crop,
    Undo,
    Redo,
    Copy,
    Save,
    Open,
    Capture,
    ZoomIn,
    ZoomOut,
    Fit,
    Delete,
    Region,
    Window,
    Screen,
    AllScreens,
    Repeat,
    Scrolling,
    Gif,
    Settings,
    Folder,
    Edit,
    Close,
    Check,
    Timer,
}

/// Draws `icon` in `rect` (square-ish) in `color`.
pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height());
    let c = rect.center();
    let u = s / 20.0; // one unit of a 20-unit grid
    let at = |x: f32, y: f32| pos2(c.x + (x - 10.0) * u, c.y + (y - 10.0) * u);
    let w = (1.6 * u).max(1.2);
    let stroke = Stroke::new(w, color);
    let line = |a: Pos2, b: Pos2| {
        p.line_segment([a, b], stroke);
    };
    let poly = |pts: Vec<Pos2>, closed: bool| {
        if closed {
            p.add(Shape::Path(PathShape::closed_line(pts, stroke)));
        } else {
            p.add(Shape::Path(PathShape::line(pts, stroke)));
        }
    };
    let fill_poly = |pts: Vec<Pos2>, fill: Color32| {
        p.add(Shape::convex_polygon(pts, fill, Stroke::NONE));
    };
    let rect_at = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_two_pos(at(x0, y0), at(x1, y1));
    match icon {
        Icon::Select => {
            fill_poly(vec![at(5.0, 2.0), at(15.0, 12.0), at(10.5, 12.5), at(13.5, 18.0), at(11.0, 19.0), at(8.2, 13.6), at(5.0, 16.5)], color);
        }
        Icon::Arrow => {
            line(at(4.0, 16.0), at(14.0, 6.0));
            fill_poly(vec![at(17.0, 3.0), at(15.4, 10.6), at(9.4, 4.6)], color);
        }
        Icon::Line => line(at(4.0, 16.0), at(16.0, 4.0)),
        Icon::Rect => {
            p.rect_stroke(rect_at(3.5, 5.0, 16.5, 15.0), 2.0 * u, stroke, StrokeKind::Middle);
        }
        Icon::Ellipse => {
            let r = rect_at(3.0, 5.0, 17.0, 15.0);
            p.add(Shape::ellipse_stroke(r.center(), r.size() / 2.0, stroke));
        }
        Icon::Text => {
            p.text(c, Align2::CENTER_CENTER, "T", FontId::proportional(15.0 * u), color);
        }
        Icon::Callout => {
            p.rect_stroke(rect_at(3.0, 3.5, 17.0, 12.5), 2.5 * u, stroke, StrokeKind::Middle);
            poly(vec![at(6.5, 12.5), at(5.0, 17.0), at(10.0, 12.5)], false);
            line(at(6.0, 7.0), at(14.0, 7.0));
            line(at(6.0, 9.5), at(11.5, 9.5));
        }
        Icon::Step => {
            p.circle_filled(c, 7.5 * u, color);
            p.text(c, Align2::CENTER_CENTER, "1", FontId::proportional(10.0 * u), contrast(color));
        }
        Icon::Pen => {
            p.add(Shape::CubicBezier(CubicBezierShape::from_points_stroke(
                [at(3.0, 14.0), at(6.0, 4.0), at(11.0, 20.0), at(17.0, 6.0)],
                false,
                Color32::TRANSPARENT,
                stroke,
            )));
        }
        Icon::Highlight => {
            let bar = Stroke::new(5.0 * u, Color32::from_rgba_unmultiplied(255, 214, 0, 170));
            p.line_segment([at(3.0, 13.0), at(17.0, 13.0)], bar);
            fill_poly(vec![at(9.0, 3.0), at(14.0, 3.0), at(13.0, 10.0), at(10.0, 10.0)], color);
        }
        Icon::Blur => {
            for (i, a) in [(0, 255u8), (1, 150), (2, 80)] {
                let r = (3.0 + i as f32 * 2.2) * u;
                p.circle_stroke(c, r, Stroke::new(w, color.gamma_multiply(f32::from(a) / 255.0)));
            }
        }
        Icon::Pixelate => {
            let cell = 4.0;
            for iy in 0..3 {
                for ix in 0..3 {
                    if (ix + iy) % 2 == 0 {
                        let (x, y) = (4.0 + ix as f32 * cell, 4.0 + iy as f32 * cell);
                        p.rect_filled(rect_at(x, y, x + cell, y + cell), 0.0, color);
                    }
                }
            }
            p.rect_stroke(rect_at(4.0, 4.0, 16.0, 16.0), 0.0, Stroke::new(w * 0.6, color), StrokeKind::Middle);
        }
        Icon::Spotlight => {
            p.rect_filled(rect_at(2.5, 4.0, 17.5, 16.0), 1.5 * u, color.gamma_multiply(0.45));
            p.circle_filled(at(10.0, 10.0), 4.0 * u, color);
        }
        Icon::Magnify => {
            p.circle_stroke(at(8.5, 8.5), 5.0 * u, stroke);
            p.line_segment([at(12.3, 12.3), at(17.0, 17.0)], Stroke::new(w * 1.6, color));
        }
        Icon::Stamp => {
            let mut pts = Vec::new();
            for i in 0..10 {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                let r = if i % 2 == 0 { 7.5 } else { 3.2 };
                pts.push(at(10.0 + a.cos() * r, 10.5 + a.sin() * r));
            }
            poly(pts, true);
        }
        Icon::Crop => {
            poly(vec![at(6.0, 2.0), at(6.0, 14.0), at(18.0, 14.0)], false);
            poly(vec![at(2.0, 6.0), at(14.0, 6.0), at(14.0, 18.0)], false);
        }
        Icon::Undo | Icon::Redo => {
            let flip = if icon == Icon::Redo { -1.0 } else { 1.0 };
            let fx = |x: f32| 10.0 + (x - 10.0) * flip;
            p.add(Shape::CubicBezier(CubicBezierShape::from_points_stroke(
                [at(fx(5.0), 8.0), at(fx(9.0), 4.0), at(fx(18.0), 6.0), at(fx(14.0), 15.0)],
                false,
                Color32::TRANSPARENT,
                stroke,
            )));
            fill_poly(
                if flip > 0.0 { vec![at(2.5, 8.0), at(7.5, 4.0), at(7.5, 12.0)] } else { vec![at(17.5, 8.0), at(12.5, 12.0), at(12.5, 4.0)] },
                color,
            );
        }
        Icon::Copy => {
            p.rect_stroke(rect_at(7.0, 7.0, 17.0, 17.0), 1.5 * u, stroke, StrokeKind::Middle);
            poly(vec![at(4.0, 13.0), at(4.0, 3.5), at(13.0, 3.5)], false);
        }
        Icon::Save => {
            p.rect_stroke(rect_at(3.5, 3.5, 16.5, 16.5), 1.5 * u, stroke, StrokeKind::Middle);
            p.rect_filled(rect_at(6.5, 3.5, 13.5, 8.0), 0.0, color);
            p.rect_stroke(rect_at(6.0, 11.0, 14.0, 16.5), 0.0, stroke, StrokeKind::Middle);
        }
        Icon::Open | Icon::Folder => {
            poly(vec![at(2.5, 15.5), at(2.5, 4.5), at(7.5, 4.5), at(9.0, 6.5), at(17.5, 6.5), at(17.5, 15.5)], true);
            if icon == Icon::Open {
                line(at(2.5, 9.0), at(17.5, 9.0));
            }
        }
        Icon::Capture | Icon::Region => {
            let k = 4.0;
            for (x, y, dx, dy) in [(3.0, 3.0, 1.0, 1.0), (17.0, 3.0, -1.0, 1.0), (3.0, 17.0, 1.0, -1.0), (17.0, 17.0, -1.0, -1.0)] {
                poly(vec![at(x, y + dy * k), at(x, y), at(x + dx * k, y)], false);
            }
            if icon == Icon::Capture {
                p.circle_filled(c, 2.5 * u, color);
            } else {
                line(at(10.0, 7.0), at(10.0, 13.0));
                line(at(7.0, 10.0), at(13.0, 10.0));
            }
        }
        Icon::ZoomIn | Icon::ZoomOut => {
            p.circle_stroke(at(8.5, 8.5), 5.5 * u, stroke);
            line(at(12.5, 12.5), at(17.0, 17.0));
            line(at(6.0, 8.5), at(11.0, 8.5));
            if icon == Icon::ZoomIn {
                line(at(8.5, 6.0), at(8.5, 11.0));
            }
        }
        Icon::Fit => {
            for (x, y, dx, dy) in [(3.0, 3.0, 1.0, 1.0), (17.0, 3.0, -1.0, 1.0), (3.0, 17.0, 1.0, -1.0), (17.0, 17.0, -1.0, -1.0)] {
                line(at(x, y), at(x + dx * 4.5, y + dy * 4.5));
                poly(vec![at(x + dx * 3.5, y), at(x, y), at(x, y + dy * 3.5)], false);
            }
        }
        Icon::Delete => {
            poly(vec![at(5.0, 6.0), at(6.0, 17.0), at(14.0, 17.0), at(15.0, 6.0)], false);
            line(at(3.5, 5.5), at(16.5, 5.5));
            line(at(8.0, 3.5), at(12.0, 3.5));
        }
        Icon::Window => {
            p.rect_stroke(rect_at(2.5, 4.0, 17.5, 16.0), 1.5 * u, stroke, StrokeKind::Middle);
            p.rect_filled(rect_at(2.5, 4.0, 17.5, 7.0), 0.0, color);
        }
        Icon::Screen | Icon::AllScreens => {
            if icon == Icon::AllScreens {
                p.rect_stroke(rect_at(1.5, 3.5, 10.5, 11.5), 1.0 * u, stroke, StrokeKind::Middle);
                p.rect_stroke(rect_at(9.5, 5.5, 18.5, 13.5), 1.0 * u, stroke, StrokeKind::Middle);
                line(at(8.0, 16.5), at(12.0, 16.5));
            } else {
                p.rect_stroke(rect_at(2.5, 3.5, 17.5, 13.5), 1.0 * u, stroke, StrokeKind::Middle);
                line(at(10.0, 13.5), at(10.0, 16.5));
                line(at(6.5, 16.5), at(13.5, 16.5));
            }
        }
        Icon::Repeat => {
            p.circle_stroke(c, 6.0 * u, stroke);
            fill_poly(vec![at(14.0, 2.5), at(18.0, 6.5), at(13.0, 7.5)], color);
        }
        Icon::Scrolling => {
            p.rect_stroke(rect_at(5.0, 2.5, 15.0, 17.5), 1.5 * u, stroke, StrokeKind::Middle);
            line(at(10.0, 6.0), at(10.0, 14.0));
            poly(vec![at(7.5, 11.5), at(10.0, 14.0), at(12.5, 11.5)], false);
        }
        Icon::Gif => {
            p.rect_stroke(rect_at(2.5, 5.0, 17.5, 15.0), 2.0 * u, stroke, StrokeKind::Middle);
            p.text(c, Align2::CENTER_CENTER, "GIF", FontId::proportional(6.5 * u), color);
        }
        Icon::Settings => {
            p.circle_stroke(c, 3.0 * u, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                line(at(10.0 + a.cos() * 5.5, 10.0 + a.sin() * 5.5), at(10.0 + a.cos() * 8.0, 10.0 + a.sin() * 8.0));
            }
        }
        Icon::Edit => {
            poly(vec![at(4.0, 16.0), at(4.5, 12.5), at(13.5, 3.5), at(16.5, 6.5), at(7.5, 15.5)], true);
        }
        Icon::Close => {
            line(at(5.0, 5.0), at(15.0, 15.0));
            line(at(15.0, 5.0), at(5.0, 15.0));
        }
        Icon::Check => poly(vec![at(4.0, 10.5), at(8.0, 14.5), at(16.0, 5.5)], false),
        Icon::Timer => {
            p.circle_stroke(at(10.0, 11.0), 7.0 * u, stroke);
            line(at(10.0, 11.0), at(10.0, 6.5));
            line(at(10.0, 11.0), at(13.0, 13.0));
            line(at(8.0, 2.5), at(12.0, 2.5));
        }
    }
}

fn contrast(c: Color32) -> Color32 {
    let l = 0.299 * f32::from(c.r()) + 0.587 * f32::from(c.g()) + 0.114 * f32::from(c.b());
    if l > 150.0 { Color32::BLACK } else { Color32::WHITE }
}

/// A square icon button with a tooltip; `on` draws it as selected.
pub fn button(ui: &mut egui::Ui, icon: Icon, tip: &str, on: bool, enabled: bool) -> egui::Response {
    let t = crate::theme::Tokens::get(ui.ctx());
    let size = vec2(30.0, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let bg = if on {
        t.selected
    } else if resp.hovered() && enabled {
        t.hover
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 5.0, bg);
    }
    if on {
        ui.painter().rect_stroke(rect, 5.0, Stroke::new(1.0, t.accent), StrokeKind::Inside);
    }
    let color = if !enabled {
        t.muted.gamma_multiply(0.5)
    } else if on {
        t.accent
    } else {
        t.text
    };
    paint(ui.painter(), rect.shrink(6.0), icon, color);
    resp.on_hover_text(tip)
}
